import AppKit
import JawKit
import JawUI

/// The app's half of the folders: the tree and the roster watched, the sidebar's host, and what the
/// folder items do, asked of jawd.
extension AppModel: SidebarHost {
    /// The tree the sidebar holds, which the menus, the keys and the palette read.
    var tree: FolderTree {
        window?.sidebar.tree ?? FolderTree()
    }

    func sidebar(_: SidebarController, pick folder: String) {
        pick(folder: folder)
    }

    func sidebar(_: SidebarController, name: String, as naming: SidebarController.Naming) {
        if let folder = naming.renaming {
            jawd.folderRename(folder, name: name)
        } else {
            jawd.folderCreate(name: name, parent: naming.parent)
        }
    }

    func sidebar(_: SidebarController, menuFor folder: String?) -> [NSMenuItem] {
        let name = folder.map { tree.label($0) } ?? ""
        let bound = FolderMenu.folderCommands(folder, tree: tree) { [weak self] act in self?.folderAct(act, name: name) }
        return Menus.items(FolderMenu.folderEntries(folder, tree: tree)) { id in bound.first { $0.id == id } }
    }

    func sidebar(_: SidebarController, do act: FolderAct, name: String) {
        folderAct(act, name: name)
    }

    /// ⇧⌘N: a new folder, in the folder picked when it's yours, or at the top, named in place.
    func newFolder() {
        guard tree.served else { return say("folders come with jawd's M6") }
        folderAct(.newFolder(parent: tree.row(folder)?.isYours == true ? folder : nil, filing: nil), name: "")
    }

    /// A chat's or someone's folder items' commands, bound to it.
    func folderCommands(for item: Filed) -> [Command] {
        FolderMenu.commands(item, tree: tree, list: folder) { [weak self] act in self?.folderAct(act, name: item.name) }
    }

    /// The focused chat's folder items, for the end of the menu bar's Chat menu.
    func focusedFolderItems() -> [NSMenuItem] {
        guard let chat = conversation?.chat else { return [] }
        let bound = folderCommands(for: Filed(chat))
        return Menus.items(ChatMenu.folderItems(chat, tree: tree, in: folder)) { id in bound.first { $0.id == id } }
    }

    static let folderItemTag = 7301

    /// What the menu bar's folder items are made from: the focused chat, the folders it's in, the
    /// list's folder, and your folders' places and names and what's on each slot.
    var folderItemsKey: [String] {
        guard let chat = conversation?.chat, tree.served else { return [] }
        return [chat.id, folder, chat.folders.joined(separator: ",")] + treeKey
    }

    /// The tree's part of that key, made again only once the tree's shape or slots have changed, as
    /// every drain asks for it and a push of the folders' counts comes with every message.
    var treeKey: [String] {
        let changes = window?.sidebar.shapeChanges ?? 0
        if let made = treeItemsKey, made.changes == changes { return made.key }
        let tree = tree
        let key = tree.yours.map { "\($0.depth) \($0.row.id) \($0.row.name)" }
            + (1 ... 9).map { "\($0) \(tree.favourite($0)?.target ?? "") \(tree.slotName($0) ?? "")" }
        treeItemsKey = (changes, key)
        return key
    }

    /// The menu bar's Chat menu ends with the focused chat's folder items, made again only when what
    /// they're made from changes, as the Layouts menu is made again when the layouts change: a delegate
    /// filling them as the menu opens would be asked again each time AppKit looks for a key's item.
    func chatMenuFoldersChanged() {
        let key = folderItemsKey
        guard key != folderItemsMade, let menu = NSApp.mainMenu?.item(withTitle: MenuPlace.chat.rawValue)?.submenu else { return }
        folderItemsMade = key
        for item in menu.items where item.tag == Self.folderItemTag {
            menu.removeItem(item)
        }
        let items = focusedFolderItems()
        guard !items.isEmpty else { return }
        for item in [NSMenuItem.separator()] + items {
            item.tag = Self.folderItemTag
            menu.addItem(item)
        }
    }

    /// m: the chat's Add to Folder, as a menu of its own under the pane's header.
    func addToFolderMenu(for chat: ChatRow) -> NSMenu? {
        guard tree.served else {
            say("folders come with jawd's M6")
            return nil
        }
        let bound = folderCommands(for: Filed(chat))
        return Menus.menu(FolderMenu.addTo(tree)) { id in bound.first { $0.id == id } }
    }

    /// Asks jawd for what a folder item does, saying what it did once jawd has done it.
    func folderAct(_ act: FolderAct, name: String) {
        let said = FolderMenu.said(act, name: name, tree: tree)
        switch act {
        case let .add(item, to): jawd.folderAdd(to, item: item, said: said)
        case let .remove(item, from): jawd.folderRemove(from, item: item, said: said)
        case let .move(item, from, to):
            jawd.moveItem(item, from: from, to: to, said: said, both: FolderMenu.inBoth(name, from: from, to: to, tree: tree))
        case let .newFolder(parent, filing):
            window?.showSidebar()
            window?.sidebar.beginNaming(parent: parent, filing: filing)
        case let .rename(id):
            window?.showSidebar()
            window?.sidebar.beginRenaming(id)
        case let .moveFolder(id, to, position):
            jawd.folderMove(id, parent: to, position: position, said: said)
        case let .delete(id):
            window?.askFolderDelete(id)
        case let .favourite(slot, target):
            jawd.favouriteSet(slot: slot, target: target, said: said)
        }
    }

