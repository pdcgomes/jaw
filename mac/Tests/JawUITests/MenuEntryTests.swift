import AppKit
@testable import JawUI
import Testing

/// Right-click menus are made from entries worked out from rows: a command under a title saying
/// what it will do, submenus, separators only between items, and what can't run greyed.

@MainActor
@Test func `a right-click menu takes each command's title or the entry's, enabled and ticked as it stands`() {
    let pinned = Ran()
    let commands = [
        Command("pin", "Pinned", place: .chat, on: { pinned.flag }) {},
        Command("pane.close", "Close Pane", key: Shortcut("w"), place: .file, enabled: { false }) {},
    ]
    let menu = Menus.menu([.command("pin", title: "Unpin"), .separator, .command("pane.close")]) { id in
        commands.first { $0.id == id }
    }
    #expect(menu.items.map { $0.isSeparatorItem ? "—" : $0.title } == ["Unpin", "—", "Close Pane"])
    #expect(menu.items[0].state == .on && menu.items[0].isEnabled)
    #expect(!menu.items[2].isEnabled, "greyed, as it can't run")
    #expect(menu.items[2].keyEquivalent == "w", "its key shown")
    #expect(!menu.autoenablesItems, "made as it opens, so as things stand then")
}

@MainActor
@Test func `an entry with no command, an empty submenu and a stray separator are left out`() {
    let commands = [Command("swap.left", "Swap Left", place: .panes, enabled: { false }) {}]
    let entries: [MenuEntry] = [
        .separator, .command("nowhere"), .submenu("Empty", [.command("nowhere")]), .separator,
        .submenu("Swap", [.command("swap.left", title: "Left")]), .separator,
    ]
    let menu = Menus.menu(entries) { id in commands.first { $0.id == id } }
    #expect(menu.items.map(\.title) == ["Swap"])
    #expect(menu.items[0].submenu?.items.map(\.title) == ["Left"])
    #expect(!menu.items[0].isEnabled, "a submenu whose items can't run is greyed")
}
