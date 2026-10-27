import AppKit
@testable import JawUI
import Testing

/// Right-click menus are made from entries worked out from rows: a command under a title saying
/// what it will do, submenus, separators only between items, and what can't run greyed.

@MainActor
@Test func `a right-click menu takes each command's title or the entry's, enabled as it stands`() throws {
    let commands = [
        Command("pin", "Pinned", place: .chat, on: { true }) {},
        Command("look.lines", "Lines", place: .view, on: { false }) {},
        Command("pane.close", "Close Pane", key: Shortcut("w"), place: .file, enabled: { false }) {},
    ]
    let entries: [MenuEntry] = [.command("pin", title: "Unpin"), .command("pin"), .command("look.lines"), .separator, .command("pane.close")]
    let menu = Menus.menu(entries) { id in commands.first { $0.id == id } }
    #expect(menu.items.map { $0.isSeparatorItem ? "—" : $0.title } == ["Unpin", "Pinned", "Lines", "—", "Close Pane"])
    #expect(menu.items[0].state == .off, "an item saying what it will do has no tick: never ✓ Unpin")
    #expect(menu.items[1].state == .on && menu.items[2].state == .off, "a setting under its own title is ticked as it stands")
    #expect(menu.items[0].isEnabled && !menu.items[4].isEnabled, "greyed, as it can't run")
    #expect(menu.items[4].keyEquivalent == "w", "its key shown")
    #expect(!menu.autoenablesItems, "made as it opens, so as things stand then")
    let target = try #require(menu.items[0].target as? CommandTarget)
    _ = target.validateMenuItem(menu.items[0])
    #expect(menu.items[0].state == .off, "and no tick put back when a menu that enables its items checks it")
}

@MainActor
@Test func `an entry with no command, an empty submenu and a stray separator are left out`() {
    let commands = [
        Command("swap.left", "Swap Left", place: .panes, enabled: { false }) {},
        Command("pin", "Pinned", place: .chat) {},
        Command("pane.close", "Close Pane", key: Shortcut("w"), place: .file) {},
    ]
    let find: (String) -> Command? = { id in commands.first { $0.id == id } }
    let greyed = Menus.menu([.separator, .command("nowhere"), .submenu("Empty", [.command("nowhere")]), .separator,
                             .submenu("Swap", [.command("swap.left", title: "Left")]), .separator], command: find)
    #expect(greyed.items.map(\.title) == ["Swap"])
    #expect(greyed.items[0].submenu?.items.map(\.title) == ["Left"])
    #expect(!greyed.items[0].isEnabled, "a submenu whose items can't run is greyed")
    #expect(greyed.items[0].submenu?.autoenablesItems == false)
    let between = Menus.menu([.command("pin"), .separator, .command("nowhere"), .separator, .command("pane.close")], command: find)
    #expect(between.items.map { $0.isSeparatorItem ? "—" : $0.title } == ["Pinned", "—", "Close Pane"], "one separator where two met")
    let open = Menus.menu([.submenu("Chat", [.command("pin")])], command: find)
    #expect(open.items.first?.isEnabled == true, "a submenu with an item that can run isn't greyed")
}
