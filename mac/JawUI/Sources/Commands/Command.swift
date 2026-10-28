import AppKit

/// The menu a command sits in, in the menu bar's order.
public enum MenuPlace: String, CaseIterable, Sendable {
    case app = "Jaw"
    case file = "File"
    case edit = "Edit"
    case find = "Find"
    case chat = "Chat"
    case message = "Message"
    case panes = "Panes"
    case layouts = "Layouts"
    case view = "View"
    case window = "Window"
    case help = "Help"
}

/// Something the app does, written once: the main menu, the toolbar's and context menus, the
/// palette and the keys sheet are all made from commands, so every key has a menu item, every menu
/// item shows its key, and the palette lists the same things by the same names.
@MainActor
public final class Command {
    public let id: String
    /// As a menu says it: "Split Down".
    public let title: String
    /// As the palette says it, a sentence ("Split this pane down"), and its second line; worked out
    /// when the palette opens, so "Pin this chat" can be "Unpin this chat".
    let says: @MainActor @Sendable () -> (String, String)
    public let symbol: String?
    public let shortcut: Shortcut?
    /// Other keys that do the same, such as ⌃K for the palette; they work, and the keys sheet lists
    /// them, but the menu shows the first.
    public let also: [Shortcut]
    /// A key the view under the focus handles itself, shown and not bound: r replies to a selected
    /// message, so a menu mustn't take r from the box.
    public let hint: String?
    public let place: MenuPlace?
    /// A submenu within the place: "Message Style".
    public let submenu: String?
    /// Commands of a place are grouped, with a separator between groups.
    public let group: Int
    /// In the palette as well as the menus.
    public let inPalette: Bool
    /// A standard action sent along the responder chain (copy:, undo:, performClose:) instead of `run`.
    public let send: Selector?
    public let enabled: @MainActor @Sendable () -> Bool
    /// A checkmark in the menu, when the command is a setting.
    public let on: (@MainActor @Sendable () -> Bool)?
    public let run: @MainActor @Sendable () -> Void

    public convenience init(_ id: String, _ title: String, phrase: String? = nil, detail: String = "",
                says: (@MainActor @Sendable () -> (String, String))? = nil, symbol: String? = nil,
                key: Shortcut? = nil, also: [Shortcut] = [], hint: String? = nil, place: MenuPlace?, submenu: String? = nil,
                group: Int = 0, palette: Bool = true,
                enabled: @escaping @MainActor @Sendable () -> Bool = { true }, on: (@MainActor @Sendable () -> Bool)? = nil,
                run: @escaping @MainActor @Sendable () -> Void)
    {
        self.init(id, title, phrase: phrase, detail: detail, says: says, symbol: symbol, key: key, also: also, hint: hint,
                  place: place, submenu: submenu, group: group, palette: palette, send: nil, enabled: enabled, on: on, run: run)
    }

    /// A standard action, sent along the responder chain to whatever takes it: copy:, undo:,
    /// performClose:.
    public convenience init(_ id: String, _ title: String, key: Shortcut? = nil, place: MenuPlace, group: Int = 0, send: Selector) {
        self.init(id, title, phrase: nil, detail: "", says: nil, symbol: nil, key: key, also: [], hint: nil, place: place, submenu: nil,
                  group: group, palette: false, send: send, enabled: { true }, on: nil, run: {})
    }

    init(_ id: String, _ title: String, phrase: String?, detail: String, says: (@MainActor @Sendable () -> (String, String))?,
         symbol: String?, key: Shortcut?, also: [Shortcut], hint: String?, place: MenuPlace?, submenu: String?, group: Int,
         palette: Bool, send: Selector?, enabled: @escaping @MainActor @Sendable () -> Bool, on: (@MainActor @Sendable () -> Bool)?,
         run: @escaping @MainActor @Sendable () -> Void)
    {
        self.id = id
        self.title = title
        let fixed = (phrase ?? title, detail)
        self.says = says ?? { fixed }
        self.symbol = symbol
        shortcut = key
        self.also = also
        self.hint = hint
        self.place = place
        self.submenu = submenu
        self.group = group
        inPalette = palette && send == nil
        self.send = send
        self.enabled = enabled
        self.on = on
        self.run = run
    }

    /// What the palette and the keys sheet show for its key.
    public var keyText: String? {
        shortcut?.display ?? hint
    }

