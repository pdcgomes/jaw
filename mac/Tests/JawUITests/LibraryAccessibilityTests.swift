import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// What VoiceOver gets of the library: each item said as it's drawn, one element a kind on the strip,
/// a press that opens an item with its menu's actions as its own, the selection said as it moves,
/// and the selected item's file for the Message menu's Save to Downloads and Show in Finder.

func sample(_ id: String) -> LibraryRow {
    Samples.library.first { $0.id == id }!
}

/// The pane's cells on screen, by item, once it's shown and laid out.
@MainActor
func shownCells(_ c: LibraryController, _ w: NSWindow) -> [LibraryCell] {
    w.orderFront(nil)
    w.layoutIfNeeded()
    c.collection.layoutSubtreeIfNeeded()
    return c.collection.visibleItems().compactMap { ($0 as? LibraryGridItem)?.cell }
}

@Test func `VoiceOver says an item as it's drawn: what it is, how its file is, a file's details, who shared it where, and when`() {
    func said(_ id: String, scoped: Bool = false) -> String {
        LibraryDrawing.spoken(sample(id), drawingContext(scoped: scoped))
    }
    #expect(said("LF0") == "Sintra topo.pdf, PDF, 2.4 MB, 12 pages, Ana in Lisbon Climbing, yesterday")
    #expect(said("LF1") == "topo-sintra.gpx, not downloaded, GPX, 48 KB, Rui in Lisbon Climbing, Tue", "a file not here says so first")
    #expect(said("LF0", scoped: true) == "Sintra topo.pdf, PDF, 2.4 MB, 12 pages, Ana, yesterday", "a chat's library doesn't say where")
    #expect(said("LP1") == "Photo, Ana, Wed", "a one-to-one chat is the person's, so they're said once")
    #expect(said("LP0") == "Photo, You in Lisbon Climbing, 14:02")
    #expect(said("LP12") == "Photo, downloading, 40%, Inês in Lisbon Climbing, 16 Sep", "a photo coming says how far")
    #expect(said("LP13") == "Photo, not downloaded, Ana in Lisbon Climbing, 15 Sep")
    #expect(said("LV0") == "Video, 0:42, Rui in Lisbon Climbing, 03:02")
    #expect(said("LW0") == "Voice message, 0:14, Inês in Lisbon Climbing, 13:02")
    #expect(said("LL1~0") == "Vertigo Climbing Center, vertigo.pt, Rui in Lisbon Climbing, Mon")
}

@MainActor @Test func `the strip gives VoiceOver a radio button a kind, the one shown selected, whose press picks it, then what's shown`() throws {
    let strip = LibraryStrip(frame: CGRect(x: 0, y: 0, width: 640, height: 32))
    strip.labels = LibraryKinds.labels(Samples.counts(of: Samples.library), capped: false)
    strip.shown = .file
    strip.said = "4 files"
    var picked: [LibraryKind?] = []
    strip.picked = { picked.append($0) }
    #expect(strip.accessibilityRole() == .tabGroup && strip.accessibilityLabel() == "Kinds")
    let children = try #require(strip.accessibilityChildren() as? [NSAccessibilityElement])
    let kinds = children.filter { $0.accessibilityRole() == .radioButton }
    #expect(kinds.map { $0.accessibilityLabel() ?? "" } == ["All", "Photos 18", "Videos 3", "Voice 2", "Files 4", "Links 3"])
    #expect(kinds.map { $0.isAccessibilitySelected() } == [false, false, false, false, true, false], "Files, shown")
    #expect(kinds.allSatisfy { $0.accessibilityParent() as? LibraryStrip === strip && $0.accessibilityFrameInParentSpace().width > 0 })
    #expect(children.last?.accessibilityRole() == .staticText && children.last?.accessibilityLabel() == "4 files")
    #expect(kinds[1].accessibilityPerformPress() && picked == [.photo], "a press picks it")

    strip.shown = .photo
    let again = try #require(strip.accessibilityChildren() as? [NSAccessibilityElement])
    #expect(zip(again, children).allSatisfy { $0 === $1 }, "the same elements while the kinds are, so VoiceOver keeps its place")
    #expect(again.filter { $0.accessibilityRole() == .radioButton }.map { $0.isAccessibilitySelected() } == [false, true, false, false, false, false])
}

