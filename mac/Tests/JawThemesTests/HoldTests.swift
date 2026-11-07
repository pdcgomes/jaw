import Foundation
@testable import JawThemes
import Testing

/// The hold every theme's roles go through after their own mapping (Owner, 6 Nov): each colour JawUI
/// draws as text or as a mark at 3:1 on every ground its draw sites put it on, the tints it lays over
/// a ground among them, a selected row's online at 4.5:1 on its selection, and the accent's fills, the
/// grounds, the text and the borders as the theme wrote them; what it moves in each half and where
/// each moved colour lands, as a port of it found on 6 November; a mention of you on its own tint, in
/// passes that end; and every theme the catalog lists, held.

/// Every half the catalog makes, as its own mapping makes it, before the hold, by the name the lists
/// below give it: Jaw's Tokyo Night, Omarchy's by ID, Neutral in macOS 27.0.1's colours, and Redlamp's
/// families.
func mappedHalves() throws -> [(name: String, roles: Roles)] {
    var out: [(name: String, roles: Roles)] = [("jaw dark", Catalog.tokyoNight), ("jaw light", Catalog.tokyoNightDay)]
    for theme in try omarchyFiles() {
        out.append(("omarchy:\(theme.id)", try #require(Omarchy.roles(theme.colours, dark: theme.dark))))
    }
    out.append(("redlamp:neutral dark", RedlampRoles.neutral(dark: true, accent: .blue)))
    out.append(("redlamp:neutral light", RedlampRoles.neutral(dark: false, accent: .blue)))
    for family in RedlampCatalog.families {
        out.append(("redlamp:\(family.id) dark", RedlampRoles.roles(family.dark, dark: true)))
        out.append(("redlamp:\(family.id) light", RedlampRoles.roles(family.light, dark: false)))
    }
    return out
}

/// Whether held roles read where JawUI draws them: each colour in `Roles.holds` at its contrast on
/// every one of its grounds, a colour's own tint made from it, as `Contrast` measures it on the
/// colours the screen is given.
func readsWhereDrawn(_ r: Roles, _ name: String) {
    for hold in Roles.holds {
        for colour in r.colours(hold.ink) {
            for place in hold.on {
                let ratio = Contrast.ratio(colour.hex, r.colour(of: place, under: colour).hex)
                #expect(ratio >= hold.at, "\(name): \(hold.ink) on \(place) at \(ratio)")
            }
        }
    }
}

@Test func `the hold is JawUI's draw sites: each colour drawn as text or as a mark, on each ground and tint it's drawn on`() {
    let bases = Roles.Base.allCases.map(Roles.Ground.plain)
    let wells: [Roles.Ground] = [.well(.ground), .well(.surface), .well(.yours), .well(.band)]
    let quotes: [Roles.Ground] = [.quote(.surface), .quote(.yours)]
    let tints: [Roles.Ground] = [.accentTint(.ground), .accentTint(.surface), .accentTint(.yours), .accentTint(.band)]
    #expect(Roles.Base.allCases == [.ground, .surface, .selection, .yours, .band])
    #expect(Roles.holds == [
        Roles.Hold(.secondary, on: bases + wells + quotes),
        Roles.Hold(.tertiary, on: [.plain(.ground), .plain(.surface)]),
        Roles.Hold(.accentText, on: bases + wells + quotes + tints + [.chip]),
        Roles.Hold(.link, on: bases + wells),
        Roles.Hold(.mention, on: bases + [.ownTint(.ground), .ownTint(.surface), .ownTint(.yours), .ownTint(.band)]),
        Roles.Hold(.online, on: [.plain(.ground), .plain(.surface)]),
        Roles.Hold(.selectedOnline, on: [.plain(.selection)], at: 4.5),
        Roles.Hold(.failed, on: bases),
        Roles.Hold(.senders, on: bases + quotes),
    ])
    #expect(Roles.holds.map(\.ink) == Roles.Ink.allCases, "every colour held, once")
    let r = Catalog.jaw.dark
    #expect([r[.ground], r[.surface], r[.selection], r[.yours]] == [r.ground, r.surface, r.selection, r.yours])
    #expect(r.bandTint == r.selection.opacity(0.55) && r[.band] == r.bandTint.over(r.ground), "a selected message's band in lines")
    #expect(r.well == r.text.opacity(0.07) && r.quoteWell == r.text.opacity(0.06) && r.accentTint == r.accent.opacity(0.15)
        && r.chipTint == r.accent.opacity(0.16) && r.mentionTint == r.mention.opacity(0.16), "the tints as JawUI lays them, in dark")
    let l = Catalog.jaw.light
    #expect(l.well == l.text.opacity(0.06) && l.quoteWell == l.text.opacity(0.05) && l.chipTint == l.accent.opacity(0.12), "and in light")
    let grey = RGBA(0x808080)
    #expect(r.colour(of: .plain(.band), under: grey) == r[.band] && r.colour(of: .well(.yours), under: grey) == r.well.over(r.yours))
    #expect(r.colour(of: .quote(.yours), under: grey) == r.quoteWell.over(r.yours))
    #expect(r.colour(of: .accentTint(.yours), under: grey) == r.accentTint.over(r.yours))
    #expect(r.colour(of: .chip, under: grey) == r.chipTint.over(r.surface), "the chip, on the box's field, the surface")
    #expect(r.colour(of: .ownTint(.band), under: grey) == grey.opacity(0.16).over(r[.band]), "a colour's own tint, made from it")
    #expect(r.colours(.senders) == r.senders && r.colours(.accentText) == [r.accentText])
    #expect(r.faint(on: .ground) == r.tertiary && r.faint(on: .surface) == r.tertiary)
    #expect(r.faint(on: .selection) == r.secondary && r.faint(on: .yours) == r.secondary && r.faint(on: .band) == r.secondary,
            "the dim text's words and marks on a selected row, your bubbles and a selected message's band take the secondary text's colour, which is held there")
}

