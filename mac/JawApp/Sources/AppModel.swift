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
    var jawd: Jawd!
    weak var window: MainWindowController?

    typealias Reach = Situation.Reach

    var reach: Reach = .waiting
    var welcome: Welcome?
    var connection = Connection(state: .connecting)
    var linked = true
    var link: LinkStep?
    var me: Set<String> = []
    var listWindow: WindowID?
    var folder = "all"
    /// Each pane's conversation, by the chat it shows; a chat shows in one pane at most.
    var panes: [String: ConversationController] = [:]
    var paneWindows: [WindowID: String] = [:]
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
    var focusMode = false
    /// Everything the app does, made once; the menus, the palette and the keys sheet read it.
    lazy var commands: [Command] = makeCommands()
    /// Messages sent and not yet answered for, by key, so a refused one goes back to its box.
    var sending: [String: (chat: String, text: String)] = [:]
    var notice: (text: String, until: Date, warn: Bool)?
    var infoFor: MessageRow?
    var focused = false
    /// What the launch bench waits for: the first frame with the chat list's rows.
    var firstRows: (() -> Void)?
    /// `JAW_TRACE=1` says each update drained, on standard error.
    let trace = ProcessInfo.processInfo.environment["JAW_TRACE"] != nil

    /// The focused pane's conversation, the one the keys and the menus act on.
    var conversation: ConversationController? {
        focusedChat.flatMap { panes[$0] }
    }

    init(launch: Launch) {
        self.launch = launch
        agent = Agent(launch: launch)
        preferences = Preferences()
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
        if let why = agent.ensure() {
            reach = .starting(failed: why)
        } else if agent.started != nil {
            reach = .starting(failed: nil)
        }
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
        listWindow = jawd.watchChats(folder, .top(ChatListController.rows))
        preferences.changed.append { [weak self] in self?.restyle() }
        glance.model = self
    }

    func stop() {
        for c in panes.values {
            c.leave()
        }
        savePanes()
        jawd?.close()
        agent.stop()
    }

    // MARK: - Draining

    /// Takes what jawd sent and applies it, input having gone first.
    func drain() {
        guard let jawd else { return }
        let updates = jawd.drain()
        guard !updates.isEmpty else { return }
        for update in updates {
            if trace { FileHandle.standardError.write(Data("jaw-mac: \(String(describing: update).prefix(160))\n".utf8)) }
            apply(update)
        }
        window?.refresh()
    }

    func apply(_ update: JawUpdate) {
        switch update {
        case let .hello(w, me):
            welcome = w
            reach = .up
            connection = w.connection
            linked = w.account.state == .linked
            self.me = Set(me)
            window?.chatList.root = w.root
        case let .unreachable(why, retryIn):
            if let stopped = agent.stopped {
                reach = .starting(failed: stopped)
            } else if agent.starting {
                reach = .starting(failed: nil)
            } else {
                reach = .unreachable(why: why, retry: Int(retryIn / .seconds(1)))
            }
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
        case let .sent(key, _, _):
            sending[key] = nil
        case let .unsent(chat, key, text, why):
            sending[key] = nil
            if let c = panes[chat] { c.unsent(text: text, why: why) } else { say("not sent: \(why)", warn: true) }
        case .unsure:
            say("asking jawd again once it's back")
        case let .acted(_, said):
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
            open(row, opened: true)
        case let .notStarted(why):
            say("no chat started: \(why)", warn: true)
        case let .status(status):
            window?.showStatus(status)
        case let .windowFailed(_, why):
            say("a window couldn't be watched: \(why)", warn: true)
        case let .layout(kept) where kept.interface == TerminalPanes.interface:
            terminalPanes(kept)
        case let .looked(chat, found):
            lookedUp(chat, found)
        case .push, .chats, .layout:
            break
        }
    }

    func say(_ text: String, warn: Bool = false) {
        notice = (text, Date().addingTimeInterval(4), warn)
        window?.refresh()
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.05) { [weak self] in self?.window?.refresh() }
    }

    func restyle() {
        window?.restyle()
        for c in panes.values {
            c.spec = spec
            c.applyTheme()
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
        situation.stateView(hasChats: !(window?.chatList.all.isEmpty ?? true), hasPanes: grid != nil)
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
        c.paneMenu = { [weak self, weak c] in
            guard let self, let c else { return nil }
            return paneMenu(for: c)
        }
        c.messageMenuTail = { [weak self] in
            guard let self else { return [] }
            return Menus.items(ChatMenu.splits) { id in command(id) }
        }
        _ = c.view
        panes[chat.id] = c
        paneWindows[jawd.watchMessages(chat.id, place)] = chat.id
        if chat.kind.isGroup { jawd.chatInfo(chat.id) }
        return c
    }

    /// Lets a chat's pane go: its window unwatched, its draft kept by jawd.
    func closePane(_ chat: String) {
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
        if let chat = slot.chat {
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

    func closeFocusedPane() {
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

    func pick(folder id: String) {
        folder = id
        if let w = listWindow { jawd.unwatch(w) }
        window?.chatList.show(folder: id)
        listWindow = jawd.watchChats(id, .top(ChatListController.rows))
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

    /// A chat's name for the editor's panes.
    func chatName(_ id: String) -> String {
        window?.chatList.row(id)?.name ?? panes[id]?.chat.name ?? id
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
        guard let slot = grid?.slot(showing: chat), panes[chat] == nil else { return }
        switch found {
        case let .row(row):
            _ = makePane(row, place: ConversationController.place(for: row))
            if focusedSlot == slot.id {
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
        for chat in panes.keys where !shown.contains(chat) {
            closePane(chat)
        }
        let reading = grid?.reading ?? []
        var outside: [String] = []
        for slot in reading.filter({ $0.id == at }) + reading.filter({ $0.id != at }) {
            guard let id = slot.chat, panes[id] == nil, !outside.contains(id) else { continue }
            if let row = window?.chatList.row(id) {
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
        allCommands.filter { $0.inPalette && $0.enabled() }.map(\.paletteAction)
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

    func conversation(_: ConversationController, open url: String) {
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
