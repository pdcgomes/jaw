import AppKit
@testable import JawThemes
@testable import JawUI
import Testing

/// The component library: one type scale, colours by tone, buttons that say their keys, shortcuts
/// that menus match, and motion that stops under Reduce Motion.

@Test func `the type scale is the one every surface draws with`() {
    let sizes = TextStyle.allCases.map { "\($0):\(Int($0.size))\($0.weight == .semibold ? "b" : "")\($0.monospaced ? "m" : "")" }
    #expect(sizes == ["title:17b", "heading:15b", "body:13", "callout:12", "caption:11", "micro:10", "mono:11m", "key:11bm", "display:34bm", "emoji:22"])
    #expect(TextStyle.body.nsFont.pointSize == 13)
    #expect(TextStyle.mono.nsFont.fontDescriptor.symbolicTraits.contains(.monoSpace))
}

@Test func `a tone is a role of the theme`() {
    let roles = Catalog.jaw.dark
    #expect(Tone.secondary.color(roles) == roles.secondary)
    #expect(Tone.warn.color(roles) == roles.failed)
    #expect(Tone.accent.color(roles) == roles.accentText, "text in the accent takes its held colour")
}

@Test func `a shortcut is shown as Apple's menus show it and given to a menu with its capital`() {
    let down = Shortcut("d", [.command, .shift])
    #expect(down.display == "⇧⌘D")
    #expect(down.menuKey == "D", "⌘⇧D reports a capital D")
    #expect(Shortcut("d").menuKey == "d" && Shortcut("d").display == "⌘D")
    #expect(Shortcut("1", [.control, .command]).display == "⌃⌘1")
    #expect(Shortcut("←", [.command, .option]).display == "⌥⌘←")
    #expect(Shortcut("←", [.command, .option]).menuKey == String(UnicodeScalar(NSLeftArrowFunctionKey)!))
    #expect(Shortcut("↩", [.command, .shift]).menuKey == "\r")
}

@MainActor
@Test func `a button's tooltip teaches its key`() {
    let button = IconButton(symbol: "xmark", label: "Close Pane", shortcut: Shortcut("w")) {}
    #expect(button.toolTip == "Close Pane (⌘W)")
    #expect(button.accessibilityLabel() == "Close Pane")
    #expect(IconButton(symbol: "xmark", label: "Drop it") {}.toolTip == "Drop it")
}

@MainActor
@Test func `VoiceOver's press does what a click does: a button's menu opens where a click opens it`() {
    let ran = Ran()
    let close = IconButton(symbol: "xmark", label: "Close Pane") { ran.n += 1 }
    #expect(close.accessibilityPerformPress())
    #expect(ran.n == 1)

    let menu = NSMenu()
    menu.addItem(withTitle: "Photos & Videos…", action: nil, keyEquivalent: "")
    let attach = IconButton(symbol: "plus", label: "Attach") { ran.n += 1 }
    attach.menuToOpen = { menu }
    var popped: [(NSMenu, NSPoint)] = []
    attach.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    #expect(attach.accessibilityPerformPress())
    #expect(popped.count == 1 && popped.first?.0 === menu, "the menu a click opens")
    #expect(popped.first?.1 == attach.menuPoint(menu), "where a click opens it")
    #expect(ran.n == 1, "rather than pressed")

    var from: IconButton?
    let layout = IconButton(symbol: "rectangle.3.group", label: "Layouts") { ran.n += 1 }
    layout.pressedFrom = { from = $0 }
    #expect(layout.accessibilityPerformPress())
    #expect(from === layout && ran.n == 1, "given to pressedFrom, as a click is")
}

@MainActor
@Test func `nothing moves under Reduce Motion`() {
    let before = Motion.reduced
    defer { Motion.reduced = before }
    Motion.reduced = { true }
    var ran = false
    Motion.run { ran = true }
    #expect(ran, "the change is made at once")
}

@MainActor
@Test func `the emoji picker finds by name, moves on its grid, keeps a tone and puts recents first`() throws {
    let defaults = try #require(UserDefaults(suiteName: "jaw.tests.emoji"))
    defaults.removePersistentDomain(forName: "jaw.tests.emoji")
    let picker = EmojiPickerModel(defaults: defaults)
    picker.query = "thumbs"
    #expect(picker.shown.contains("👍"))
    picker.query = ""
    picker.move(-1, -1)
    #expect(picker.index == 0, "the grid's first")
    picker.move(1, 1)
    #expect(picker.index == EmojiPickerModel.columns + 1)
    picker.tone = 3
    #expect(picker.toned("👍") == "👍🏽" && picker.toned("🎉") == "🎉", "a tone only where it's taken")
    let thumbs = picker.shown.firstIndex(of: "👍")!
    #expect(picker.pick(thumbs) == "👍🏽")
    #expect(picker.shown.first == "👍" && picker.recents.first == "👍")
    #expect(EmojiPickerModel(defaults: defaults).tone == 3 && EmojiPickerModel(defaults: defaults).recents == ["👍"], "both kept")
}
