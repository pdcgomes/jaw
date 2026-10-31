@testable import JawKit
@testable import JawUI
import Testing

/// The tree as jawd keeps it: the built-in folders, yours nested by parent and position, the smart
/// ones, the nine slots, and what's read from it.

@Test func `before jawd sends its tree there are All and Archived, and its first update replaces them`() {
    var tree = FolderTree()
    #expect(!tree.served && tree.builtIn.map(\.id) == ["all", "archived"] && tree.yours.isEmpty && tree.smart.isEmpty)
    tree.apply(Update(seq: 300, upsert: Samples.folders.filter { $0.id != "archived" }, favourites: Samples.favourites))
    #expect(tree.served && tree.builtIn.map(\.name) == ["All", "Unread", "Groups", "Communities"], "an Archived the update doesn't have goes")
}

@Test func `your folders nest by parent and sit by position, and the smart ones come by theirs`() {
    let tree = Samples.tree
    #expect(tree.children(of: nil).map(\.name) == ["Family", "Climbing", "Friends", "Work"])
    #expect(tree.children(of: "folder:1").map(\.name) == ["Cousins"])
    #expect(tree.yours.map { String(repeating: "·", count: $0.depth) + $0.row.name } == ["Family", "·Cousins", "Climbing", "Friends", "Work", "·Clients"])
    #expect(tree.smart.map(\.name) == ["Waiting on me", "Mentions me", "Muted, unread"])
    #expect(tree.order == ["all", "unread", "groups", "communities", "archived", "folder:1", "folder:2", "folder:3", "folder:4",
                           "folder:5", "folder:6", "smart:1", "smart:2", "smart:3"])
    #expect(tree.subtree("folder:5") == ["folder:5", "folder:6"] && tree.ancestors("folder:2") == ["folder:1"])
}

@Test func `a folder two levels down is in yours at its depth, in the top folder's subtree, and named by its path from the top`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:7", v: 61, parent: "folder:2", name: "Climbing", position: 0)]))
    #expect(tree.yours.map { String(repeating: "·", count: $0.depth) + $0.row.name }
        == ["Family", "·Cousins", "··Climbing", "Climbing", "Friends", "Work", "·Clients"])
    #expect(tree.subtree("folder:1") == ["folder:1", "folder:2", "folder:7"])
    #expect(tree.ancestors("folder:7") == ["folder:2", "folder:1"], "nearest first")
    #expect(tree.label("folder:7") == "Family › Cousins › Climbing")
}

@Test func `a built-in folder a newer jawd adds shows among the built-in ones`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "starred", name: "Starred", position: 5)]))
    #expect(tree.builtIn.map(\.name) == ["All", "Unread", "Groups", "Communities", "Archived", "Starred"])
    #expect(tree.order == ["all", "unread", "groups", "communities", "archived", "starred", "folder:1", "folder:2", "folder:3",
                           "folder:4", "folder:5", "folder:6", "smart:1", "smart:2", "smart:3"])
}

@Test func `a row's badges: its digit from the slots, an @ for its mentions, its unread total but none for Archived`() {
    let tree = Samples.tree
    #expect(tree.badges("folder:1") == FolderTree.Badges(digit: 1, mention: true, unread: 3))
    #expect(tree.badges("folder:3") == FolderTree.Badges(digit: 3, mention: false, unread: 0))
    #expect(tree.badges("folder:4") == FolderTree.Badges(digit: nil, mention: false, unread: 2), "unread messages that don't mention you have no @")
    #expect(tree.badges("archived").unread == 0, "Archived has no unread total, as the terminal's")
}

@Test func `the slots come whole when one changes, and stay as they are when none did`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, favourites: [Favourite(slot: 1, folder: "folder:3")]))
    #expect(tree.slot(of: "folder:3") == 1 && tree.slot(of: "folder:1") == nil && tree.favourite(2) == nil)
    tree.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:4", v: 60, name: "Friends", position: 2, unread: 5)]))
    #expect(tree.slot(of: "folder:3") == 1, "a push with no slots leaves them")
    #expect(tree.badges("folder:4").unread == 5 && tree.badges("folder:3").digit == 1)
}

@Test func `a row sent again with the same version is the newer, an older one changes nothing, and a removed one goes`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:1", v: 50, name: "Family", position: 0, unread: 9)]))
    #expect(tree.row("folder:1")?.unread == 9)
    tree.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:1", v: 40, name: "Old", position: 0)]))
    #expect(tree.row("folder:1")?.name == "Family")
    tree.apply(Update(seq: 303, remove: ["folder:2"]))
    #expect(tree.children(of: "folder:1").isEmpty)
}

@Test func `a folder the palette finds that the tree doesn't hold waits for jawd's M6 before the tree is served, and is gone after`() {
    #expect(FolderTree().notHeld("Family") == "folders come with jawd's M6")
    #expect(Samples.tree.notHeld("Family") == "Family is gone")
}

@Test func `a community's key says nothing while the chat list shows its groups, and the key that shows the list while it's folded`() {
    #expect(FolderTree.groupsLine("Climbing Portugal", listShown: true, focusMode: false) == nil)
    #expect(FolderTree.groupsLine("Climbing Portugal", listShown: false, focusMode: false)
        == "Climbing Portugal's groups are in the chat list: ⇧⌘L shows it")
    #expect(FolderTree.groupsLine("Climbing Portugal", listShown: false, focusMode: true)
        == "Climbing Portugal's groups are in the chat list: ⇧⌘↩ shows it")
}

