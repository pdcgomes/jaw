import Darwin
import Foundation
import Synchronization

/// The connection an interface keeps to jawd, as jaw-client's `client.rs` keeps it, which never
/// makes its caller wait. A thread reads what jawd sends and decodes each line once; a serial queue
/// writes what the caller asks. Replies and pushes wait in an inbox the caller drains, each window's
/// updates merged as they come. The answer to anything the caller waits for (hello, a window's
/// watch or move landing where it was asked to be, a request) wakes it at once; pushes wake it so
/// it drains at its next display frame.
///
/// When jawd goes away, or isn't there yet, the client says so, tries again from 100 ms doubling to
/// 2 s, says hello again and watches every window again with the rows its model holds, so the
/// models end as a fresh watch would leave them. A `resync` is handled the same way. A request in
/// flight when the connection drops is answered with a failure that says so.
public final class Client: Sendable {
    public struct Config: Sendable {
        public var socket: String
        /// The interface's name and version, for hello.
        public var client: String
        public var version: String
        /// Whether the interface is in front when it connects.
        public var focus: Bool
        /// How many of the caller's requests can wait to be written before `request` refuses more.
        public var queue = 256
        /// How many replies and pushes can wait for the caller before the client stops reading.
        public var events = 4096

        public init(socket: String, client: String, version: String, focus: Bool = false) {
            self.socket = socket
            self.client = client
            self.version = version
            self.focus = focus
        }
    }

    static let firstRetry: Duration = .milliseconds(100)
    static let lastRetry: Duration = .seconds(2)
    /// The longest line jawd sends, as the protocol has it.
    static let longestLine = 16 << 20

    let config: Config
    /// Called from the client's threads when something waits to be drained; `true` when it's an
    /// answer the caller waits for, to drain at once rather than at its next frame.
    let wake: @Sendable (Bool) -> Void
    let state: Mutex<State>
    let writes = DispatchQueue(label: "dev.badcoffee.jaw.client.write", qos: .userInitiated)
    /// Wakes the wait between connections when the client closes.
    let pause = DispatchSemaphore(value: 0)

    public init(config: Config, wake: @escaping @Sendable (Bool) -> Void) {
        self.config = config
        self.wake = wake
        state = Mutex(State(focus: config.focus))
    }

    /// Starts connecting to jawd, in the background.
    public func start() {
        let thread = Thread { [self] in session() }
        thread.name = "jaw-client"
        thread.qualityOfService = .userInitiated
        thread.start()
    }

    /// Stops for good: the connection closes and nothing more comes.
    public func close() {
        let fd = state.withLock { s -> Int32 in
            s.closed = true
            return s.fd
        }
        if fd >= 0 { shutdown(fd, SHUT_RDWR) }
        pause.signal()
    }

    // MARK: - What the caller asks

    /// Watches a folder's chat list. Its updates come in frames under the id this returns.
    public func watchChats(_ folder: String, _ place: Place) -> WindowID {
        open(.chats, folder, place)
    }

    /// Watches a chat's messages.
    public func watchMessages(_ chat: String, _ place: Place) -> WindowID {
        open(.messages, chat, place)
    }

    /// Moves a window, for scrolling. Moves asked while one is in flight wait for it, and only the
    /// latest is sent.
    public func move(_ window: WindowID, _ place: Place) {
        let line = state.withLock { s -> Line? in
            guard s.windows[window] != nil else { return nil }
            s.windows[window]!.place = place
            guard s.windows[window]!.asking == nil, s.live else { return nil }
            return asking(&s, window, again: false)
        }
        if let line { write(line) }
    }

    /// Stops watching a window; nothing more comes for it.
    public func unwatch(_ window: WindowID) {
        let line = state.withLock { s -> Line? in
            s.chats[window] = nil
            s.messages[window] = nil
            guard let w = s.windows[window] else { return nil }
            if w.asking != nil {
                s.windows[window]!.closing = true
                return nil
            }
            s.windows[window] = nil
            guard let watch = w.watch else { return nil }
            s.watches[watch] = nil
            return unwatchLine(&s, watch)
        }
        if let line { write(line) }
    }

    /// Sends a request; its reply comes as `Event.reply` with the id this returns.
    public func request(_ method: String, _ params: (some Encodable & Sendable)?,
                        expect: Expect = .json) -> Result<UInt64, Failure>
    {
        send(method, params, asked: .request(expect))
    }

