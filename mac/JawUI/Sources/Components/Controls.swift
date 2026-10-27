import AppKit
import JawThemes

/// A symbol button without a bezel: the panes' close, the box's strip, the message's hover actions,
/// the bottom bar's controls. It draws its symbol itself, at the symbol's own size and centred,
/// since a button cell scales a symbol into its own metrics and squeezes it; with a title, the
/// title follows the symbol. It's tinted by its tone, filled while the pointer is over it, and its
/// tooltip gives its key, so the mouse teaches the keyboard. With a menu, a click opens it.
@MainActor
public final class IconButton: NSControl {
    public enum Size: Sendable {
        case small, regular

        var side: CGFloat {
            self == .small ? 22 : 26
        }

        var point: CGFloat {
            self == .small ? 11 : 13
        }
    }

    public let label: String
    public let shortcut: Shortcut?
    let symbol: String
    let size: Size
    let tone: Tone
    let pressed: () -> Void
    /// A title after the symbol, as the bottom bar's layout button has.
    public var title: String? {
        didSet {
            invalidateIntrinsicContentSize()
            needsDisplay = true
        }
    }

    /// Drawn as a circle in the accent, its symbol in the ground: the box's Send.
    public var filled = false {
        didSet { needsDisplay = true }
    }

    /// A menu a click opens, above the button when it sits low in the window.
    public var menuToOpen: (() -> NSMenu?)?
    /// Run on a click with the button, for a menu shown from it elsewhere.
    public var pressedFrom: ((IconButton) -> Void)?
    var hovering = false {
        didSet { needsDisplay = true }
    }

    public var roles: Roles? {
        didSet { needsDisplay = true }
    }

    public init(symbol: String, label: String, shortcut: Shortcut? = nil, size: Size = .regular, tone: Tone = .tertiary,
                title: String? = nil, _ pressed: @escaping () -> Void)
    {
        self.label = label
        self.shortcut = shortcut
        self.symbol = symbol
        self.size = size
        self.tone = tone
        self.title = title
        self.pressed = pressed
        super.init(frame: NSRect(x: 0, y: 0, width: size.side, height: size.side))
        toolTip = Self.tip(label, shortcut)
        setAccessibilityRole(.button)
        setAccessibilityLabel(label)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: size.side).isActive = true
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// "Close Pane (⌘W)", or the label alone.
    public nonisolated static func tip(_ label: String, _ shortcut: Shortcut?) -> String {
        shortcut.map { "\(label) (\($0.display))" } ?? label
    }

    var configuration: NSImage.SymbolConfiguration {
        NSImage.SymbolConfiguration(pointSize: size.point, weight: .medium, scale: .medium)
    }

    var titleFont: NSFont {
        symbol.isEmpty ? TextStyle.caption.emphasised : TextStyle.caption.nsFont
    }

    override public var intrinsicContentSize: NSSize {
        guard let title, !title.isEmpty else { return NSSize(width: size.side, height: size.side) }
        let symbolWidth = symbol.isEmpty ? 0
            : (NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?.withSymbolConfiguration(configuration)?.size.width ?? 0) + Space.xs
        let text = (title as NSString).size(withAttributes: [.font: titleFont]).width
        return NSSize(width: ceil(symbolWidth + text + Space.m * 2), height: size.side)
    }

