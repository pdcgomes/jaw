import AppKit
import CoreGraphics
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The strip of kinds: its labels from jawd's counts, Tab's order, what its end says, a kind chosen
/// watched from the top, and the strip drawn as its references, dark and light.

/// What a pane asked of jawd, for the tests.
@MainActor
final class Asked: LibraryHost {
    var watched: [(LibraryFilter, Place)] = []
    var moved: [Place] = []
    var opened: [(LibraryRow, [LibraryRow])] = []
    var went: [LibraryRow] = []
    var shown: [[String]] = []
    var closed = 0
    func library(_: LibraryController, watch filter: LibraryFilter, _ place: Place) { watched.append((filter, place)) }
    func library(_: LibraryController, move place: Place) { moved.append(place) }
    func library(_: LibraryController, open item: LibraryRow, among: [LibraryRow]) { opened.append((item, among)) }
    func library(_: LibraryController, goTo item: LibraryRow) { went.append(item) }
    func library(_: LibraryController, shown items: [String]) { shown.append(items) }
    func libraryClose(_: LibraryController) { closed += 1 }
}

/// A library pane in a window of its own, off the screen, given the made-up items at `kind`.
@MainActor
func pane(_ kind: LibraryKind?, scope: LibraryScope = .every, style: Style = libraryStyle(), height: CGFloat = 700)
    -> (LibraryController, Asked, NSWindow)
{
    let asked = Asked()
    let c = LibraryController(scope: scope, style: style, kind: kind)
    c.host = asked
    c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
    let w = NSWindow(contentRect: CGRect(x: -30000, y: -30000, width: 560, height: height), styleMask: .borderless, backing: .buffered,
                     defer: false)
    w.isReleasedWhenClosed = false
    w.contentView = c.view
    w.layoutIfNeeded()
    c.apply(Samples.libraryUpdate(of: Samples.library, kind: kind))
    return (c, asked, w)
}

@Test func `the strip says each kind with jawd's count, 1,000+ where it counted only so far, and All first`() {
    let counts = LibraryCounts(photo: 214, video: 31, voice: 0, file: 1000, link: 40)
    #expect(LibraryKinds.labels(counts, capped: false).map(\.label) == ["All", "Photos 214", "Videos 31", "Voice", "Files 1,000", "Links 40"])
    #expect(LibraryKinds.labels(counts, capped: true).map(\.label)[4] == "Files 1,000+")
    #expect(LibraryKinds.labels(nil, capped: false).map(\.label) == ["All", "Photos", "Videos", "Voice", "Files", "Links"], "before jawd's answer")
    #expect(LibraryKinds.labels(counts, capped: false).map(\.kind) == [nil, .photo, .video, .voice, .file, .link])
}

@Test func `Tab steps through the kinds in the strip's order, going round, and ⇧Tab back`() {
    var kind: LibraryKind? = nil
    var seen: [LibraryKind?] = []
    for _ in 0 ..< 6 {
        kind = LibraryKinds.step(kind, by: 1)
        seen.append(kind)
    }
    #expect(seen == [.photo, .video, .voice, .file, .link, nil])
    #expect(LibraryKinds.step(nil, by: -1) == .link && LibraryKinds.step(.photo, by: -1) == nil)
}

@Test func `the strip's end says what's shown: that jawd's asked, the newest first, or how many`() {
    #expect(LibraryKinds.said(.photo, loaded: false, total: 0, capped: false, filtered: false) == "finding…")
    #expect(LibraryKinds.said(nil, loaded: true, total: 30, capped: false, filtered: false) == "newest first")
    #expect(LibraryKinds.said(nil, loaded: true, total: 30, capped: false, filtered: true) == "30 items")
    #expect(LibraryKinds.said(.photo, loaded: true, total: 214, capped: false, filtered: false) == "214 photos")
    #expect(LibraryKinds.said(.voice, loaded: true, total: 1, capped: false, filtered: false) == "1 voice message")
    #expect(LibraryKinds.said(.file, loaded: true, total: 1000, capped: true, filtered: true) == "1,000+ files")
}

