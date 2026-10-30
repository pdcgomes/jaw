import JawKit

/// Something filed in folders: a chat, or someone from the roster.
public struct Filed: Sendable, Hashable {
    /// A chat's id, or a `person:` id.
    public var id: String
    public var name: String
    /// Your folders it sits in directly.
    public var folders: [String]
    /// The chat a favourite slot holds for it: the chat's own id, or someone's one-to-one chat; nil for
    /// someone with no chat yet.
    public var chat: String?

    public init(_ chat: ChatRow) {
        id = chat.id
        name = chat.name
        folders = chat.folders
        self.chat = chat.id
    }

    public init(_ person: RosterRow) {
        id = person.id
        name = person.name
        folders = person.folders
        chat = person.chat
    }
}

/// What a folder item asks for, which the app asks jawd for.
public enum FolderAct: Sendable, Equatable {
    /// Files an item in a folder of yours, or takes it out.
    case add(item: String, to: String)
    case remove(item: String, from: String)
    /// Files an item in `to`, then, once jawd has it there, takes it out of `from`.
    case move(item: String, from: String, to: String)
    /// A new folder, in `parent` or at the top, named in place, and what to file in it once it's made.
    case newFolder(parent: String?, filing: Filed?)
    /// A folder of yours, its name opened in place.
    case rename(String)
    /// A folder of yours nested in `to`, or put back at the top, at `position` among its new siblings
    /// or last.
    case moveFolder(String, to: String?, position: Int?)
    /// A folder of yours deleted, after asking.
    case delete(String)
    /// A folder or a chat put on a slot, or the slot emptied with nil.
    case favourite(slot: Int, target: String?)
}

/// The folder items of a chat's menus and of someone's in the roster: entries worked out from the tree
/// and the item's row as a menu opens, and the commands they name, bound to the item.
public enum FolderMenu {
    /// Add to Folder's entries: your folders as a tree, then New Folder… after a separator.
    public static func addTo(_ tree: FolderTree) -> [MenuEntry] {
        tree.yours.map { MenuEntry.command("file:\($0.row.id)") } + [.separator, .command("file.new")]
    }

    /// A chat's or someone's folder items: Add to Folder ▸; and while the list shows one of your
    /// folders that holds the item, directly or through a folder in it, Move to ▸ and Remove from it.
    /// Move to lists every folder of yours, as Add to Folder does, the list's own greyed: left out, the
    /// folders in it would be indented under nothing. None before jawd serves folders.
    public static func entries(_ item: Filed, tree: FolderTree, list: String) -> [MenuEntry] {
        guard tree.served else { return [] }
        let add = MenuEntry.submenu("Add to Folder", addTo(tree))
        guard holds(list, item, tree) else { return [add] }
        return [add, .submenu("Move to", tree.yours.map { .command("move:\($0.row.id)") }), .command("unfile")]
    }

    /// Whether `list` is one of your folders holding `item`, directly or through a folder in it.
    static func holds(_ list: String, _ item: Filed, _ tree: FolderTree) -> Bool {
        tree.row(list)?.isYours == true && !tree.subtree(list).isDisjoint(with: item.folders)
    }

    /// The commands those entries name, bound to `item`: each folder of yours, ticked where the item
    /// sits directly, filing it there or taking it out; Move to's, filing it in another and taking it
    /// out of the list's; and Remove from. Move to's and Remove from are greyed while the item is in
    /// the list's folder only through a folder in it, since neither can take it out of this one.
    @MainActor
    public static func commands(_ item: Filed, tree: FolderTree, list: String,
                                act: @escaping @MainActor @Sendable (FolderAct) -> Void) -> [Command]
    {
        let here = holds(list, item, tree), direct = item.folders.contains(list)
        var out: [Command] = []
        for (row, depth) in tree.yours {
            let folder = row.id, filed = item.folders.contains(folder), label = tree.label(folder)
            out.append(Command("file:\(folder)", row.name, phrase: filed ? "Take this chat out of \(label)" : "Add this chat to \(label)",
                               place: nil, indent: depth, on: { filed })
            {
                act(filed ? .remove(item: item.id, from: folder) : .add(item: item.id, to: folder))
            })
            if here {
                out.append(Command("move:\(folder)", row.name, phrase: "Move this chat to \(label)", place: nil, indent: depth,
                                   enabled: { direct && folder != list })
                {
                    act(.move(item: item.id, from: list, to: folder))
                })
            }
        }
        if here, let shown = tree.row(list) {
            out.append(Command("unfile", "Remove from \(shown.name)", place: nil, palette: false, enabled: { direct }) {
                act(.remove(item: item.id, from: list))
            })
        }
        out.append(Command("file.new", "New Folder…", phrase: "Add this chat to a new folder", place: nil) {
            act(.newFolder(parent: nil, filing: item))
        })
        return out + favouriteCommands(item.chat, name: "this chat", tree: tree, act: act)
    }

    /// What the status line says once jawd has done it, as the terminal says it.
    public static func said(_ act: FolderAct, name: String, tree: FolderTree) -> String? {
        switch act {
        case let .add(_, to): "\(name) filed in \(tree.label(to))"
        case let .remove(_, from): "\(name) taken out of \(tree.label(from))"
        case let .move(_, from, to): "\(name) moved from \(tree.label(from)) to \(tree.label(to))"
        case let .moveFolder(folder, to, _): "\(tree.label(folder)) moved to \(to.map { tree.label($0) } ?? "the top")"
        case let .delete(folder): "\(tree.label(folder)) deleted; its chats stay wherever else they're filed"
        case let .favourite(slot, target): target == nil ? "⌘\(slot) is free" : "⌘\(slot) goes to \(name)"
        case .newFolder, .rename: nil
        }
    }

