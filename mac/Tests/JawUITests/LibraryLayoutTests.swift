import AppKit
import CoreGraphics
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The library laid out and drawn: tiles from the width, each taking jawd's 256 preview, months
/// heading their items, rows of each kind, every kind's shelves, nothing moved by a picture landing,
/// a tile asking for its picture once and letting it go, the item on screen kept where it was as the
/// window moves, and the library drawn as its references in both looks, dark and light.

let lisbon = TimeZone(identifier: "Europe/Lisbon")!

func libraryStyle(_ look: Look = .bubbles, dark: Bool = true, density: Density = .comfortable) -> Style {
    Style(roles: Catalog.jaw.roles(dark: dark), look: look, density: density)
}

func libraryLaid(_ kind: LibraryKind?, width: CGFloat = 560, style: Style = libraryStyle(), items: [LibraryRow] = Samples.library,
                 capped: Bool = false) -> LibraryLaid
{
    LibraryLayout.lay(items, kind: kind, counts: Samples.counts(of: items), capped: capped, note: nil, said: "Nothing shared here yet.",
                      width: width, style: style, tz: lisbon)
}

func drawingContext(_ style: Style = libraryStyle(), scoped: Bool = false) -> LibraryDrawing.Context {
    LibraryDrawing.Context(style: style, now: Samples.now, tz: lisbon, scoped: scoped)
}

func drawn(_ e: LibraryLaid.Element, _ items: [LibraryRow] = Samples.library, style: Style = libraryStyle(), selected: Bool = false)
    -> LibraryDrawn
{
    LibraryDrawing.drawn(e.what, item: items.first { $0.id == e.item }, size: e.frame.size, selected: selected, drawingContext(style))
}

func tiles(_ l: LibraryLaid) -> [LibraryLaid.Element] {
    l.elements.filter { if case .tile = $0.what { true } else { false } }
}

func headings(_ l: LibraryLaid) -> [String] {
    l.elements.compactMap { if case let .heading(title, _, _) = $0.what { title } else { nil } }
}

