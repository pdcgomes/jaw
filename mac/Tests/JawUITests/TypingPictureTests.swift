import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// Who's typing as it's drawn, compared with reference pictures: a pane's header naming them and the
/// bubble after the newest message, in bubbles and lines, dark and light.

/// `view` drawn at 2x, as the harness draws its pictures whatever the screen's scale, over `ground`,
/// which views drawn into a picture leave out.
@MainActor
func picture(_ view: NSView, on ground: RGBA) throws -> CGImage {
    let bounds = view.bounds
    let wide = Int((bounds.width * 2).rounded()), high = Int((bounds.height * 2).rounded())
    let rep = try #require(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: wide, pixelsHigh: high, bitsPerSample: 8,
                                           samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                           bytesPerRow: wide * 4, bitsPerPixel: 32))
    rep.size = bounds.size
    view.cacheDisplay(in: bounds, to: rep)
    let drawn = try #require(rep.cgImage)
    let ctx = try #require(CGContext(data: nil, width: wide, height: high, bitsPerComponent: 8, bytesPerRow: 0,
                                     space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                     bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    let all = CGRect(x: 0, y: 0, width: wide, height: high)
    ctx.setFillColor(ground.cgColor)
    ctx.fill(all)
    ctx.draw(drawn, in: all)
    return try #require(ctx.makeImage())
}

/// Runs as of the samples' now, so the days and times drawn don't depend on when it runs.
@MainActor
func asOfSamples(_ body: () async throws -> Void) async rethrows {
    let before = Format.currentTime
    Format.currentTime = { Samples.now }
    defer { Format.currentTime = before }
    try await body()
}

@MainActor @Test(.timeLimit(.minutes(2))) func `a pane draws who's typing in its header and the bubble after the newest message, in both looks, dark and light`() async throws {
    try await stillMotion {
        try await asOfSamples {
            for look in Look.allCases {
                for dark in [true, false] {
                    let layout = spec(look, dark: dark)
                    let (c, _) = await pane(typing: [anaTyping, ruiRecording], layout: layout, width: 480, height: 300)
                    c.scroll.verticalScroller?.isHidden = true
                    #expect(c.showsTyping && c.isAtBottom && c.header.typingLabel.phrase == "Ana, Rui typing")
                    try compare(try picture(c.view, on: layout.style.roles.ground), "typing-pane-\(look.rawValue)-\(dark ? "dark" : "light")")
                }
            }
        }
    }
}
