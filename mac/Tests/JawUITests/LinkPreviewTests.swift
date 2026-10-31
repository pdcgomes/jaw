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
/// most and its host, in both looks, for messages received and sent; and jawd's folder giving a
/// thumbnail by its path.

func sampleLink(_ id: String) -> MessageRow {
    Samples.links.first { $0.id == id }!
}

func laid(_ m: MessageRow, _ s: LayoutSpec, width: CGFloat = 640) -> Laid {
    MessageLayout.lay(m, width: width, context: MessageContext(group: true, firstOfRun: true, showTime: true, day: nil),
                      style: s.style, fonts: s.fonts, me: s.me, pictures: s.pictures)
}

/// The card a click opens a link from: where it is, and what it opens.
func linkCard(_ l: Laid) -> (rect: CGRect, url: String)? {
    for (rect, region) in l.regions {
        if case let .open(url) = region { return (rect, url) }
    }
    return nil
}

func pictureParts(_ l: Laid) -> Int {
    l.parts.count { if case .picture = $0 { true } else { false } }
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
                                  style: s.style, fonts: s.fonts, me: s.me, pictures: s.pictures)
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

@Test func `a preview with neither title nor description has no card, and one whose thumbnail can't be read keeps its place`() throws {
    let s = spec(.bubbles, dark: true)
    var bare = sampleLink("L1")
    bare.link = LinkPreview(url: "https://www.ipma.pt/")
    #expect(linkCard(laid(bare, s)) == nil)
    let unread = LayoutSpec(style: s.style, me: s.me)
    let read = laid(sampleLink("L1"), s), without = laid(sampleLink("L1"), unread)
    #expect(pictureParts(without) == 0, "nothing to draw it from")
    #expect(without.height == read.height && linkCard(without)?.rect == linkCard(read)?.rect, "nothing moves when it's read")
    #expect(MessageLayout.Builder.spoken(sampleLink("L1")) == "link: Previsão para Sintra · IPMA, ipma.pt")
    #expect(MessageLayout.Builder.spoken(bare) == nil)
}

@Test func `jawd's folder gives a thumbnail by its path, and one jawd keeps later is read then`() throws {
    let root = "/tmp/jawp-\(UInt32.random(in: 0 ... .max))"
    try FileManager.default.createDirectory(atPath: root + "/media/links", withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: root) }
    let folder = FolderPictures(root: root)
    #expect(folder.id == root)
    #expect(folder.picture("media/links/ipma.jpg", pixels: 140) == nil, "not kept yet")
    let drawn = try #require(Samples.pictures.picture("media/links/ipma.jpg", pixels: 140))
    let destination = try #require(CGImageDestinationCreateWithURL(URL(fileURLWithPath: root + "/media/links/ipma.jpg") as CFURL,
                                                                   UTType.jpeg.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, drawn, nil)
    #expect(CGImageDestinationFinalize(destination))
    let read = try #require(folder.picture("media/links/ipma.jpg", pixels: 140), "read once it's there")
    #expect(read.width == 140 && read.height == 140)
    #expect(folder.picture("media/links/ipma.jpg", pixels: 140) === read, "and kept")
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

@Test func `jawd's folder decodes a picture so its short side fills its square, and never larger than it is`() throws {
    #expect(FolderPictures.longSide(400, 400, short: 140) == 140)
    #expect(FolderPictures.longSide(1000, 523, short: 140) == 268, "a page's wide picture, 140 high")
    #expect(FolderPictures.longSide(140, 73, short: 140) == 140, "one jawd made, as it is")
    #expect(FolderPictures.longSide(4000, 400, short: 140) == 560, "a strip, at four times its square")
    let (root, done) = try pictureFolder()
    defer { done() }
    let folder = FolderPictures(root: root)
    for (w, h, decoded) in [(400, 400, [140, 140]), (560, 280, [280, 140]), (280, 560, [140, 280]), (140, 73, [140, 73]), (96, 96, [96, 96])] {
        try writeJPEG("\(root)/media/links/\(w)x\(h).jpg", w, h)
        let image = try #require(folder.picture("media/links/\(w)x\(h).jpg", pixels: 140))
        #expect([image.width, image.height] == decoded, "\(w)×\(h)")
    }
}

