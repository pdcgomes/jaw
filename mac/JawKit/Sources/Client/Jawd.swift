import Foundation

/// jawd as the app sees it: the terminal's `wire.rs` over `Client`. What jawd turns down is said
/// where the user acted: a message that wasn't sent goes back to its box, an action says why, and
/// what goes on in the background (drafts, receipts, typing) says nothing. A message or an action
/// whose answer a dropped connection lost is asked for again with its key once jawd is back, and so
/// are the drafts and read positions jawd hadn't answered for.
@MainActor
public final class Jawd {
    /// How long after a message or an action was first asked for it's asked for again with its key.
    /// jawd keeps a key for a day, so past that a second ask could do it twice.
    public static let keyKept: TimeInterval = 20 * 60 * 60

    public let client: Client
    var asked: [UInt64: Asked] = [:]
    var again: [Again] = []
    /// The newest `draft.set` and `read` asked for each chat that jawd hasn't answered.
    var owed: [OwedKey: Owed] = [:]
    /// The client's id for each `find`, to the interface's.
    var finds: [UInt64: UInt64] = [:]
    /// How long ago things happened, by the wall clock, which jawd's day of keys is counted by.
    var now: () -> Date = Date.init

    public init(socket: String, client: String, version: String, focus: Bool,
                wake: @escaping @Sendable (Bool) -> Void)
    {
        self.client = Client(config: .init(socket: socket, client: client, version: version, focus: focus), wake: wake)
    }

    public func start() {
        client.start()
    }

    public func close() {
        client.close()
    }

    /// A key for a message or an action: `jaw-` and 32 hex digits, 128 random bits.
    public nonisolated static func key() -> String {
        var g = SystemRandomNumberGenerator()
        return "jaw-" + String(format: "%016llx%016llx", g.next() as UInt64, g.next() as UInt64)
    }

    // MARK: - Windows

    public func watchChats(_ folder: String, _ place: Place) -> WindowID {
        client.watchChats(folder, place)
    }

    public func watchMessages(_ chat: String, _ place: Place) -> WindowID {
        client.watchMessages(chat, place)
    }

    public func move(_ window: WindowID, _ place: Place) {
        client.move(window, place)
    }

    public func unwatch(_ window: WindowID) {
        client.unwatch(window)
    }

    // MARK: - Sending and acting

    /// Sends a message with the key the box made for it.
    public func sendText(chat: String, text: String, key: String, reply: String? = nil, mentions: [String] = []) {
        let sending = Sending(params: SendText(chat: chat, text: text, key: key, reply: reply, mentions: mentions),
                              first: now())
        ask("send.text", sending.params, .send(sending), expect: .sent)
    }

    @discardableResult
    public func react(chat: String, message: String, emoji: String) -> String {
        let key = Self.key()
        act(.react(React(message: message, emoji: emoji, chat: chat, key: key)), chat: chat, key: key, what: "reacting")
        return key
    }

    @discardableResult
    public func edit(chat: String, message: String, text: String) -> String {
        let key = Self.key()
        act(.edit(Edit(message: message, text: text, chat: chat, key: key)), chat: chat, key: key, what: "editing")
        return key
    }

    @discardableResult
    public func delete(chat: String, message: String, everyone: Bool) -> String {
        let key = Self.key()
        let params = Delete(message: message, for: everyone ? .everyone : .me, chat: chat, key: key)
        act(.delete(params), chat: chat, key: key, what: "deleting")
        return key
    }

    @discardableResult
    public func forward(chat: String, messages: [String], to: [String]) -> String {
        let key = Self.key()
        let done = to.count == 1 ? "forwarded" : "forwarded to \(to.count) chats"
        act(.forward(Forward(messages: messages, to: to, chat: chat, key: key)), chat: chat, key: key,
            what: "forwarding", done: done)
        return key
    }

    /// Pins, archives, mutes or marks a chat (`chat.pin`, `chat.archive`, `chat.mute`,
    /// `chat.mark_unread`); one jawd doesn't serve yet is said to come with its next step.
    public func chatAction(_ method: String, chat: String, _ fields: [String: JSONValue], what: String) {
        var params = fields
        params["chat"] = .string(chat)
        ask(method, JSONValue.object(params), .did(what))
    }

