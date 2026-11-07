import Foundation

/// Where a theme comes from: Jaw's own, Omarchy's as jawd serves them, or Redlamp's, copied into jaw.
public enum ThemeSource: String, CaseIterable, Sendable, Hashable {
    case jaw, omarchy, redlamp

    /// Its section in the gallery, and what follows a theme's name, as in "Gruvbox, Omarchy's".
    public var title: String {
        switch self {
        case .jaw: "Jaw's"
        case .omarchy: "Omarchy's"
        case .redlamp: "Redlamp's"
        }
    }
}

/// The two slots a theme fills: the one Jaw draws while the Mac is dark, and the one while it's light.
public enum Slot: String, CaseIterable, Sendable, Hashable {
    case dark, light

    public var isDark: Bool {
        self == .dark
    }

    public var title: String {
        isDark ? "Dark" : "Light"
    }
}

/// A theme to choose: its ID, as a slot keeps it (`jaw`, `omarchy:<id>` or `redlamp:<id>`, since
/// Omarchy's and Redlamp's share names such as `nord`), its name, its light half's where it has one of
/// its own, its source, and its halves as the roles Jaw draws in, a dark one, a light one or both, each
/// held where JawUI draws it (`Roles.held()`), whichever source it comes from.
public struct Theme: Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    /// What its light half is called where that isn't `name`: Jaw's Tokyo Night Day.
    public var lightName: String?
    public var source: ThemeSource
    public var dark: Roles?
    public var light: Roles?

    public init(id: String, name: String, lightName: String? = nil, source: ThemeSource, dark: Roles?, light: Roles?) {
        self.id = id
        self.name = name
        self.lightName = lightName
        self.source = source
        self.dark = dark?.held()
        self.light = light?.held()
    }

    /// The half `slot` would take, where the theme has one.
    public func roles(_ slot: Slot) -> Roles? {
        slot.isDark ? dark : light
    }

    /// What the half `slot` would take is called, as the gallery's card and the slot's tile say it.
    public func name(_ slot: Slot) -> String {
        slot.isDark ? name : lightName ?? name
    }
}

/// One of Omarchy's themes as jawd serves it: its ID and its name as jawd gives them, its mode, and its
/// colours by Omarchy's keys, `#rrggbb`, as jawd sends them and the app remembers them.
public struct OmarchyTheme: Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    public var dark: Bool
    public var colours: [String: String]

    public init(id: String, name: String, dark: Bool, colours: [String: String]) {
        self.id = id
        self.name = name
        self.dark = dark
        self.colours = colours
    }

    /// One read from its `colors.toml`, named as jawd names it; nil where jaw-proto wouldn't read it.
    public init?(id: String, file text: String) {
        guard let read = Omarchy.file(text) else { return nil }
        self.init(id: id, name: Omarchy.name(id), dark: read.dark, colours: read.colours)
    }
}

/// One of the system's colours as AppKit draws it in a dark appearance and in a light one.
public struct SystemColour: Sendable, Hashable {
    public var dark: RGBA
    public var light: RGBA

    public init(dark: RGBA, light: RGBA) {
        self.dark = dark
        self.light = light
    }

    /// macOS's orange, green and red as AppKit draws them on macOS 27.0.1, as the build Mac does, which
    /// the tests and the harness draw Neutral in, so their pictures don't depend on the Mac's.
    public static let orange = SystemColour(dark: RGBA(0xFF9230), light: RGBA(0xFF8D28))
    public static let green = SystemColour(dark: RGBA(0x30D158), light: RGBA(0x34C759))
    public static let red = SystemColour(dark: RGBA(0xFF4245), light: RGBA(0xFF383C))
}

/// The system's colours Neutral takes, as AppKit draws them in a dark appearance and in a light one: the
/// accent chosen in System Settings, `dark` and `light`, and the orange, green and red its mentions,
/// online and failed take.
public struct SystemAccent: Sendable, Hashable {
    public var dark: RGBA
    public var light: RGBA
    public var orange: SystemColour
    public var green: SystemColour
    public var red: SystemColour

    public init(dark: RGBA, light: RGBA, orange: SystemColour = .orange, green: SystemColour = .green, red: SystemColour = .red) {
        self.dark = dark
        self.light = light
        self.orange = orange
        self.green = green
        self.red = red
    }

