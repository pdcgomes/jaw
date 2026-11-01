import AppKit
import JawKit
import JawThemes

/// One message's row: it draws the layout made for it, and nothing it would have to measure, and
/// its pictures from the bitmaps decoded for them, asking for what isn't there and letting it go
/// when it's reused or leaves the transcript, so what's scrolled past is never decoded.
final class MessageView: NSView {
    var laid: Laid? {
        didSet {
            if laid !== oldValue {
                drawn.removeAll()
                drewOnce = false
                askPictures()
            }
            needsDisplay = true
        }
    }

    /// The window's pictures, and this cell's claim on them.
    var pictures: MessagePictures? {
        didSet {
            guard pictures !== oldValue else { return }
            if let token { oldValue?.letGo(token) }
            token = pictures?.token()
        }
    }

    var token: PictureToken?
    /// What each slot was last drawn from, by its place in the layout: 1 its blurred thumbnail, 2 its
    /// preview; a slot drawn as its well has none. Whatever comes better after the cell has drawn
    /// fades in.
    var drawn: [Int: Int] = [:]
    var drewOnce = false

    /// The display's scale, for the pixels each picture is decoded at.
    var scale: CGFloat {
        window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2
    }

    var selected = false {
        didSet { if selected != oldValue { needsDisplay = true } }
    }

    var roles: Roles? {
        didSet { needsDisplay = true }
    }

    var message: MessageRow?
    /// Told of a click: where it landed in the layout, and how many clicks it was.
    var clicked: ((MessageView, CGPoint, Int) -> Void)?
    var menuFor: ((MessageView) -> NSMenu?)?
    var popUp = MenuPopUp()
    var hovered: ((MessageView, Bool) -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var isFlipped: Bool {
        true
    }

    override var wantsUpdateLayer: Bool {
        false
    }

    override func draw(_: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext, let laid else { return }
        if selected, let roles {
            ctx.setFillColor(roles.selection.opacity(0.55).cgColor)
            ctx.fill(CGRect(x: 0, y: laid.body.minY - 2, width: bounds.width, height: laid.body.height + 4))
        }
        Laid.draw(laid.parts, in: ctx, y: 0)
        let scale = scale
        for (i, slot) in laid.pictures.enumerated() {
            guard let drawable = pictures?.drawable(slot, scale: scale) else { continue }
            Laid.draw(drawable.image, in: slot, ctx: ctx, y: 0)
            drawn[i] = drawable.isPreview ? 2 : 1
        }
        drewOnce = true
        Laid.draw(laid.over, in: ctx, y: 0)
    }

    /// Asks for each picture the layout shows that isn't decoded at this cell's pixels.
    func askPictures() {
        guard let laid, let pictures, let token else { return }
        let scale = scale
        for slot in laid.pictures {
            pictures.ask(slot, scale: scale, for: token) { [weak self] in self?.pictureCame() }
        }
    }

    /// Lets go of what this cell asked for, as it's reused or leaves the transcript.
    func letGo() {
        if let token { pictures?.letGo(token) }
    }

    /// A picture came: drawn again, a preview that takes its thumbnail's place, or the well's, fading
    /// in on Core Animation, shorter under Reduce Motion; one decoded before the cell first drew it
    /// is just there.
    @discardableResult
    func pictureCame() -> Bool {
        guard let laid, let pictures else { return false }
        let scale = scale
        let fades = drewOnce && laid.pictures.indices.contains { i in
            (pictures.drawable(laid.pictures[i], scale: scale).map { $0.isPreview ? 2 : 1 } ?? 0) > drawn[i, default: 0]
        }
        if fades {
            let fade = CATransition()
            fade.type = .fade
            fade.duration = Motion.reduced() ? Motion.quick / 2 : Motion.quick
            layer?.add(fade, forKey: "picture")
        }
        needsDisplay = true
        return fades
    }

    override func prepareForReuse() {
        super.prepareForReuse()
        letGo()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { letGo() } else { askPictures() }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        askPictures()
        needsDisplay = true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas {
            removeTrackingArea(area)
        }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                                       owner: self))
    }

    override func mouseEntered(with _: NSEvent) {
        hovered?(self, true)
    }

    override func mouseExited(with _: NSEvent) {
        hovered?(self, false)
    }

    override func mouseDown(with event: NSEvent) {
        clicked?(self, convert(event.locationInWindow, from: nil), event.clickCount)
    }

    /// A right click on the message itself, where a left click acts on it, or on the buttons it shows
    /// while hovered, has the message's menu; one on the blank space around it in its row has none
    /// here, so it goes on up to the transcript and the pane's menu.
    override func menu(for event: NSEvent) -> NSMenu? {
        let point = convert(event.locationInWindow, from: nil)
        let onButtons = subviews.contains { $0 is HoverBar && $0.frame.contains(point) }
        guard onButtons || laid?.contains(point) == true else { return nil }
        return menuFor?(self)
    }

    override func resetCursorRects() {
        guard let laid else { return }
        for (rect, region) in laid.regions {
            if case .reaction = region { addCursorRect(rect, cursor: .pointingHand) } else {
                addCursorRect(rect, cursor: .pointingHand)
            }
        }
    }

    // MARK: VoiceOver

    override func isAccessibilityElement() -> Bool {
        true
    }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .staticText
    }

    /// Show Menu, which has no pointer for `menu(for:)` to find the message by: the message's menu
    /// under its bubble, the pane taking the keys first, as for a right click.
    override func accessibilityPerformShowMenu() -> Bool {
        let body = laid?.body ?? bounds
        return popUp.show(menuFor?(self), at: NSPoint(x: body.minX, y: body.maxY), in: self)
    }

    override func accessibilityLabel() -> String? {
        guard let m = message else { return nil }
        let who = m.mine ? "You" : (m.from?.name ?? "")
        let reactions = m.reactions.map { "\($0.n) \($0.emoji)" }.joined(separator: ", ")
        return [who, MessageLayout.Builder.said(m), MessageLayout.Builder.spoken(m) ?? "", Format.clock(m.at), m.status?.rawValue ?? "", reactions]
            .filter { !$0.isEmpty }
            .joined(separator: ", ")
    }
}

