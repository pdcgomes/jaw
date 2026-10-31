import Foundation

/// Window models, as jaw-client's `window.rs` keeps them: a window's rows as jawd sent them, kept
/// by id and ordered by `sort`, applying watch results, moves and pushes alike. They know nothing
/// of the connection, so a test drives them with made-up updates.
///
/// An upserted row replaces the one held when its `v` is at least as new, since the same `v` can
/// come again with another `sort` while a chat list is held during a catch-up.

/// A watch or move that landed: the window's new place, and how many messages came after its last
/// row then, from which what arrives since is counted.
public struct Moved: Sendable, Hashable {
    public var place: Place
    public var newer: UInt64

    public init(place: Place, newer: UInt64) {
        self.place = place
        self.newer = newer
    }
}

/// What changes a window: a watch's or move's result, a push, or several of them merged.
public struct Update<R: WindowRow>: Sendable {
    public var seq: UInt64 = 0
    /// Rows that left the window, applied before `upsert`.
    public var remove: [String] = []
    public var upsert: [R] = []
    public var total: UInt64?
    public var older: Bool?
    public var newer: UInt64?
    public var moved: Moved?
    /// A library window's counts of each kind, when they changed.
    public var counts: LibraryCounts?
    /// A library window's note, as a watch's or move's answer says it: nil in an update that says
    /// nothing of it, and `.some(nil)` from an answer that had none.
    public var note: String??
    /// The folder tree's slots, whole, when they changed.
    public var favourites: [Favourite]?

    public init(seq: UInt64 = 0, remove: [String] = [], upsert: [R] = [], total: UInt64? = nil, older: Bool? = nil,
                newer: UInt64? = nil, moved: Moved? = nil, counts: LibraryCounts? = nil, note: String?? = nil,
                favourites: [Favourite]? = nil)
    {
        self.seq = seq
        self.remove = remove
        self.upsert = upsert
        self.total = total
        self.older = older
        self.newer = newer
        self.moved = moved
        self.counts = counts
        self.note = note
        self.favourites = favourites
    }
}

/// Updates merged as they arrive, so however many come between two frames, a window applies one,
/// and applying it ends where applying each in turn would. It keeps no more than the window does:
/// a row that came and went since the last frame leaves nothing behind.
public struct Merged<R: WindowRow>: Sendable {
    var seq: UInt64 = 0
    var remove: Set<String> = []
    var upsert: [String: R] = [:]
    /// Rows upserted here that the model didn't hold when the last frame took the merge.
    var fresh: Set<String> = []
    var total: UInt64?
    var older: Bool?
    var newer: UInt64?
    var moved: Moved?
    var kinds: LibraryCounts?
    var note: String??
    var favourites: [Favourite]?

    public init() {}

    /// Takes `update` in after what's merged. `ids` are the rows the model will hold once the merge
    /// is applied, which this keeps current.
    public mutating func add(_ update: Update<R>, ids: inout Set<String>) {
        for id in update.remove {
            guard ids.remove(id) != nil else { continue }
            upsert[id] = nil
            if fresh.remove(id) == nil {
                remove.insert(id)
            }
        }
        for row in update.upsert {
            if !ids.contains(row.id), !remove.contains(row.id) {
                fresh.insert(row.id)
            }
            ids.insert(row.id)
            if let held = upsert[row.id], held.v > row.v { continue }
            upsert[row.id] = row
        }
        seq = max(seq, update.seq)
        total = update.total ?? total
        older = update.older ?? older
        newer = update.newer ?? newer
        moved = update.moved ?? moved
        kinds = update.counts ?? kinds
        if let said = update.note { note = said }
        favourites = update.favourites ?? favourites
    }

    public var isEmpty: Bool {
        remove.isEmpty && upsert.isEmpty && total == nil && older == nil && newer == nil && moved == nil && kinds == nil
            && note == nil && favourites == nil
    }

    public func update() -> Update<R> {
        Update(seq: seq, remove: Array(remove), upsert: Array(upsert.values), total: total, older: older,
               newer: newer, moved: moved, counts: kinds, note: note, favourites: favourites)
    }

    /// The upserts it holds, for a test's check that a merge keeps no more than the window does.
    var counts: (remove: Int, upsert: Int) {
        (remove.count, upsert.count)
    }
}

/// A row's place: its `sort`, compared bytewise, then its id, so two rows passing through the same
/// `sort` while a batch applies can't lose each other.
struct Key: Comparable, Hashable, Sendable {
    let sort: [UInt8]
    let id: String

    init(_ sort: String, _ id: String) {
        self.sort = Array(sort.utf8)
        self.id = id
    }

    static func < (a: Key, b: Key) -> Bool {
        if a.sort != b.sort { return a.sort.lexicographicallyPrecedes(b.sort) }
        return a.id.utf8.lexicographicallyPrecedes(b.id.utf8)
    }
}