    /// macOS's blue, the accent until another is chosen in System Settings, which AppKit resolves to
    /// `007AFF` in both appearances on macOS 27.0.1, as the build Mac draws it, with macOS's orange,
    /// green and red: what the tests and the harness draw Neutral in, so their pictures don't depend on
    /// the Mac's.
    public static let blue = SystemAccent(dark: RGBA(0x007AFF), light: RGBA(0x007AFF))
}

public extension Omarchy {
    /// What jawd shows a theme as, as jaw-proto names it: its ID's words capitalised, and Rosé Pine
    /// with its accent.
    static func name(_ id: String) -> String {
        if id == "rose-pine" { return "Rosé Pine" }
        return id.split(separator: "-", omittingEmptySubsequences: false).map { $0.prefix(1).uppercased() + $0.dropFirst() }
            .joined(separator: " ")
    }
}

public extension Catalog {
    /// Jaw's Tokyo Night as a theme to choose, with both halves, its light one Tokyo Night Day, its ID
    /// `jaw` as before.
    static let jawTheme = Theme(id: jaw.id, name: "Tokyo Night", lightName: "Tokyo Night Day", source: .jaw, dark: jaw.dark,
                                light: jaw.light)

    /// Every theme there is to choose, in the gallery's order: Jaw's Tokyo Night; Omarchy's in the
    /// order jawd serves them, but for one whose colours don't map; and Redlamp's, Neutral in `accent`,
    /// then its families in Redlamp's order.
    static func themes(omarchy: [OmarchyTheme], accent: SystemAccent) -> [Theme] {
        let served = omarchy.compactMap { theme($0) }
        return [jawTheme] + served + [neutral(accent)] + redlampFamilies
    }

    /// The theme `id` names, from the source it names, or nil where that source hasn't it.
    static func theme(id: String, omarchy: [OmarchyTheme], accent: SystemAccent) -> Theme? {
        if id == jawTheme.id { return jawTheme }
        if id == "redlamp:neutral" { return neutral(accent) }
        if id.hasPrefix("omarchy:") { return omarchy.first { "omarchy:" + $0.id == id }.flatMap { theme($0) } }
        return RedlampCatalog.families.first { "redlamp:" + $0.id == id }.map { theme($0) }
    }

    /// What `slot` draws: its theme's half for it, or Jaw's Tokyo Night's where no source has the theme
    /// or it has no half of the slot's mode, as the terminal draws a theme it doesn't know.
    static func roles(_ slot: Slot, of slots: Slots, omarchy: [OmarchyTheme], accent: SystemAccent) -> Roles {
        theme(id: slots[slot], omarchy: omarchy, accent: accent)?.roles(slot) ?? jaw.roles(dark: slot.isDark)
    }
}

extension Catalog {
    /// One of Omarchy's, `omarchy:<id>`, with the half of its mode; nil where its colours don't map,
    /// which jawd's own reading of them keeps from happening.
    static func theme(_ o: OmarchyTheme) -> Theme? {
        guard let roles = Omarchy.roles(o.colours, dark: o.dark) else { return nil }
        return Theme(id: "omarchy:" + o.id, name: o.name, source: .omarchy, dark: o.dark ? roles : nil, light: o.dark ? nil : roles)
    }

    /// One of Redlamp's families, `redlamp:<id>`, with both halves.
    static func theme(_ family: RedlampCatalog.Family) -> Theme {
        Theme(id: "redlamp:" + family.id, name: family.name, source: .redlamp,
              dark: RedlampRoles.roles(family.dark, dark: true), light: RedlampRoles.roles(family.light, dark: false))
    }

    /// Neutral, `redlamp:neutral`, in the system's colours.
    static func neutral(_ accent: SystemAccent) -> Theme {
        Theme(id: "redlamp:neutral", name: "Neutral", source: .redlamp, dark: RedlampRoles.neutral(dark: true, accent: accent),
              light: RedlampRoles.neutral(dark: false, accent: accent))
    }

    /// Redlamp's families as themes, mapped once, as they never change while the app runs.
    static let redlampFamilies = RedlampCatalog.families.map { theme($0) }
}
