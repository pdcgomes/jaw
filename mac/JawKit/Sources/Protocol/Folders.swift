import Foundation

// The folder tree, the favourite slots, the roster and the folder requests, as jawd's M6 sends and
// takes them: protocol/README.md's "Folders and the roster", and jaw-proto's windows.rs.

/// A folder in the tree, as `folders.watch` sends it: built in (`all`, `unread`, `groups`,
/// `communities`, `archived`), smart (`smart:` and a number) or yours (`folder:` and a number).
public struct FolderRow: WindowRow, Decodable, Hashable {
    public var id: String
    /// Its version, which stays as it was when only the folder's counts change, so a row sent again
    /// with the same `v` is the newer, as any window's upsert is.
    public var v: UInt64
    /// The folder it's in; nil at the top.
    public var parent: String?
    public var name: String
    /// Its place among its siblings of its kind.
    public var position: Int
    /// A smart folder's rule; nil for the others and for a rule jawd couldn't read.
    public var smart: JSONValue?
    /// Its chats' and its subfolders' chats' unread messages, and those that mention you, each chat
    /// counted once.
    public var unread: Int
    public var mentions: Int
    /// The chats and people in it and under it, each once; a smart folder's counted up to 1,000.
    public var count: Int
    /// Its favourite slot, 1 to 9, as the row has it; the tree's slots say the same, and the sidebar
    /// reads those.
    public var favourite: Int?

    /// A folder has no place in a list of its own: the tree is laid out by `parent` and `position`.
    public var sort: String {
        ""
    }

    /// Neither smart nor yours: the five above, and any built-in folder a newer jawd adds.
    public var isBuiltIn: Bool {
        !isSmart && !isYours
    }

    public var isSmart: Bool {
        id.hasPrefix("smart:")
    }

    /// One of yours, which you name, nest, move and file into.
    public var isYours: Bool {
        id.hasPrefix("folder:")
    }

    public init(id: String, v: UInt64 = 0, parent: String? = nil, name: String, position: Int = 0, smart: JSONValue? = nil,
                unread: Int = 0, mentions: Int = 0, count: Int = 0, favourite: Int? = nil)
    {
        self.id = id
        self.v = v
        self.parent = parent
        self.name = name
        self.position = position
        self.smart = smart
        self.unread = unread
        self.mentions = mentions
        self.count = count
        self.favourite = favourite
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        v = try c.decodeIfPresent(UInt64.self, forKey: .v) ?? 0
        parent = try c.decodeIfPresent(String.self, forKey: .parent)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        position = try c.decodeIfPresent(Int.self, forKey: .position) ?? 0
        smart = try c.decodeIfPresent(JSONValue.self, forKey: .smart)
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        mentions = try c.decodeIfPresent(Int.self, forKey: .mentions) ?? 0
        count = try c.decodeIfPresent(Int.self, forKey: .count) ?? 0
        favourite = try c.decodeIfPresent(Int.self, forKey: .favourite)
    }

    enum Keys: String, CodingKey {
        case id, v, parent, name, position, smart, unread, mentions, count, favourite
    }
}

/// A favourite slot: what ⌘ and its digit go to, a folder, or a chat with its name.
public struct Favourite: Decodable, Sendable, Hashable {
    public var slot: Int
    public var folder: String?
    public var chat: String?
    /// The chat's name, for a chat.
    public var name: String?

    public init(slot: Int, folder: String? = nil, chat: String? = nil, name: String? = nil) {
        self.slot = slot
        self.folder = folder
        self.chat = chat
        self.name = name
    }

    /// The folder's or the chat's id.
    public var target: String? {
        folder ?? chat
    }
}

/// What `folders.watch` returns: every folder, and the slots held, the empty ones left out.
public struct FoldersWatched: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var rows: [FolderRow]
    public var remove: [String]
    public var favourites: [Favourite]
    /// Rows that weren't jaw's protocol, left out, and why the first wasn't.
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        rows = try c.rows(.rows, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
        favourites = try c.decodeIfPresent([Favourite].self, forKey: .favourites) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, rows, remove, favourites
    }
}

