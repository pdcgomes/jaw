import Foundation

/// The protocol version this client speaks; jawd refuses any other with error 6.
public let protocolVersion = 2

/// jawd's answer to a request it didn't do, with a sentence an interface can show as it is.
public struct JawError: Error, Sendable, Hashable, Codable {
    public var code: Int
    public var message: String
    public var data: JSONValue?

    public init(code: Int, message: String, data: JSONValue? = nil) {
        self.code = code
        self.message = message
        self.data = data
    }
}

/// The protocol's error codes.
public enum ErrorCode {
    public static let parse = -32700
    public static let invalidRequest = -32600
    public static let unknownMethod = -32601
    public static let invalidParams = -32602
    public static let `internal` = -32603
    public static let notLinked = 1
    public static let engineUnavailable = 2
    public static let notFound = 3
    public static let notAllowed = 4
    public static let limit = 5
    public static let version = 6
    public static let cancelled = 7
}

/// Params that are an empty object, for methods that take none but are answered with `{}`.
public struct NoParams: Encodable, Sendable {
    public init() {}
}

/// A request line: an id the client picks, a method and its params, which are left out when nil.
struct RequestLine<P: Encodable>: Encodable {
    let id: UInt64
    let method: String
    let params: P?

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(id, forKey: .id)
        try c.encode(method, forKey: .method)
        try c.encodeIfPresent(params, forKey: .params)
    }

    enum Keys: String, CodingKey {
        case id, method, params
    }
}

enum Lines {
    /// One request as a line, newline included.
    static func encode(id: UInt64, method: String, params: (some Encodable)?) -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        var data = (try? encoder.encode(RequestLine(id: id, method: method, params: params))) ?? Data()
        data.append(0x0A)
        return data
    }
}