@Test func `with no room for the counts, the strip has the names alone`() {
    let labels = LibraryKinds.labels(LibraryCounts(photo: 214, video: 31, voice: 58, file: 12, link: 40), capped: false)
    let wide = LibraryKinds.segments(labels, width: 600)
    #expect(wide.map(\.label)[1] == "Photos 214" && wide.last!.frame.maxX <= 600)
    let narrow = LibraryKinds.segments(labels, width: 260)
    #expect(narrow.map(\.label) == ["All", "Photos", "Videos", "Voice", "Files", "Links"])
    #expect(zip(narrow, narrow.dropFirst()).allSatisfy { $1.frame.minX > $0.frame.maxX })
}

@MainActor @Test func `a kind chosen on the strip, with Tab or on a shelf's heading is watched from the top with that kind`() throws {
    let (c, asked, w) = pane(nil)
    defer { w.close() }
    c.strip.picked?(.file)
    #expect(c.kind == .file && asked.watched.last?.0 == LibraryFilter(kind: .file) && asked.watched.last?.1 == .top(LibraryController.page))
    #expect(c.strip.said == "finding…" && c.strip.labels.map(\.label)[4] == "Files 4", "the counts stand until jawd's answer")
    #expect(c.key(key("\t", code: 48)) && c.kind == .link)
    #expect(c.key(key("\t", code: 48, .shift)) && c.kind == .file, "and ⇧Tab back")
    c.choose(nil)
    c.apply(Samples.libraryUpdate(of: Samples.library, kind: nil))
    #expect(c.strip.said == "newest first" && c.strip.shown == nil)
    w.layoutIfNeeded()
    let heading = try #require(c.laid.elements.firstIndex { $0.what == .heading("Links", count: "3", kind: .link) })
    let cell = LibraryCell(frame: c.laid.elements[heading].frame)
    cell.show(c.laid.elements[heading].what, item: nil, context: drawingContext(), selected: false)
    c.clicked(cell, 1)
    #expect(c.kind == .link && asked.watched.last?.0.kind == .link, "a shelf's heading goes to its kind")
    #expect(c.header.detail.stringValue == "every chat · 30 shared")
}

/// The strip as it's drawn, `width` wide at 2x, a line for each of `strips`.
func renderStrips(_ strips: [(LibraryCounts?, Bool, LibraryKind?, String)], roles: Roles, width: CGFloat = 560) -> CGImage? {
    let h = LibraryController.stripHeight, scale: CGFloat = 2, height = h * CGFloat(strips.count)
    guard let ctx = CGContext(data: nil, width: Int(width * scale), height: Int(height * scale), bitsPerComponent: 8, bytesPerRow: 0,
                              space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    ctx.scaleBy(x: scale, y: scale)
    ctx.translateBy(x: 0, y: height)
    ctx.scaleBy(x: 1, y: -1)
    ctx.setFillColor(roles.ground.cgColor)
    ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
    for (i, (counts, capped, shown, said)) in strips.enumerated() {
        let top = ((h - LibraryKinds.height) / 2).rounded()
        let segments = LibraryKinds.segments(LibraryKinds.labels(counts, capped: capped), width: width - 2 * Space.l - 120)
            .map { ($0.kind, $0.label, $0.frame.offsetBy(dx: Space.l, dy: top)) }
        Laid.draw(LibraryStrip.parts(segments, shown: shown, said: said, width: width, height: h, roles: roles), in: ctx, y: CGFloat(i) * h)
    }
    return ctx.makeImage()
}

@Test func `the strip draws as its references, dark and light`() throws {
    let counts = LibraryCounts(photo: 214, video: 31, voice: 58, file: 12, link: 40)
    for dark in [true, false] {
        let roles = Catalog.jaw.roles(dark: dark)
        let image = try #require(renderStrips([
            (counts, false, nil, "newest first"), (counts, false, .photo, "214 photos"),
            (LibraryCounts(photo: 1000, video: 31, voice: 0, file: 12, link: nil), true, .file, "finding…"),
        ], roles: roles))
        try compare(image, "library-strip-\(dark ? "dark" : "light")")
    }
}
