import Foundation

// Media in: how a message's file is, and `media.fetch`, `media.save` and `media.cancel`, as
// protocol/README.md's "Media in" has them and jaw-proto's media.rs sends and takes them.

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

/// `media.cancel`: stops the message's download, and anything waiting to be done for it.
public struct MediaCancel: Encodable, Sendable, Hashable {
    public var message: String
    public var chat: String?

    public init(message: String, chat: String? = nil) {
        self.message = message
        self.chat = chat
    }
}
