import CoreGraphics
import Foundation
import ImageIO
@testable import JawKit
@testable import JawUI
import Testing
import UniformTypeIdentifiers

/// The pictures: the preview the pixels drawn need, decodes at those pixels off the main thread,
/// four at a time, what's let go never decoded, and the cache's bytes kept to its limit, the least
/// recently drawn going first.

/// A JPEG of flat colour, `w` by `h`, as a preview of a picture is.
func jpeg(_ w: Int, _ h: Int, grey: CGFloat = 0.5) -> Data {
    let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.setFillColor(CGColor(gray: grey, alpha: 1))
    ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
    ctx.setFillColor(CGColor(gray: 1 - grey, alpha: 1))
    ctx.fill(CGRect(x: 0, y: 0, width: w / 2, height: h / 2))
    let data = NSMutableData()
    let destination = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, ctx.makeImage()!, nil)
    CGImageDestinationFinalize(destination)
    return data as Data
}

/// Waits until `condition` holds or a few seconds pass; says whether it held.
@MainActor
func eventually(seconds: Double = 5, _ condition: () -> Bool) async -> Bool {
    let until = Date().addingTimeInterval(seconds)
    while Date() < until {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(5))
    }
    return condition()
}

/// A JPEG of noise, which decodes as slowly as a camera's photo of that size.
func noisy(_ w: Int, _ h: Int) -> Data {
    let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
    let bytes = ctx.data!.assumingMemoryBound(to: UInt32.self)
    var seed: UInt32 = 0x2545_F491
    for i in 0 ..< w * h {
        seed ^= seed << 13
        seed ^= seed >> 17
        seed ^= seed << 5
        bytes[i] = seed
    }
    let data = NSMutableData()
    let destination = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, ctx.makeImage()!, [kCGImageDestinationLossyCompressionQuality: 0.9] as CFDictionary)
    CGImageDestinationFinalize(destination)
    return data as Data
}

func key(_ path: String, _ w: Int, _ h: Int, fit: PictureFit = .fill, blurred: Bool = false) -> PictureKey {
    PictureKey(path: path, width: w, height: h, fit: fit, blurred: blurred)
}

@Test func `the 256-pixel preview is drawn up to 256 pixels, the 768 above, then the other`() {
    let both = PictureSource(small: "s.jpg", large: "l.jpg", thumbnail: "t.jpg")
    #expect(both.previews(for: 128) == ["s.jpg", "l.jpg"])
    #expect(both.previews(for: 256) == ["s.jpg", "l.jpg"], "256 pixels or fewer")
    #expect(both.previews(for: 257) == ["l.jpg", "s.jpg"])
    #expect(PictureSource(small: nil, large: "l.jpg").previews(for: 100) == ["l.jpg"])
    #expect(PictureSource(thumbnail: "t.jpg").previews(for: 640).isEmpty && !PictureSource(thumbnail: "t.jpg").isEmpty)
    #expect(PictureSource().isEmpty)

    let tile = PictureSlot(rect: CGRect(x: 0, y: 0, width: 128, height: 128), radius: 4, source: both, message: "m")
    #expect(tile.keys(1).previews.first == key("s.jpg", 128, 128), "an album's tile at 1x takes the 256")
    #expect(tile.keys(2).previews.first == key("s.jpg", 256, 256), "and at 2x still")
    let photo = PictureSlot(rect: CGRect(x: 0, y: 0, width: 320, height: 240), radius: 10, source: both, message: "m")
    #expect(photo.keys(2).previews.first == key("l.jpg", 640, 480), "a photo at 2x takes the 768")
    #expect(photo.keys(2).thumbnail == key("t.jpg", 160, 120, blurred: true), "a blurred thumbnail at a quarter of its pixels each way")
}

