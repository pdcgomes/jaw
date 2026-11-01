import Foundation

// The library: every photo, video, voice message, file and link shared, across all chats or in one,
// as a window jawd keeps, as protocol/README.md's "Library" has it and jaw-proto's library.rs sends
// and takes it.

/// What an item is.
public enum LibraryKind: String, Codable, Sendable, Hashable {
    case photo
    /// Videos, GIFs and video messages.
    case video
    case voice
    /// Documents and audio files.
    case file
    case link
    /// A kind jawd adds later, which the library leaves out.
    case other

    /// The kinds the library shows, in the strip's order.
    public static let all: [LibraryKind] = [.photo, .video, .voice, .file, .link]

    public init(from decoder: any Decoder) throws {
        self = try LibraryKind(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

/// A voice message: how long it is, and, once jawd sends them (the Mac design's P14), its waveform and
/// whether you've played it.
public struct LibraryVoice: Decodable, Sendable, Hashable {
    public var seconds: Double?
    /// 64 values from 0 to 100, as WhatsApp sends them; empty until jawd sends them.
    public var waveform: [Int]
    public var played: Bool?

    public init(seconds: Double?, waveform: [Int] = [], played: Bool? = nil) {
        self.seconds = seconds
        self.waveform = waveform
        self.played = played
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        seconds = try? c.decodeIfPresent(Double.self, forKey: .seconds)
        waveform = (try? c.decodeIfPresent([Int].self, forKey: .waveform)) ?? []
        played = try? c.decodeIfPresent(Bool.self, forKey: .played)
    }

    enum Keys: String, CodingKey {
        case seconds, waveform, played
    }
}

/// A file: its name (an audio file, which has none, goes by its line, "Audio, 3:02"), what it is as
/// shown, its size and pages, and its path once it's here; and, once jawd sends them (P14), how its
/// copy is, as a message row's media says it.
public struct LibraryFile: Decodable, Sendable, Hashable {
    public var name: String
    /// `PDF`, `GPX`: from its name, or its MIME type.
    public var type: String?
    public var bytes: Int64?
    public var pages: Int?
    public var path: String?
    public var state: MediaState?
    public var progress: Double?
    public var error: String?

    public init(name: String, type: String? = nil, bytes: Int64? = nil, pages: Int? = nil, path: String? = nil) {
        self.name = name
        self.type = type
        self.bytes = bytes
        self.pages = pages
        self.path = path
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        name = (try? c.decodeIfPresent(String.self, forKey: .name)) ?? ""
        type = try? c.decodeIfPresent(String.self, forKey: .type)
        bytes = try? c.decodeIfPresent(Int64.self, forKey: .bytes)
        pages = try? c.decodeIfPresent(Int.self, forKey: .pages)
        path = try? c.decodeIfPresent(String.self, forKey: .path)
        state = try? c.decodeIfPresent(MediaState.self, forKey: .state)
        progress = try? c.decodeIfPresent(Double.self, forKey: .progress)
        error = try? c.decodeIfPresent(String.self, forKey: .error)
    }

    enum Keys: String, CodingKey {
        case name, type, bytes, pages, path, state, progress, error
    }
}

/// A link: where it goes, made whole, its host, its preview's words where it had one, and, once
/// jawd sends it (P14), its preview's thumbnail.
public struct LibraryLink: Decodable, Sendable, Hashable {
    public var url: String
    public var host: String
    public var title: String?
    public var description: String?
    /// The thumbnail's path under jawd's root, as a message row's link has it.
    public var preview: String?

    public init(url: String, host: String, title: String? = nil, description: String? = nil, preview: String? = nil) {
        self.url = url
        self.host = host
        self.title = title
        self.description = description
        self.preview = preview
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        url = try c.decode(String.self, forKey: .url)
        host = (try? c.decodeIfPresent(String.self, forKey: .host)) ?? ""
        title = try? c.decodeIfPresent(String.self, forKey: .title)
        description = try? c.decodeIfPresent(String.self, forKey: .description)
        preview = try? c.decodeIfPresent(String.self, forKey: .preview)
    }

    enum Keys: String, CodingKey {
        case url, host, title, description, preview
    }
}

/// An item, as the library shows it: a message's photo, video, voice message or file, or one of the
/// links it gives. A library window reads from the largest `sort` down, newest first.
public struct LibraryRow: WindowRow, Decodable, Hashable {
    /// Its message's ID, and `~` and its place among the message's links for a link.
    public var id: String
    /// Its message's version.
    public var v: UInt64
    public var sort: String
    public var kind: LibraryKind
    /// Its message, to go to.
    public var message: String
    public var chat: String
    public var chatName: String
    /// Who sent it; absent on yours.
    public var from: Sender?
    public var mine: Bool
    public var at: Int64
    /// Its message's caption on one line, with the words the window's text matched marked.
    public var caption: [Span]
    /// A photo's or a video's, as message rows have it.
    public var media: Media?
    public var voice: LibraryVoice?
    public var file: LibraryFile?
    public var link: LibraryLink?

    public init(id: String, v: UInt64, sort: String, kind: LibraryKind, message: String? = nil, chat: String, chatName: String,
                from: Sender? = nil, mine: Bool = false, at: Int64, caption: [Span] = [], media: Media? = nil,
                voice: LibraryVoice? = nil, file: LibraryFile? = nil, link: LibraryLink? = nil)
    {
        self.id = id
        self.v = v
        self.sort = sort
        self.kind = kind
        self.message = message ?? id
        self.chat = chat
        self.chatName = chatName
        self.from = from
        self.mine = mine
        self.at = at
        self.caption = caption
        self.media = media
        self.voice = voice
        self.file = file
        self.link = link
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        v = try c.decode(UInt64.self, forKey: .v)
        sort = try c.decode(String.self, forKey: .sort)
        kind = try c.decodeIfPresent(LibraryKind.self, forKey: .kind) ?? .other
        message = try c.decodeIfPresent(String.self, forKey: .message) ?? id
        chat = try c.decodeIfPresent(String.self, forKey: .chat) ?? ""
        chatName = try c.decodeIfPresent(String.self, forKey: .chatName) ?? ""
        from = try? c.decodeIfPresent(Sender.self, forKey: .from)
        mine = try c.decodeIfPresent(Bool.self, forKey: .mine) ?? false
        at = try c.decodeIfPresent(Int64.self, forKey: .at) ?? 0
        caption = (try? c.decodeIfPresent([Span].self, forKey: .caption)) ?? []
        media = try? c.decodeIfPresent(Media.self, forKey: .media)
        voice = try? c.decodeIfPresent(LibraryVoice.self, forKey: .voice)
        file = try? c.decodeIfPresent(LibraryFile.self, forKey: .file)
        link = try? c.decodeIfPresent(LibraryLink.self, forKey: .link)
    }

    enum Keys: String, CodingKey {
        case id, v, sort, kind, message, chat, from, mine, at, caption, media, voice, file, link
        case chatName = "chat_name"
    }
}

/// How many items of each kind match a window's filters but its kind, for the strip: exact for a kind
/// across chats or in one, counted up to `counted` with a person, a time or words.
public struct LibraryCounts: Decodable, Sendable, Hashable {
    public var photo: Int
    public var video: Int
    public var voice: Int
    public var file: Int
    /// Absent from a jawd from before links were kept.
    public var link: Int?

    /// The most of a kind jawd counts when a window has a filter other than its chat; that many means
    /// at least that many.
    public static let counted = 1000

    public init(photo: Int = 0, video: Int = 0, voice: Int = 0, file: Int = 0, link: Int? = 0) {
        self.photo = photo
        self.video = video
        self.voice = voice
        self.file = file
        self.link = link
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        photo = try c.decodeIfPresent(Int.self, forKey: .photo) ?? 0
        video = try c.decodeIfPresent(Int.self, forKey: .video) ?? 0
        voice = try c.decodeIfPresent(Int.self, forKey: .voice) ?? 0
        file = try c.decodeIfPresent(Int.self, forKey: .file) ?? 0
        link = try c.decodeIfPresent(Int.self, forKey: .link)
    }

    enum Keys: String, CodingKey {
        case photo, video, voice, file, link
    }

    public subscript(_ kind: LibraryKind) -> Int {
        switch kind {
        case .photo: photo
        case .video: video
        case .voice: voice
        case .file: file
        case .link: link ?? 0
        case .other: 0
        }
    }

    /// Every kind's together.
    public var shared: Int {
        photo + video + voice + file + (link ?? 0)
    }
}

/// What a library window holds: one kind, or the newest of every kind, and what narrows them. Every
/// filter can be absent.
public struct LibraryFilter: Sendable, Hashable {
    /// Nil for every kind, which isn't paged: it holds the newest of each.
    public var kind: LibraryKind?
    /// One chat's items, by its id.
    public var chat: String?
    /// One sender's: `person:` and a number, or `me` for yours.
    public var person: String?
    /// Items sent at or after this time, in milliseconds since the epoch, and at or before this one.
    public var since: Int64?
    public var until: Int64?
    /// Words each item must have, matched as `find` matches messages.
    public var text: String

    public init(kind: LibraryKind? = nil, chat: String? = nil, person: String? = nil, since: Int64? = nil, until: Int64? = nil,
                text: String = "")
    {
        self.kind = kind
        self.chat = chat
        self.person = person
        self.since = since
        self.until = until
        self.text = text
    }
}

/// `library.watch`'s params: the filter, and where the window is.
struct LibraryWatch: Encodable, Sendable {
    var filter: LibraryFilter
    var around: Around
    var before: UInt32
    var after: UInt32

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(filter.kind?.rawValue ?? "all", forKey: .kind)
        try c.encodeIfPresent(filter.chat, forKey: .chat)
        try c.encodeIfPresent(filter.person, forKey: .person)
        try c.encodeIfPresent(filter.since, forKey: .since)
        try c.encodeIfPresent(filter.until, forKey: .until)
        if !filter.text.isEmpty { try c.encode(filter.text, forKey: .text) }
        try c.encode(around, forKey: .around)
        try c.encode(before, forKey: .before)
        try c.encode(after, forKey: .after)
    }

    enum Keys: String, CodingKey {
        case kind, chat, person, since, until, text, around, before, after
    }
}

/// What `library.watch` returns, and `watch.move` on a library window.
public struct LibraryWatched: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    /// How many items of the window's kind match, as `counts` counts them; every kind's for `all`.
    public var total: UInt64
    /// Whether more items come after the window's last row.
    public var older: Bool
    public var counts: LibraryCounts
    public var rows: [LibraryRow]
    public var remove: [String]
    /// What the window can't hold yet and why, in words to show.
    public var note: String?
    /// Rows that weren't jaw's protocol, left out, and why the first wasn't.
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total) ?? 0
        older = try c.decodeIfPresent(Bool.self, forKey: .older) ?? false
        counts = (try? c.decodeIfPresent(LibraryCounts.self, forKey: .counts)) ?? LibraryCounts(link: nil)
        rows = try c.rows(.rows, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
        note = try c.decodeIfPresent(String.self, forKey: .note)
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, older, counts, rows, remove, note
    }
}

/// The `library` push: items that came or changed, those that went, and `total`, `older` and
/// `counts` when they changed.
public struct LibraryPush: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64?
    public var older: Bool?
    public var counts: LibraryCounts?
    public var upsert: [LibraryRow]
    public var remove: [String]
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total)
        older = try c.decodeIfPresent(Bool.self, forKey: .older)
        counts = try? c.decodeIfPresent(LibraryCounts.self, forKey: .counts)
        upsert = try c.rows(.upsert, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, older, counts, upsert, remove
    }
}

/// `library.shown`'s params: the items an interface has on screen, by their `id`, so jawd asks media
/// for what they lack.
public struct LibraryShown: Encodable, Sendable, Hashable {
    public var items: [String]

    /// The most it names at once; more is error 5.
    public static let most = 200

    public init(items: [String]) {
        self.items = items
    }
}
