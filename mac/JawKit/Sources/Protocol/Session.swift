import Foundation

/// `hello`'s params: the first request on a connection.
public struct Hello: Encodable, Sendable, Hashable {
    public var `protocol`: Int
    public var client: String
    public var version: String
    public var focus: Bool
    public var since: UInt64?

    public init(client: String, version: String, focus: Bool, since: UInt64? = nil) {
        self.protocol = protocolVersion
        self.client = client
        self.version = version
        self.focus = focus
        self.since = since
    }
}

/// jawd's answer to hello.
public struct Welcome: Decodable, Sendable, Hashable {
    public var `protocol`: Int
    public var jawd: String
    /// jaw's folder: every `path` jawd sends is relative to it.
    public var root: String
    public var seq: UInt64
    public var account: Account
    public var connection: Connection
}

public struct Account: Decodable, Sendable, Hashable {
    public var state: AccountState
    public var me: Me?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        state = (try? c.decode(AccountState.self, forKey: .state)) ?? .linked
        me = try? c.decodeIfPresent(Me.self, forKey: .me)
    }

    enum Keys: String, CodingKey {
        case state, me
    }
}

public enum AccountState: String, Decodable, Sendable, Hashable {
    case unlinked, linking, linked

    /// A state jawd may add later reads as linked, so the app shows what jawd has.
    public init(from decoder: any Decoder) throws {
        self = try AccountState(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .linked
    }
}

public struct Me: Decodable, Sendable, Hashable {
    public var id: String
    public var name: String
    public var phone: String

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = (try? c.decode(String.self, forKey: .id)) ?? ""
        name = (try? c.decodeIfPresent(String.self, forKey: .name)) ?? ""
        phone = (try? c.decodeIfPresent(String.self, forKey: .phone)) ?? ""
    }

    enum Keys: String, CodingKey {
        case id, name, phone
    }
}

/// The connection to WhatsApp, as `hello`, `status` and the `connection` push say it.
public struct Connection: Decodable, Sendable, Hashable {
    public var state: ConnectionState
    public var done: UInt64?
    public var total: UInt64?
    public var until: Int64?
    public var reason: String?

    public init(state: ConnectionState, done: UInt64? = nil, total: UInt64? = nil, until: Int64? = nil,
                reason: String? = nil)
    {
        self.state = state
        self.done = done
        self.total = total
        self.until = until
        self.reason = reason
    }
}

public enum ConnectionState: String, Decodable, Sendable, Hashable {
    case connecting
    case online
    case catchingUp = "catching_up"
    case phoneOffline = "phone_offline"
    case offline
    case loggedOut = "logged_out"
    case updateNeeded = "update_needed"
    case banned
    case engineRestarting = "engine_restarting"
    /// A state jawd may add later.
    case other

    public init(from decoder: any Decoder) throws {
        self = try ConnectionState(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

/// `link.start`'s params: by QR code, or by a code typed on the phone for this number.
public struct LinkStart: Encodable, Sendable {
    public var phone: String?
    public init(phone: String? = nil) {
        self.phone = phone
    }
}

/// The `link` push, as linking goes.
public enum LinkStep: Decodable, Sendable, Hashable {
    case qr(code: String, expires: Int64)
    case code(String)
    case done(phone: String)
    case failed(reason: String)

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .state) {
        case "qr": self = try .qr(code: c.decode(String.self, forKey: .code), expires: c.decode(Int64.self, forKey: .expires))
        case "code": self = try .code(c.decode(String.self, forKey: .code))
        case "done": self = try .done(phone: c.decodeIfPresent(String.self, forKey: .phone) ?? "")
        default: self = try .failed(reason: c.decodeIfPresent(String.self, forKey: .reason) ?? "linking failed")
        }
    }

    enum Keys: String, CodingKey {
        case state, code, expires, phone, reason
    }
}

/// The `notice` push: something to say once.
public struct Notice: Decodable, Sendable, Hashable {
    public var text: String
    public var tone: Tone

    public enum Tone: String, Decodable, Sendable, Hashable {
        case info, warning, error
    }
}

/// `focus`'s params.
public struct Focus: Encodable, Sendable {
    public var focused: Bool
}

/// `status`'s answer: the parts the app shows, and the whole of it for jawd's status window.
public struct Status: Decodable, Sendable, Hashable {
    public var jawd: String
    public var `protocol`: Int
    public var root: String
    public var account: Account
    public var connection: Connection
    public var engine: Engine
    public var missing: [Missing]
    /// Everything `status` said, for the window that shows what `jaw status` prints.
    public var raw: JSONValue

    public struct Engine: Decodable, Sendable, Hashable {
        public var kind: String
        public var state: String
        public var version: String?
    }

    public struct Missing: Decodable, Sendable, Hashable {
        public var what: String
        public var neededFor: String

        enum CodingKeys: String, CodingKey {
            case what
            case neededFor = "needed_for"
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        jawd = try c.decode(String.self, forKey: .jawd)
        self.protocol = try c.decode(Int.self, forKey: .protocol)
        root = try c.decode(String.self, forKey: .root)
        account = try c.decode(Account.self, forKey: .account)
        connection = try c.decode(Connection.self, forKey: .connection)
        engine = try c.decode(Engine.self, forKey: .engine)
        missing = try c.decodeIfPresent([Missing].self, forKey: .missing) ?? []
        raw = try JSONValue(from: decoder)
    }

    enum Keys: String, CodingKey {
        case jawd, `protocol`, root, account, connection, engine, missing
    }
}
