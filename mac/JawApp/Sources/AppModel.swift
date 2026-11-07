import AppKit
import JawKit
import JawThemes
import JawUI
import SwiftUI

/// The app as one interface on jawd: the connection, the chat list's window and one window for each
/// pane's chat, what's said, and focus. jawd's updates are drained at the next display frame, or at
/// once for an answer the screen waits for, and applied before the frame draws.
@MainActor
final class AppModel {
    let launch: Launch
    let agent: Agent
    let preferences: Preferences
    let layouts = Layouts()
    /// Every pane's pictures, decoded once, at most 64 MB of them.
    let pictures = MessagePictures()
    var jawd: Jawd!
    weak var window: MainWindowController?

    typealias Reach = Situation.Reach

    var reach: Reach = .waiting {
        didSet { preferences.omarchyState = OmarchyState(reach) }
    }
    var welcome: Welcome?
    var connection = Connection(state: .connecting)
    var linked = true
    var link: LinkStep?
    var me: Set<String> = []
    var listWindow: WindowID?
    var foldersWindow: WindowID?
    /// The key the menu bar's folder items were last made from.
    var folderItemsMade: [String] = []
    /// The tree's part of that key, and the sidebar's count of the tree's changes it was made at.
    var treeItemsKey: (changes: Int, key: [String])?
    /// A chat a favourite or the roster opens once jawd finds its row.
    var opening: Opening?
    var rosterWindow: WindowID?
    /// The chats started and not yet answered, each with whether it opens in a pane of its own.
    var starts = ChatStarts()
    var folder = "all"
    /// Who's typing in the chats jawd's windows show, as it last said.
    var typing: Typists = [:]
    /// Each pane's conversation, by the chat it shows; a chat shows in one pane at most.
    var panes: [String: ConversationController] = [:]
    var paneWindows: [WindowID: String] = [:]
    /// Each library pane, by its slot's chat (`LibraryPane`), its window, and the chat each was opened
    /// from, which takes its place when it closes with no other pane.
    var libraries: [String: LibraryController] = [:]
    var libraryWindows: [WindowID: String] = [:]
    var libraryCameFrom: [String: String] = [:]
    /// The named layouts and the one on screen.
    var book = LayoutBook()
    /// The kept layouts couldn't be read, so they're left as they are rather than written over.
    var keepKept = false
    var focusedSlot: Int?
    var restored = false
    /// What a slot of the layout on screen says whose chat won't have a pane, by that chat: that it's
    /// gone, or jawd's words for refusing to look it up.
    var unopened: [String: String] = [:]
    let glance = Glance()
    /// Settings' window, made the first time it's shown.
    var settings: SettingsWindowController?
    var focusMode = false
    /// Everything the app does, made once; the menus, the palette and the keys sheet read it.
    lazy var commands: [Command] = makeCommands()
    /// Messages sent and not yet answered for, by key, so a refused one goes back to its box.
    var sending: [String: (chat: String, text: String)] = [:]
    /// Votes asked and not yet answered for, by key, so a pane forgets one jawd didn't do.
    var voting: [String: (chat: String, message: String)] = [:]
    /// The contact forms whose cards are on their way, so one jawd turns down opens again.
    let contactsSent = ContactFormsSent()
    /// Files sent from a box and not yet answered for, by key, so those jawd turns down go back to
    /// their strip, and those the app wrote go once jawd has its own copy.
    var filesSent: [String: (chat: String, files: [AttachedFile], caption: String, hd: Bool)] = [:]
    /// What jawd says sending videos, GIFs and audio files needs, when its status says it has no
    /// ffmpeg.
    var ffmpegMissing: String?
    let contactPicking = ContactPicking()
    var notice: (text: String, until: Date, warn: Bool)?
    var infoFor: MessageRow?
    var focused = false
    /// What the launch bench waits for: the first frame with the chat list's rows.
    var firstRows: (() -> Void)?
    /// A file asked to be looked at or opened before it was here, to be once it is.
    var pending: PendingOpen?
    /// Saves asked of jawd, by ticket: the message, whether to show the copy in the Finder once it's
    /// made, and the drag's promise, which takes the file jawd wrote.
    var saving: [String: (message: String, reveal: Bool, done: (@Sendable (Result<URL, any Error>) -> Void)?)] = [:]
    /// Where each message's file was saved this run, for Show in Finder.
    var saved: [String: URL] = [:]
    /// What the pick bench waits for: the chat list's update reaching the app, before it's applied.
    var listCame: (() -> Void)?
    /// `JAW_TRACE=1` says each update drained and when, and what the window shows of jawd, on
    /// standard error.
    let trace = ProcessInfo.processInfo.environment["JAW_TRACE"] != nil
    /// What the trace last said the window shows of jawd.
    var traced: String?

    /// The focused pane's conversation, the one the keys and the menus act on.
    var conversation: ConversationController? {
        focusedChat.flatMap { panes[$0] }
    }

    /// The focused pane's library, when it's one, whose selected item the Message menu's file
    /// commands act on.
    var focusedLibrary: LibraryController? {
        focusedChat.flatMap { libraries[$0] }
    }

    init(launch: Launch) {
        self.launch = launch
        agent = Agent(launch: launch)
        preferences = Preferences(keeps: launch.snapshot == nil)
    }

    var dark: Bool {
        preferences.isDark(system: NSApp.effectiveAppearance)
    }

    var style: Style {
        preferences.style(dark: dark)
    }

    var roles: Roles {
        style.roles
    }

    var spec: LayoutSpec {
        if let made = madeSpec, made.style == style, made.me == me { return made }
        let spec = LayoutSpec(style: style, me: me)
        madeSpec = spec
        return spec
    }

    var madeSpec: LayoutSpec?

