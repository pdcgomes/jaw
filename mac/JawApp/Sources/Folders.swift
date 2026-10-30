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
    /// list's folder, and your folders' places and names.
    var folderItemsKey: [String] {
        guard let chat = conversation?.chat, tree.served else { return [] }
        return [chat.id, folder, chat.folders.joined(separator: ",")] + tree.yours.map { "\($0.depth) \($0.row.id) \($0.row.name)" }
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
        }
    }

    /// A filing jawd has done: a pane's chat the list doesn't hold gets no push to say where it's filed
    /// now, so its row takes what jawd's answer did.
    func filed(_ filing: Filing) {
        for (id, c) in panes where window?.chatList.row(id) == nil {
            if let row = FolderMenu.filed(filing, c.chat) { c.update(chat: row) }
        }
    }
}
