import AppKit

/// The window's title bar, as the app's window has it and the harness draws it: the window's style,
/// and how a window made with it is dressed, its title bar clear and its title hidden, with the
/// sidebar's button beside the window's buttons.
@MainActor
public enum TitleBar {
    /// Titled, closable, miniaturizable and resizable, with the content filling the window under its
    /// title bar.
    public static let style: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]

    /// Dresses `window`, made with `style`: its title bar clear and its title hidden, and the sidebar's
    /// button, which runs `toggle`, beside the window's buttons. It returns the button, to be given
    /// the window's roles.
    public static func dress(_ window: NSWindow, toggle: @escaping () -> Void) -> IconButton {
        window.titlebarAppearsTransparent = true
        // No toolbar: its controls are in the bottom bar, so every pane has the window's height.
        window.titleVisibility = .hidden
        // The sidebar's button beside the window's, in the title bar as an accessory: a toolbar
        // would make the title bar taller and take the clicks of the panes' headers under it.
        let button = sidebarButton(toggle: toggle)
        let holder = NSView(frame: NSRect(x: 0, y: 0, width: 34, height: 28))
        holder.addSubview(button)
        NSLayoutConstraint.activate([
            button.leadingAnchor.constraint(equalTo: holder.leadingAnchor, constant: Space.xs),
            button.centerYAnchor.constraint(equalTo: holder.centerYAnchor),
        ])
        let accessory = NSTitlebarAccessoryViewController()
        accessory.view = holder
        accessory.layoutAttribute = .leading
        window.addTitlebarAccessoryViewController(accessory)
        return button
    }

    /// The sidebar's button as the title bar has it, running `toggle`.
    public static func sidebarButton(toggle: @escaping () -> Void) -> IconButton {
        IconButton(symbol: "sidebar.left", label: "Show or fold the sidebar", shortcut: Shortcut("b"), tone: .secondary, toggle)
    }
}