    func start() {
        agent.say = { [weak self] text, warn in self?.say(text, warn: warn) }
        reach = Reach(agent.launched())
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
        jawd = Jawd(socket: launch.socket, client: "jaw-mac", version: version, focus: NSApp.isActive) { urgent in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let model = AppDelegate.shared?.model else { return }
                    if urgent { model.drain() } else { model.window?.wantFrame() }
                }
            }
        }
        jawd.start()
        Opened.sweep()
        listWindow = jawd.watchChats(folder, .top(ChatListController.rows))
        foldersWindow = jawd.watchFolders()
        preferences.changed.append { [weak self] in self?.restyle() }
        glance.model = self
        window?.refresh()
        if trace { DispatchQueue.main.async { [weak self] in self?.traceShown() } }
    }

    func stop() {
        for c in panes.values {
            c.leave()
        }
        savePanes()
        jawd?.close()
        agent.stop()
        // A file written for a send jawd hasn't answered for may still be being kept: the next
        // start's sweep takes the run's folder instead.
        if !filesSent.values.contains(where: { $0.files.contains(where: \.written) }) { Opened.leave() }
    }

    // MARK: - Draining

    /// Takes what jawd sent and applies it, input having gone first.
    func drain() {
        guard let jawd else { return }
        let updates = jawd.drain()
        guard !updates.isEmpty else { return }
        for update in updates {
            if trace { FileHandle.standardError.write(Data("jaw-mac: \(String(describing: update).prefix(160)) at \(Self.sinceStart) ms\n".utf8)) }
            apply(update)
        }
        window?.refresh()
        traceShown()
    }

    func apply(_ update: JawUpdate) {
        switch update {
        case let .hello(w, me):
            agent.answered()
            welcome = w
            reach = .up
            connection = w.connection
            linked = w.account.state == .linked
            self.me = Set(me)
            window?.chatList.root = w.root
            window?.sidebar.roster.root = w.root
            pictures.files = .folder(w.root)
            jawd.themes()
            jawd.status(shown: false)
        case let .unreachable(why, retryIn):
            reach = Reach(agent.unreachable(), why: why, retry: Int(retryIn / .seconds(1)))
        case let .connection(c):
            connection = c
            if c.state == .loggedOut { linked = false }
            if c.state == .online || c.state == .catchingUp { linked = true }
        case let .link(step):
            link = step
            if case .done = step {
                linked = true
                link = nil
            }
        case let .chats(w, u) where w == listWindow:
            listCame?()
            window?.chatList.apply(u)
            for row in u.upsert {
                panes[row.id]?.update(chat: row)
            }
            glance.chatsChanged()
            if !u.upsert.isEmpty {
                restorePanes()
                if let first = firstRows {
                    firstRows = nil
                    DispatchQueue.main.async { first() }
                }
            }
        case let .messages(w, u):
            if let chat = paneWindows[w] { panes[chat]?.apply(u) } else { glance.apply(w, u) }
            if pending != nil { cameFor(u.upsert) }
        case let .library(w, u):
            if let id = libraryWindows[w] { libraries[id]?.apply(u) }
            if pending != nil { cameFor(items: u.upsert) }
        case let .typing(now):
            typing = now
            ConversationController.give(now, to: panes.values)
            window?.chatList.typing = now
            window?.sidebar.roster.typing = rosterTyping()
        case let .sent(key, _, _):
            sending[key] = nil
            contactsSent.taken(key)
        case let .unsent(chat, key, text, why):
            sending[key] = nil
            if let c = panes[chat] { c.unsent(text: text, why: why) } else { say("not sent: \(why)", warn: true) }
        case let .pollUnsent(chat, _, poll, why):
            if let c = panes[chat], window?.pollSheet == nil {
                window?.showPollForm(PollForm(chat: c.chat.name, returned: poll, said: why), chat: chat)
            } else {
                say("the poll \"\(poll.question)\" wasn't sent: \(why)", warn: true)
            }
        case let .contactUnsent(_, key, cards, why):
            if window?.contactSheet == nil, let form = contactsSent.refused(key, why: why) {
                window?.showContactForm(form)
            } else {
                contactsSent.taken(key)
                say("\(cards.count == 1 ? "the card wasn't" : "the cards weren't") sent: \(why)", warn: true)
            }
        case .unsure:
            say("asking jawd again once it's back")
        case let .acted(key, said):
            if let vote = voting.removeValue(forKey: key), said != nil { panes[vote.chat]?.forgetVote(in: vote.message) }
            if let said { say(said) }
        case let .found(id, found):
            window?.palette.model.answered(id: id, found)
        case let .findFailed(id, cancelled, why):
            if !cancelled { window?.palette.model.failed(id: id, why: why) }
        case let .notice(text, tone):
            say(text, warn: tone != .info)
        case let .info(message, info, why):
            window?.showInfo(info: info, why: why, message: infoFor?.id == message ? infoFor : nil)
        case let .members(chat, members):
            panes[chat]?.members = members
        case let .chatStarted(row):
            if starts.answered() {
                openInNewPane(row)
            } else {
                open(row, opened: true)
            }
        case let .notStarted(why):
            starts.answered()
            say("no chat started: \(why)", warn: true)
        case let .status(status):
            window?.showStatus(status)
        case let .folders(w, u) where w == foldersWindow:
            let slots = (1 ... 9).map { tree.slotName($0) }
            window?.sidebar.apply(u)
            // A folder on a slot renamed changes what the slot says, though the push has no slots.
            if (1 ... 9).map({ tree.slotName($0) }) != slots { window?.favouritesChanged() }
            let showing = tree.showing(folder)
            if showing != folder {
                pick(folder: showing)
            } else {
                window?.chatList.emptyLine = tree.emptyLine(folder)
            }
            if rosterWindow == nil, tree.served { watchRoster() }
        case let .windowFailed(w, failure) where w == foldersWindow:
            // A jawd from before M6 doesn't know folders.watch: the sidebar keeps All and Archived.
            if failure.code != ErrorCode.unknownMethod { say("the folders couldn't be watched: \(failure)", warn: true) }
        case let .roster(w, u) where w == rosterWindow:
            window?.sidebar.roster.apply(u)
            window?.sidebar.roster.typing = rosterTyping()
        case let .windowFailed(w, failure) where w == rosterWindow:
            // A folder gone is said once, by its list's failure, or not at all when the tree's push
            // takes the list to All first.
            if failure.code != ErrorCode.notFound { say("the people couldn't be watched: \(failure)", warn: true) }
        case let .windowFailed(w, failure) where w == listWindow && folder != "all":
            say(tree.refused(folder, gone: failure.code == ErrorCode.notFound, why: "\(failure)"), warn: true)
            pick(folder: "all")
        case let .windowFailed(w, failure) where w == listWindow || paneWindows[w] != nil || glance.windows[w] != nil:
            say("a window couldn't be watched: \(failure)", warn: true)
        case let .windowFailed(w, why) where libraryWindows[w] != nil:
            if let id = libraryWindows[w] { libraries[id]?.refuse("\(why)") }
        case .windowFailed:
            // A window let go since, as the roster's once its folder's list failing took the list to All.
            break
        case let .layout(kept) where kept.interface == TerminalPanes.interface:
            terminalPanes(kept)
        case let .looked(chat, found):
            lookedUp(chat, found)
        case let .notFetched(message, why):
            if pending?.message == message { pending = nil }
            say("not downloaded: \(why)", warn: true)
        case let .saved(ticket, message, path, why):
            savedAnswer(ticket, message, path, why)
        case let .filed(filing):
            filed(filing)
        case let .named(row):
            let filing = window?.sidebar.naming?.filing
            window?.sidebar.named(row)
            if let row, let filing { folderAct(.add(item: filing.id, to: row.id), name: filing.name) }
        case let .notNamed(why):
            window?.sidebar.notNamed(why)
        case let .mediaSent(key, _, _):
            filesTaken(key)
        case let .mediaUnsent(chat, key, files, why):
            filesUnsent(chat: chat, key: key, files: files, why: why)
        case let .job(job):
            for c in panes.values {
                c.took(job)
            }
        case let .statusRead(status):
            read(status)
        case let .themes(served, unread):
            preferences.serve(served.map { OmarchyTheme(id: $0.id, name: $0.name, dark: $0.mode == .dark, colours: $0.colors) },
                              unread: Set(unread))
        case let .noThemes(older, why):
            preferences.omarchyState = older ? .older : .refused(why)
        case .push, .chats, .layout, .folders, .roster:
            break
        }
    }

    func say(_ text: String, warn: Bool = false) {
        notice = (text, Date().addingTimeInterval(4), warn)
        window?.refresh()
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.05) { [weak self] in self?.window?.refresh() }
    }

    /// Shows Settings, on `pane` when one's asked for, or brings it to the front.
    func showSettings(_ pane: SettingsPane? = nil) {
        if settings == nil { settings = SettingsWindowController(preferences: preferences, keeps: launch.snapshot == nil) }
        settings?.show(pane, over: window?.window)
    }

    func restyle() {
        settings?.window?.appearance = preferences.nsAppearance
        window?.restyle()
        for c in panes.values {
            c.spec = spec
            c.applyTheme()
            c.composer.hdByDefault = preferences.sendHD
        }
        for l in libraries.values {
            l.style = style
        }
        window?.showPanes()
    }

    // MARK: - What the screen says

    /// jawd's situation as this model has it, which says what the screen says.
    var situation: Situation {
        Situation(reach: reach, linked: linked, connection: connection, link: link,
                  said: notice.map { Situation.Said($0.text, until: $0.until, warn: $0.warn) })
    }

    /// The chats that need you, those no pane shows: mentions first, then people, then groups, each
    /// by its unread, as the spike's status line reads them from the chat list it already watches.
    var needs: [ChatRow] {
        Situation.needs(window?.chatList.all ?? [], shown: Set(panes.keys))
    }

    /// What fills the panes' place, when it isn't chats.
    var stateView: StateView.Kind? {
        situation.stateView(hasChats: !(window?.chatList.all.isEmpty ?? true) || folder != "all", hasPanes: grid != nil)
    }

    /// With `JAW_TRACE`, says on standard error what the window shows of jawd at launch and at each
    /// update it drains, once a frame with it is drawn, with the time since the process started: the
    /// status line's state, what fills the panes' place by its case, jawd's words when it didn't
    /// start, and whether the window is hidden, when any of that has changed.
    func traceShown() {
        guard trace else { return }
        let place = stateView.map { kind in String("\(kind)".prefix { $0 != "(" }) } ?? "chats"
        let why = if case let .starting(failed?) = reach { " · \(failed)" } else { "" }
        let hidden = window?.window?.isVisible == true ? "" : " · window hidden"
        let shows = "\(situation.stateLine?.text ?? "online") · \(place)\(why)\(hidden)"
        guard shows != traced else { return }
        traced = shows
        Bench.nextFrame {
            FileHandle.standardError.write(Data("jaw-mac: shown at \(Self.sinceStart) ms: \(shows)\n".utf8))
        }
    }

    /// Milliseconds since the process started, as the trace counts them.
    static var sinceStart: Int {
        Int(Date().timeIntervalSince(Launch.processStart() ?? Date()) * 1000)
    }

    // MARK: - Panes

    /// The layout on screen; a change of its chats is saved at once, a change of its shape kept as
    /// the layout's edit.
    var grid: PaneGrid? {
        get { book.grid }
        set {
            book.update(newValue)
            window?.layoutsChanged()
        }
    }

    /// The focused pane's chat, when it has one.
    var focusedChat: String? {
        focusedSlot.flatMap { grid?.slot($0)?.chat }
    }

    /// The grid's size on screen, for the room a split needs.
    var gridSize: CGSize {
        window?.detail.panes.bounds.size ?? CGSize(width: 1000, height: 800)
    }

    /// Opens a chat: in the pane that shows it, or in the focused pane in place of its chat; `opened`
    /// when you picked it yourself, for frecency.
    func open(_ chat: ChatRow, opened: Bool, around message: String? = nil) {
        if let existing = panes[chat.id], let slot = grid?.slot(showing: chat.id) {
            focus(slot: slot.id)
            if let message { existing.go(to: message) }
            existing.focusBox()
            return
        }
        let place = message.map { Place.around($0, before: ConversationController.rowsAround, after: ConversationController.rowsAround) }
            ?? ConversationController.place(for: chat)
        let c = makePane(chat, place: place)
        if let grid, let at = focusedSlot, grid.slot(at) != nil {
            if let old = grid.slot(at)?.chat { closePane(old) }
            self.grid = grid.with(chat: chat.id, in: at)
        } else {
            grid = .single(chat.id)
            focusedSlot = 1
        }
        if opened { jawd.opened(chat.id) }
        focus(slot: focusedSlot!, force: true)
        if let message { c.select(message) }
        savePanes()
    }

    func makePane(_ chat: ChatRow, place: Place) -> ConversationController {
        let c = ConversationController(chat: chat, spec: spec, layouts: layouts, place: place)
        ConversationController.give(typing, to: [c])
        c.pictures = pictures
        c.host = self
        c.notice = { [weak self] in self?.say($0) }
        c.avatar = window?.chatList.avatar(for: chat)
        c.composer.plusMenu = { [weak self] in
            self.map { m in
                Menus.menu(["attach.photos", "attach.document", "attach.camera", "attach.contact", "compose.poll", "compose.event", "attach.location"]
                    .compactMap(m.command))
            }
        }
        c.composer.gif = { [weak self] in self?.command("box.gif")?.run() }
        c.composer.hdByDefault = preferences.sendHD
        c.composer.ffmpegMissing = ffmpegMissing
        c.paneMenu = { [weak self, weak c] in
            guard let self, let c else { return nil }
            return paneMenu(for: c)
        }
        c.messageMenuTail = { [weak self] in
            guard let self else { return [] }
            return Menus.items(ChatMenu.splits) { id in command(id) }
        }
        c.folderMenu = { [weak self, weak c] in
            guard let self, let c else { return nil }
            return addToFolderMenu(for: c.chat)
        }
        _ = c.view
        panes[chat.id] = c
        paneWindows[jawd.watchMessages(chat.id, place)] = chat.id
        if chat.kind.isGroup { jawd.chatInfo(chat.id) }
        return c
    }

    /// Lets a chat's pane go: its window unwatched, its draft kept by jawd; or a library's.
    func closePane(_ chat: String) {
        if let l = libraries.removeValue(forKey: chat) {
            l.letGoOfPictures()
            l.view.removeFromSuperview()
            for (w, id) in libraryWindows where id == chat {
                jawd.unwatch(w)
                libraryWindows[w] = nil
            }
            libraryCameFrom[chat] = nil
            return
        }
        guard let c = panes.removeValue(forKey: chat) else { return }
        c.leave()
        c.view.removeFromSuperview()
        for (w, id) in paneWindows where id == chat {
            jawd.unwatch(w)
            paneWindows[w] = nil
        }
    }

    /// Gives a pane the keys: its chat selected in the list, its box focused, its chat read.
    func focus(slot id: Int, force: Bool = false) {
        guard let slot = grid?.slot(id) else { return }
        let changed = focusedSlot != id
        if changed { conversation?.leave() }
        focusedSlot = id
        for c in panes.values {
            c.lookingChanged()
        }
        if changed || force { window?.showPanes() }
        if let id = slot.chat, let l = libraries[id] {
            l.focus()
        } else if let chat = slot.chat {
            window?.chatList.select(chat)
            panes[chat]?.focusBox()
            panes[chat]?.readIfLooking()
        } else {
            window?.window?.makeFirstResponder(window?.detail.panes)
        }
        window?.refresh()
    }

    func focus(_ chat: String) {
        if let slot = grid?.slot(showing: chat) { focus(slot: slot.id) }
    }

    /// The next chat in the list no pane shows, after the focused one, for a new pane.
    func nextUnshown() -> ChatRow? {
        let list = (window?.chatList.all ?? []).filter { $0.kind != .community }
        guard !list.isEmpty else { return nil }
        let start = focusedChat.flatMap { id in list.firstIndex { $0.id == id } } ?? -1
        for i in 1 ... list.count {
            let c = list[(start + i) % list.count]
            if panes[c.id] == nil { return c }
        }
        return nil
    }

    /// Splits the focused pane along `axis`, the new pane taking `chat`, or the next chat no pane
    /// shows, and the keys.
    func split(_ axis: PaneGrid.Axis, with chat: ChatRow? = nil) {
        guard let grid, let at = focusedSlot else { return }
        guard let next = chat ?? nextUnshown() else { return say("every chat is on screen already") }
        switch grid.splitting(at, axis, chat: next.id, size: gridSize) {
        case let .failure(refusal):
            say(refusal.said)
        case let .success((g, new)):
            _ = makePane(next, place: ConversationController.place(for: next))
            self.grid = g
            focus(slot: new)
            savePanes()
        }
    }

    /// A chat dropped from the list onto a pane: on its middle it takes the pane, moving from the
    /// pane that showed it; on an edge the pane splits that way for it.
    func drop(_ id: String, on slot: Int, _ zone: GridView.DropZone) {
        guard let grid, let row = window?.chatList.row(id) ?? panes[id]?.chat else { return }
        switch zone {
        case .middle:
            guard grid.slot(slot)?.chat != id else { return focus(slot: slot) }
            if let old = grid.slot(slot)?.chat { closePane(old) }
            if panes[id] == nil { _ = makePane(row, place: ConversationController.place(for: row)) }
            self.grid = grid.with(chat: id, in: slot)
            focusedSlot = nil
            focus(slot: slot)
            savePanes()
        case .left, .right, .top, .bottom:
            guard panes[id] == nil else { return say("\(row.name) is on screen already; drop it on a pane's middle to move it there") }
            focus(slot: slot)
            split(zone == .left || zone == .right ? .across : .down, with: row)
            if zone == .left || zone == .top, let new = focusedSlot {
                self.grid = self.grid?.swapping(slot, new)
                focusedSlot = nil
                focus(slot: slot)
                savePanes()
            }
        }
    }

    /// Closes the focused pane, or Settings when the key Close Pane was chosen by was pressed in its
    /// window, which passes ⌘W on to the menu bar where its W key types something else, as on a
    /// Hebrew layout (`SettingsWindow.pressed(_:)`).
    func closeFocusedPane() {
        if let shown = settings?.window as? SettingsWindow, shown.pressed(NSApp.currentEvent) {
            shown.performClose(nil)
            return
        }
        guard let grid, let at = focusedSlot, grid.slots.count > 1 else {
            window?.window?.performClose(nil)
            return
        }
        close(slot: at)
    }

    /// Closes a pane of several: its space goes to the neighbour sharing its whole edge, or it stays,
    /// empty, in a pinwheel; the keys go to the neighbour when it had them.
    func close(slot id: Int) {
        guard let grid, grid.slots.count > 1, let slot = grid.slot(id) else { return }
        let (g, grew) = grid.closing(id)
        if let chat = slot.chat { closePane(chat) }
        self.grid = g
        if focusedSlot == id, let grew { focusedSlot = nil; focus(slot: grew) }
        window?.showPanes()
        window?.refresh()
        savePanes()
    }

    func move(_ direction: PaneGrid.Direction) {
        guard let at = focusedSlot, let to = grid?.neighbour(of: at, direction) else { return }
        focus(slot: to)
    }

    func goToPane(_ n: Int) {
        guard let reading = grid?.reading, n >= 1, n <= reading.count else { return }
        focus(slot: reading[n - 1].id)
    }

    /// Swaps the focused pane's chat with its neighbour's; the keys go with it.
    func swap(_ direction: PaneGrid.Direction) {
        guard let at = focusedSlot, let other = grid?.neighbour(of: at, direction) else { return }
        grid = grid?.swapping(at, other)
        focusedSlot = nil
        focus(slot: other)
        savePanes()
    }

    /// A gutter dragged: the line moves live, and is kept when the drag ends.
    func moved(_ line: PaneGrid.Line, to position: Double, done: Bool) {
        grid = grid?.moving(line, to: position, size: gridSize)
        if done { savePanes() }
    }

    /// The chat list made the folder's window, its row selected in the sidebar; `saying`, for a key or a
    /// favourite, names it in the status line while the sidebar isn't shown.
    func pick(folder id: String, saying: Bool = false) {
        folder = id
        window?.sidebar.select(id)
        if let w = listWindow { jawd.unwatch(w) }
        window?.chatList.emptyLine = tree.emptyLine(id)
        window?.chatList.show(folder: id)
        listWindow = jawd.watchChats(id, .top(ChatListController.rows))
        if tree.served { watchRoster() }
        if saying, let line = FolderTree.pickedLine(tree.label(id), sidebarShown: window?.sidebar.isShown ?? false,
                                                     listShown: window?.chatList.isShown ?? false, focusMode: focusMode)
        {
            say(line)
        }
    }

    /// ⌥↑ and ⌥↓: the chat before or after, passing over chats other panes show.
    func step(_ by: Int) {
        let others = Set(panes.keys).subtracting([focusedChat].compactMap(\.self))
        window?.chatList.step(by, skipping: others)
    }

    // MARK: - Layouts

    /// Goes to a layout by name, or to the unnamed one, with its chats and its focus.
    func switchLayout(_ name: String?) {
        guard name != book.current else { return }
        book.focus = focusedSlot
        conversation?.leave()
        book.switchTo(name)
        reconcile(focus: book.focus)
    }

    func saveLayout() {
        book.save()
        savePanes()
        window?.layoutsChanged()
    }

    func revertLayout() {
        book.revert()
        reconcile(focus: focusedSlot)
    }

    func saveLayoutAs(_ name: String) {
        do {
            book.focus = focusedSlot
            try book.saveAs(name)
            savePanes()
            window?.layoutsChanged()
            say("saved as \(book.current ?? name)")
        } catch {
            say(error.said, warn: true)
        }
    }

    /// What the editor gave back: the layout on screen reshaped and saved, or saved as a new one.
    func applyEditor(_ mode: LayoutEditorModel.Mode, name: String, grid new: PaneGrid) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            book.focus = focusedSlot
            book.update(new)
            switch mode {
            case .saveAs:
                try book.saveAs(name)
            case .edit:
                if let old = book.current {
                    book.save()
                    if !name.isEmpty { try book.rename(old, to: name) }
                } else if !name.isEmpty {
                    try book.saveAs(name)
                }
            }
        } catch {
            say(error.said, warn: true)
        }
        reconcile(focus: focusedSlot)
    }

    /// A chat's name for the editor's panes, or a library's.
    func chatName(_ id: String) -> String {
        if let name = LibraryPane.name(id, chat: { chatName($0) }) { return name }
        return window?.chatList.row(id)?.name ?? panes[id]?.chat.name ?? id
    }

    func renameLayout(_ old: String, to new: String) {
        do {
            try book.rename(old, to: new)
            savePanes()
            window?.layoutsChanged()
        } catch {
            say(error.said, warn: true)
        }
    }

    func deleteLayout(_ name: String) {
        book.delete(name)
        savePanes()
        window?.layoutsChanged()
    }

    /// Asks jawd for the terminal's panes, which open as the unsaved panes once it answers.
    func openTerminalPanes() {
        jawd.layout(of: TerminalPanes.interface, what: "opening the terminal's panes")
    }

    /// The terminal's panes as jawd keeps them for it, read as a grid in place of what the unsaved
    /// panes held, its pane with the keys taking them; the layout on screen is left as switching
    /// leaves it, and what can't open is said.
    func terminalPanes(_ kept: KeptLayout) {
        switch TerminalPanes.read(kept) {
        case let .failure(refusal):
            say(refusal.said, warn: refusal != .nothing)
        case let .success(terminal):
            book.focus = focusedSlot
            conversation?.leave()
            book.switchTo(nil)
            book.update(terminal.grid)
            reconcile(focus: terminal.focus)
            say(terminal.said)
        }
    }

    /// A chat the chat list didn't hold, as jawd found it: its pane made where the layout on screen
    /// shows it, taking the keys if its slot has them, or its slot saying why it won't have one.
    func lookedUp(_ chat: String, _ found: Looked) {
        let wanted = opening?.chat == chat ? opening : nil
        if wanted != nil { opening = nil }
        guard let slot = grid?.slot(showing: chat), panes[chat] == nil else {
            if let wanted { opened(wanted, found) }
            return
        }
        switch found {
        case let .row(row):
            _ = makePane(row, place: ConversationController.place(for: row))
            if focusedSlot == slot.id || wanted != nil {
                focus(slot: slot.id, force: true)
                return
            }
        case .gone:
            unopened[chat] = GridView.gone
        case let .refused(why):
            unopened[chat] = why
            say("a pane's chat couldn't be looked up: \(why)", warn: true)
        }
        window?.showPanes()
    }

    /// The panes for the layout on screen: windows let go for chats it doesn't show, opened for
    /// the ones it does; a chat the list doesn't hold is looked up, the focused slot's first, its
    /// pane made once jawd finds it (`lookedUp`), and a slot whose chat won't have one says why.
    func reconcile(focus at: Int?) {
        let shown = Set(grid?.chats ?? [])
        for chat in Array(panes.keys) + Array(libraries.keys) where !shown.contains(chat) {
            closePane(chat)
        }
        let reading = grid?.reading ?? []
        var outside: [String] = []
        for slot in reading.filter({ $0.id == at }) + reading.filter({ $0.id != at }) {
            guard let id = slot.chat, panes[id] == nil, libraries[id] == nil, !outside.contains(id) else { continue }
            if let scope = LibraryPane.scope(of: id, name: chatName) {
                _ = makeLibrary(id, scope)
            } else if let row = window?.chatList.row(id) {
                _ = makePane(row, place: ConversationController.place(for: row))
            } else {
                outside.append(id)
            }
        }
        unopened = [:]
        jawd.lookUp(outside)
        focusedSlot = nil
        if let grid {
            focus(slot: at.flatMap { grid.slot($0)?.id } ?? grid.reading.first!.id, force: true)
        } else {
            window?.refresh()
        }
        savePanes()
        window?.layoutsChanged()
    }

    // MARK: - Keeping the panes

    struct Kept: Codable {
        let grid: PaneGrid
        let focus: Int?
    }

    /// The tree mac-m1 kept before the grid, read once.
    struct KeptTree: Codable {
        let layout: PaneLayout
        let focused: String?
    }

    var keptKey: String {
        "layouts:" + launch.home
    }

    /// The layouts kept in the app's preferences, per jaw folder, until jawd keeps an interface's
    /// state (P1).
    func savePanes() {
        guard restored, !keepKept else { return }
        book.focus = focusedSlot
        if let data = try? JSONEncoder().encode(book) { UserDefaults.standard.set(data, forKey: keptKey) }
    }

    /// Opens the layouts kept from last time, once the chat list has the chats they show; what an
    /// earlier build kept (the grid alone, or the tree) is read as the unnamed layout.
    func restorePanes() {
        guard !restored, launch.bench == nil, launch.snapshot == nil else { return }
        restored = true
        let defaults = UserDefaults.standard
        if let data = defaults.data(forKey: keptKey) {
            do {
                book = try JSONDecoder().decode(LayoutBook.self, from: data)
            } catch {
                say("the saved layouts couldn't be read: \((error as? LayoutBook.Problem)?.said ?? "\(error)"); they're kept as they were",
                    warn: true)
                keepKept = true
                return
            }
        } else if let data = defaults.data(forKey: "grid:" + launch.home), let kept = try? JSONDecoder().decode(Kept.self, from: data) {
            book.update(kept.grid)
            book.focus = kept.focus
        } else if let old = defaults.data(forKey: "panes:" + launch.home), let tree = try? JSONDecoder().decode(KeptTree.self, from: old) {
            let g = PaneGrid.from(tree.layout)
            book.update(g)
            book.focus = tree.focused.flatMap { g.slot(showing: $0)?.id }
        }
        guard let grid, grid.holds else {
            window?.layoutsChanged()
            return
        }
        reconcile(focus: book.focus)
    }

    // MARK: - Focus

    /// Focus is true while the app is active and its window visible and not minimised. Each pane's
    /// Mimic is told too, since the window can stop being key without focus changing, as for a sheet.
    func updateFocus() {
        let w = window?.window
        let now = NSApp.isActive && (w?.isVisible ?? false) && !(w?.isMiniaturized ?? true)
            && (w?.occlusionState.contains(.visible) ?? false)
        if now != focused {
            focused = now
            jawd?.focus(now)
            if now { conversation?.readIfLooking() }
        }
        for c in panes.values {
            c.lookingChanged()
        }
    }

    // MARK: - The palette's actions

    /// Every action the palette offers now: the commands that can run, by the palette's phrases.
    func actions() -> [PaletteAction] {
        (allCommands + folderPaletteCommands()).filter { $0.inPalette && $0.enabled() }.map(\.paletteAction)
    }

    /// The next chat with unread messages after the focused one.
    func goToUnread() {
        let unread = unreadChats()
        guard !unread.isEmpty else { return say("nothing unread") }
        let at = focusedChat.flatMap { id in unread.firstIndex { $0.id == id } } ?? -1
        open(unread[(at + 1) % unread.count], opened: true)
    }

    func toggleFocusMode() {
        focusMode.toggle()
        window?.focusMode(focusMode)
    }

    /// The poll form for the focused pane's chat: the + menu's Poll…, and New poll in the palette.
    func composePoll() {
        guard let c = conversation else { return say("open a chat to make a poll in it") }
        window?.showPollForm(PollForm(chat: c.chat.name), chat: c.chat.id)
    }

    /// Sends a poll as its form has it, with the key its form made; jawd's row brings it into the chat.
    func sendPoll(chat: String, _ poll: (question: String, options: [String], max: Int), key: String) {
        jawd.sendPoll(chat: chat, question: poll.question, options: poll.options, max: poll.max, key: key)
    }

    /// With no window open the decoded pictures go, as the design's "under 25 MB with no window
    /// open" asks; the panes' cells ask for theirs again once it's back.
    func windowClosed() {
        for l in libraries.values {
            l.letGoOfPictures()
        }
        ConversationController.windowClosed(panes.values, pictures: pictures)
    }

    func windowShown() {
        let closed = pictures.cache.isClosed
        ConversationController.windowShown(panes.values, pictures: pictures)
        if closed {
            for l in libraries.values {
                l.askPicturesAgain()
            }
        }
    }

    // MARK: - The library

    /// Opens the library of `scope` in a pane: the one that shows it, or beside the focused pane,
    /// as `LibraryPane.open` says, taking the keys.
    func openLibrary(_ scope: LibraryScope) {
        let id = LibraryPane.id(scope)
        switch LibraryPane.open(id, in: grid, focused: focusedSlot, size: gridSize) {
        case let .shown(slot):
            focus(slot: slot)
        case let .into(g, slot, replacing):
            let from = focusedChat.flatMap { LibraryPane.isLibrary($0) ? nil : $0 }
            if let replacing { closePane(replacing) }
            if libraries[id] == nil { _ = makeLibrary(id, scope) }
            libraryCameFrom[id] = from
            grid = g
            focusedSlot = nil
            focus(slot: slot, force: true)
            savePanes()
        }
    }

    /// A library pane, its window watched as it asks.
    func makeLibrary(_ id: String, _ scope: LibraryScope) -> LibraryController {
        let l = LibraryController(scope: scope, style: style)
        l.pictures = pictures
        l.host = self
        l.chats = { [weak self] in (self?.window?.chatList.all ?? []).filter { $0.kind != .community }.map { (id: $0.id, name: $0.name) } }
        l.people = { [weak self] in
            guard let self else { return [] }
            // Who the app knows by their person: the members of the chats open in panes.
            var seen: Set<String> = []
            return panes.values.flatMap(\.members).filter { !me.contains($0.person) && seen.insert($0.person).inserted }
                .map { (id: $0.person, name: $0.name) }
        }
        _ = l.view
        libraries[id] = l
        l.ask()
        return l
    }

    /// The library's slot closes, the keys going to the pane beside it, or the chat it came from
    /// taking its place when it was alone.
    func closeLibrary(_ id: String) {
        guard let grid else { return }
        let (g, next) = LibraryPane.close(id, in: grid, cameFrom: libraryCameFrom[id])
        closePane(id)
        self.grid = g
        if let chat = next.flatMap({ g.slot($0)?.chat }), panes[chat] == nil, let row = window?.chatList.row(chat) {
            _ = makePane(row, place: ConversationController.place(for: row))
        }
        focusedSlot = nil
        if let next { focus(slot: next, force: true) }
        window?.showPanes()
        window?.refresh()
        savePanes()
    }

    func paneID(of l: LibraryController) -> String? {
        libraries.first { $0.value === l }?.key
    }
}

