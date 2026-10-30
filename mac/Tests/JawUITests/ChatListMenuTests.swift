import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// A right click on a chat in the list asks the host for that chat's menu, made as it opens, and so
/// does VoiceOver's Show Menu on its row, which makes no click for the table's menu to be filled for.

@MainActor
final class ListHost: ChatListHost {
    var asked: [String] = []

    func chatList(_: ChatListController, open _: ChatRow) {}

    func chatList(_: ChatListController, move _: Place) {}

    /// None for a community's row, as the app's host has none before jawd serves folders; after, it has the folder items alone.
    func chatList(_: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem] {
        asked.append(chat.id)
        guard chat.hasChatActions else { return [] }
        return [NSMenuItem(title: chat.pinned ? "Unpin" : "Pin", action: nil, keyEquivalent: "")]
    }
}

@MainActor
@Test func `a right click on a chat asks for that chat's menu, and off the rows for none`() throws {
    let list = ChatListController(folder: "all", roles: Catalog.jaw.roles(dark: true))
    let host = ListHost()
    list.host = host
    _ = list.view
    list.shown = Samples.chats
    let menu = try #require(list.table.menu)
    #expect(menu.delegate === list, "filled for the row right-clicked as it opens")
    #expect(list.menuItems(forRow: 1).map(\.title) == [Samples.chats[1].pinned ? "Unpin" : "Pin"])
    #expect(host.asked == [Samples.chats[1].id])
    #expect(list.menuItems(forRow: -1).isEmpty && list.menuItems(forRow: Samples.chats.count).isEmpty)
    #expect(host.asked.count == 1, "nothing asked off the rows")
}

@MainActor
@Test func `VoiceOver's Show Menu on a chat's row opens that chat's menu under it, and on a community's says there's none`() throws {
    let list = ChatListController(folder: "all", roles: Catalog.jaw.roles(dark: true))
    let host = ListHost()
    list.host = host
    _ = list.view
    list.shown = Samples.chats
    list.table.reloadData()
    let row = try #require(list.table.view(atColumn: 0, row: 1, makeIfNecessary: true) as? ChatCell)
    var popped: [(NSMenu, NSPoint)] = []
    row.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    #expect(row.accessibilityPerformShowMenu())
    #expect(popped.first?.0.items.map(\.title) == [Samples.chats[1].pinned ? "Unpin" : "Pin"], "that chat's menu")
    #expect(popped.first?.1 == NSPoint(x: row.bounds.minX, y: row.bounds.maxY), "under its row")
    #expect(host.asked == [Samples.chats[1].id])

    let at = try #require(Samples.chats.firstIndex { $0.kind == .community })
    let community = try #require(list.table.view(atColumn: 0, row: at, makeIfNecessary: true) as? ChatCell)
    community.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    #expect(!community.accessibilityPerformShowMenu(), "a community's row has no menu")
    #expect(host.asked == [Samples.chats[1].id, Samples.chats[at].id])
    #expect(popped.count == 1)
}
