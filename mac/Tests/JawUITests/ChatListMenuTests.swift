import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// A right click on a chat in the list asks the host for that chat's menu, made as it opens.

@MainActor
final class ListHost: ChatListHost {
    var asked: [String] = []

    func chatList(_: ChatListController, open _: ChatRow) {}

    func chatList(_: ChatListController, move _: Place) {}

    func chatList(_: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem] {
        asked.append(chat.id)
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
