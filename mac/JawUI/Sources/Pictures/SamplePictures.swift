import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Pictures drawn for the harness and the tests, as jawd's previews and thumbnails would be of
/// them: shapes and colours, plainly not photos, so a review judges the frame and not the picture.
/// Each is held in memory under the path a row names it by, made once, and the same each time.
public enum SamplePictures {
    /// A made-up picture: its name, the original's size, and its colours, ground first.
    struct Drawn {
        let name: String
        let w: Int
        let h: Int
        let colours: [UInt32]
    }

    static let drawn: [Drawn] = [
        Drawn(name: "sunset", w: 4032, h: 3024, colours: [0x3D2C5E, 0xFF9E64, 0xF7768E, 0xE0AF68]),
        Drawn(name: "lake", w: 1600, h: 1200, colours: [0x1A3B5C, 0x7AA2F7, 0x9ECE6A, 0x2AC3DE]),
        Drawn(name: "wall", w: 3024, h: 4032, colours: [0x2B2D3A, 0xBB9AF7, 0x7DCFFF, 0xC0CAF5]),
        Drawn(name: "ridge", w: 4000, h: 1500, colours: [0x24283B, 0x73DACA, 0xFF9E64, 0x414868]),
        Drawn(name: "rope", w: 2000, h: 2000, colours: [0x3B2F2F, 0xF7768E, 0xE0AF68, 0x9ECE6A]),
        Drawn(name: "poster", w: 1080, h: 1920, colours: [0x16161E, 0x7AA2F7, 0xBB9AF7, 0xF7768E]),
        Drawn(name: "note", w: 640, h: 640, colours: [0x1F2335, 0x9ECE6A, 0x7DCFFF, 0xE0AF68]),
    ]

    /// Every picture's previews and thumbnail, and the sticker's, by path.
    public static let files: [String: Data] = {
        var out: [String: Data] = [:]
        // PNGs, whose flat colours come out as flat as they went in, so the pictures drawn from them
        // compress as the harness's and the tests' own pictures should.
        for d in drawn {
            for long in [256, 768] {
                out[preview(d.name, long)] = png(draw(d, long: long))
            }
            out[thumbnail(d.name)] = png(draw(d, long: 24, upTo: 256))
        }
        out[preview("smile", 256)] = png(sticker(256))
        out[preview("smile", 768)] = png(sticker(512))
        out[thumbnail("smile")] = png(sticker(64))
        for (path, drawn) in links {
            out[path] = encoded(link(drawn), UTType.jpeg, [kCGImageDestinationLossyCompressionQuality: 1.0])
        }
        return out
    }()

    /// The sample links' thumbnails, 140 pixels square as jawd keeps one, and JPEGs, as the protocol
    /// says they are: a sky, a hill in front of it, and a sun for the forecast.
    static let links: [String: (sky: UInt32, hill: UInt32, sun: Bool)] = [
        "media/links/ipma.jpg": (0x7AA2F7, 0x9ECE6A, true),
        "media/links/sintra.jpg": (0xBB9AF7, 0x414868, false),
        "media/links/pena.jpg": (0xFF9E64, 0xF7768E, false),
    ]

    public static func preview(_ name: String, _ long: Int) -> String {
        "media/previews/\(name).\(long).png"
    }

    public static func thumbnail(_ name: String) -> String {
        "media/previews/\(name).instant.png"
    }