/// What the hold moves, colour by colour, in each half as its mapping leaves it, as a port of it found
/// on 6 November at Omarchy's 4c1b4cb and Redlamp's 9fa9d91, Neutral in macOS 27.0.1's colours. Online
/// and a selected row's online read wherever they're drawn already, so they move nowhere.
let heldMoves: [Roles.Ink: [String]] = [
    .secondary: [
        "jaw dark", "omarchy:last-horizon", "redlamp:neutral dark", "redlamp:neutral light", "redlamp:redlamp light",
        "redlamp:kanagawa light", "redlamp:tokyo-night dark", "redlamp:tokyo-night light", "redlamp:moonlight dark",
        "redlamp:rose-pine dark", "redlamp:rose-pine light", "redlamp:nord dark", "redlamp:everforest dark",
        "redlamp:everforest light", "redlamp:dracula dark", "redlamp:solarized dark", "redlamp:one-dark dark",
        "redlamp:one-dark light", "redlamp:monokai dark", "redlamp:sakura dark", "redlamp:copper dark",
        "redlamp:mc-vinyl-noir dark", "redlamp:mc-neon-dusk dark", "redlamp:mc-burnt-tape dark", "redlamp:mc-burnt-tape light",
        "redlamp:mc-phantom-frequency dark", "redlamp:mc-velvet-static dark",
    ],
    .tertiary: [
        "jaw dark", "jaw light", "omarchy:catppuccin-latte", "omarchy:catppuccin", "omarchy:ethereal", "omarchy:everforest",
        "omarchy:flexoki-light", "omarchy:gruvbox", "omarchy:kanagawa", "omarchy:last-horizon", "omarchy:matte-black",
        "omarchy:miasma", "omarchy:nord", "omarchy:osaka-jade", "omarchy:retro-82", "omarchy:ristretto", "omarchy:rose-pine",
        "omarchy:solitude", "omarchy:starship", "omarchy:tokyo-night", "omarchy:vantablack", "omarchy:white",
        "redlamp:neutral dark", "redlamp:redlamp dark", "redlamp:redlamp light", "redlamp:kanagawa dark", "redlamp:kanagawa light",
        "redlamp:tokyo-night light", "redlamp:moonlight light", "redlamp:catppuccin light", "redlamp:rose-pine dark",
        "redlamp:nord dark", "redlamp:nord light", "redlamp:everforest dark", "redlamp:everforest light", "redlamp:gruvbox dark",
        "redlamp:gruvbox light", "redlamp:dracula dark", "redlamp:dracula light", "redlamp:solarized dark",
        "redlamp:solarized light", "redlamp:one-dark light", "redlamp:monokai light", "redlamp:ocean dark", "redlamp:ocean light",
        "redlamp:sakura dark", "redlamp:sakura light", "redlamp:copper dark", "redlamp:copper light", "redlamp:mc-vinyl-noir dark",
        "redlamp:mc-vinyl-noir light", "redlamp:mc-neon-dusk dark", "redlamp:mc-neon-dusk light", "redlamp:mc-burnt-tape dark",
        "redlamp:mc-burnt-tape light", "redlamp:mc-phantom-frequency dark", "redlamp:mc-phantom-frequency light",
        "redlamp:mc-velvet-static dark", "redlamp:mc-velvet-static light",
    ],
    .accentText: [
        "jaw light", "omarchy:last-horizon", "omarchy:miasma", "omarchy:nord", "omarchy:osaka-jade", "omarchy:rose-pine",
        "omarchy:solitude", "omarchy:white", "redlamp:neutral dark", "redlamp:neutral light", "redlamp:redlamp dark",
        "redlamp:redlamp light", "redlamp:tokyo-night light", "redlamp:rose-pine light", "redlamp:nord dark",
        "redlamp:nord light", "redlamp:everforest light", "redlamp:gruvbox light", "redlamp:solarized dark",
        "redlamp:solarized light", "redlamp:monokai dark", "redlamp:monokai light", "redlamp:sakura light",
        "redlamp:copper light", "redlamp:mc-vinyl-noir light",
    ],
    .link: [
        "jaw light", "omarchy:catppuccin-latte", "omarchy:flexoki-light", "omarchy:kanagawa", "omarchy:rose-pine",
        "omarchy:solitude", "redlamp:neutral dark", "redlamp:neutral light", "redlamp:redlamp dark", "redlamp:redlamp light",
        "redlamp:tokyo-night light", "redlamp:nord dark", "redlamp:nord light", "redlamp:everforest light",
        "redlamp:gruvbox light", "redlamp:solarized dark", "redlamp:solarized light", "redlamp:monokai dark",
        "redlamp:copper light",
    ],
    .mention: [
        "jaw light", "omarchy:catppuccin-latte", "omarchy:flexoki-light", "omarchy:kanagawa", "omarchy:last-horizon",
        "omarchy:matte-black", "omarchy:miasma", "omarchy:osaka-jade", "omarchy:rose-pine", "redlamp:neutral light",
        "redlamp:redlamp light", "redlamp:kanagawa light", "redlamp:tokyo-night light", "redlamp:moonlight light",
        "redlamp:catppuccin light", "redlamp:rose-pine light", "redlamp:nord light", "redlamp:everforest light",
        "redlamp:gruvbox light", "redlamp:dracula light", "redlamp:solarized light", "redlamp:one-dark dark",
        "redlamp:monokai light", "redlamp:ocean light", "redlamp:sakura light", "redlamp:copper light",
        "redlamp:mc-vinyl-noir light", "redlamp:mc-neon-dusk light", "redlamp:mc-burnt-tape light",
        "redlamp:mc-phantom-frequency light", "redlamp:mc-velvet-static light",
    ],
    .failed: [
        "jaw light", "omarchy:flexoki-light", "omarchy:gruvbox", "omarchy:kanagawa", "omarchy:last-horizon", "omarchy:lumon",
        "omarchy:miasma", "omarchy:nord", "omarchy:retro-82", "omarchy:solitude", "redlamp:neutral light", "redlamp:redlamp dark",
        "redlamp:tokyo-night light", "redlamp:moonlight light", "redlamp:nord dark", "redlamp:nord light",
        "redlamp:everforest dark", "redlamp:everforest light", "redlamp:gruvbox dark", "redlamp:dracula dark",
        "redlamp:dracula light", "redlamp:solarized dark", "redlamp:one-dark light", "redlamp:ocean light", "redlamp:copper dark",
        "redlamp:mc-burnt-tape dark",
    ],
]

