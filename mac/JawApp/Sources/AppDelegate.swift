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

        window = MainWindowController(model: model)
        assert(Menus.problems(model.commands).isEmpty, Menus.problems(model.commands).joined(separator: "; "))
        NSApp.mainMenu = Menus.bar(model.commands)
        NSApp.windowsMenu = NSApp.mainMenu?.item(withTitle: MenuPlace.window.rawValue)?.submenu
        NSApp.helpMenu = NSApp.mainMenu?.item(withTitle: MenuPlace.help.rawValue)?.submenu
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

    // MARK: - Asked from a menu

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
                ? "jaw agent install's LaunchAgent is installed. Jaw can stop it, remove it, and start its own jawd at login in its place, under the same label, so there's still only one."
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
}
