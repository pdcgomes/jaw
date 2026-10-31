import Foundation

/// Where a window is anchored: the chat list's `top`, a chat's `newest` or first `unread` message,
/// or a row by its `id`.
public enum Around: Sendable, Hashable, Encodable {
    case top
    case newest
    case unread
    case row(String)

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .top: try c.encode("top")
        case .newest: try c.encode("newest")
        case .unread: try c.encode("unread")
        case let .row(id): try c.encode(id)
        }
    }
}

/// Where a window is anchored, and how many rows it keeps on each side of the anchor.
public struct Place: Sendable, Hashable {
    public var around: Around
    public var before: UInt32
    public var after: UInt32

    public init(around: Around, before: UInt32, after: UInt32) {
        self.around = around
        self.before = before
        self.after = after
    }

    /// The chat list from its first row.
    public static func top(_ after: UInt32) -> Place {
        Place(around: .top, before: 0, after: after)
    }

    /// A chat's newest messages; the window follows new ones.
    public static func newest(_ before: UInt32) -> Place {
        Place(around: .newest, before: before, after: 0)
    }

    /// A chat's first unread message, found once when the window is watched, or its newest.
    public static func unread(before: UInt32, after: UInt32) -> Place {
        Place(around: .unread, before: before, after: after)
    }

    /// Around a row: a chat in the list, or a message in a chat.
    public static func around(_ id: String, before: UInt32, after: UInt32) -> Place {
        Place(around: .row(id), before: before, after: after)
    }
}

/// A window's rows read one by one: a row that isn't jaw's protocol is left out and counted, so it
/// can be said, rather than losing the rest of the window with it.
struct Lossy<R: Decodable>: Decodable {
    var rows: [R] = []
    var unread = 0
    var why: String?

    init(from decoder: any Decoder) throws {
        var c = try decoder.unkeyedContainer()
        while !c.isAtEnd {
            do {
                rows.append(try c.decode(R.self))
            } catch {
                unread += 1
                why = why ?? "\(error)"
                _ = try? c.decode(JSONValue.self)
            }
        }
    }
}

extension KeyedDecodingContainer {
    func rows<R: Decodable>(_ key: Key, into unread: inout Int, why: inout String?) throws -> [R] {
        guard let lossy = try decodeIfPresent(Lossy<R>.self, forKey: key) else { return [] }
        unread += lossy.unread
        why = why ?? lossy.why
        return lossy.rows
    }

    /// An answer's `typing`, each typist read on its own: one that isn't jaw's protocol is left out,
    /// a chat left with nobody has nobody typing, and a `typing` that isn't a map at all is none, so
    /// the window still reads.
    func typists(_ key: Key) -> Typists {
        guard let map = try? decodeIfPresent([String: Lossy<Typist>].self, forKey: key) else { return [:] }
        return map.compactMapValues { $0.rows.isEmpty ? nil : $0.rows }
    }
}

/// Someone typing or recording in a chat, as jawd says it.
public struct Typist: Decodable, Sendable, Hashable {
    /// `person:` and a number.
    public var id: String
    /// Their name, as a message from them shows it.
    public var name: String
    public var state: State

    public enum State: String, Decodable, Sendable, Hashable {
        case typing, recording

        /// A state jawd may add later reads as typing, since they're doing something in the chat.
        public init(from decoder: any Decoder) throws {
            self = try State(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .typing
        }
    }

    public init(id: String, name: String, state: State = .typing) {
        self.id = id
        self.name = name
        self.state = state
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        state = try c.decodeIfPresent(State.self, forKey: .state) ?? .typing
    }

    enum Keys: String, CodingKey {
        case id, name, state
    }
}

/// Who's typing in each chat where someone is, by the chat's `id`, each chat's oldest first.
public typealias Typists = [String: [Typist]]

/// The `typing` push: who's typing in a chat now, oldest first; empty once nobody is.
public struct TypingPush: Decodable, Sendable {
    public var chat: String
    public var who: [Typist]

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        chat = try c.decode(String.self, forKey: .chat)
        who = try c.decode(Lossy<Typist>.self, forKey: .who).rows
    }

    enum Keys: String, CodingKey {
        case chat, who
    }
}

struct ChatsWatch: Encodable, Sendable {
    var folder: String
    var around: Around
    var before: UInt32
    var after: UInt32
    var since: UInt64?
    var have: [String]

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(folder, forKey: .folder)
        try c.encode(around, forKey: .around)
        try c.encode(before, forKey: .before)
        try c.encode(after, forKey: .after)
        try c.encodeIfPresent(since, forKey: .since)
        if !have.isEmpty { try c.encode(have, forKey: .have) }
    }

    enum Keys: String, CodingKey {
        case folder, around, before, after, since, have
    }
}

struct MessagesWatch: Encodable, Sendable {
    var chat: String
    var around: Around
    var before: UInt32
    var after: UInt32
    var since: UInt64?
    var have: [String]

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(chat, forKey: .chat)
        try c.encode(around, forKey: .around)
        try c.encode(before, forKey: .before)
        try c.encode(after, forKey: .after)
        try c.encodeIfPresent(since, forKey: .since)
        if !have.isEmpty { try c.encode(have, forKey: .have) }
    }

    enum Keys: String, CodingKey {
        case chat, around, before, after, since, have
    }
}

struct WatchMove: Encodable, Sendable {
    var watch: String
    var around: Around
    var before: UInt32
    var after: UInt32
}

/// `unwatch`'s params, and the `resync` push's.
struct WatchRef: Codable, Sendable {
    var watch: String
}

/// What `chats.watch` returns, and `watch.move` on a chat list.
public struct ChatsWatched: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64
    public var rows: [ChatRow]
    public var remove: [String]
    /// Who's typing in the window's chats now, where someone is. It replaces what was held for every
    /// chat of the window, so a chat left out has nobody typing.
    public var typing: Typists
    /// Rows that weren't jaw's protocol, left out, and why the first wasn't.
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total) ?? 0
        rows = try c.rows(.rows, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
        typing = c.typists(.typing)
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, rows, remove, typing
    }
}

/// What `messages.watch` returns, and `watch.move` on a chat's messages.
public struct MessagesWatched: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64
    public var older: Bool
    public var newer: UInt64
    public var rows: [MessageRow]
    public var remove: [String]
    /// Who's typing in the window's chat now, if anyone is, as a chat list's answer has it.
    public var typing: Typists
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total) ?? 0
        older = try c.decodeIfPresent(Bool.self, forKey: .older) ?? false
        newer = try c.decodeIfPresent(UInt64.self, forKey: .newer) ?? 0
        rows = try c.rows(.rows, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
        typing = c.typists(.typing)
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, older, newer, rows, remove, typing
    }
}

/// The `chats` push.
public struct ChatsPush: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64?
    public var upsert: [ChatRow]
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

/// The `messages` push.
public struct MessagesPush: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64?
    public var older: Bool?
    public var newer: UInt64?
    public var upsert: [MessageRow]
    public var remove: [String]
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total)
        older = try c.decodeIfPresent(Bool.self, forKey: .older)
        newer = try c.decodeIfPresent(UInt64.self, forKey: .newer)
        upsert = try c.rows(.upsert, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, older, newer, upsert, remove
    }
}
