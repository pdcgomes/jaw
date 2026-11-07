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
    #expect(preferences.style(dark: true).roles == RedlampRoles.roles(nord.dark, dark: true).held())
    #expect(preferences.style(dark: false).roles.accent.hex == 0x007AFF, "Neutral in the system's blue")
    #expect(preferences.themes.first { $0.id == "redlamp:neutral" }?.light?.accent.hex == 0x007AFF, "the list, worked out once")
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
    let gruvbox = try omarchy("gruvbox")
    let seeded = gruvbox.colours.merging(["id": "gruvbox", "name": "Gruvbox", "accent": "#ff0000"]) { $1 }
    defaults.set("omarchy:gruvbox", forKey: "themeDark")
    defaults.set(seeded, forKey: "themeDarkColours")
    let preferences = Preferences(defaults: defaults, accent: .blue, keeps: false)
    preferences.served = [gruvbox]
    var told = 0
    preferences.changed.append { told += 1 }
    #expect(preferences.slots.dark == "omarchy:gruvbox", "it reads what's kept")
    preferences.slots = Slots(dark: "jaw", light: "jaw")
    preferences.appearance = .dark
    #expect(told == 2 && preferences.slots.dark == "jaw" && preferences.appearance == .dark)
    #expect(defaults.string(forKey: "themeDark") == "omarchy:gruvbox" && defaults.string(forKey: "appearance") == nil,
            "and writes nothing back, so a run on the build Mac leaves the next one's preferences as they were")
    #expect(defaults.dictionary(forKey: "themeDarkColours") as? [String: String] == seeded,
            "the copy of the slot's Omarchy theme too, though jawd served it with other colours and the slot left it")
}

/// A suite's defaults with preferences given for one run, as `-themeDark omarchy:nord` gives one to the
/// app: read before what's kept, and the argument domain's, here the test's own, as the process's is
/// shared by every test running beside it.
final class GivenDefaults: UserDefaults {
    var given: [String: Any] = [:]

    override func object(forKey key: String) -> Any? {
        given[key] ?? super.object(forKey: key)
    }

    override func volatileDomain(forName name: String) -> [String: Any] {
        name == UserDefaults.argumentDomain ? given : super.volatileDomain(forName: name)
    }
}

@MainActor @Test func `a preference given for one run is read before what's kept and never written, nor the copy of a slot given so, whatever the run changes`() throws {
    let suite = "jaw.tests.slots-one-run"
    let defaults = try #require(GivenDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    let gruvbox = try omarchy("gruvbox"), white = try omarchy("white")
    let darkCopy = gruvbox.colours.merging(["id": "gruvbox", "name": "Gruvbox", "accent": "#ff0000"]) { $1 }
    let lightCopy = white.colours.merging(["id": "white", "name": "White"]) { $1 }
    defaults.set("omarchy:gruvbox", forKey: "themeDark")
    defaults.set(darkCopy, forKey: "themeDarkColours")
    defaults.set("omarchy:white", forKey: "themeLight")
    defaults.set(lightCopy, forKey: "themeLightColours")
    defaults.set("light", forKey: "appearance")
    defaults.set("bubbles", forKey: "look")
    defaults.given = ["themeDark": "omarchy:nord", "appearance": "dark", "look": "lines"]
    let run = Preferences(defaults: defaults, accent: .blue)
    #expect(run.slots == Slots(dark: "omarchy:nord", light: "omarchy:white") && run.appearance == .dark && run.look == .lines,
            "what's given for the run is read before what's kept")
    run.served = try allOmarchy()
    let arrowed = try #require(Gallery(run.themes, for: .dark, omarchyLine: nil).moving(from: run.slots.dark, .right))
    run.preview(arrowed, for: .dark)
    let kanagawa = try #require(run.themes.first { $0.id == "omarchy:kanagawa" })
    run.choose(kanagawa, for: .dark, only: false)
    run.appearance = .system
    run.textSize = 15
    #expect(arrowed.id == "omarchy:osaka-jade" && run.slots == Slots(dark: "omarchy:kanagawa", light: "omarchy:white"),
            "an arrow, then a click on Kanagawa's card, drawn in the run")
    defaults.given = [:]
    let next = Preferences(defaults: defaults, accent: .blue)
    #expect(next.slots == Slots(dark: "omarchy:gruvbox", light: "omarchy:white"),
            "the next launch finds the owner's own slots, though the run chose for the one given")
    #expect(next.appearance == .light && next.look == .bubbles, "and the appearance and the look as they were kept")
    #expect(defaults.dictionary(forKey: "themeDarkColours") as? [String: String] == darkCopy,
            "the copy of the owner's own dark theme as it was, though jawd served the run's Nord and the run left it")
    #expect(defaults.dictionary(forKey: "themeLightColours") as? [String: String] == lightCopy, "and the light slot's, which jawd serves as it was")
    #expect(next.textSize == 15, "while a preference the run wasn't given is kept as it changes")
    #expect(next.roles(.dark) == Omarchy.roles(darkCopy.filter { $0.key != "id" && $0.key != "name" }, dark: true)?.held(),
            "so before jawd answers, the next launch draws the owner's Gruvbox from its copy")
}

