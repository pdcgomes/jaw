import Foundation
@testable import JawThemes
import Testing

/// Redlamp's catalog as jaw copies it, and jaw's roles from it: the families in Redlamp's order, the
/// Mac design's table, the same bands as Omarchy's, which move what a port of them found on
/// 6 November, the Redlamp family's red kept to its accent, and every half reading, Neutral's in the
/// system's blue.

@Test func `the copy holds Redlamp's 21 families of ten colours in its order, Neutral apart`() {
    #expect(RedlampCatalog.families.map { $0.id } == [
        "redlamp", "kanagawa", "tokyo-night", "moonlight", "catppuccin", "rose-pine", "nord", "everforest", "gruvbox", "dracula",
        "solarized", "one-dark", "monokai", "ocean", "sakura", "copper", "mc-vinyl-noir", "mc-neon-dusk", "mc-burnt-tape",
        "mc-phantom-frequency", "mc-velvet-static",
    ])
    #expect(Array(RedlampCatalog.families.map { $0.name }.suffix(5)) == ["Vinyl Noir", "Neon Dusk", "Burnt Tape", "Phantom Frequency", "Velvet Static"])
    #expect(RedlampCatalog.hsl("240 16% 14%").hex == 0x1E1E29, "Kanagawa's dark ground, as Redlamp parses it")
    #expect(RedlampCatalog.hsl("nonsense").hex == 0x000000, "a typo yields black, as in Redlamp")
    let redlamp = RedlampCatalog.families[0]
    #expect(redlamp.dark.background.hex == 0x0A0707 && redlamp.dark.primary.hex == 0xE0402E && redlamp.dark.primaryRoles == .accent)
    #expect(redlamp.light.background.hex == 0xF3EEE8 && redlamp.light.neutral.hex == 0x57504E, "steel, the light half's grey")
}

/// What holding Redlamp's halves in the bands moves, role by role, as a port of the rule found on
/// 6 November; every other half comes through as the Mac design's table maps it.
let redlampMoves: [String: [String]] = [
    "dim text": [
        "redlamp light", "kanagawa light", "tokyo-night dark", "tokyo-night light", "moonlight dark", "moonlight light",
        "catppuccin light", "rose-pine dark", "rose-pine light", "nord dark", "everforest dark", "everforest light", "gruvbox light",
        "dracula dark", "dracula light", "solarized dark", "solarized light", "one-dark dark", "one-dark light", "monokai dark",
        "monokai light", "ocean light", "sakura dark", "sakura light", "copper dark", "copper light", "mc-vinyl-noir dark",
        "mc-vinyl-noir light", "mc-neon-dusk dark", "mc-neon-dusk light", "mc-burnt-tape dark", "mc-burnt-tape light",
        "mc-phantom-frequency dark", "mc-phantom-frequency light", "mc-velvet-static dark", "mc-velvet-static light",
    ],
    "mention": [
        "redlamp light", "kanagawa light", "tokyo-night light", "moonlight light", "catppuccin light", "rose-pine light", "nord light",
        "everforest light", "gruvbox light", "solarized light", "monokai light", "ocean light", "sakura light", "copper light",
        "mc-vinyl-noir light", "mc-burnt-tape light", "mc-phantom-frequency light", "mc-velvet-static light",
    ],
    "online": [
        "kanagawa light", "moonlight light", "catppuccin light", "nord light", "everforest light", "gruvbox light", "dracula light",
        "solarized light", "monokai light",
    ],
    "secondary": [
        "tokyo-night dark", "rose-pine light", "nord dark", "everforest dark", "everforest light", "dracula dark", "solarized dark",
        "one-dark dark", "monokai dark",
    ],
    "accent": ["tokyo-night light", "everforest light"],
    "failed": ["tokyo-night light", "moonlight light", "nord dark", "everforest light", "solarized dark"],
    "selection": ["everforest dark", "everforest light"],
    "yours": ["everforest light"],
]