/// A window's rows by id, and their order from the smallest `sort` up. Windows hold at most a few
/// thousand rows, so the order is a sorted array searched by halves.
struct Rows<R: WindowRow>: Sendable {
    var byID: [String: R] = [:]
    var order: [Key] = []

    /// Where `key` is, or would go.
    func index(of key: Key) -> Int {
        var lo = 0, hi = order.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if order[mid] < key { lo = mid + 1 } else { hi = mid }
        }
        return lo
    }

    /// The first row whose sort is at least `sort`.
    func lowerBound(_ sort: String) -> Int {
        index(of: Key(sort, ""))
    }

    /// The first row whose sort is greater than `sort`.
    func upperBound(_ sort: String) -> Int {
        let s = Array(sort.utf8)
        var i = lowerBound(sort)
        while i < order.count, order[i].sort == s {
            i += 1
        }
        return i
    }

    mutating func insert(_ key: Key) {
        order.insert(key, at: index(of: key))
    }

    mutating func remove(_ key: Key) {
        let i = index(of: key)
        if i < order.count, order[i] == key { order.remove(at: i) }
    }

    /// Applies an update's rows; returns the ids that left.
    mutating func apply(remove: [String], upsert: [R]) -> [String] {
        var gone: [String] = []
        for id in remove {
            if let row = byID.removeValue(forKey: id) {
                self.remove(Key(row.sort, id))
                gone.append(id)
            }
        }
        for row in upsert {
            if let held = byID[row.id] {
                if held.v > row.v { continue }
                if held.sort != row.sort {
                    self.remove(Key(held.sort, row.id))
                    insert(Key(row.sort, row.id))
                }
            } else {
                insert(Key(row.sort, row.id))
            }
            byID[row.id] = row
        }
        return gone
    }

    func row(at i: Int) -> R {
        byID[order[i].id]!
    }
}

/// A chat list: its chats from the largest `sort` down. While the interface moves through it, it
/// can hold its order, so the chat under the pointer stays put as others move.
public struct Chats<R: WindowRow>: Sendable {
    public var folder: String
    public var place: Place
    var rows = Rows<R>()
    var held: Held?
    /// How many chats the whole list has.
    public var total: UInt64 = 0
    /// The feed's seq the window is as of.
    public var seq: UInt64 = 0
    /// Whether the window has had its first rows.
    public var loaded = false

    struct Held: Sendable {
        var order: [String]
        var ids: Set<String>
    }

    public init(folder: String, place: Place) {
        self.folder = folder
        self.place = place
    }

    /// Applies an update; returns the ids that left.
    @discardableResult
    public mutating func apply(_ update: Update<R>) -> [String] {
        let gone = rows.apply(remove: update.remove, upsert: update.upsert)
        if let total = update.total { self.total = total }
        if let moved = update.moved { place = moved.place }
        seq = max(seq, update.seq)
        loaded = true
        return gone
    }

    public func get(_ id: String) -> R? {
        rows.byID[id]
    }

    public var count: Int {
        rows.byID.count
    }

    public var isEmpty: Bool {
        rows.byID.isEmpty
    }

    /// The chats as the list shows them: from the largest `sort` down, or, while held, as they were
    /// when the hold began, with any that came since first.
    public var list: [R] {
        guard let held else { return rows.order.reversed().map { rows.byID[$0.id]! } }
        let came = rows.order.reversed().filter { !held.ids.contains($0.id) }.map { rows.byID[$0.id]! }
        return came + held.order.compactMap { rows.byID[$0] }
    }

    /// Where a chat is in the list as shown.
    public func position(_ id: String) -> Int? {
        list.firstIndex { $0.id == id }
    }

    /// Keeps the list's order as it is until `release`; chats that come meanwhile go first.
    public mutating func hold() {
        guard held == nil else { return }
        let order = rows.order.reversed().map(\.id)
        held = Held(order: order, ids: Set(order))
    }

    /// Lets the list take its order again; says whether it was held.
    @discardableResult
    public mutating func release() -> Bool {
        defer { held = nil }
        return held != nil
    }

    public var isHeld: Bool {
        held != nil
    }
}

/// A roster: a folder's people from the smallest `sort` up, online first, then by when last seen,
/// then those jawd has no last seen for, as jawd sorts them.
public struct People<R: WindowRow>: Sendable {
    public var folder: String
    public var place: Place
    var rows = Rows<R>()
    /// How many people the folder has.
    public var total: UInt64 = 0
    public var seq: UInt64 = 0
    public var loaded = false

    public init(folder: String, place: Place) {
        self.folder = folder
        self.place = place
    }

    /// Applies an update; returns the ids that left.
    @discardableResult
    public mutating func apply(_ update: Update<R>) -> [String] {
        let gone = rows.apply(remove: update.remove, upsert: update.upsert)
        if let total = update.total { self.total = total }
        if let moved = update.moved { place = moved.place }
        seq = max(seq, update.seq)
        loaded = true
        return gone
    }