    /// Sends a find for the palette, at each keystroke: only the latest one's answer comes.
    public func find(_ find: Find) -> Result<UInt64, Failure> {
        send("find", find, asked: .find(find.query))
    }

    /// Says whether the interface is in front, now and in every hello from here on.
    @discardableResult
    public func focus(_ focused: Bool) -> Result<UInt64, Failure> {
        state.withLock { $0.focus = focused }
        return request("focus", Focus(focused: focused))
    }

    /// Takes everything that came since the last drain.
    public func drain() -> Frame {
        state.withLock { s in
            s.signalled = false
            s.urgent = false
            let frame = Frame(
                events: s.events,
                chats: s.chats.mapValues { $0.update() },
                messages: s.messages.mapValues { $0.update() },
            )
            s.events.removeAll(keepingCapacity: true)
            s.chats.removeAll(keepingCapacity: true)
            s.messages.removeAll(keepingCapacity: true)
            if !frame.isEmpty { s.stats.frames += 1 }
            return frame
        }
    }

    public var stats: Stats {
        state.withLock { $0.stats }
    }

    // MARK: - Inside

    struct Line: Sendable {
        var id: UInt64 = 0
        let data: Data
        let generation: UInt64
        /// The caller's request, counted against `queue` until it's written.
        let counted: Bool
    }

    func open(_ kind: Kind, _ of: String, _ place: Place) -> WindowID {
        let (window, line) = state.withLock { s -> (WindowID, Line?) in
            s.nextWindow += 1
            let window = WindowID(raw: s.nextWindow)
            s.windows[window] = Win(kind: kind, of: of, place: place)
            return (window, s.live ? asking(&s, window, again: false) : nil)
        }
        if let line { write(line) }
        return window
    }

    func send(_ method: String, _ params: (some Encodable & Sendable)?, asked: Asked) -> Result<UInt64, Failure> {
        let result = state.withLock { s -> Result<Line, Failure> in
            guard s.live else { return .failure(.unreachable) }
            guard s.queued < config.queue else { return .failure(.full(config.queue)) }
            let id = s.id()
            if case .find = asked { s.find = id }
            s.asked[id] = asked
            s.queued += 1
            return .success(Line(id: id, data: Lines.encode(id: id, method: method, params: params),
                                 generation: s.generation, counted: true))
        }
        switch result {
        case let .success(line):
            write(line)
            return .success(line.id)
        case let .failure(failure):
            return .failure(failure)
        }
    }

    /// The line a window needs to be where the caller wants it: a move if jawd has it, otherwise a
    /// watch, with the rows its model holds when it's watched again.
    func asking(_ s: inout State, _ window: WindowID, again: Bool) -> Line? {
        guard let w = s.windows[window] else { return nil }
        let id = s.id()
        let place = w.place
        let data: Data
        if let watch = w.watch, !again {
            let moving = WatchMove(watch: watch, around: place.around, before: place.before, after: place.after)
            data = Lines.encode(id: id, method: "watch.move", params: moving)
        } else {
            let have = w.ids.sorted()
            switch w.kind {
            case .chats:
                // A chat list held while catching up sends rows with an old sort and their v
                // unchanged, so a chat list comes whole, its have saying which rows left.
                let watch = ChatsWatch(folder: w.of, around: place.around, before: place.before,
                                       after: place.after, since: nil, have: have)
                data = Lines.encode(id: id, method: "chats.watch", params: watch)
            case .messages:
                let watch = MessagesWatch(chat: w.of, around: place.around, before: place.before,
                                          after: place.after, since: have.isEmpty ? nil : w.seq, have: have)
                data = Lines.encode(id: id, method: "messages.watch", params: watch)
            }
        }
        let landingAgain: Bool? = (w.watch == nil || again) ? again : nil
        s.windows[window]!.asking = id
        s.asked[id] = .window(Landing(id: id, window: window, place: place, again: landingAgain))
        return Line(data: data, generation: s.generation, counted: false)
    }

    func unwatchLine(_ s: inout State, _ watch: String) -> Line? {
        guard s.live else { return nil }
        let id = s.id()
        s.asked[id] = .unwatch
        return Line(data: Lines.encode(id: id, method: "unwatch", params: WatchRef(watch: watch)),
                    generation: s.generation, counted: false)
    }

