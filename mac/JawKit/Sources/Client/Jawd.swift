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

    /// Sends a poll with the key its form made for it, as a message is sent: asked again with that key
    /// if the answer was lost, and handed back whole if jawd turns it down.
    public func sendPoll(chat: String, question: String, options: [String], max: Int, key: String) {
        let polling = Polling(params: SendPoll(chat: chat, question: question, options: options, max: max, key: key), first: now())
        ask("send.poll", polling.params, .sendPoll(polling), expect: .sent)
    }

    /// Your vote in a poll: every option you choose now, by name, none to take your vote back.
    @discardableResult
    public func vote(chat: String, message: String, options: [String]) -> String {
        let key = Self.key()
        act(.vote(PollVote(message: message, options: options, chat: chat, key: key)), chat: chat, key: key, what: "voting")
        return key
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

    /// Asks for the panes jawd keeps for `interface`, by the name it gives in hello. They come as
    /// `.layout`; a jawd that doesn't serve `layout.get` is said as `what` coming with the next step,
    /// and anything else that stops them, a malformed tree included, is said in its own words.
    public func layout(of interface: String, what: String) {
        ask("layout.get", LayoutGet(interface: interface), .layout(what), expect: .layout)
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

    // MARK: - Chats no window holds

    /// Where a lookup looks, in turn: the chats not archived, then the archived ones, the two lists
    /// that between them hold every chat jawd has.
    static let lookIn = ["all", "archived"]

    /// The chats still to look up, in turn.
    var toLookUp: [String] = []
    /// The chat being looked up, and its window of one row.
    var lookup: Lookup?

    /// Looks up the row of each of `chats`, which none of the caller's windows holds: a window of
    /// that one row around it in `all`, then in `archived` when `all` doesn't hold it, let go as soon
    /// as jawd answers, one chat at a time, so a lookup holds one of jawd's 32 windows at most. Each
    /// comes as `.looked`. The chats asked for before and not yet looked up are forgotten; the one
    /// being looked up still answers.
    public func lookUp(_ chats: [String]) {
        toLookUp = chats.filter { $0 != lookup?.chat }
        lookNext()
    }

    func lookNext() {
        guard lookup == nil, !toLookUp.isEmpty else { return }
        look(toLookUp.removeFirst(), in: Self.lookIn[0])
    }

    func look(_ chat: String, in folder: String) {
        lookup = Lookup(window: client.watchChats(folder, .around(chat, before: 0, after: 0)), chat: chat, folder: folder)
    }

    /// What a lookup's window answered: the chat's row, or that the list doesn't hold it, when the
    /// next list is looked in, and after the last, that it's gone. Its window is let go either way.
    func looked(_ update: Update<ChatRow>, _ out: inout [JawUpdate]) {
        guard let lookup else { return }
        let row = update.upsert.first { $0.id == lookup.chat }
        guard row != nil || update.moved != nil else { return }
        client.unwatch(lookup.window)
        self.lookup = nil
        if let row {
            out.append(.looked(chat: lookup.chat, .row(row)))
        } else if let next = Self.lookIn.drop(while: { $0 != lookup.folder }).dropFirst().first {
            look(lookup.chat, in: next)
            return
        } else {
            out.append(.looked(chat: lookup.chat, .gone))
        }
        lookNext()
    }

    /// A lookup's window that jawd refused: a chat it has no row for is gone, and any other refusal,
    /// such as every window being taken, is said in jawd's words.
    func lookFailed(_ failure: Failure, _ out: inout [JawUpdate]) {
        guard let lookup else { return }
        client.unwatch(lookup.window)
        self.lookup = nil
        out.append(.looked(chat: lookup.chat, failure.code == ErrorCode.notFound ? .gone : .refused(failure.description)))
        lookNext()
    }

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
            if window == lookup?.window { looked(update, &out) } else { out.append(.chats(window, update)) }
        }
        for (window, update) in frame.messages.sorted(by: { $0.key < $1.key }) {
            out.append(.messages(window, update))
        }
        // After the windows, which may bring in the chats it's for.
        if let typing = frame.typing { out.append(.typing(typing)) }
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
            if window == lookup?.window { lookFailed(failure, &out) } else { out.append(.windowFailed(window, failure.description)) }
        case let .connection(c):
            out.append(.connection(c))
        case let .link(step):
            out.append(.link(step))
        case let .notice(notice):
            out.append(.notice(notice.text, notice.tone))
        case let .push(method, params):
            out.append(.push(method: method, params: params))
        case let .unreadable(why):
            out.append(.notice(why, .error))
        }
    }

    func answered(_ asked: Asked, _ reply: Reply, _ out: inout [JawUpdate]) {
        switch (asked, reply) {
        case let (.send(sending), .sent(sent)):
            out.append(.sent(key: sending.params.key ?? "", message: sent.message, row: sent.row))
        case let (.sendPoll(polling), .sent(sent)):
            out.append(.sent(key: polling.params.key ?? "", message: sent.message, row: sent.row))
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
        case let (.layout, .layout(kept)):
            out.append(.layout(kept))
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

    /// A poll being sent, as `Sending` is a message.
    struct Polling {
        let params: SendPoll
        let first: Date
    }

    /// A chat being looked up: its window of one row, and the list it's looked for in.
    struct Lookup {
        let window: WindowID
        let chat: String
        let folder: String
    }

    enum ActParams {
        case react(React), edit(Edit), delete(Delete), forward(Forward), vote(PollVote)
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
        case sendPoll(Polling)
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
        case sendPoll(Polling)
        case act(Acting)
        case retry
        case owed(OwedKey, Owed)
        /// Something the user did, named as it's said: "linking".
        case did(String)
        case startChat
        case info(String)
        case members(String)
        case status
        /// Panes asked for, named as opening them is said: "opening the terminal's panes".
        case layout(String)
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
        case let .vote(p): ask("poll.vote", p, .act(acting))
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
                                   why: Self.awayTooLong))
            case let .send(sending):
                ask("send.text", sending.params, .send(sending), expect: .sent)
            case let .sendPoll(polling) where away(polling.first):
                out.append(.pollUnsent(chat: polling.params.chat, key: polling.params.key ?? "", poll: polling.params,
                                       why: Self.awayTooLong))
            case let .sendPoll(polling):
                ask("send.poll", polling.params, .sendPoll(polling), expect: .sent)
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

    /// Why a message or a poll whose answer was lost goes back to where it was written.
    static let awayTooLong = "jawd was away too long to tell whether it went, so look in the chat before sending it again"

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
        case let .sendPoll(polling):
            let chat = polling.params.chat, key = polling.params.key ?? ""
            if failure == .lost {
                again.append(.sendPoll(polling))
                return [.unsure(chat: chat, key: key)]
            }
            return [.pollUnsent(chat: chat, key: key, poll: polling.params, why: Self.whyUnsent(failure))]
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
        case let .layout(what):
            return [.notice(Self.whyNot(failure, what), .warning)]
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

/// What a lookup found of a chat no window of the caller's holds.
public enum Looked: Sendable, Hashable {
    /// Its row, from `all` or from `archived`.
    case row(ChatRow)
    /// In neither list, as for a chat jawd has no row for.
    case gone
    /// jawd refused its window, in jawd's words.
    case refused(String)
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
    /// Who's typing in each chat the windows show, all of it as it is now: a chat left out has
    /// nobody typing.
    case typing(Typists)
    case windowFailed(WindowID, String)
    /// A message jawd has, as the box's key named it.
    case sent(key: String, message: String, row: MessageRow?)
    /// A message jawd didn't take: its text goes back to its box, with why.
    case unsent(chat: String, key: String, text: String, why: String)
    /// A poll jawd didn't take: it goes back to its form whole, with why.
    case pollUnsent(chat: String, key: String, poll: SendPoll, why: String)
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
    /// The panes jawd keeps for an interface, as `layout.get` answered.
    case layout(KeptLayout)
    /// A chat looked up outside the caller's windows, and what was found of it.
    case looked(chat: String, Looked)
    case push(method: String, params: JSONValue)
}