/// Laid-out libraries drawn one under another, `width` wide at 2x, their pictures decoded from the
/// made-up ones, as a pane's cells draw them.
func renderLibrary(_ sections: [LibraryLaid], items: [LibraryRow] = Samples.library, style: Style, selected: String? = nil,
                   scoped: Bool = false) -> CGImage?
{
    let width = sections.map(\.width).max() ?? 1, height = sections.reduce(0) { $0 + $1.height }
    let scale: CGFloat = 2
    guard let ctx = CGContext(data: nil, width: Int(width * scale), height: Int(height * scale), bitsPerComponent: 8, bytesPerRow: 0,
                              space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    ctx.scaleBy(x: scale, y: scale)
    ctx.translateBy(x: 0, y: height)
    ctx.scaleBy(x: 1, y: -1)
    ctx.setFillColor(style.roles.ground.cgColor)
    ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let c = drawingContext(style, scoped: scoped)
    var top: CGFloat = 0
    for laid in sections {
        for e in laid.elements {
            let d = LibraryDrawing.drawn(e.what, item: items.first { $0.id == e.item }, size: e.frame.size, selected: e.item == selected && e.item != nil, c)
            ctx.saveGState()
            ctx.translateBy(x: e.frame.minX, y: top + e.frame.minY)
            d.draw(in: ctx, picture: d.slot.flatMap(decoded))
            ctx.restoreGState()
        }
        top += laid.height
    }
    return ctx.makeImage()
}

@Test func `tiles are as wide as fits from 96 to 128 points, 2 apart, at every width a pane can be`() {
    for width in stride(from: CGFloat(240), through: 1800, by: 7) {
        let (n, side) = LibraryLayout.columns(width, margin: Space.l)
        let room = width - 2 * Space.l
        #expect(side <= LibraryLayout.most && (n == 1 || side >= LibraryLayout.least), "\(width): \(n) of \(side)")
        #expect(CGFloat(n) * side + CGFloat(n - 1) * LibraryLayout.gap <= room, "\(width): they fit")
        #expect(side == side.rounded(), "\(width): whole points")
    }
    #expect(LibraryLayout.columns(640, margin: Space.l).count == 5 && LibraryLayout.columns(640, margin: Space.l).side == 121)
    let line = tiles(libraryLaid(.photo, width: 640)).prefix(5)
    #expect(Set(line.map(\.frame.minY)).count == 1 && line.map(\.frame.width).allSatisfy { $0 == 121 })
    #expect(zip(line, line.dropFirst()).allSatisfy { $1.frame.minX == $0.frame.maxX + LibraryLayout.gap }, "2 points apart")
}

@Test func `each tile takes jawd's 256 preview, at 2x as at 1x, and a video's says its length`() {
    let items = Samples.library
    for e in tiles(libraryLaid(.photo, width: 640)) + tiles(libraryLaid(.video, width: 640)) {
        let item = items.first { $0.id == e.item }!
        guard let slot = drawn(e).slot else {
            #expect(item.media?.preview(256) == nil && item.media?.thumbnail == nil, "\(item.id) has nothing to draw from")
            continue
        }
        let small = item.media?.preview(256) ?? item.media?.thumbnail
        #expect(slot.keys(2).previews.first?.path == item.media?.preview(256) || item.media?.preview(256) == nil, "\(item.id): \(String(describing: small))")
        #expect(slot.keys(1).previews.first?.path == item.media?.preview(256) || item.media?.preview(256) == nil)
        #expect(slot.rect.size == e.frame.size && slot.fit == .fill)
    }
    let video = items.first { $0.id == "LV0" }!, note = items.first { $0.id == "LV2" }!
    #expect(LibraryDrawing.badge(video) == "▶ 0:42" && LibraryDrawing.badge(note) == "▶ 0:14")
    #expect(LibraryDrawing.badge(items.first { $0.kind == .photo }!) == nil)
    let coming = libraryLaid(.photo, width: 640).elements.first { $0.item == "LP12" }!
    #expect(drawn(coming).over.contains { if case .ring = $0 { true } else { false } }, "a photo coming shows its ring")
}

@Test func `a kind's items come newest first under their months`() {
    let l = libraryLaid(.photo)
    let photos = Samples.library.filter { $0.kind == .photo }
    let months = photos.map { Format.formatted($0.at, "MMMM yyyy", lisbon) }.reduce(into: [String]()) { if $0.last != $1 { $0.append($1) } }
    #expect(headings(l) == months && months.count == 3, "\(headings(l))")
    #expect(tiles(l).map(\.item) == photos.map(\.id), "every photo, in the window's order")
    var month = ""
    for e in l.elements {
        if case let .heading(title, _, _) = e.what { month = title; continue }
        let item = photos.first { $0.id == e.item }!
        #expect(Format.formatted(item.at, "MMMM yyyy", lisbon) == month, "\(item.id) under its month")
    }
    #expect(LibraryLaid.firstEnding(after: 0, l.elements) == 0)
    let shown = l.indices(in: CGRect(x: 0, y: 200, width: 560, height: 100))
    #expect(shown.allSatisfy { l.elements[$0].frame.maxY >= 200 && l.elements[$0].frame.minY <= 300 } && !shown.isEmpty)
}

@Test func `a layout formats only its months' headings, once each, however many items they head`() {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = lisbon
    let september = Int64(calendar.date(from: DateComponents(year: 2026, month: 9, day: 1))!.timeIntervalSince1970 * 1000)
    let edges = [september, september - 1].map { at in
        LibraryRow(id: "E\(at)", v: 1, sort: "\(at)~0000000", kind: .photo, chat: "c", chatName: "C", at: at, media: Media(w: 1000, h: 800))
    }
    let items = (photos(2000) + edges).sorted { $0.at > $1.at }
    let monthOf = Dictionary(uniqueKeysWithValues: items.map { ($0.id, Format.formatted($0.at, "MMMM yyyy", lisbon)) })
    let months = items.map { monthOf[$0.id]! }.reduce(into: [String]()) { if $0.last != $1 { $0.append($1) } }
    let formats = Formats()
    let l = Format.$formats.withValue(formats) { libraryLaid(.photo, items: items) }
    #expect(headings(l) == months && months == ["October 2026", "September 2026", "August 2026", "July 2026"], "\(headings(l))")
    #expect(formats.made.withLock { $0 } == months.count, "one a heading, none an item")
    var month = ""
    for e in l.elements {
        if case let .heading(title, _, _) = e.what { month = title; continue }
        #expect(monthOf[e.item ?? ""] == month, "\(e.item ?? "") under its month")
    }
}

@Test func `a picture landing moves nothing: every place is known from the window's items`() {
    let bare = Samples.library.map { item in
        var item = item
        item.media = item.media.map { Media(w: $0.w, h: $0.h, seconds: $0.seconds, state: .remote) }
        return item
    }
    for kind: LibraryKind? in [.photo, .video, nil] {
        #expect(libraryLaid(kind, items: bare).elements == libraryLaid(kind).elements, "\(String(describing: kind))")
    }
}

/// A tile's cell in a window of its own, as the grid's are, kept until the test ends.
@MainActor
func shownTile(_ pictures: MessagePictures) -> (LibraryCell, NSWindow) {
    let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 128, height: 128), styleMask: .borderless, backing: .buffered, defer: true)
    let cell = LibraryCell(frame: CGRect(x: 0, y: 0, width: 121, height: 121))
    cell.pictures = pictures
    window.contentView?.addSubview(cell)
    return (cell, window)
}

