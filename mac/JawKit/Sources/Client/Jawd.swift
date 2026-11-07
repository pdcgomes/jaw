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

    /// The library's window for `filter`: a kind's items, or the newest of every kind.
    public func watchLibrary(_ filter: LibraryFilter, _ place: Place) -> WindowID {
        client.watchLibrary(filter, place)
    }

    /// Names the library's items on screen, by `id`, 200 at most, so jawd asks media for what they
    /// lack, as it does for a messages window's rows: their thumbnails, previews and files.
    public func shown(_ items: [String]) {
        ask("library.shown", LibraryShown(items: Array(items.prefix(LibraryShown.most))), .quiet)
    }

    public func watchFolders() -> WindowID {
        client.watchFolders()
    }

    public func watchRoster(_ folder: String, _ place: Place) -> WindowID {
        client.watchRoster(folder, place)
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

    /// Sends contact cards, one or several in one message, with the key their form made for them, as
    /// a message is sent: asked again with that key if the answer was lost, and handed back whole if
    /// jawd turns them down.
    public func sendContact(chat: String, cards: [SendContact.Card], key: String) {
        let carding = Carding(params: SendContact(chat: chat, cards: cards, key: key), first: now())
        ask("send.contact", carding.params, .sendContact(carding), expect: .sent)
    }

    /// Sends files to a chat with the key the box made for them, a message each, as a message is
    /// sent: asked again with that key if the answer was lost, and handed back whole if jawd turns
    /// them down.
    public func sendMedia(chat: String, files: [SendMedia.File], quality: Variant, reply: String?, key: String) {
        let attaching = Attaching(params: SendMedia(chat: chat, files: files, quality: quality, reply: reply, key: key), first: now())
        ask("send.media", attaching.params, .sendMedia(attaching), expect: .sentMedia)
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

    // MARK: - Folders and favourites

    /// Makes a folder of yours, named in place: `named` with its row once it's made, or with none if
    /// it went before jawd read it back, or `notNamed` with jawd's words for the field.
    public func folderCreate(name: String, parent: String?) {
        ask("folder.create", FolderCreate(name: name, parent: parent), .naming, expect: .folder)
    }

    public func folderRename(_ folder: String, name: String) {
        ask("folder.rename", FolderRename(folder: folder, name: name), .naming)
    }

    /// Nests a folder of yours in `parent`, or puts it back at the top, at `position` among its new
    /// siblings or last; `said` once jawd has done it.
    public func folderMove(_ folder: String, parent: String?, position: Int?, said: String?) {
        ask("folder.move", FolderMove(folder: folder, parent: parent, position: position), .filed(nil, said: said))
    }

    public func folderDelete(_ folder: String, said: String?) {
        ask("folder.delete", FolderID(folder: folder), .filed(nil, said: said))
    }

    public func folderAdd(_ folder: String, item: String, said: String?) {
        ask("folder.add", FolderItem(folder: folder, item: item), .filed(.added(item: item, to: folder), said: said))
    }

    public func folderRemove(_ folder: String, item: String, said: String?) {
        ask("folder.remove", FolderItem(folder: folder, item: item), .filed(.removed(item: item, from: folder), said: said))
    }

    /// Files `item` in `to`, and takes it out of `from` only once jawd has it there, so a failure
    /// between the two leaves it in both, never in neither, and says so with `both`; a half whose
    /// answer a drop lost is asked again once jawd is back, since adding and removing are idempotent.
    public func moveItem(_ item: String, from: String, to: String, said: String?, both: String) {
        ask(Moving(item: item, from: from, to: to, said: said, both: both))
    }

    func ask(_ moving: Moving) {
        let folder = moving.removing ? moving.from : moving.to
        ask(moving.removing ? "folder.remove" : "folder.add", FolderItem(folder: folder, item: moving.item), .moving(moving))
    }

    /// Puts a folder or a chat on `slot`, taking it off the slot it had and putting out what the slot
    /// held; nil empties the slot.
    public func favouriteSet(slot: Int, target: String?, said: String?) {
        ask("favourite.set", FavouriteSet(slot: slot, target: target), .filed(nil, said: said))
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

    /// Logs the account out of WhatsApp; once jawd has forgotten it, its `connection` push says so,
    /// logged out from jaw, and the window shows linking again.
    public func linkLogout() {
        ask("link.logout", NoParams(), .did("logging out"))
    }

    /// jawd's status: shown, as the Help menu asks for it, or read for what the app needs to know of
    /// it, what jawd is missing, as `.statusRead`.
    public func status(shown: Bool = true) {
        ask("status", NoParams?.none, shown ? .status : .statusRead, expect: .status)
    }

    /// Asks for Omarchy's themes, which come as `.themes`; a jawd that doesn't serve them is said as
    /// `.noThemes`, and a drop says nothing, as the app asks again at the next hello.
    public func themes() {
        ask("themes", NoParams?.none, .themes, expect: .themes)
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

    // MARK: - Media

    /// Asks for a message's file now (`media.fetch`), ahead of what downloads by itself; its progress
    /// and its file come as upserts to the message. One jawd turns down is said, as the message it
    /// was for, so what waited to open it stops waiting.
    public func fetch(chat: String, message: String, variant: Variant? = nil) {
        ask("media.fetch", MediaFetch(message: message, chat: chat, variant: variant), .fetch(message))
    }

    /// Saves a message's file to `to`, a folder or a file's path (`media.save`), downloading it first
    /// if it isn't here yet. jawd answers once the copy is whole and synced, which can take minutes,
    /// as `.saved` with the ticket this returns.
    @discardableResult
    public func save(chat: String, message: String, to: String, variant: Variant? = nil) -> String {
        let ticket = Self.key()
        ask("media.save", MediaSave(message: message, to: to, chat: chat, variant: variant), .save(ticket, message),
            expect: .saved)
        return ticket
    }

    /// Stops a message's download and anything waiting to be done for it, or a file of yours on its
    /// way (`media.cancel`); what's stopped is said as `what` if jawd doesn't.
    public func cancel(chat: String, message: String, what: String = "stopping the download") {
        ask("media.cancel", MediaCancel(message: message, chat: chat), .did(what))
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
        for (window, update) in frame.library.sorted(by: { $0.key < $1.key }) {
            out.append(.library(window, update))
        }
        for (window, update) in frame.folders.sorted(by: { $0.key < $1.key }) {
            out.append(.folders(window, update))
        }
        for (window, update) in frame.roster.sorted(by: { $0.key < $1.key }) {
            out.append(.roster(window, update))
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
            if window == lookup?.window { lookFailed(failure, &out) } else { out.append(.windowFailed(window, failure)) }
        case let .connection(c):
            out.append(.connection(c))
        case let .link(step):
            out.append(.link(step))
        case let .notice(notice):
            out.append(.notice(notice.text, notice.tone))
        case let .job(job):
            out.append(.job(job))
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
        case let (.sendContact(carding), .sent(sent)):
            out.append(.sent(key: carding.params.key ?? "", message: sent.message, row: sent.row))
        case let (.sendMedia(attaching), .sentMedia(sent)):
            out.append(.mediaSent(key: attaching.params.key ?? "", messages: sent.messages, rows: sent.rows))
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
        case let (.themes, .themes(themes, unread)):
            out.append(.themes(themes, unread: unread))
        case let (.statusRead, .status(status)):
            out.append(.statusRead(status))
        case let (.layout, .layout(kept)):
            out.append(.layout(kept))
        case let (.save(ticket, message), .saved(saved)):
            out.append(.saved(ticket: ticket, message: message, path: saved.path, why: nil))
        case let (.naming, .folder(row)):
            out.append(.named(row))
        case (.naming, _):
            out.append(.named(nil))
        case let (.filed(filing, said), _):
            if let filing { out.append(.filed(filing)) }
            if let said { out.append(.notice(said, .info)) }
        case let (.moving(moving), _) where !moving.removing:
            var rest = moving
            rest.removing = true
            ask(rest)
        case let (.moving(moving), _):
            out.append(.filed(moving.moved))
            if let said = moving.said { out.append(.notice(said, .info)) }
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

    /// Contact cards being sent, as `Sending` is a message.
    struct Carding {
        let params: SendContact
        let first: Date
    }

    /// Files being sent, as `Sending` is a message.
    struct Attaching {
        let params: SendMedia
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

    /// Move to: `item` filed in `to`, then taken out of `from` once jawd has it there.
    struct Moving {
        let item: String
        let from: String
        let to: String
        let said: String?
        /// What's said if it ends in both folders.
        let both: String
        /// Whether jawd has it in `to`, so taking it out of `from` is what's left.
        var removing = false
        /// Whether a drop has lost it, its answer or its request, so it's asked again whenever jawd
        /// is back until jawd has answered.
        var lost = false

        var moved: Filing {
            .moved(item: item, from: from, to: to)
        }
    }

    /// What a dropped connection lost the answer to, asked for again with its key once jawd is
    /// back, in the order it was first asked, so two edits of a message end as the second.
    enum Again {
        case send(Sending)
        case sendPoll(Polling)
        case sendContact(Carding)
        case sendMedia(Attaching)
        case act(Acting)
        case move(Moving)
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
        case sendContact(Carding)
        case sendMedia(Attaching)
        case act(Acting)
        case retry
        case owed(OwedKey, Owed)
        /// Something the user did, named as it's said: "linking".
        case did(String)
        case startChat
        case info(String)
        case members(String)
        case status
        case themes
        /// `status` read, not shown.
        case statusRead
        /// Panes asked for, named as opening them is said: "opening the terminal's panes".
        case layout(String)
        /// `media.fetch` for a message, and `media.save` under its ticket.
        case fetch(String)
        case save(String, String)
        /// A folder's name, made or changed in place.
        case naming
        /// A folder change, what it does to an item's folders if it files one, and what's said once
        /// jawd has done it.
        case filed(Filing?, said: String?)
        /// A half of Move to.
        case moving(Moving)
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
            case let .sendContact(carding) where away(carding.first):
                out.append(.contactUnsent(chat: carding.params.chat, key: carding.params.key ?? "", cards: carding.params.cards,
                                          why: Self.awayTooLong))
            case let .sendContact(carding):
                ask("send.contact", carding.params, .sendContact(carding), expect: .sent)
            case let .sendMedia(attaching) where away(attaching.first):
                out.append(.mediaUnsent(chat: attaching.params.chat, key: attaching.params.key ?? "", files: attaching.params.files,
                                        why: Self.awayTooLong))
            case let .sendMedia(attaching):
                ask("send.media", attaching.params, .sendMedia(attaching), expect: .sentMedia)
            case let .act(acting) where away(acting.first):
                out.append(.acted(key: acting.key,
                                  said: "jawd was away too long to tell whether it was done, so look in the chat before \(acting.what) again"))
            case let .act(acting):
                ask(acting)
            case let .move(moving):
                ask(moving)
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
        case let .sendContact(carding):
            let chat = carding.params.chat, key = carding.params.key ?? ""
            if failure == .lost {
                again.append(.sendContact(carding))
                return [.unsure(chat: chat, key: key)]
            }
            let why = failure.code == ErrorCode.unknownMethod ? Self.whyNot(failure, "sending contacts") : Self.whyUnsent(failure)
            return [.contactUnsent(chat: chat, key: key, cards: carding.params.cards, why: why)]
        case let .sendMedia(attaching):
            let chat = attaching.params.chat, key = attaching.params.key ?? ""
            if failure == .lost {
                again.append(.sendMedia(attaching))
                return [.unsure(chat: chat, key: key)]
            }
            let why = failure.code == ErrorCode.unknownMethod ? Self.whyNot(failure, "sending files") : Self.whyFilesUnsent(failure)
            return [.mediaUnsent(chat: chat, key: key, files: attaching.params.files, why: why)]
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
        case let .fetch(message):
            return [.notFetched(message: message, why: Self.whyNot(failure, "downloading"))]
        case let .save(ticket, message):
            return [.saved(ticket: ticket, message: message, path: nil, why: Self.whyNot(failure, "saving"))]
        case .naming:
            return [.notNamed(Self.whyNot(failure, "naming folders"))]
        case .filed:
            return [.notice(Self.whyNot(failure, "filing"), .warning)]
        case let .moving(moving):
            // The second half is asked as the first's answer is read, and a lost half as jawd comes
            // back, so a drop can cut either off unsent.
            if failure == .lost || (failure == .unreachable && (moving.removing || moving.lost)) {
                var later = moving
                later.lost = true
                again.append(.move(later))
                return []
            }
            guard moving.removing else { return [.notice(Self.whyNot(failure, "moving between folders"), .warning)] }
            // Taking out what isn't there is done already, so not finding `from` or the chat means
            // it went between the halves, leaving the chat in `to` alone or, when it's the chat that
            // went, in no folder, its row going too.
            if failure.code == ErrorCode.notFound {
                return [.filed(moving.moved)] + (moving.said.map { [.notice($0, .info)] } ?? [])
            }
            return [.filed(.added(item: moving.item, to: moving.to)),
                    .notice("\(moving.both): \(Self.whyNot(failure, "taking it out"))", .warning)]
        case .themes:
            if failure == .lost || failure == .unreachable { return [] }
            return [.noThemes(older: failure.code == ErrorCode.unknownMethod, why: failure.description)]
        case .status, .statusRead, .quiet:
            return []
        }
    }

    /// Why jawd didn't take files, for the box: as `whyUnsent`, a file jawd names by its path named by
    /// its name, as the strip shows it.
    public nonisolated static func whyFilesUnsent(_ failure: Failure) -> String {
        let why = whyUnsent(failure)
        guard case let .jawd(e) = failure, let path = e.data?["path"]?.string, !path.isEmpty else { return why }
        return why.replacingOccurrences(of: path, with: (path as NSString).lastPathComponent)
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
    case library(WindowID, Update<LibraryRow>)
    case folders(WindowID, Update<FolderRow>)
    case roster(WindowID, Update<RosterRow>)
    /// Who's typing in each chat the windows show, all of it as it is now: a chat left out has
    /// nobody typing.
    case typing(Typists)
    /// A folder named in place: made, with its row, or renamed, or made and gone before jawd read it
    /// back, with none.
    case named(FolderRow?)
    /// A name jawd turned down, in its words, for under the field.
    case notNamed(String)
    /// A filing jawd has done, so a row held for the item can show it where jawd's pushes don't
    /// reach; what's said of it comes as a notice.
    case filed(Filing)
    case windowFailed(WindowID, Failure)
    /// A message jawd has, as the box's key named it.
    case sent(key: String, message: String, row: MessageRow?)
    /// A message jawd didn't take: its text goes back to its box, with why.
    case unsent(chat: String, key: String, text: String, why: String)
    /// A poll jawd didn't take: it goes back to its form whole, with why.
    case pollUnsent(chat: String, key: String, poll: SendPoll, why: String)
    /// Contact cards jawd didn't take: they go back to their form whole, with why.
    case contactUnsent(chat: String, key: String, cards: [SendContact.Card], why: String)
    /// Files jawd has, as the box's key named them: their messages and their rows, the album's first.
    case mediaSent(key: String, messages: [String], rows: [MessageRow])
    /// Files jawd didn't take: they go back to their box whole, with why.
    case mediaUnsent(chat: String, key: String, files: [SendMedia.File], why: String)
    /// How far a copy of one of your files has got.
    case job(Job)
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
    /// Omarchy's themes, as `themes` answered, and the IDs of those it served in a shape this Mac can't
    /// read, whose copies the slots that hold them keep.
    case themes([ServedTheme], unread: [String])
    /// jawd didn't serve its themes: an older one, which doesn't know `themes`, or another refusal, in
    /// jawd's words.
    case noThemes(older: Bool, why: String)
    /// jawd's status, read rather than shown.
    case statusRead(Status)
    /// The panes jawd keeps for an interface, as `layout.get` answered.
    case layout(KeptLayout)
    /// A chat looked up outside the caller's windows, and what was found of it.
    case looked(chat: String, Looked)
    /// jawd couldn't take `media.fetch` for a message, and why, in its words.
    case notFetched(message: String, why: String)
    /// `media.save`'s answer under its ticket: where the copy is, or why there's none.
    case saved(ticket: String, message: String, path: String?, why: String?)
    case push(method: String, params: JSONValue)
}

/// What jawd has done to an item's folders, once it answers a filing: filed in one, taken out of
/// one, or moved from one to another.
public enum Filing: Sendable, Hashable {
    case added(item: String, to: String)
    case removed(item: String, from: String)
    case moved(item: String, from: String, to: String)

    /// A chat's id, or a `person:` id.
    public var item: String {
        switch self {
        case let .added(item, _), let .removed(item, _), let .moved(item, _, _): item
        }
    }

    /// The folders the item is in now, from those a row held for it said.
    public func applied(to folders: [String]) -> [String] {
        switch self {
        case let .added(_, to): folders.contains(to) ? folders : folders + [to]
        case let .removed(_, from): folders.filter { $0 != from }
        case let .moved(item, from, to): Filing.added(item: item, to: to).applied(to: folders.filter { $0 != from })
        }
    }
}
