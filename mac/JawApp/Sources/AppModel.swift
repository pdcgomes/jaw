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

    enum Reach: Equatable {
        case waiting
        case starting(failed: String?)
        case up
        case unreachable(why: String, retry: Int)
    }

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
    var layout: PaneLayout?
    var focusedChat: String?
    var restored = false
    let glance = Glance()
    var focusMode = false
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
        case .push, .chats:
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

    /// The status line's left half: jawd's state when it isn't online, as the terminal's header says it.
    var stateLine: (String, Bool)? {
        switch reach {
        case .waiting: return ("connecting to jawd…", false)
        case .starting(nil): return ("starting jawd…", false)
        case .starting: return ("jawd didn't start", true)
        case let .unreachable(_, retry): return ("can't reach jawd · trying again in \(retry) s", true)
        case .up: break
        }
        if !linked { return ("not linked", true) }
        switch connection.state {
        case .online: return nil
        case .connecting: return ("connecting", false)
        case .catchingUp:
            guard let done = connection.done, let total = connection.total else { return ("catching up", false) }
            return ("catching up \(Format.num(done)) of \(Format.num(total))", false)
        case .phoneOffline: return ("phone offline: photos and older history wait for it", true)
        case .offline: return ("offline: what you send waits until it's back", true)
        case .updateNeeded: return ("update needed: WhatsApp wants a newer Jaw · reading still works", true)
        case .banned: return ("temporarily banned until \(connection.until.map { Format.until($0) } ?? "later")", true)
        case .engineRestarting: return ("reconnecting: the engine restarted", true)
        case .loggedOut: return ("logged out", true)
        case .other: return ("connection: \(connection.reason ?? "a state this Jaw doesn't know")", true)
        }
    }

    /// How the focused chat is, without its name, which its box and header already say.
    var chatLine: String? {
        if let notice, notice.until > Date() { return notice.text }
        guard let chat = conversation?.chat else { return nil }
        var parts: [String] = []
        if !chat.typing.isEmpty { parts.append(chat.kind.isGroup ? "someone's typing…" : "typing…") }
        if chat.muted { parts.append("muted") }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// The chats that need you, those no pane shows: mentions first, then people, then groups, each
    /// by its unread, as the spike's status line reads them from the chat list it already watches.
    var needs: [ChatRow] {
        let shown = Set(panes.keys)
        let waiting = (window?.chatList.all ?? []).filter {
            !shown.contains($0.id) && !$0.muted && !$0.archived && $0.kind != .community && ($0.unread > 0 || $0.markedUnread)
        }
        func rank(_ c: ChatRow) -> Int {
            c.mentions > 0 ? 0 : c.kind.isGroup ? 2 : 1
        }
        return waiting.sorted { (rank($0), -$0.unread) < (rank($1), -$1.unread) }
    }

    /// What fills the panes' place, when it isn't chats.
    var stateView: StateView.Kind? {
        switch reach {
        case let .starting(failed): return .starting(failed: failed)
        case let .unreachable(why, retry) where window?.chatList.all.isEmpty ?? true: return .asleep(why: why, retryIn: retry)
        default: break
        }
        if reach == .up, !linked {
            return .linking(step: link, loggedOut: connection.state == .loggedOut ? (connection.reason ?? "logged out from the phone") : nil)
        }
        if panes.isEmpty {
            return (window?.chatList.all.isEmpty ?? true) && reach == .up ? .noChats : .noChat
        }
        return nil
    }

    // MARK: - Panes

    /// Opens a chat: in the pane that shows it, or in the focused pane in place of its chat; `opened`
    /// when you picked it yourself, for frecency.
    func open(_ chat: ChatRow, opened: Bool, around message: String? = nil) {
        if let existing = panes[chat.id] {
            focus(chat.id)
            if let message { existing.go(to: message) }
            existing.focusBox()
            return
        }
        let place = message.map { Place.around($0, before: ConversationController.rowsAround, after: ConversationController.rowsAround) }
            ?? ConversationController.place(for: chat)
        let c = makePane(chat, place: place)
        if let old = focusedChat, let layout {
            closePane(old, keepLayout: true)
            self.layout = layout.replacing(old, with: chat.id)
        } else {
            layout = .pane(chat.id)
        }
        if opened { jawd.opened(chat.id) }
        focus(chat.id)
        if let message { c.select(message) }
        savePanes()
    }

    func makePane(_ chat: ChatRow, place: Place) -> ConversationController {
        let c = ConversationController(chat: chat, spec: spec, layouts: layouts, place: place)
        c.host = self
        c.notice = { [weak self] in self?.say($0) }
        _ = c.view
        panes[chat.id] = c
        paneWindows[jawd.watchMessages(chat.id, place)] = chat.id
        if chat.kind.isGroup { jawd.chatInfo(chat.id) }
        return c
    }

    func closePane(_ chat: String, keepLayout: Bool = false) {
        guard let c = panes.removeValue(forKey: chat) else { return }
        c.leave()
        c.view.removeFromSuperview()
        for (w, id) in paneWindows where id == chat {
            jawd.unwatch(w)
            paneWindows[w] = nil
        }
        if !keepLayout { layout = layout?.removing(chat) }
    }

    func focus(_ chat: String) {
        guard panes[chat] != nil else { return }
        let changed = focusedChat != chat
        focusedChat = chat
        window?.chatList.select(chat)
        if changed { window?.showPanes() }
        panes[chat]?.focusBox()
        panes[chat]?.readIfLooking()
        window?.refresh()
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

    func split(_ axis: PaneLayout.Axis) {
        guard let layout, let focused = focusedChat else { return }
        guard panes.count < PaneLayout.most else { return say("\(PaneLayout.most) panes at most") }
        guard let next = nextUnshown() else { return say("every chat in the list has a pane") }
        _ = makePane(next, place: ConversationController.place(for: next))
        self.layout = layout.splitting(focused, axis, with: next.id)
        focus(next.id)
        window?.showPanes()
        savePanes()
    }

    func closeFocusedPane() {
        guard let focused = focusedChat, panes.count > 1 else {
            window?.window?.performClose(nil)
            return
        }
        let next = layout?.neighbour(of: focused, .left) ?? layout?.neighbour(of: focused, .up)
            ?? layout?.panes.first { $0 != focused }
        closePane(focused)
        focusedChat = nil
        if let next { focus(next) }
        window?.showPanes()
        savePanes()
    }

    func move(_ direction: PaneLayout.Direction) {
        guard let focused = focusedChat, let to = layout?.neighbour(of: focused, direction) else { return }
        focus(to)
    }

    func goToPane(_ n: Int) {
        guard let panes = layout?.panes, n >= 1, n <= panes.count else { return }
        focus(panes[n - 1])
    }

    func swap(_ direction: PaneLayout.Direction) {
        guard let focused = focusedChat, let other = layout?.neighbour(of: focused, direction) else { return }
        layout = layout?.swapping(focused, other)
        window?.showPanes()
        savePanes()
    }

    func resized(_ pane: String, _ axis: PaneLayout.Axis, _ share: Double) {
        layout = layout?.resizing(pane, axis, to: share)
        savePanes()
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

    // MARK: - Keeping the panes

    struct Kept: Codable {
        let layout: PaneLayout
        let focused: String?
    }

    var keptKey: String {
        "panes:" + launch.home
    }

    /// The layout kept in the app's preferences, per jaw folder, until jawd keeps an interface's
    /// state (P1).
    func savePanes() {
        guard restored else { return }
        if let layout, let data = try? JSONEncoder().encode(Kept(layout: layout, focused: focusedChat)) {
            UserDefaults.standard.set(data, forKey: keptKey)
        } else {
            UserDefaults.standard.removeObject(forKey: keptKey)
        }
    }

    /// Opens the panes kept from last time, once the chat list has the chats they show.
    func restorePanes() {
        guard !restored, launch.bench == nil, launch.snapshot == nil else { return }
        restored = true
        guard let data = UserDefaults.standard.data(forKey: keptKey), let kept = try? JSONDecoder().decode(Kept.self, from: data),
              let list = window?.chatList
        else { return }
        var layout: PaneLayout? = kept.layout
        for id in kept.layout.panes {
            if let row = list.row(id) {
                _ = makePane(row, place: ConversationController.place(for: row))
            } else {
                layout = layout?.removing(id)
            }
        }
        self.layout = layout
        if let focused = kept.focused, panes[focused] != nil { focus(focused) } else if let first = layout?.panes.first { focus(first) }
        window?.showPanes()
    }

    // MARK: - Focus

    /// Focus is true while the app is active and its window visible and not minimised.
    func updateFocus() {
        let w = window?.window
        let now = NSApp.isActive && (w?.isVisible ?? false) && !(w?.isMiniaturized ?? true)
            && (w?.occlusionState.contains(.visible) ?? false)
        guard now != focused else { return }
        focused = now
        jawd?.focus(now)
        if now { conversation?.readIfLooking() }
    }

    // MARK: - The palette's actions

    /// Every action the palette offers now, with its key, as the terminal's palette lists them.
    func actions() -> [PaletteAction] {
        var all: [PaletteAction] = []
        let w = window
        let unread = (w?.chatList.all ?? []).filter { $0.unread > 0 && !$0.muted && $0.kind != .community }
        all.append(PaletteAction("Start a chat", "with anyone, or a number", key: "⌘N") { w?.showPalette(nil) })
        all.append(PaletteAction("Go to unread", unread.isEmpty ? "nothing unread" : "\(unread.count) chats") { [weak self] in self?.goToUnread() })
        all.append(PaletteAction("Search messages", "everywhere", key: "⌘⇧F") { w?.showPalette(kind: .message) })
        if let c = conversation {
            all.append(PaletteAction("Search this chat", "its messages", key: "⌘F") { w?.showPalette(kind: .message, scope: (c.chat.id, c.chat.name)) })
            if let m = c.selectedRow {
                if m.mine, m.status == .failed { all.append(PaletteAction("Send it again", "it wasn't sent", key: "⏎") { c.act("\n") }) }
                all.append(PaletteAction("Reply to this message", "the selected message", key: "r") { c.act("r") })
                all.append(PaletteAction("React to this message", "the selected message", key: "+") { c.act("+") })
                if ConversationController.notEditable(m) == nil { all.append(PaletteAction("Edit this message", "the selected message", key: "e") { c.act("e") }) }
                all.append(PaletteAction("Delete this message", "for you, or for everyone", key: "d") { c.act("d") })
                if ConversationController.notForwardable(m) == nil {
                    all.append(PaletteAction("Forward this message", "to up to five chats", key: "f") { c.act("f") })
                }
                all.append(PaletteAction("This message's info", "who has it and read it", key: "i") { c.act("i") })
                all.append(PaletteAction("Copy this message", "its text", key: "y") { c.act("y") })
            }
            all.append(contentsOf: chatActions(c.chat))
            all.append(PaletteAction("Split this pane across", "another chat beside it", key: "⌘D") { [weak self] in self?.split(.across) })
            all.append(PaletteAction("Split this pane down", "another chat below it", key: "⌘⇧D") { [weak self] in self?.split(.down) })
            if panes.count > 1 {
                all.append(PaletteAction("Close this pane", "its chat off the screen", key: "⌘W") { [weak self] in self?.closeFocusedPane() })
            }
            all.append(PaletteAction(focusMode ? "All chats again" : "Focus on this chat",
                                     focusMode ? "the chat list and sidebar back" : "the chat list and sidebar folded away", key: "⌘⇧↩")
            { [weak self] in self?.toggleFocusMode() })
        }
        all.append(PaletteAction("The glance", "the chats that need you, staying here", key: "⌘J") { w?.showGlance() })
        all.append(PaletteAction("Show or fold the sidebar", "All and Archived", key: "⌘B") { w?.toggleSidebar() })
        all.append(PaletteAction("Bubbles", "how messages look") { [weak self] in self?.preferences.look = .bubbles })
        all.append(PaletteAction("Lines", "how messages look, as the terminal's") { [weak self] in self?.preferences.look = .lines })
        all.append(PaletteAction("New poll", "comes with jawd's polls") { [weak self] in self?.say("polls come later") })
        all.append(PaletteAction("Attach files", "comes with jawd's M4", key: "⌘O") { [weak self] in self?.say("attaching comes with jawd's M4") })
        all.append(PaletteAction("Record a voice message", "comes with jawd's M5", key: "⌘R") { [weak self] in self?.say("recording comes with jawd's M5") })
        all.append(PaletteAction("Library", "comes with jawd's M3", key: "⌘L") { [weak self] in self?.say("the library comes with jawd's M3") })
        if !linked { all.append(PaletteAction("Link a device", "this Mac, by QR code") { w?.refresh() }) }
        all.append(PaletteAction("Settings", "appearance and messages", key: "⌘,") { w?.showSettings() })
        all.append(PaletteAction("jawd's status", "as jaw status prints it") { [weak self] in self?.jawd.status() })
        all.append(PaletteAction("The keys", "every command with its key", key: "⌘/") { w?.showKeys() })
        all.append(PaletteAction("Quit", "Jaw", key: "⌘Q") { NSApp.terminate(nil) })
        return all
    }

    /// Pinning, archiving, muting and marking, which jawd says when it serves them.
    func chatActions(_ chat: ChatRow) -> [PaletteAction] {
        let id = chat.id
        var all: [PaletteAction] = []
        if chat.muted {
            all.append(PaletteAction("Unmute this chat", "its notifications back") { [weak self] in
                self?.jawd.chatAction("chat.mute", chat: id, ["until": .null], what: "unmuting")
            })
        } else {
            let now = Double(Format.now())
            for (name, until) in [("for 8 hours", now + 8 * 3_600_000), ("for a week", now + 7 * 86_400_000), ("always", -1)] {
                all.append(PaletteAction("Mute this chat \(name)", "no notifications from it") { [weak self] in
                    self?.jawd.chatAction("chat.mute", chat: id, ["until": .number(until)], what: "muting")
                })
            }
        }
        all.append(PaletteAction(chat.archived ? "Unarchive this chat" : "Archive this chat", "as on the phone") { [weak self] in
            self?.jawd.chatAction("chat.archive", chat: id, ["archived": .bool(!chat.archived)], what: "archiving")
        })
        all.append(PaletteAction(chat.pinned ? "Unpin this chat" : "Pin this chat", "at the top of the list") { [weak self] in
            self?.jawd.chatAction("chat.pin", chat: id, ["pinned": .bool(!chat.pinned)], what: "pinning")
        })
        let unread = chat.unread > 0 || chat.markedUnread
        all.append(PaletteAction(unread ? "Mark this chat read" : "Mark this chat unread", "as on the phone") { [weak self] in
            self?.jawd.chatAction("chat.mark_unread", chat: id, ["unread": .bool(!unread)], what: unread ? "marking a chat read" : "marking a chat unread")
        })
        return all
    }

    /// The next chat with unread messages after the focused one.
    func goToUnread() {
        let unread = (window?.chatList.all ?? []).filter { $0.unread > 0 && !$0.muted && $0.kind != .community }
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
}
