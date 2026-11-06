import AppKit
import SwiftUI

/// Settings' panes as the Mac's own Settings windows show them: each an icon over its name in the
/// window's toolbar, its SwiftUI form hosted at its pane's size, and the window taking that size as
/// its tab is chosen, keeping its top edge where it is. Every change in a pane is kept as it's made,
/// so there's nothing to confirm.
@MainActor
public final class SettingsTabs: NSTabViewController {
    /// Told each pane chosen, so the next Settings opens on it.
    public var chose: (SettingsPane) -> Void = { _ in }

    public init(preferences: Preferences, showing pane: SettingsPane) {
        super.init(nibName: nil, bundle: nil)
        tabStyle = .toolbar
        transitionOptions = []
        for each in SettingsPane.allCases {
            let host = NSHostingController(rootView: Self.form(each, preferences))
            host.sizingOptions = []
            host.title = each.title
            let item = NSTabViewItem(viewController: host)
            item.identifier = each.rawValue
            item.label = each.title
            item.image = NSImage(systemSymbolName: each.symbol, accessibilityDescription: each.title)
            addTabViewItem(item)
        }
        show(pane)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// The pane on show.
    public var pane: SettingsPane {
        SettingsPane.allCases[max(selectedTabViewItemIndex, 0)]
    }

    /// Shows `pane`, as a click on its tab does.
    public func show(_ pane: SettingsPane) {
        selectedTabViewItemIndex = SettingsPane.allCases.firstIndex(of: pane) ?? 0
    }

    /// Titles and sizes the window for the pane chosen. The first item's choosing comes while `init`
    /// adds it, before the view is loaded, so it doesn't load the view to ask for its window.
    override public func tabView(_ tabView: NSTabView, didSelect tabViewItem: NSTabViewItem?) {
        super.tabView(tabView, didSelect: tabViewItem)
        guard let pane = (tabViewItem?.identifier as? String).flatMap(SettingsPane.init(rawValue:)) else { return }
        let window = isViewLoaded ? view.window : nil
        window?.title = pane.title
        fit(pane, animated: window?.isVisible == true)
        chose(pane)
    }

    /// Sizes the window so its content is `pane`'s size, keeping the window's top edge where it is,
    /// whatever height its title bar and toolbar have. The panes give no preferred size of their own,
    /// so this is the one place the window is sized.
    public func fit(_ pane: SettingsPane, animated: Bool) {
        guard isViewLoaded, let window = view.window, let content = window.contentView else { return }
        var frame = window.frame
        let taller = pane.size.height - content.frame.height
        frame.origin.y -= taller
        frame.size.height += taller
        frame.size.width += pane.size.width - content.frame.width
        window.setFrame(frame, display: true, animate: animated && !Motion.reduced())
    }

    static func form(_ pane: SettingsPane, _ preferences: Preferences) -> AnyView {
        switch pane {
        case .appearance: AnyView(AppearancePane(preferences: preferences))
        case .messages: AnyView(MessagesPane(preferences: preferences))
        case .panes: AnyView(PanesPane(preferences: preferences))
        }
    }
}

/// Settings' window, which ⌘W closes, as it closes a window elsewhere. jaw's ⌘W is the menu bar's
/// Close Pane, the main window's, and AppKit offers a key equivalent to the key window before the
/// menu bar, so this takes it first, whatever Caps Lock says. On a layout with no W, as a Cyrillic
/// one, the key where a US keyboard has W is taken, as the menu bar takes it there.
public final class SettingsWindow: NSWindow {
    override public func performKeyEquivalent(with event: NSEvent) -> Bool {
        let typed = event.charactersIgnoringModifiers?.lowercased() ?? ""
        let w = typed == "w" || (!typed.allSatisfy(\.isASCII) && event.keyCode == 13)
        if event.modifierFlags.intersection(MenuBar.modifiers) == .command, w {
            performClose(nil)
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    /// Whether `event` is a key pressed in the window while it's open. On a layout whose W key types
    /// something else in ASCII, as Hebrew's ', the window passes ⌘W on and the menu bar matches it as
    /// ⌘W through an ASCII-capable layout, so Close Pane asks this of the key it was chosen by and
    /// closes Settings in place of a pane.
    public func pressed(_ event: NSEvent?) -> Bool {
        guard let event, isVisible else { return false }
        return event.type == .keyDown && event.windowNumber == windowNumber
    }
}
