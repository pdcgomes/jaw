import AppKit
import JawThemes
@testable import JawUI
import Testing

/// The two slots in the app's preferences: Jaw's Tokyo Night in both at first, the `theme` key read
/// into both once while they're unset and left as it is, `themeDark` and `themeLight` kept, and the
/// style drawn from the slot of the appearance, worked out again when the slots or the accent change.

@MainActor @Test func `the slots start as Jaw's Tokyo Night, take the theme key once while unset, and are kept as themeDark and themeLight`() throws {
    #expect(try scratchPreferences("slots-fresh").slots == Slots(dark: "jaw", light: "jaw"))
    let suite = "jaw.tests.slots-kept"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    defaults.set("redlamp:nord", forKey: "theme")
    let preferences = Preferences(defaults: defaults, accent: .blue)
    #expect(preferences.slots == Slots(dark: "redlamp:nord", light: "redlamp:nord"), "read into both while they're unset")
    preferences.slots[.dark] = "omarchy:gruvbox"
    #expect(defaults.string(forKey: "themeDark") == "omarchy:gruvbox" && defaults.string(forKey: "themeLight") == "redlamp:nord")
    #expect(defaults.string(forKey: "theme") == "redlamp:nord", "left as it is, so an earlier build still reads it")
    defaults.set("jaw", forKey: "theme")
    #expect(Preferences(defaults: defaults, accent: .blue).slots == Slots(dark: "omarchy:gruvbox", light: "redlamp:nord"),
            "read only while the slots are unset")
}

@MainActor @Test func `the style is the slot's theme's half for the appearance, drawn again when the slots or the accent change`() throws {
    let preferences = try scratchPreferences("slots-style")
    var told = 0
    preferences.changed.append { told += 1 }
    #expect(preferences.style(dark: true).roles == Catalog.jaw.dark && preferences.style(dark: false).roles == Catalog.jaw.light)
    let nord = try #require(RedlampCatalog.families.first { $0.id == "nord" })
    preferences.slots = Slots(dark: "redlamp:nord", light: "redlamp:neutral")
    #expect(preferences.style(dark: true).roles == RedlampRoles.roles(nord.dark, dark: true))
    #expect(preferences.style(dark: false).roles.accent.hex == 0x007AFF, "Neutral in the system's blue")
    let purple = SystemAccent(dark: RGBA(0xBF5AF2), light: RGBA(0xAF52DE))
    preferences.accent = purple
    #expect(preferences.style(dark: false).roles.accent.hex == 0xAF52DE, "and again in the accent the system has now")
    #expect(preferences.themes.first { $0.id == "redlamp:neutral" }?.light?.accent.hex == 0xAF52DE)
    preferences.accent = purple
    #expect(told == 2, "once for the slots and once for the accent, and not for an accent that didn't change")
    #expect(preferences.themes.map { $0.id } == Catalog.themes(omarchy: [], accent: purple).map { $0.id })
}

@MainActor @Test func `a snapshot's preferences read what's kept and write nothing back, telling the app all the same`() throws {
    let suite = "jaw.tests.slots-unkept"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    defaults.set("redlamp:nord", forKey: "themeDark")
    let preferences = Preferences(defaults: defaults, accent: .blue, keeps: false)
    var told = 0
    preferences.changed.append { told += 1 }
    #expect(preferences.slots.dark == "redlamp:nord", "it reads what's kept")
    preferences.slots = Slots(dark: "jaw", light: "jaw")
    preferences.appearance = .dark
    #expect(told == 2 && preferences.slots.dark == "jaw" && preferences.appearance == .dark)
    #expect(defaults.string(forKey: "themeDark") == "redlamp:nord" && defaults.string(forKey: "appearance") == nil,
            "and writes nothing back, so a run on the build Mac leaves the next one's preferences as they were")
}
