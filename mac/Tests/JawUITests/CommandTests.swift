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
@Test func `a command can say what it will do as its menu opens, and fill a submenu as it opens, greyed while it can't be used`() throws {
    let state = Ran(), apps = Ran()
    let act = Command("file.state", "Download", titled: { state.n == 0 ? "Download" : "Stop Downloading" }, place: .message,
                      enabled: { state.flag }) {}
    let openWith = Command("open.with", "Open With", place: .message, enabled: { apps.flag },
                           fill: { (0 ..< apps.n).map { NSMenuItem(title: "App \($0)", action: nil, keyEquivalent: "") } })
    let message = try #require(Menus.bar([act, openWith]).item(withTitle: "Message")?.submenu)
    message.update()
    #expect(message.items.map(\.title) == ["Download", "Open With"])
    state.n = 1
    message.update()
    #expect(message.items[0].title == "Stop Downloading", "its title as things stand when it opens")
    let holder = message.items[1]
    let sub = try #require(holder.submenu)
    apps.n = 2
    sub.delegate?.menuNeedsUpdate?(sub)
    #expect(sub.items.map(\.title) == ["App 0", "App 1"])
    apps.n = 1
    sub.delegate?.menuNeedsUpdate?(sub)
    #expect(sub.items.map(\.title) == ["App 0"], "made again each time it opens")
    #expect(holder.isEnabled)
    apps.flag = false
    message.update()
    #expect(!holder.isEnabled, "greyed while there's nothing to open")
    #expect(!openWith.inPalette)
    let own = act.menuItem(title: "Ask the Phone Again")
    _ = CommandTarget.shared.validateMenuItem(own)
    #expect(own.title == "Ask the Phone Again", "a title of its own stays")
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

/// Return pressed with `flags`, as AppKit hands a key to the menus.
func returnKey(_ flags: NSEvent.ModifierFlags) throws -> NSEvent {
    try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0, context: nil,
                                  characters: "\r", charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 36))
}

@MainActor
@Test func `⌘↩ isn't ⇧⌘↩: a key on Return answers only with its own modifiers`() throws {
    let ran = Ran()
    let focus = Command("focus.mode", "Focus on This Chat", key: Shortcut("↩", [.command, .shift]), place: .panes) { ran.n += 1 }
    let bar = Menus.bar([focus])
    #expect(try !bar.performKeyEquivalent(with: returnKey(.command)), "AppKit alone runs focus mode for ⌘↩")
    #expect(try !bar.performKeyEquivalent(with: returnKey([.command, .option])))
    #expect(ran.n == 0)
    #expect(try bar.performKeyEquivalent(with: returnKey([.command, .shift])))
    #expect(ran.n == 1)
}

@MainActor
@Test func `⌘↩ and ⇧⌘↩ each run their own command, whichever comes first, and a greyed one runs neither`() throws {
    for shiftedFirst in [false, true] {
        let plain = Ran(), shifted = Ran()
        var commands = [
            Command("plain", "Plain", key: Shortcut("↩"), place: .panes) { plain.n += 1 },
            Command("shifted", "Shifted", key: Shortcut("↩", [.command, .shift]), place: .panes, enabled: { shifted.flag }) { shifted.n += 1 },
        ]
        if shiftedFirst { commands.reverse() }
        let bar = Menus.bar(commands)
        #expect(try bar.performKeyEquivalent(with: returnKey(.command)))
        #expect(plain.n == 1 && shifted.n == 0, "AppKit alone runs the first for both; shifted first: \(shiftedFirst)")
        #expect(try bar.performKeyEquivalent(with: returnKey([.command, .shift])))
        #expect(plain.n == 1 && shifted.n == 1, "shifted first: \(shiftedFirst)")
        shifted.flag = false
        #expect(try !bar.performKeyEquivalent(with: returnKey([.command, .shift])))
        #expect(plain.n == 1 && shifted.n == 1, "shifted first: \(shiftedFirst)")
    }
}

@MainActor
@Test func `a message's keys are shown but not bound, so the box keeps its letters`() {
    let reply = Command("message.reply", "Reply", phrase: "Reply to this message", hint: "r", place: .message) {}
    #expect(reply.menuItem().keyEquivalent.isEmpty)
    #expect(reply.keyText == "r" && reply.paletteAction.key == "r")
}