    /// Writes a line on the write queue, if its connection is still the client's.
    func write(_ line: Line) {
        writes.async { [self] in
            let fd = state.withLock { s -> Int32 in
                if line.counted { s.queued -= 1 }
                return s.live && s.generation == line.generation ? s.fd : -1
            }
            guard fd >= 0 else { return }
            if let why = Self.writeAll(fd, line.data) {
                state.withLock { s in
                    if s.generation == line.generation, s.writeFailed == nil { s.writeFailed = why }
                }
                shutdown(fd, SHUT_RDWR)
            }
        }
    }

    static func writeAll(_ fd: Int32, _ data: Data) -> String? {
        data.withUnsafeBytes { raw -> String? in
            var offset = 0
            while offset < raw.count {
                let n = Darwin.write(fd, raw.baseAddress! + offset, raw.count - offset)
                if n < 0 {
                    if errno == EINTR { continue }
                    if errno == EAGAIN { return "jawd stopped reading what jaw sends for 10 s" }
                    return "can't write to jawd: \(String(cString: strerror(errno)))"
                }
                offset += n
            }
            return nil
        }
    }

    /// Wakes the caller for what was just put in the inbox: at once for an answer it waits for.
    func notify(urgent: Bool) {
        let call = state.withLock { s -> Bool in
            if urgent, !s.urgent {
                s.urgent = true
                s.signalled = true
                return true
            }
            if !s.signalled {
                s.signalled = true
                return true
            }
            return false
        }
        if call { wake(urgent) }
    }

    /// Waits for room in the inbox while connection `generation` lasts.
    func waitForRoom(_ generation: UInt64) {
        while true {
            let full = state.withLock { s in
                s.events.count >= config.events && !s.closed && s.live && s.generation == generation
            }
            guard full else { return }
            usleep(1000)
        }
    }

    // MARK: - The session

    /// Connects, serves the connection until it drops, and connects again, until the client closes.
    func session() {
        var retry = Self.firstRetry
        while true {
            if state.withLock({ $0.closed }) { return }
            let why: String
            switch Self.connect(config.socket) {
            case let .success(fd):
                why = serve(fd)
            case let .failure(unreached):
                why = unreached.why
            }
            if lost(why, &retry) { return }
            _ = pause.wait(timeout: .now() + .milliseconds(Int(retry / .milliseconds(1))))
            if state.withLock({ $0.closed }) { return }
            retry = min(retry * 2, Self.lastRetry)
        }
    }

