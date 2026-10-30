import AppKit
@testable import JawKit
@testable import JawUI
import Testing

/// A chat's folder items say where it's filed and do what they say: Add to Folder, ticked where it
/// sits directly; Move to and Remove from only in one of your folders that holds it, greyed while it's
/// there only through a folder in it; the same in each of its menus; none before jawd serves folders.

/// A menu made from entries and the commands bound to them, as the app makes one as it opens.
@MainActor
func folderMenu(_ entries: [MenuEntry], _ commands: [Command]) -> NSMenu {
    Menus.menu(entries) { id in commands.first { $0.id == id } }
}

/// What a menu shows: each item's title, after a "· " for each level it's indented and a "✓ " when
/// it's ticked, in brackets when it's greyed, and a submenu's items after it.
@MainActor
func shows(_ menu: NSMenu) -> [String] {
    menu.items.filter { !$0.isHidden }.map { item in
        if item.isSeparatorItem { return "—" }
        let title = String(repeating: "· ", count: item.indentationLevel) + (item.state == .on ? "✓ " : "") + item.title
        let shown = item.isEnabled ? title : "[\(title)]"
        guard let sub = item.submenu else { return shown }
        return "\(shown) ▸ " + shows(sub).joined(separator: ", ")
    }
}

/// Runs a menu item, as a click does.
@MainActor
func run(_ item: NSMenuItem?) {
    guard let item else { return }
    (item.target as? CommandTarget)?.runCommand(item)
}

/// An entry's title, or its command's id when it has none.
func head(_ entry: MenuEntry) -> String {
    switch entry {
    case let .command(id, title): title ?? id
    case let .submenu(title, _): title
    case .separator: "—"
    }
}

@MainActor
final class Acts {
    var done: [FolderAct] = []
}

func ana(in folders: [String]) -> ChatRow {
    var chat = Samples.direct
    chat.folders = folders
    return chat
}

@MainActor
@Test func `Add to Folder lists your folders as a tree, ticked where the chat sits directly, and a click files it or takes it out`() {
    let acts = Acts()
    let item = Filed(ana(in: ["folder:1"]))
    let menu = folderMenu(FolderMenu.entries(item, tree: Samples.tree, list: "all"),
                          FolderMenu.commands(item, tree: Samples.tree, list: "all") { acts.done.append($0) })
    #expect(shows(menu) == ["Add to Folder ▸ ✓ Family, · Cousins, Climbing, Friends, Work, · Clients"])
    let add = menu.items.first?.submenu
    run(add?.items[0])
    run(add?.items[2])
    #expect(acts.done == [.remove(item: Samples.direct.id, from: "folder:1"), .add(item: Samples.direct.id, to: "folder:3")])
}

@MainActor
@Test func `while the list shows one of your folders, Move to files the chat in another and Remove from takes it out of this one`() {
    let acts = Acts()
    let item = Filed(ana(in: ["folder:1"]))
    let menu = folderMenu(FolderMenu.entries(item, tree: Samples.tree, list: "folder:1"),
                          FolderMenu.commands(item, tree: Samples.tree, list: "folder:1") { acts.done.append($0) })
    #expect(Array(shows(menu).dropFirst()) == ["Move to ▸ [Family], · Cousins, Climbing, Friends, Work, · Clients", "Remove from Family"])
    run(menu.items[1].submenu?.items[4])
    run(menu.items[2])
    #expect(acts.done == [.move(item: Samples.direct.id, from: "folder:1", to: "folder:5"), .remove(item: Samples.direct.id, from: "folder:1")])
}

@MainActor
@Test func `Move to and Remove from are greyed while the chat is in the list's folder only through a folder in it`() {
    let item = Filed(ana(in: ["folder:2"]))
    let menu = folderMenu(FolderMenu.entries(item, tree: Samples.tree, list: "folder:1"),
                          FolderMenu.commands(item, tree: Samples.tree, list: "folder:1") { _ in })
    #expect(Array(shows(menu).dropFirst())
        == ["[Move to] ▸ [Family], [· Cousins], [Climbing], [Friends], [Work], [· Clients]", "[Remove from Family]"])
}

@Test func `in All, a built-in or a smart folder, or one of yours that doesn't hold the chat, there's no Move to or Remove from`() {
    let item = Filed(ana(in: ["folder:1"]))
    for list in ["all", "unread", "smart:2", "folder:3"] {
        #expect(FolderMenu.entries(item, tree: Samples.tree, list: list).map(head) == ["Add to Folder"], "\(list)")
    }
}