/// How many of each half's eight senders the hold moves.
let heldSenders: [String: Int] = [
    "jaw light": 6, "omarchy:catppuccin-latte": 6, "omarchy:everforest": 1, "omarchy:flexoki-light": 7, "omarchy:gruvbox": 1,
    "omarchy:kanagawa": 5, "omarchy:last-horizon": 4, "omarchy:lumon": 2, "omarchy:lupine": 1, "omarchy:matte-black": 2,
    "omarchy:miasma": 5, "omarchy:nord": 3, "omarchy:osaka-jade": 4, "omarchy:retro-82": 4, "omarchy:rose-pine": 6,
    "omarchy:solitude": 4, "redlamp:redlamp dark": 3, "redlamp:tokyo-night dark": 1, "redlamp:one-dark dark": 2,
    "redlamp:monokai dark": 3, "redlamp:ocean dark": 4,
]

@Test func `every half reads where JawUI draws it once held, the hold moving what a port of it found, each colour to within 0.05 of its floor`() throws {
    var moved: [Roles.Ink: [String]] = [:]
    var senders: [String: Int] = [:]
    for (name, mapped) in try mappedHalves() {
        let held = mapped.held()
        readsWhereDrawn(held, name)
        #expect(held.held() == held, "\(name): roles that are held come through the hold as they are")
        #expect([held.ground, held.surface, held.selection, held.yours, held.text, held.separator, held.accent]
            == [mapped.ground, mapped.surface, mapped.selection, mapped.yours, mapped.text, mapped.separator, mapped.accent],
            "\(name): the grounds, the text, the borders and the accent's fills as the theme wrote them")
        for hold in Roles.holds {
            let (was, now) = (mapped.colours(hold.ink), held.colours(hold.ink))
            for (before, after) in zip(was, now) where after != before {
                let worst = hold.on.map { Contrast.ratio(after.hex, held.colour(of: $0, under: after).hex) }.min() ?? 0
                #expect(worst <= hold.at + 0.05, "\(name): \(hold.ink) lands at \(worst) on the worst of its grounds")
            }
            if hold.ink == .senders {
                let n = zip(was, now).count { $0 != $1 }
                if n > 0 { senders[name] = n }
            } else if was != now {
                moved[hold.ink, default: []].append(name)
            }
        }
    }
    for ink in Roles.Ink.allCases where ink != .senders {
        #expect(moved[ink, default: []].sorted() == heldMoves[ink, default: []].sorted(), "\(ink)")
    }
    #expect(senders == heldSenders)
}