    static func connect(_ path: String) -> Result<Int32, Unreached> {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return .failure(Unreached("can't make a socket: \(String(cString: strerror(errno)))")) }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: addr.sun_path) else {
            Darwin.close(fd)
            return .failure(Unreached("the socket's path is too long: \(path)"))
        }
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        addr.sun_len = UInt8(size)
        let r = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, size) }
        }
        if r != 0 {
            let e = errno
            Darwin.close(fd)
            if e == ENOENT || e == ECONNREFUSED {
                return .failure(Unreached("jawd isn't running: nothing answers on \(path)"))
            }
            return .failure(Unreached("can't reach jawd on \(path): \(String(cString: strerror(e)))"))
        }
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        var stall = timeval(tv_sec: 10, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &stall, socklen_t(MemoryLayout<timeval>.size))
        return .success(fd)
    }

    /// Serves one connection: hello and the windows first, then whatever comes. Returns why it ended.
    func serve(_ fd: Int32) -> String {
        let (generation, lines) = state.withLock { s -> (UInt64, [Line]) in
            s.generation += 1
            s.live = true
            s.greeted = false
            s.fd = fd
            s.writeFailed = nil
            let id = s.id()
            s.asked[id] = .hello
            let hello = Hello(client: config.client, version: config.version, focus: s.focus)
            var lines = [Line(data: Lines.encode(id: id, method: "hello", params: hello),
                              generation: s.generation, counted: false)]
            for window in s.windows.keys.sorted() {
                if s.windows[window]!.closing {
                    s.windows[window] = nil
                    continue
                }
                let again = s.windows[window]!.shown != nil
                if let line = asking(&s, window, again: again) { lines.append(line) }
            }
            return (s.generation, lines)
        }
        for line in lines {
            write(line)
        }
        var why = read(fd, generation)
        state.withLock { s in
            s.live = false
            s.fd = -1
            if let failed = s.writeFailed { why = failed }
        }
        writes.sync {}
        Darwin.close(fd)
        return why
    }

    /// Reads lines until the connection ends; says why it did.
    func read(_ fd: Int32, _ generation: UInt64) -> String {
        let decoder = JSONDecoder()
        decoder.userInfo[.want] = WantLookup { [self] id in
            state.withLock { s in
                switch s.asked[id] {
                case .hello: .hello
                case let .request(expect): .reply(expect)
                case .find: .find
                case let .window(landing):
                    s.windows[landing.window]?.kind == .messages ? .messages : .chats
                case .unwatch, nil: .nothing
                }
            }
        }
        var pending = Data()
        var scanned = 0
        let chunk = 1 << 16
        var buffer = [UInt8](repeating: 0, count: chunk)
        while true {
            let n = buffer.withUnsafeMutableBytes { Darwin.read(fd, $0.baseAddress!, chunk) }
            if n == 0 { return "jawd closed the connection" }
            if n < 0 {
                if errno == EINTR { continue }
                return "the connection to jawd broke: \(String(cString: strerror(errno)))"
            }
            pending.append(contentsOf: buffer[0 ..< n])
            while let newline = pending[(pending.startIndex + scanned)...].firstIndex(of: 0x0A) {
                let line = pending[pending.startIndex ..< newline]
                if !take(IncomingLine.decode(Data(line), with: decoder), generation) {
                    return "the client closed"
                }
                pending.removeSubrange(pending.startIndex ... newline)
                scanned = 0
            }
            scanned = pending.count
            if pending.count >= Self.longestLine {
                return "jawd sent a line longer than the protocol allows"
            }
        }
    }

    /// After a connection dropped or couldn't be made: fails what was in flight, forgets jawd's
    /// watch ids, and says when it tries again, from 100 ms after a connection that was greeted.
    /// Says whether the client is closing.
    func lost(_ why: String, _ retry: inout Duration) -> Bool {
        let (closing, wait) = state.withLock { [retry] s -> (Bool, Duration) in
            s.live = false
            var retry = retry
            if s.greeted {
                s.greeted = false
                retry = Self.firstRetry
            }
            var events: [Event] = []
            for (id, asked) in s.asked.sorted(by: { $0.key < $1.key }) {
                switch asked {
                case .request: events.append(.reply(id: id, .failure(.lost)))
                case let .find(query) where s.find == id: events.append(.found(id: id, query: query, .failure(.lost)))
                default: break
                }
            }
            s.asked.removeAll()
            s.find = nil
            s.watches.removeAll()
            s.windows = s.windows.filter { !$0.value.closing }
            for window in s.windows.keys {
                s.windows[window]!.watch = nil
                s.windows[window]!.asking = nil
            }
            events.append(.disconnected(why: why, retryIn: retry))
            s.events.append(contentsOf: events)
            return (s.closed, retry)
        }
        retry = wait
        notify(urgent: false)
        return closing
    }

    // MARK: - Taking what jawd sends

    /// Takes one line from jawd. False once this connection is no longer the client's.
    func take(_ incoming: Incoming, _ generation: UInt64) -> Bool {
        switch incoming {
        case .skip:
            return state.withLock { s in
                s.stats.skipped += 1
                return s.current(generation)
            }
        case let .chatsPush(push):
            let update = Update<ChatRow>(seq: push.seq, remove: push.remove, upsert: push.upsert, total: push.total)
            return windowPush(push.watch, generation) { s, window in s.mergeChats(window, update) }
        case let .messagesPush(push):
            let update = Update<MessageRow>(seq: push.seq, remove: push.remove, upsert: push.upsert,
                                            total: push.total, older: push.older, newer: push.newer)
            return windowPush(push.watch, generation) { s, window in s.mergeMessages(window, update) }
        case let .resync(watch):
            let (current, line) = state.withLock { s -> (Bool, Line?) in
                guard s.current(generation) else { return (false, nil) }
                s.stats.lines += 1
                guard let window = s.watches.removeValue(forKey: watch) else { return (true, nil) }
                s.stats.resyncs += 1
                guard s.windows[window] != nil else { return (true, nil) }
                s.windows[window]!.watch = nil
                if s.windows[window]!.closing {
                    s.windows[window] = nil
                    return (true, nil)
                }
                return (true, asking(&s, window, again: true))
            }
            if let line { write(line) }
            return current
        case let .connection(connection):
            return event(.connection(connection), generation, urgent: false)
        case let .link(step):
            return event(.link(step), generation, urgent: false)
        case let .notice(notice):
            return event(.notice(notice), generation, urgent: false)
        case let .push(method, params):
            return event(.push(method: method, params: params), generation, urgent: false)
        case let .hello(id, welcome):
            return hello(id, .success(welcome), generation)
        case let .error(id, error):
            return reply(id, .failure(.jawd(error)), generation)
        case let .bad(id, why):
            return reply(id, .failure(.jawd(JawError(code: ErrorCode.internal, message: why))), generation)
        case let .reply(id, value):
            return reply(id, .success(.reply(value)), generation)
        case let .found(id, found):
            return reply(id, .success(.found(found)), generation)
        case let .chats(id, watched):
            return reply(id, .success(.chats(watched)), generation)
        case let .messages(id, watched):
            return reply(id, .success(.messages(watched)), generation)
        }
    }

    func event(_ event: Event, _ generation: UInt64, urgent: Bool) -> Bool {
        waitForRoom(generation)
        let current = state.withLock { s -> Bool in
            guard s.current(generation) else { return false }
            s.stats.lines += 1
            s.events.append(event)
            return true
        }
        if current { notify(urgent: urgent) }
        return current
    }

    func windowPush(_ watch: String, _ generation: UInt64, merge: (inout State, WindowID) -> Void) -> Bool {
        let (current, merged) = state.withLock { s -> (Bool, Bool) in
            guard s.current(generation) else { return (false, false) }
            s.stats.lines += 1
            guard let window = s.watches[watch] else { return (true, false) }
            merge(&s, window)
            return (true, true)
        }
        if merged { notify(urgent: false) }
        return current
    }

    func hello(_ id: UInt64, _ welcome: Result<Welcome, Failure>, _ generation: UInt64) -> Bool {
        enum Greeted { case gone, greeted, refused(Int32) }
        let greeted = state.withLock { s -> Greeted in
            guard s.current(generation) else { return .gone }
            s.stats.lines += 1
            s.asked[id] = nil
            switch welcome {
            case let .success(w):
                s.greeted = true
                s.stats.connects += 1
                s.events.append(.connected(w))
                return .greeted
            case let .failure(failure):
                if s.writeFailed == nil { s.writeFailed = failure.description }
                return .refused(s.fd)
            }
        }
        switch greeted {
        case .gone:
            return false
        case .greeted:
            notify(urgent: true)
            return true
        case let .refused(fd):
            shutdown(fd, SHUT_RDWR)
            return true
        }
    }

    /// What a reply answers, once it's read.
    enum Answer: Sendable {
        case reply(Reply)
        case found(Found)
        case chats(ChatsWatched)
        case messages(MessagesWatched)
    }

    func reply(_ id: UInt64, _ outcome: Result<Answer, Failure>, _ generation: UInt64) -> Bool {
        guard let asked = state.withLock({ s -> Asked?? in
            guard s.current(generation) else { return nil }
            s.stats.lines += 1
            return .some(s.asked.removeValue(forKey: id))
        }) else { return false }
        guard let asked else { return true }
        switch asked {
        case .hello:
            // A hello whose answer didn't read as a welcome.
            if case let .failure(failure) = outcome { return hello(id, .failure(failure), generation) }
            return true
        case .request:
            let result: Result<Reply, Failure> = outcome.flatMap { answer in
                if case let .reply(reply) = answer { return .success(reply) }
                return .failure(.jawd(JawError(code: ErrorCode.internal, message: "jawd's answer wasn't what was asked")))
            }
            return event(.reply(id: id, result), generation, urgent: true)
        case let .find(query):
            let result: Result<Found, Failure> = outcome.flatMap { answer in
                if case let .found(found) = answer { return .success(found) }
                return .failure(.jawd(JawError(code: ErrorCode.internal, message: "jawd's answer to find wasn't one")))
            }
            let latest = state.withLock { s -> Bool in
                guard s.find == id else { return false }
                s.find = nil
                return true
            }
            return latest ? event(.found(id: id, query: query, result), generation, urgent: true) : true
        case .unwatch:
            return true
        case let .window(landing):
            return landed(landing, outcome, generation)
        }
    }

    /// A watch's or move's answer.
    func landed(_ landing: Landing, _ outcome: Result<Answer, Failure>, _ generation: UInt64) -> Bool {
        var lines: [Line] = []
        var failed: (WindowID, Failure)?
        var urgent = false
        let current = state.withLock { s -> Bool in
            guard s.current(generation) else { return false }
            // One superseded by watching again after a resync changes nothing.
            guard let w = s.windows[landing.window], w.asking == landing.id else { return true }
            s.windows[landing.window]!.asking = nil
            let window = landing.window
            let place = landing.place
            switch outcome {
            case let .success(answer):
                let watch: String
                switch answer {
                case let .chats(watched):
                    watch = watched.watch
                case let .messages(watched):
                    watch = watched.watch
                default:
                    failed = (window, .jawd(JawError(code: ErrorCode.internal, message: "jawd's answer to a watch wasn't one")))
                    return true
                }
                if w.closing {
                    s.windows[window] = nil
                    lines.append(contentsOf: [unwatchLine(&s, watch)].compactMap(\.self))
                    return true
                }
                let moved = (landing.again != true || w.shown != place)
                let follow = w.place != place
                s.windows[window]!.shown = place
                s.windows[window]!.watch = watch
                s.watches[watch] = window
                switch answer {
                case let .chats(watched):
                    var update = Update<ChatRow>(seq: watched.seq, remove: watched.remove, upsert: watched.rows,
                                                 total: watched.total)
                    if moved { update.moved = Moved(place: place, newer: 0) }
                    s.mergeChats(window, update)
                case let .messages(watched):
                    var update = Update<MessageRow>(seq: watched.seq, remove: watched.remove, upsert: watched.rows,
                                                    total: watched.total, older: watched.older, newer: watched.newer)
                    if moved { update.moved = Moved(place: place, newer: watched.newer) }
                    s.mergeMessages(window, update)
                default:
                    break
                }
                if follow {
                    lines.append(contentsOf: [asking(&s, window, again: false)].compactMap(\.self))
                }
                urgent = !follow
            case let .failure(failure):
                if w.closing {
                    s.windows[window] = nil
                    return true
                }
                let gone = if case let .jawd(error) = failure { error.code == ErrorCode.notFound } else { false }
                if landing.again == true, gone {
                    s.windows[window]!.place = Self.fallback(w.kind, place)
                    lines.append(contentsOf: [asking(&s, window, again: true)].compactMap(\.self))
                } else if w.place != place {
                    lines.append(contentsOf: [asking(&s, window, again: false)].compactMap(\.self))
                } else {
                    if let shown = w.shown { s.windows[window]!.place = shown }
                    failed = (window, failure)
                }
            }
            return true
        }
        for line in lines {
            write(line)
        }
        if let (window, failure) = failed {
            return event(.window(window, failure), generation, urgent: true)
        }
        if current { notify(urgent: urgent) }
        return current
    }

    /// The default anchor of a window's list, keeping as many rows, for a window whose anchor has gone.
    static func fallback(_ kind: Kind, _ place: Place) -> Place {
        let rows = place.before + place.after
        switch kind {
        case .chats: return .top(rows)
        case .messages: return .newest(rows)
        }
    }
}