    /// A folder's menu in the sidebar: for one of yours, New Folder Inside…, Rename, Move to ▸ (the top,
    /// or a folder of yours that isn't it or under it, where it is now greyed) and Delete…; for a
    /// built-in or smart folder, and on the sidebar's space, New Folder…. None before jawd serves folders.
    public static func folderEntries(_ folder: String?, tree: FolderTree) -> [MenuEntry] {
        guard tree.served else { return [] }
        guard let folder else { return [.command("folder.new.top")] }
        guard tree.row(folder)?.isYours == true else { return ChatMenu.with([.command("folder.new.top")], favourites(folder, tree: tree)) }
        let under = tree.subtree(folder)
        let targets = tree.yours.filter { !under.contains($0.row.id) }.map { MenuEntry.command("folder.to:\($0.row.id)") }
        return ChatMenu.with([.command("folder.new.inside"), .command("folder.rename"),
                              .submenu("Move to", [.command("folder.to.top"), .separator] + targets), .command("folder.delete")],
                             favourites(folder, tree: tree))
    }

    /// The commands those entries name, bound to `folder`.
    @MainActor
    public static func folderCommands(_ folder: String?, tree: FolderTree,
                                      act: @escaping @MainActor @Sendable (FolderAct) -> Void) -> [Command]
    {
        var out = [Command("folder.new.top", "New Folder…", place: nil, palette: false) { act(.newFolder(parent: nil, filing: nil)) }]
        guard let folder else { return out }
        guard let row = tree.row(folder), row.isYours else {
            return out + favouriteCommands(folder, name: tree.label(folder), tree: tree, act: act)
        }
        let label = tree.label(folder), under = tree.subtree(folder)
        out.append(Command("folder.new.inside", "New Folder Inside…", phrase: "New folder in \(label)", detail: "nested in it", place: nil) {
            act(.newFolder(parent: folder, filing: nil))
        })
        out.append(Command("folder.rename", "Rename", phrase: "Rename \(label)", hint: "↩", place: nil) { act(.rename(folder)) })
        out.append(Command("folder.to.top", "The Top", phrase: "Move \(label) to the top", place: nil, enabled: { row.parent != nil }) {
            act(.moveFolder(folder, to: nil, position: nil))
        })
        for (target, depth) in tree.yours where !under.contains(target.id) {
            out.append(Command("folder.to:\(target.id)", target.name, phrase: "Move \(label) into \(tree.label(target.id))", place: nil,
                               indent: depth, enabled: { row.parent != target.id })
            {
                act(.moveFolder(folder, to: target.id, position: nil))
            })
        }
        out.append(Command("folder.delete", "Delete…", phrase: "Delete the folder \(label)", detail: "its chats stay wherever else they're filed",
                           key: Shortcut("⌫"), place: nil) { act(.delete(folder)) })
        return out + favouriteCommands(folder, name: label, tree: tree, act: act)
    }

    /// Favourite ▸: the nine slots, each saying what's on it, ticked on the one holding `target`, and
    /// Remove from Favourites when one does. None for nothing a slot can hold, nor before jawd serves
    /// folders.
    public static func favourites(_ target: String?, tree: FolderTree) -> [MenuEntry] {
        guard tree.served, target != nil else { return [] }
        return [.submenu("Favourite", (1 ... 9).map { .command("favourite.slot:\($0)") } + [.separator, .command("favourite.off")])]
    }

    /// The commands those entries name: a slot puts `target` there, which takes it off its old slot and
    /// puts out what the slot held, as `favourite.set` does; Remove from Favourites empties its slot.
    /// `name` is what the palette calls it: "this chat", or a folder's name.
    @MainActor
    public static func favouriteCommands(_ target: String?, name: String, tree: FolderTree,
                                         act: @escaping @MainActor @Sendable (FolderAct) -> Void) -> [Command]
    {
        guard let target else { return [] }
        let held = tree.slot(of: target)
        var out = (1 ... 9).map { slot in
            Command("favourite.slot:\(slot)", "⌘\(slot)  \(tree.slotName(slot) ?? "empty")", phrase: "Put \(name) on ⌘\(slot)",
                    detail: tree.slotName(slot).map { "\($0) now" } ?? "nothing on it", place: nil, palette: held != slot,
                    on: { held == slot }) { act(.favourite(slot: slot, target: target)) }
        }
        if let held {
            out.append(Command("favourite.off", "Remove from Favourites", phrase: "Take \(name) off ⌘\(held)", place: nil) {
                act(.favourite(slot: held, target: nil))
            })
        }
        return out
    }

    /// What the status line says, before jawd's words, when Move to leaves the item in both folders:
    /// jawd filed it in `to` and didn't take it out of `from`.
    public static func inBoth(_ name: String, from: String, to: String, tree: FolderTree) -> String {
        "\(name) is now in both \(tree.label(to)) and \(tree.label(from))"
    }

    /// `row` as a filing jawd has done leaves it, for a row jawd's pushes don't reach, as a pane's whose
    /// chat the list doesn't hold; nil when the filing isn't the row's chat's. `chat` is the filed
    /// item's chat when the item is someone, whose filing files their one-to-one chat.
    public static func filed(_ filing: Filing, _ row: ChatRow, chat: String? = nil) -> ChatRow? {
        guard (chat ?? filing.item) == row.id else { return nil }
        var out = row
        out.folders = filing.applied(to: row.folders)
        return out
    }
}
