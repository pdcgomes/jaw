import AppKit
import CoreGraphics
import CoreText
import ImageIO
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing
import UniformTypeIdentifiers

/// A link's preview: under its text, with its thumbnail, its title, two lines of its description at
/// most and its host, in both looks, for messages received and sent; its thumbnail a picture slot the
/// image queue fills from jawd's folder by its path, a JPEG alone, decoded to fill its square.

func sampleLink(_ id: String) -> MessageRow {
    Samples.links.first { $0.id == id }!
}

func laid(_ m: MessageRow, _ s: LayoutSpec, width: CGFloat = 640) -> Laid {
    MessageLayout.lay(m, width: width, context: MessageContext(group: true, firstOfRun: true, showTime: true, day: nil),
                      style: s.style, fonts: s.fonts, me: s.me)
}

/// The card a click opens a link from: where it is, and what it opens.
func linkCard(_ l: Laid) -> (rect: CGRect, url: String)? {
    for (rect, region) in l.regions {
        if case let .open(url) = region { return (rect, url) }
    }
    return nil
}

/// The thumbnails it shows: its picture slots, as a link's card has no other.
func pictureParts(_ l: Laid) -> Int {
    l.pictures.count
}

@Test func `a link's preview sits under its text, with its thumbnail, title, two lines of description and host`() throws {
    for look in Look.allCases {
        let s = spec(look, dark: true)
        let ipma = laid(sampleLink("L1"), s)
        let card = try #require(linkCard(ipma), "\(look)")
        #expect(card.url == "https://www.ipma.pt/pt/otempo/prev.localidade.hora/", "a click opens the link")
        let text = try #require(ipma.text?.rect)
        #expect(card.rect.minY >= text.maxY, "\(look): under the text")
        #expect(pictureParts(ipma) == 1, "\(look): its thumbnail")
        let history = laid(sampleLink("L2"), s)
        #expect(pictureParts(history) == 0 && linkCard(history) != nil, "\(look): a card without a thumbnail from history")
        let yours = laid(sampleLink("L3"), s)
        let mine = try #require(linkCard(yours))
        #expect(pictureParts(yours) == 1 && mine.rect.height == card.rect.height, "\(look): yours as theirs, as tall as its thumbnail")
        if look == .lines {
            #expect(card.rect.minX == text.minX, "at the message's indent")
        } else {
            #expect(mine.rect.minX > card.rect.minX, "yours in your bubble, on the right")
        }
        #expect(linkCard(laid(sampleLink("L5"), s)) == nil, "\(look): no preview, no card")
    }
}

@Test func `a link's description runs to two lines at most, cut with an ellipsis`() {
    let s = spec(.bubbles, dark: true)
    let b = MessageLayout.Builder(m: sampleLink("L3"), width: 640, context: MessageContext(group: true, firstOfRun: true, showTime: true, day: nil),
                                  style: s.style, fonts: s.fonts, me: s.me)
    let long = sampleLink("L3").link!
    let lines = b.descriptionLines(long, width: 200)
    #expect(lines.count == 2)
    #expect(b.wrapped(long.description!, s.fonts.small, s.style.roles.secondary, width: 200, most: 9).count > 2, "so it was cut")
    #expect(CGFloat(CTLineGetTypographicBounds(lines[1], nil, nil, nil)) <= 200, "the second line cut to fit")
    #expect(b.descriptionLines(LinkPreview(url: "https://climb.pt", description: "short\nand sweet"), width: 200).count == 1,
            "newlines run on as spaces")
    #expect(b.descriptionLines(LinkPreview(url: "https://climb.pt", title: "Climb"), width: 200).isEmpty)
    let words = b.linkWordsHeight(width: 200)
    #expect(words == b.lineHeight(s.fonts.name) + 3 * b.lineHeight(s.fonts.small), "a title, two lines and the host")
}

@MainActor @Test func `a preview with neither title nor description has no card, and one whose thumbnail can't be read keeps its place`() throws {
    let s = spec(.bubbles, dark: true)
    var bare = sampleLink("L1")
    bare.link = LinkPreview(url: "https://www.ipma.pt/")
    #expect(linkCard(laid(bare, s)) == nil)
    let read = laid(sampleLink("L1"), s)
    let slot = try #require(read.pictures.first)
    #expect(slot.source == PictureSource(small: "media/links/ipma.jpg", jpeg: true) && slot.rect.size == CGSize(width: 70, height: 70)
        && slot.fit == .fill, "its thumbnail a slot of its own, filled from its centre")
    #expect(linkCard(read).map { $0.rect.contains(slot.rect) } == true)
    #expect(read.parts.contains { if case let .fill(rect, _, _) = $0 { rect == slot.rect } else { false } },
            "its well under it, which shows until it's decoded, and where it can't be")
    #expect(MessagePictures().drawable(slot, scale: 2) == nil, "nothing to draw it from yet: its place is the row's, so nothing moves when it's read")
    #expect(!read.regions.contains { if case .media = $0.1 { true } else { false } }, "a click opens the link, not the picture")
    #expect(MessageLayout.Builder.spoken(sampleLink("L1")) == "link: Previsão para Sintra · IPMA, ipma.pt")
    #expect(MessageLayout.Builder.spoken(bare) == nil)
}

