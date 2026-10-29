import JawKit

/// The folder tree as jawd keeps it, and what the sidebar, the menus and the keys read from it: the
/// built-in folders, yours nested by parent and position, the smart ones, and the nine favourite
/// slots. Until jawd serves folders (its M6) it has All and Archived, as jawd serves them now.
public struct FolderTree: Sendable, Equatable {
    public private(set) var rows: [String: FolderRow]
    /// The slots held, in their order; the empty ones aren't there.
    public private(set) var favourites: [Favourite] = []
    /// Whether jawd has sent its tree: until it has, the menus have no folder items.
    public private(set) var served = false

    /// All and Archived, before jawd has sent its tree.
    public init() {
        rows = Dictionary(uniqueKeysWithValues: Self.unserved.map { ($0.id, $0) })
    }

    public init(_ rows: [FolderRow], favourites: [Favourite]) {
        self.rows = Dictionary(rows.map { ($0.id, $0) }) { $1 }
        self.favourites = favourites.sorted { $0.slot < $1.slot }
        served = true
    }

    static let unserved = [FolderRow(id: "all", name: "All", position: 0), FolderRow(id: "archived", name: "Archived", position: 4)]

    /// Takes the tree's window's update: rows gone; rows changed, one sent again with the same version
    /// being the newer, as a folder's counts change without it; and the slots, whole, when they
    /// changed. The first replaces All and Archived with jawd's tree.
    public mutating func apply(_ update: Update<FolderRow>) {
        if !served {
            rows = [:]
            served = true
        }
        for id in update.remove {
            rows[id] = nil
        }
        for row in update.upsert {
            if let held = rows[row.id], held.v > row.v { continue }
            rows[row.id] = row
        }
        if let slots = update.favourites { favourites = slots.sorted { $0.slot < $1.slot } }
    }

    public func row(_ id: String) -> FolderRow? {
        rows[id]
    }

    static func ordered(_ rows: some Sequence<FolderRow>) -> [FolderRow] {
        rows.sorted { ($0.position, $0.name, $0.id) < ($1.position, $1.name, $1.id) }
    }

    /// The built-in folders, in their order.
    public var builtIn: [FolderRow] {
        Self.ordered(rows.values.filter(\.isBuiltIn))
    }

    /// The smart folders, in their order.
    public var smart: [FolderRow] {
        Self.ordered(rows.values.filter(\.isSmart))
    }

    /// Your folders in `parent`, or at the top for nil, in their order.
    public func children(of parent: String?) -> [FolderRow] {
        Self.ordered(rows.values.filter { $0.isYours && $0.parent == parent })
    }

    /// Your folders in the tree's order with every one open, each with how deep it is: Add to Folder's
    /// and Move to's order.
    public var yours: [(row: FolderRow, depth: Int)] {
        var out: [(row: FolderRow, depth: Int)] = []
        var seen: Set<String> = []
        func walk(_ parent: String?, _ depth: Int) {
            for row in children(of: parent) where seen.insert(row.id).inserted {
                out.append((row, depth))
                walk(row.id, depth + 1)
            }
        }
        walk(nil, 0)
        return out
    }

    /// Every folder in the tree's order with every one open: the built-in ones, yours, the smart ones.
    public var order: [String] {
        builtIn.map(\.id) + yours.map(\.row.id) + smart.map(\.id)
    }

    /// A folder and every folder under it.
    public func subtree(_ id: String) -> Set<String> {
        var out: Set<String> = [id]
        var next = [id]
        while let parent = next.popLast() {
            for row in rows.values where row.parent == parent && out.insert(row.id).inserted {
                next.append(row.id)
            }
        }
        return out
    }

    /// The folders a folder is in, nearest first.
    public func ancestors(_ id: String) -> [String] {
        var out: [String] = []
        var at = rows[id]?.parent
        while let parent = at, parent != id, !out.contains(parent) {
            out.append(parent)
            at = rows[parent]?.parent
        }
        return out
    }

    /// A folder as the palette and the status line name it: its name, or, for one of yours whose name
    /// another of yours has, its path from the top, as "Family › Cousins".
    public func label(_ id: String) -> String {
        guard let row = rows[id] else { return id }
        guard row.isYours, rows.values.contains(where: { $0.isYours && $0.id != id && $0.name == row.name }) else { return row.name }
        return (ancestors(id).reversed().compactMap { rows[$0]?.name } + [row.name]).joined(separator: " › ")
    }

    /// The slot that holds a folder or a chat.
    public func slot(of target: String) -> Int? {
        favourites.first { $0.target == target }?.slot
    }

    public func favourite(_ slot: Int) -> Favourite? {
        favourites.first { $0.slot == slot }
    }

    /// What's on a slot, as its menus say it: the folder's name, or the chat's.
    public func slotName(_ slot: Int) -> String? {
        guard let f = favourite(slot) else { return nil }
        if let folder = f.folder { return label(folder) }
        return f.name ?? f.chat
    }

    /// What a folder's row shows beside its name.
    public struct Badges: Equatable, Sendable {
        /// Its ⌘ digit, from the slots.
        public var digit: Int?
        public var mention: Bool
        /// Its unread total; none for Archived, as the terminal's.
        public var unread: Int
    }

    public func badges(_ id: String) -> Badges {
        let row = rows[id]
        return Badges(digit: slot(of: id), mention: (row?.mentions ?? 0) > 0, unread: id == "archived" ? 0 : row?.unread ?? 0)
    }

    /// What lays the tree out: each folder's place and name. A change to anything else, such as its
    /// counts, draws its row again where it is.
    var shape: [String: Shape] {
        rows.mapValues { Shape(parent: $0.parent, position: $0.position, name: $0.name) }
    }

    struct Shape: Hashable {
        let parent: String?
        let position: Int
        let name: String
    }
}