/// A window the caller watches. It outlives the connection: jawd's own id for it changes each time
/// it's watched again.
public struct WindowID: Hashable, Comparable, Sendable {
    public let raw: UInt64

    public static func < (a: WindowID, b: WindowID) -> Bool {
        a.raw < b.raw
    }
}

/// Why a request has no answer from jawd, in words an interface can show.
public enum Failure: Error, Sendable, Hashable, CustomStringConvertible {
    /// jawd answered with an error.
    case jawd(JawError)
    /// The connection dropped after it was sent, so jawd may or may not have done it.
    case lost
    /// It wasn't sent: jawd isn't connected.
    case unreachable
    /// It wasn't sent: this many requests are already waiting for jawd to read them.
    case full(Int)

    public var description: String {
        switch self {
        case let .jawd(error): error.message
        case .lost: "the connection to jawd dropped before it answered, so it may not have been done"
        case .unreachable: "jawd isn't connected; jaw is trying to reach it, so try again in a moment"
        case let .full(n): "jawd has stopped reading what jaw sends: \(n) requests are waiting for it"
        }
    }

    public var code: Int? {
        if case let .jawd(error) = self { error.code } else { nil }
    }
}

/// What the caller learns from jawd, in the order it came.
public enum Event: Sendable {
    /// jawd answered hello, first or after connecting again.
    case connected(Welcome)
    /// jawd couldn't be reached, or the connection dropped; the client tries again in `retryIn`.
    case disconnected(why: String, retryIn: Duration)
    /// A request's answer.
    case reply(id: UInt64, Result<Reply, Failure>)
    /// The answer to the latest `find`; earlier ones are dropped.
    case found(id: UInt64, query: String, Result<Found, Failure>)
    /// A window couldn't be watched or moved as asked, and stays where it was.
    case window(WindowID, Failure)
    case connection(Connection)
    case link(LinkStep)
    case notice(Notice)
    /// Any other push.
    case push(method: String, params: JSONValue)
}

