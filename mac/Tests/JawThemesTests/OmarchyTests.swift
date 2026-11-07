import Foundation
@testable import JawThemes
import Testing

/// Omarchy's themes, read from `protocol/themes/` as jaw-proto reads them and mapped as `theme.rs`
/// maps them, then held on the Mac's surface: the port lands on the terminal's own colours, each theme
/// moves what `theme.rs` and the surface move and nothing else, and every theme reads.

let themesFolder = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().appending(path: "protocol/themes")

/// Omarchy's themes as `protocol/themes/` holds them, by ID, each read as jaw-proto reads it.
func omarchyFiles() throws -> [(id: String, dark: Bool, colours: [String: String])] {
    let names = try FileManager.default.contentsOfDirectory(atPath: themesFolder.path).filter { $0.hasSuffix(".toml") }.sorted()
    return try names.map { name in
        let text = try String(contentsOf: themesFolder.appending(path: name), encoding: .utf8)
        let read = try #require(Omarchy.file(text), "\(name) reads")
        return (String(name.dropLast(5)), read.dark, read.colours)
    }
}

/// What every theme's roles read at, of every source and in each half: text at 4.5:1 wherever it's
/// drawn, every colour at 3:1 on the ground and on the surface, the accent on the ground, the dim text
/// in its band, and a selected row's online at 4.5:1 on its selection.
func reads(_ r: Roles, _ name: String) {
    for (ground, colour) in [("ground", r.ground), ("surface", r.surface), ("selection", r.selection), ("yours", r.yours)] {
        #expect(r.text.contrast(with: colour) >= 4.5, "\(name): text on the \(ground), \(r.text.contrast(with: colour))")
    }
    let colours = [("secondary", r.secondary), ("link", r.link), ("mention", r.mention), ("online", r.online), ("failed", r.failed)]
        + r.senders.map { ("a sender", $0) }
    for (role, colour) in colours {
        #expect(colour.contrast(with: r.ground) >= 3 && colour.contrast(with: r.surface) >= 3,
                "\(name): \(role) on the ground and on the surface")
    }
    #expect(r.accent.contrast(with: r.ground) >= 3, "\(name): the accent")
    #expect((2.5 ... 4.51).contains(r.tertiary.contrast(with: r.ground)), "\(name): dim text \(r.tertiary.contrast(with: r.ground))")
    #expect(r.selectedOnline.contrast(with: r.selection) >= 4.5, "\(name): a selected row's online")
}

@Test func `every theme file reads as jaw-proto reads it, with its mode, its 25 colours and the fallbacks it fills`() throws {
    let themes = try omarchyFiles()
    #expect(themes.count == 23)
    #expect(themes.filter { !$0.dark }.map { $0.id }.sorted() == ["catppuccin-latte", "flexoki-light", "lupine", "rose-pine", "white"])
    #expect(themes.allSatisfy { $0.colours.count == 25 })
    for (id, brown) in [("last-horizon", "#362f3a"), ("solitude", "#6d6e6e"), ("white", "#252525")] {
        let c = try #require(themes.first { $0.id == id }?.colours)
        #expect(c["orange"] == c["yellow"] && c["brown"] == brown, "\(id): orange is its yellow, and brown that mixed half with black")
    }
    let gruvbox = try #require(themes.first { $0.id == "gruvbox" }?.colours)
    #expect(gruvbox["orange"] != gruvbox["yellow"] && gruvbox["brown"] == "#70432e", "a theme's own orange and brown are kept")
    #expect(Omarchy.file("mode = \"dim\"\n") == nil, "a mode that isn't dark or light")
    #expect(Omarchy.file("mode = \"dark\"\naccent \"#ffffff\"\n") == nil, "a line that isn't key = value")
    let file = try String(contentsOf: themesFolder.appending(path: "gruvbox.toml"), encoding: .utf8)
    #expect(Omarchy.file(file.replacingOccurrences(of: "accent = \"#7daea3\"", with: "accent = \"teal\"")) == nil, "a colour that isn't hex")
    #expect(Omarchy.file(file.replacingOccurrences(of: "mode = \"dark\"", with: "")) == nil, "a file with no mode")
    #expect(Omarchy.file(file.replacingOccurrences(of: "brown = \"#70432e\"", with: ""))?.colours["brown"] == "#71442e",
            "a brown left out comes from the theme's own orange, not its yellow")
}

