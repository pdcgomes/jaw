import AppKit
import JawKit
import JawUI

/// Everything the app does, once: the menu bar, the toolbar's menus, the palette and the keys
/// sheet are made from this list.
extension AppModel {
    func makeCommands() -> [Command] {
        var all: [Command] = []
        func add(_ c: Command) {
            all.append(c)
        }
        let selected: @MainActor @Sendable () -> MessageRow? = { [weak self] in self?.conversation?.selectedRow }
        let chat: @MainActor @Sendable () -> ChatRow? = { [weak self] in self?.conversation?.chat }
        let many: @MainActor @Sendable () -> Bool = { [weak self] in (self?.grid?.slots.count ?? 0) > 1 }

        // Jaw
        add(Command("about", "About Jaw", place: .app, send: #selector(NSApplication.orderFrontStandardAboutPanel(_:))))
        add(Command("settings", "Settings…", phrase: "Settings", detail: "appearance and messages", key: Shortcut(","), place: .app, group: 1)
            { [weak self] in self?.window?.showSettings() })
        add(Command("agent", "jawd at Login…", phrase: "jawd at login", detail: "start jawd when you log in", place: .app, group: 1)
            { AppDelegate.shared?.jawdAtLogin(nil) })
        add(Command("hide", "Hide Jaw", key: Shortcut("h"), place: .app, group: 2, send: #selector(NSApplication.hide(_:))))
        add(Command("hide.others", "Hide Others", key: Shortcut("h", [.command, .option]), place: .app, group: 2,
                    send: #selector(NSApplication.hideOtherApplications(_:))))
        add(Command("show.all", "Show All", place: .app, group: 2, send: #selector(NSApplication.unhideAllApplications(_:))))
        add(Command("quit", "Quit Jaw", phrase: "Quit", detail: "Jaw", key: Shortcut("q"), place: .app, group: 3) { NSApp.terminate(nil) })

        // File
        add(Command("chat.new", "New Chat", phrase: "Start a chat", detail: "with anyone, or a number", symbol: "square.and.pencil",
                    key: Shortcut("n"), place: .file) { [weak self] in self?.window?.showPalette(nil) })
        add(Command("pane.close", "Close Pane", phrase: "Close this pane", detail: "its chat off the screen", symbol: "xmark.rectangle",
                    key: Shortcut("w"), place: .file, group: 1, enabled: many) { [weak self] in self?.closeFocusedPane() })
        add(Command("window.close", "Close Window", key: Shortcut("w", [.command, .shift]), place: .file, group: 1,
                    send: #selector(NSWindow.performClose(_:))))

        // Edit
        for (id, title, key, mods, action) in [
            ("undo", "Undo", "z", NSEvent.ModifierFlags.command, Selector(("undo:"))),
            ("redo", "Redo", "z", [.command, .shift], Selector(("redo:"))),
        ] {
            add(Command(id, title, key: Shortcut(key, mods), place: .edit, send: action))
        }
        for (id, title, key, action) in [
            ("cut", "Cut", "x", #selector(NSText.cut(_:))), ("copy", "Copy", "c", #selector(NSText.copy(_:))),
            ("paste", "Paste", "v", #selector(NSText.paste(_:))), ("select.all", "Select All", "a", #selector(NSText.selectAll(_:))),
        ] {
            add(Command(id, title, key: Shortcut(key), place: .edit, group: 1, send: action))
        }
        add(Command("emoji", "Emoji & Symbols", key: Shortcut(" ", [.command, .control]), place: .edit, group: 2,
                    send: #selector(NSApplication.orderFrontCharacterPalette(_:))))

        // Find
        add(Command("palette", "Command Palette…", phrase: "The palette", detail: "chats, people, messages and every action",
                    symbol: "magnifyingglass", key: Shortcut("k"), also: [Shortcut("k", .control)], place: .find, palette: false)
            { [weak self] in self?.window?.showPalette(nil) })
        add(Command("search.chat", "Search This Chat", phrase: "Search this chat", detail: "its messages", key: Shortcut("f"), place: .find)
            { [weak self] in
                guard let self, let c = conversation else { return }
                window?.showPalette(kind: .message, scope: (c.chat.id, c.chat.name))
            })
        add(Command("search.all", "Search Every Chat", phrase: "Search messages", detail: "everywhere", key: Shortcut("f", [.command, .shift]),
                    place: .find) { [weak self] in self?.window?.showPalette(kind: .message) })
        add(Command("unread", "Go to Unread", phrase: "Go to unread", says: { [weak self] in
            let n = self?.unreadChats().count ?? 0
            return ("Go to unread", n == 0 ? "nothing unread" : "\(n) chats")
        }, key: Shortcut("u", [.command, .shift]), place: .find, group: 1) { [weak self] in self?.goToUnread() })
        add(Command("glance", "The Glance", phrase: "The glance", detail: "the chats that need you, staying here", key: Shortcut("j"),
                    place: .find, group: 1) { [weak self] in self?.window?.showGlance() })

        // Chat
        let isChat: @MainActor @Sendable () -> Bool = { chat() != nil }
        let now = Double(Format.now())
        for (id, name, until) in [("mute.8h", "For 8 Hours", now + 8 * 3_600_000), ("mute.week", "For a Week", now + 7 * 86_400_000),
                                  ("mute.always", "Always", -1)]
        {
            add(Command(id, name, phrase: "Mute this chat \(name.lowercased())", detail: "no notifications from it", place: .chat,
                        submenu: "Mute", enabled: { chat().map { !$0.muted } ?? false }) { [weak self] in
                    guard let c = chat() else { return }
                    self?.jawd.chatAction("chat.mute", chat: c.id, ["until": .number(until)], what: "muting")
                })
        }
        add(Command("unmute", "Unmute", phrase: "Unmute this chat", detail: "its notifications back", place: .chat, submenu: "Mute",
                    enabled: { chat()?.muted ?? false }) { [weak self] in
                guard let c = chat() else { return }
                self?.jawd.chatAction("chat.mute", chat: c.id, ["until": .null], what: "unmuting")
            })
        add(Command("archive", "Archived", says: { (chat()?.archived ?? false ? "Unarchive this chat" : "Archive this chat", "as on the phone") },
                    place: .chat, group: 1, enabled: isChat, on: { chat()?.archived ?? false }) { [weak self] in
                guard let c = chat() else { return }
                self?.jawd.chatAction("chat.archive", chat: c.id, ["archived": .bool(!c.archived)], what: "archiving")
            })
        add(Command("pin", "Pinned", says: { (chat()?.pinned ?? false ? "Unpin this chat" : "Pin this chat", "at the top of the list") },
                    place: .chat, group: 1, enabled: isChat, on: { chat()?.pinned ?? false }) { [weak self] in
                guard let c = chat() else { return }
                self?.jawd.chatAction("chat.pin", chat: c.id, ["pinned": .bool(!c.pinned)], what: "pinning")
            })
        let isUnread: @MainActor @Sendable () -> Bool = { chat().map { $0.unread > 0 || $0.markedUnread } ?? false }
        add(Command("mark", "Unread", says: { (isUnread() ? "Mark this chat read" : "Mark this chat unread", "as on the phone") },
                    place: .chat, group: 1, enabled: isChat, on: isUnread) { [weak self] in
                guard let c = chat() else { return }
                let unread = isUnread()
                self?.jawd.chatAction("chat.mark_unread", chat: c.id, ["unread": .bool(!unread)],
                                      what: unread ? "marking a chat read" : "marking a chat unread")
            })
        for (id, title, phrase, detail, key, later) in [
            ("attach", "Attach Files…", "Attach files", "comes with jawd's M4", Shortcut("o"), "attaching comes with jawd's M4"),
            ("record", "Record a Voice Message", "Record a voice message", "comes with jawd's M5", Shortcut("r"), "recording comes with jawd's M5"),
            ("library", "Library", "Library", "comes with jawd's M3", Shortcut("l"), "the library comes with jawd's M3"),
        ] {
            add(Command(id, title, phrase: phrase, detail: detail, key: key, place: .chat, group: 2) { [weak self] in self?.say(later) })
        }
        add(Command("poll", "New Poll", phrase: "New poll", detail: "comes with jawd's polls", place: .chat, group: 2)
            { [weak self] in self?.say("polls come later") })

        // Message: keys the conversation takes itself while a message is selected.
        for (id, title, phrase, key) in [
            ("reply", "Reply", "Reply to this message", "r"), ("react", "React…", "React to this message", "+"),
            ("edit", "Edit", "Edit this message", "e"), ("delete", "Delete…", "Delete this message", "d"),
            ("forward", "Forward…", "Forward this message", "f"), ("info", "Info", "This message's info", "i"),
            ("copy.text", "Copy Text", "Copy this message", "y"),
        ] {
            add(Command("message.\(id)", title, phrase: phrase, detail: "the selected message", hint: key, place: .message,
                        enabled: { selected().map { Self.can(id, $0) } ?? false }) { [weak self] in self?.conversation?.act(Character(key)) })
        }
        add(Command("message.retry", "Send Again", phrase: "Send it again", detail: "it wasn't sent", hint: "↩", place: .message, group: 1,
                    enabled: { selected().map { $0.mine && $0.status == .failed } ?? false }) { [weak self] in self?.conversation?.act("\n") })

        // Panes
        add(Command("split.across", "Split Across", phrase: "Split this pane across", detail: "another chat beside it",
                    symbol: "rectangle.split.2x1", key: Shortcut("d"), place: .panes, enabled: { [weak self] in self?.grid != nil }) { [weak self] in self?.split(.across) })
        add(Command("split.down", "Split Down", phrase: "Split this pane down", detail: "another chat below it",
                    symbol: "rectangle.split.1x2", key: Shortcut("d", [.command, .shift]), place: .panes,
                    enabled: { [weak self] in self?.grid != nil })
            { [weak self] in self?.split(.down) })
        for (i, (way, arrow)) in [("Left", "←"), ("Right", "→"), ("Up", "↑"), ("Down", "↓")].enumerated() {
            let direction = [PaneGrid.Direction.left, .right, .up, .down][i]
            add(Command("pane.\(way.lowercased())", "Pane \(way)", phrase: "The pane \(way.lowercased())", key: Shortcut(arrow, [.command, .option]),
                        place: .panes, group: 1, palette: false, enabled: many) { [weak self] in self?.move(direction) })
        }
        for (i, (way, arrow)) in [("Left", "←"), ("Right", "→"), ("Up", "↑"), ("Down", "↓")].enumerated() {
            let direction = [PaneGrid.Direction.left, .right, .up, .down][i]
            // ⌘⇧ and an arrow select to a line's end in the box, so swapping takes ⌃⌘.
            add(Command("swap.\(way.lowercased())", "Swap \(way)", phrase: "Swap with the pane \(way.lowercased())",
                        key: Shortcut(arrow, [.command, .control]), place: .panes, group: 2, palette: false, enabled: many)
                { [weak self] in self?.swap(direction) })
        }
        for n in 1 ... 9 {
            add(Command("pane.\(n)", "Pane \(n)", key: Shortcut("\(n)", [.command, .option]), place: .panes, group: 3, palette: false,
                        enabled: { [weak self] in
                            let count = self?.grid?.slots.count ?? 0
                            return count > 1 && n <= count
                        })
            { [weak self] in self?.goToPane(n) })
        }
        add(Command("focus.mode", "Focus on This Chat", says: { [weak self] in
            self?.focusMode ?? false ? ("All chats again", "the chat list and sidebar back") : ("Focus on this chat", "the chat list and sidebar folded away")
        }, key: Shortcut("↩", [.command, .shift]), place: .panes, group: 4, enabled: isChat, on: { [weak self] in self?.focusMode ?? false })
            { [weak self] in self?.toggleFocusMode() })

        // View
        add(Command("look.bubbles", "Bubbles", phrase: "Bubbles", detail: "how messages look", place: .view, submenu: "Message Style",
                    on: { [weak self] in self?.preferences.look == .bubbles }) { [weak self] in self?.preferences.look = .bubbles })
        add(Command("look.lines", "Lines", phrase: "Lines", detail: "how messages look, as the terminal's", place: .view,
                    submenu: "Message Style", on: { [weak self] in self?.preferences.look == .lines }) { [weak self] in self?.preferences.look = .lines })
        add(Command("compact", "Compact", phrase: "Compact messages", detail: "less space between them", place: .view,
                    submenu: "Message Style", on: { [weak self] in self?.preferences.density == .compact }) { [weak self] in
                guard let self else { return }
                preferences.density = preferences.density == .compact ? .comfortable : .compact
            })
        add(Command("bigger", "Bigger", phrase: "Bigger text", detail: "in messages", key: Shortcut("+"), place: .view, group: 1)
            { [weak self] in
                guard let self else { return }
                preferences.textSize = min(preferences.textSize + 1, Preferences.textSizes.upperBound)
            })
        add(Command("smaller", "Smaller", phrase: "Smaller text", detail: "in messages", key: Shortcut("-"), place: .view, group: 1)
            { [weak self] in
                guard let self else { return }
                preferences.textSize = max(preferences.textSize - 1, Preferences.textSizes.lowerBound)
            })
        add(Command("actual.size", "Actual Size", phrase: "Actual text size", detail: "in messages", key: Shortcut("0"), place: .view, group: 1)
            { [weak self] in self?.preferences.textSize = 13 })
        add(Command("sidebar", "Sidebar", phrase: "Show or fold the sidebar", detail: "All and Archived", key: Shortcut("b"), place: .view,
                    group: 2) { [weak self] in self?.window?.toggleSidebar() })
        add(Command("chat.list", "Chat List", phrase: "Show or fold the chat list", detail: "every chat", key: Shortcut("l", [.command, .shift]),
                    place: .view, group: 2) { [weak self] in self?.window?.toggleChatList() })
        add(Command("chat.previous", "Previous Chat", phrase: "The chat before", detail: "in the list", key: Shortcut("↑", .option),
                    place: .view, group: 3, palette: false) { [weak self] in self?.step(-1) })
        add(Command("chat.next", "Next Chat", phrase: "The chat after", detail: "in the list", key: Shortcut("↓", .option),
                    place: .view, group: 3, palette: false) { [weak self] in self?.step(1) })
        add(Command("full.screen", "Enter Full Screen", key: Shortcut("f", [.command, .control]), place: .view, group: 4,
                    send: #selector(NSWindow.toggleFullScreen(_:))))

        // Window
        add(Command("minimize", "Minimize", key: Shortcut("m"), place: .window, send: #selector(NSWindow.performMiniaturize(_:))))
        add(Command("zoom", "Zoom", place: .window, send: #selector(NSWindow.performZoom(_:))))

        // Help
        add(Command("keys", "The Keys", phrase: "The keys", detail: "every command with its key", key: Shortcut("/"), place: .help)
            { [weak self] in self?.window?.showKeys() })
        add(Command("status", "jawd's Status", phrase: "jawd's status", detail: "as jaw status prints it", place: .help)
            { [weak self] in self?.jawd.status() })
        add(Command("link", "Link a Device…", phrase: "Link a device", detail: "this Mac, by QR code", place: .help, group: 1,
                    enabled: { [weak self] in !(self?.linked ?? true) }) { [weak self] in self?.window?.refresh() })
        return all
    }

    /// Whether a message action applies to a message: only yours are edited, and some kinds can't
    /// be forwarded.
    static func can(_ action: String, _ m: MessageRow) -> Bool {
        switch action {
        case "edit": ConversationController.notEditable(m) == nil
        case "forward": ConversationController.notForwardable(m) == nil
        default: true
        }
    }

    /// The chats with unread messages that aren't muted, in the list's order.
    func unreadChats() -> [ChatRow] {
        (window?.chatList.all ?? []).filter { $0.unread > 0 && !$0.muted && $0.kind != .community }
    }

    func command(_ id: String) -> Command? {
        commands.first { $0.id == id }
    }
}
