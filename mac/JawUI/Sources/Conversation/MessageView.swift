import AppKit
import JawKit
import JawThemes

/// What a transcript's cells cost to draw while a bench measures them: how many were drawn and
/// the milliseconds each took, and how many pictures came for them.
@MainActor
public final class CellTiming {
    public static var now: CellTiming?
    public var draws = 0
    public var drawMs: [Double] = []
    public var came = 0

    public init() {}
}

/// One message's row: it draws the layout made for it, and nothing it would have to measure, and
/// its pictures from the bitmaps decoded for them, asking for what isn't there and letting it go
/// when it's reused or leaves the transcript, so what's scrolled past is never decoded.
final class MessageView: NSView {
    var laid: Laid? {
        didSet {
            if laid !== oldValue {
                drawn.removeAll()
                drewOnce = false
                if window != nil { askPictures() }
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
    /// The scale its pictures were last asked for at.
    var askedScale: CGFloat?
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

    override func draw(_ rect: NSRect) {
        guard let timing = CellTiming.now else { return drawn(rect) }
        let start = CACurrentMediaTime()
        drawn(rect)
        timing.draws += 1
        timing.drawMs.append((CACurrentMediaTime() - start) * 1000)
    }

    func drawn(_: NSRect) {
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

    /// Asks for each picture the layout shows that isn't decoded at this cell's pixels; only once it's
    /// in a window, whose scale says the pixels, so a new cell asks once.
    func askPictures() {
        guard window != nil, let laid, let pictures, let token else { return }
        let scale = scale
        askedScale = scale
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
        CellTiming.now?.came += 1
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

    /// On a display of another scale, what was asked for at the old one is let go before its own is
    /// asked for.
    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard window != nil, askedScale != nil, scale != askedScale else { return }
        letGo()
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

    /// What a picture or a file drags out as, and the picture of it to drag.
    var dragFor: ((MessageView, Int?) -> NSPasteboardWriting?)?
    /// A press on a picture or a file, which acts when it's let go unless it's dragged first.
    var down: (point: CGPoint, item: Int?, event: NSEvent)?

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if case let .media(item) = laid?.region(at: point) {
            down = (point, item, event)
            return
        }
        clicked?(self, point, event.clickCount)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let pressed = down else { return }
        let point = convert(event.locationInWindow, from: nil)
        guard hypot(point.x - pressed.point.x, point.y - pressed.point.y) > 4 else { return }
        down = nil
        guard let writer = dragFor?(self, pressed.item) else { return }
        let frame = laid?.regions.first { $0.1 == .media(item: pressed.item) }?.0 ?? bounds
        let dragging = NSDraggingItem(pasteboardWriter: writer)
        dragging.setDraggingFrame(frame, contents: snapshot(frame))
        beginDraggingSession(with: [dragging], event: pressed.event, source: self)
    }

    override func mouseUp(with event: NSEvent) {
        guard let pressed = down else { return }
        down = nil
        clicked?(self, pressed.point, event.clickCount)
    }

    /// What's drawn in `rect`, to drag.
    func snapshot(_ rect: CGRect) -> NSImage? {
        guard let rep = bitmapImageRepForCachingDisplay(in: rect) else { return nil }
        cacheDisplay(in: rect, to: rep)
        let image = NSImage(size: rect.size)
        image.addRepresentation(rep)
        return image
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
        let hd = m.media?.hd == true ? "HD" : ""
        let file = m.media.flatMap(Self.said) ?? ""
        let items = m.album?.items ?? []
        let tiles = items.enumerated().compactMap { i, item in
            item.media.flatMap(Self.said).map { "\(item.kind == "video" ? "video" : "photo") \(i + 1) of \(items.count): \($0)" }
        }.joined(separator: "; ")
        return [who, MessageLayout.Builder.said(m), MessageLayout.Builder.spoken(m) ?? "", hd, file, tiles, Format.clock(m.at),
                m.status?.rawValue ?? "", reactions]
            .filter { !$0.isEmpty }
            .joined(separator: ", ")
    }

    /// A file's state as VoiceOver says it, nothing once it's here: jawd's whole words and what can
    /// be done, how much has come, or that it isn't downloaded.
    static func said(_ media: Media) -> String? {
        if let words = MessageLayout.Builder.comingWords(media) { return [words.why, words.act].compactMap(\.self).joined(separator: ", ") }
        switch media.state {
        case .downloading: return "downloading" + (media.progress.map { ", \(Int(($0 * 100).rounded()))%" } ?? "")
        case .remote: return "not downloaded"
        default: return nil
        }
    }
}

extension MessageView: NSDraggingSource {
    func draggingSession(_: NSDraggingSession, sourceOperationMaskFor _: NSDraggingContext) -> NSDragOperation {
        .copy
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