@Test func `mix saturates past either end of its line as theme.rs's cast does, rather than trapping`() {
    #expect(Contrast.mix(0x204060, 0xFFFFFF, -1) == 0x000000, "below 0 a channel is 0, as Rust's `as u32` makes a negative")
    #expect(Contrast.mix(0x204060, 0xFFFFFF, 2) == 0xFFFFFF, "above 255 it's 255, as theme.rs's min(255) keeps it")
    #expect(Contrast.mix(0x204060, 0xFFFFFF, .nan) == 0x000000, "and NaN is 0, as Rust's cast makes it")
}

@Test func `the port lands on theme.rs's own Tokyo Night, grey by grey and colour by colour`() throws {
    let tokyo = try #require(omarchyFiles().first { $0.id == "tokyo-night" })
    let t = try #require(Omarchy.terminal(tokyo.colours.compactMapValues(Omarchy.hex)))
    #expect([t.bg, t.surface, t.selection, t.fg, t.text, t.muted, t.comment, t.gutter]
        == [0x1A1B26, 0x24283B, 0x2D3247, 0xC0CAF5, 0xA9B1D6, 0x8088B0, 0x565F89, 0x414868], "as theme.rs's own test pins them")
    #expect([t.accent, t.link, t.mention, t.senders[3], t.senders[7]] == [0x7AA2F7, 0x449DAB, 0xEB927B, 0xAD8EE6, 0x0DB9D7])
}

/// What `theme.rs` moves in each theme at Omarchy's 4c1b4cb, as its own test lists it.
let terminalMoves: [(moved: [String], themes: [String])] = [
    ([], ["catppuccin", "flexoki-light", "gruvbox", "hackerman", "lumon", "matte-black", "rose-pine"]),
    (["dim text raised"], ["catppuccin-latte", "everforest", "last-horizon", "miasma", "nord", "solitude", "white"]),
    (["borders lowered", "dim text lowered"], ["ethereal", "osaka-jade", "retro-82", "starship"]),
    (["borders lowered"], ["miasma", "ristretto", "vantablack", "white"]),
    (["surface raised"], ["last-horizon", "lupine", "solitude"]),
    (["selection raised"], ["kanagawa", "tokyo-night", "vantablack", "white"]),
]

@Test func `each theme moves the greys theme.rs moves, and no others, each grey and colour it moves landing within 0.05 of its target`() throws {
    for theme in try omarchyFiles() {
        let c = theme.colours.compactMapValues(Omarchy.hex)
        let t = try #require(Omarchy.terminal(c))
        let greys: [(String, UInt32, UInt32, UInt32)] = [
            ("surface", c["lighter_background"]!, t.surface, t.bg), ("selection", c["selection"]!, t.selection, t.surface),
            ("dim text", c["dark_foreground"]!, t.comment, t.bg), ("borders", c["muted"]!, t.gutter, t.bg),
        ]
        var moved: [String] = []
        for (what, was, now, ground) in greys where now != was {
            moved.append("\(what) \(Contrast.ratio(now, ground) > Contrast.ratio(was, ground) ? "raised" : "lowered")")
        }
        let listed = terminalMoves.filter { $0.themes.contains(theme.id) }.flatMap { $0.moved }
        #expect(moved.sorted() == listed.sorted(), "\(theme.id)")
        let picked = Contrast.fit(c["selection"]!, on: t.bg, toward: t.fg, low: 1.15, target: 1.25)
        var lands: [(String, UInt32, UInt32, UInt32, Double)] = [
            ("surface", c["lighter_background"]!, t.surface, t.bg, 1.1), ("dim text", c["dark_foreground"]!, t.comment, t.bg, 3),
            ("borders", c["muted"]!, t.gutter, t.bg, 1.8), ("selection", c["selection"]!, picked, t.bg, 1.25),
            ("selection on the surface", picked, t.selection, t.surface, 1.15), ("link", c["cyan"]!, t.link, t.bg, 3),
            ("mention", c["orange"]!, t.mention, t.bg, 3), ("online", c["green"]!, t.online, t.bg, 3),
            ("notice", c["yellow"]!, t.notice, t.bg, 3),
        ]
        for (was, now) in zip(["red", "yellow", "blue", "magenta", "cyan", "green", "orange", "bright_cyan"].map { c[$0]! }, t.senders) {
            lands.append(("a sender", was, now, t.bg, 3))
        }
        for (what, was, now, ground, target) in lands where now != was {
            #expect(abs(Contrast.ratio(now, ground) - target) <= 0.05, "\(theme.id): \(what) lands at \(Contrast.ratio(now, ground))")
        }
    }
}

