import Foundation

/// Any JSON, for the few places the protocol leaves open: an error's `data`, a push JawKit has no
/// type for, a `find` row of a kind it doesn't read.
public enum JSONValue: Sendable, Hashable, Codable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            self = .null
        } else if let b = try? c.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? c.decode(Double.self) {
            self = .number(n)
        } else if let s = try? c.decode(String.self) {
            self = .string(s)
        } else if let a = try? c.decode([JSONValue].self) {
            self = .array(a)
        } else {
            self = try .object(c.decode([String: JSONValue].self))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case let .bool(b): try c.encode(b)
        case let .number(n):
            if n == n.rounded(), abs(n) < 9_007_199_254_740_992 {
                try c.encode(Int64(n))
            } else {
                try c.encode(n)
            }
        case let .string(s): try c.encode(s)
        case let .array(a): try c.encode(a)
        case let .object(o): try c.encode(o)
        }
    }

    public subscript(key: String) -> JSONValue? {
        if case let .object(o) = self { o[key] } else { nil }
    }

    public var string: String? {
        if case let .string(s) = self { s } else { nil }
    }

    public var int: Int? {
        if case let .number(n) = self { Int(exactly: n.rounded()) } else { nil }
    }

    public var array: [JSONValue]? {
        if case let .array(a) = self { a } else { nil }
    }

    public var bool: Bool? {
        if case let .bool(b) = self { b } else { nil }
    }

    /// Reads JSON text.
    public static func parse(_ data: Data) throws -> JSONValue {
        try JSONDecoder().decode(JSONValue.self, from: data)
    }
}