@Test func `before jawd serves folders the menus have no folder items`() {
    let chat = ana(in: ["folder:1"])
    #expect(FolderMenu.entries(Filed(chat), tree: FolderTree(), list: "all").isEmpty)
    #expect(ChatMenu.list(chat, tree: FolderTree(), in: "all") == ChatMenu.list(chat))
    #expect(ChatMenu.pane(chat, tree: FolderTree(), in: "all") == ChatMenu.pane(chat))
}

@Test func `a community's row has the folder items alone, and a chat's menus have them after its actions`() {
    let community = ChatRow(id: "c@g.us", v: 1, sort: "1", kind: .community, name: "Bairro Alto", folders: ["folder:1"])
    #expect(ChatMenu.list(community, tree: Samples.tree, in: "folder:1").map(head) == ["Add to Folder", "Move to", "unfile"])
    let chat = ana(in: ["folder:1"])
    #expect(ChatMenu.list(chat, tree: Samples.tree, in: "folder:1").map(head)
        == ["chat.open.pane", "—", "Unpin", "Mute", "Archive", "Mark as Unread", "—", "Add to Folder", "Move to", "unfile"])
    #expect(ChatMenu.pane(chat, tree: Samples.tree, in: "all").map(head)
        == ["split.across", "split.down", "pane.close", "Swap", "—", "Unpin", "Mute", "Archive", "Mark as Unread", "—", "Add to Folder", "—",
            "search.chat", "chat.library"])
}

@Test func `a chat's folder items are one list, which the chat list's menu, the pane's and the menu bar's Chat menu all have`() {
    let chat = ana(in: ["folder:1"])
    let items = ChatMenu.folderItems(chat, tree: Samples.tree, in: "folder:1")
    #expect(items.map(head) == ["Add to Folder", "Move to", "unfile"])
    #expect(Array(ChatMenu.list(chat, tree: Samples.tree, in: "folder:1").suffix(items.count)) == items)
    #expect(Array(ChatMenu.pane(chat, tree: Samples.tree, in: "folder:1").dropLast(3).suffix(items.count)) == items,
            "before Search This Chat and This Chat's Library")
    #expect(ChatMenu.folderItems(chat, tree: Samples.tree, in: "all").map(head) == ["Add to Folder"])
}

@Test func `what's said once jawd has filed a chat, taken it out or moved it, and when a move leaves it in both folders`() {
    let tree = Samples.tree
    #expect(FolderMenu.said(.add(item: "x", to: "folder:1"), name: "Ana Ribeiro", tree: tree) == "Ana Ribeiro filed in Family")
    #expect(FolderMenu.said(.remove(item: "x", from: "folder:2"), name: "Ana Ribeiro", tree: tree) == "Ana Ribeiro taken out of Cousins")
    #expect(FolderMenu.said(.move(item: "x", from: "folder:1", to: "folder:5"), name: "Ana Ribeiro", tree: tree)
        == "Ana Ribeiro moved from Family to Work")
    #expect(FolderMenu.inBoth("Ana Ribeiro", from: "folder:1", to: "folder:5", tree: tree) == "Ana Ribeiro is now in both Work and Family")
}

@Test func `a filing jawd has done shows in a pane's row for a chat the list doesn't hold, which jawd's pushes don't reach`() {
    let chat = ana(in: ["folder:1"])
    #expect(FolderMenu.filed(.added(item: chat.id, to: "folder:3"), chat)?.folders == ["folder:1", "folder:3"])
    #expect(FolderMenu.filed(.removed(item: chat.id, from: "folder:1"), chat)?.folders == [])
    #expect(FolderMenu.filed(.moved(item: chat.id, from: "folder:1", to: "folder:5"), chat)?.folders == ["folder:5"])
    #expect(FolderMenu.filed(.added(item: Samples.group.id, to: "folder:3"), chat) == nil, "another chat's")
    #expect(FolderMenu.filed(.added(item: "person:1", to: "folder:3"), chat) == nil, "someone's, unless this is their chat")
    #expect(FolderMenu.filed(.added(item: "person:1", to: "folder:3"), chat, chat: chat.id)?.folders == ["folder:1", "folder:3"],
            "filing someone files their one-to-one chat")
}