@Test func `a mention of you is held on its own tint, which moves with it, in passes that always end`() throws {
    let grounds = try #require(Roles.holds.first { $0.ink == .mention }).on
    var most = 0
    for (name, mapped) in try mappedHalves() {
        let (colour, passes) = mapped.lifted(mapped.mention, on: grounds, at: 3)
        #expect(colour == mapped.held().mention, "\(name)")
        most = max(most, passes)
    }
    #expect(most == 3, "three passes at the most in any half, as a port of the hold found")
    let r = Catalog.jaw.dark
    let (far, passes) = r.lifted(r.surface, on: [.ownTint(.surface)], at: 3)
    #expect(far.hex == 0x767F9D && passes == 3, "the surface's own colour on its own tint, as far from reading as a colour gets, in three")
    #expect(Contrast.ratio(far.hex, r.colour(of: .ownTint(.surface), under: far).hex) >= 3)
    let none = r.lifted(r.mention, on: [], at: 3)
    #expect(none.colour == r.mention && none.passes == 0, "with no grounds, nothing to move")
}

@Test func `every theme the catalog lists comes held, Jaw's Tokyo Night too, and an unread count reads on the accent's badge`() throws {
    let themes = Catalog.themes(omarchy: try servedThemes(), accent: .blue)
    let drawn = themes.flatMap { theme in Slot.allCases.compactMap { theme.roles($0) } }
    let held = try mappedHalves().map { $0.roles.held() }
    #expect(drawn.count == 69 && held.count == 69 && Set(drawn) == Set(held), "the catalog's halves are the mappings' held")
    #expect(Catalog.jaw.dark == Catalog.tokyoNight.held() && Catalog.jaw.light == Catalog.tokyoNightDay.held(),
            "and so is the family the slots fall back on and the harness and the pictures draw in")
    let counts = drawn.map { r in Contrast.ratio(r.onFill.hex, r.accent.hex) }
    #expect(counts.allSatisfy { $0 >= 3 } && abs((counts.min() ?? 0) - 3.4245) < 0.0001,
            "the count on its badge, in the accent as written, at 3.42:1 at the least, in Rosé Pine")
}