@MainActor @Test func `jawd's folder gives a thumbnail by its path, and one jawd keeps later is read then`() async throws {
    let (root, done) = try pictureFolder()
    defer { done() }
    let queue = PictureQueue(cache: PictureCache(), retry: .milliseconds(100))
    queue.files = .folder(root)
    let slot = try #require(laid(sampleLink("L1"), spec(.bubbles, dark: true)).pictures.first)
    let key = try #require(slot.keys(2).previews.first)
    #expect(key == PictureKey(path: "media/links/ipma.jpg", width: 140, height: 140, fit: .fill, blurred: false, jpeg: true),
            "at its square's pixels, a JPEG alone")
    let token = queue.token()
    queue.ask(key, for: token) { _ in }
    #expect(await eventually { queue.failed(key) }, "not kept yet")
    try #require(SamplePictures.files["media/links/ipma.jpg"]).write(to: URL(fileURLWithPath: root + "/media/links/ipma.jpg"))
    queue.ask(key, for: token) { _ in }
    #expect(queue.failed(key) && !queue.busy, "tried once, for a while")
    try await Task.sleep(for: .milliseconds(150))
    final class Came: @unchecked Sendable {
        var n = 0
    }
    let came = Came()
    queue.ask(key, for: token) { _ in came.n += 1 }
    #expect(await eventually { came.n == 1 }, "read once it's there, a while later")
    let read = try #require(queue.cache.image(key))
    #expect(read.width == 140 && read.height == 140)
}

/// A folder for a test's pictures, as jawd's root is, removed by `done`.
func pictureFolder() throws -> (root: String, done: () -> Void) {
    let root = "/tmp/jawp-\(UInt32.random(in: 0 ... .max))"
    try FileManager.default.createDirectory(atPath: root + "/media/links", withIntermediateDirectories: true)
    return (root, { try? FileManager.default.removeItem(atPath: root) })
}

/// A picture `w` by `h` of one colour, written as a JPEG at `path`.
func writeJPEG(_ path: String, _ w: Int, _ h: Int) throws {
    let ctx = try #require(CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                                     space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue))
    ctx.setFillColor(CGColor(srgbRed: 0.48, green: 0.64, blue: 0.97, alpha: 1))
    ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
    let image = try #require(ctx.makeImage())
    let destination = try #require(CGImageDestinationCreateWithURL(URL(fileURLWithPath: path) as CFURL, UTType.jpeg.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, image, nil)
    #expect(CGImageDestinationFinalize(destination))
}

/// A link's thumbnail at its square's pixels at 2x, from jawd's folder at `root`, a JPEG alone unless
/// `jpeg` says otherwise.
func thumbnail(_ path: String, in root: String, jpeg: Bool = true) -> CGImage? {
    Decode.picture(PictureKey(path: path, width: 140, height: 140, fit: .fill, blurred: false, jpeg: jpeg), from: .folder(root))
}

@Test func `a thumbnail is decoded so its short side fills its square, never larger than it is, a strip at four times its square`() throws {
    let square = PictureKey(path: "", width: 140, height: 140, fit: .fill, blurred: false, jpeg: true)
    #expect(Decode.longSide(400, 400, square) == 140)
    #expect(Decode.longSide(1000, 523, square) == 268, "a page's wide picture, 140 high")
    #expect(Decode.longSide(560, 280, square) == 280 && Decode.longSide(280, 560, square) == 280)
    #expect(Decode.longSide(140, 73, square) == 140, "one jawd made, as it is")
    #expect(Decode.longSide(96, 96, square) == 96, "and one smaller than its square")
    #expect(Decode.longSide(4000, 400, square) == 560, "a strip, at four times its square")
    let (root, done) = try pictureFolder()
    defer { done() }
    for (w, h) in [(400, 400), (560, 280), (280, 560), (140, 73), (96, 96)] {
        try writeJPEG("\(root)/media/links/\(w)x\(h).jpg", w, h)
        let image = try #require(thumbnail("media/links/\(w)x\(h).jpg", in: root))
        let drawn = pixels(image)
        #expect([image.width, image.height] == [140, 140] && drawn[3] == 255 && drawn[(140 * 140 - 1) * 4 + 3] == 255,
                "\(w)×\(h): drawn at its square's pixels, filling it to its corners")
    }
}

@Test func `only a JPEG is decoded for a link's thumbnail, as the protocol says one is`() throws {
    let (root, done) = try pictureFolder()
    defer { done() }
    try writeJPEG("\(root)/media/links/a.jpg", 140, 140)
    let ipma = try #require(SamplePictures.files["media/links/ipma.jpg"])
    let source = try #require(CGImageSourceCreateWithData(ipma as CFData, nil))
    let drawn = try #require(CGImageSourceCreateImageAtIndex(source, 0, nil))
    for (name, type) in [("b", UTType.png), ("c", UTType.tiff)] {
        let destination = try #require(CGImageDestinationCreateWithURL(URL(fileURLWithPath: "\(root)/media/links/\(name).jpg") as CFURL,
                                                                       type.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(destination, drawn, nil)
        #expect(CGImageDestinationFinalize(destination))
    }
    #expect(thumbnail("media/links/a.jpg", in: root) != nil)
    #expect(thumbnail("media/links/b.jpg", in: root) == nil, "a PNG under a JPEG's name")
    #expect(thumbnail("media/links/c.jpg", in: root) == nil, "nor a TIFF, though ImageIO reads both")
    #expect(thumbnail("media/links/b.jpg", in: root, jpeg: false) != nil, "as a photo's preview, which may be a PNG, is read")
}

@Test func `every link preview draws as its reference, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let image = try #require(render(spec(look, dark: dark), Samples.links, typing: false))
            try compare(image, "links-\(look.rawValue)-\(dark ? "dark" : "light")")
        }
    }
}