    /// The picture drawn here that an attached file named `name` shows in the box's strip, by its
    /// name without the extension: lake.jpg as the lake; nil for one drawn as its icon.
    public static func attached(_ name: String) -> CGImage? {
        guard let data = files[preview((name as NSString).deletingPathExtension, 256)],
              let source = CGImageSourceCreateWithData(data as CFData, nil)
        else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    /// The original's size of a picture drawn here.
    public static func size(_ name: String) -> (w: Int, h: Int) {
        drawn.first { $0.name == name }.map { ($0.w, $0.h) } ?? (512, 512)
    }

    static func colour(_ hex: UInt32) -> CGColor {
        CGColor(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }

    static func context(_ w: Int, _ h: Int) -> CGContext {
        CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    }

    /// The picture at `long` pixels on its long side, and drawn up to `upTo` from there, as jawd draws
    /// the tiny copy a message brings at 256 pixels.
    static func draw(_ d: Drawn, long: Int, upTo: Int? = nil) -> CGImage {
        let scale = Double(long) / Double(max(d.w, d.h))
        let (w, h) = (max(Int(Double(d.w) * scale), 1), max(Int(Double(d.h) * scale), 1))
        let ctx = context(w, h)
        let (fw, fh) = (CGFloat(w), CGFloat(h))
        ctx.setFillColor(colour(d.colours[0]))
        ctx.fill(CGRect(x: 0, y: 0, width: fw, height: fh))
        ctx.setFillColor(colour(d.colours[3]).copy(alpha: 0.35)!)
        for i in stride(from: -1, through: 8, by: 2) {
            let x = fw * CGFloat(i) / 8
            ctx.move(to: CGPoint(x: x, y: 0))
            ctx.addLine(to: CGPoint(x: x + fw / 8, y: 0))
            ctx.addLine(to: CGPoint(x: x + fw / 8 + fh * 0.6, y: fh))
            ctx.addLine(to: CGPoint(x: x + fh * 0.6, y: fh))
            ctx.fillPath()
        }
        ctx.setFillColor(colour(d.colours[1]))
        let r = min(fw, fh) * 0.28
        ctx.fillEllipse(in: CGRect(x: fw * 0.62 - r, y: fh * 0.62 - r, width: 2 * r, height: 2 * r))
        ctx.setFillColor(colour(d.colours[2]))
        ctx.move(to: CGPoint(x: 0, y: 0))
        ctx.addLine(to: CGPoint(x: fw * 0.45, y: fh * 0.55))
        ctx.addLine(to: CGPoint(x: fw * 0.8, y: 0))
        ctx.fillPath()
        let image = ctx.makeImage()!
        guard let upTo else { return image }
        let s = Double(upTo) / Double(max(w, h))
        let big = context(Int(Double(w) * s), Int(Double(h) * s))
        big.interpolationQuality = .low
        big.draw(image, in: CGRect(x: 0, y: 0, width: big.width, height: big.height))
        return big.makeImage()!
    }

    /// A link's thumbnail as its colours say.
    static func link(_ drawn: (sky: UInt32, hill: UInt32, sun: Bool)) -> CGImage {
        let ctx = context(140, 140)
        ctx.setFillColor(colour(drawn.sky))
        ctx.fill(CGRect(x: 0, y: 0, width: 140, height: 140))
        if drawn.sun {
            ctx.setFillColor(colour(0xE0AF68))
            ctx.fillEllipse(in: CGRect(x: 86, y: 84, width: 32, height: 32))
        }
        ctx.setFillColor(colour(drawn.hill))
        ctx.move(to: CGPoint(x: 0, y: 0))
        ctx.addLine(to: CGPoint(x: 0, y: 52))
        ctx.addLine(to: CGPoint(x: 58, y: 92))
        ctx.addLine(to: CGPoint(x: 140, y: 36))
        ctx.addLine(to: CGPoint(x: 140, y: 0))
        ctx.closePath()
        ctx.fillPath()
        return ctx.makeImage()!
    }

    /// A round face on nothing, as a sticker's transparent preview.
    static func sticker(_ side: Int) -> CGImage {
        let ctx = context(side, side)
        let s = CGFloat(side)
        ctx.setFillColor(colour(0xE0AF68))
        ctx.fillEllipse(in: CGRect(x: s * 0.08, y: s * 0.08, width: s * 0.84, height: s * 0.84))
        ctx.setFillColor(colour(0x1A1B26))
        ctx.fillEllipse(in: CGRect(x: s * 0.32, y: s * 0.55, width: s * 0.1, height: s * 0.14))
        ctx.fillEllipse(in: CGRect(x: s * 0.58, y: s * 0.55, width: s * 0.1, height: s * 0.14))
        ctx.setLineWidth(s * 0.05)
        ctx.setStrokeColor(colour(0x1A1B26))
        ctx.addArc(center: CGPoint(x: s * 0.5, y: s * 0.5), radius: s * 0.22, startAngle: .pi * 1.15, endAngle: .pi * 1.85, clockwise: false)
        ctx.strokePath()
        return ctx.makeImage()!
    }

    static func png(_ image: CGImage) -> Data {
        encoded(image, UTType.png, [:])
    }

    static func encoded(_ image: CGImage, _ type: UTType, _ options: [CFString: Any]) -> Data {
        let data = NSMutableData()
        let destination = CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, image, options as CFDictionary)
        CGImageDestinationFinalize(destination)
        return data as Data
    }
}