@Test func `a folder's list refused says the folder is gone, by its name, or jawd's words for why`() {
    #expect(Samples.tree.refused("folder:1", gone: true, why: "jawd has no folder folder:1") == "Family is gone", "not jawd's id for it")
    #expect(Samples.tree.refused("folder:1", gone: false, why: "every window is taken") == "Family couldn't be watched: every window is taken")
    #expect(Samples.tree.refused("folder:99", gone: true, why: "jawd has no folder folder:99") == "The folder is gone", "one the tree no longer holds")
}

@Test func `a slot says what's on it, and a folder of yours is named by its path where another has its name`() {
    var tree = Samples.tree
    #expect(tree.slotName(1) == "Family" && tree.slotName(2) == "Ana Ribeiro" && tree.slotName(4) == nil)
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:7", v: 61, parent: "folder:4", name: "Cousins", position: 0)]))
    #expect(tree.label("folder:2") == "Family › Cousins" && tree.label("folder:7") == "Friends › Cousins")
    #expect(tree.label("folder:3") == "Climbing" && tree.label("all") == "All")
    #expect(tree.label("folder:6") == "Clients", "a folder whose name no other of yours has is named alone, however deep")
}

@Test func `an empty folder says so in the terminal's words, and All says nothing, the panes' place speaking for it`() {
    let tree = Samples.tree
    #expect(tree.emptyLine("folder:1") == "No chats in this folder." && tree.emptyLine("unread") == "No chats in this folder.")
    #expect(tree.emptyLine("smart:2") == "No chats in this folder.")
    #expect(tree.emptyLine("all") == nil && tree.emptyLine("folder:99") == nil)
}

@Test func `the list keeps its folder while the tree has it, and goes back to All once the folder's gone`() {
    var tree = FolderTree()
    #expect(tree.showing("archived") == "archived" && tree.showing("folder:1") == "folder:1", "nothing's gone before jawd's tree")
    tree.apply(Samples.folderUpdate)
    #expect(tree.showing("folder:1") == "folder:1" && tree.showing("smart:2") == "smart:2")
    tree.apply(Update(seq: 301, remove: ["folder:1"]))
    #expect(tree.showing("folder:1") == "all", "deleted from another interface")
    #expect(tree.showing("folder:2") == "folder:2", "a folder that was in it is still there")
}

@Test func `a chat dropped onto a folder of yours is filed there, and anywhere else isn't a drop`() {
    let tree = Samples.tree
    let chat = FolderTree.Dragged.item("ana@s.whatsapp.net", name: "Ana")
    #expect(tree.drop(chat, onto: "folder:1", at: nil) == .add(item: "ana@s.whatsapp.net", to: "folder:1"))
    #expect(tree.drop(chat, onto: "all", at: nil) == nil && tree.drop(chat, onto: "smart:1", at: nil) == nil)
    #expect(tree.drop(chat, onto: "folder:1", at: 0) == nil && tree.drop(chat, onto: nil, at: 2) == nil, "between rows isn't a folder")
}

@Test func `a folder dropped onto another nests in it, between rows goes there, and never into itself or a folder under it`() {
    let tree = Samples.tree
    #expect(tree.drop(.folder("folder:3"), onto: "folder:1", at: nil) == .moveFolder("folder:3", to: "folder:1", position: nil))
    #expect(tree.drop(.folder("folder:1"), onto: "folder:2", at: nil) == nil, "Cousins is under Family")
    #expect(tree.drop(.folder("folder:1"), onto: "folder:1", at: nil) == nil)
    #expect(tree.drop(.folder("folder:5"), onto: nil, at: 0) == .moveFolder("folder:5", to: nil, position: 0), "Work first at the top")
    #expect(tree.drop(.folder("folder:1"), onto: nil, at: 3) == .moveFolder("folder:1", to: nil, position: 2),
            "before Work, among siblings that don't count Family")
    #expect(tree.drop(.folder("all"), onto: "folder:1", at: nil) == nil && tree.drop(.folder("folder:3"), onto: "smart:1", at: nil) == nil)
}

@Test func `the folder before or after, in the tree's order with every folder open, staying at either end`() {
    let tree = Samples.tree
    #expect(tree.step(from: "archived", by: 1) == "folder:1" && tree.step(from: "folder:1", by: 1) == "folder:2", "into Family's Cousins")
    #expect(tree.step(from: "folder:6", by: 1) == "smart:1" && tree.step(from: "smart:3", by: 1) == "smart:3")
    #expect(tree.step(from: "all", by: -1) == "all" && tree.step(from: "folder:5", by: -1) == "folder:4")
    #expect(FolderTree().step(from: "all", by: 1) == "archived")
}

@Test func `a folder a key picks is named while the sidebar isn't shown, with the key that shows its chats when the list isn't either`() {
    #expect(FolderTree.pickedLine("Family", sidebarShown: true, listShown: true, focusMode: false) == nil)
    #expect(FolderTree.pickedLine("Family", sidebarShown: false, listShown: true, focusMode: false) == "Family")
    #expect(FolderTree.pickedLine("Family", sidebarShown: false, listShown: false, focusMode: false) == "Family: ⇧⌘L shows its chats")
    #expect(FolderTree.pickedLine("Family", sidebarShown: false, listShown: false, focusMode: true) == "Family: ⇧⌘↩ shows its chats")
}