    public func retry(message: String) {
        ask("send.retry", MessageRef(message: message, chat: nil), .retry)
    }

    public func messageInfo(chat: String, message: String) {
        ask("message.info", MessageRef(message: message, chat: chat), .info(message), expect: .info)
    }

    public func chatInfo(_ chat: String) {
        ask("chat.info", ChatRef(chat: chat), .members(chat), expect: .chatInfo)
    }

    public func draft(chat: String, text: String) {
        owe(.draft(DraftSet(chat: chat, text: text)))
    }

    public func read(chat: String, upTo: String) {
        owe(.read(ReadUpTo(chat: chat, upTo: upTo)))
    }

    public func typing(chat: String, state: TypingState) {
        ask("typing", Typing(chat: chat, state: state), .quiet)
    }

    public func opened(_ chat: String) {
        ask("chat.opened", ChatRef(chat: chat), .quiet)
    }

    public func historyMore(_ chat: String) {
        ask("history.more", ChatRef(chat: chat), .quiet)
    }

    public func focus(_ focused: Bool) {
        if case let .success(id) = client.focus(focused) { asked[id] = .quiet }
    }

    public func linkStart(phone: String? = nil) {
        ask("link.start", LinkStart(phone: phone), .did("linking"))
    }

    public func linkCancel() {
        ask("link.cancel", NoParams(), .quiet)
    }

    public func status() {
        ask("status", NoParams?.none, .status, expect: .status)
    }

    public func startChat(person: String) {
        ask("chat.start", ChatStart(phone: nil, person: person), .startChat, expect: .chat)
    }

    public func startChat(phone: String) {
        ask("chat.start", ChatStart(phone: phone, person: nil), .startChat, expect: .chat)
    }

    /// Asks `find` for the palette, which keeps only the answer to the latest one, numbered `id`.
    public func find(id: UInt64, query: String, kinds: [FindKind] = [], chat: String? = nil, limit: UInt32? = nil) {
        switch client.find(Find(query: query, kinds: kinds, chat: chat, limit: limit)) {
        case let .success(asked): finds[asked] = id
        case let .failure(failure): pending.append(.findFailed(id: id, cancelled: false, why: failure.description))
        }
    }

    /// What was said without asking jawd, waiting for the next drain.
    var pending: [JawUpdate] = []

    // MARK: - Draining

    /// Everything since the last drain, as the app takes it.
    public func drain() -> [JawUpdate] {
        let frame = client.drain()
        var out = pending
        pending.removeAll()
        for event in frame.events {
            take(event, &out)
        }
        for (window, update) in frame.chats.sorted(by: { $0.key < $1.key }) {
            out.append(.chats(window, update))
        }
        for (window, update) in frame.messages.sorted(by: { $0.key < $1.key }) {
            out.append(.messages(window, update))
        }
        return out
    }

    func take(_ event: Event, _ out: inout [JawUpdate]) {
        switch event {
        case let .connected(welcome):
            var me: [String] = []
            if let m = welcome.account.me {
                me.append(m.id)
                let digits = m.phone.filter(\.isASCII).filter(\.isNumber)
                if !digits.isEmpty { me.append("\(digits)@s.whatsapp.net") }
            }
            out.append(.hello(welcome, me: me))
            askAgain(&out)
        case let .disconnected(why, retryIn):
            out.append(.unreachable(why: why, retryIn: retryIn))
        case let .reply(id, outcome):
            guard let asked = asked.removeValue(forKey: id) else { return }
            switch outcome {
            case let .success(reply): answered(asked, reply, &out)
            case let .failure(failure): out.append(contentsOf: refused(asked, failure))
            }
        case let .found(id, _, outcome):
            guard let view = finds.removeValue(forKey: id) else { return }
            finds = finds.filter { $0.key > id }
            switch outcome {
            case let .success(found): out.append(.found(id: view, found))
            case let .failure(.jawd(e)) where e.code == ErrorCode.cancelled:
                out.append(.findFailed(id: view, cancelled: true, why: e.message))
            case let .failure(failure): out.append(.findFailed(id: view, cancelled: false, why: failure.description))
            }
        case let .window(window, failure):
            out.append(.windowFailed(window, failure.description))
        case let .connection(c):
            out.append(.connection(c))
        case let .link(step):
            out.append(.link(step))
        case let .notice(notice):
            out.append(.notice(notice.text, notice.tone))
        case let .push(method, params):
            out.append(.push(method: method, params: params))
        }
    }