/// The `folders` push: the folders that changed, and the slots whole when one of them changed, nil
/// when none did.
public struct FoldersPush: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var upsert: [FolderRow]
    public var remove: [String]
    public var favourites: [Favourite]?
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        upsert = try c.rows(.upsert, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
        favourites = try c.decodeIfPresent([Favourite].self, forKey: .favourites)
    }

    enum Keys: String, CodingKey {
        case watch, seq, upsert, remove, favourites
    }
}

/// How someone is: online, last seen, or hidden, when they hide it or WhatsApp hasn't told jawd.
/// Typing and recording aren't among them: they show from the chat's typing, which jaw.db never
/// holds, so they never move someone in a roster.
public enum PresenceState: String, Decodable, Sendable, Hashable {
    case online, seen, hidden

    /// A state jawd may add later reads as hidden, which says nothing.
    public init(from decoder: any Decoder) throws {
        self = try PresenceState(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .hidden
    }
}

public struct Presence: Decodable, Sendable, Hashable {
    public var state: PresenceState
    /// When they were last seen, for `seen`.
    public var at: Int64?
    /// False while jawd isn't following them, so it's what was last known, drawn faintly.
    public var known: Bool

    public init(state: PresenceState, at: Int64? = nil, known: Bool = true) {
        self.state = state
        self.at = at
        self.known = known
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        state = (try? c.decodeIfPresent(PresenceState.self, forKey: .state)) ?? .hidden
        at = try c.decodeIfPresent(Int64.self, forKey: .at)
        known = try c.decodeIfPresent(Bool.self, forKey: .known) ?? true
    }

    enum Keys: String, CodingKey {
        case state, at, known
    }

    /// Online. Someone typing or recording in their chat is around too, which the roster reads from
    /// the chat.
    public var around: Bool {
        state == .online
    }
}

/// Someone in a roster, which reads from the smallest `sort` up: online first, by name; then by
/// when last seen, the most recent first; then those jawd has no last seen for, by name.
public struct RosterRow: WindowRow, Decodable, Hashable {
    /// `person:` and a number.
    public var id: String
    public var v: UInt64
    public var sort: String
    public var name: String
    public var phone: String?
    public var about: String?
    /// Their picture's path, relative to jaw's folder, once fetched.
    public var avatar: String?
    public var presence: Presence
    /// Their one-to-one chat's id, if there is one.
    public var chat: String?
    /// Your folders they're filed in, themselves or through their chat.
    public var folders: [String]

    public init(id: String, v: UInt64, sort: String, name: String, phone: String? = nil, presence: Presence,
                chat: String? = nil, folders: [String] = [])
    {
        self.id = id
        self.v = v
        self.sort = sort
        self.name = name
        self.phone = phone
        about = nil
        avatar = nil
        self.presence = presence
        self.chat = chat
        self.folders = folders
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        v = try c.decode(UInt64.self, forKey: .v)
        sort = try c.decode(String.self, forKey: .sort)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        phone = try c.decodeIfPresent(String.self, forKey: .phone)
        about = try c.decodeIfPresent(String.self, forKey: .about)
        avatar = try c.decodeIfPresent(String.self, forKey: .avatar)
        presence = (try? c.decodeIfPresent(Presence.self, forKey: .presence)) ?? Presence(state: .hidden)
        chat = try c.decodeIfPresent(String.self, forKey: .chat)
        folders = (try? c.decodeIfPresent([String].self, forKey: .folders)) ?? []
    }

    enum Keys: String, CodingKey {
        case id, v, sort, name, phone, about, avatar, presence, chat, folders
    }
}

/// What `roster.watch` returns, and `watch.move` on a roster.
public struct RosterWatched: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64
    public var rows: [RosterRow]
    public var remove: [String]
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total) ?? 0
        rows = try c.rows(.rows, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, rows, remove
    }
}

