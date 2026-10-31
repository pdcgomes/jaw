import AppKit
import JawThemes
import QuartzCore

/// The header's "typing" while someone types: its dots counting up, ".", "..", "...", over and
/// over, under a subtle shimmer that sweeps across the words and the dots. Still, it's the words
/// with all three dots and no shimmer. Room for the three dots is always kept, so what follows them
/// doesn't move as they count. In a group the names of who's typing come first, cut short with "…"
/// when the header hasn't room for them, so the words and their dots always show.
final class TypingLabel: LiveStateView {
    /// Who's typing, in a group: "Ana, Rui".
    var who = "" {
        didSet { if who != oldValue { relayout() } }
    }

    var words = "typing" {
        didSet { if words != oldValue { relayout() } }
    }

    /// The most it may take across, which its header gives it.
    var room = CGFloat.greatestFiniteMagnitude {
        didSet { if room != oldValue { relayout() } }
    }

    /// `who` as it fits in `room`, with a space after it: whole, cut short with "…", or nothing.
    private(set) var shownWho = ""

    /// What comes before the dots, as it's drawn.
    var phrase: String {
        shownWho + words
    }

    /// What follows the dots, as " · muted".
    var rest = "" {
        didSet { if rest != oldValue { relayout() } }
    }

    var colour = Catalog.jaw.dark.online {
        didSet { if colour != oldValue { relayout() } }
    }

    let font = TextStyle.caption.nsFont
    let dotLayers = (0 ..< 3).map { _ in CALayer() }
    let shimmer = CAGradientLayer()

    /// How long each count shows.
    static let step: CFTimeInterval = 0.4
    /// The shimmer's sweep across, and its rest off the edge before the next.
    static let sweep: CFTimeInterval = 1.4
    static let pause: CFTimeInterval = 0.8
    /// The words' opacity outside the shimmer's band.
    static let shade: CGFloat = 0.7

    init() {
        super.init(frame: .zero)
        // Below a window's hold on its size, so names never widen the window, and above a stack's
        // hugging, so it keeps the width it asks for once `room` has fitted them.
        setContentCompressionResistancePriority(NSLayoutConstraint.Priority(490), for: .horizontal)
        for d in dotLayers {
            d.isHidden = true
            d.contentsGravity = .resize
            d.contentsScale = 2
            d.actions = ["position": NSNull(), "bounds": NSNull(), "opacity": NSNull(), "hidden": NSNull(),
                         "contents": NSNull()]
            layer?.addSublayer(d)
        }
        shimmer.startPoint = CGPoint(x: 0, y: 0.5)
        shimmer.endPoint = CGPoint(x: 1, y: 0.5)
        shimmer.colors = [CGColor(gray: 1, alpha: Self.shade), CGColor(gray: 1, alpha: 1), CGColor(gray: 1, alpha: Self.shade)]
        shimmer.locations = Self.band(at: -0.3)
        shimmer.actions = ["position": NSNull(), "bounds": NSNull(), "locations": NSNull()]
        relayout()
    }

    /// The shimmer's bright band, centred at `x` of the label's width, three tenths each side.
    static func band(at x: Double) -> [NSNumber] {
        [x - 0.3, x, x + 0.3].map { NSNumber(value: $0) }
    }

    var attributes: [NSAttributedString.Key: Any] {
        [.font: font, .foregroundColor: colour.nsColor]
    }

    func width(_ s: String) -> CGFloat {
        NSAttributedString(string: s, attributes: attributes).size().width
    }

    /// Where the `i`th dot starts: after the words and the dots before it.
    func dotX(_ i: Int) -> CGFloat {
        width(phrase + String(repeating: ".", count: i))
    }

    var lineHeight: CGFloat {
        ceil(NSAttributedString(string: phrase + "..." + rest, attributes: attributes).size().height)
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: ceil(width(phrase + "..." + rest)), height: lineHeight)
    }

    override var isFlipped: Bool {
        true
    }

    override func draw(_: NSRect) {
        let a = attributes
        if playing {
            NSAttributedString(string: phrase, attributes: a).draw(at: .zero)
            NSAttributedString(string: rest, attributes: a).draw(at: NSPoint(x: dotX(3), y: 0))
        } else {
            NSAttributedString(string: phrase + "..." + rest, attributes: a).draw(at: .zero)
        }
    }

    override func layout() {
        super.layout()
        shimmer.frame = bounds
    }

    /// Its size, the names that fit, the dots' places and their image follow the words, the room
    /// and the colour.
    func relayout() {
        shownWho = TypingWords.fitted(who, before: words + "..." + rest, in: room, width: width)
        invalidateIntrinsicContentSize()
        let (image, w) = dotImage()
        for (i, d) in dotLayers.enumerated() {
            d.frame = CGRect(x: dotX(i), y: 0, width: w, height: lineHeight)
            d.contents = image
        }
        needsDisplay = true
    }

    /// A dot drawn as the words draw it, from its advance's start, and its width in whole points.
    func dotImage() -> (CGImage?, CGFloat) {
        let size = CGSize(width: ceil(dotX(1) - dotX(0)), height: lineHeight)
        let scale: CGFloat = 2
        guard let ctx = CGContext(data: nil, width: Int(size.width * scale), height: Int(size.height * scale), bitsPerComponent: 8,
                                  bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return (nil, size.width) }
        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: 0, y: size.height)
        ctx.scaleBy(x: 1, y: -1)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        NSAttributedString(string: ".", attributes: attributes).draw(at: .zero)
        NSGraphicsContext.restoreGraphicsState()
        return (ctx.makeImage(), size.width)
    }

    override func play() {
        let counts: [[NSNumber]] = [[1, 1, 1], [0, 1, 1], [0, 0, 1]]
        for (i, d) in dotLayers.enumerated() {
            let a = CAKeyframeAnimation(keyPath: "opacity")
            a.values = counts[i]
            a.keyTimes = [0, NSNumber(value: 1.0 / 3), NSNumber(value: 2.0 / 3), 1]
            a.calculationMode = .discrete
            a.duration = 3 * Self.step
            a.repeatCount = .infinity
            a.beginTime = begin(d)
            a.isRemovedOnCompletion = false
            d.opacity = 1
            d.add(a, forKey: "count")
            d.isHidden = false
        }
        let sweep = CAKeyframeAnimation(keyPath: "locations")
        sweep.values = [Self.band(at: -0.3), Self.band(at: 1.3), Self.band(at: 1.3)]
        sweep.keyTimes = [0, NSNumber(value: Self.sweep / (Self.sweep + Self.pause)), 1]
        sweep.timingFunctions = [CAMediaTimingFunction(name: .easeInEaseOut), CAMediaTimingFunction(name: .linear)]
        sweep.duration = Self.sweep + Self.pause
        sweep.repeatCount = .infinity
        sweep.beginTime = begin(shimmer)
        sweep.isRemovedOnCompletion = false
        shimmer.frame = bounds
        shimmer.add(sweep, forKey: "sweep")
        layer?.mask = shimmer
        super.play()
    }

    override func stop() {
        for d in dotLayers {
            d.removeAllAnimations()
            d.isHidden = true
        }
        shimmer.removeAllAnimations()
        layer?.mask = nil
        super.stop()
    }
}
