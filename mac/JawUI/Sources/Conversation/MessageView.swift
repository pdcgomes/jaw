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

    override func menu(for _: NSEvent) -> NSMenu? {
        menuFor?(self)
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

    override func accessibilityLabel() -> String? {
        guard let m = message else { return nil }
        let who = m.mine ? "You" : (m.from?.name ?? "")
        let what = m.deleted ? "This message was deleted" : (MessageLayout.Builder.kindLine(m).map { "\($0). " } ?? "") + m.plain
        let reactions = m.reactions.map { "\($0.n) \($0.emoji)" }.joined(separator: ", ")
        return [who, what, Format.clock(m.at), m.status?.rawValue ?? "", reactions].filter { !$0.isEmpty }
            .joined(separator: ", ")
    }
}

/// The buttons a hovered message shows beside it: react, reply, and the rest.
final class HoverBar: NSStackView {
    var react: (() -> Void)?
    var reply: (() -> Void)?
    var more: ((NSView) -> Void)?

    init() {
        super.init(frame: .zero)
        orientation = .horizontal
        spacing = 2
        edgeInsets = NSEdgeInsets(top: 2, left: 4, bottom: 2, right: 4)
        wantsLayer = true
        layer?.cornerRadius = 12
        for (symbol, label, action) in [
            ("face.smiling", "React", #selector(tapReact)),
            ("arrowshape.turn.up.left", "Reply", #selector(tapReply)),
            ("ellipsis", "More", #selector(tapMore(_:))),
        ] {
            let button = NSButton(image: NSImage(systemSymbolName: symbol, accessibilityDescription: label)!,
                                  target: self, action: action)
            button.isBordered = false
            button.toolTip = label
            button.contentTintColor = .secondaryLabelColor
            button.widthAnchor.constraint(equalToConstant: 22).isActive = true
            button.heightAnchor.constraint(equalToConstant: 20).isActive = true
            addArrangedSubview(button)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc func tapReact() {
        react?()
    }

    @objc func tapReply() {
        reply?()
    }

    @objc func tapMore(_ sender: NSView) {
        more?(sender)
    }
}