    func answered(_ asked: Asked, _ reply: Reply, _ out: inout [JawUpdate]) {
        switch (asked, reply) {
        case let (.send(sending), .sent(sent)):
            out.append(.sent(key: sending.params.key ?? "", message: sent.message, row: sent.row))
        case let (.act(acting), _):
            out.append(.acted(key: acting.key, said: acting.done))
        case let (.owed(key, owed), _):
            settle(key, owed)
        case let (.startChat, .chat(row)):
            out.append(.chatStarted(row))
        case let (.info(message), .info(info)):
            out.append(.info(message: message, info: info, why: nil))
        case let (.members(chat), .chatInfo(info)):
            out.append(.members(chat: chat, info.members))
        case let (.status, .status(status)):
            out.append(.status(status))
        default:
            break
        }
    }

    // MARK: - Inside

    struct Sending {
        let params: SendText
        /// When it was first asked for, by the wall clock, which goes on while the Mac sleeps.
        let first: Date
    }

    enum ActParams {
        case react(React), edit(Edit), delete(Delete), forward(Forward)
    }

    /// An action on messages, asked with a key made for it alone and kept until jawd answers for it.
    struct Acting {
        let params: ActParams
        let chat: String
        let key: String
        /// What it is, as it's said: "reacting".
        let what: String
        /// What's said once jawd has done it, if anything.
        let done: String?
        let first: Date
    }

    /// What a dropped connection lost the answer to, asked for again with its key once jawd is
    /// back, in the order it was first asked, so two edits of a message end as the second.
    enum Again {
        case send(Sending)
        case act(Acting)
    }

    struct OwedKey: Hashable {
        let method: String
        let chat: String
    }

    enum Owed: Hashable {
        case draft(DraftSet), read(ReadUpTo)

        var key: OwedKey {
            switch self {
            case let .draft(d): OwedKey(method: "draft.set", chat: d.chat)
            case let .read(r): OwedKey(method: "read", chat: r.chat)
            }
        }
    }

    /// What a request was, for what to say if jawd turns it down.
    enum Asked {
        case send(Sending)
        case act(Acting)
        case retry
        case owed(OwedKey, Owed)
        /// Something the user did, named as it's said: "linking".
        case did(String)
        case startChat
        case info(String)
        case members(String)
        case status
        case quiet
    }

    func ask(_ method: String, _ params: (some Encodable & Sendable)?, _ what: Asked, expect: Expect = .json) {
        switch client.request(method, params, expect: expect) {
        case let .success(id): asked[id] = what
        case let .failure(failure): pending.append(contentsOf: refused(what, failure))
        }
    }

    func act(_ params: ActParams, chat: String, key: String, what: String, done: String? = nil) {
        let acting = Acting(params: params, chat: chat, key: key, what: what, done: done, first: now())
        ask(acting)
    }

    func ask(_ acting: Acting) {
        switch acting.params {
        case let .react(p): ask("react", p, .act(acting))
        case let .edit(p): ask("edit", p, .act(acting))
        case let .delete(p): ask("delete", p, .act(acting))
        case let .forward(p): ask("forward", p, .act(acting))
        }
    }

    func owe(_ owing: Owed) {
        owed[owing.key] = owing
        ask(owing)
    }

    func ask(_ owed: Owed) {
        switch owed {
        case let .draft(p): ask("draft.set", p, .owed(owed.key, owed))
        case let .read(p): ask("read", p, .owed(owed.key, owed))
        }
    }

    /// Settles what was owed for a chat, unless something newer has been asked for it since.
    func settle(_ key: OwedKey, _ owed: Owed) {
        if self.owed[key] == owed { self.owed[key] = nil }
    }