    /// A menu item that runs the command, enabled and checked as it says; under a title of its own,
    /// which says what it will do (Unpin), it has no tick.
    public func menuItem(title: String? = nil) -> NSMenuItem {
        let item = NSMenuItem(title: title ?? self.title, action: send ?? #selector(CommandTarget.runCommand(_:)),
                              keyEquivalent: shortcut?.menuKey ?? "")
        item.keyEquivalentModifierMask = shortcut?.modifiers ?? []
        item.target = send == nil ? CommandTarget.shared : nil
        item.representedObject = CommandBox(self, ticks: title == nil)
        if let symbol { item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: self.title) }
        return item
    }

    /// Hidden items for the other keys, which still answer them.
    func alsoItems() -> [NSMenuItem] {
        also.map { key in
            let item = NSMenuItem(title: title, action: #selector(CommandTarget.runCommand(_:)), keyEquivalent: key.menuKey)
            item.keyEquivalentModifierMask = key.modifiers
            item.target = CommandTarget.shared
            item.representedObject = CommandBox(self)
            item.isHidden = true
            item.allowsKeyEquivalentWhenHidden = true
            return item
        }
    }

    public var phrase: String {
        says().0
    }

    public var paletteAction: PaletteAction {
        let (phrase, detail) = says()
        return PaletteAction(phrase, detail, key: keyText, run: run)
    }
}

/// Holds a command in a menu item, which keeps its represented object strongly.
final class CommandBox: NSObject {
    let command: Command
    /// Whether the item shows the command's tick: not when its title says what it will do (Unpin).
    let ticks: Bool

    @MainActor init(_ command: Command, ticks: Bool = true) {
        self.command = command
        self.ticks = ticks
    }
}

/// Runs and validates every command's menu items.
@MainActor
final class CommandTarget: NSObject, NSMenuItemValidation {
    static let shared = CommandTarget()

    @objc func runCommand(_ item: NSMenuItem) {
        guard let command = (item.representedObject as? CommandBox)?.command, command.enabled() else { return }
        command.run()
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        guard let box = item.representedObject as? CommandBox else { return true }
        if box.ticks, let on = box.command.on { item.state = on() ? .on : .off }
        return box.command.enabled()
    }
}

/// Menus made from commands.
@MainActor
public enum Menus {
    /// The menu bar: each place's commands in their groups, submenus gathered where they first
    /// appear.
    public static func bar(_ commands: [Command]) -> NSMenu {
        let main = MenuBar()
        for place in MenuPlace.allCases {
            let mine = commands.filter { $0.place == place }
            guard !mine.isEmpty else { continue }
            let item = NSMenuItem(title: place.rawValue, action: nil, keyEquivalent: "")
            item.submenu = menu(mine, title: place.rawValue)
            main.addItem(item)
        }
        return main
    }

    /// One menu of commands, in their groups.
    public static func menu(_ commands: [Command], title: String = "") -> NSMenu {
        let menu = NSMenu(title: title)
        var group: Int?
        var submenus: [String: NSMenu] = [:]
        for c in commands {
            if let sub = c.submenu {
                if let existing = submenus[sub] {
                    existing.addItem(c.menuItem())
                    continue
                }
                if group != nil, group != c.group { menu.addItem(.separator()) }
                group = c.group
                let holder = NSMenuItem(title: sub, action: nil, keyEquivalent: "")
                let inner = NSMenu(title: sub)
                inner.addItem(c.menuItem())
                holder.submenu = inner
                submenus[sub] = inner
                menu.addItem(holder)
                continue
            }
            if group != nil, group != c.group { menu.addItem(.separator()) }
            group = c.group
            menu.addItem(c.menuItem())
            for hidden in c.alsoItems() {
                menu.addItem(hidden)
            }
        }
        return menu
    }

