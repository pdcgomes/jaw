import AppKit

/// A command's key, as a menu item takes it and as the app shows it beside the command.
///
/// A letter with Shift is given to the menu as its capital: the key press reports `D` for ⌘⇧D, and
/// a menu item written `d` with Shift in its mask never matches it.
public struct Shortcut: Hashable, Sendable {
    /// The key: a letter, a digit, a symbol, `↩`, `⌫`, `⎋`, or an arrow (`←`, `→`, `↑`, `↓`).
    public let key: String
    let mask: UInt

    public var modifiers: NSEvent.ModifierFlags {
        NSEvent.ModifierFlags(rawValue: mask)
    }

    public init(_ key: String, _ modifiers: NSEvent.ModifierFlags = .command) {
        self.key = key
        mask = modifiers.intersection([.command, .shift, .option, .control]).rawValue
    }

    /// As Apple's menus show it: ⌃⌥⇧⌘ and the key.
    public var display: String {
        var out = ""
        if modifiers.contains(.control) { out += "⌃" }
        if modifiers.contains(.option) { out += "⌥" }
        if modifiers.contains(.shift) { out += "⇧" }
        if modifiers.contains(.command) { out += "⌘" }
        return out + (key.count == 1 && key.first!.isLetter ? key.uppercased() : key)
    }

    /// The key equivalent a menu item is given.
    public var menuKey: String {
        switch key {
        case "↩": return "\r"
        case "⌫": return "\u{8}"
        case "⎋": return "\u{1b}"
        case "←": return String(UnicodeScalar(NSLeftArrowFunctionKey)!)
        case "→": return String(UnicodeScalar(NSRightArrowFunctionKey)!)
        case "↑": return String(UnicodeScalar(NSUpArrowFunctionKey)!)
        case "↓": return String(UnicodeScalar(NSDownArrowFunctionKey)!)
        default:
            return modifiers.contains(.shift) && key.count == 1 && key.first!.isLetter ? key.uppercased() : key.lowercased()
        }
    }
}
