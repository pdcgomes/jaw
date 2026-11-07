import AppKit
import JawThemes
@testable import JawUI
import SwiftUI
import Testing

/// The Appearance pane's gallery and slots: a slot's half of every theme that has one, in Jaw's,
/// Omarchy's and Redlamp's sections; the Omarchy section while jawd's themes aren't in; where the
/// arrows go, and the arrows pressed in a window, choosing and scrolling with no click first, and a
/// click anywhere in the pane giving the gallery the keys back; what each slot's tile says; what
/// VoiceOver reads a card as; and every card for each slot compared with a reference picture, so a
/// change to any mapping shows.

/// Omarchy's 23 as jawd serves them, read from `protocol/themes/`, in the order of their IDs.
func allOmarchy() throws -> [OmarchyTheme] {
    let folder = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().appending(path: "protocol/themes")
    return try FileManager.default.contentsOfDirectory(atPath: folder.path).filter { $0.hasSuffix(".toml") }
        .map { String($0.dropLast(5)) }.sorted().map { try omarchy($0) }
}

@MainActor @Test func `the gallery shows a slot's half of every theme that has one, in three sections, 41 for dark and 28 for light`() throws {
    let preferences = try scratchPreferences("gallery-sections")
    preferences.served = try allOmarchy()
    let dark = Gallery(preferences.themes, for: .dark, omarchyLine: preferences.omarchyLine)
    #expect(dark.sections.map(\.source) == [.jaw, .omarchy, .redlamp] && dark.sections.map(\.themes.count) == [1, 18, 22])
    #expect(dark.cards.count == 41 && dark.sections.allSatisfy { $0.line == nil })
    #expect(dark.cards.allSatisfy { $0.dark != nil })
    #expect(dark.sections.map(\.header) == ["Jaw's", "Omarchy's", "Redlamp's"])
    #expect(dark.sections.map(\.footer) == ["Jaw's own Tokyo Night, with Tokyo Night Day as its light half.",
                                            "As the terminal draws them; jawd serves them.",
                                            "Redlamp's theme families, each a dark and a light half."], "as the wireframes have them")
    let light = Gallery(preferences.themes, for: .light, omarchyLine: nil)
    #expect(light.sections.map(\.themes.count) == [1, 5, 22] && light.cards.count == 28)
    #expect(light.sections[1].themes.map(\.name) == ["Catppuccin Latte", "Flexoki Light", "Lupine", "Rosé Pine", "White"])
    #expect(light.cards[0].name(.light) == "Tokyo Night Day", "Jaw's light half, by its own name")
    #expect(Array(dark.sections[2].themes.prefix(3).map(\.name)) == ["Neutral", "Redlamp", "Kanagawa"])
}

@MainActor @Test func `while jawd's themes aren't in, the Omarchy section shows the slots' own from their copies and says why`() throws {
    let suite = "jaw.tests.gallery-waiting"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    let first = Preferences(defaults: defaults, accent: .blue)
    first.served = try allOmarchy()
    first.slots = Slots(dark: "omarchy:gruvbox", light: "jaw")
    let next = Preferences(defaults: defaults, accent: .blue)
    next.omarchyState = .unreachable
    let gallery = Gallery(next.themes, for: .dark, omarchyLine: next.omarchyLine)
    #expect(gallery.sections[1].themes.map(\.id) == ["omarchy:gruvbox"])
    #expect(gallery.sections[1].line == "Omarchy's themes come from jawd, which isn't answering; they show once it does")
    #expect(gallery.sections[1].header == "Omarchy's, from jawd" && gallery.sections[1].footer == gallery.sections[1].line,
            "the line in its footer's place, as the wireframes have it")
    #expect(gallery.sections[0].themes.count == 1 && gallery.sections[2].themes.count == 22, "Jaw's and Redlamp's never wait")
}

