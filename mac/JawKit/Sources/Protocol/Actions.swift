import Foundation

/// The params of the methods that send and act, as jaw-proto's `send.rs` has them.

public struct SendText: Encodable, Sendable, Hashable {
    public var chat: String
    public var text: String
    public var key: String?
    public var reply: String?
    public var mentions: [String]

    public init(chat: String, text: String, key: String?, reply: String? = nil, mentions: [String] = []) {
        self.chat = chat
        self.text = text
        self.key = key
        self.reply = reply
        self.mentions = mentions
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(chat, forKey: .chat)
        try c.encode(text, forKey: .text)
        try c.encodeIfPresent(key, forKey: .key)
        try c.encodeIfPresent(reply, forKey: .reply)
        if !mentions.isEmpty { try c.encode(mentions, forKey: .mentions) }
    }

    enum Keys: String, CodingKey {
        case chat, text, key, reply, mentions
    }
}

/// `send.poll`'s params: a poll's question, its options in their order, each named once, and how many
/// of them one vote may choose; it's answered as `send.text` is.
public struct SendPoll: Encodable, Sendable, Hashable {
    public var chat: String
    public var question: String
    public var options: [String]
    public var max: Int
    public var key: String?

    public init(chat: String, question: String, options: [String], max: Int = 1, key: String?) {
        self.chat = chat
        self.question = question
        self.options = options
        self.max = max
        self.key = key
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(chat, forKey: .chat)
        try c.encode(question, forKey: .question)
        try c.encode(options, forKey: .options)
        try c.encode(max, forKey: .max)
        try c.encodeIfPresent(key, forKey: .key)
    }

    enum Keys: String, CodingKey {
        case chat, question, options, max, key
    }
}

/// `poll.vote`'s params: your vote in poll `message`, every option you choose now by name, none to
/// take your vote back.
public struct PollVote: Encodable, Sendable, Hashable {
    public var message: String
    public var options: [String]
    public var chat: String?
    public var key: String?
}

/// `send.text`'s answer, and `send.poll`'s.
public struct Sent: Decodable, Sendable, Hashable {
    public var message: String
    public var row: MessageRow?
}

public struct MessageRef: Encodable, Sendable, Hashable {
    public var message: String
    public var chat: String?
}

public struct ChatRef: Encodable, Sendable, Hashable {
    public var chat: String
}

public struct ChatStart: Encodable, Sendable, Hashable {
    public var phone: String?
    public var person: String?
}

struct ChatStarted: Decodable, Sendable {
    var chat: ChatRow
}

public struct DraftSet: Encodable, Sendable, Hashable {
    public var chat: String
    public var text: String
}

public struct ReadUpTo: Encodable, Sendable, Hashable {
    public var chat: String
    public var upTo: String

    enum CodingKeys: String, CodingKey {
        case chat
        case upTo = "up_to"
    }
}

public enum TypingState: String, Encodable, Sendable, Hashable {
    case composing, recording, paused
}

public struct Typing: Encodable, Sendable, Hashable {
    public var chat: String
    public var state: TypingState
}

public struct React: Encodable, Sendable, Hashable {
    public var message: String
    public var emoji: String
    public var chat: String?
    public var key: String?
}

public struct Edit: Encodable, Sendable, Hashable {
    public var message: String
    public var text: String
    public var chat: String?
    public var key: String?
}

public enum DeleteFor: String, Encodable, Sendable, Hashable {
    case me, everyone
}

public struct Delete: Encodable, Sendable, Hashable {
    public var message: String
    public var `for`: DeleteFor
    public var chat: String?
    public var key: String?
}

public struct Forward: Encodable, Sendable, Hashable {
    public var messages: [String]
    public var to: [String]
    public var chat: String?
    public var key: String?
}

/// `message.info`'s answer.
public struct MessageInfo: Decodable, Sendable, Hashable {
    public var message: String
    public var sent: Int64?
    public var recipients: [Recipient]

    public struct Recipient: Decodable, Sendable, Hashable {
        public var person: String
        public var name: String
        public var delivered: Int64?
        public var read: Int64?
        public var played: Int64?
    }
}

/// `chat.info`'s answer.
public struct ChatInfo: Decodable, Sendable, Hashable {
    public var chat: String
    public var kind: String
    public var members: [Member]
    public var community: String?
    public var ephemeral: Int

    public struct Member: Decodable, Sendable, Hashable {
        public var person: String
        public var name: String
        public var phone: String?
        public var admin: Bool
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        chat = try c.decode(String.self, forKey: .chat)
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? ""
        members = try c.decodeIfPresent([Member].self, forKey: .members) ?? []
        community = try c.decodeIfPresent(String.self, forKey: .community)
        ephemeral = try c.decodeIfPresent(Int.self, forKey: .ephemeral) ?? 0
    }

    enum Keys: String, CodingKey {
        case chat, kind, members, community, ephemeral
    }
}