let busyItem = LibraryRow(id: "T1", v: 1, sort: "1~0000000", kind: .photo, chat: "c", chatName: "C", at: Samples.now,
                          media: Media(w: 1000, h: 800, previews: ["256": "a.256.jpg", "768": "a.768.jpg"], thumbnail: "a.jpg", path: "a"))

@MainActor @Test func `a tile asks for its picture once, and lets it go as it's reused or a prefetch is called off`() async throws {
    let pictures = busyPictures()
    let (cell, window) = shownTile(pictures)
    cell.show(.tile("T1"), item: busyItem, context: drawingContext(), selected: false)
    cell.show(.tile("T1"), item: busyItem, context: drawingContext(), selected: true)
    #expect(pictures.queue.counts.asked == PictureQueue.most + 2, "its thumbnail and its preview, once each")
    let item = LibraryGridItem()
    item.view = cell
    item.prepareForReuse()
    #expect(await eventually(seconds: 20) { !pictures.queue.busy })
    #expect(pictures.queue.counts.dropped == 2 && pictures.queue.counts.decoded == PictureQueue.most,
            "let go before they started, so never decoded: \(pictures.queue.counts)")
    #expect(cell.window === window)

    let more = busyPictures()
    let c = LibraryController(scope: .every, style: libraryStyle(), kind: .photo)
    c.pictures = more
    let w = NSWindow(contentRect: CGRect(x: -30000, y: -30000, width: 560, height: 400), styleMask: .borderless, backing: .buffered, defer: false)
    w.isReleasedWhenClosed = false
    w.contentView = c.view
    defer { w.close() }
    w.layoutIfNeeded()
    c.apply(Samples.libraryUpdate(of: [busyItem], kind: .photo))
    let at = try #require(c.laid.element(of: "T1"))
    let asked = more.queue.counts.asked
    c.collectionView(c.collection, prefetchItemsAt: [IndexPath(item: at, section: 0)])
    #expect(more.queue.counts.asked >= asked + 1 && c.prefetched["T1"] != nil, "prefetched ahead: \(more.queue.counts)")
    c.collectionView(c.collection, cancelPrefetchingForItemsAt: [IndexPath(item: at, section: 0)])
    #expect(c.prefetched.isEmpty)
    #expect(await eventually(seconds: 20) { !more.queue.busy })
    #expect(more.queue.counts.dropped >= 1, "and let go when it's called off: \(more.queue.counts)")
}