extension AppModel: LibraryHost {
    func library(_ l: LibraryController, watch filter: LibraryFilter, _ place: Place) {
        guard let id = paneID(of: l) else { return }
        for (w, pane) in libraryWindows where pane == id {
            jawd.unwatch(w)
            libraryWindows[w] = nil
        }
        libraryWindows[jawd.watchLibrary(filter, place)] = id
    }

    func library(_ l: LibraryController, move place: Place) {
        guard let id = paneID(of: l), let w = libraryWindows.first(where: { $0.value == id })?.key else { return }
        jawd.move(w, place)
    }

    func library(_: LibraryController, shown items: [String]) {
        jawd.shown(items)
    }

    func libraryClose(_ l: LibraryController) {
        if let id = paneID(of: l) { closeLibrary(id) }
    }

    func libraryFocused(_ l: LibraryController) {
        if let id = paneID(of: l), let slot = grid?.slot(showing: id), slot.id != focusedSlot { focus(slot: slot.id) }
    }

    /// Enter on an item, as 2.3 opens a message's: a photo or a video looked at with Quick Look, the
    /// photos and videos here around it paged through in the library's order; a file looked at; a
    /// link in the browser; one not here asked for and looked at once it is.
    func library(_: LibraryController, open item: LibraryRow, among: [LibraryRow]) {
        switch item.kind {
        case .photo, .video:
            let looks = among.filter { $0.kind == .photo || $0.kind == .video }
            guard let at = looks.firstIndex(where: { $0.id == item.id }) else { return }
            // Quick Look pages through those near it, so a long library isn't cloned whole.
            let near = Array(looks[max(at - 50, 0) ..< min(at + 51, looks.count)])
            let offers = near.compactMap(MediaOffer.of)
            guard let i = offers.firstIndex(where: { $0.message == item.message }) else { return say("there's nothing to look at in this one") }
            look(offers, at: i, item: item)
        case .file:
            guard let offer = MediaOffer.of(item) else { return }
            look([offer], at: 0, item: item)
        case .link:
            if let url = item.link?.url { openLink(url) }
        case .voice:
            say("playing voice messages comes later")
        case .other:
            break
        }
    }

