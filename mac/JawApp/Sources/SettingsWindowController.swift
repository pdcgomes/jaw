import AppKit
import JawUI

/// Settings, in a window of its own beside the chats: its panes the toolbar's icon tabs
/// (`SettingsTabs`), closable and nothing else, in the app's appearance, opened on the pane last
/// shown, centred over the main window the first time and where it was left after.
@MainActor
final class SettingsWindowController: NSWindowController {
    let tabs: SettingsTabs
    let preferences: Preferences
    /// Whether the window has its place: kept from an earlier run, or given it as it first opened.
    var placed = false

    /// `keeps` is false for a snapshot, which takes nothing from the last run and leaves nothing for
    /// the next.
    init(preferences: Preferences, keeps: Bool) {
        self.preferences = preferences
        let kept = keeps ? UserDefaults.standard.string(forKey: "settingsPane").flatMap(SettingsPane.init(rawValue:)) : nil
        let pane = kept ?? .appearance
        tabs = SettingsTabs(preferences: preferences, showing: pane)
        let window = SettingsWindow(contentViewController: tabs)
        window.styleMask = [.titled, .closable]
        window.toolbarStyle = .preference
        window.isReleasedWhenClosed = false
        window.appearance = preferences.nsAppearance
        window.title = pane.title
        super.init(window: window)
        if keeps {
            window.setFrameAutosaveName("jaw.settings")
            placed = window.frame.origin != .zero
            tabs.chose = { UserDefaults.standard.set($0.rawValue, forKey: "settingsPane") }
        }
        tabs.fit(pane, animated: false)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Shows the window, or brings it to the front, on `pane` when one's asked for; the first time,
    /// centred over `main`.
    func show(_ pane: SettingsPane?, over main: NSWindow?) {
        preferences.choosing = nil
        guard let window else { return }
        if let pane { tabs.show(pane) }
        if !placed, let main {
            let size = window.frame.size
            window.setFrameOrigin(NSPoint(x: main.frame.midX - size.width / 2, y: main.frame.midY - size.height / 2))
            placed = true
        }
        showWindow(nil)
    }
}
