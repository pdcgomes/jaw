import AppKit
import JawThemes
@testable import JawUI
import Testing

/// Settings as a window of its own: its panes the toolbar's icon tabs, in order, each its name over
/// its symbol, the window taking each pane's size as its tab is chosen, keeping its top edge, and ⌘W
/// closing it.

/// A `Preferences` on defaults of the test's own, emptied first, so it starts from the app's defaults.
@MainActor
func scratchPreferences(_ name: String) throws -> Preferences {
    let suite = "jaw.tests.\(name)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    return Preferences(defaults: defaults)
}

@MainActor @Test func `Settings' panes are the toolbar's icon tabs, each its name over its symbol, in the toolbar's order`() throws {
    let tabs = SettingsTabs(preferences: try scratchPreferences("settings-tabs"), showing: .messages)
    #expect(tabs.tabStyle == .toolbar)
    #expect(tabs.tabViewItems.map(\.label) == ["Appearance", "Messages", "Panes"])
    #expect(tabs.tabViewItems.map { $0.identifier as? String } == ["appearance", "messages", "panes"])
    #expect(tabs.tabViewItems.map { $0.image?.accessibilityDescription } == ["Appearance", "Messages", "Panes"],
            "each its SF Symbol, named for VoiceOver")
    #expect(SettingsPane.allCases.map(\.symbol) == ["paintpalette", "text.bubble", "rectangle.split.2x1"])
    #expect(SettingsPane.allCases.allSatisfy { $0.size.width == 500 }, "500 points wide, as Redlamp's")
    #expect(tabs.pane == .messages, "it opens on the pane it's asked to")
}

@MainActor @Test func `choosing a pane sizes the window to it, keeping its top edge, titles the window and says which`() throws {
    let tabs = SettingsTabs(preferences: try scratchPreferences("settings-fit"), showing: .appearance)
    let window = NSWindow(contentViewController: tabs)
    window.styleMask = [.titled, .closable]
    window.isReleasedWhenClosed = false
    defer { window.close() }
    tabs.fit(.appearance, animated: false)
    #expect(window.contentView?.frame.size == SettingsPane.appearance.size)
    let top = window.frame.maxY
    var chosen: [SettingsPane] = []
    tabs.chose = { chosen.append($0) }
    for pane in [SettingsPane.panes, .messages, .appearance] {
        tabs.show(pane)
        #expect(window.contentView?.frame.size == pane.size, "\(pane)")
        #expect(window.frame.maxY == top, "\(pane): its top edge stays where it was")
        #expect(window.title == pane.title, "\(pane)")
    }
    #expect(chosen == [.panes, .messages, .appearance] && tabs.pane == .appearance)
}

@MainActor @Test func `⌘W closes Settings' window before the menu bar's Close Pane, the main window's, can take it`() throws {
    let window = SettingsWindow(contentViewController: SettingsTabs(preferences: try scratchPreferences("settings-close"), showing: .panes))
    window.styleMask = [.titled, .closable]
    window.isReleasedWhenClosed = false
    window.setFrameOrigin(NSPoint(x: -30000, y: -30000))
    window.orderFront(nil)
    defer { window.close() }
    func key(_ characters: String, _ modifiers: NSEvent.ModifierFlags) throws -> NSEvent {
        try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0,
                                      windowNumber: window.windowNumber, context: nil, characters: characters,
                                      charactersIgnoringModifiers: characters, isARepeat: false, keyCode: 13))
    }
    #expect(!window.performKeyEquivalent(with: try key("w", [.command, .shift])) && window.isVisible, "⇧⌘W is the menu's Close Window")
    #expect(!window.performKeyEquivalent(with: try key("z", .command)) && window.isVisible,
            "⌘Z stays undo on a layout whose Z is where a US keyboard has W")
    #expect(window.performKeyEquivalent(with: try key("w", .command)) && !window.isVisible)
    window.orderFront(nil)
    #expect(window.performKeyEquivalent(with: try key("ц", .command)) && !window.isVisible,
            "on a layout with no W, the key where a US keyboard has it, as the menu bar matches ⌘W")
}

@MainActor @Test func `⌘W that Settings' window passes on, as on a Hebrew layout, reaches the menu bar's Close Pane, which closes Settings when the key was pressed in it`() throws {
    let window = SettingsWindow(contentViewController: SettingsTabs(preferences: try scratchPreferences("settings-close-pane"), showing: .panes))
    window.styleMask = [.titled, .closable]
    window.isReleasedWhenClosed = false
    window.setFrameOrigin(NSPoint(x: -30000, y: -30000))
    window.orderFront(nil)
    defer { window.close() }
    let other = NSWindow(contentRect: NSRect(x: -30000, y: -30000, width: 200, height: 200), styleMask: [.titled, .closable],
                         backing: .buffered, defer: false)
    other.isReleasedWhenClosed = false
    other.orderFront(nil)
    defer { other.close() }
    func key(_ characters: String, in pressed: NSWindow) throws -> NSEvent {
        try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command, timestamp: 0,
                                      windowNumber: pressed.windowNumber, context: nil, characters: characters,
                                      charactersIgnoringModifiers: characters, isARepeat: false, keyCode: 13))
    }
    let hebrew = try key("'", in: window)
    #expect(!window.performKeyEquivalent(with: hebrew) && window.isVisible,
            "Hebrew's W key types ', which the window passes on, and the menu bar matches as ⌘W")
    #expect(window.pressed(hebrew), "so Close Pane, chosen by it, closes Settings in place of a pane")
    #expect(!window.pressed(try key("w", in: other)), "⌘W pressed in another window, as the main window, is a pane's")
    let click = try #require(NSEvent.mouseEvent(with: .leftMouseUp, location: .zero, modifierFlags: [], timestamp: 0,
                                                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                                                pressure: 0))
    #expect(!window.pressed(click) && !window.pressed(nil), "and so is Close Pane chosen with the pointer, or run with no key")
    window.close()
    #expect(!window.pressed(hebrew), "or once Settings has closed")
}