@Test func `a mark on a fill keeps the ground in dark and white in light wherever that reads at 3:1 on it, as in every half there is, and otherwise takes whichever reads more`() throws {
    var pale = Catalog.jaw.light
    pale.accent = RGBA(0xF4E27A)
    #expect(Contrast.ratio(0xFFFFFF, pale.accent.hex) < 3, "white on a pale accent, as a newer jawd's theme could bring")
    #expect(pale.onFill(pale.accent) == pale.text && Contrast.ratio(pale.onFill.hex, pale.accent.hex) >= 3, "takes the text there")
    var deep = Catalog.jaw.dark
    deep.accent = RGBA(0x2A2F6E)
    #expect(Contrast.ratio(deep.ground.hex, deep.accent.hex) < 3, "and the ground on a deep one in dark")
    #expect(deep.onFill(deep.accent) == RGBA(0xFFFFFF) && Contrast.ratio(deep.onFill.hex, deep.accent.hex) >= 3, "takes white there")
    var halves = Catalog.themes(omarchy: try servedThemes(), accent: .blue).flatMap { theme in Slot.allCases.compactMap { theme.roles($0) } }
    for accent in systemAccents {
        halves += Slot.allCases.compactMap { Catalog.neutral(accent).roles($0) }
    }
    for r in halves {
        for fill in [r.accent, r.accent.mixed(with: r.text, 0.15), r.tertiary, r.mention] {
            #expect(r.onFill(fill) == (r.isDark ? r.ground : RGBA(0xFFFFFF)), "every half there is keeps its marks' colour exactly")
        }
    }
}

/// The system's accents as AppKit draws them on macOS 27.2, light then dark: each colour System
/// Settings offers, Graphite as the system's grey, and the system's blue beside the default accent;
/// and orange, yellow and green as AppKit drew them before macOS 26.
let systemAccents: [SystemAccent] = ([
    (0x0088FF, 0x0091FF), (0xCB30E0, 0xDB34F2), (0xFF2D55, 0xFF375F), (0xFF383C, 0xFF4245), (0xFF8D28, 0xFF9230),
    (0xFFCC00, 0xFFD600), (0x34C759, 0x30D158), (0x8E8E93, 0x98989D), (0xF7821B, 0xF7821B), (0xFFC600, 0xFFC600),
    (0x62BA46, 0x62BA46),
] as [(UInt32, UInt32)]).map { SystemAccent(dark: RGBA($0.1), light: RGBA($0.0)) }

@Test func `every word and mark drawn on a fill of a theme's own colours reads at 3:1, in every half and in Neutral under each of the system's accents`() throws {
    let themes = Catalog.themes(omarchy: try servedThemes(), accent: .blue)
    var halves = themes.flatMap { theme in Slot.allCases.compactMap { slot in theme.roles(slot).map { ("\(theme.id) \(slot)", $0) } } }
    for accent in [SystemAccent.blue] + systemAccents {
        for slot in Slot.allCases {
            halves.append(("Neutral \(slot) under \(String(format: "%06X", slot.isDark ? accent.dark.hex : accent.light.hex))",
                           try #require(Catalog.neutral(accent).roles(slot))))
        }
    }
    #expect(halves.count == 69 + 24)
    for (name, r) in halves {
        let fills = [("the accent: the unread count, the discs' ▶, ♪ and ×, a poll's ✓ and the \"new ↓\" pill", r.accent),
                     ("a filled button under the pointer", r.accent.mixed(with: r.text, 0.15)),
                     ("a muted chat's badge", r.tertiary), ("the @ badge", r.mention)]
        for (what, fill) in fills {
            let ratio = Contrast.ratio(r.onFill.hex, fill.hex)
            #expect(ratio >= 3, "\(name): on \(what) at \(ratio)")
        }
    }
}
