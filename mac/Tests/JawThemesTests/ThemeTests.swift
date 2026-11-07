import Foundation
@testable import JawThemes
import Testing

/// Every theme there is to choose, the two slots that hold them, and what each slot draws: Jaw's
/// Tokyo Night first, Omarchy's as jawd serves them, then Redlamp's, Neutral in the accent it's given;
/// a choice filling both slots, the slot of its mode, or one; and Tokyo Night drawn where a slot's
/// theme can't be had.

/// Omarchy's themes as jawd serves them: read from `protocol/themes/`, named as jaw-proto names them,
/// in the order of their IDs.
func servedThemes() throws -> [OmarchyTheme] {
    try omarchyFiles().map { OmarchyTheme(id: $0.id, name: Omarchy.name($0.id), dark: $0.dark, colours: $0.colours) }
        .sorted { $0.id < $1.id }
}

@Test func `the catalog lists Jaw's Tokyo Night, then Omarchy's as jawd serves them, then Redlamp's, and finds each by its ID`() throws {
    let served = try servedThemes()
    let themes = Catalog.themes(omarchy: served, accent: .blue)
    let sources: [ThemeSource] = [.jaw] + Array(repeating: ThemeSource.omarchy, count: 23) + Array(repeating: ThemeSource.redlamp, count: 22)
    #expect(themes.map { $0.source } == sources)
    #expect(themes[0] == Catalog.jawTheme && Catalog.jawTheme.id == "jaw" && Catalog.jawTheme.name == "Tokyo Night")
    #expect(Catalog.jawTheme.name(.dark) == "Tokyo Night" && Catalog.jawTheme.name(.light) == "Tokyo Night Day", "each half by its own name")
    #expect(themes.filter { $0.id != "jaw" }.allSatisfy { $0.name(.dark) == $0.name && $0.name(.light) == $0.name },
            "every other theme's halves share its name")
    #expect(themes[0].dark == Catalog.jaw.dark && themes[0].light == Catalog.jaw.light)
    #expect(Array(themes.map { $0.id }[1 ... 3]) == ["omarchy:catppuccin", "omarchy:catppuccin-latte", "omarchy:ethereal"])
    #expect(Array(themes.map { $0.id }[24 ... 26]) == ["redlamp:neutral", "redlamp:redlamp", "redlamp:kanagawa"])
    #expect(themes.filter { $0.dark != nil }.count == 41 && themes.filter { $0.light != nil }.count == 28,
            "the dark slot's gallery has 41 cards and the light one's 28, as the design counts them")
    #expect(themes.filter { $0.source == .omarchy }.allSatisfy { ($0.dark == nil) != ($0.light == nil) }, "one half each")
    #expect(themes.first { $0.id == "omarchy:rose-pine" }?.name == "Rosé Pine")
    #expect(Omarchy.name("last-horizon") == "Last Horizon" && Omarchy.name("retro-82") == "Retro 82")
    for theme in themes {
        #expect(Catalog.theme(id: theme.id, omarchy: served, accent: .blue) == theme, "\(theme.id)")
    }
    #expect(Catalog.theme(id: "nord", omarchy: served, accent: .blue) == nil, "an ID names its source")
    #expect(Catalog.theme(id: "omarchy:nord", omarchy: [], accent: .blue) == nil, "Omarchy's only once jawd serves them")
    let broken = OmarchyTheme(id: "broken", name: "Broken", dark: true, colours: ["accent": "#ffffff"])
    #expect(Catalog.themes(omarchy: [broken], accent: .blue).count == 23, "one whose colours don't map is left out")
}

