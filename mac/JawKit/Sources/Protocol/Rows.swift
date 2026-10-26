import Foundation

/// A window's row: an id, a version that only grows, and where it sorts, bytewise.
public protocol WindowRow: Sendable {
    var id: String { get }
    var v: UInt64 { get }
    var sort: String { get }
}

/// A chat, as the chat list shows it. A chat list reads from the largest `sort` down.
public struct ChatRow: WindowRow, Decodable, Hashable {
    public var id: String
    public var v: UInt64
    public var sort: String
    public var kind: ChatKind
    public var name: String
    /// Its picture's path, relative to jaw's folder, once fetched.
    public var avatar: String?
    public var last: Last?
    public var unread: Int
    public var mentions: Int
    public var pinned: Bool
    public var mutedUntil: Int64?
    public var archived: Bool
    public var markedUnread: Bool
    public var draft: String?
    /// Who's typing or recording now, by person `id`.
    public var typing: [String]
    /// For a group in a community, the community's `id`.
    public var community: String?
    /// For a community, its groups' unread total.
    public var childrenUnread: Int?

    public var muted: Bool {
        guard let until = mutedUntil else { return false }
        return until < 0 || until > Int64(Date().timeIntervalSince1970 * 1000)
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        v = try c.decode(UInt64.self, forKey: .v)
        sort = try c.decode(String.self, forKey: .sort)
        kind = try c.decodeIfPresent(ChatKind.self, forKey: .kind) ?? .unknown
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        avatar = try c.decodeIfPresent(String.self, forKey: .avatar)
        last = try c.decodeIfPresent(Last.self, forKey: .last)
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        mentions = try c.decodeIfPresent(Int.self, forKey: .mentions) ?? 0
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
        mutedUntil = try c.decodeIfPresent(Int64.self, forKey: .mutedUntil)
        archived = try c.decodeIfPresent(Bool.self, forKey: .archived) ?? false
        markedUnread = try c.decodeIfPresent(Bool.self, forKey: .markedUnread) ?? false
        draft = try c.decodeIfPresent(String.self, forKey: .draft)
        typing = try c.decodeIfPresent([String].self, forKey: .typing) ?? []
        community = try c.decodeIfPresent(String.self, forKey: .community)
        childrenUnread = try c.decodeIfPresent(Int.self, forKey: .childrenUnread)
    }

    public init(id: String, v: UInt64, sort: String, kind: ChatKind = .direct, name: String, last: Last? = nil,
                unread: Int = 0, mentions: Int = 0, pinned: Bool = false, draft: String? = nil,
                typing: [String] = [], community: String? = nil)
    {
        self.id = id
        self.v = v
        self.sort = sort
        self.kind = kind
        self.name = name
        avatar = nil
        self.last = last
        self.unread = unread
        self.mentions = mentions
        self.pinned = pinned
        mutedUntil = nil
        archived = false
        markedUnread = false
        self.draft = draft
        self.typing = typing
        self.community = community
        childrenUnread = nil
    }

    enum Keys: String, CodingKey {
        case id, v, sort, kind, name, avatar, last, unread, mentions, pinned, archived, draft, typing, community
        case mutedUntil = "muted_until"
        case markedUnread = "marked_unread"
        case childrenUnread = "children_unread"
    }
}

public enum ChatKind: String, Decodable, Sendable, Hashable {
    case direct, group, community, announcement, broadcast
    case `self`
    case unknown

    public init(from decoder: any Decoder) throws {
        self = try ChatKind(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .unknown
    }

    /// Whether messages in it have senders worth naming.
    public var isGroup: Bool {
        self == .group || self == .community || self == .announcement || self == .broadcast
    }
}

/// A chat's newest message, for its line in the chat list.
public struct Last: Decodable, Sendable, Hashable {
    /// Who sent it; absent on your own.
    public var from: String?
    public var mine: Bool
    public var summary: String
    public var kind: MessageKind
    public var status: MessageStatus?
    public var at: Int64

