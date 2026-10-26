import AppKit
import CoreGraphics
import ImageIO

/// The Mimic, badcoffee.dev's creature, from its 45 by 26 pixels, with `art.rs`'s looks: awake,
/// mid-chomp (its lid, eyes and upper teeth dropped three pixels onto its lower teeth), and asleep
/// (grey, its eyes shut).
public enum Mimic {
    public enum Look: Sendable, Hashable {
        case awake, chomp, asleep
    }

    /// Its eyes' rows: open, each is a ring on rows 2 to 4; shut, a line on row 4.
    static let eyes = 2 ... 4
    static let lid = 10
    static let chomp = 3

    struct Pixels {
        let w: Int
        let h: Int
        /// RGBA, row by row from the top; alpha 0 where it isn't.
        let px: [UInt8]

        func at(_ x: Int, _ y: Int) -> (UInt8, UInt8, UInt8)? {
            guard x >= 0, y >= 0, x < w, y < h else { return nil }
            let i = (y * w + x) * 4
            return px[i + 3] >= 128 ? (px[i], px[i + 1], px[i + 2]) : nil
        }
    }

    @MainActor static var pixels: Pixels? = load()

    static func load() -> Pixels? {
        guard let url = Bundle(for: Marker.self).url(forResource: "mimic", withExtension: "png")
            ?? Bundle.main.url(forResource: "mimic", withExtension: "png"),
            let source = CGImageSourceCreateWithURL(url as CFURL, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return nil }
        return pixels(of: image)
    }

    static func pixels(of image: CGImage) -> Pixels? {
        let (w, h) = (image.width, image.height)
        var px = [UInt8](repeating: 0, count: w * h * 4)
        guard let ctx = CGContext(data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        return Pixels(w: w, h: h, px: px)
    }

    static func luma(_ p: (UInt8, UInt8, UInt8)) -> Double {
        0.299 * Double(p.0) + 0.587 * Double(p.1) + 0.114 * Double(p.2)
    }

    /// The pixel at (x, y) as it looks, as `art.rs`'s `looking` has it.
    static func looking(_ s: Pixels, _ look: Look, _ x: Int, _ y: Int) -> (UInt8, UInt8, UInt8)? {
        var sy = y
        if look == .chomp {
            if y < chomp { return nil }
            if y <= lid + chomp { sy = y - chomp }
        }
        guard let p = s.at(x, sy) else { return nil }
        guard look == .asleep else { return p }
        var out = p
        if eyes.contains(sy) {
            let white = { (q: (UInt8, UInt8, UInt8)) in luma(q) > 190 }
            let iris = s.at(x, eyes.lowerBound).map(white) ?? false
            let lidColour = s.at(x, eyes.lowerBound - 1) ?? p
            if iris, sy == eyes.upperBound {
                out = (0xFB, 0xF6, 0xE6)
            } else if white(p) {
                out = lidColour
            }
        }
        let g = UInt8(min(luma(out) * 0.55, 247))
        return (g, g, g + 8)
    }

    /// The Mimic in a look, a pixel to a pixel, for drawing at whole multiples with nearest-neighbour
    /// scaling, never smoothed.
    @MainActor public static func image(_ look: Look) -> CGImage? {
        if let made = images[look] { return made }
        guard let s = pixels else { return nil }
        var px = [UInt8](repeating: 0, count: s.w * s.h * 4)
        for y in 0 ..< s.h {
            for x in 0 ..< s.w {
                guard let p = looking(s, look, x, y) else { continue }
                let i = (y * s.w + x) * 4
                (px[i], px[i + 1], px[i + 2], px[i + 3]) = (p.0, p.1, p.2, 255)
            }
        }
        let made = px.withUnsafeMutableBytes { raw -> CGImage? in
            CGContext(data: raw.baseAddress, width: s.w, height: s.h, bitsPerComponent: 8, bytesPerRow: s.w * 4,
                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)?.makeImage()
        }
        images[look] = made
        return made
    }

    @MainActor static var images: [Look: CGImage] = [:]

    public static let size = CGSize(width: 45, height: 26)

    final class Marker {}
}

/// The Mimic drawn at a whole multiple of its pixels. It chomps for 1.2 s when told to, its frames
/// stepped every 150 ms as the terminal steps them, unless Reduce Motion is on.
public final class MimicView: NSView {
    public var look: Mimic.Look = .awake {
        didSet { show() }
    }

    public var scale: CGFloat = 4 {
        didSet { invalidateIntrinsicContentSize() }
    }

    var chompTimer: Timer?
    var chompUntil: Date?
    var shut = false

    public init(scale: CGFloat = 4) {
        self.scale = scale
        super.init(frame: .zero)
        wantsLayer = true
        layer?.magnificationFilter = .nearest
        layer?.contentsGravity = .resizeAspect
        show()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public var intrinsicContentSize: NSSize {
        NSSize(width: Mimic.size.width * scale, height: Mimic.size.height * scale)
    }

    func show(_ shown: Mimic.Look? = nil) {
        layer?.contents = Mimic.image(shown ?? look)
    }

    /// Chomps, as when a message comes into an empty chat or linking is done.
    public func chomp() {
        guard look == .awake, !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else { return }
        chompUntil = Date().addingTimeInterval(1.2)
        chompTimer?.invalidate()
        shut = false
        chompTimer = Timer.scheduledTimer(withTimeInterval: 0.15, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let until = self.chompUntil, Date() < until else {
                    self?.chompTimer?.invalidate()
                    self?.show()
                    return
                }
                self.shut.toggle()
                self.show(self.shut ? .chomp : self.look)
            }
        }
    }
}