    /// A right-click menu from its entries, each command found by `command`: an entry with no command
    /// is left out, a submenu with nothing in it too, and a separator only stands between items.
    /// Items are enabled and ticked as things stand when it's made, which is as it opens.
    /// A command sent along the responder chain (copy:) doesn't belong in one: nothing asks the chain.
    public static func menu(_ entries: [MenuEntry], command: (String) -> Command?) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in items(entries, command: command) {
            menu.addItem(item)
        }
        return menu
    }

    /// The items `menu(_:command:)` makes, to go on the end of another menu.
    public static func items(_ entries: [MenuEntry], command: (String) -> Command?) -> [NSMenuItem] {
        var out: [NSMenuItem] = []
        for entry in entries {
            switch entry {
            case let .command(id, title):
                guard let c = command(id) else { continue }
                assert(c.send == nil, "\(id) goes along the responder chain, which a right-click menu doesn't ask")
                let item = c.menuItem(title: title)
                item.isEnabled = CommandTarget.shared.validateMenuItem(item)
                out.append(item)
            case let .submenu(title, inner):
                let made = items(inner, command: command)
                guard !made.isEmpty else { continue }
                let sub = NSMenu(title: title)
                sub.autoenablesItems = false
                for item in made {
                    sub.addItem(item)
                }
                let holder = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                holder.submenu = sub
                holder.isEnabled = made.contains(where: \.isEnabled)
                out.append(holder)
            case .separator:
                if let last = out.last, !last.isSeparatorItem { out.append(.separator()) }
            }
        }
        while out.last?.isSeparatorItem == true {
            out.removeLast()
        }
        return out
    }

    /// What's wrong with a list of commands: two with one key, a key with no menu to show it, a
    /// command twice. Debug builds stop on any at launch, and the tests run it.
    public static func problems(_ commands: [Command]) -> [String] {
        var out: [String] = []
        var keys: [Shortcut: String] = [:]
        var ids: Set<String> = []
        for c in commands {
            if !ids.insert(c.id).inserted { out.append("\(c.id) is listed twice") }
            for key in [c.shortcut].compactMap(\.self) + c.also {
                if let other = keys[key] { out.append("\(key.display) is both \(other) and \(c.id)") }
                keys[key] = c.id
                if c.place == nil { out.append("\(c.id) has \(key.display) but no menu") }
            }
        }
        return out
    }

    /// The keys sheet's sections: each place with the commands that have a key.
    public static func keys(_ commands: [Command]) -> [(String, [(String, String)])] {
        MenuPlace.allCases.compactMap { place in
            let rows: [(String, String)] = commands.filter { $0.place == place && $0.keyText != nil && $0.send == nil }
                .map { c in (([c.keyText!] + c.also.map(\.display)).joined(separator: " or "), c.phrase) }
            return rows.isEmpty ? nil : (place.rawValue, rows)
        }
    }
}

/// The menu bar. AppKit matches Return in a key whatever Shift says and runs the first item on it,
/// so ⌘↩ ran focus mode's ⇧⌘↩; the bar runs a command on Return only for its own modifiers.
@MainActor
final class MenuBar: NSMenu {
    nonisolated static let modifiers: NSEvent.ModifierFlags = [.command, .shift, .option, .control]

    /// AppKit asks the menu bar for a key on the main thread, though NSMenu doesn't say so.
    override nonisolated func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.type == .keyDown, event.charactersIgnoringModifiers == "\r" else { return super.performKeyEquivalent(with: event) }
        nonisolated(unsafe) let bar = self, key = event
        return MainActor.assumeIsolated { bar.runOnReturn(key) }
    }

    /// Runs the command on Return with exactly the key's modifiers, when there's one and it's enabled.
    func runOnReturn(_ event: NSEvent) -> Bool {
        guard let item = Self.item(in: self, key: "\r", modifiers: event.modifierFlags.intersection(Self.modifiers)),
              let action = item.action else { return false }
        if let target = item.target as? NSMenuItemValidation, !target.validateMenuItem(item) { return false }
        return NSApplication.shared.sendAction(action, to: item.target, from: item)
    }

    /// The item in `menu` or under it on `key` with exactly `modifiers`.
    static func item(in menu: NSMenu, key: String, modifiers: NSEvent.ModifierFlags) -> NSMenuItem? {
        for item in menu.items {
            if let inner = item.submenu, let found = Self.item(in: inner, key: key, modifiers: modifiers) { return found }
            if item.keyEquivalent == key, item.keyEquivalentModifierMask.intersection(Self.modifiers) == modifiers,
               !item.isHidden || item.allowsKeyEquivalentWhenHidden
            {
                return item
            }
        }
        return nil
    }
}

/// One entry of a right-click menu: a command by its id, under a title of its own when that says
/// what it will do (Unpin); a submenu; a separator. A menu's entries are worked out from rows, so
/// what it offers is tested without AppKit, and made into items as it opens.
public enum MenuEntry: Equatable, Sendable {
    case command(String, title: String? = nil)
    case submenu(String, [MenuEntry])
    case separator
}

/// How a view pops its right-click menu up for VoiceOver's Show Menu, which has no pointer to place
/// it by, nor a click for `menu(for:)` or a table's menu. A popped-up menu runs until it closes, so
/// tests stand in for `run`.
@MainActor
struct MenuPopUp {
    /// On the run loop's next turn: a menu opened in the turn an accessibility call started in
    /// doesn't post that it opened, so VoiceOver wouldn't announce it or move into it.
    var run: @MainActor (NSMenu, NSPoint, NSView) -> Void = { menu, point, view in
        DispatchQueue.main.async { menu.popUp(positioning: nil, at: point, in: view) }
    }

    /// Pops `menu` up at `point` in `view`; false when there's no menu.
    func show(_ menu: NSMenu?, at point: NSPoint, in view: NSView) -> Bool {
        guard let menu else { return false }
        run(menu, point, view)
        return true
    }
}