/// The buttons a hovered message shows beside it: react, reply, and the rest, each saying the key
/// that does the same once the message is selected.
final class HoverBar: NSStackView {
    var react: (() -> Void)?
    var reply: (() -> Void)?
    var more: ((NSView) -> Void)?
    var buttons: [IconButton] = []
    var roles: Roles? {
        didSet { colour() }
    }

    init() {
        super.init(frame: .zero)
        orientation = .horizontal
        spacing = Space.xxs
        edgeInsets = NSEdgeInsets(top: Space.xxs, left: Space.xs, bottom: Space.xxs, right: Space.xs)
        wantsLayer = true
        layer?.cornerRadius = Radius.field + 2
        layer?.borderWidth = Stroke.hairline
        buttons = [
            IconButton(symbol: "face.smiling", label: "React", shortcut: Shortcut("+", []), size: .small) { [weak self] in self?.react?() },
            IconButton(symbol: "arrowshape.turn.up.left", label: "Reply", shortcut: Shortcut("r", []), size: .small) { [weak self] in self?.reply?() },
        ]
        let more = IconButton(symbol: "ellipsis", label: "More", size: .small) {}
        more.pressedFrom = { [weak self] button in self?.tapMore(button) }
        buttons.append(more)
        for b in buttons {
            addArrangedSubview(b)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc func tapMore(_ sender: NSView) {
        more?(sender)
    }

    func colour() {
        guard let roles else { return }
        layer?.backgroundColor = roles.surface.cgColor
        layer?.borderColor = roles.separator.cgColor
        for b in buttons {
            b.roles = roles
        }
    }
}