@Test func `a picture is decoded at the pixels it's drawn at, filled from its centre or fitted whole`() throws {
    let files = PictureFiles.memory(["wide.jpg": jpeg(768, 384), "small.jpg": jpeg(64, 48)])
    let filled = try #require(Decode.picture(key("wide.jpg", 320, 320), from: files))
    #expect(filled.width == 320 && filled.height == 320)
    let fitted = try #require(Decode.picture(key("wide.jpg", 320, 320, fit: .fit), from: files))
    let corner = pixels(fitted)
    #expect(fitted.width == 320 && corner[3] == 0, "fitted whole, nothing over its top corner")
    #expect(pixels(filled)[3] == 255, "filled, its corner drawn")
    let up = try #require(Decode.picture(key("small.jpg", 640, 480), from: files))
    #expect(up.width == 640 && up.height == 480, "a small file still fills its place")
    #expect(Decode.picture(key("missing.jpg", 10, 10), from: files) == nil)
    #expect(Decode.picture(key("wide.jpg", 10, 10), from: .none) == nil, "nothing before jawd says where its folder is")
}

@Test func `a thumbnail is blurred the same each time, and differs from itself unblurred`() throws {
    let files = PictureFiles.memory(["t.jpg": jpeg(96, 72, grey: 0.15)])
    let a = try #require(Decode.picture(key("t.jpg", 320, 240, blurred: true), from: files))
    let b = try #require(Decode.picture(key("t.jpg", 320, 240, blurred: true), from: files))
    let sharp = try #require(Decode.picture(key("t.jpg", 320, 240), from: files))
    #expect(pixels(a) == pixels(b))
    #expect(differing(pixels(a), pixels(sharp)) > 100, "the edge between its halves is soft")
    #expect(a.width == 320 && a.height == 240)
}

