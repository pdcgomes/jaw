import AppKit
import JawKit
import JawUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static weak var shared: AppDelegate?
    let model: AppModel
    var window: MainWindowController!

    init(launch: Launch) {
        model = AppModel(launch: launch)
        super.init()
        Self.shared = self
    }

    func applicationDidFinishLaunching(_: Notification) {
        // The desktop picture's tint would fight the theme's colours in every window.
        var arguments = UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain)
        arguments["NSReduceDesktopTinting"] = true
        UserDefaults.standard.setVolatileDomain(arguments, forName: UserDefaults.argumentDomain)

        NSApp.mainMenu = Menus.make(target: self)
        window = MainWindowController(model: model)
        model.window = window
        model.start()
        window.showWindow(nil)
        if let bench = model.launch.bench { Bench.run(bench, model: model) }
        if let path = model.launch.snapshot { Bench.snapshot(to: path, model: model) }
        for name in [NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { _ in
                MainActor.assumeIsolated { AppDelegate.shared?.model.updateFocus() }
            }
        }
        DistributedNotificationCenter.default().addObserver(forName: .init("AppleInterfaceThemeChangedNotification"), object: nil,
                                                            queue: .main)
        { _ in
            MainActor.assumeIsolated { AppDelegate.shared?.model.restyle() }
        }
    }

    func applicationWillTerminate(_: Notification) {
        model.stop()
    }

    func applicationShouldHandleReopen(_: NSApplication, hasVisibleWindows: Bool) -> Bool {
        if !hasVisibleWindows { window.showWindow(nil) }
        return true
    }

    // MARK: - Menu actions

    @objc func showPalette(_: Any?) {
        window.showPalette(nil)
    }

    @objc func showSettings(_: Any?) {
        window.showSettings()
    }

    @objc func showStatus(_: Any?) {
        model.jawd.status()
    }

    @objc func searchChat(_: Any?) {
        guard let c = model.conversation else { return window.showPalette(kind: .message) }
        window.showPalette(kind: .message, scope: (c.chat.id, c.chat.name))
    }

    @objc func searchAll(_: Any?) {
        window.showPalette(kind: .message)
    }

    @objc func glance(_: Any?) {
        window.showGlance()
    }

    @objc func focusMode(_: Any?) {
        model.toggleFocusMode()
    }

    @objc func keys(_: Any?) {
        window.showKeys()
    }

    @objc func splitAcross(_: Any?) {
        model.split(.across)
    }

    @objc func splitDown(_: Any?) {
        model.split(.down)
    }

    @objc func closePane(_: Any?) {
        model.closeFocusedPane()
    }

    @objc func paneMove(_ item: NSMenuItem) {
        model.move(Self.direction(item.tag))
    }

    @objc func paneSwap(_ item: NSMenuItem) {
        model.swap(Self.direction(item.tag))
    }

    @objc func paneGo(_ item: NSMenuItem) {
        model.goToPane(item.tag)
    }

    static func direction(_ tag: Int) -> PaneLayout.Direction {
        [.left, .right, .up, .down][max(0, min(tag, 3))]
    }

    @objc func nextChat(_: Any?) {
        model.step(1)
    }

    @objc func previousChat(_: Any?) {
        model.step(-1)
    }

    @objc func toggleSidebarFolders(_: Any?) {
        window.sidebarItem.animator().isCollapsed.toggle()
    }

    @objc func showChatList(_: Any?) {
        window.listItem.animator().isCollapsed.toggle()
    }

    @objc func lookBubbles(_: Any?) {
        model.preferences.look = .bubbles
    }

    @objc func lookLines(_: Any?) {
        model.preferences.look = .lines
    }

    @objc func toggleCompact(_: Any?) {
        model.preferences.density = model.preferences.density == .compact ? .comfortable : .compact
    }

    @objc func bigger(_: Any?) {
        model.preferences.textSize = min(model.preferences.textSize + 1, Preferences.textSizes.upperBound)
    }

    @objc func smaller(_: Any?) {
        model.preferences.textSize = max(model.preferences.textSize - 1, Preferences.textSizes.lowerBound)
    }

    @objc func actualSize(_: Any?) {
        model.preferences.textSize = 13
    }

    @objc func jawdAtLogin(_: Any?) {
        let alert = NSAlert()
        #if DEBUG
            alert.messageText = "A build from source doesn't start jawd at login"
            alert.informativeText = "Only a signed release registers its jawd as the LaunchAgent; this build runs \(model.launch.demo ? "a made-up account in \(model.launch.home)" : "against the jawd already running")."
            alert.runModal()
        #else
            let terminal = FileManager.default.fileExists(atPath: Agent.terminalAgent)
            alert.messageText = terminal ? "Take over the terminal's jawd?" : "Start jawd at login?"
            alert.informativeText = terminal
                ? "jaw agent install's LaunchAgent is installed. jaw can stop it, remove it, and start its own jawd at login in its place, under the same label, so there's still only one."
                : "jawd keeps your messages and brings notifications with no window open."
            alert.addButton(withTitle: terminal ? "Take Over" : "Start at Login")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
            let own = Bundle.main.object(forInfoDictionaryKey: "JawdVersion") as? String
            if let why = terminal ? model.agent.takeOver(running: model.welcome?.jawd, own: own) : model.agent.registerAgent() {
                model.say(why, warn: true)
            }
        #endif
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        switch item.action {
        case #selector(lookBubbles(_:)): item.state = model.preferences.look == .bubbles ? .on : .off
        case #selector(lookLines(_:)): item.state = model.preferences.look == .lines ? .on : .off
        case #selector(toggleCompact(_:)): item.state = model.preferences.density == .compact ? .on : .off
        case #selector(focusMode(_:)): item.state = model.focusMode ? .on : .off
        case #selector(paneMove(_:)), #selector(paneSwap(_:)): return model.panes.count > 1
        case #selector(paneGo(_:)): return item.tag <= model.panes.count && model.panes.count > 1
        default: break
        }
        return true
    }
}