@MainActor @Test func `the arrows go along the gallery's order, and up and down its columns into the sections around, nowhere past either end`() throws {
    let preferences = try scratchPreferences("gallery-arrows")
    preferences.served = try allOmarchy()
    let gallery = Gallery(preferences.themes, for: .dark, omarchyLine: nil)
    #expect(gallery.moving(from: "jaw", .right)?.id == "omarchy:catppuccin", "from Jaw's into Omarchy's")
    #expect(gallery.moving(from: "omarchy:catppuccin", .left)?.id == "jaw")
    #expect(gallery.moving(from: "omarchy:vantablack", .right)?.id == "redlamp:neutral", "from Omarchy's into Redlamp's")
    #expect(gallery.moving(from: "jaw", .down)?.id == "omarchy:catppuccin", "the card under Jaw's lone one")
    #expect(gallery.moving(from: "omarchy:ethereal", .down)?.id == "omarchy:hackerman", "down its column")
    #expect(gallery.moving(from: "omarchy:ethereal", .up)?.id == "jaw", "up into a shorter row, to its last card")
    #expect(gallery.moving(from: "omarchy:tokyo-night", .down)?.id == "redlamp:redlamp", "into the next section's first row, the same column")
    #expect(gallery.moving(from: "redlamp:redlamp", .up)?.id == "omarchy:tokyo-night", "and back into the last row of the one before")
    #expect(gallery.moving(from: "redlamp:mc-burnt-tape", .down)?.id == "redlamp:mc-velvet-static", "into a shorter last row")
    #expect(gallery.moving(from: "jaw", .left) == nil && gallery.moving(from: "jaw", .up) == nil)
    #expect(gallery.moving(from: "redlamp:mc-velvet-static", .right) == nil && gallery.moving(from: "redlamp:mc-velvet-static", .down) == nil)
    #expect(gallery.moving(from: "omarchy:catppuccin-latte", .right) == nil, "a light theme isn't in the dark gallery")
}

@MainActor @Test(.timeLimit(.minutes(1))) func `the arrows choose card after card with no click and no keyboard navigation, and the gallery scrolls to each`() async throws {
    let preferences = try scratchPreferences("gallery-keys")
    preferences.served = try allOmarchy()
    let host = NSHostingView(rootView: AppearancePane(preferences: preferences))
    let window = NSWindow(contentRect: NSRect(origin: CGPoint(x: -30000, y: -30000), size: SettingsPane.appearance.size),
                          styleMask: [.borderless], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: .darkAqua)
    window.contentView = host
    window.orderFront(nil)
    defer { window.close() }
    #expect(await eventually { window.firstResponder is ArrowCatcher }, "the gallery holds the window's keys as the pane opens")
    /// An arrow pressed as the keyboard sends one, with its function key's character, which AppKit
    /// reads as it offers a key no view takes to the menus.
    func press(_ keyCode: UInt16, _ modifiers: NSEvent.ModifierFlags = []) async throws {
        let functionKeys = [123: NSLeftArrowFunctionKey, 124: NSRightArrowFunctionKey, 125: NSDownArrowFunctionKey, 126: NSUpArrowFunctionKey]
        let typed = try #require(functionKeys[Int(keyCode)].flatMap { UnicodeScalar($0) }.map { String($0) })
        let key = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers.union([.numericPad, .function]),
                                                timestamp: 0, windowNumber: window.windowNumber, context: nil, characters: typed,
                                                charactersIgnoringModifiers: typed, isARepeat: false, keyCode: keyCode))
        window.sendEvent(key)
        try await Task.sleep(for: .milliseconds(100))
    }
    let (left, right, down, up): (UInt16, UInt16, UInt16, UInt16) = (123, 124, 125, 126)
    try await press(right)
    try await press(right)
    try await press(down)
    #expect(preferences.slots == Slots(dark: "omarchy:hackerman", light: "jaw"), "→ → ↓ from Jaw's: Catppuccin, Ethereal, then Hackerman under it")
    try await press(right, .command)
    #expect(preferences.slots.dark == "omarchy:hackerman", "⌘→ isn't the gallery's, and goes on up the responder chain")
    func scrolls(_ view: NSView) -> NSScrollView? {
        view as? NSScrollView ?? view.subviews.lazy.compactMap { scrolls($0) }.first
    }
    let scroll = try #require(scrolls(host), "the pane scrolls")
    let top = scroll.documentVisibleRect.minY
    for _ in 0 ..< 7 {
        try await press(down)
    }
    #expect(preferences.slots.dark == "redlamp:nord", "down Hackerman's column, then into Redlamp's")
    #expect(abs(scroll.documentVisibleRect.minY - top) > 400, "scrolled to the card chosen, far below the pane's first rows")
    for _ in 0 ..< 7 {
        try await press(up)
    }
    try await press(left)
    #expect(preferences.slots.dark == "omarchy:gruvbox", "back up the same way, then ← to the card before Hackerman")
}

