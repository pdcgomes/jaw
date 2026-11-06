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
        "kanagawa light", "tokyo-night light", "moonlight light", "catppuccin light", "rose-pine light", "nord light",
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

@Test func `Redlamp's halves map by the Mac design's table, held by the bands, which leave five dark halves as they are`() {
    var moved: [String: [String]] = [:]
    var untouched: [String] = []
    for family in RedlampCatalog.families {
        for (half, p, dark) in [("dark", family.dark, true), ("light", family.light, false)] {
            let r = RedlampRoles.roles(p, dark: dark)
            let tint = p.primaryRoles.contains(.selection) ? p.primary : p.neutral
            let accent = p.primaryRoles.contains(.accent) ? p.primary : p.neutral
            let table: [(String, RGBA, RGBA)] = [
                ("dim text", p.muted.opacity(0.28 / 0.45).over(p.background), r.tertiary), ("mention", p.accent, r.mention),
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
            #expect(r.ground.hex == p.background.hex && r.surface.hex == p.card.hex && r.text.hex == p.foreground.hex, "\(name)")
            #expect(r.link == r.accent && r.separator.hex == p.border.hex, "\(name): links as the accent, borders as authored")
        }
    }
    #expect(untouched == ["redlamp dark", "kanagawa dark", "catppuccin dark", "gruvbox dark", "ocean dark"])
    for (role, halves) in redlampMoves {
        #expect(moved[role, default: []].sorted() == halves.sorted(), "\(role)")
    }
    #expect(Set(moved.keys) == Set(redlampMoves.keys))
}

@Test func `the Redlamp family keeps its red to the accent and links, its selection and your bubbles from its grey`() {
    let redlamp = RedlampCatalog.families[0]
    let dark = RedlampRoles.roles(redlamp.dark, dark: true)
    #expect(dark.accent.hex == 0xE0402E && dark.link.hex == 0xE0402E, "the safelight")
    #expect(dark.selection.hex == RedlampCatalog.Brand.ring.opacity(0.2).over(redlamp.dark.card).hex, "ring, not red")
    #expect(dark.yours.hex == RedlampCatalog.Brand.ring.opacity(0.18).over(redlamp.dark.background).hex)
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
    for (dark, accent) in [(true, RGBA(0x0A84FF)), (false, RGBA(0x007AFF))] {
        let r = RedlampRoles.neutral(dark: dark, accent: accent)
        reads(r, "neutral \(dark ? "dark" : "light")")
        #expect(r.ground.hex == (dark ? 0x1D1D1D : 0xECECEC), "Redlamp's panel grey")
        #expect(r.accent.hex == accent.hex, "the system's accent, which reads as it is")
    }
}
