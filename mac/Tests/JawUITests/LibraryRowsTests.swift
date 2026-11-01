import AppKit
import CoreGraphics
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// Voice messages, files and links as rows, and every kind's window as a shelf of each.

func rows(_ l: LibraryLaid) -> [LibraryLaid.Element] {
    l.elements.filter { if case .row = $0.what { true } else { false } }
}

/// Where each line a drawing draws ends, to check none runs past its row.
func lineEnds(_ d: LibraryDrawn) -> [CGFloat] {
    (d.under + d.over).compactMap { if case let .line(l, p) = $0 { p.x + LibraryDrawing.width(l) } else { nil } }
}

@Test func `voice messages, files and links are rows of their kind's height, across the pane, under their months`() {
    for density in Density.allCases {
        for look in Look.allCases {
            let style = libraryStyle(look, density: density)
            let m = LibraryLayout.Metrics(style)
            for kind in [LibraryKind.voice, .file, .link] {
                let l = libraryLaid(kind, width: 480, style: style)
                let these = Samples.library.filter { $0.kind == kind }
                #expect(rows(l).map(\.item) == these.map(\.id), "\(kind): every item, newest first")
                #expect(rows(l).allSatisfy { $0.frame.height == m.row && $0.frame.width == 480 - 2 * m.margin && $0.frame.minX == m.margin })
                #expect(zip(rows(l), rows(l).dropFirst()).allSatisfy { $1.frame.minY >= $0.frame.maxY + m.rowGap }, "\(kind) in \(look)")
                #expect(!headings(l).isEmpty && tiles(l).isEmpty)
            }
        }
    }
    #expect(LibraryLayout.Metrics(libraryStyle(.lines)).rowGap == 0, "lines part rows with hairlines, not gaps")
}

@Test func `a row's words fit it: a long name is cut short, and when and what follows stay whole`() {
    for width: CGFloat in [300, 480, 720] {
        for kind in [LibraryKind.voice, .file, .link] {
            for e in rows(libraryLaid(kind, width: width)) {
                let ends = lineEnds(drawn(e))
                #expect(!ends.isEmpty && ends.allSatisfy { $0 <= e.frame.width - Space.m + 0.5 }, "\(e.item ?? "") at \(width): \(ends)")
            }
        }
    }
}

@Test func `a file says what it is, how big, its pages and who shared it where; one not here says so`() {
    let c = drawingContext()
    let here = Samples.library.first { $0.id == "LF0" }!, coming = Samples.library.first { $0.id == "LF1" }!
    #expect(LibraryDrawing.coming(here.file) == nil && LibraryDrawing.coming(coming.file) == "not downloaded")
    var downloading = coming.file!
    downloading.state = .downloading
    downloading.progress = 0.4
    #expect(LibraryDrawing.coming(downloading) == "downloading 40%", "once jawd says how its copy is (P14)")
    #expect(LibraryDrawing.fileSymbol(here.file) == "doc.richtext" && LibraryDrawing.fileSymbol(coming.file) == "map")
    #expect(LibraryDrawing.who(here, c) == "Ana in Lisbon Climbing")
    #expect(LibraryDrawing.who(here, drawingContext(scoped: true)) == "Ana", "a chat's library needn't say where")
    let direct = Samples.library.first { $0.id == "LL0~0" }!
    #expect(LibraryDrawing.who(direct, c) == "Ana", "a one-to-one chat's is the person's")
    let mine = Samples.library.first { $0.id == "LF2" }!
    #expect(LibraryDrawing.who(mine, c) == "You in Ana Ribeiro")
}

@Test func `a voice message's bars follow its waveform once jawd sends one, and are even until then`() {
    func bars(_ id: String) -> [CGFloat] {
        let e = rows(libraryLaid(.voice)).first { $0.item == id }!
        return drawn(e).under.compactMap { if case let .fill(r, _, _) = $0, r.width == 2 { r.height } else { nil } }
    }
    let even = bars("LW0"), wave = bars("LW1")
    #expect(!even.isEmpty && Set(even).count == 1, "even: \(Set(even))")
    #expect(Set(wave).count > 3, "following its waveform: \(Set(wave))")
}

@Test func `every kind's window is a shelf of each, headed by its kind and count, a heading going to its kind`() {
    let l = libraryLaid(nil, width: 560)
    let counts = Samples.counts(of: Samples.library)
    let shelves = l.elements.compactMap { e -> (String, String?, LibraryKind?)? in
        if case let .heading(title, count, kind) = e.what { (title, count, kind) } else { nil }
    }
    #expect(shelves.map(\.0) == ["Photos and videos", "Files", "Links", "Voice messages"])
    #expect(shelves.map(\.1) == ["\(counts.photo + counts.video)", "\(counts.file)", "\(counts.link!)", "\(counts.voice)"])
    #expect(shelves.map(\.2) == [.photo, .file, .link, .voice])
    #expect(tiles(l).count == 2 * l.columns, "two lines of tiles")
    let pictures = Samples.library.filter { $0.kind == .photo || $0.kind == .video }.prefix(2 * l.columns).map(\.id)
    #expect(tiles(l).map(\.item!) == Array(pictures), "photos and videos together, newest first")
    let kinds = rows(l).map { e in Samples.library.first { $0.id == e.item }!.kind }
    #expect(kinds == [.file, .file, .file, .link, .link, .link, .voice, .voice])
    #expect(libraryLaid(nil, capped: true).elements.count == l.elements.count)

    let noVoice = libraryLaid(nil, items: Samples.library.filter { $0.kind != .voice })
    #expect(!headings(noVoice).contains("Voice messages"), "a shelf with nothing isn't drawn")
    let noted = LibraryLayout.lay(Samples.library.filter { $0.kind != .link }, kind: nil, counts: LibraryCounts(link: nil), capped: false,
                                  note: "links come with jawd's next step", said: nil, width: 560, style: libraryStyle(), tz: lisbon)
    #expect(noted.elements.contains { $0.what == .said("links come with jawd's next step") }, "the links' shelf says jawd's note")
    let nothing = libraryLaid(nil, items: [])
    #expect(nothing.elements.map(\.what) == [.said("Nothing shared here yet.")])
    #expect(LibraryLayout.counted(1000, capped: true) == "1,000+" && LibraryLayout.counted(1000, capped: false) == "1,000"
        && LibraryLayout.counted(214, capped: true) == "214")
}

@Test func `rows and every kind's shelves draw as their references, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let style = libraryStyle(look, dark: dark)
            let name = "\(look.rawValue)-\(dark ? "dark" : "light")"
            let sections = [LibraryKind.file, .link, .voice].map { libraryLaid($0, width: 480, style: style) }
            try compare(try #require(renderLibrary(sections, style: style, selected: "LF0")), "library-rows-\(name)")
            try compare(try #require(renderLibrary([libraryLaid(nil, style: style)], style: style)), "library-all-\(name)")
        }
    }
}