@MainActor @Test func `in a pane the grid's tiles draw from their pictures: a preview, or the blurred thumbnail of one still coming`() async throws {
    let pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
    let c = LibraryController(scope: .every, style: libraryStyle(), kind: .photo)
    c.pictures = pictures
    let w = NSWindow(contentRect: CGRect(x: -30000, y: -30000, width: 560, height: 1400), styleMask: .borderless, backing: .buffered,
                     defer: false)
    w.isReleasedWhenClosed = false
    w.contentView = c.view
    w.orderFront(nil)
    defer { w.close() }
    c.apply(Samples.libraryUpdate(of: Samples.library, kind: .photo))
    w.layoutIfNeeded()
    w.displayIfNeeded()
    func cell(_ id: String) -> LibraryCell? {
        c.collection.visibleItems().compactMap { ($0 as? LibraryGridItem)?.cell }.first { $0.item?.id == id }
    }
    #expect(await eventually {
        w.displayIfNeeded()
        return cell("LP12").flatMap { $0.drawn?.slot }.flatMap { pictures.drawable($0, scale: cell("LP12")!.scale, space: cell("LP12")!.space) } != nil
    }, "the coming photo's blurred thumbnail: \(String(describing: cell("LP12")?.bounds)), asked at \(String(describing: cell("LP12")?.askedSize)), \(pictures.queue.counts)")
    let here = try #require(cell("LP0"))
    #expect(here.drawn?.slot.flatMap { pictures.drawable($0, scale: here.scale, space: here.space) }?.isPreview == true, "a photo here, its preview")
}

/// A cell 120 points square in a window of its own off screen, shown, kept until the test ends.
@MainActor
func shownCell(_ pictures: MessagePictures) -> (LibraryCell, NSWindow) {
    let window = NSWindow(contentRect: CGRect(x: -30000, y: -30000, width: 120, height: 120), styleMask: .borderless, backing: .buffered,
                          defer: false)
    window.isReleasedWhenClosed = false
    let cell = LibraryCell(frame: CGRect(x: 0, y: 0, width: 120, height: 120))
    cell.pictures = pictures
    window.contentView?.addSubview(cell)
    window.orderFront(nil)
    return (cell, window)
}

/// The picture a cell's layer holds, when what it holds is a picture rather than a backing store.
@MainActor
func layerPicture(_ cell: LibraryCell) -> CGImage? {
    guard let contents = cell.layer?.contents, CFGetTypeID(contents as CFTypeRef) == CGImage.typeID else { return nil }
    return (contents as! CGImage)
}

/// Displays `cell` as Core Animation does as it commits: its layer, from what the cell gives it.
@MainActor
func display(_ cell: LibraryCell) {
    cell.needsDisplay = true
    cell.layer?.displayIfNeeded()
}

/// Shows `item` in `cell` once its preview's here, and displays it.
@MainActor
func showHere(_ cell: LibraryCell, _ item: LibraryRow, _ what: LibraryLaid.Element.What, selected: Bool = false) async throws {
    cell.show(what, item: item, context: drawingContext(), selected: selected)
    let slot = try #require(cell.drawn?.slot)
    #expect(await eventually(seconds: 20) { cell.pictures?.drawable(slot, scale: cell.scale, space: cell.space)?.isPreview == true }, "\(item.id)'s preview")
    display(cell)
}