    public init(from: String?, mine: Bool, summary: String, kind: MessageKind = .text, status: MessageStatus? = nil,
                at: Int64)
    {
        self.from = from
        self.mine = mine
        self.summary = summary
        self.kind = kind
        self.status = status
        self.at = at
    }
}

public enum MessageKind: String, Decodable, Sendable, Hashable {
    case text, image, video, gif, voice, audio, document, sticker, album, location, contact, poll, event, system,
         call, unknown
    case videoNote = "video_note"
    case liveLocation = "live_location"

    public init(from decoder: any Decoder) throws {
        self = try MessageKind(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .unknown
    }
}

public enum MessageStatus: String, Decodable, Sendable, Hashable, Comparable {
    case pending, sent, delivered, read, played, failed

    private var rank: Int {
        switch self {
        case .failed: -1
        case .pending: 0
        case .sent: 1
        case .delivered: 2
        case .read: 3
        case .played: 4
        }
    }

    public static func < (a: MessageStatus, b: MessageStatus) -> Bool {
        a.rank < b.rank
    }
}

/// A message, render-ready. A chat's messages read from the smallest `sort` up.
public struct MessageRow: WindowRow, Decodable, Hashable {
    public var id: String
    public var v: UInt64
    public var sort: String
    public var chat: String
    /// When it was sent, in milliseconds since the epoch.
    public var at: Int64
    /// Who sent it; absent on your own.
    public var from: Sender?
    public var mine: Bool
    public var kind: MessageKind
    /// The text and caption, parsed.
    public var spans: [Span]
    public var reply: Quote?
    /// How many times it was forwarded, 0 if it wasn't.
    public var forwarded: Int
    public var reactions: [Reaction]
    public var status: MessageStatus?
    public var edited: Bool
    public var deleted: Bool
    public var expiresAt: Int64?
    public var viewOnce: Bool
    public var starred: Bool
    /// For `system` and `call`: the one sentence to show.
    public var system: String?
    public var media: Media?
    public var voice: Voice?
    public var album: Album?
    public var link: LinkPreview?
    public var poll: Poll?

    /// The text as one string, for copying and for the box when editing.
    public var plain: String {
        spans.map(\.s).joined()
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        v = try c.decode(UInt64.self, forKey: .v)
        sort = try c.decode(String.self, forKey: .sort)
        chat = try c.decodeIfPresent(String.self, forKey: .chat) ?? ""
        at = try c.decodeIfPresent(Int64.self, forKey: .at) ?? 0
        from = try c.decodeIfPresent(Sender.self, forKey: .from)
        mine = try c.decodeIfPresent(Bool.self, forKey: .mine) ?? false
        kind = try c.decodeIfPresent(MessageKind.self, forKey: .kind) ?? .unknown
        spans = try c.decodeIfPresent([Span].self, forKey: .spans) ?? []
        reply = try c.decodeIfPresent(Quote.self, forKey: .reply)
        forwarded = try c.decodeIfPresent(Int.self, forKey: .forwarded) ?? 0
        reactions = try c.decodeIfPresent([Reaction].self, forKey: .reactions) ?? []
        status = try c.decodeIfPresent(MessageStatus.self, forKey: .status)
        edited = try c.decodeIfPresent(Bool.self, forKey: .edited) ?? false
        deleted = try c.decodeIfPresent(Bool.self, forKey: .deleted) ?? false
        expiresAt = try c.decodeIfPresent(Int64.self, forKey: .expiresAt)
        viewOnce = try c.decodeIfPresent(Bool.self, forKey: .viewOnce) ?? false
        starred = try c.decodeIfPresent(Bool.self, forKey: .starred) ?? false
        system = try c.decodeIfPresent(String.self, forKey: .system)
        media = try? c.decodeIfPresent(Media.self, forKey: .media)
        voice = try? c.decodeIfPresent(Voice.self, forKey: .voice)
        album = try? c.decodeIfPresent(Album.self, forKey: .album)
        link = try? c.decodeIfPresent(LinkPreview.self, forKey: .link)
        poll = try? c.decodeIfPresent(Poll.self, forKey: .poll)
    }