@Test func `the cache keeps to its bytes, letting go of the least recently drawn first`() throws {
    let files = PictureFiles.memory(["a": jpeg(100, 100), "b": jpeg(100, 100), "c": jpeg(100, 100)])
    let each = 200 * 200 * 4
    let cache = PictureCache(limit: 2 * each + 100)
    for name in ["a", "b"] {
        cache.keep(try #require(Decode.picture(key(name, 200, 200), from: files)), as: key(name, 200, 200))
    }
    #expect(cache.count == 2 && cache.bytes == 2 * each)
    _ = cache.image(key("a", 200, 200))
    cache.keep(try #require(Decode.picture(key("c", 200, 200), from: files)), as: key("c", 200, 200))
    #expect(cache.has(key("a", 200, 200)) && !cache.has(key("b", 200, 200)) && cache.has(key("c", 200, 200)),
            "b, drawn least recently, went")
    #expect(cache.bytes <= cache.limit && cache.peak == 2 * each)
    #expect(cache.another(key("a", 100, 100)) != nil, "a at another size can stand in meanwhile")
    let huge = try #require(Decode.picture(key("a", 400, 400), from: files))
    cache.keep(huge, as: key("a", 400, 400))
    #expect(!cache.has(key("a", 400, 400)), "one larger than the whole cache isn't kept")
    #expect(PictureCache().limit == 64 << 20)
}

@MainActor @Test func `forty asked decode four at a time, and what's let go before it starts is never decoded`() async throws {
    // Noise, so each takes a camera photo's time to decode, as a flat picture wouldn't.
    let noise = noisy(2400, 1800)
    var files: [String: Data] = [:]
    for i in 0 ..< 40 {
        files["p\(i)"] = noise
    }
    let cache = PictureCache()
    let queue = PictureQueue(cache: cache)
    queue.files = .memory(files)
    let token = queue.token()
    final class Came: @unchecked Sendable {
        var keys: [PictureKey] = []
    }
    let came = Came()
    for i in 0 ..< 40 {
        queue.ask(key("p\(i)", 300, 225), for: token) { k in came.keys.append(k) }
    }
    queue.letGo(token)
    let quiet = await eventually(seconds: 10) { !queue.busy }
    #expect(quiet)
    let counts = queue.counts
    #expect(counts.mostAtOnce == 4, "\(counts)")
    #expect(counts.decoded + counts.dropped == 40, "each was decoded or dropped: \(counts)")
    #expect(counts.dropped >= 30, "those not yet started when they were let go weren't decoded: \(counts)")
    #expect(came.keys.count <= counts.decoded, "those let go while decoding were kept but not handed over")
    #expect(cache.count == counts.decoded)

    let again = queue.token()
    let done = Came()
    queue.ask(key("p39", 300, 225), for: again) { k in done.keys.append(k) }
    #expect(await eventually { done.keys == [key("p39", 300, 225)] }, "what's asked and kept is handed over on the main thread")
}

@MainActor @Test func `a slot draws its best preview, another size of it meanwhile, then its blurred thumbnail, and asks only for what it lacks`() async throws {
    let files: [String: Data] = ["s.jpg": jpeg(256, 192), "l.jpg": jpeg(768, 576), "t.jpg": jpeg(96, 72)]
    let pictures = MessagePictures(files: .memory(files))
    let slot = PictureSlot(rect: CGRect(x: 0, y: 0, width: 320, height: 240), radius: 10,
                           source: PictureSource(small: "s.jpg", large: "l.jpg", thumbnail: "t.jpg"), message: "m")
    #expect(pictures.drawable(slot, scale: 2) == nil, "nothing yet: its well shows")
    final class Count: @unchecked Sendable {
        var n = 0
    }
    let came = Count()
    let token = pictures.token()
    pictures.ask(slot, scale: 2, for: token) { came.n += 1 }
    #expect(await eventually { came.n == 2 }, "its thumbnail and its preview come")
    let drawn = try #require(pictures.drawable(slot, scale: 2))
    #expect(drawn.isPreview && drawn.exact && drawn.image.width == 640)
    #expect(pictures.queue.counts.decoded == 2)
    #expect(!pictures.cache.has(try #require(slot.keys(2).thumbnail)), "its thumbnail let go once its preview is drawn")

    let wider = PictureSlot(rect: CGRect(x: 0, y: 0, width: 300, height: 225), radius: 10, source: slot.source, message: "m")
    let meanwhile = try #require(pictures.drawable(wider, scale: 2))
    #expect(meanwhile.isPreview && !meanwhile.exact, "another size of its preview stands in while its own is decoded")
    pictures.ask(wider, scale: 2, for: token) { came.n += 1 }
    #expect(await eventually { came.n == 3 }, "and only its preview is asked for: \(came.n)")

    let broken = PictureSlot(rect: slot.rect, radius: 10, source: PictureSource(small: "s.jpg", large: "gone.jpg", thumbnail: "t.jpg"), message: "m")
    pictures.ask(broken, scale: 2, for: token) { came.n += 1 }
    _ = await eventually { pictures.queue.failed(key("gone.jpg", 640, 480)) }
    pictures.ask(broken, scale: 2, for: token) { came.n += 1 }
    #expect(await eventually { pictures.drawable(broken, scale: 2)?.isPreview == true }, "a preview that can't be read gives way to the other")
}

@MainActor @Test func `pictures that must come out whole are decoded as they're asked for`() throws {
    let pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
    let source = PictureSource(small: SamplePictures.preview("lake", 256), large: SamplePictures.preview("lake", 768),
                               thumbnail: SamplePictures.thumbnail("lake"))
    let slot = PictureSlot(rect: CGRect(x: 0, y: 0, width: 320, height: 240), radius: 10, source: source, message: "m")
    final class Count: @unchecked Sendable {
        var n = 0
    }
    let came = Count()
    pictures.ask(slot, scale: 2, for: pictures.token()) { came.n += 1 }
    #expect(came.n >= 1 && pictures.drawable(slot, scale: 2)?.isPreview == true)
    #expect(SamplePictures.files.count == 3 * 8, "each made-up picture's two previews and thumbnail, and the sticker's")
}