    /// Looks at `offers` from the one at `at`, or asks for it and looks once it's here.
    func look(_ offers: [MediaOffer], at: Int, item: LibraryRow) {
        let offer = offers[at]
        guard offer.isHere else {
            if offer.media.state != .downloading, offer.media.state != .waitingForPhone { jawd.fetch(chat: offer.chat, message: offer.message) }
            pending = PendingOpen(.look, offer, of: item)
            return say(offer.waitingWords)
        }
        pending = nil
        hand(.look, offers, at: at)
    }

    /// ⌘↩ on an item: its message, in the pane that shows its chat, the library staying beside it;
    /// or its chat in a new pane beside the library, or in the library's place with no room, as the
    /// terminal's Ctrl-G closes its library.
    func library(_ l: LibraryController, goTo item: LibraryRow) {
        if let slot = grid?.slot(showing: item.chat), let c = panes[item.chat] {
            focus(slot: slot.id)
            c.go(to: item.message)
            return
        }
        guard let row = window?.chatList.row(item.chat) else { return say("that chat isn't in the list yet") }
        if let grid, let at = focusedSlot, case let .success((g, new)) = grid.splitting(at, .across, chat: row.id, size: gridSize) {
            _ = makePane(row, place: .around(item.message, before: ConversationController.rowsAround, after: ConversationController.rowsAround))
            self.grid = g
            focus(slot: new)
            jawd.opened(row.id)
            panes[row.id]?.select(item.message)
            savePanes()
        } else {
            if let id = paneID(of: l) { libraryCameFrom[id] = nil }
            open(row, opened: true, around: item.message)
        }
    }

