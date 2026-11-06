import Foundation

/// Omarchy's themes as jawd serves them (`themes`), mapped onto the Mac's roles as the terminal's
/// `theme.rs` maps them onto its own, then held at 3:1 against the surface too, where the Mac draws
/// their bubbles and its chat list, as the terminal holds its colours against the ground.
public enum Omarchy {
    /// The 25 colours jawd serves for each theme, by Omarchy's names for them.
    public static let keys = [
        "accent", "selection", "muted", "background", "dark_background", "darker_background", "lighter_background",
        "foreground", "dark_foreground", "light_foreground", "bright_foreground", "red", "yellow", "orange", "green", "cyan",
        "blue", "magenta", "brown", "bright_red", "bright_yellow", "bright_green", "bright_cyan", "bright_blue", "bright_magenta",
    ]

    /// The terminal's roles for a theme, as `theme.rs`'s `Roles` has them, by its names for them.
    public struct Terminal: Equatable, Sendable {
        public var bg, surface, selection, fg, text, muted, comment, gutter: UInt32
        public var accent, link, mention, online, notice: UInt32
        public var senders: [UInt32]
    }

    /// `theme.rs`'s `Roles::of`, ported as it is: a grey outside its band of contrast with the ground
    /// moved toward the ground or the text until it's in, keeping its hue; the selection then held
    /// apart from the surface too, which overlays draw it on; and every colour at least 3:1. Nil when
    /// one of the colours it reads is missing.
    public static func terminal(_ c: [String: UInt32]) -> Terminal? {
        guard let bg = c["background"], let bright = c["bright_foreground"], let plain = c["foreground"],
              let dim = c["dark_foreground"], let lighter = c["lighter_background"], let picked = c["selection"],
              let muted = c["muted"], let accent = c["accent"], let cyan = c["cyan"], let orange = c["orange"],
              let green = c["green"], let yellow = c["yellow"]
        else { return nil }
        let named = ["red", "yellow", "blue", "magenta", "cyan", "green", "orange", "bright_cyan"].compactMap { c[$0] }
        guard named.count == 8 else { return nil }
        let (fg, text) = Contrast.ratio(bright, bg) >= Contrast.ratio(plain, bg) ? (bright, plain) : (plain, bright)
        func held(_ colour: UInt32, _ low: Double, _ high: Double, _ target: Double) -> UInt32 {
            Contrast.fit(colour, on: bg, toward: fg, low: low, high: high, target: target)
        }
        func hue(_ colour: UInt32) -> UInt32 {
            held(colour, 3, .infinity, 3)
        }
        let comment = held(dim, 2.5, 4.5, 3)
        let surface = held(lighter, 1.05, .infinity, 1.1)
        let selection = held(picked, 1.15, .infinity, 1.25)
        return Terminal(
            bg: bg, surface: surface, selection: Contrast.fit(selection, on: surface, toward: fg, low: 1.15, target: 1.15),
            fg: fg, text: text, muted: Contrast.mix(comment, text, 0.5), comment: comment, gutter: held(muted, 1.4, 2.6, 1.8),
            accent: accent, link: hue(cyan), mention: hue(orange), online: hue(green), notice: hue(yellow), senders: named.map(hue),
        )
    }

    /// The Mac's roles for one of Omarchy's themes, from its colours by Omarchy's keys as `#rrggbb`:
    /// the terminal's, the Mac's own beside them (your bubbles, a selected row's online, failed), and
    /// each colour the terminal holds at 3:1 against the ground held so against the surface too. Nil
    /// when a colour is missing or isn't `#rrggbb`.
    public static func roles(_ colours: [String: String], dark: Bool) -> Roles? {
        var c: [String: UInt32] = [:]
        for key in keys {
            guard let value = colours[key].flatMap(hex) else { return nil }
            c[key] = value
        }
        guard let t = terminal(c), let red = c["red"] else { return nil }
        func surfaced(_ colour: UInt32) -> UInt32 {
            Contrast.fit(colour, on: t.surface, toward: t.fg, low: 3, target: 3)
        }
        let online = surfaced(t.online)
        return Roles(
            isDark: dark, ground: RGBA(t.bg), surface: RGBA(t.surface), selection: RGBA(t.selection), text: RGBA(t.fg),
            secondary: RGBA(t.muted), tertiary: RGBA(t.comment), separator: RGBA(t.gutter), accent: RGBA(t.accent),
            link: RGBA(surfaced(t.link)), yours: RGBA(t.accent).opacity(dark ? 0.18 : 0.12).over(RGBA(t.bg)),
            mention: RGBA(surfaced(t.mention)), online: RGBA(online),
            selectedOnline: RGBA(Contrast.fit(online, on: t.selection, toward: t.fg, low: 4.5, target: 4.5)),
            failed: RGBA(surfaced(Contrast.fit(red, on: t.bg, toward: t.fg, low: 3, target: 3))),
            senders: t.senders.map { RGBA(surfaced($0)) },
        )
    }

    /// `#rrggbb`, in either case, as `0xRRGGBB`.
    public static func hex(_ text: String) -> UInt32? {
        guard text.count == 7, text.hasPrefix("#"), text.dropFirst().allSatisfy(\.isHexDigit) else { return nil }
        return UInt32(text.dropFirst(), radix: 16)
    }

    /// A theme's `colors.toml`, as `protocol/themes/` holds Omarchy's, read as jaw-proto reads it: its
    /// mode, and its colours by Omarchy's keys as lowercase `#rrggbb`, with `orange` its `yellow` and
    /// `brown` its `orange` mixed half with black, each channel rounded half up, where it leaves them
    /// out. Nil when a line isn't `key = "value"`, the mode isn't `dark` or `light`, or a colour is
    /// missing. For the tests and the harness, which ask jawd for nothing; the app reads jawd's `themes`.
    public static func file(_ text: String) -> (dark: Bool, colours: [String: String])? {
        var values: [String: String] = [:]
        for raw in text.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.isEmpty || line.hasPrefix("#") { continue }
            guard let equals = line.firstIndex(of: "=") else { return nil }
            let key = line[..<equals].trimmingCharacters(in: .whitespaces)
            values[key] = line[line.index(after: equals)...].trimmingCharacters(in: .whitespaces)
                .trimmingCharacters(in: CharacterSet(charactersIn: "\""))
        }
        guard let mode = values["mode"], mode == "dark" || mode == "light" else { return nil }
        if values["orange"] == nil { values["orange"] = values["yellow"] }
        if values["brown"] == nil, let orange = values["orange"].flatMap(hex) {
            func half(_ shift: UInt32) -> UInt32 {
                ((((orange >> shift) & 0xFF) + 1) / 2) << shift
            }
            values["brown"] = String(format: "#%06x", half(16) | half(8) | half(0))
        }
        var colours: [String: String] = [:]
        for key in keys {
            guard let value = values[key].flatMap(hex) else { return nil }
            colours[key] = String(format: "#%06x", value)
        }
        return (mode == "dark", colours)
    }
}