    public func get(_ id: String) -> R? {
        rows.byID[id]
    }

    public var count: Int {
        rows.byID.count
    }

    /// From the smallest `sort` up.
    public var list: [R] {
        rows.order.map { rows.byID[$0.id]! }
    }

    /// The last person held, as jawd sorts them.
    public var last: R? {
        rows.order.last.flatMap { rows.byID[$0.id] }
    }
}

/// A chat's messages, from the smallest `sort` up.
public struct Messages<R: WindowRow>: Sendable {
    public var chat: String
    public var place: Place
    var rows = Rows<R>()
    /// How many messages the chat has.
    public var total: UInt64 = 0
    /// Whether there are messages before the window.
    public var older = false
    /// How many messages come after the window's last row, counted up to 1,000.
    public var newer: UInt64 = 0
    /// `newer` when the window was last watched or moved.
    var newerThen: UInt64 = 0
    public var seq: UInt64 = 0
    public var loaded = false

    public init(chat: String, place: Place) {
        self.chat = chat
        self.place = place
    }

    /// Applies an update; returns the ids that left, so what was laid out for them can go too.
    @discardableResult
    public mutating func apply(_ update: Update<R>) -> [String] {
        let gone = rows.apply(remove: update.remove, upsert: update.upsert)
        if let total = update.total { self.total = total }
        if let older = update.older { self.older = older }
        if let newer = update.newer { self.newer = newer }
        if let moved = update.moved {
            place = moved.place
            newerThen = moved.newer
        }
        seq = max(seq, update.seq)
        loaded = true
        return gone
    }

    /// Whether the window follows the chat's newest messages.
    public var following: Bool {
        place.around == .newest
    }

    /// Whether the window's last row is the chat's newest message.
    public var holdsNewest: Bool {
        following || newer == 0
    }

    /// How many messages have come after the window since it was last watched or moved: the
    /// "2 new" an interface shows below it.
    public var arrived: UInt64 {
        newer > newerThen ? newer - newerThen : 0
    }

    public func get(_ id: String) -> R? {
        rows.byID[id]
    }

    public var count: Int {
        rows.byID.count
    }

    public var isEmpty: Bool {
        rows.byID.isEmpty
    }

    /// Oldest first.
    public var list: [R] {
        rows.order.map { rows.byID[$0.id]! }
    }

    public var oldest: R? {
        rows.order.first.flatMap { rows.byID[$0.id] }
    }

    public var newest: R? {
        rows.order.last.flatMap { rows.byID[$0.id] }
    }

    /// The message just before `sort`.
    public func before(_ sort: String) -> R? {
        let i = rows.lowerBound(sort)
        return i > 0 ? rows.row(at: i - 1) : nil
    }

    /// The message just after `sort`.
    public func after(_ sort: String) -> R? {
        let i = rows.upperBound(sort)
        return i < rows.order.count ? rows.row(at: i) : nil
    }

    /// The message at `sort`, or the nearest after it, or the newest, for an anchor whose message
    /// has gone.
    public func nearest(_ sort: String) -> R? {
        let i = rows.lowerBound(sort)
        if i < rows.order.count { return rows.row(at: i) }
        return newest
    }

    /// How many come after `sort`.
    public func countAfter(_ sort: String) -> Int {
        rows.order.count - rows.upperBound(sort)
    }

    /// How many come before `sort`, counting no further than `most`.
    public func countBefore(_ sort: String, most: Int) -> Int {
        min(rows.lowerBound(sort), most)
    }
}

/// A library window: its items from the largest `sort` down, newest first, with how many of each kind
/// match its filters.
public struct Library<R: WindowRow>: Sendable {
    public var place: Place
    var rows = Rows<R>()
    /// How many items of its kind match, every kind's for a window of all of them.
    public var total: UInt64 = 0
    /// Whether more items come after its last row.
    public var older = false
    public var counts: LibraryCounts?
    /// What it can't hold yet and why, as jawd's latest answer said.
    public var note: String?
    public var seq: UInt64 = 0
    public var loaded = false

    public init(place: Place) {
        self.place = place
    }

    /// Applies an update; returns the ids that left.
    @discardableResult
    public mutating func apply(_ update: Update<R>) -> [String] {
        let gone = rows.apply(remove: update.remove, upsert: update.upsert)
        if let total = update.total { self.total = total }
        if let older = update.older { self.older = older }
        if let counts = update.counts { self.counts = counts }
        if let note = update.note { self.note = note }
        if let moved = update.moved { place = moved.place }
        seq = max(seq, update.seq)
        loaded = true
        return gone
    }

    public func get(_ id: String) -> R? {
        rows.byID[id]
    }

    public var count: Int {
        rows.byID.count
    }

    public var isEmpty: Bool {
        rows.byID.isEmpty
    }

    /// Newest first.
    public var list: [R] {
        rows.order.reversed().map { rows.byID[$0.id]! }
    }
}