    /// The folder actions the palette lists, for the focused chat and the folder picked.
    func folderPaletteCommands() -> [Command] {
        FolderMenu.paletteCommands(conversation.map { Filed($0.chat) }, folder: folder, tree: tree) { [weak self] act, name in
            self?.folderAct(act, name: name)
        }
    }

    /// ⌘1 to ⌘9: the slot's folder, picked, or its chat, opened in the focused pane.
    func goToFavourite(_ slot: Int) {
        guard tree.served else { return say("favourites come with jawd's M6") }
        guard let f = tree.favourite(slot) else {
            return say("nothing is on ⌘\(slot); a chat's or a folder's Favourite menu puts one there")
        }
        if let folder = f.folder {
            pick(folder: folder, saying: true)
        } else if let chat = f.chat {
            openChat(chat, name: f.name ?? chat)
        }
    }

    /// ⌥⇧↑ and ⌥⇧↓: the folder before or after, from anywhere ⌥↑ and ⌥↓ work, picked as a click picks it,
    /// the folders it's in opened.
    func stepFolder(_ by: Int) {
        guard tree.served else { return say("folders come with jawd's M6") }
        let next = tree.step(from: folder, by: by)
        if next != folder { pick(folder: next, saying: true) }
    }

    /// A chat to open that the app may hold no row for, as a favourite's or someone's in the roster:
    /// at once when the list or a pane has its row, or else once jawd finds it, in All and then
    /// Archived, the panes still waiting for theirs looked up after it.
    func openChat(_ id: String, name: String, inNewPane: Bool = false) {
        if let row = window?.chatList.row(id) ?? panes[id]?.chat {
            return open(found: row, inNewPane: inNewPane)
        }
        opening = Opening(chat: id, name: name, inNewPane: inNewPane)
        let waiting = (grid?.reading ?? []).compactMap(\.chat).filter { panes[$0] == nil && unopened[$0] == nil && $0 != id }
        jawd.lookUp([id] + waiting)
    }

    /// A chat being opened once jawd finds it.
    struct Opening {
        var chat: String
        var name: String
        var inNewPane: Bool
    }

    /// A chat being opened, as jawd found it: opened, or why it won't be.
    func opened(_ wanted: Opening, _ found: Looked) {
        switch found {
        case let .row(row): open(found: row, inNewPane: wanted.inNewPane)
        case .gone: say("\(wanted.name) isn't among your chats now", warn: true)
        case let .refused(why): say("\(wanted.name) couldn't be opened: \(why)", warn: true)
        }
    }

    /// A chat found to open: in the focused pane or a new one; or, for a community, which can't go in a
    /// pane, its groups shown under it in the chat list, the list taken to the list that holds it when
    /// the folder it shows doesn't, and the key that shows the list said while it's folded.
    func open(found row: ChatRow, inNewPane: Bool) {
        guard row.kind == .community else { return inNewPane ? openInNewPane(row) : open(row, opened: true) }
        if window?.chatList.row(row.id) == nil { pick(folder: row.archived ? "archived" : "all", saying: true) }
        window?.chatList.showGroups(of: row.id)
        if let line = FolderTree.groupsLine(row.name, listShown: window?.chatList.isShown ?? false, focusMode: focusMode) { say(line) }
    }

    /// A filing jawd has done: a pane's chat the list doesn't hold gets no push to say where it's filed
    /// now, so its row takes what jawd's answer did; someone filed from the roster files their chat.
    func filed(_ filing: Filing) {
        let person = filing.item.hasPrefix("person:") ? window?.sidebar.roster.people.get(filing.item)?.chat : nil
        for (id, c) in panes where window?.chatList.row(id) == nil {
            if let row = FolderMenu.filed(filing, c.chat, chat: person) { c.update(chat: row) }
        }
    }
}

extension AppModel: RosterHost {
    /// The picked folder's people, under the tree; watching them is what tells jawd whose presence to
    /// follow, within its limits.
    func watchRoster() {
        if let w = rosterWindow { jawd.unwatch(w) }
        window?.sidebar.roster.show(folder: folder)
        rosterWindow = jawd.watchRoster(folder, .top(RosterController.rows))
    }

    func roster(_: RosterController, move place: Place) {
        if let w = rosterWindow { jawd.move(w, place) }
    }

    func roster(_: RosterController, open person: RosterRow) {
        openPerson(person)
    }

    func roster(_: RosterController, menuItemsFor person: RosterRow) -> [NSMenuItem] {
        let bound = [
            Command(RosterMenu.open, "Open Chat", place: nil, palette: false) { [weak self] in self?.openPerson(person) },
            Command(RosterMenu.inNewPane, "Open in New Pane", place: nil, palette: false) { [weak self] in
                self?.openPerson(person, inNewPane: true)
            },
        ] + folderCommands(for: Filed(person))
        return Menus.items(RosterMenu.entries(person, tree: tree, list: folder)) { id in bound.first { $0.id == id } }
    }

    /// Someone's chat, opened, or started with them when they have none yet, as `chat.start` with the
    /// person does.
    func openPerson(_ person: RosterRow, inNewPane: Bool = false) {
        if let chat = person.chat { return openChat(chat, name: person.name, inNewPane: inNewPane) }
        startChat(person: person.id, inNewPane: inNewPane)
    }

    /// Who in the roster is typing or recording in their one-to-one chat, and which, from who jawd
    /// says is typing in the chats its windows show, the roster's people's own among them, as a pane
    /// reads its typing.
    func rosterTyping() -> [String: Typist.State] {
        guard let roster = window?.sidebar.roster else { return [:] }
        return RosterLines.typing(roster.people.list, in: typing)
    }
}