/// Everything that came since the last drain: events in order, and one merged update for each
/// window that changed.
public struct Frame: Sendable {
    public var events: [Event]
    public var chats: [WindowID: Update<ChatRow>]
    public var messages: [WindowID: Update<MessageRow>]

    public var isEmpty: Bool {
        events.isEmpty && chats.isEmpty && messages.isEmpty
    }
}

/// Counts since the client started.
public struct Stats: Sendable, Hashable {
    public var lines: UInt64 = 0
    public var skipped: UInt64 = 0
    public var connects: UInt64 = 0
    public var resyncs: UInt64 = 0
    public var frames: UInt64 = 0
}

/// Why jawd couldn't be reached.
struct Unreached: Error {
    let why: String
    init(_ why: String) {
        self.why = why
    }
}

enum Kind: Sendable {
    case chats, messages
}

/// A watch or a move in flight.
struct Landing: Sendable {
    let id: UInt64
    let window: WindowID
    let place: Place
    /// For a watch, whether the window is being watched again; a move has none.
    let again: Bool?
}

/// What a request in flight was, for its reply.
enum Asked: Sendable {
    case hello
    case request(Expect)
    case find(String)
    case window(Landing)
    case unwatch
}

struct Win: Sendable {
    let kind: Kind
    /// The folder or the chat.
    let of: String
    /// Where the caller wants it.
    var place: Place
    /// Where jawd last put it.
    var shown: Place?
    /// jawd's id for it on this connection.
    var watch: String?
    /// The watch or move in flight, by request id.
    var asking: UInt64?
    /// The ids its model will hold once what's merged is applied.
    var ids: Set<String> = []
    /// The newest seq its updates carried.
    var seq: UInt64?
    /// Unwatched while a request for it was in flight.
    var closing = false

