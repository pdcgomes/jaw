import Foundation

/// One of Omarchy's themes as `themes` serves it, as jaw-proto's `Theme` has it: its ID, the name it's
/// shown as, its mode, and its 25 colours by Omarchy's keys, `#rrggbb`.
public struct ServedTheme: Codable, Sendable, Hashable {
    public enum Mode: String, Codable, Sendable, Hashable {
        case dark, light
    }

    public var id: String
    public var name: String
    public var mode: Mode
    public var colors: [String: String]

    public init(id: String, name: String, mode: Mode, colors: [String: String]) {
        self.id = id
        self.name = name
        self.mode = mode
        self.colors = colors
    }
}

/// `themes`' result, read theme by theme as a window's rows are: a theme that isn't jaw's protocol, a
/// mode this Mac doesn't know or a colour missing, is left out rather than losing the rest with it, its
/// ID kept where it has one, as a slot holding it keeps its copy.
struct ServedThemes: Decodable {
    let themes: [ServedTheme]
    /// The IDs of the themes left out.
    let unread: [String]

    init(from decoder: any Decoder) throws {
        var list = try decoder.container(keyedBy: Keys.self).nestedUnkeyedContainer(forKey: .themes)
        var themes: [ServedTheme] = [], unread: [String] = []
        while !list.isAtEnd {
            if let theme = try? list.decode(ServedTheme.self) {
                themes.append(theme)
            } else if let named = try? list.decode(Named.self) {
                unread.append(named.id)
            } else {
                _ = try? list.decode(JSONValue.self)
            }
        }
        (self.themes, self.unread) = (themes, unread)
    }

    /// A theme left out, by the ID it may still have.
    struct Named: Decodable {
        let id: String
    }

    enum Keys: String, CodingKey {
        case themes
    }
}
