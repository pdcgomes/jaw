import AppKit
import CoreGraphics
import ImageIO
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing
import UniformTypeIdentifiers

/// The layout: names and times decided as the terminal decides them, the cache keyed as it should
/// be, and every sample drawn in both looks, dark and light, compared with its reference picture.

let references = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appending(path: "References")

func spec(_ look: Look, dark: Bool, density: Density = .comfortable) -> LayoutSpec {
    LayoutSpec(style: Style(roles: Catalog.jaw.roles(dark: dark), look: look, density: density), me: ["person:1"])
}

@Test func `names show where a run starts, and days where they change`() {
    let rows = Samples.messages
    let cs = MessageContext.of(rows, group: true, unfolded: [], now: Samples.now)
    #expect(cs[0].day == "Yesterday" && cs[1].day == nil)
    #expect(cs.first { $0.day == "Today" } != nil, "a day separator where the day changes")
    let ana = rows.firstIndex { $0.id.hasSuffix("02") }!
    #expect(cs[ana].firstOfRun, "after a system line a run starts")
    let mine = rows.firstIndex { $0.id.hasSuffix("15") }!
    #expect(!cs[mine].firstOfRun, "your messages within five minutes are one run")
}

@Test func `a long message folds after 40 lines until it's unfolded`() {
    let long = Samples.messages.first { $0.id.hasSuffix("10") }!
    let s = spec(.bubbles, dark: true)
    let context = MessageContext(group: true, firstOfRun: true, showTime: true, day: nil)
    let folded = MessageLayout.lay(long, width: 600, context: context, style: s.style, fonts: s.fonts, me: s.me)
    var open = context
    open.unfolded = true
    let unfolded = MessageLayout.lay(long, width: 600, context: open, style: s.style, fonts: s.fonts, me: s.me)
    #expect(folded.height < unfolded.height)
    #expect(folded.regions.contains { $0.1 == .showMore } && !unfolded.regions.contains { $0.1 == .showMore })
}

@Test func `the cache lays out a message once per version, width and look`() {
    let layouts = Layouts()
    let m = Samples.messages[1]
    let context = MessageContext(group: true, firstOfRun: true, showTime: true, day: nil)
    let a = layouts.laid(m, width: 600.4, spec: spec(.bubbles, dark: true), context: context)
    let b = layouts.laid(m, width: 600.9, spec: spec(.bubbles, dark: true), context: context)
    #expect(a === b, "widths count in whole points")
    _ = layouts.laid(m, width: 600, spec: spec(.lines, dark: true), context: context)
    var edited = m
    edited.v += 1
    _ = layouts.laid(edited, width: 600, spec: spec(.bubbles, dark: true), context: context)
    #expect(layouts.count == 3)
    layouts.keep([m.id], width: 600, spec: spec(.bubbles, dark: true))
    #expect(layouts.count == 2, "only this width and look are kept")
}

@Test func `a tap on a reaction, a retry or a quote lands on it`() {
    let s = spec(.bubbles, dark: true)
    let rows = Samples.messages
    let cs = MessageContext.of(rows, group: true, unfolded: [], now: Samples.now)
    func laid(_ suffix: String) -> Laid {
        let i = rows.firstIndex { $0.id.hasSuffix(suffix) }!
        return MessageLayout.lay(rows[i], width: 640, context: cs[i], style: s.style, fonts: s.fonts, me: s.me)
    }
    #expect(laid("02").regions.contains { $0.1 == .reaction("❤") })
    #expect(laid("15").regions.contains { $0.1 == .retry })
    #expect(laid("03").regions.contains { if case .quote = $0.1 { true } else { false } })
    let link = laid("04")
    #expect(link.text != nil)
}

