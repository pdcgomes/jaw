import AppKit
import JawKit
import JawThemes

/// One message's row: it draws the layout made for it, and nothing it would have to measure.
final class MessageView: NSView {
    var laid: Laid? {
        didSet { needsDisplay = true }
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
        laid.draw(in: ctx)
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
        let what = m.deleted ? "This message was deleted" : (MessageLayout.Builder.kindLine(m).map { "\($0). " } ?? "") + m.plain
        let reactions = m.reactions.map { "\($0.n) \($0.emoji)" }.joined(separator: ", ")
        return [who, what, MessageLayout.Builder.spoken(m) ?? "", Format.clock(m.at), m.status?.rawValue ?? "", reactions]
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