    /// jawd is back: what the connection lost is asked for again, messages and actions with their
    /// keys, so jawd answers for any it already has rather than doing it again.
    func askAgain(_ out: inout [JawUpdate]) {
        let away = { (first: Date) in self.now().timeIntervalSince(first) > Self.keyKept }
        let lost = again
        again.removeAll()
        for item in lost {
            switch item {
            case let .send(sending) where away(sending.first):
                out.append(.unsent(chat: sending.params.chat, key: sending.params.key ?? "", text: sending.params.text,
                                   why: "jawd was away too long to tell whether it went, so look in the chat before sending it again"))
            case let .send(sending):
                ask("send.text", sending.params, .send(sending), expect: .sent)
            case let .act(acting) where away(acting.first):
                out.append(.acted(key: acting.key,
                                  said: "jawd was away too long to tell whether it was done, so look in the chat before \(acting.what) again"))
            case let .act(acting):
                ask(acting)
            }
        }
        for owed in owed.values.sorted(by: { "\($0.key)" < "\($1.key)" }) {
            ask(owed)
        }
    }

    /// What the app hears of a request jawd didn't do.
    func refused(_ asked: Asked, _ failure: Failure) -> [JawUpdate] {
        switch asked {
        case let .send(sending):
            let chat = sending.params.chat, key = sending.params.key ?? ""
            if failure == .lost {
                again.append(.send(sending))
                return [.unsure(chat: chat, key: key)]
            }
            return [.unsent(chat: chat, key: key, text: sending.params.text, why: Self.whyUnsent(failure))]
        case let .act(acting):
            if failure == .lost {
                again.append(.act(acting))
                return [.unsure(chat: acting.chat, key: acting.key)]
            }
            return [.acted(key: acting.key, said: Self.whyNot(failure, acting.what))]
        case .retry:
            return [.notice("not sent again: \(failure)", .warning)]
        case let .owed(key, owed):
            guard case let .jawd(e) = failure else { return [] }
            settle(key, owed)
            return key.method == "draft.set" ? [.notice("the draft wasn't kept: \(e.message)", .warning)] : []
        case let .did(what):
            return [.notice(Self.whyNot(failure, what), .warning)]
        case .startChat:
            return [.notStarted(failure.description)]
        case let .info(message):
            return [.info(message: message, info: nil, why: Self.whyNot(failure, "a message's info"))]
        case let .members(chat):
            return [.members(chat: chat, [])]
        case .status, .quiet:
            return []
        }
    }

    /// What to say about a request jawd turned down: one it doesn't serve yet comes later.
    public nonisolated static func whyNot(_ failure: Failure, _ what: String) -> String {
        if case let .jawd(e) = failure, e.code == ErrorCode.unknownMethod {
            return "\(what) comes with the next step"
        }
        return failure.description
    }

    /// Why jawd didn't take a message, for its box: the outbox's limit in a few words, or jawd's own.
    public nonisolated static func whyUnsent(_ failure: Failure) -> String {
        guard case let .jawd(e) = failure, e.code == ErrorCode.limit else { return failure.description }
        guard e.data?["limit"]?.string == "outbox", let waiting = e.data?["waiting"]?.int else { return e.message }
        let failed = (e.data?["failed"]?.int).flatMap { $0 > 0 ? ", \($0) failed" : nil } ?? ""
        return "\(waiting) wait to send\(failed); it can go once some have"
    }
}

/// What the app hears from jawd, drained once a frame.
public enum JawUpdate: Sendable {
    /// jawd answered hello: the folder its paths are relative to, and the IDs you go by, so a
    /// mention of you stands out.
    case hello(Welcome, me: [String])
    case unreachable(why: String, retryIn: Duration)
    case connection(Connection)
    case link(LinkStep)
    case chats(WindowID, Update<ChatRow>)
    case messages(WindowID, Update<MessageRow>)
    case windowFailed(WindowID, String)
    /// A message jawd has, as the box's key named it.
    case sent(key: String, message: String, row: MessageRow?)
    /// A message jawd didn't take: its text goes back to its box, with why.
    case unsent(chat: String, key: String, text: String, why: String)
    /// A message or an action whose answer was lost, to be asked for again once jawd is back.
    case unsure(chat: String, key: String)
    /// An action jawd answered for, and what to say of it, if anything.
    case acted(key: String, said: String?)
    case found(id: UInt64, Found)
    case findFailed(id: UInt64, cancelled: Bool, why: String)
    case notice(String, Notice.Tone)
    case info(message: String, info: MessageInfo?, why: String?)
    case members(chat: String, [ChatInfo.Member])
    case chatStarted(ChatRow)
    case notStarted(String)
    case status(Status)
    case push(method: String, params: JSONValue)
}
