import AppKit
import JawThemes
import QuartzCore

/// The bubble at the bottom of a conversation while someone types, as WhatsApp draws it now: three
/// dots that rise and brighten one after another, in a bubble of their messages' colour, or bare in
/// lines, which has no bubbles.
enum TypingBubble {
    /// As high as a short message's bubble.
    static let size = CGSize(width: 57, height: 31)
    static let dot: CGFloat = 7
    static let gap: CGFloat = 4
    /// Each dot's round: up and bright in its first quarter, back by its half, the next dot a fifth
    /// of a second behind.
    static let round: CFTimeInterval = 1.4
    static let lag: CFTimeInterval = 0.2
    static let rise: CGFloat = 3
    /// A dot's opacity at rest, and at the top of its round.
    static let dim: Float = 0.4

    /// Where each dot is: centred in the bubble, or from the left edge in lines.
    static func dotRect(_ i: Int, look: Look) -> CGRect {
        let across = 3 * dot + 2 * gap
        let left = look == .bubbles ? (size.width - across) / 2 : 0
        return CGRect(x: left + CGFloat(i) * (dot + gap), y: (size.height - dot) / 2, width: dot, height: dot)
    }

    /// Draws it still in a context whose y runs down, its top left at `origin`: the bubble in
    /// bubbles, and the three dots unless they're on layers.
    static func draw(in ctx: CGContext, at origin: CGPoint, style: Style, dots: Bool = true) {
        ctx.saveGState()
        ctx.translateBy(x: origin.x, y: origin.y)
        if style.look == .bubbles {
            let radius = MessageLayout.Metrics(style).radius
            ctx.addPath(CGPath(roundedRect: CGRect(origin: .zero, size: size), cornerWidth: radius, cornerHeight: radius,
                               transform: nil))
            ctx.setFillColor(style.roles.surface.cgColor)
            ctx.fillPath()
        }
        if dots {
            ctx.setFillColor(style.roles.secondary.cgColor)
            for i in 0 ..< 3 {
                ctx.fillEllipse(in: dotRect(i, look: style.look))
            }
        }
        ctx.restoreGState()
    }
}

/// The typing bubble as a view: still, it draws the bubble and its dots; playing, the dots are
/// layers whose rounds run on the render server.
final class TypingDotsView: LiveStateView {
    var style = Style(roles: Catalog.jaw.dark) {
        didSet {
            needsDisplay = true
            placeDots()
        }
    }

    let dotLayers = (0 ..< 3).map { _ in CALayer() }

    init() {
        super.init(frame: NSRect(origin: .zero, size: TypingBubble.size))
        for d in dotLayers {
            d.isHidden = true
            d.actions = ["position": NSNull(), "bounds": NSNull(), "opacity": NSNull(), "hidden": NSNull(),
                         "backgroundColor": NSNull(), "cornerRadius": NSNull()]
            layer?.addSublayer(d)
        }
        placeDots()
    }

    override var intrinsicContentSize: NSSize {
        TypingBubble.size
    }

    override var isFlipped: Bool {
        true
    }

    override func draw(_: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        TypingBubble.draw(in: ctx, at: .zero, style: style, dots: !playing)
    }

    /// The dots' layers where the still dots are drawn, in this view's points, whose y runs down.
    func placeDots() {
        for (i, d) in dotLayers.enumerated() {
            d.frame = TypingBubble.dotRect(i, look: style.look)
            d.cornerRadius = TypingBubble.dot / 2
            d.backgroundColor = style.roles.secondary.cgColor
        }
    }

    override func play() {
        let times: [NSNumber] = [0, 0.25, 0.5, 1]
        let ease = CAMediaTimingFunction(name: .easeInEaseOut)
        for (i, d) in dotLayers.enumerated() {
            let begin = begin(d, after: TypingBubble.lag * Double(i))
            func round(_ path: String, _ values: [Any]) -> CAKeyframeAnimation {
                let a = CAKeyframeAnimation(keyPath: path)
                a.values = values
                a.keyTimes = times
                a.timingFunctions = [ease, ease, ease]
                a.duration = TypingBubble.round
                a.repeatCount = .infinity
                a.beginTime = begin
                a.isRemovedOnCompletion = false
                return a
            }
            let dim = TypingBubble.dim
            let brighten: [Float] = [dim, 1, dim, dim]
            let up: [CGFloat] = [0, -TypingBubble.rise, 0, 0]
            d.opacity = dim
            d.add(round("opacity", brighten), forKey: "brighten")
            d.add(round("transform.translation.y", up), forKey: "rise")
            d.isHidden = false
        }
        super.play()
    }

    override func stop() {
        for d in dotLayers {
            d.removeAllAnimations()
            d.isHidden = true
        }
        super.stop()
    }
}

/// The row after the newest message while someone types: the typing bubble where their message
/// will land, at the left edge their messages start from, with a run's space above it.
final class TypingRowView: NSView {
    let dots = TypingDotsView()
    var style = Style(roles: Catalog.jaw.dark) {
        didSet {
            dots.style = style
            needsLayout = true
        }
    }

    /// What VoiceOver says of it, as the header says it.
    var words = "typing…"
    /// A click gives the pane the keys, as one on a message does, and selects nothing.
    var clicked: (() -> Void)?
    /// Counts each coming and going, so a fade that's been overtaken leaves the row alone.
    var turn = 0
    var rising = false
    var fading = false

    init() {
        super.init(frame: .zero)
        addSubview(dots)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    nonisolated static func height(_ style: Style) -> CGFloat {
        MessageLayout.Metrics(style).runGap + TypingBubble.size.height + 1
    }

    override var isFlipped: Bool {
        true
    }

    /// Where the bubble rests: at the margin their messages start from, under a run's space.
    var rest: NSPoint {
        let metrics = MessageLayout.Metrics(style)
        return NSPoint(x: metrics.margin, y: metrics.runGap)
    }

    override func layout() {
        super.layout()
        if !rising { dots.setFrameOrigin(rest) }
    }

    /// Rises 6 points and fades in, as an arriving message does; at once under Reduce Motion.
    func arrive() {
        turn += 1
        fading = false
        guard !Motion.reduced() else {
            dots.alphaValue = 1
            dots.setFrameOrigin(rest)
            return
        }
        let mine = turn
        rising = true
        dots.alphaValue = 0
        dots.setFrameOrigin(NSPoint(x: rest.x, y: rest.y + 6))
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            dots.animator().alphaValue = 1
            dots.animator().setFrameOrigin(rest)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.16) { [weak self] in
            guard let self, turn == mine else { return }
            rising = false
        }
    }

    /// Typing came back while it was fading out: it stays.
    func stay() {
        guard fading else { return }
        turn += 1
        fading = false
        dots.alphaValue = 1
    }

    /// Fades out, then `gone` takes the row away; at once under Reduce Motion.
    func leave(_ gone: @escaping @MainActor () -> Void) {
        turn += 1
        guard !Motion.reduced() else { return gone() }
        let mine = turn
        fading = true
        NSAnimationContext.runAnimationGroup { context in
            context.duration = Motion.quick
            dots.animator().alphaValue = 0
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + Motion.quick) { [weak self] in
            guard let self, turn == mine else { return }
            fading = false
            gone()
        }
    }

    override func mouseDown(with _: NSEvent) {
        clicked?()
    }

    // MARK: VoiceOver

    override func isAccessibilityElement() -> Bool {
        true
    }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .staticText
    }

    override func accessibilityLabel() -> String? {
        words
    }
}
