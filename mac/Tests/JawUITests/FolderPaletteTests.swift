@testable import JawKit
@testable import JawUI
import Testing

/// The palette lists each folder action by name, for the focused chat and the folder picked, so it
/// reaches them without a mode: "Add this chat to Family", "Move this chat to Work", "Put this chat on
/// ⌘3"; each name once, a folder of yours named by its path where another has its name; none before
/// jawd serves folders.

@MainActor
@Test func `the palette names each folder action for the focused chat and the folder picked, each name once`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:7", v: 61, parent: "folder:4", name: "Cousins", position: 0)]))
    let names = FolderMenu.paletteCommands(Filed(ana(in: ["folder:1"])), folder: "folder:1", tree: tree) { _, _ in }.map(\.phrase)
    #expect(Set(names).count == names.count, "each once: \(names)")
    #expect(names.contains("Take this chat out of Family") && names.contains("Add this chat to Family › Cousins")
        && names.contains("Add this chat to Friends › Cousins") && names.contains("Add this chat to a new folder"))
    #expect(names.contains("Move this chat to Work") && !names.contains("Move this chat to Family"))
    #expect(names.contains("Put this chat on ⌘4") && !names.contains("Put this chat on ⌘2") && names.contains("Take this chat off ⌘2"))
    #expect(names.contains("New folder in Family") && names.contains("Rename Family") && names.contains("Move Family into Work")
        && names.contains("Delete the folder Family") && !names.contains("Move Family to the top"))
    #expect(names.contains("Put Family on ⌘3") && names.contains("Take Family off ⌘1"))
    #expect(!names.contains("Remove from Family") && !names.contains("New Folder…"), "the menus' own, which the palette has by other names")
}

@MainActor
@Test func `the palette lists no folder action before jawd serves folders, and with no chat focused only the folder picked's`() {
    #expect(FolderMenu.paletteCommands(Filed(ana(in: [])), folder: "all", tree: FolderTree()) { _, _ in }.isEmpty)
    let names = FolderMenu.paletteCommands(nil, folder: "folder:3", tree: Samples.tree) { _, _ in }.map(\.phrase)
    #expect(names.contains("Rename Climbing") && !names.contains { $0.contains("this chat") })
}

@MainActor
@Test func `each palette action asks for what it says, naming what it's done to for what's said`() {
    let acts = Acts()
    let commands = FolderMenu.paletteCommands(Filed(ana(in: ["folder:1"])), folder: "folder:3", tree: Samples.tree) { act, name in
        acts.done.append(act)
        acts.names.append(name)
    }
    commands.first { $0.phrase == "Add this chat to Friends" }?.run()
    commands.first { $0.phrase == "Rename Climbing" }?.run()
    #expect(acts.done == [.add(item: Samples.direct.id, to: "folder:4"), .rename("folder:3")])
    #expect(acts.names == ["Ana Ribeiro", "Climbing"])
}

@MainActor
@Test func `a folder's name typed in the palette finds its actions first`() {
    let model = PaletteModel()
    let chat = ana(in: ["folder:1"])
    model.actions = { FolderMenu.paletteCommands(Filed(chat), folder: "all", tree: Samples.tree) { _, _ in }.map(\.paletteAction) }
    model.query = "fri"
    #expect(model.items.first.map { $0.title.map(\.s).joined() } == "Add this chat to Friends")
}