/// One of Omarchy's themes as jawd serves it, read from `protocol/themes/`.
func omarchy(_ id: String) throws -> OmarchyTheme {
    let file = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().appending(path: "protocol/themes/\(id).toml")
    return try #require(OmarchyTheme(id: id, file: String(contentsOf: file, encoding: .utf8)))
}

@MainActor @Test func `a slot's Omarchy theme is remembered as jawd served it, drawn before jawd answers, and dropped once jawd serves themes without it`() throws {
    let suite = "jaw.tests.slots-remembered"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    let gruvbox = try omarchy("gruvbox"), latte = try omarchy("catppuccin-latte")
    let first = Preferences(defaults: defaults, accent: .blue)
    var toldFirst = 0
    first.changed.append { toldFirst += 1 }
    first.served = [gruvbox, latte]
    #expect(toldFirst == 0, "jawd's first answer leaves Jaw's in both slots, so nothing is drawn again")
    let chosen = try #require(first.themes.first { $0.id == "omarchy:gruvbox" })
    first.slots = first.slots.choosing(chosen, for: .dark)
    let dark = try #require(Omarchy.roles(gruvbox.colours, dark: true)).held()
    #expect(first.roles(.dark) == dark && first.roles(.light) == Catalog.jaw.light)
    let copy = defaults.dictionary(forKey: "themeDarkColours") as? [String: String]
    #expect(copy?["id"] == "gruvbox" && copy?["name"] == "Gruvbox" && copy?["background"] == "#282828" && copy?.count == 27)
    #expect(defaults.dictionary(forKey: "themeLightColours") == nil, "the light slot holds Jaw's")
    let nord = try #require(first.themes.first { $0.id == "redlamp:nord" })
    first.slots = first.slots.choosing(nord, for: .dark)
    #expect(defaults.dictionary(forKey: "themeDarkColours") == nil, "a copy goes as its slot leaves Omarchy's themes")
    first.slots = Slots(dark: "omarchy:gruvbox", light: "jaw")
    #expect(defaults.dictionary(forKey: "themeDarkColours") as? [String: String] == copy, "and comes back with it")

    let next = Preferences(defaults: defaults, accent: .blue)
    var told = 0
    next.changed.append { told += 1 }
    #expect(next.served == nil && next.roles(.dark) == dark, "the next launch draws Gruvbox before jawd answers")
    #expect(next.themes.filter { $0.source == .omarchy }.map { $0.id } == ["omarchy:gruvbox"], "the slots' themes, from their copies")
    next.served = [gruvbox, latte]
    #expect(told == 0, "jawd serving the colours the copy has draws nothing again, on the launch's path")
    #expect(next.themes.filter { $0.source == .omarchy }.map { $0.id } == ["omarchy:gruvbox", "omarchy:catppuccin-latte"],
            "and once jawd answers, every theme it serves")
    var newer = gruvbox
    newer.colours["accent"] = "#ff0000"
    next.served = [newer, latte]
    #expect(told == 1 && next.roles(.dark) == Omarchy.roles(newer.colours, dark: true)?.held(), "jawd's colours replace the copy, drawn once")
    #expect((defaults.dictionary(forKey: "themeDarkColours") as? [String: String])?["accent"] == "#ff0000")
    next.served = [latte]
    #expect(told == 2 && next.roles(.dark) == Catalog.jaw.dark && next.slots.dark == "omarchy:gruvbox",
            "drawn as Tokyo Night, and the choice kept")
    #expect(defaults.dictionary(forKey: "themeDarkColours") == nil)
    defaults.set(["id": "nord", "name": "Nord"].merging(gruvbox.colours) { $1 }, forKey: "themeDarkColours")
    #expect(Preferences(defaults: defaults, accent: .blue).roles(.dark) == Catalog.jaw.dark, "a copy of another theme isn't drawn")
    let last = Preferences(defaults: defaults, accent: .blue)
    #expect(last.themes.allSatisfy { $0.source != .omarchy })
    last.slots.dark = "omarchy:nord"
    #expect(last.themes.filter { $0.source == .omarchy }.map { $0.id } == ["omarchy:nord"] && last.roles(.dark) == dark,
            "and is once the slot holds its theme")
}

@MainActor @Test func `until jawd serves its themes, the Omarchy section says why, and nothing while the app connects or asks`() throws {
    #expect(OmarchyState(.waiting).line == nil && OmarchyState(.up).line == nil)
    #expect(OmarchyState(.starting(failed: nil)).line == "Starting jawd")
    #expect(OmarchyState(.unreachable(why: "no socket", retry: 2)).line
        == "Omarchy's themes come from jawd, which isn't answering; they show once it does")
    #expect(OmarchyState(.starting(failed: "jawd isn't in the app")) == .unreachable, "a jawd that couldn't start isn't answering")
    #expect(OmarchyState.older.line == "This jawd is older and serves no themes; restart it")
    #expect(OmarchyState.refused("jawd is busy").line == "Omarchy's themes didn't come: jawd is busy")
    let preferences = try scratchPreferences("omarchy-state")
    #expect(preferences.omarchyState == .connecting && preferences.omarchyLine == nil)
    preferences.omarchyState = .older
    #expect(preferences.omarchyLine == OmarchyState.older.line)
    preferences.served = [try omarchy("gruvbox")]
    #expect(preferences.omarchyLine == nil, "once jawd has served its themes there's nothing to say, even when it goes")
}