    override public func draw(_: NSRect) {
        guard let roles else { return }
        if filled {
            (hovering ? roles.accent.mixed(with: roles.text, 0.15) : roles.accent).nsColor.setFill()
            NSBezierPath(ovalIn: bounds.insetBy(dx: 1, dy: 1)).fill()
        } else if hovering {
            roles.selection.nsColor.setFill()
            NSBezierPath(roundedRect: bounds, xRadius: Radius.small, yRadius: Radius.small).fill()
        }
        let colour = filled ? (roles.isDark ? roles.ground : RGBA(0xFFFFFF)).nsColor : (hovering ? Tone.primary : tone).color(roles).nsColor
        var x = Space.m
        if let image = symbol.isEmpty ? nil : NSImage(systemSymbolName: symbol, accessibilityDescription: label)?
            .withSymbolConfiguration(configuration.applying(.init(hierarchicalColor: colour)))
        {
            let s = image.size
            x = title == nil ? (bounds.width - s.width) / 2 : Space.m
            image.draw(in: NSRect(x: x.rounded(), y: ((bounds.height - s.height) / 2).rounded(), width: s.width, height: s.height))
            x += s.width + Space.xs
        }
        if let title {
            let text = NSAttributedString(string: title, attributes: [.font: titleFont, .foregroundColor: colour])
            let t = text.size()
            if symbol.isEmpty { x = ((bounds.width - t.width) / 2).rounded() }
            text.draw(at: NSPoint(x: x, y: ((bounds.height - t.height) / 2).rounded()))
        }
    }

    override public func mouseDown(with _: NSEvent) {
        if let menu = menuToOpen?() {
            let below = (window?.contentView.map { convert(bounds, to: $0).minY } ?? 100) < 200
            let y = below ? bounds.height + menu.size.height + Space.xs : -Space.xs
            menu.popUp(positioning: nil, at: NSPoint(x: 0, y: isFlipped ? -y + bounds.height : y), in: self)
        } else if let pressedFrom {
            pressedFrom(self)
        } else {
            pressed()
        }
    }

    override public func mouseEntered(with _: NSEvent) {
        hovering = true
    }

    override public func mouseExited(with _: NSEvent) {
        hovering = false
    }

    override public func accessibilityPerformPress() -> Bool {
        pressed()
        return true
    }
}

/// Labels in the type scale and the theme's tones.
@MainActor
public enum Labels {
    public static func make(_ text: String = "", _ style: TextStyle = .body, _ tone: Tone = .primary, roles: Roles? = nil) -> NSTextField {
        let field = NSTextField(labelWithString: text)
        field.font = style.nsFont
        field.lineBreakMode = .byTruncatingTail
        field.maximumNumberOfLines = 1
        if let roles { field.textColor = tone.color(roles).nsColor }
        return field
    }

    public static func style(_ field: NSTextField, _ tone: Tone, roles: Roles) {
        field.textColor = tone.color(roles).nsColor
    }
}

/// A rounded, layer-backed surface: the box's field, a popover's list, the hover bar, an empty
/// pane's well. Its fill and border are roles, so a theme change restyles it.
@MainActor
public final class Surface: NSView {
    public enum Kind: Sendable {
        /// The box's field: the surface's fill, a hairline.
        case field
        /// A list over the content: completions, the hover bar.
        case popover
        /// An empty place waiting for something: no fill, a hairline.
        case well
    }

    public let kind: Kind
    public var roles: Roles? {
        didSet { colour() }
    }

    let radius: CGFloat

    public init(_ kind: Kind, radius: CGFloat = Radius.control) {
        self.kind = kind
        self.radius = radius
        super.init(frame: .zero)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func colour() {
        needsDisplay = true
    }

    /// Its look is set each time it's drawn, so a layer AppKit makes again, when the view moves
    /// to another window or parent, still has it.
    override public var wantsUpdateLayer: Bool {
        true
    }

    override public func updateLayer() {
        guard let roles, let layer else { return }
        layer.cornerRadius = radius
        layer.borderWidth = Stroke.hairline
        layer.backgroundColor = kind == .well ? nil : roles.surface.cgColor
        layer.borderColor = roles.separator.cgColor
    }

    /// Gives a layer-backed view the same look, for one that can't be a `Surface`, such as the
    /// box's scroll view.
    public static func dress(_ view: NSView, _ kind: Kind, radius: CGFloat = Radius.field, roles: Roles) {
        view.wantsLayer = true
        view.layer?.cornerRadius = radius
        view.layer?.borderWidth = Stroke.hairline
        view.layer?.borderColor = roles.separator.cgColor
        view.layer?.backgroundColor = kind == .well ? nil : roles.surface.cgColor
    }
}
