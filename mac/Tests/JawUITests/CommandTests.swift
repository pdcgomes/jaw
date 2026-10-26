import AppKit
@testable import JawUI
import Testing

@MainActor
final class Ran {
    var n = 0
    var flag = true
}

/// Commands are written once and every menu, the palette and the keys sheet are made from them:
/// each key has a menu item showing it, a Shift letter is given as its capital, and a command
/// that can't run says so in every place at once.

@MainActor
@Test func `a command's menu item carries its key and runs it`() throws {
    let ran = Ran()
    let split = Command("split.down", "Split Down", phrase: "Split this pane down", key: Shortcut("d", [.command, .shift]), place: .panes)
    { ran.n += 1 }
    let item = split.menuItem()
    #expect(item.keyEquivalent == "D" && item.keyEquivalentModifierMask == [.command, .shift])
    #expect(item.title == "Split Down")
    #expect(item.action == NSSelectorFromString("runCommand:"), "not NSObject's perform(_:), which is performSelector:")
    let target = try #require(item.target as? CommandTarget)
    target.runCommand(item)
    #expect(ran.n == 1)
    #expect(split.paletteAction.name == "Split this pane down" && split.paletteAction.key == "⇧⌘D")
}

@MainActor
@Test func `a command that can't run is greyed in the menu and isn't run`() throws {
    let ran = Ran()
    let close = Command("pane.close", "Close Pane", key: Shortcut("w"), place: .file, enabled: { false }) { ran.n += 1 }
    let item = close.menuItem()
    let target = try #require(item.target as? CommandTarget)
    #expect(!target.validateMenuItem(item))
    target.runCommand(item)
    #expect(ran.n == 0)
}

@MainActor
@Test func `a setting's item is checked as it stands`() throws {
    let lines = Ran()
    let look = Command("look.lines", "Lines", place: .view, submenu: "Message Style", on: { lines.flag }) {}
    let item = look.menuItem()
    let target = try #require(item.target as? CommandTarget)
    _ = target.validateMenuItem(item)
    #expect(item.state == .on)
    lines.flag = false
    _ = target.validateMenuItem(item)
    #expect(item.state == .off)
}

@MainActor
@Test func `menus are made by place, in groups, with submenus where they first come`() throws {
    let commands = [
        Command("a", "Bubbles", place: .view, submenu: "Message Style") {},
        Command("b", "Lines", place: .view, submenu: "Message Style") {},
        Command("c", "Bigger", key: Shortcut("+"), place: .view, group: 1) {},
        Command("d", "Smaller", key: Shortcut("-"), place: .view, group: 1) {},
        Command("e", "The Keys", key: Shortcut("/"), place: .help) {},
        Command("f", "Palette", key: Shortcut("k"), also: [Shortcut("k", .control)], place: .find) {},
    ]
    let bar = Menus.bar(commands)
    #expect(bar.items.map(\.title) == ["Find", "View", "Help"], "in the menu bar's order")
    let view = try #require(bar.item(withTitle: "View")?.submenu)
    #expect(view.items.map { $0.isSeparatorItem ? "—" : $0.title } == ["Message Style", "—", "Bigger", "Smaller"])
    #expect(view.items.first?.submenu?.items.map(\.title) == ["Bubbles", "Lines"])
    let find = try #require(bar.item(withTitle: "Find")?.submenu)
    #expect(find.items.count == 2 && find.items[1].isHidden && find.items[1].keyEquivalentModifierMask == .control,
            "⌃K answers from a hidden item")
    #expect(Menus.keys(commands).map(\.0) == ["Find", "View", "Help"])
    #expect(Menus.keys(commands).first?.1.first?.0 == "⌘K or ⌃K")
}

@MainActor
@Test func `two commands can't share a key, and a key needs a menu`() {
    let problems = Menus.problems([
        Command("a", "One", key: Shortcut("d"), place: .panes) {},
        Command("b", "Two", key: Shortcut("d"), place: .panes) {},
        Command("c", "Three", key: Shortcut("x", [.command, .option]), place: nil) {},
    ])
    #expect(problems == ["⌘D is both a and b", "c has ⌥⌘X but no menu"])
}

@MainActor
@Test func `a message's keys are shown but not bound, so the box keeps its letters`() {
    let reply = Command("message.reply", "Reply", phrase: "Reply to this message", hint: "r", place: .message) {}
    #expect(reply.menuItem().keyEquivalent.isEmpty)
    #expect(reply.keyText == "r" && reply.paletteAction.key == "r")
}
