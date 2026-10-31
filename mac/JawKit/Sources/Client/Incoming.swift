import Foundation

/// What a request's result is read as, so each line is decoded once, into the value it is.
public enum Expect: Sendable, Hashable {
    case json, sent, chat, info, chatInfo, status, layout
}

/// A request's answer, read as what it was asked for.
public enum Reply: Sendable, Hashable {
    case json(JSONValue)
    case sent(Sent)
    case chat(ChatRow)
    case info(MessageInfo)
    case chatInfo(ChatInfo)
    case status(Status)
    case layout(KeptLayout)
}

/// What the reader expects of the reply with an id: looked up as the line decodes.
enum Want: Sendable {
    case hello, find, chats, messages, nothing
    case reply(Expect)
}

struct WantLookup: Sendable {
    let want: @Sendable (UInt64) -> Want
}

extension CodingUserInfoKey {
    static let want = CodingUserInfoKey(rawValue: "jaw.want")!
}

/// A line from jawd, read as what it is.
enum Incoming: Sendable {
    case hello(UInt64, Welcome)
    case reply(UInt64, Reply)
    case found(UInt64, Found)
    case chats(UInt64, ChatsWatched)
    case messages(UInt64, MessagesWatched)
    case error(UInt64, JawError)
    /// A reply that isn't jaw's protocol, and why.
    case bad(UInt64, String)
    case chatsPush(ChatsPush)
    case messagesPush(MessagesPush)
    case typing(TypingPush)
    case resync(String)
    case connection(Connection)
    case link(LinkStep)
    case notice(Notice)
    case push(String, JSONValue)
    /// A line that isn't jaw's protocol.
    case skip
    /// A push that isn't jaw's protocol, and why, to be said.
    case badPush(String, String)
}

struct IncomingLine: Decodable {
    let value: Incoming

    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        if let id = try c.decodeIfPresent(UInt64.self, forKey: .id) {
            if let error = try c.decodeIfPresent(JawError.self, forKey: .error) {
                value = .error(id, error)
                return
            }
            guard c.contains(.result) else {
                value = .bad(id, "jawd answered with neither a result nor an error")
                return
            }
            let want = (decoder.userInfo[.want] as? WantLookup)?.want(id) ?? .nothing
            do {
                switch want {
                case .hello: value = try .hello(id, c.decode(Welcome.self, forKey: .result))
                case .find: value = try .found(id, c.decode(Found.self, forKey: .result))
                case .chats: value = try .chats(id, c.decode(ChatsWatched.self, forKey: .result))
                case .messages: value = try .messages(id, c.decode(MessagesWatched.self, forKey: .result))
                case .nothing: value = .reply(id, .json(.null))
                case .reply(.json): value = try .reply(id, .json(c.decode(JSONValue.self, forKey: .result)))
                case .reply(.sent): value = try .reply(id, .sent(c.decode(Sent.self, forKey: .result)))
                case .reply(.chat): value = try .reply(id, .chat(c.decode(ChatStarted.self, forKey: .result).chat))
                case .reply(.info): value = try .reply(id, .info(c.decode(MessageInfo.self, forKey: .result)))
                case .reply(.chatInfo): value = try .reply(id, .chatInfo(c.decode(ChatInfo.self, forKey: .result)))
                case .reply(.status): value = try .reply(id, .status(c.decode(Status.self, forKey: .result)))
                case .reply(.layout): value = try .reply(id, .layout(c.decode(KeptLayout.self, forKey: .result)))
                }
            } catch {
                value = .bad(id, "what jawd sent isn't jaw's protocol: \(error)")
            }
            return
        }
        guard let method = try c.decodeIfPresent(String.self, forKey: .method) else {
            value = .skip
            return
        }
        do {
            switch method {
            case "chats": value = try .chatsPush(c.decode(ChatsPush.self, forKey: .params))
            case "messages": value = try .messagesPush(c.decode(MessagesPush.self, forKey: .params))
            case "typing": value = try .typing(c.decode(TypingPush.self, forKey: .params))
            case "resync": value = try .resync(c.decode(WatchRef.self, forKey: .params).watch)
            case "connection": value = try .connection(c.decode(Connection.self, forKey: .params))
            case "link": value = try .link(c.decode(LinkStep.self, forKey: .params))
            case "notice": value = try .notice(c.decode(Notice.self, forKey: .params))
            default: value = try .push(method, c.decodeIfPresent(JSONValue.self, forKey: .params) ?? .null)
            }
        } catch {
            value = .badPush(method, "\(error)")
        }
    }

    enum Keys: String, CodingKey {
        case id, method, params, result, error
    }

    /// Reads one line, looking up what each reply was asked for with `want`.
    static func decode(_ line: Data, with decoder: JSONDecoder) -> Incoming {
        (try? decoder.decode(IncomingLine.self, from: line).value) ?? .skip
    }
}