/// The `roster` push.
public struct RosterPush: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64?
    public var upsert: [RosterRow]
    public var remove: [String]
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total)
        upsert = try c.rows(.upsert, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, upsert, remove
    }
}

/// `folders.watch`'s params. The tree is one window, watched whole; watched again, it says the rows
/// held, and the answer's `remove` says which went. jawd compares the tree row by row whatever
/// `since` says, as a folder's counts change without its version, so `since` isn't sent.
struct FoldersWatch: Encodable, Sendable {
    var have: [String]

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        if !have.isEmpty { try c.encode(have, forKey: .have) }
    }

    enum Keys: String, CodingKey {
        case have
    }
}

/// `roster.watch`'s params: a folder's people, from the top or around someone in it. `offline` is
/// always true, since the sidebar counts the people it folds.
struct RosterWatch: Encodable, Sendable {
    var folder: String
    var around: Around
    var before: UInt32
    var after: UInt32
    var have: [String]

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(folder, forKey: .folder)
        try c.encode(around, forKey: .around)
        try c.encode(before, forKey: .before)
        try c.encode(after, forKey: .after)
        try c.encode(true, forKey: .offline)
        if !have.isEmpty { try c.encode(have, forKey: .have) }
    }

    enum Keys: String, CodingKey {
        case folder, around, before, after, offline, have
    }
}

/// `folder.create`'s params: a folder of yours in `parent`, or at the top when it's nil.
public struct FolderCreate: Encodable, Sendable, Hashable {
    public var name: String
    public var parent: String?

    public init(name: String, parent: String?) {
        self.name = name
        self.parent = parent
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(name, forKey: .name)
        try c.encodeIfPresent(parent, forKey: .parent)
    }

    enum Keys: String, CodingKey {
        case name, parent
    }
}

/// `folder.rename`'s params.
public struct FolderRename: Encodable, Sendable, Hashable {
    public var folder: String
    public var name: String

    public init(folder: String, name: String) {
        self.folder = folder
        self.name = name
    }
}

/// `folder.move`'s params: into `parent`, or back to the top with nil, which is sent as null; at
/// `position` among its new siblings, or last without one.
public struct FolderMove: Encodable, Sendable, Hashable {
    public var folder: String
    public var parent: String?
    public var position: Int?

    public init(folder: String, parent: String?, position: Int?) {
        self.folder = folder
        self.parent = parent
        self.position = position
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(folder, forKey: .folder)
        if let parent { try c.encode(parent, forKey: .parent) } else { try c.encodeNil(forKey: .parent) }
        try c.encodeIfPresent(position, forKey: .position)
    }

    enum Keys: String, CodingKey {
        case folder, parent, position
    }
}

/// `folder.delete`'s params.
public struct FolderID: Encodable, Sendable, Hashable {
    public var folder: String

    public init(folder: String) {
        self.folder = folder
    }
}

/// `folder.add`'s and `folder.remove`'s params: the folder, and a chat's `id` or a `person:` id.
public struct FolderItem: Encodable, Sendable, Hashable {
    public var folder: String
    public var item: String

    public init(folder: String, item: String) {
        self.folder = folder
        self.item = item
    }
}

/// `favourite.set`'s params: a folder's or a chat's id on `slot`, or nil, sent as null, to empty it.
public struct FavouriteSet: Encodable, Sendable, Hashable {
    public var slot: Int
    public var target: String?

    public init(slot: Int, target: String?) {
        self.slot = slot
        self.target = target
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(slot, forKey: .slot)
        if let target { try c.encode(target, forKey: .target) } else { try c.encodeNil(forKey: .target) }
    }

    enum Keys: String, CodingKey {
        case slot, target
    }
}

/// `folder.create`'s answer: the folder made, or nil when it went before jawd read it back.
struct FolderMade: Decodable, Sendable {
    var folder: FolderRow?
}