@MainActor @Test func `a tile's picture is its layer's contents, never drawn, and only what's over it is drawn, on a layer over it`() async throws {
    let pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
    let (cell, window) = shownCell(pictures)
    defer { window.close() }
    let photo = try #require(Samples.library.first { $0.id == "LP0" })
    try await showHere(cell, photo, .tile(photo.id))
    let image = try #require(cell.drawn?.slot.flatMap { pictures.drawable($0, scale: cell.scale, space: cell.space) }?.image)
    #expect(cell.space == LibraryCell.space(of: window) && image.colorSpace == window.colorSpace?.cgColorSpace,
            "decoded in its window's colours, so Core Animation shows it as it is")
    #expect(cell.wantsUpdateLayer && layerPicture(cell) === image, "its preview is its layer's")
    #expect(cell.layer?.masksToBounds == true && cell.layer?.cornerRadius == drawingContext().metrics.radius)
    #expect(cell.overlay.contents == nil && cell.drawnFrom == 2, "nothing over a photo that's here")

    cell.selected = true
    display(cell)
    #expect(cell.overlay.contents != nil && layerPicture(cell) === image, "the selection over it, the picture as it was")
    cell.letGo()
    #expect(layerPicture(cell) == nil, "let go as it's reused or scrolled past, so the pictures held are the cache's")
    display(cell)
    #expect(layerPicture(cell) === image, "and shown again from the cache")

    let file = try #require(Samples.library.first { $0.kind == .file })
    cell.show(.row(file.id), item: file, context: drawingContext(), selected: false)
    display(cell)
    #expect(!cell.wantsUpdateLayer && cell.overlay.contents == nil && cell.layer?.backgroundColor == nil && cell.layer?.masksToBounds == false,
            "reused for a row, it draws")
    #expect(layerPicture(cell) == nil, "into a backing store, not the picture")
}

@MainActor @Test func `a tile shown by its layers looks as its parts draw it: a photo selected, a video with its length, a photo coming`() async throws {
    let pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
    let (cell, window) = shownCell(pictures)
    defer { window.close() }
    for (id, selected) in [("LP0", true), ("LV0", false), ("LP12", false)] {
        let item = try #require(Samples.library.first { $0.id == id })
        cell.show(.tile(id), item: item, context: drawingContext(), selected: selected)
        let slot = try #require(cell.drawn?.slot)
        #expect(await eventually(seconds: 20) { pictures.drawable(slot, scale: cell.scale, space: cell.space) != nil }, "\(id)'s picture")
        display(cell)
        let scale = cell.scale, size = cell.bounds.size
        /// Drawn from the top left, as AppKit draws a flipped view.
        func bitmap(_ draw: (CGContext) -> Void) -> [UInt8] {
            let (w, h) = (Int(size.width * scale), Int(size.height * scale))
            let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            ctx.scaleBy(x: scale, y: scale)
            ctx.translateBy(x: 0, y: size.height)
            ctx.scaleBy(x: 1, y: -1)
            draw(ctx)
            return Array(UnsafeBufferPointer(start: ctx.data!.assumingMemoryBound(to: UInt8.self), count: w * h * 4))
        }
        let layers = bitmap { ctx in cell.layer?.render(in: ctx) }
        let parts = bitmap { ctx in cell.drawn?.draw(in: ctx, picture: pictures.drawable(slot, scale: scale, space: cell.space)?.image) }
        let apart = zip(layers, parts).map { abs(Int($0) - Int($1)) }
        let off = stride(from: 0, to: apart.count, by: 4).count { apart[$0 ..< $0 + 4].max()! > 6 }
        #expect(Double(off) < Double(apart.count / 4) * 0.015 && apart.reduce(0, +) < apart.count, "\(id): \(off) pixels apart")
    }
}

/// `n` photos a day apart from `from`, each with the made-up previews.
func photos(_ n: Int, from: Int = 0) -> [LibraryRow] {
    (from ..< from + n).map { i in
        LibraryRow(id: String(format: "N%04d", i), v: 1, sort: "\(Samples.now - Int64(i) * 3_600_000)~0000000", kind: .photo, chat: "c",
                   chatName: "C", at: Samples.now - Int64(i) * 3_600_000,
                   media: Media(w: 1000, h: 800, previews: ["256": SamplePictures.preview("lake", 256)], path: "x"))
    }
}