    /// An item's menu's Save to Downloads and Show in Finder, as a message's are.
    func library(_: LibraryController, media act: MediaOffer.Act, item: LibraryRow) {
        guard let offer = MediaOffer.of(item) else { return }
        switch act {
        case .look, .open, .openWith:
            look([offer], at: 0, item: item)
        default:
            fileAct(act, offer)
        }
    }
}

extension AppModel: ConversationHost {
    func conversation(_ c: ConversationController, move place: Place) {
        guard let w = paneWindows.first(where: { $0.value == c.chat.id })?.key else { return }
        jawd.move(w, place)
    }

    func conversation(_ c: ConversationController, send text: String, key: String, reply: String?, mentions: [String]) {
        sending[key] = (c.chat.id, text)
        jawd.sendText(chat: c.chat.id, text: text, key: key, reply: reply, mentions: mentions)
    }

    func conversation(_ c: ConversationController, react message: String, emoji: String) {
        jawd.react(chat: c.chat.id, message: message, emoji: emoji)
    }

    func conversation(_ c: ConversationController, edit message: String, text: String) {
        jawd.edit(chat: c.chat.id, message: message, text: text)
    }

    func conversation(_ c: ConversationController, delete message: String, everyone: Bool) {
        jawd.delete(chat: c.chat.id, message: message, everyone: everyone)
    }