    init(kind: Kind, of: String, place: Place) {
        self.kind = kind
        self.of = of
        self.place = place
    }
}

struct State: Sendable {
    var closed = false
    /// Counts connections, so a reader of one that has gone changes nothing.
    var generation: UInt64 = 0
    /// Whether the current connection takes requests: hello has gone and it hasn't dropped.
    var live = false
    /// Whether the current connection's hello was answered.
    var greeted = false
    var fd: Int32 = -1
    /// Why a write on the current connection failed, which ends it.
    var writeFailed: String?
    var focus: Bool
    var nextID: UInt64 = 0
    var asked: [UInt64: Asked] = [:]
    var queued = 0
    var nextWindow: UInt64 = 0
    var windows: [WindowID: Win] = [:]
    /// jawd's watch ids, to their windows.
    var watches: [String: WindowID] = [:]
    /// The latest find.
    var find: UInt64?
    var events: [Event] = []
    var chats: [WindowID: Merged<ChatRow>] = [:]
    var messages: [WindowID: Merged<MessageRow>] = [:]
    /// Whether the caller has been woken for what's in the inbox, and whether at once.
    var signalled = false
    var urgent = false
    var stats = Stats()

    init(focus: Bool) {
        self.focus = focus
    }

    mutating func id() -> UInt64 {
        nextID += 1
        return nextID
    }

    /// Whether a reader of connection `generation` may still change anything.
    func current(_ generation: UInt64) -> Bool {
        live && self.generation == generation
    }

    mutating func mergeChats(_ window: WindowID, _ update: Update<ChatRow>) {
        guard var w = windows[window], !w.closing else { return }
        w.seq = max(w.seq ?? update.seq, update.seq)
        chats[window, default: Merged()].add(update, ids: &w.ids)
        windows[window] = w
    }

    mutating func mergeMessages(_ window: WindowID, _ update: Update<MessageRow>) {
        guard var w = windows[window], !w.closing else { return }
        w.seq = max(w.seq ?? update.seq, update.seq)
        messages[window, default: Merged()].add(update, ids: &w.ids)
        windows[window] = w
    }
}
