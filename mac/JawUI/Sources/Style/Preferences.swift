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
    /// The theme in each slot, by ID, kept as `themeDark` and `themeLight`. Any change but the
    /// gallery's arrows' own ends their run (`arrowsFrom`).
    public var slots: Slots {
        didSet {
            made = [:]
            listed = nil
            arrowsFrom = nil
            remember()
            save()
        }
    }

    /// The system's colours in each appearance, its accent, orange, green and red, which Neutral draws
    /// in, read again by `systemColoursChanged()`, and not kept, as they're the system's.
    public var accent: SystemAccent {
        didSet {
            guard accent != oldValue else { return }
            made = [:]
            listed = nil
            tell()
        }
    }

    /// Omarchy's themes as jawd served them this run, nil until it has. They stay when jawd goes away,
    /// as its answer still holds. The app is told only when what a slot draws changes, so jawd's first
    /// answer, which comes as a launch's first rows do, restyles no window while the slots hold Jaw's or
    /// Redlamp's themes or a copy whose colours jawd serves as they were; Settings, which observes
    /// this, redraws either way.
    public var served: [OmarchyTheme]? = nil {
        didSet {
            guard served != oldValue else { return }
            let drawn = Slot.allCases.map { made[$0] ?? Catalog.roles($0, of: slots, omarchy: oldValue ?? remembered(slots), accent: accent) }
            made = [:]
            listed = nil
            remember()
            if Slot.allCases.map({ roles($0) }) != drawn { tell() }
        }
    }

    /// What the gallery's Omarchy section says while jawd hasn't served its themes.
    public var omarchyState: OmarchyState = .connecting

    /// The slot the gallery chooses for, nil for the one in use; Settings sets it back each time it
    /// opens, and it isn't kept. Setting it ends the arrows' run.
    public var choosing: Slot? = nil {
        didSet { arrowsFrom = nil }
    }

    /// The slots as they were at the first arrow of a run through the gallery, which each arrow previews
    /// from (`preview(_:for:)`), nil between runs; any other change of the slots ends a run, as does a
    /// click in the pane, choosing for another slot and Settings opening. Not kept.
    public var arrowsFrom: Slots? = nil {
        didSet {
            if arrowsFrom != oldValue { listed = nil }
        }
    }

    /// How much panes without the keys are dimmed, 0 to 0.6.
    public var paneDim: Double { didSet { save() } }
    /// Whether the focused pane is outlined, and in what colour: nil for the theme's accent.
    public var paneOutline: Bool { didSet { save() } }
    public var paneOutlineColour: RGBA? { didSet { save() } }
    /// Whether photos and videos attached go in HD unless Tab says otherwise.
    public var sendHD: Bool { didSet { save() } }

    @ObservationIgnored let defaults: UserDefaults
    @ObservationIgnored public var changed: [() -> Void] = []
    /// Each slot's roles and every theme there is, worked out once until what they rest on changes,
    /// since the app reads its style on every layout.
    @ObservationIgnored var made: [Slot: Roles] = [:]
    @ObservationIgnored var listed: [Theme]?
    /// Whether what changes is written to `defaults`: a snapshot's preferences aren't, as every
    /// checkout's snapshots on the build Mac share the app's preferences.
    @ObservationIgnored let keeps: Bool

    public static let textSizes: ClosedRange<CGFloat> = 11 ... 17
    public static let paneDims: ClosedRange<Double> = 0 ... 0.6

    /// `accent` is the system's unless one's given, as the tests give macOS's blue; `keeps` is false
    /// for a snapshot, which reads what's kept and writes nothing back.
    public init(defaults: UserDefaults = .standard, accent: SystemAccent? = nil, keeps: Bool = true) {
        self.keeps = keeps
        self.defaults = defaults
        look = Look(rawValue: defaults.string(forKey: "look") ?? "") ?? .bubbles
        density = Density(rawValue: defaults.string(forKey: "density") ?? "") ?? .comfortable
        let size = defaults.double(forKey: "textSize")
        textSize = size == 0 ? 13 : min(max(size, Self.textSizes.lowerBound), Self.textSizes.upperBound)
        font = MessageFont(rawValue: defaults.string(forKey: "font") ?? "") ?? .system
        appearance = Appearance(rawValue: defaults.string(forKey: "appearance") ?? "") ?? .system
        // The theme key holds the one theme there was before the slots: it's read into both while
        // they're unset, and left as it is, so an earlier build still reads it.
        let theme = defaults.string(forKey: "theme") ?? Catalog.jaw.id
        slots = Slots(dark: defaults.string(forKey: "themeDark") ?? theme, light: defaults.string(forKey: "themeLight") ?? theme)
        self.accent = accent ?? .current()
        paneDim = defaults.object(forKey: "paneDim") == nil ? 0.18 : min(max(defaults.double(forKey: "paneDim"), 0), 0.6)
        paneOutline = defaults.object(forKey: "paneOutline") == nil ? true : defaults.bool(forKey: "paneOutline")
        paneOutlineColour = defaults.object(forKey: "paneOutlineColour") == nil ? nil : RGBA(UInt32(truncatingIfNeeded: defaults.integer(forKey: "paneOutlineColour")))
        sendHD = defaults.bool(forKey: "sendHD")
    }

    func save() {
        if keeps {
            defaults.set(look.rawValue, forKey: "look")
            defaults.set(density.rawValue, forKey: "density")
            defaults.set(Double(textSize), forKey: "textSize")
            defaults.set(font.rawValue, forKey: "font")
            defaults.set(appearance.rawValue, forKey: "appearance")
            defaults.set(slots.dark, forKey: "themeDark")
            defaults.set(slots.light, forKey: "themeLight")
            defaults.set(paneDim, forKey: "paneDim")
            defaults.set(paneOutline, forKey: "paneOutline")
            if let c = paneOutlineColour { defaults.set(Int(c.hex), forKey: "paneOutlineColour") } else { defaults.removeObject(forKey: "paneOutlineColour") }
            defaults.set(sendHD, forKey: "sendHD")
        }
        tell()
    }

    /// Tells the app to draw again, as something it draws from has changed.
    func tell() {
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
        Style(roles: roles(dark ? .dark : .light), look: look, density: density, textSize: textSize, font: font)
    }

    /// What `slot` draws: its theme's half, or Jaw's Tokyo Night's where no source has the theme. What
    /// it rests on is read before what's made, so a view drawn from it redraws as that changes.
    public func roles(_ slot: Slot) -> Roles {
        let slots = self.slots, accent = self.accent, served = self.served
        if let roles = made[slot] { return roles }
        let roles = Catalog.roles(slot, of: slots, omarchy: served ?? remembered(slots), accent: accent)
        made[slot] = roles
        return roles
    }

    /// Every theme there is to choose, in the gallery's order: Omarchy's as jawd serves them, or until
    /// it has, the slots' own from their copies, and while the arrows run, those of the slots they
    /// started from, so they can come back to them.
    public var themes: [Theme] {
        let slots = self.slots, accent = self.accent, served = self.served, start = self.arrowsFrom
        if let listed { return listed }
        let themes = Catalog.themes(omarchy: served ?? remembered(slots, or: start), accent: accent)
        listed = themes
        return themes
    }

    /// Fills the slots as the gallery's arrows reach `theme`, in `slot`, previewing it from the slots
    /// they started from, as the terminal previews a theme as it moves through its list: those slots as
    /// they were where it's the theme they started on, or as a click there would fill them, so the other
    /// slot takes a theme with both halves only while the arrows rest on it, and → ← loses nothing.
    public func preview(_ theme: Theme, for slot: Slot) {
        let start = arrowsFrom ?? slots
        slots = theme.id == start[slot] ? start : start.choosing(theme, for: slot)
        arrowsFrom = start
    }

    /// The line the Omarchy section shows in place of jawd's themes, or nil once they're in.
    public var omarchyLine: String? {
        served == nil ? omarchyState.line : nil
    }

    /// The slots' Omarchy themes as jawd last served them, from the copies kept beside the slots, or
    /// beside `start`'s, the slots the arrows started from; a copy of a theme neither holds in its slot
    /// is passed over.
    func remembered(_ slots: Slots, or start: Slots? = nil) -> [OmarchyTheme] {
        Slot.allCases.compactMap { slot in
            guard var colours = defaults.dictionary(forKey: Self.copy(slot)) as? [String: String],
                  let id = colours.removeValue(forKey: "id"), let name = colours.removeValue(forKey: "name"),
                  slots[slot] == "omarchy:" + id || start?[slot] == "omarchy:" + id
            else { return nil }
            return OmarchyTheme(id: id, name: name, dark: slot.isDark, colours: colours)
        }
    }

    /// Where a slot's copy of its Omarchy theme is kept.
    static func copy(_ slot: Slot) -> String {
        slot.isDark ? "themeDarkColours" : "themeLightColours"
    }

    /// Keeps a copy of each slot's Omarchy theme as jawd serves it, with its ID and name, and drops one
    /// jawd no longer serves, so the next launch draws what this one does before jawd answers; a
    /// snapshot's preferences keep none.
    func remember() {
        guard keeps, let served else { return }
        for slot in Slot.allCases {
            if let theme = served.first(where: { "omarchy:" + $0.id == slots[slot] && $0.dark == slot.isDark }) {
                defaults.set(theme.colours.merging(["id": theme.id, "name": theme.name]) { $1 }, forKey: Self.copy(slot))
            } else {
                defaults.removeObject(forKey: Self.copy(slot))
            }
        }
    }

    /// Reads the system's accent, orange, green and red again, as the app does when AppKit says the system's colours changed.
    public func systemColoursChanged() {
        accent = .current()
    }

    /// How the panes say which has the keys, in a theme's roles.
    public func paneLook(_ roles: Roles) -> PaneLook {
        PaneLook(dim: paneDim, outline: paneOutline ? paneOutlineColour ?? roles.accent : nil)
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

public extension SystemAccent {
    /// The system's accent, orange, green and red as AppKit draws them now, in a dark appearance and in
    /// a light one; where AppKit has no appearance of the name, the fixed colours of the one asked for.
    @MainActor static func current() -> SystemAccent {
        func drawn(_ colour: NSColor, otherwise fixed: SystemColour) -> SystemColour {
            func half(_ name: NSAppearance.Name, _ otherwise: RGBA) -> RGBA {
                var out = otherwise
                NSAppearance(named: name)?.performAsCurrentDrawingAppearance { out = RGBA(colour) }
                return out
            }
            return SystemColour(dark: half(.darkAqua, fixed.dark), light: half(.aqua, fixed.light))
        }
        let accent = drawn(.controlAccentColor, otherwise: SystemColour(dark: blue.dark, light: blue.light))
        return SystemAccent(dark: accent.dark, light: accent.light, orange: drawn(.systemOrange, otherwise: .orange),
                            green: drawn(.systemGreen, otherwise: .green), red: drawn(.systemRed, otherwise: .red))
    }
}