@MainActor @Test(.timeLimit(.minutes(1))) func `a click anywhere in the pane gives the gallery the keys back, a card's still choosing it and a radio button's the appearance`() async throws {
    let preferences = try scratchPreferences("gallery-clicks")
    preferences.served = try allOmarchy()
    let host = NSHostingView(rootView: AppearancePane(preferences: preferences))
    let window = NSWindow(contentRect: NSRect(origin: CGPoint(x: -30000, y: -30000), size: SettingsPane.appearance.size),
                          styleMask: [.borderless], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: .darkAqua)
    window.contentView = host
    window.orderFront(nil)
    defer { window.close() }
    #expect(await eventually { window.firstResponder is ArrowCatcher })
    /// A click at `point`, in the window, through its own event handling, as the bench's `click:` makes one.
    func click(_ point: NSPoint) async throws {
        for (type, pressure) in [(NSEvent.EventType.leftMouseDown, Float(1)), (.leftMouseUp, 0)] {
            window.sendEvent(try #require(NSEvent.mouseEvent(with: type, location: point, modifierFlags: [],
                                                             timestamp: ProcessInfo.processInfo.systemUptime,
                                                             windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                                                             clickCount: 1, pressure: pressure)))
        }
        try await Task.sleep(for: .milliseconds(100))
    }
    /// Where the dark slot's card for `id` is in the window: the middle of what its theme's ground fills
    /// in the pane as it's drawn, as SwiftUI's views, unlike AppKit's, don't say where they are.
    func card(_ id: String) throws -> NSPoint {
        let ground = try #require(preferences.themes.first { $0.id == id }?.dark?.ground)
        let wanted = [ground.r, ground.g, ground.b].map { Int(($0 * 255).rounded()) }
        let image = try picture(host, on: RGBA(0x1E1E1E))
        let p = pixels(image)
        var (minX, minY, maxX, maxY) = (Int.max, Int.max, Int.min, Int.min)
        for y in 0 ..< image.height {
            for x in 0 ..< image.width where (0 ..< 3).allSatisfy({ abs(Int(p[(y * image.width + x) * 4 + $0]) - wanted[$0]) <= 1 }) {
                (minX, minY, maxX, maxY) = (min(minX, x), min(minY, y), max(maxX, x), max(maxY, y))
            }
        }
        try #require(maxX >= minX, "\(id)'s card is in sight")
        return NSPoint(x: CGFloat(minX + maxX) / 4, y: host.bounds.height - CGFloat(minY + maxY) / 4)
    }
    window.makeFirstResponder(nil)
    try await click(try card("omarchy:catppuccin"))
    #expect(preferences.slots.dark == "omarchy:catppuccin", "a card's click chooses it, as it did")
    #expect(await eventually { window.firstResponder is ArrowCatcher }, "and gives the gallery the keys")
    window.makeFirstResponder(nil)
    try await click(NSPoint(x: Space.xs, y: host.bounds.height - Space.xs))
    #expect(await eventually { window.firstResponder is ArrowCatcher }, "a click on the pane's ground gives them back too")
    func views(_ view: NSView) -> [NSView] {
        [view] + view.subviews.flatMap { views($0) }
    }
    let scroll = try #require(views(host).lazy.compactMap { $0 as? NSScrollView }.first)
    let radios = try #require(views(scroll.contentView).first { $0.acceptsFirstResponder },
                              "the appearance's radio buttons, which SwiftUI draws, have a view of AppKit's to take the keys")
    let row = radios.convert(radios.bounds, to: nil)
    window.makeFirstResponder(nil)
    try await click(NSPoint(x: row.maxX - Space.l, y: row.midY))
    #expect(preferences.appearance == .light, "a click on Light, the last radio button, sets the appearance, as it did")
    #expect(await eventually { window.firstResponder is ArrowCatcher }, "and gives the gallery the keys")
}