@MainActor @Test func `an item is a button whose press opens it, with its menu's actions as its own, said selected once it is`() throws {
    let (c, asked, w) = pane(.file)
    defer { w.close() }
    let cells = shownCells(c, w)
    let file = try #require(cells.first { $0.item?.id == "LF0" })
    #expect(file.accessibilityRole() == .button && file.accessibilityLabel() == LibraryDrawing.spoken(sample("LF0"), c.context))
    #expect(file.accessibilityLabel()?.hasPrefix("Sintra topo.pdf, PDF, 2.4 MB, 12 pages, Ana in Lisbon Climbing, ") == true)
    #expect(file.accessibilityCustomActions()?.map(\.name) == ["Quick Look", "Go to Message", "Save to Downloads", "Show in Finder"],
            "its menu's, in its order")
    #expect(!file.isAccessibilitySelected())
    #expect(file.accessibilityPerformPress())
    #expect(asked.opened.map(\.0.id) == ["LF0"] && c.selectedItem?.id == "LF0" && file.isAccessibilitySelected(), "opened as Enter opens it")

    let other = try #require(cells.first { $0.item?.id == "LF1" })
    #expect(other.accessibilityCustomActions()?.first { $0.name == "Show in Finder" }?.handler?() == true)
    #expect(asked.media.map { "\($0.0) \($0.1.id)" } == ["reveal LF1"] && c.selectedItem?.id == "LF1", "on its own item, selected first")
    let month = try #require(cells.first { if case .heading = $0.element { true } else { false } })
    #expect(month.accessibilityRole() == .staticText && !month.accessibilityPerformPress(), "a month's heading is words")

    let (all, allAsked, allWindow) = pane(nil)
    defer { allWindow.close() }
    let shelf = try #require(shownCells(all, allWindow).first { if case .heading(_, _, .file?)? = $0.element { true } else { false } })
    #expect(shelf.accessibilityRole() == .button && shelf.accessibilityLabel() == "Files, 4")
    #expect(shelf.accessibilityPerformPress() && allAsked.watched.last?.0.kind == .file, "a shelf's heading goes to its kind")

    let (links, _, linksWindow) = pane(.link)
    defer { linksWindow.close() }
    let link = try #require(shownCells(links, linksWindow).first { $0.item?.id == "LL1~0" })
    #expect(link.accessibilityCustomActions()?.map(\.name) == ["Open in Browser", "Go to Message", "Copy Link"])
}

@MainActor @Test func `the selection is said to VoiceOver as it moves, as its item is drawn, and not again where it stays`() {
    let (c, _, w) = pane(.file)
    defer { w.close() }
    var said: [String] = []
    c.announce = { said.append($0) }
    c.move(0, 1)
    c.move(0, 1)
    c.select("LF1")
    #expect(said == ["LF0", "LF1"].map { LibraryDrawing.spoken(sample($0), c.context) }, "\(said)")
    #expect(said.first?.hasPrefix("Sintra topo.pdf, PDF") == true)
}

@MainActor @Test func `the selected item's file is the Message menu's to save or show while the pane is focused, and a link's isn't`() {
    let (c, asked, w) = pane(nil)
    defer { w.close() }
    #expect(c.selectedOffer == nil)
    c.select("LL1~0")
    c.media(.save)
    #expect(c.selectedOffer == nil && asked.media.isEmpty, "a link has no file")
    c.select("LF0")
    c.media(.save)
    c.media(.reveal)
    #expect(c.selectedOffer?.message == "LF0" && asked.media.map { "\($0.0) \($0.1.id)" } == ["save LF0", "reveal LF0"])
}
