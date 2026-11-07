import AppKit
@testable import JawHarness
@testable import JawUI
import SwiftUI
import Testing

/// Settings' scenes in the harness's own window: its scene list keeps the keys as the stage shows the
/// Appearance pane, whose gallery takes a window's keys as the pane opens in Settings.

/// The harness's window as `HarnessApp` opens it, offscreen: the scene list, the conditions and the
/// stage.
@MainActor
func harnessWindow() -> NSWindow {
    let window = NSWindow(contentRect: NSRect(x: -30000, y: -30000, width: 1400, height: 900), styleMask: [.borderless],
                          backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentViewController = NSHostingController(rootView: HarnessRootView())
    window.orderFront(nil)
    return window
}

/// ↓ pressed in `window` as the keyboard sends it, with its function key's character.
@MainActor
func pressDown(in window: NSWindow) async throws {
    let typed = try #require(UnicodeScalar(NSDownArrowFunctionKey).map { String($0) })
    let key = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [.numericPad, .function], timestamp: 0,
                                            windowNumber: window.windowNumber, context: nil, characters: typed,
                                            charactersIgnoringModifiers: typed, isARepeat: false, keyCode: 125))
    window.sendEvent(key)
    try await Task.sleep(for: .milliseconds(100))
}

/// Every view in `view`, itself first.
@MainActor
func views(_ view: NSView) -> [NSView] {
    [view] + view.subviews.flatMap { views($0) }
}

/// Waits until `condition` holds or a few seconds pass; says whether it held.
@MainActor
func eventually(seconds: Double = 5, _ condition: () -> Bool) async -> Bool {
    let until = Date().addingTimeInterval(seconds)
    while Date() < until {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(5))
    }
    return condition()
}

@MainActor @Test(.timeLimit(.minutes(1))) func `↓ down the scene list goes on past Settings' panes, whose gallery leaves the list the keys`() async throws {
    let window = harnessWindow()
    defer { window.close() }
    let root = try #require(window.contentView)
    #expect(await eventually { views(root).contains { $0 is NSTableView } }, "the scene list is drawn")
    let list = try #require(views(root).lazy.compactMap { $0 as? NSTableView }.first)
    func gallery() -> Bool {
        views(root).contains { $0 is ArrowCatcher }
    }
    for _ in 0 ..< 3 * HarnessCatalog.shared.scenes.count where !gallery() {
        window.makeFirstResponder(list)
        try await pressDown(in: window)
    }
    try #require(gallery(), "↓ from the first scene reaches Settings' panes, the one scene with the Appearance pane")
    try await Task.sleep(for: .milliseconds(500))
    #expect(window.firstResponder === list, "the scene list keeps the keys as the pane opens on the stage")
    try await pressDown(in: window)
    #expect(await eventually { !gallery() }, "so the next ↓ goes on to the theme gallery, rather than choosing a theme")
}
