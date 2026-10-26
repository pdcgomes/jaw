import AppKit
import JawKit
import JawThemes
import JawUI
import SwiftUI

/// The app as one interface on jawd: the connection, the chat list's window and the open chat's,
/// what's said, and focus. jawd's updates are drained at the next display frame, or at once for an
/// answer the screen waits for, and applied before the frame draws.
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
    var me: Set<String> = []
    var listWindow: WindowID?
    var folder = "all"
    var conversation: ConversationController?
    var conversationWindow: WindowID?
    /// Messages sent and not yet answered for, by key, so a refused one goes back to its box.
    var sending: [String: (chat: String, text: String)] = [:]
    var notice: (text: String, until: Date, warn: Bool)?
    var infoFor: MessageRow?
    var focused = false
    var frameStarted: Date?
    /// What the launch bench waits for: the first frame with the chat list's rows.
    var firstRows: (() -> Void)?

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
    /// `JAW_TRACE=1` says each update drained, on standard error.
    let trace = ProcessInfo.processInfo.environment["JAW_TRACE"] != nil

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
    }

    func stop() {
        conversation?.leave()
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
        case let .chats(w, u) where w == listWindow:
            window?.chatList.apply(u)
            if let open = conversation, let row = u.upsert.first(where: { $0.id == open.chat.id }) {
                open.update(chat: row)
            }
            if !u.upsert.isEmpty, let first = firstRows {
                firstRows = nil
                DispatchQueue.main.async { first() }
            }
        case let .messages(w, u) where w == conversationWindow:
            conversation?.apply(u)
        case let .sent(key, _, _):
            sending[key] = nil
        case let .unsent(chat, key, text, why):
            sending[key] = nil
            if let c = conversation, c.chat.id == chat { c.unsent(text: text, why: why) } else { say("not sent: \(why)", warn: true) }
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
            if conversation?.chat.id == chat { conversation?.members = members }
        case let .chatStarted(row):
            open(row, opened: true)
        case let .notStarted(why):
            say("no chat started: \(why)", warn: true)
        case let .status(status):
            window?.showStatus(status)
        case .windowFailed, .link, .push, .chats, .messages:
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
        conversation?.spec = spec
        conversation?.applyTheme()
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
        case .updateNeeded: return ("update needed: WhatsApp wants a newer jaw · reading still works", true)
        case .banned: return ("temporarily banned until \(connection.until.map { Format.until($0) } ?? "later")", true)
        case .engineRestarting: return ("reconnecting: the engine restarted", true)
        case .loggedOut: return ("logged out", true)
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

    /// What fills the conversation's place, when it isn't a chat.
    var stateView: StateView.Kind? {
        switch reach {
        case let .starting(failed): return .starting(failed: failed)
        case let .unreachable(why, retry) where window?.chatList.all.isEmpty ?? true: return .asleep(why: why, retryIn: retry)
        default: break
        }
        if reach == .up, !linked {
            return connection.state == .loggedOut ? .loggedOut(reason: connection.reason) : .unlinked
        }
        if conversation == nil {
            return (window?.chatList.all.isEmpty ?? true) && reach == .up ? .noChats : .noChat
        }
        return nil
    }

    // MARK: - Opening chats

    /// Opens a chat in the conversation pane; `opened` when you picked it yourself, for frecency.
    func open(_ chat: ChatRow, opened: Bool, around message: String? = nil) {
        if let current = conversation, current.chat.id == chat.id, message == nil {
            current.focusBox()
            return
        }
        conversation?.leave()
        if let w = conversationWindow { jawd.unwatch(w) }
        let place = message.map { Place.around($0, before: ConversationController.rowsAround, after: ConversationController.rowsAround) }
            ?? ConversationController.place(for: chat)
        let c = ConversationController(chat: chat, spec: spec, layouts: layouts, place: place)
        c.host = self
        c.notice = { [weak self] in self?.say($0) }
        conversation = c
        conversationWindow = jawd.watchMessages(chat.id, place)
        if chat.kind.isGroup { jawd.chatInfo(chat.id) }
        if opened { jawd.opened(chat.id) }
        window?.chatList.select(chat.id)
        window?.show(conversation: c)
        if let message { c.select(message) }
    }

    func step(_ by: Int) {
        window?.chatList.step(by)
    }

    func pick(folder id: String) {
        folder = id
        if let w = listWindow { jawd.unwatch(w) }
        window?.chatList.show(folder: id)
        listWindow = jawd.watchChats(id, .top(ChatListController.rows))
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
}

extension AppModel: ConversationHost {
    func conversation(_ c: ConversationController, move place: Place) {
        guard c === conversation, let w = conversationWindow else { return }
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

    func conversationIsRead(_: ConversationController) -> Bool {
        focused && (window?.window?.isKeyWindow ?? false)
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
