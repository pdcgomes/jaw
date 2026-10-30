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
        for c in chatCommands(for: chat) {
            add(c)
        }
        // The box's + menu: attachments, and what's composed; those still to come say what they wait on.
        let plus: [(String, String, String, String, String, Shortcut?, String?)] = [
            ("attach.photos", "Photos & Videos…", "Send photos or videos", "from Finder or Photos", "photo.on.rectangle", Shortcut("o"),
             "sending photos and videos comes with jawd's M4"),
            ("attach.document", "Document…", "Send a document", "any file", "doc", Shortcut("o", [.command, .shift]),
             "sending documents comes with jawd's M4"),
            ("attach.camera", "Camera…", "Take a photo with the iPhone", "Continuity Camera", "camera", nil, "the camera comes with jawd's M4"),
            ("attach.contact", "Contact…", "Send a contact", "from Contacts", "person.crop.circle", nil, "sending contacts comes with jawd's send.contact"),
            ("compose.poll", "Poll…", "New poll", "a question and its options", "chart.bar.xaxis", nil, nil),
            ("compose.event", "Event…", "New event", "a time, a place, a call link", "calendar", nil, "events come with jawd's send.event"),
            ("attach.location", "Location…", "Send a place", "a search, or where you are", "mappin.and.ellipse", nil,
             "sending places comes with jawd's send.location"),
        ]
        for (id, title, phrase, detail, symbol, key, later) in plus {
            add(Command(id, title, phrase: phrase, detail: detail, symbol: symbol, key: key, place: .chat, group: 2, enabled: isChat)
                { [weak self] in
                    if let later { self?.say(later) } else { self?.composePoll() }
                })
        }
        add(Command("box.emoji", "Emoji…", phrase: "Emoji", detail: "find one by name", symbol: "face.smiling",
                    key: Shortcut("e", [.control, .command]), place: .chat, group: 3, enabled: isChat)
            { [weak self] in self?.conversation?.composer.showEmoji() })
        add(Command("box.gif", "GIFs…", phrase: "GIFs", detail: "from Giphy, through jawd", key: Shortcut("g", [.control, .command]), place: .chat,
                    group: 3, enabled: isChat) { [weak self] in self?.say("GIF search comes with jawd's gif.search") })
        add(Command("record", "Record a Voice Message", phrase: "Record a voice message", detail: "comes with jawd's M5", key: Shortcut("r"),
                    place: .chat, group: 3) { [weak self] in self?.say("recording comes with jawd's M5") })
        add(Command("library", "Library", phrase: "Library", detail: "everything shared, in every chat", symbol: "photo.on.rectangle.angled",
                    key: Shortcut("l"), place: .chat, group: 3) { [weak self] in self?.openLibrary(.every) })
        add(Command("chat.library", "This Chat's Library", phrase: "This chat's library", detail: "everything shared in it",
                    key: Shortcut("l", [.command, .option]), place: .chat, group: 3, enabled: isChat)
            { [weak self] in
                guard let self, let c = conversation else { return }
                openLibrary(.chat(id: c.chat.id, name: c.chat.name))
            })

        // Message: keys the conversation takes itself while a message is selected.
        for (id, title, phrase, key) in [
            ("reply", "Reply", "Reply to this message", "r"), ("react", "React…", "React to this message", "+"),
            ("edit", "Edit", "Edit this message", "e"), ("delete", "Delete…", "Delete this message", "d"),
            ("forward", "Forward…", "Forward this message", "f"), ("info", "Info", "This message's info", "i"),
            ("copy.text", "Copy Text", "Copy this message", "y"), ("file", "Add to Folder…", "Add this chat to a folder", "m"),
        ] {
            add(Command("message.\(id)", title, phrase: phrase, detail: "the selected message", hint: key, place: .message,
                        enabled: { selected().map { Self.can(id, $0) } ?? false }) { [weak self] in self?.conversation?.act(Character(key)) })
        }
        add(Command("message.retry", "Send Again", phrase: "Send it again", detail: "it wasn't sent", hint: "↩", place: .message, group: 1,
                    enabled: { selected().map { $0.mine && $0.status == .failed } ?? false }) { [weak self] in self?.conversation?.act("\n") })
        // A message's file, as its menu has it: the conversation takes the keys itself while a message
        // is selected.
        let offer: @MainActor @Sendable () -> MediaOffer? = { selected().flatMap { MediaOffer.of($0) } }
        func fileCommand(_ id: String, _ title: String, _ phrase: String, _ key: String?, _ act: MediaOffer.Act,
                         enabled: @escaping @MainActor @Sendable () -> Bool = { offer() != nil })
        {
            add(Command("message.\(id)", title, phrase: phrase, detail: "the selected message's", hint: key, place: .message, group: 2,
                        enabled: enabled) { [weak self] in self?.conversation?.media(act) })
        }
        // Saving and showing a file act on a library's selected item too, while its pane is focused.
        let library: @MainActor @Sendable () -> LibraryController? = { [weak self] in self?.focusedLibrary }
        func itemFileCommand(_ id: String, _ title: String, _ phrase: String, _ key: String?, _ act: MediaOffer.Act) {
            let enabled: @MainActor @Sendable () -> Bool = { library().map { $0.selectedOffer != nil } ?? (offer() != nil) }
            add(Command("message.\(id)", title, says: { (phrase, library() == nil ? "the selected message's" : "the selected item's") },
                        hint: key, place: .message, group: 2, enabled: enabled) { [weak self] in
                if let library = library() { library.media(act) } else { self?.conversation?.media(act) }
            })
        }
        fileCommand("look", "Quick Look", "Look at this file", "Space", .look)
        fileCommand("open", "Open", "Open this file in its app", "⌘↩", .open)
        add(Command("message.open.with", "Open With", place: .message, group: 2, enabled: { offer()?.isHere ?? false },
                    fill: { [weak self] in self?.conversation?.openWithItems() ?? [] }))
        itemFileCommand("save", "Save to Downloads", "Save this file to Downloads", "s", .save)
        fileCommand("save.hd", "Save HD Copy to Downloads", "Save this photo's HD copy to Downloads", nil, .saveHD, enabled: { offer()?.media.hd ?? false })
        itemFileCommand("reveal", "Show in Finder", "Show this file in Finder", nil, .reveal)
        add(Command("message.file.state", "Download", says: { (offer()?.statePhrase ?? "Download this file", "the selected message's") },
                    titled: { offer()?.stateAct?.1 ?? "Download" }, place: .message, group: 2, enabled: { offer()?.stateAct != nil })
            { [weak self] in
                guard let act = offer()?.stateAct?.0 else { return }
                self?.conversation?.media(act)
            })

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

        // Layouts: the layouts themselves are `layoutCommands()`, made again as they change.
        let book: @MainActor @Sendable () -> LayoutBook? = { [weak self] in self?.book }
        add(Command("layout.save", "Save Layout", phrase: "Save this layout", detail: "keep its new shape", key: Shortcut("s", [.command, .option]),
                    place: .layouts, group: 1, enabled: { book()?.isEdited ?? false }) { [weak self] in self?.saveLayout() })
        add(Command("layout.revert", "Revert Layout", phrase: "Revert this layout", detail: "back to its saved shape", place: .layouts,
                    group: 1, enabled: { book()?.isEdited ?? false }) { [weak self] in self?.revertLayout() })
        add(Command("layout.save.as", "Save Layout As…", phrase: "Save these panes as a layout", detail: "with a name, to come back to",
                    symbol: "rectangle.3.group", key: Shortcut("s", [.command, .option, .shift]), place: .layouts, group: 1,
                    enabled: { [weak self] in self?.grid != nil }) { [weak self] in self?.window?.showLayoutEditor(.saveAs) })
        add(Command("layout.edit", "Edit Layout…", phrase: "Edit this layout", detail: "its panes on a grid", symbol: "square.grid.3x2",
                    key: Shortcut("e", [.command, .option]), place: .layouts, group: 1, enabled: { [weak self] in self?.grid != nil })
            { [weak self] in self?.window?.showLayoutEditor(.edit) })
        add(Command("layout.rename", "Rename Layout…", phrase: "Rename this layout", detail: "", place: .layouts, group: 2,
                    enabled: { book()?.current != nil }) { [weak self] in self?.window?.askLayoutRename() })
        add(Command("layout.delete", "Delete Layout…", phrase: "Delete this layout", detail: "its panes stay on screen", place: .layouts,
                    group: 2, enabled: { book()?.current != nil }) { [weak self] in self?.window?.askLayoutDelete() })
        add(Command("layout.terminal", "Open the Terminal's Panes", phrase: "Open the terminal's panes",
                    detail: "as the terminal left them, in place of the unsaved panes", symbol: "terminal", place: .layouts, group: 3)
            { [weak self] in self?.openTerminalPanes() })

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
        add(Command("sidebar", "Sidebar", phrase: "Show or fold the sidebar", detail: "the folders and the people", key: Shortcut("b"), place: .view,
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

    /// The layouts to go to, first in the Layouts menu: the unnamed one with ⌃⌘0, each saved one
    /// by its place with ⌃⌘1 to ⌃⌘9.
    func layoutCommands() -> [Command] {
        var out = [Command("layout.unnamed", "Unsaved Panes", phrase: "Go to the unsaved panes", detail: "the panes before any layout",
                           key: Shortcut("0", [.control, .command]), place: .layouts, palette: book.current != nil,
                           on: { [weak self] in self?.book.current == nil }) { [weak self] in self?.switchLayout(nil) }]
        for (i, name) in book.names.enumerated() {
            out.append(Command("layout.\(i + 1)", name, phrase: "Go to \(name)", detail: "a layout", symbol: "rectangle.3.group",
                               key: i < 9 ? Shortcut("\(i + 1)", [.control, .command]) : nil, place: .layouts,
                               on: { [weak self] in self?.book.current == name }) { [weak self] in self?.switchLayout(name) })
        }
        return out
    }

    /// The commands now, the layouts' among them.
    var allCommands: [Command] {
        layoutCommands() + commands
    }

    /// The chats with unread messages that aren't muted, in the list's order.
    func unreadChats() -> [ChatRow] {
        (window?.chatList.all ?? []).filter { $0.unread > 0 && !$0.muted && $0.kind != .community }
    }

    func command(_ id: String) -> Command? {
        allCommands.first { $0.id == id }
    }
}
