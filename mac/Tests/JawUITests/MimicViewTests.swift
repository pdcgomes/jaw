import AppKit
@testable import JawUI
import Testing

/// The Mimic's view: each of its pixels a whole square, in a picture of it as on the screen, and its
/// chomp still when `Motion` says motion is reduced.

@MainActor @Test func `the Mimic's view draws each of its pixels as a whole square, in a picture as on the screen`() throws {
    let view = MimicView(scale: 8)
    view.frame = NSRect(origin: .zero, size: view.intrinsicContentSize)
    let (wide, high) = (Int(view.bounds.width), Int(view.bounds.height))
    let rep = try #require(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: wide, pixelsHigh: high, bitsPerSample: 8,
                                            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                            bytesPerRow: wide * 4, bitsPerPixel: 32))
    rep.size = view.bounds.size
    view.cacheDisplay(in: view.bounds, to: rep)
    let data = try #require(rep.bitmapData)
    func colour(_ x: Int, _ y: Int) -> UInt32 {
        let i = y * rep.bytesPerRow + x * 4
        return UInt32(data[i]) << 24 | UInt32(data[i + 1]) << 16 | UInt32(data[i + 2]) << 8 | UInt32(data[i + 3])
    }
    let pixels = try #require(Mimic.pixels)
    let (w, h) = (Int(Mimic.size.width), Int(Mimic.size.height))
    var mixed = 0
    var misplaced = 0
    for y in 0 ..< h {
        for x in 0 ..< w {
            let corner = colour(x * 8, y * 8)
            if (0 ..< 64).contains(where: { colour(x * 8 + $0 % 8, y * 8 + $0 / 8) != corner }) { mixed += 1 }
            let opaque = corner & 0xFF == 0xFF
            if opaque != (Mimic.looking(pixels, .awake, x, y) != nil) { misplaced += 1 }
        }
    }
    #expect(mixed == 0, "\(mixed) of its \(w * h) blocks of 8 by 8 aren't one colour")
    #expect(misplaced == 0, "\(misplaced) blocks are opaque where it has no pixel, or not where it has one")
}

@MainActor @Test func `the Mimic's chomp asks Motion whether to move, not the system`() {
    let before = Motion.reduced
    defer { Motion.reduced = before }
    let mimic = MimicView()
    Motion.reduced = { true }
    mimic.chomp()
    #expect(mimic.chompTimer == nil, "still under Reduce Motion")
    Motion.reduced = { false }
    mimic.chomp()
    #expect(mimic.chompTimer != nil, "chomping when motion isn't reduced")
    mimic.chompTimer?.invalidate()
}
