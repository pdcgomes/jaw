import Foundation

/// What `find` can answer with.
public enum FindKind: String, Codable, Sendable, Hashable, CaseIterable, Comparable {
    case chat, person, folder, message, link, media, file

    private var order: Int {
        FindKind.allCases.firstIndex(of: self) ?? 0
    }

    public static func < (a: FindKind, b: FindKind) -> Bool {
        a.order < b.order
    }
}

/// `find`'s params.
public struct Find: Encodable, Sendable, Hashable {
    public var query: String
    public var kinds: [FindKind]
    /// A chat's id, to find text only in that chat. Names are found anywhere.
    public var chat: String?
    /// The most results of each kind; 10 when absent, and at most 200.
    public var limit: UInt32?

    public init(query: String, kinds: [FindKind] = [], chat: String? = nil, limit: UInt32? = nil) {
        self.query = query
        self.kinds = kinds
        self.chat = chat
        self.limit = limit
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(query, forKey: .query)
        if !kinds.isEmpty { try c.encode(kinds, forKey: .kinds) }
        try c.encodeIfPresent(chat, forKey: .chat)
        try c.encodeIfPresent(limit, forKey: .limit)
    }

    enum Keys: String, CodingKey {
        case query, kinds, chat, limit
    }
}

/// `find`'s answer: names first, best first, then messages, newest first.
public struct Found: Decodable, Sendable, Hashable {
    public var results: [Hit]
    /// How many of each kind matched; text kinds counted up to 1,000.
    public var counts: [FindKind: Int]

    public init(results: [Hit] = [], counts: [FindKind: Int] = [:]) {
        self.results = results
        self.counts = counts
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        results = try c.decodeIfPresent([Hit].self, forKey: .results) ?? []
        let raw = try c.decodeIfPresent([String: Int].self, forKey: .counts) ?? [:]
        counts = Dictionary(uniqueKeysWithValues: raw.compactMap { k, n in FindKind(rawValue: k).map { ($0, n) } })
    }

    enum Keys: String, CodingKey {
        case results, counts
    }
}

/// One thing `find` found.
public struct Hit: Decodable, Sendable, Hashable {
    public var kind: FindKind
    public var score: Int
    /// What matched, with `mark` spans where the query did.
    public var text: [Span]
    /// For a message, its chat's name.
    public var chatName: String?
    public var folders: [String]
    public var row: Found.Row

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        kind = try c.decode(FindKind.self, forKey: .kind)
        score = try c.decodeIfPresent(Int.self, forKey: .score) ?? 0
        text = try c.decodeIfPresent([Span].self, forKey: .text) ?? []
        chatName = try c.decodeIfPresent(String.self, forKey: .chatName)
        folders = try c.decodeIfPresent([String].self, forKey: .folders) ?? []
        switch kind {
        case .chat:
            row = try (try? c.decode(ChatRow.self, forKey: .row)).map(Found.Row.chat) ?? .other(c.decode(JSONValue.self, forKey: .row))
        case .message:
            row = try (try? c.decode(MessageRow.self, forKey: .row)).map(Found.Row.message) ?? .other(c.decode(JSONValue.self, forKey: .row))
        case .person:
            row = try (try? c.decode(Person.self, forKey: .row)).map(Found.Row.person) ?? .other(c.decode(JSONValue.self, forKey: .row))
        case .folder:
            row = try (try? c.decode(FolderRef.self, forKey: .row)).map(Found.Row.folder) ?? .other(c.decode(JSONValue.self, forKey: .row))
        case .link, .media, .file:
            row = try .other(c.decode(JSONValue.self, forKey: .row))
        }
    }

    enum Keys: String, CodingKey {
        case kind, score, text, folders, row
        case chatName = "chat_name"
    }
}

public extension Found {
    /// What a hit found.
    enum Row: Sendable, Hashable {
        case chat(ChatRow)
        case message(MessageRow)
        case person(Person)
        case folder(FolderRef)
        case other(JSONValue)
    }
}

/// A person `find` found, whom you have no chat with yet.
public struct Person: Decodable, Sendable, Hashable {
    public var id: String
    public var name: String
    public var phone: String?
}

public struct FolderRef: Decodable, Sendable, Hashable {
    public var id: String
    public var name: String
    public var parent: String?
}