/// The menus, every command with its key, so the menus teach the keys as the palette does.
@MainActor
enum Menus {
    static func make(target: AppDelegate) -> NSMenu {
        let main = NSMenu()
        func menu(_ title: String, _ items: [NSMenuItem]) {
            let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            let sub = NSMenu(title: title)
            for i in items {
                sub.addItem(i)
            }
            item.submenu = sub
            main.addItem(item)
        }
        func item(_ title: String, _ action: Selector?, _ key: String = "", _ mods: NSEvent.ModifierFlags = .command,
                  target t: AnyObject? = nil) -> NSMenuItem
        {
            let i = NSMenuItem(title: title, action: action, keyEquivalent: key)
            i.keyEquivalentModifierMask = mods
            i.target = t
            return i
        }
        let up = String(UnicodeScalar(NSUpArrowFunctionKey)!)
        let down = String(UnicodeScalar(NSDownArrowFunctionKey)!)
        menu("jaw", [
            item("About jaw", #selector(NSApplication.orderFrontStandardAboutPanel(_:))),
            .separator(),
            item("Settings…", #selector(AppDelegate.showSettings(_:)), ",", target: target),
            item("jawd at Login…", #selector(AppDelegate.jawdAtLogin(_:)), target: target),
            .separator(),
            item("Hide jaw", #selector(NSApplication.hide(_:)), "h"),
            item("Hide Others", #selector(NSApplication.hideOtherApplications(_:)), "h", [.command, .option]),
            item("Show All", #selector(NSApplication.unhideAllApplications(_:))),
            .separator(),
            item("Quit jaw", #selector(NSApplication.terminate(_:)), "q"),
        ])
        menu("File", [
            item("New Chat", #selector(AppDelegate.showPalette(_:)), "n", target: target),
            item("Find", #selector(AppDelegate.showPalette(_:)), "k", .control, target: target),
            .separator(),
            item("Close Pane", #selector(AppDelegate.closePane(_:)), "w", target: target),
            item("Close Window", #selector(NSWindow.performClose(_:)), "w", [.command, .shift]),
        ])
        let arrows = [NSLeftArrowFunctionKey, NSRightArrowFunctionKey, NSUpArrowFunctionKey, NSDownArrowFunctionKey]
            .map { String(UnicodeScalar($0)!) }
        let ways = ["Left", "Right", "Up", "Down"]
        var paneItems = [
            item("Split Across", #selector(AppDelegate.splitAcross(_:)), "d", target: target),
            item("Split Down", #selector(AppDelegate.splitDown(_:)), "d", [.command, .shift], target: target),
            .separator(),
        ]
        for (i, way) in ways.enumerated() {
            let m = item("Pane \(way)", #selector(AppDelegate.paneMove(_:)), arrows[i], [.command, .option], target: target)
            m.tag = i
            paneItems.append(m)
        }
        paneItems.append(.separator())
        for (i, way) in ways.enumerated() {
            // ⌘⇧ and an arrow select to a line's end in the box, so swapping takes ⌃⌘.
            let m = item("Swap \(way)", #selector(AppDelegate.paneSwap(_:)), arrows[i], [.command, .control], target: target)
            m.tag = i
            paneItems.append(m)
        }
        paneItems.append(.separator())
        for n in 1 ... 9 {
            let m = item("Pane \(n)", #selector(AppDelegate.paneGo(_:)), "\(n)", [.command, .option], target: target)
            m.tag = n
            paneItems.append(m)
        }
        menu("Panes", paneItems)
        menu("Find", [
            item("Palette", #selector(AppDelegate.showPalette(_:)), "k", target: target),
            item("Search This Chat", #selector(AppDelegate.searchChat(_:)), "f", target: target),
            item("Search Every Chat", #selector(AppDelegate.searchAll(_:)), "f", [.command, .shift], target: target),
            item("The Glance", #selector(AppDelegate.glance(_:)), "j", target: target),
        ])
        menu("Edit", [
            item("Undo", Selector(("undo:")), "z"),
            item("Redo", Selector(("redo:")), "z", [.command, .shift]),
            .separator(),
            item("Cut", #selector(NSText.cut(_:)), "x"),
            item("Copy", #selector(NSText.copy(_:)), "c"),
            item("Paste", #selector(NSText.paste(_:)), "v"),
            item("Select All", #selector(NSText.selectAll(_:)), "a"),
            .separator(),
            item("Emoji & Symbols", #selector(NSApplication.orderFrontCharacterPalette(_:)), " ", [.command, .control]),
        ])
        let style = NSMenuItem(title: "Message Style", action: nil, keyEquivalent: "")
        let styles = NSMenu()
        styles.addItem(item("Bubbles", #selector(AppDelegate.lookBubbles(_:)), target: target))
        styles.addItem(item("Lines", #selector(AppDelegate.lookLines(_:)), target: target))
        styles.addItem(.separator())
        styles.addItem(item("Compact", #selector(AppDelegate.toggleCompact(_:)), target: target))
        style.submenu = styles
        menu("View", [
            style,
            item("Bigger", #selector(AppDelegate.bigger(_:)), "+", target: target),
            item("Smaller", #selector(AppDelegate.smaller(_:)), "-", target: target),
            item("Actual Size", #selector(AppDelegate.actualSize(_:)), "0", target: target),
            .separator(),
            item("Sidebar", #selector(AppDelegate.toggleSidebarFolders(_:)), "b", target: target),
            item("Chat List", #selector(AppDelegate.showChatList(_:)), "l", [.command, .shift], target: target),
            item("Focus on This Chat", #selector(AppDelegate.focusMode(_:)), "\r", [.command, .shift], target: target),
            .separator(),
            item("Previous Chat", #selector(AppDelegate.previousChat(_:)), up, .option, target: target),
            item("Next Chat", #selector(AppDelegate.nextChat(_:)), down, .option, target: target),
            .separator(),
            item("Enter Full Screen", #selector(NSWindow.toggleFullScreen(_:)), "f", [.command, .control]),
        ])
        menu("Window", [
            item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"),
            item("Zoom", #selector(NSWindow.performZoom(_:))),
        ])
        menu("Help", [
            item("The Keys", #selector(AppDelegate.keys(_:)), "/", target: target),
            item("jawd's Status", #selector(AppDelegate.showStatus(_:)), target: target),
        ])
        NSApp.windowsMenu = main.item(withTitle: "Window")?.submenu
        return main
    }
}