@Test func `jawd's folder decodes only a JPEG, as the protocol says a link's thumbnail is`() throws {
    let (root, done) = try pictureFolder()
    defer { done() }
    try writeJPEG("\(root)/media/links/a.jpg", 140, 140)
    let drawn = try #require(Samples.pictures.picture("media/links/ipma.jpg", pixels: 140))
    for (name, type) in [("b", UTType.png), ("c", UTType.tiff)] {
        let destination = try #require(CGImageDestinationCreateWithURL(URL(fileURLWithPath: "\(root)/media/links/\(name).jpg") as CFURL,
                                                                       type.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(destination, drawn, nil)
        #expect(CGImageDestinationFinalize(destination))
    }
    let folder = FolderPictures(root: root)
    #expect(folder.picture("media/links/a.jpg", pixels: 140) != nil)
    #expect(folder.picture("media/links/b.jpg", pixels: 140) == nil, "a PNG under a JPEG's name")
    #expect(folder.picture("media/links/c.jpg", pixels: 140) == nil, "nor a TIFF, though ImageIO reads both")
}

@Test func `jawd's folder keeps the pictures drawn last within its bytes, and reads again one it let go`() throws {
    let (root, done) = try pictureFolder()
    defer { done() }
    let paths = (0 ..< 4).map { "media/links/\($0).jpg" }
    for path in paths {
        try writeJPEG("\(root)/\(path)", 140, 140)
    }
    let probe = try #require(FolderPictures(root: root).picture(paths[0], pixels: 140))
    let one = probe.bytesPerRow * probe.height
    let folder = FolderPictures(root: root, most: 3 * one)
    let first = try paths.prefix(3).map { try #require(folder.picture($0, pixels: 140)) }
    #expect(folder.kept.withLock { $0.bytes } == 3 * one, "three fill it")
    #expect(folder.picture(paths[0], pixels: 140) === first[0], "the first, drawn again")
    let fourth = try #require(folder.picture(paths[3], pixels: 140), "a fourth, past its bound")
    #expect(folder.kept.withLock { $0.bytes } == 3 * one && folder.kept.withLock { $0.pictures.count } == 3, "still within its bytes")
    #expect(folder.picture(paths[0], pixels: 140) === first[0] && folder.picture(paths[2], pixels: 140) === first[2],
            "those drawn last are kept")
    let again = try #require(folder.picture(paths[1], pixels: 140), "the one drawn longest ago is read again")
    #expect(again !== first[1] && again.width == 140 && again.height == 140)
    #expect(folder.picture(paths[3], pixels: 140) !== fourth, "and then the fourth, drawn longest ago, went")
    #expect(folder.kept.withLock { $0.bytes } <= 3 * one)
    let small = FolderPictures(root: root, most: one - 1)
    #expect(small.picture(paths[0], pixels: 140) != nil && small.kept.withLock { $0.pictures.isEmpty }, "one larger than its bound is drawn, not kept")
}

@Test func `a picture fills its rect from the centre, keeping its shape`() {
    let square = CGRect(x: 10, y: 20, width: 70, height: 70)
    #expect(Fill.rect(CGSize(width: 280, height: 140), filling: square) == CGRect(x: -25, y: 20, width: 140, height: 70))
    #expect(Fill.rect(CGSize(width: 96, height: 96), filling: square) == square)
}

@Test func `every link preview draws as its reference, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let image = try #require(render(spec(look, dark: dark), Samples.links, typing: false))
            try compare(image, "links-\(look.rawValue)-\(dark ? "dark" : "light")")
        }
    }
}
