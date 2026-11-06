import Foundation

// Media in: how a message's file is, and `media.fetch`, `media.save` and `media.cancel`; and media
// out: `send.media` and its `job` pushes; as protocol/README.md's "Media in" and "Media out" have
// them and jaw-proto's media.rs sends and takes them.

/// Where a message's file is, as its row's `media.state` says.
public enum MediaState: String, Decodable, Sendable, Hashable {
    /// Not here, and it can be fetched; with `error`, a download that failed and can be tried again.
    case remote
    /// Coming, `progress` of it.
    case downloading
    case local
    /// WhatsApp's server no longer has it, or the phone asked for it didn't answer; `error` says which.
    case expired
    /// The phone was asked to send it again.
    case waitingForPhone = "waiting_for_phone"
    /// The phone no longer has it either.
    case gone

    /// A state jawd may add later reads as remote, since nothing says the file is here.
    public init(from decoder: any Decoder) throws {
        self = try MediaState(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .remote
    }
}

/// Which copy of a message's file: the one every message has, or the HD copy a photo was sent with.
public enum Variant: String, Codable, Sendable, Hashable {
    case sd, hd
}

/// `media.fetch`: downloads the copy now, ahead of what downloads by itself.
public struct MediaFetch: Encodable, Sendable, Hashable {
    public var message: String
    public var chat: String?
    public var variant: Variant?

    public init(message: String, chat: String? = nil, variant: Variant? = nil) {
        self.message = message
        self.chat = chat
        self.variant = variant
    }
}

/// `media.save`: copies the file to `to`, a folder or a file's path, whole or from `~/`,
/// downloading it first if it isn't here yet.
public struct MediaSave: Encodable, Sendable, Hashable {
    public var message: String
    public var to: String
    public var chat: String?
    public var variant: Variant?

    public init(message: String, to: String, chat: String? = nil, variant: Variant? = nil) {
        self.message = message
        self.to = to
        self.chat = chat
        self.variant = variant
    }
}

/// `media.save`'s answer: where the copy is, whole and synced.
public struct Saved: Decodable, Sendable, Hashable {
    public var path: String
}

/// `media.cancel`: stops the message's download, and anything waiting to be done for it; on a file of
/// yours still being made or uploaded, its sending.
public struct MediaCancel: Encodable, Sendable, Hashable {
    public var message: String
    public var chat: String?

    public init(message: String, chat: String? = nil) {
        self.message = message
        self.chat = chat
    }
}

/// What a file goes as; left out, jawd sends it as what it is.
public enum MediaKind: String, Codable, Sendable, Hashable {
    case image, video, gif, audio, document
}

/// `send.media`'s params: files sent to a chat, a message each, in the order given, each with its
/// caption; with `quality` `hd`, an HD copy of each photo and video that has more to give.
public struct SendMedia: Encodable, Sendable, Hashable {
    public var chat: String
    public var files: [File]
    public var quality: Variant
    public var reply: String?
    public var key: String?

    /// The most files one `send.media` takes, as the phones let you pick.
    public static let filesMost = 30

    public struct File: Encodable, Sendable, Hashable {
        /// Whole, or from your home folder with `~/`.
        public var path: String
        public var kind: MediaKind?
        public var caption: String?

        public init(path: String, kind: MediaKind? = nil, caption: String? = nil) {
            self.path = path
            self.kind = kind
            self.caption = caption
        }

        public func encode(to encoder: any Encoder) throws {
            var c = encoder.container(keyedBy: Keys.self)
            try c.encode(path, forKey: .path)
            try c.encodeIfPresent(kind, forKey: .kind)
            try c.encodeIfPresent(caption, forKey: .caption)
        }

        enum Keys: String, CodingKey {
            case path, kind, caption
        }
    }

    public init(chat: String, files: [File], quality: Variant, reply: String? = nil, key: String?) {
        self.chat = chat
        self.files = files
        self.quality = quality
        self.reply = reply
        self.key = key
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(chat, forKey: .chat)
        try c.encode(files, forKey: .files)
        try c.encode(quality, forKey: .quality)
        try c.encodeIfPresent(reply, forKey: .reply)
        try c.encodeIfPresent(key, forKey: .key)
    }

    enum Keys: String, CodingKey {
        case chat, files, quality, reply, key
    }
}

/// `send.media`'s answer, once every message is on disk: the files' messages, in the order sent, the
/// album's when two or more photos and videos went, and their rows, the album's first.
public struct SentMedia: Decodable, Sendable, Hashable {
    public var messages: [String]
    public var album: String?
    public var rows: [MessageRow]
}

/// A `job` push: how far a copy of one of your files has got, as it's made and then uploaded.
public struct Job: Decodable, Sendable, Hashable {
    public var id: String
    public var kind: Kind
    /// The copy's message; an HD copy's is its SD message's.
    public var message: String
    public var variant: Variant?
    /// 0 to 1.
    public var progress: Double

    public enum Kind: String, Decodable, Sendable, Hashable {
        case transcode, upload, download
        /// One jawd may add later.
        case other

        public init(from decoder: any Decoder) throws {
            self = try Kind(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .other
        }
    }

    public init(id: String, kind: Kind, message: String, variant: Variant? = nil, progress: Double) {
        self.id = id
        self.kind = kind
        self.message = message
        self.variant = variant
        self.progress = progress
    }
}
