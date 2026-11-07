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

/// `themes`' result.
struct ServedThemes: Decodable {
    let themes: [ServedTheme]
}
