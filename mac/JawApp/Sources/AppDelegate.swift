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
        assert(Menus.problems(model.allCommands).isEmpty, Menus.problems(model.allCommands).joined(separator: "; "))
        NSApp.mainMenu = Menus.bar(model.allCommands)
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
}