@Test func `every sample draws as its reference, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let name = "conversation-\(look.rawValue)-\(dark ? "dark" : "light")"
            let image = try #require(render(spec(look, dark: dark)))
            try compare(image, name)
        }
    }
    try compare(try #require(render(spec(.bubbles, dark: true, density: .compact))), "conversation-bubbles-dark-compact")
}

/// The samples drawn as a conversation 640 points wide, at 2x, as a conversation's rows draw them,
/// with the typing bubble still after the newest message, since Rui is typing in the sample group.
func render(_ spec: LayoutSpec) -> CGImage? {
    let rows = Samples.messages
    let cs = MessageContext.of(rows, group: true, unfolded: [], now: Samples.now)
    let laid = zip(rows, cs).map { MessageLayout.lay($0, width: 640, context: $1, style: spec.style, fonts: spec.fonts, me: spec.me) }
    let height = laid.reduce(0) { $0 + $1.height } + TypingRowView.height(spec.style) + 16
    let scale: CGFloat = 2
    guard let ctx = CGContext(data: nil, width: Int(640 * scale), height: Int(height * scale), bitsPerComponent: 8,
                              bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    ctx.scaleBy(x: scale, y: scale)
    ctx.translateBy(x: 0, y: height)
    ctx.scaleBy(x: 1, y: -1)
    ctx.setFillColor(spec.style.roles.ground.cgColor)
    ctx.fill(CGRect(x: 0, y: 0, width: 640, height: height))
    var y: CGFloat = 8
    for l in laid {
        l.draw(in: ctx, y: y)
        y += l.height
    }
    let metrics = MessageLayout.Metrics(spec.style)
    TypingBubble.draw(in: ctx, at: CGPoint(x: metrics.margin, y: y + metrics.runGap), style: spec.style)
    return ctx.makeImage()
}

/// Compares with the reference picture, writing it the first time, which fails until it's looked at
/// and committed.
func compare(_ image: CGImage, _ name: String) throws {
    let url = references.appending(path: "\(name).png")
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil), let reference = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        try FileManager.default.createDirectory(at: references, withIntermediateDirectories: true)
        let destination = try #require(CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(destination, image, nil)
        CGImageDestinationFinalize(destination)
        Issue.record("\(name): no reference yet; wrote \(url.lastPathComponent) to look at and commit")
        return
    }
    #expect(reference.width == image.width && reference.height == image.height, "\(name): its size changed")
    guard reference.width == image.width, reference.height == image.height else { return }
    let differ = differing(pixels(image), pixels(reference))
    #expect(differ <= image.width * image.height / 1000, "\(name): \(differ) pixels differ from the reference")
}

func pixels(_ image: CGImage) -> [UInt8] {
    var out = [UInt8](repeating: 0, count: image.width * image.height * 4)
    let ctx = CGContext(data: &out, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    ctx?.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
    return out
}

/// Pixels further apart than anti-aliasing puts them.
func differing(_ a: [UInt8], _ b: [UInt8]) -> Int {
    var n = 0
    for i in stride(from: 0, to: min(a.count, b.count), by: 4) {
        let d = max(abs(Int(a[i]) - Int(b[i])), abs(Int(a[i + 1]) - Int(b[i + 1])), abs(Int(a[i + 2]) - Int(b[i + 2])))
        if d > 24 { n += 1 }
    }
    return n
}

@MainActor @Test func `the Mimic chomps, dropping its lid three pixels, and sleeps grey with its eyes shut`() throws {
    let awake = try #require(Mimic.pixels)
    #expect(awake.w == 45 && awake.h == 26)
    #expect(Mimic.looking(awake, .chomp, 12, 0) == nil, "its top rows go")
    #expect(Mimic.looking(awake, .chomp, 12, 8 + 3).map { "\($0)" } == Mimic.looking(awake, .awake, 12, 8).map { "\($0)" },
            "the upper teeth drop with the lid")
    for y in 0 ..< awake.h {
        for x in 0 ..< awake.w {
            guard let p = Mimic.looking(awake, .asleep, x, y) else { continue }
            #expect(p.0 == p.1 && p.2 >= p.0 && p.2 - p.0 <= 8, "grey, a little cool, at (\(x), \(y))")
        }
    }
}

@MainActor @Test func `the box finds the @ or : being typed, after a space`() {
    #expect(ComposerController.token("hi @an").map { "\($0.0)\($0.2)" } == "@an")
    #expect(ComposerController.token("hi :thu").map { "\($0.0)\($0.2)" } == ":thu")
    #expect(ComposerController.token("mail@an") == nil, "not inside a word")
    #expect(ComposerController.token("@an na") == nil, "not once a space follows")
    #expect(Emoji.matching("thumbs", most: 3).contains { $0.emoji == "👍" })
}

@Test func `times say themselves as the terminal says them`() {
    let tz = TimeZone(identifier: "UTC")!
    let now = Samples.now
    #expect(Format.when(now - 60000, now: now, tz: tz) == Format.clock(now - 60000, tz: tz))
    #expect(Format.when(now - 86_400_000, now: now, tz: tz) == "yesterday")
    #expect(Format.day(now, now: now, tz: tz) == "Today")
    #expect(Format.num(1204) == "1,204" && Format.length(74) == "1:14")
    #expect(Format.ago(now - 5 * 60000, now: now, tz: tz) == "5 min ago")
}

@Test func `every kind jawd draws a card for decodes into its sample, and lays out with its card`() {
    let kinds = Samples.kinds
    #expect(kinds.map(\.kind) == [.video, .document, .location, .poll, .contact, .text, .voice, .album])
    #expect(kinds[0].media?.seconds == 42 && kinds[1].media?.pages == 12 && kinds[2].location?.name == "Cabo da Roca")
    #expect(kinds[3].poll?.options.count == 2 && kinds[3].poll?.options.first?.mine == true && kinds[4].contact?.name == "Climbing shop")
    #expect(kinds[5].link?.description != nil && kinds[7].album?.items.count == 5)
}