    func conversation(_ c: ConversationController, info message: String) {
        infoFor = c.window.get(message)
        window?.showInfo(info: nil, why: nil, message: infoFor)
        jawd.messageInfo(chat: c.chat.id, message: message)
    }

    func conversation(_: ConversationController, retry message: String) {
        jawd.retry(message: message)
    }

    func conversation(_ c: ConversationController, read upTo: String) {
        jawd.read(chat: c.chat.id, upTo: upTo)
    }

    func conversation(_ c: ConversationController, typing state: TypingState) {
        jawd.typing(chat: c.chat.id, state: state)
    }

    func conversation(_ c: ConversationController, draft text: String) {
        jawd.draft(chat: c.chat.id, text: text)
    }

    func conversationWantsHistory(_ c: ConversationController) {
        jawd.historyMore(c.chat.id)
    }

    func conversation(_ c: ConversationController, vote message: String, options: [String]) {
        voting[jawd.vote(chat: c.chat.id, message: message, options: options)] = (c.chat.id, message)
    }

    func conversation(_: ConversationController, open url: String) {
        openLink(url)
    }

    /// Opens a link in the browser, with `https://` before one written without how it's reached.
    func openLink(_ url: String) {
        guard let u = URL(string: url.contains("://") ? url : "https://" + url) else { return }
        NSWorkspace.shared.open(u)
    }