/// What holding Omarchy's colours at 3:1 on the Mac's surface too moves, beyond what `theme.rs` moves,
/// at Omarchy's 4c1b4cb, as a port of the rule found on 6 November.
let surfaceMoves: [String: [String]] = [
    "catppuccin-latte": ["link", "online", "senders 5"],
    "flexoki-light": ["link", "mention", "online", "senders 6"],
    "kanagawa": ["failed", "senders 1"],
    "last-horizon": ["mention", "senders 2"],
    "matte-black": ["senders 1"],
    "miasma": ["failed", "mention", "senders 2"],
    "nord": ["failed", "senders 1"],
    "rose-pine": ["link", "mention", "senders 5"],
    "solitude": ["failed", "senders 1"],
]

@Test func `the Mac holds the terminal's colours at 3:1 on its surface too, which moves nine themes' and no others', each to within 0.05 of 3:1 there`() throws {
    for theme in try omarchyFiles() {
        let c = theme.colours.compactMapValues(Omarchy.hex)
        let t = try #require(Omarchy.terminal(c))
        let r = try #require(Omarchy.roles(theme.colours, dark: theme.dark))
        let red = Contrast.fit(c["red"]!, on: t.bg, toward: t.fg, low: 3, target: 3)
        var moved: [String] = []
        for (what, was, now) in [("link", t.link, r.link), ("mention", t.mention, r.mention), ("online", t.online, r.online),
                                 ("failed", red, r.failed)] where now.hex != was
        {
            moved.append(what)
            #expect(abs(Contrast.ratio(now.hex, t.surface) - 3) <= 0.05, "\(theme.id): \(what) lands at \(Contrast.ratio(now.hex, t.surface))")
        }
        let senders = zip(t.senders, r.senders).count { $0.0 != $0.1.hex }
        if senders > 0 { moved.append("senders \(senders)") }
        for (was, now) in zip(t.senders, r.senders) where now.hex != was {
            #expect(abs(Contrast.ratio(now.hex, t.surface) - 3) <= 0.05, "\(theme.id): a sender lands at \(Contrast.ratio(now.hex, t.surface))")
        }
        #expect(moved.sorted() == (surfaceMoves[theme.id] ?? []).sorted(), "\(theme.id)")
        #expect([r.ground, r.surface, r.selection, r.text, r.secondary, r.tertiary, r.separator, r.accent].map(\.hex)
            == [t.bg, t.surface, t.selection, t.fg, t.muted, t.comment, t.gutter, t.accent], "\(theme.id): the rest as the terminal has it")
        #expect(r.accentText == r.accent, "\(theme.id): the accent drawn as text, as written until the hold")
    }
}

@Test func `every one of Omarchy's themes reads on the Mac, in the half its mode is`() throws {
    for theme in try omarchyFiles() {
        let roles = try #require(Omarchy.roles(theme.colours, dark: theme.dark), "\(theme.id) maps")
        #expect(roles.isDark == theme.dark)
        reads(roles, theme.id)
    }
    var missing = try #require(omarchyFiles().first).colours
    missing["accent"] = nil
    #expect(Omarchy.roles(missing, dark: true) == nil, "a theme without one of its colours isn't mapped")
}