@Test func `a slot's tile says its theme and its source, that it's in use or when it would be, and why it draws Tokyo Night`() throws {
    let themes = Catalog.themes(omarchy: [try omarchy("gruvbox")], accent: .blue)
    let slots = Slots(dark: "omarchy:gruvbox", light: "redlamp:nord")
    let dark = SlotTile(.dark, slots: slots, themes: themes, served: true, appearance: .system, dark: true)
    #expect(dark.name == "Gruvbox, Omarchy's" && dark.note == "In use")
    let light = SlotTile(.light, slots: slots, themes: themes, served: true, appearance: .system, dark: true)
    #expect(light.name == "Nord, Redlamp's" && light.note == nil, "left to the system, the other slot says nothing")
    #expect(SlotTile(.light, slots: slots, themes: themes, served: true, appearance: .dark, dark: true).note == "Used when Appearance is Light")
    let gone = Slots(dark: "omarchy:nord", light: "omarchy:gruvbox")
    #expect(SlotTile(.dark, slots: gone, themes: themes, served: true, appearance: .system, dark: true).name == "Nord isn't among jawd's themes")
    #expect(SlotTile(.dark, slots: gone, themes: themes, served: false, appearance: .system, dark: true).name == "Nord, once jawd serves it")
    #expect(SlotTile(.light, slots: gone, themes: themes, served: true, appearance: .system, dark: true).name
        == "Tokyo Night Day, as Gruvbox has no light half", "the half of Jaw's it draws in its place, by its name")
    #expect(SlotTile(.dark, slots: Slots(dark: "redlamp:nowhere"), themes: themes, served: true, appearance: .system, dark: true).name
        == "Tokyo Night, as no source has redlamp:nowhere")
    #expect(SlotTile(.light, slots: Slots(), themes: themes, served: true, appearance: .system, dark: true).name == "Tokyo Night Day, Jaw's")
}

@Test func `VoiceOver reads a card as the name of the half it shows, its section and the slots it fills`() throws {
    let themes = Catalog.themes(omarchy: [try omarchy("gruvbox")], accent: .blue)
    func card(_ id: String) throws -> Theme {
        try #require(themes.first { $0.id == id })
    }
    let slots = Slots(dark: "omarchy:gruvbox", light: "jaw")
    #expect(Gallery.label(try card("omarchy:gruvbox"), for: .dark, slots) == "Gruvbox, Omarchy's, the dark theme")
    #expect(Gallery.label(try card("jaw"), for: .light, slots) == "Tokyo Night Day, Jaw's, the light theme")
    #expect(Gallery.label(try card("redlamp:nord"), for: .dark, Slots(dark: "redlamp:nord", light: "redlamp:nord"))
        == "Nord, Redlamp's, the dark and light themes")
    #expect(Gallery.label(try card("redlamp:kanagawa"), for: .light, slots) == "Kanagawa, Redlamp's")
}

@MainActor @Test(.timeLimit(.minutes(2))) func `every theme's card for the dark slot and for the light one, three across at 144 points, as the gallery draws them`() async throws {
    let preferences = try scratchPreferences("gallery-pictures")
    preferences.served = try allOmarchy()
    for slot in Slot.allCases {
        let gallery = Gallery(preferences.themes, for: slot, omarchyLine: nil)
        let grid = GalleryGrid(themes: gallery.cards, slot: slot, slots: Slots(dark: "", light: "")) { _, _ in }
            .frame(width: ThemeChat.size.width * 3 + Space.m * 2)
            .padding(Space.m)
        let host = NSHostingView(rootView: grid)
        host.frame.size = host.fittingSize
        let w = NSWindow(contentRect: NSRect(origin: CGPoint(x: -30000, y: -30000), size: host.frame.size), styleMask: [.borderless],
                         backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.appearance = NSAppearance(named: slot.isDark ? .darkAqua : .aqua)
        w.contentView = host
        w.orderFront(nil)
        defer { w.close() }
        try await Task.sleep(for: .milliseconds(200))
        host.layoutSubtreeIfNeeded()
        try compare(try picture(host, on: slot.isDark ? RGBA(0x1E1E1E) : RGBA(0xF2F2F2)), "theme-gallery-\(slot.rawValue)")
    }
}
