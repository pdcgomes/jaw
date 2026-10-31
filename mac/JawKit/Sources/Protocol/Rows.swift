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
        last = try? c.decodeIfPresent(Last.self, forKey: .last)
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        mentions = try c.decodeIfPresent(Int.self, forKey: .mentions) ?? 0
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
        mutedUntil = try? c.decodeIfPresent(Int64.self, forKey: .mutedUntil)
        archived = try c.decodeIfPresent(Bool.self, forKey: .archived) ?? false
        markedUnread = try c.decodeIfPresent(Bool.self, forKey: .markedUnread) ?? false
        draft = try c.decodeIfPresent(String.self, forKey: .draft)
        community = try c.decodeIfPresent(String.self, forKey: .community)
        childrenUnread = try c.decodeIfPresent(Int.self, forKey: .childrenUnread)
    }

    public init(id: String, v: UInt64, sort: String, kind: ChatKind = .direct, name: String, last: Last? = nil,
                unread: Int = 0, mentions: Int = 0, pinned: Bool = false, draft: String? = nil, community: String? = nil)
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
        self.community = community
        childrenUnread = nil
    }

    enum Keys: String, CodingKey {
        case id, v, sort, kind, name, avatar, last, unread, mentions, pinned, archived, draft, community
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

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        from = try? c.decodeIfPresent(String.self, forKey: .from)
        mine = (try? c.decodeIfPresent(Bool.self, forKey: .mine)) ?? false
        summary = (try? c.decodeIfPresent(String.self, forKey: .summary)) ?? ""
        kind = (try? c.decodeIfPresent(MessageKind.self, forKey: .kind)) ?? .unknown
        status = try? c.decodeIfPresent(MessageStatus.self, forKey: .status)
        at = (try? c.decodeIfPresent(Int64.self, forKey: .at)) ?? 0
    }

    enum Keys: String, CodingKey {
        case from, mine, summary, kind, status, at
    }

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
    case text, image, video, gif, voice, audio, document, sticker, album, location, contact, poll, event, invite,
         system, call, unknown
    case videoNote = "video_note"
    case liveLocation = "live_location"

    public init(from decoder: any Decoder) throws {
        self = try MessageKind(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .unknown
    }
}

public enum MessageStatus: String, Decodable, Sendable, Hashable, Comparable {
    case pending, sent, delivered, read, played, failed

    /// A status jawd may add later reads as pending, since it isn't known to have gone.
    public init(from decoder: any Decoder) throws {
        self = try MessageStatus(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .pending
    }

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
    public var location: Location?
    public var contact: Contact?
    public var invite: Invite?

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
        from = try? c.decodeIfPresent(Sender.self, forKey: .from)
        mine = try c.decodeIfPresent(Bool.self, forKey: .mine) ?? false
        kind = try c.decodeIfPresent(MessageKind.self, forKey: .kind) ?? .unknown
        spans = (try? c.decodeIfPresent([Span].self, forKey: .spans)) ?? []
        reply = try? c.decodeIfPresent(Quote.self, forKey: .reply)
        forwarded = try c.decodeIfPresent(Int.self, forKey: .forwarded) ?? 0
        reactions = (try? c.decodeIfPresent([Reaction].self, forKey: .reactions)) ?? []
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
        location = try? c.decodeIfPresent(Location.self, forKey: .location)
        contact = try? c.decodeIfPresent(Contact.self, forKey: .contact)
        invite = try? c.decodeIfPresent(Invite.self, forKey: .invite)
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
        location = nil
        contact = nil
        invite = nil
    }

    enum Keys: String, CodingKey {
        case id, v, sort, chat, at, from, mine, kind, spans, reply, forwarded, reactions, status, edited, deleted
        case starred, system, media, voice, album, link, poll, location, contact, invite
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
    /// A document's pages.
    public var pages: Int?
    /// jawd's previews by their size ("256", "768"), paths under its root, once it downloads media.
    public var previews: [String: String]?
}

/// Where a location message points.
public struct Location: Decodable, Sendable, Hashable {
    public var lat: Double?
    public var lon: Double?
    public var name: String?
    public var address: String?
    public var url: String?
}

/// A contact card.
public struct Contact: Decodable, Sendable, Hashable {
    public var name: String?
}

/// An invitation to join a group, or to follow a channel or be one of its admins, as it says.
public struct Invite: Decodable, Sendable, Hashable {
    /// The group's JID, for an invitation to join it.
    public var group: String?
    /// The channel's JID, for an invitation to follow it or help run it.
    public var channel: String?
    public var name: String?
    /// When it runs out, in milliseconds since the epoch.
    public var expires: Int64?
    /// Whether it's to be one of the channel's admins.
    public var admin: Bool?
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

/// A link's preview, as its message carries it: where it goes, the page's title and description, and
/// its thumbnail once jawd keeps one.
public struct LinkPreview: Decodable, Sendable, Hashable {
    public var url: String
    public var title: String?
    public var description: String?
    /// The thumbnail's path under jawd's root, in `media/links/`, once jawd keeps it: 140 pixels on its
    /// long side for a preview you send, as it came for one someone else sends.
    public var preview: String?

    public init(url: String, title: String? = nil, description: String? = nil, preview: String? = nil) {
        self.url = url
        self.title = title
        self.description = description
        self.preview = preview
    }

    /// Where it goes, as its card says it: the URL's host in its own letters rather than percent
    /// escapes, in lower case and without `www.`, or the URL when it has none.
    public var host: String {
        guard let encoded = URL(string: url)?.host(), !encoded.isEmpty else { return url }
        let host = (encoded.removingPercentEncoding ?? encoded).lowercased()
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }
}

public struct Poll: Decodable, Sendable, Hashable {
    public var question: String
    public var options: [Option]
    /// How many options a vote may choose.
    public var max: Int
    /// How many people chose any option.
    public var voters: Int

    /// The options you chose, in their order.
    public var mine: [String] {
        options.filter(\.mine).map(\.name)
    }

    public init(question: String, options: [Option], max: Int = 1, voters: Int = 0) {
        self.question = question
        self.options = options
        self.max = max
        self.voters = voters
    }

    public struct Option: Decodable, Sendable, Hashable {
        public var name: String
        /// How many chose it.
        public var votes: Int
        /// Whether you did.
        public var mine: Bool

        public init(name: String, votes: Int = 0, mine: Bool = false) {
            self.name = name
            self.votes = votes
            self.mine = mine
        }

        public init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: Keys.self)
            name = try c.decode(String.self, forKey: .name)
            votes = try c.decodeIfPresent(Int.self, forKey: .votes) ?? 0
            mine = try c.decodeIfPresent(Bool.self, forKey: .mine) ?? false
        }

        enum Keys: String, CodingKey {
            case name, votes, mine
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        question = try c.decode(String.self, forKey: .question)
        options = (try? c.decodeIfPresent([Option].self, forKey: .options)) ?? []
        max = try c.decodeIfPresent(Int.self, forKey: .max) ?? 1
        voters = try c.decodeIfPresent(Int.self, forKey: .voters) ?? 0
    }

    enum Keys: String, CodingKey {
        case question, options, max, voters
    }
}