    /// Only the focused pane reads its chat, as only the terminal's pane with the keys does.
    func conversationIsRead(_ c: ConversationController) -> Bool {
        c.chat.id == focusedChat && focused && (window?.window?.isKeyWindow ?? false)
    }

    func conversation(_ c: ConversationController, forward message: MessageRow) {
        window?.showPalette(mode: .forward(messages: [message.id], from: c.chat.id, summary: ComposerController.summary(message)))
    }

    func conversation(_: ConversationController, reactTo message: MessageRow) {
        window?.showPalette(mode: .react(message: message.id, summary: ComposerController.summary(message)))
    }

    func conversationFocused(_ c: ConversationController) {
        if focusedChat != c.chat.id { focus(c.chat.id) }
    }

    func conversationLibrary(_ c: ConversationController) {
        focus(c.chat.id)
        openLibrary(.chat(id: c.chat.id, name: c.chat.name))
    }

    func conversation(_: ConversationController, media act: MediaOffer.Act, _ offers: [MediaOffer], at: Int, of row: MessageRow) {
        file(act, offers, at: at, of: row)
    }

    func conversation(_: ConversationController, dragging offer: MediaOffer) -> NSPasteboardWriting? {
        if let here = offer.here, let file = pictures.url(here), let clone = Opened.clone(of: file, named: offer.name, for: offer.message) {
            return clone as NSURL
        }
        return promise(for: offer)
    }

    /// The promise a file not here drags out as: jawd saves it into a folder of the app's own, and
    /// the promise puts it where it's dropped, which is said.
    func promise(for offer: MediaOffer) -> Dragged {
        Dragged(offer, save: { [weak self] folder, done in
            guard let self else { return done(.failure(NotSaved("Jaw has gone"))) }
            let ticket = jawd.save(chat: offer.chat, message: offer.message, to: folder.path)
            saving[ticket] = (offer.message, false, done)
        }, placed: { [weak self] url in self?.say(saved: url) })
    }

    func conversation(_: ConversationController, startChat phone: String, name: String) {
        startChat(phone: phone, name: name)
    }

    func conversation(_: ConversationController, cardsOf message: MessageRow) {
        window?.showCards(message)
    }

    /// A chat with whoever a contact card shares, by the number to dial: it opens once jawd has it,
    /// and jawd's words say why when it hasn't.
    func startChat(phone: String, name: String) {
        say("opening a chat with \(name)")
        startChat(phone: phone)
    }

    /// A chat started with one of jawd's people, as `chat.start` does: it opens once jawd has it, in
    /// a pane of its own when `inNewPane`. Every start goes into `starts` as it's sent, so each of
    /// jawd's answers opens its own start's chat where that start said.
    func startChat(person: String, inNewPane: Bool = false) {
        starts.sent(inNewPane: inNewPane)
        jawd.startChat(person: person)
    }

    /// A chat started with a number, in place, as `startChat(person:)` starts one with someone.
    func startChat(phone: String) {
        starts.sent(inNewPane: false)
        jawd.startChat(phone: phone)
    }