@Test func `a slot draws its theme's half, or Jaw's Tokyo Night's where no source has the theme or it has no half of the slot's mode`() throws {
    let served = try servedThemes()
    let nord = try #require(RedlampCatalog.families.first { $0.id == "nord" })
    let both = Slots(dark: "redlamp:nord", light: "redlamp:nord")
    #expect(Catalog.roles(.dark, of: both, omarchy: served, accent: .blue) == RedlampRoles.roles(nord.dark, dark: true).held())
    #expect(Catalog.roles(.light, of: both, omarchy: served, accent: .blue) == RedlampRoles.roles(nord.light, dark: false).held())
    let gruvbox = try #require(served.first { $0.id == "gruvbox" })
    let held = Slots(dark: "omarchy:gruvbox", light: "omarchy:gruvbox")
    #expect(Catalog.roles(.dark, of: held, omarchy: served, accent: .blue) == Omarchy.roles(gruvbox.colours, dark: true)?.held())
    #expect(Catalog.roles(.light, of: held, omarchy: served, accent: .blue) == Catalog.jaw.light, "Gruvbox has no light half")
    #expect(Catalog.roles(.dark, of: held, omarchy: [], accent: .blue) == Catalog.jaw.dark, "jawd hasn't served it")
    let unknown = Slots(dark: "nord", light: "redlamp:nowhere")
    #expect(Catalog.roles(.dark, of: unknown, omarchy: served, accent: .blue) == Catalog.jaw.dark)
    #expect(Catalog.roles(.light, of: unknown, omarchy: served, accent: .blue) == Catalog.jaw.light)
}

@Test func `a theme with both halves fills both slots, one of Omarchy's the slot of its mode, and ⌥ only the slot chosen for`() throws {
    let themes = Catalog.themes(omarchy: try servedThemes(), accent: .blue)
    func theme(_ id: String) throws -> Theme {
        try #require(themes.first { $0.id == id })
    }
    let start = Slots()
    #expect(start == Slots(dark: "jaw", light: "jaw") && start[.dark] == "jaw" && start[.light] == "jaw")
    let nord = try start.choosing(theme("redlamp:nord"), for: .dark)
    #expect(nord == Slots(dark: "redlamp:nord", light: "redlamp:nord"))
    let gruvbox = try nord.choosing(theme("omarchy:gruvbox"), for: .dark)
    #expect(gruvbox == Slots(dark: "omarchy:gruvbox", light: "redlamp:nord"))
    let latte = try gruvbox.choosing(theme("omarchy:catppuccin-latte"), for: .dark)
    #expect(latte == Slots(dark: "omarchy:gruvbox", light: "omarchy:catppuccin-latte"), "a light theme fills the light slot")
    let only = try latte.choosing(theme("jaw"), for: .dark, only: true)
    #expect(only == Slots(dark: "jaw", light: "omarchy:catppuccin-latte"))
    let wrong = try only.choosing(theme("omarchy:nord"), for: .light, only: true)
    #expect(wrong == Slots(dark: "omarchy:nord", light: "omarchy:catppuccin-latte"), "a slot never holds a theme of the other mode")
}

@Test func `Neutral draws in the accent it's given, in each appearance, and Redlamp's families don't change with it`() throws {
    let purple = SystemAccent(dark: RGBA(0xBF5AF2), light: RGBA(0xAF52DE))
    let neutral = try #require(Catalog.theme(id: "redlamp:neutral", omarchy: [], accent: purple))
    #expect(neutral.dark?.accent.hex == 0xBF5AF2 && neutral.light?.accent.hex == 0xAF52DE, "the system's purple reads as it is")
    #expect(neutral.dark == RedlampRoles.neutral(dark: true, accent: purple).held(), "its halves are Neutral's mapping in the purple, held")
    #expect(purple.orange == .orange && purple.green == .green && purple.red == .red,
            "an accent given without the system's other colours takes macOS 27.0.1's")
    #expect(SystemAccent.blue == SystemAccent(dark: RGBA(0x007AFF), light: RGBA(0x007AFF),
                                              orange: SystemColour(dark: RGBA(0xFF9230), light: RGBA(0xFF8D28)),
                                              green: SystemColour(dark: RGBA(0x30D158), light: RGBA(0x34C759)),
                                              red: SystemColour(dark: RGBA(0xFF4245), light: RGBA(0xFF383C))),
            "macOS 27.0.1's default accent, the same blue in both appearances, and its orange, green and red, as the build Mac draws them")
    #expect(neutral != Catalog.theme(id: "redlamp:neutral", omarchy: [], accent: .blue))
    #expect(Catalog.theme(id: "redlamp:nord", omarchy: [], accent: purple) == Catalog.theme(id: "redlamp:nord", omarchy: [], accent: .blue))
}
