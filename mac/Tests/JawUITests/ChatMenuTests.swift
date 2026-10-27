@testable import JawKit
@testable import JawUI
import Testing

/// A chat's menus say what each item will do as the chat stands, and leave out what doesn't apply.

func titles(_ entries: [MenuEntry]) -> [String] {
    entries.map { entry in
        switch entry {
        case let .command(id, title): title ?? id
        case let .submenu(title, inner): "\(title) ▸ \(titles(inner).joined(separator: ", "))"
        case .separator: "—"
        }
    }
}

@Test func `a chat's menu says what each item will do`() {
    var chat = ChatRow(id: "ana", v: 1, sort: "1", name: "Ana")
    #expect(titles(ChatMenu.list(chat)) == [
        "chat.open.pane", "—", "Pin", "Mute ▸ mute.8h, mute.week, mute.always", "Archive", "Mark as Unread",
    ])
    chat.pinned = true
    chat.mutedUntil = -1
    chat.archived = true
    chat.unread = 2
    #expect(titles(ChatMenu.list(chat)) == ["chat.open.pane", "—", "Unpin", "unmute", "Unarchive", "Mark as Read"])
    chat.unread = 0
    chat.markedUnread = true
    #expect(titles(ChatMenu.actions(chat)).last == "Mark as Read", "a chat marked unread is read by it")
}

@Test func `a community's row has none of a chat's actions, and its groups have them`() {
    let community = ChatRow(id: "c", v: 1, sort: "1", kind: .community, name: "Bairro Alto")
    #expect(ChatMenu.list(community).isEmpty)
    let group = ChatRow(id: "g", v: 1, sort: "1", kind: .group, name: "Bairro Alto: Announcements", community: "c")
    #expect(titles(ChatMenu.list(group)).contains("Pin"))
}

@Test func `the pane's menu has its splits and Swap, then its chat's actions`() {
    let chat = ChatRow(id: "ana", v: 1, sort: "1", name: "Ana")
    #expect(titles(ChatMenu.pane(chat)) == [
        "split.across", "split.down", "pane.close", "Swap ▸ Left, Right, Up, Down", "—",
        "Pin", "Mute ▸ mute.8h, mute.week, mute.always", "Archive", "Mark as Unread", "—", "search.chat",
    ])
    #expect(titles(ChatMenu.splits) == ["split.across", "split.down", "pane.close"], "a message's menu's end, an empty pane's menu")
}

@Test func `a mute's end is counted from when it's chosen, and always is -1`() {
    let now: Int64 = 1_792_159_320_000
    #expect(MuteLength.eightHours.until(now: now) == now + 28_800_000)
    #expect(MuteLength.week.until(now: now) == now + 604_800_000)
    #expect(MuteLength.always.until(now: now) == -1)
    #expect(MuteLength.allCases.map(\.title) == ["For 8 Hours", "For a Week", "Always"])
}
