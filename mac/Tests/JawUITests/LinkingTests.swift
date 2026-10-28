import AppKit
@testable import JawUI
import Testing

/// The linking screen's QR code: kept for its text, and made again for new text and kept in the first's
/// place; in a picture of it at one and at two pixels a point, as phase three's check and a sweep draw
/// it, every module a hard-edged square where the code has it; and what VoiceOver hears of it.

@MainActor @Test func `the QR code is kept until jawd's text changes, drawn crisp at one and two pixels a point, its dark modules where the code's are, and heard as the code to scan`() throws {
    let text = "2@jaw-harness,QWERTYUIOPASDFGHJKL,ZXCVBNM="
    let image = try #require(LinkingView.qr(text))
    #expect(LinkingView.qr(text) === image, "the same text gives the code kept")
    let another = try #require(LinkingView.qr("2@another"))
    #expect(another !== image, "new text gives a new code")
    #expect(LinkingView.qr("2@another") === another, "and is kept in place of the first")
    let code = NSBitmapImageRep(cgImage: image)
    let modules = code.pixelsWide
    func dark(_ x: Int, _ y: Int) -> Bool {
        (code.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB)?.redComponent ?? 1) < 0.5
    }
    let view = QRCodeView()
    view.image = image
    view.frame = NSRect(x: 0, y: 0, width: 260, height: 260)
    for scale in [1, 2] {
        let wide = 260 * scale
        let rep = try #require(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: wide, pixelsHigh: wide, bitsPerSample: 8,
                                                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                                bytesPerRow: wide * 4, bitsPerPixel: 32))
        rep.size = view.bounds.size
        view.cacheDisplay(in: view.bounds, to: rep)
        let data = try #require(rep.bitmapData)
        var grey = 0
        for y in 0 ..< wide {
            for x in 0 ..< wide {
                let i = y * rep.bytesPerRow + x * 4
                let pixel = (data[i], data[i + 1], data[i + 2], data[i + 3])
                if pixel != (0, 0, 0, 255), pixel != (255, 255, 255, 255) { grey += 1 }
            }
        }
        var misplaced = 0
        for y in 0 ..< modules {
            for x in 0 ..< modules {
                let centre = (Int((Double(x) + 0.5) * Double(wide) / Double(modules)), Int((Double(y) + 0.5) * Double(wide) / Double(modules)))
                if (data[centre.1 * rep.bytesPerRow + centre.0 * 4] < 128) != dark(x, y) { misplaced += 1 }
            }
        }
        #expect(grey == 0, "at \(scale)×, \(grey) of its \(wide * wide) pixels are neither black nor white")
        #expect(misplaced == 0, "at \(scale)×, \(misplaced) of its \(modules * modules) modules aren't as dark as the code's")
    }
    #expect(view.isAccessibilityElement())
    #expect(view.accessibilityRole() == .image)
    #expect(view.accessibilityLabel() == "The code to scan with your phone")
}