    /// The contact form for the focused pane's chat, its picker shown at once.
    func composeContact() {
        guard let c = conversation else { return }
        window?.showContactForm(ContactForm(chat: c.chat.id, to: c.chat.name))
    }

    /// Sends a form's cards to its chat in one message, with a key made for them, the form kept
    /// until jawd answers; the card shows in the chat once jawd's row has it.
    func send(_ form: ContactForm) {
        guard form.ready else { return }
        let key = Jawd.key()
        contactsSent.going(form, key: key)
        jawd.sendContact(chat: form.chat, cards: form.sending, key: key)
    }
}

extension AppModel: ChatListHost {
    func chatList(_: ChatListController, open chat: ChatRow) {
        open(chat, opened: true)
    }

    func chatList(_: ChatListController, move place: Place) {
        if let w = listWindow { jawd.move(w, place) }
    }

    func chatList(_: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem] {
        chatMenuItems(for: chat)
    }
}

/// A message's files: looked at with Quick Look, opened in their apps, saved into Downloads and asked
/// for, each from a clone of jawd's file, never jawd's own (`Opened`).
extension AppModel {
    func file(_ act: MediaOffer.Act, _ offers: [MediaOffer], at: Int, of row: MessageRow) {
        let offer = offers[at]
        switch act {
        case .look, .open, .openWith:
            guard offer.isHere else {
                // A file of yours from the outbox is jawd's own copy, being made: it's waited for, never fetched.
                if offer.outbox, row.status == .failed { return say("\(offer.name) wasn't sent, and jawd kept no copy of it to open") }
                if !offer.outbox, offer.media.state != .downloading, offer.media.state != .waitingForPhone {
                    jawd.fetch(chat: offer.chat, message: offer.message)
                }
                pending = PendingOpen(act, offer, of: row)
                return say(offer.waitingWords)
            }
            pending = nil
            hand(act, offers, at: at)
        case .save, .saveHD, .reveal, .fetch, .stop:
            fileAct(act, offer)
        }
    }

    /// What's done with a file that needn't be here first: saved to Downloads, shown in the Finder,
    /// asked for, or stopped.
    func fileAct(_ act: MediaOffer.Act, _ offer: MediaOffer) {
        switch act {
        case .save, .saveHD:
            let ticket = jawd.save(chat: offer.chat, message: offer.message, to: launch.downloads, variant: offer.variant(act))
            saving[ticket] = (offer.message, false, nil)
            if !offer.isHere { say("\(offer.name) is coming; it's saved once it's here") }
        case .reveal:
            if let url = saved[offer.message], FileManager.default.fileExists(atPath: url.path) {
                NSWorkspace.shared.activateFileViewerSelecting([url])
            } else {
                let ticket = jawd.save(chat: offer.chat, message: offer.message, to: launch.downloads)
                saving[ticket] = (offer.message, true, nil)
            }
        case .fetch:
            jawd.fetch(chat: offer.chat, message: offer.message)
        case .stop:
            if pending?.message == offer.message { pending = nil }
            jawd.cancel(chat: offer.chat, message: offer.message, what: offer.sending ? "stopping it" : "stopping the download")
        case .look, .open, .openWith:
            break
        }
    }

    /// The files here among `offers`, cloned off the main thread, then looked at, from the one asked
    /// for, or the one asked for opened.
    func hand(_ act: MediaOffer.Act, _ offers: [MediaOffer], at: Int) {
        let chosen: [(Int, MediaOffer)] = act == .look ? offers.indices.filter { offers[$0].isHere }.map { ($0, offers[$0]) } : [(at, offers[at])]
        let files = chosen.compactMap { i, o in o.here.flatMap { pictures.url($0) }.map { (i, $0, o.name, o.message) } }
        guard !files.isEmpty else { return say("it isn't here yet") }
        DispatchQueue.global(qos: .userInitiated).async {
            var clones: [(Int, URL, String)] = []
            var why: String?
            for (i, file, name, message) in files {
                do {
                    clones.append((i, try Opened.copy(of: file, named: name, for: message), name))
                } catch {
                    why = error.localizedDescription
                }
            }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let model = AppDelegate.shared?.model else { return }
                    guard !clones.isEmpty else { return model.say("couldn't open it: \(why ?? "no file")", warn: true) }
                    switch act {
                    case .look:
                        QuickLook.shared.show(clones.map { ($0.1, $0.2) }, at: clones.firstIndex { $0.0 == at } ?? 0)
                    case let .openWith(app):
                        NSWorkspace.shared.open([clones[0].1], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
                    default:
                        NSWorkspace.shared.open(clones[0].1)
                    }
                }
            }
        }
    }

    /// Library items that came while one's file waited to be looked at: once it's here it is, and
    /// once it can't come it isn't.
    func cameFor(items: [LibraryRow]) {
        guard let p = pending else { return }
        for item in items {
            switch p.next(item) {
            case .wait:
                continue
            case let .open(offers, at):
                pending = nil
                hand(p.act, offers, at: at)
            case .stop:
                pending = nil
            }
            return
        }
    }

    /// Rows that came while a file waited to be looked at or opened: once it's here it is, and once
    /// it can't come it isn't, its card saying why (`PendingOpen`).
    func cameFor(_ rows: [MessageRow]) {
        guard let p = pending else { return }
        for row in rows {
            switch p.next(row) {
            case .wait:
                continue
            case let .open(offers, at):
                pending = nil
                hand(p.act, offers, at: at)
            case .stop:
                pending = nil
            }
            return
        }
    }

    /// media.save's answer: handed to the drag's promise, which quarantines the file it puts where
    /// it was dropped; or the copy quarantined the same way, as jawd doesn't yet (the Mac design's
    /// P13), then said in the status line and shown in the Finder when that was asked.
    func savedAnswer(_ ticket: String, _ message: String, _ path: String?, _ why: String?) {
        let asked = saving.removeValue(forKey: ticket)
        if let done = asked?.done {
            return done(path.map { .success(URL(fileURLWithPath: $0)) } ?? .failure(NotSaved(why ?? "jawd didn't save it")))
        }
        guard let path else { return say("not saved: \(why ?? "jawd didn't say why")", warn: true) }
        let url = URL(fileURLWithPath: path)
        try? Opened.handOver(url, readOnly: false)
        saved[message] = url
        say(saved: url)
        if asked?.reveal == true { NSWorkspace.shared.activateFileViewerSelecting([url]) }
    }

    /// Where a copy went, as the terminal says it: "saved to Downloads as …".
    func say(saved url: URL) {
        let parent = url.deletingLastPathComponent().path
        let folder = parent == FileManager.default.homeDirectoryForCurrentUser.appending(path: "Downloads").path
            ? "Downloads" : (parent as NSString).abbreviatingWithTildeInPath
        say("saved to \(folder) as \(url.lastPathComponent)")
    }
}