@MainActor @Test func `a move that lets items above go keeps the item on screen where it was, and the grid moves its window near its ends`() async throws {
    final class Moves: LibraryHost {
        var moved: [Place] = []
        func library(_: LibraryController, watch _: LibraryFilter, _: Place) {}
        func library(_: LibraryController, move place: Place) { moved.append(place) }
    }
    let host = Moves()
    let c = LibraryController(scope: .every, style: libraryStyle(), kind: .photo)
    c.host = host
    c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
    let w = NSWindow(contentRect: CGRect(x: -30000, y: -30000, width: 560, height: 500), styleMask: .borderless, backing: .buffered, defer: false)
    w.isReleasedWhenClosed = false
    w.contentView = c.view
    defer { w.close() }
    w.layoutIfNeeded()
    c.apply(Update(seq: 1, upsert: photos(240), total: 1000, older: true, moved: Moved(place: .top(240), newer: 0),
                   counts: LibraryCounts(photo: 1000), note: .some(nil)))
    c.scroll(to: 4000)
    let visible = c.scroll.contentView.bounds
    let first = try #require(c.laid.indices(in: visible).compactMap { c.laid.elements[$0].item }.first)
    let offset = c.laid.elements[c.laid.element(of: first)!].frame.minY - visible.minY
    c.apply(Update(seq: 2, remove: photos(60).map(\.id), upsert: photos(60, from: 240), older: true,
                   moved: Moved(place: .around(first, before: 300, after: 300), newer: 0)))
    let now = c.scroll.contentView.bounds
    #expect(c.laid.element(of: photos(1).first!.id) == nil, "the newest let go")
    #expect(abs(c.laid.elements[c.laid.element(of: first)!].frame.minY - now.minY - offset) < 1, "\(first) where it was on screen")

    c.scroll(to: c.laid.height)
    c.settle()
    let last = try #require(host.moved.last, "near the window's end, it's moved")
    if case let .row(id) = last.around { #expect(c.index[id] != nil && last.before == 300 && last.after == 300) } else {
        Issue.record("moved around \(last)")
    }
    let count = host.moved.count
    c.settle()
    #expect(host.moved.count == count, "not asked again while that move is on its way")
}

@Test func `the library's photos and videos draw as their references, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let style = libraryStyle(look, dark: dark)
            let image = try #require(renderLibrary([libraryLaid(.photo, style: style), libraryLaid(.video, style: style)], style: style, selected: "LP1"))
            try compare(image, "library-grid-\(look.rawValue)-\(dark ? "dark" : "light")")
        }
    }
}

@MainActor @Test func `the items on screen are named to jawd as the terminal names them: in order, as they change, and not while the window's closed`() async throws {
    let (c, asked, w) = pane(.photo, height: 400)
    defer { w.close() }
    let visible = c.scroll.contentView.bounds
    let expected = c.laid.elements.filter { $0.item != nil && $0.frame.maxY >= visible.minY && $0.frame.minY <= visible.maxY }.map(\.item!)
    #expect(asked.shown.last == expected && !expected.isEmpty && c.onScreen == expected, "\(asked.shown) of \(expected)")
    try await Task.sleep(for: .milliseconds(250))
    c.nameShown()
    #expect(asked.shown.count == 1, "the same items aren't named again")
    c.scroll(to: c.laid.height)
    #expect(await eventually { asked.shown.count == 2 })
    #expect(asked.shown.last == c.onScreen && asked.shown.last != expected, "named again once they change")
    c.letGoOfPictures()
    c.scroll(to: 0)
    try await Task.sleep(for: .milliseconds(250))
    c.nameShown()
    #expect(asked.shown.count == 2, "nothing named while the window's closed")
    c.askPicturesAgain()
    #expect(asked.shown.count == 3 && asked.shown.last == expected, "and what's on screen once it's back")
}