    public init(id: String, v: UInt64, sort: String, chat: String, at: Int64, from: Sender? = nil, mine: Bool = false,
                kind: MessageKind = .text, spans: [Span], reply: Quote? = nil, reactions: [Reaction] = [],
                status: MessageStatus? = nil, edited: Bool = false, deleted: Bool = false, system: String? = nil)
    {
        self.id = id
        self.v = v
        self.sort = sort
        self.chat = chat
        self.at = at
        self.from = from
        self.mine = mine
        self.kind = kind
        self.spans = spans
        self.reply = reply
        forwarded = 0
        self.reactions = reactions
        self.status = status
        self.edited = edited
        self.deleted = deleted
        expiresAt = nil
        viewOnce = false
        starred = false
        self.system = system
        media = nil
        voice = nil
        album = nil
        link = nil
        poll = nil
    }

    enum Keys: String, CodingKey {
        case id, v, sort, chat, at, from, mine, kind, spans, reply, forwarded, reactions, status, edited, deleted
        case starred, system, media, voice, album, link, poll
        case expiresAt = "expires_at"
        case viewOnce = "view_once"
    }
}

/// Who sent a message.
public struct Sender: Decodable, Sendable, Hashable {
    /// `person:` and a number.
    public var id: String
    public var name: String
    /// Which of the theme's eight sender colours to draw their name in.
    public var colour: Int

    public init(id: String, name: String, colour: Int) {
        self.id = id
        self.name = name
        self.colour = colour
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        if let s = try? c.decode(String.self, forKey: .id) {
            id = s
        } else {
            id = try "person:\(c.decode(Int.self, forKey: .id))"
        }
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        colour = try c.decodeIfPresent(Int.self, forKey: .colour) ?? 0
    }

    enum Keys: String, CodingKey {
        case id, name, colour
    }
}

/// A stretch of a message's text, as jawd parsed it, or of what `find` matched.
public struct Span: Decodable, Sendable, Hashable {
    public var t: SpanKind
    public var s: String
    /// A link's address.
    public var url: String?
    /// The person a mention names.
    public var person: String?

    public init(_ t: SpanKind, _ s: String, url: String? = nil, person: String? = nil) {
        self.t = t
        self.s = s
        self.url = url
        self.person = person
    }
}

public enum SpanKind: String, Decodable, Sendable, Hashable {
    case text, bold, italic, strike, mono, quote, list, link, mention, mark, unknown

    public init(from decoder: any Decoder) throws {
        self = try SpanKind(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .unknown
    }
}

/// The message a reply quotes.
public struct Quote: Decodable, Sendable, Hashable {
    public var id: String
    public var from: String
    public var summary: String

    public init(id: String, from: String, summary: String) {
        self.id = id
        self.from = from
        self.summary = summary
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        from = try c.decodeIfPresent(String.self, forKey: .from) ?? ""
        summary = try c.decodeIfPresent(String.self, forKey: .summary) ?? ""
    }

    enum Keys: String, CodingKey {
        case id, from, summary
    }
}

public struct Reaction: Decodable, Sendable, Hashable {
    public var emoji: String
    public var n: Int
    public var mine: Bool

    public init(emoji: String, n: Int, mine: Bool) {
        self.emoji = emoji
        self.n = n
        self.mine = mine
    }
}

/// A media message's file, as far as Mac-M1 draws it: its size and name for its line.
public struct Media: Decodable, Sendable, Hashable {
    public var w: Int?
    public var h: Int?
    public var seconds: Double?
    public var bytes: Int64?
    public var name: String?
    public var mime: String?
}

public struct Voice: Decodable, Sendable, Hashable {
    public var seconds: Double
    public var waveform: [Int]
    public var played: Bool?
}

public struct Album: Decodable, Sendable, Hashable {
    public var items: [Item]

    public struct Item: Decodable, Sendable, Hashable {
        public var id: String
        public var kind: String
    }
}

public struct LinkPreview: Decodable, Sendable, Hashable {
    public var url: String
    public var title: String?
}

public struct Poll: Decodable, Sendable, Hashable {
    public var question: String
}
