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
}

/// The folder items of a chat's menus and of someone's in the roster: entries worked out from the tree
/// and the item's row as a menu opens, and the commands they name, bound to the item.
public enum FolderMenu {
    /// Add to Folder's entries: your folders as a tree.
    public static func addTo(_ tree: FolderTree) -> [MenuEntry] {
        tree.yours.map { MenuEntry.command("file:\($0.row.id)") }
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
        return out
    }

    /// What the status line says once jawd has done it, as the terminal says it.
    public static func said(_ act: FolderAct, name: String, tree: FolderTree) -> String? {
        switch act {
        case let .add(_, to): "\(name) filed in \(tree.label(to))"
        case let .remove(_, from): "\(name) taken out of \(tree.label(from))"
        case let .move(_, from, to): "\(name) moved from \(tree.label(from)) to \(tree.label(to))"
        }
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
