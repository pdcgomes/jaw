@testable import JawHarness
import JawThemes
import JawUI
import SwiftUI
import Testing

/// The catalog the sidebar lists, the variants a sweep draws a scene in, and the conditions it's
/// drawn in.

func stub(_ id: String, _ section: HarnessSection, axes: Axes = []) -> HarnessScene {
    HarnessScene(id: id, title: id, symbol: "circle", synopsis: "", section: section, covers: [], axes: axes) { _ in
        EmptyView()
    }
}

@Test func `the sidebar lists the sections that have scenes in their order, and each one's scenes as registered`() {
    var catalog = HarnessCatalog()
    catalog.register(stub("b", .states))
    catalog.register(stub("a", .foundations))
    catalog.register(stub("c", .states))
    #expect(catalog.sections == [.foundations, .states])
    #expect(catalog.scenes(in: .states).map(\.id) == ["b", "c"])
    #expect(catalog.scene(id: "c")?.section == .states)
    #expect(catalog.scene(id: "z") == nil && catalog.scene(id: nil) == nil)
}

@Test func `a sweep draws every scene dark and light, and in each look and density it asks for`() {
    #expect(Variant.all([]).map(\.name) == ["dark", "light"])
    #expect(Variant.all(.look).map(\.name) == ["dark-bubbles", "dark-lines", "light-bubbles", "light-lines"])
    #expect(Variant.all([.look, .density]).map(\.name) == [
        "dark-bubbles", "dark-bubbles-compact", "dark-lines", "dark-lines-compact",
        "light-bubbles", "light-bubbles-compact", "light-lines", "light-lines-compact",
    ])
    #expect(stub("s", .conversation, axes: .density).variants.map(\.name) == ["dark", "dark-compact", "light", "light-compact"])
}

@Test func `a variant changes only what it names`() {
    var mine = Conditions()
    mine.textSize = 15
    mine.look = .lines
    mine.density = .compact
    mine.font = .mono
    mine.reduceMotion = true
    var expected = mine
    expected.dark = false
    #expect(Variant(dark: false, look: nil, density: nil).applied(to: mine) == expected)
    expected = mine
    expected.look = .bubbles
    expected.density = .comfortable
    #expect(Variant(dark: true, look: .bubbles, density: .comfortable).applied(to: mine) == expected)
}

@Test func `a scene is drawn in the app's own style, in the theme's roles`() {
    var conditions = Conditions()
    #expect(conditions.roles == Catalog.jaw.dark)
    conditions.dark = false
    #expect(conditions.style == Style(roles: Catalog.jaw.light))
}

@Test func `a scene is drawn in any theme's half for its appearance, and in Tokyo Night's where the theme has none`() throws {
    #expect(Fixtures.omarchy.count == 23 && Fixtures.themes.count == 46, "Omarchy's, read from the harness's bundle, beside Jaw's and Redlamp's")
    var conditions = Conditions()
    conditions.theme = "redlamp:nord"
    #expect(conditions.roles == (try #require(Fixtures.themes.first { $0.id == "redlamp:nord" })).dark)
    conditions.theme = "omarchy:gruvbox"
    conditions.dark = false
    #expect(conditions.roles == Catalog.jaw.light, "Gruvbox is dark only")
    conditions.theme = "redlamp:neutral"
    #expect(conditions.roles.accent.hex == 0x007AFF, "Neutral in #007AFF, the accent the harness gives it")
}