@Test func `Redlamp's halves map by the Mac design's table, held by the bands, which leave five dark halves as they are and land what they move within 0.05 of its target`() {
    var moved: [String: [String]] = [:]
    var untouched: [String] = []
    for family in RedlampCatalog.families {
        for (half, p, dark) in [("dark", family.dark, true), ("light", family.light, false)] {
            let r = RedlampRoles.roles(p, dark: dark)
            let tint = p.primaryRoles.contains(.selection) ? p.primary : p.neutral
            let accent = p.primaryRoles.contains(.accent) ? p.primary : p.neutral
            let mention = p.accent == RedlampCatalog.Brand.safelight ? RedlampCatalog.Brand.filament : p.accent
            let table: [(String, RGBA, RGBA)] = [
                ("dim text", p.muted.opacity(0.28 / 0.45).over(p.background), r.tertiary), ("mention", mention, r.mention),
                ("online", p.success, r.online), ("secondary", p.muted, r.secondary), ("accent", accent, r.accent),
                ("failed", p.destructive, r.failed), ("selection", tint.opacity(dark ? 0.2 : 0.16).over(p.card), r.selection),
                ("yours", tint.opacity(dark ? 0.18 : 0.12).over(p.background), r.yours),
            ]
            let name = "\(family.id) \(half)"
            let differ = table.filter { $0.1.hex != $0.2.hex }.map { $0.0 }
            for role in differ {
                moved[role, default: []].append(name)
            }
            if differ.isEmpty { untouched.append(name) }
            let (g, s, t) = (r.ground.hex, r.surface.hex, r.text.hex)
            for (role, was, now) in table where now.hex != was.hex {
                let target = role == "selection" || role == "yours" ? 4.5 : 3
                let reached = switch role {
                case "dim text": Contrast.ratio(now.hex, g)
                case "selection", "yours": Contrast.ratio(t, now.hex)
                default: min(Contrast.ratio(now.hex, g), Contrast.ratio(now.hex, s))
                }
                #expect(abs(reached - target) <= 0.05, "\(name): \(role) lands at \(reached), its target \(target)")
            }
            #expect(r.ground.hex == p.background.hex && r.surface.hex == p.card.hex && r.text.hex == p.foreground.hex, "\(name)")
            #expect(r.link == r.accent && r.accentText == r.accent && r.separator.hex == p.border.hex,
                    "\(name): links and the accent drawn as text as the accent, borders as authored")
        }
    }
    #expect(untouched == ["redlamp dark", "kanagawa dark", "catppuccin dark", "gruvbox dark", "ocean dark"])
    for (role, halves) in redlampMoves {
        #expect(moved[role, default: []].sorted() == halves.sorted(), "\(role)")
    }
    #expect(Set(moved.keys) == Set(redlampMoves.keys))
}

@Test func `the Redlamp family keeps its red to the accent and links in both halves, its mentions the filament and its senders' hues away from the red`() {
    let redlamp = RedlampCatalog.families[0]
    let dark = RedlampRoles.roles(redlamp.dark, dark: true)
    #expect(dark.accent.hex == 0xE0402E && dark.link.hex == 0xE0402E, "the safelight")
    #expect(dark.selection.hex == RedlampCatalog.Brand.ring.opacity(0.2).over(redlamp.dark.card).hex, "ring, not red")
    #expect(dark.yours.hex == RedlampCatalog.Brand.ring.opacity(0.18).over(redlamp.dark.background).hex)
    #expect(dark.mention.hex == 0xFFB08A, "the filament, as it is")
    let light = RedlampRoles.roles(redlamp.light, dark: false)
    #expect(light.accent.hex == 0xD8352A && light.link.hex == 0xD8352A, "the safelight drawn on paper")
    #expect(light.selection.hex == RedlampCatalog.Brand.steel.opacity(0.16).over(redlamp.light.card).hex, "steel, not red")
    #expect(light.yours.hex == RedlampCatalog.Brand.steel.opacity(0.12).over(redlamp.light.background).hex)
    #expect(redlamp.light.accent == RedlampCatalog.Brand.safelight && light.mention.hex == 0xAC7760,
            "its accent field is the safelight, so its mentions take the filament, darkened to read on paper, not a red beside the unread counts")
    let (heldDark, heldLight) = (dark.held(), light.held())
    #expect(heldDark.accentText.hex == 0xE46353 && heldDark.link.hex == 0xE46353 && heldDark.accent == dark.accent,
            "held where they're drawn: the red as text and links a step lighter, to read on the dark selection, its fills the safelight")
    #expect(heldLight.accentText.hex == 0xCF3429 && heldLight.link.hex == 0xD7352A && heldLight.mention.hex == 0x936654,
            "and darker in light: as text for the accent's tint behind a card's symbols in your bubbles, links for the selection, the mention for its own tint in your bubbles")
    #expect(dark.senders.map(\.hex) == [0xE0D12E, 0x80E02E, 0x2EE04B, 0x2EE0BA, 0x2E96E0, 0x7C77EB, 0xB75AE7, 0xE340B4])
    #expect(light.senders.map(\.hex) == [0x736715, 0x447315, 0x167722, 0x15735B, 0x1C6C9C, 0x2A2AD8, 0x972AD8, 0xB82292])
    for (half, r, primary) in [("dark", heldDark, redlamp.dark.primary), ("light", heldLight, redlamp.light.primary)] {
        let red = RedlampRoles.hsl(primary).hue
        for (i, sender) in r.senders.enumerated() {
            let apart = abs(RedlampRoles.hsl(sender).hue - red)
            #expect(min(apart, 360 - apart) >= 30, "\(half): sender \(i) is \(min(apart, 360 - apart))° from the safelight's hue")
        }
    }
}

