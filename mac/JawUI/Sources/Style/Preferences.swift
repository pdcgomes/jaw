import AppKit
import JawThemes
import Observation

/// How messages look: bubbles, yours on the right, or lines, the terminal's layout.
public enum Look: String, CaseIterable, Sendable, Hashable {
    case bubbles, lines

    public var title: String {
        switch self {
        case .bubbles: "Bubbles"
        case .lines: "Lines"
        }
    }
}

public enum Density: String, CaseIterable, Sendable, Hashable {
    case comfortable, compact

    public var title: String {
        rawValue.capitalized
    }
}

public enum MessageFont: String, CaseIterable, Sendable, Hashable {
    case system, mono

    public var title: String {
        switch self {
        case .system: "The system's font"
        case .mono: "SF Mono"
        }
    }
}

/// What a message's layout depends on beyond the message: its look, its density, its text size
/// and font, and the theme's roles. A value, so layouts are made off the main thread.
public struct Style: Sendable, Hashable {
    public var roles: Roles
    public var look: Look
    public var density: Density
    public var textSize: CGFloat
    public var font: MessageFont

    public init(roles: Roles, look: Look = .bubbles, density: Density = .comfortable, textSize: CGFloat = 13,
                font: MessageFont = .system)
    {
        self.roles = roles
        self.look = look
        self.density = density
        self.textSize = textSize
        self.font = font
    }

    public var compact: Bool {
        density == .compact
    }
}

/// The app's settings, kept in the user's defaults. Changing one redraws every window at once.
@MainActor @Observable
public final class Preferences {
    public var look: Look { didSet { save() } }
    public var density: Density { didSet { save() } }
    public var textSize: CGFloat { didSet { save() } }
    public var font: MessageFont { didSet { save() } }
    public var appearance: Appearance { didSet { save() } }
    public var theme: String { didSet { save() } }

    @ObservationIgnored let defaults: UserDefaults
    @ObservationIgnored public var changed: [() -> Void] = []

    public static let textSizes: ClosedRange<CGFloat> = 11 ... 17

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        look = Look(rawValue: defaults.string(forKey: "look") ?? "") ?? .bubbles
        density = Density(rawValue: defaults.string(forKey: "density") ?? "") ?? .comfortable
        let size = defaults.double(forKey: "textSize")
        textSize = size == 0 ? 13 : min(max(size, Self.textSizes.lowerBound), Self.textSizes.upperBound)
        font = MessageFont(rawValue: defaults.string(forKey: "font") ?? "") ?? .system
        appearance = Appearance(rawValue: defaults.string(forKey: "appearance") ?? "") ?? .system
        theme = defaults.string(forKey: "theme") ?? Catalog.jaw.id
    }

    func save() {
        defaults.set(look.rawValue, forKey: "look")
        defaults.set(density.rawValue, forKey: "density")
        defaults.set(Double(textSize), forKey: "textSize")
        defaults.set(font.rawValue, forKey: "font")
        defaults.set(appearance.rawValue, forKey: "appearance")
        defaults.set(theme, forKey: "theme")
        for change in changed {
            change()
        }
    }

    /// Whether the app draws dark: the setting, or the system's when it's left to the system.
    public func isDark(system: NSAppearance?) -> Bool {
        switch appearance {
        case .dark: true
        case .light: false
        case .system: system?.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        }
    }

    public func style(dark: Bool) -> Style {
        Style(roles: Catalog.family(theme).roles(dark: dark), look: look, density: density, textSize: textSize,
              font: font)
    }

    public var nsAppearance: NSAppearance? {
        switch appearance {
        case .system: nil
        case .dark: NSAppearance(named: .darkAqua)
        case .light: NSAppearance(named: .aqua)
        }
    }
}

public extension RGBA {
    var nsColor: NSColor {
        NSColor(srgbRed: r, green: g, blue: b, alpha: a)
    }
}
