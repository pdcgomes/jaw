import Foundation

/// jaw's roles from Redlamp's families, by the Mac design's table, then held as Omarchy's are: every
/// colour at 3:1 against the ground and the surface, secondary text too, the dim text at 2.5 to 4.5:1,
/// and the selection and your bubbles moved toward their ground until text reads at 4.5:1 on them, so
/// a family made for a photo editor's panels reads in a chat, readable over exact (Owner, 6 Nov).
public enum RedlampRoles {
    /// A family's half: `primary`, or `neutral` where `primaryRoles` keeps it out, takes the accent and
    /// links, the selection at 20% over `card` (16% in light) and your bubbles at 18% over `background`
    /// (12%); `accent` takes mentions, `success` online and `destructive` failed; the senders are eight
    /// hues 45° apart from `primary`'s.
    public static func roles(_ p: ThemePalette, dark: Bool) -> Roles {
        let tint = p.primaryRoles.contains(.selection) ? p.primary : p.neutral
        return held(
            dark: dark, ground: p.background, surface: p.card, text: p.foreground,
            selection: tint.opacity(dark ? 0.2 : 0.16).over(p.card), yours: tint.opacity(dark ? 0.18 : 0.12).over(p.background),
            secondary: p.muted, tertiary: p.muted.opacity(0.28 / 0.45).over(p.background), separator: p.border,
            accent: p.primaryRoles.contains(.accent) ? p.primary : p.neutral,
            mention: p.accent, online: p.success, failed: p.destructive, hue: p.primary,
        )
    }

    /// Neutral, Redlamp's own grey: its standard tokens' greys at their alphas over its panel, white
    /// over 11.5% grey in dark and black over 92.5% in light; its accent the system's, which Redlamp's
    /// Neutral leaves its accent to; and the system's orange, green and red for mentions, online and
    /// failed, as macOS draws them in each appearance, held as a family's are, its senders' hues from
    /// the accent's.
    public static func neutral(dark: Bool, accent: RGBA) -> Roles {
        let ink = dark ? RGBA(0xFFFFFF) : RGBA(0x000000)
        let ground = dark ? RGBA(r: 0.115, g: 0.115, b: 0.115) : RGBA(r: 0.925, g: 0.925, b: 0.925)
        let surface = RGBA(0xFFFFFF).opacity(dark ? 0.05 : 0.55).over(ground)
        return held(
            dark: dark, ground: ground, surface: surface, text: ink.opacity(dark ? 0.9 : 0.88).over(ground),
            selection: ink.opacity(dark ? 0.1 : 0.08).over(surface), yours: accent.opacity(dark ? 0.18 : 0.12).over(ground),
            secondary: ink.opacity(dark ? 0.45 : 0.5).over(ground), tertiary: ink.opacity(dark ? 0.28 : 0.3).over(ground),
            separator: ink.opacity(dark ? 0.07 : 0.09).over(ground), accent: accent,
            mention: dark ? RGBA(0xFF9F0A) : RGBA(0xFF9500), online: dark ? RGBA(0x30D158) : RGBA(0x34C759),
            failed: dark ? RGBA(0xFF453A) : RGBA(0xFF3B30), hue: accent,
        )
    }

    /// The roles from what a half gives each, held: colours at 3:1 on the ground and then the surface,
    /// the dim text in its band, the selection and your bubbles under text, a selected row's online at
    /// 4.5:1 on its selection, links as the accent and the borders as authored.
    static func held(dark: Bool, ground: RGBA, surface: RGBA, text: RGBA, selection: RGBA, yours: RGBA, secondary: RGBA,
                     tertiary: RGBA, separator: RGBA, accent: RGBA, mention: RGBA, online: RGBA, failed: RGBA, hue: RGBA) -> Roles
    {
        let (g, s, t) = (ground.hex, surface.hex, text.hex)
        func both(_ c: RGBA) -> UInt32 {
            Contrast.fit(Contrast.fit(c.hex, on: g, toward: t, low: 3, target: 3), on: s, toward: t, low: 3, target: 3)
        }
        let picked = under(t, selection.hex, toward: s)
        let on = both(online)
        let a = RGBA(both(accent))
        return Roles(
            isDark: dark, ground: RGBA(g), surface: RGBA(s), selection: RGBA(picked), text: RGBA(t), secondary: RGBA(both(secondary)),
            tertiary: RGBA(Contrast.fit(tertiary.hex, on: g, toward: t, low: 2.5, high: 4.5, target: 3)), separator: RGBA(separator.hex),
            accent: a, link: a, yours: RGBA(under(t, yours.hex, toward: g)), mention: RGBA(both(mention)), online: RGBA(on),
            selectedOnline: RGBA(Contrast.fit(on, on: picked, toward: t, low: 4.5, target: 4.5)), failed: RGBA(both(failed)),
            senders: senders(from: hue, on: s, dark: dark),
        )
    }

    /// `fill` moved toward `ground` until `text` reads at 4.5:1 on it.
    static func under(_ text: UInt32, _ fill: UInt32, toward ground: UInt32) -> UInt32 {
        guard Contrast.ratio(text, fill) < 4.5 else { return fill }
        var (near, far) = (0.0, 1.0)
        for _ in 0 ..< 30 {
            let t = (near + far) / 2
            if Contrast.ratio(text, Contrast.mix(fill, ground, t)) < 4.5 {
                near = t
            } else {
                far = t
            }
        }
        return Contrast.mix(fill, ground, far)
    }

    /// Eight hues 45° apart from `primary`'s, its saturation and lightness kept, each lightened in dark
    /// or darkened in light half a percent at a time until it reads at 4.5:1 on `surface`.
    static func senders(from primary: RGBA, on surface: UInt32, dark: Bool) -> [RGBA] {
        let (hue, saturation, lightness) = hsl(primary)
        return (0 ..< 8).map { k in
            var l = lightness
            var colour = RGBA(hue: hue + Double(k) * 45, saturation: saturation, lightness: l).hex
            var steps = 0
            while Contrast.ratio(colour, surface) < 4.5, steps < 200 {
                l = min(max(l + (dark ? 0.005 : -0.005), 0), 1)
                colour = RGBA(hue: hue + Double(k) * 45, saturation: saturation, lightness: l).hex
                steps += 1
            }
            return RGBA(colour)
        }
    }

    /// A colour's hue in degrees, its saturation and its lightness, as HSL has them.
    static func hsl(_ c: RGBA) -> (hue: Double, saturation: Double, lightness: Double) {
        let (high, low) = (max(c.r, c.g, c.b), min(c.r, c.g, c.b))
        let lightness = (high + low) / 2
        guard high > low else { return (0, 0, lightness) }
        let d = high - low
        let saturation = lightness > 0.5 ? d / (2 - high - low) : d / (high + low)
        let hue: Double = switch high {
        case c.r: (c.g - c.b) / d + (c.g < c.b ? 6 : 0)
        case c.g: (c.b - c.r) / d + 2
        default: (c.r - c.g) / d + 4
        }
        return (hue * 60, saturation, lightness)
    }
}