@Test func `every half of Redlamp's reads, its senders at 4.5:1 on their bubble, and Neutral's in the system's blue`() {
    for family in RedlampCatalog.families {
        for (half, p, dark) in [("dark", family.dark, true), ("light", family.light, false)] {
            let r = RedlampRoles.roles(p, dark: dark)
            reads(r, "\(family.id) \(half)")
            #expect(r.senders.allSatisfy { $0.contrast(with: r.surface) >= 4.5 }, "\(family.id) \(half): senders on their bubble")
            #expect(r.isDark == dark)
        }
    }
    for dark in [true, false] {
        let r = RedlampRoles.neutral(dark: dark, accent: .blue)
        reads(r, "neutral \(dark ? "dark" : "light")")
        #expect(r.ground.hex == (dark ? 0x1D1D1D : 0xECECEC), "Redlamp's panel grey")
        #expect(r.accent.hex == 0x007AFF, "the system's accent, which reads as it is")
        #expect([r.mention.hex, r.online.hex, r.failed.hex] == (dark ? [0xFF9230, 0x30D158, 0xFF4245] : [0xC97225, 0x2E9B49, 0xFF383C]),
                "the system's orange, green and red as macOS 27.0.1 draws them, light's orange and green darkened to read on its ground")
    }
    let other = SystemAccent(dark: RGBA(0x0A84FF), light: RGBA(0x007AFF), red: SystemColour(dark: RGBA(0xFF6961), light: RGBA(0xD70015)))
    #expect(RedlampRoles.neutral(dark: true, accent: other).failed.hex == 0xFF6961
        && RedlampRoles.neutral(dark: false, accent: other).failed.hex == 0xD70015, "Neutral's failed is the system's red, whatever AppKit gives")
}

@Test func `a half's senders are eight hues 45° apart from primary's, or, where it keeps primary rare, 37.5° apart away from it`() {
    #expect(RedlampRoles.hues(from: 10, rare: false) == [10, 55, 100, 145, 190, 235, 280, 325])
    #expect(RedlampRoles.hues(from: 10, rare: true) == [58.75, 96.25, 133.75, 171.25, 208.75, 246.25, 283.75, 321.25],
            "48.75° past it to 48.75° short of it, none within 30° of it")
    #expect(RedlampCatalog.families.filter { !$0.dark.primaryRoles.contains(.selection) }.map(\.id) == ["redlamp"],
            "only the Redlamp family keeps its primary rare, so every other family's senders are as they were")
}

/// Graphite's accent is a grey, which only AppKit gives with Graphite chosen; these are the two Task 3's
/// review found Neutral's senders all but one colour in: macOS's system grey in dark, and a grey with no
/// hue at all in light.
@Test func `with a grey accent, as Graphite's is, Neutral's senders take the system's blue's hue and come out as eight colours`() {
    let graphite = SystemAccent(dark: RGBA(0x98989D), light: RGBA(0x8C8C8C))
    for dark in [true, false] {
        let name = "Neutral in graphite, \(dark ? "dark" : "light")"
        let grey = dark ? graphite.dark : graphite.light
        let r = RedlampRoles.neutral(dark: dark, accent: graphite)
        reads(r, name)
        readsWhereDrawn(r.held(), name)
        let blue = RedlampRoles.hsl(dark ? SystemAccent.blue.dark : SystemAccent.blue.light)
        let tinted = RGBA(hue: blue.hue, saturation: blue.saturation, lightness: RedlampRoles.hsl(grey).lightness)
        #expect(r.senders == RedlampRoles.senders(from: tinted, on: r.surface.hex, dark: dark), "\(name): the blue's hue at the grey's lightness")
        #expect(Set(r.held().senders.map(\.hex)).count == 8, "\(name): eight colours, not eight greys")
    }
    #expect(RedlampRoles.neutral(dark: true, accent: graphite).senders.map(\.hex)
        == [0x3696FF, 0x9D78FF, 0xFF36FB, 0xFF4D76, 0xFF9F36, 0xC8FF36, 0x36FF3A, 0x36FFD1])
    #expect(RedlampRoles.neutral(dark: false, accent: graphite).senders.map(\.hex)
        == [0x006CE2, 0x5719FF, 0xC900C5, 0xE20034, 0xAD5A00, 0x597A00, 0x008403, 0x007F62])
    #expect(RedlampRoles.neutral(dark: false, accent: .blue).senders.map(\.hex)
        == [0x006DE3, 0x4500FF, 0xC900C5, 0xE30034, 0xAD5A00, 0x597A00, 0x008203, 0x007F62], "an accent with a hue keeps its own")
    let neutral = Catalog.theme(id: "redlamp:neutral", omarchy: [], accent: graphite)
    #expect(Set(neutral?.dark?.senders.map(\.hex) ?? []).count == 8 && Set(neutral?.light?.senders.map(\.hex) ?? []).count == 8)
}
