import AppKit
import JawThemes

/// A symbol button without a bezel: the panes' close, the box's strip, the message's hover actions,
/// the editor's split. It's tinted by its tone, filled while the pointer is over it, and its tooltip
/// gives its key, so the mouse teaches the keyboard.
@MainActor
public final class IconButton: NSButton {
    public enum Size: Sendable {
        case small, regular

        var side: CGFloat {
            self == .small ? 20 : 24
        }

        var point: CGFloat {
            self == .small ? 10 : 12
        }
    }

    public let label: String
    public let shortcut: Shortcut?
    let size: Size
    let tone: Tone
    let pressed: () -> Void
    var hovering = false {
        didSet { colour() }
    }

    public var roles: Roles? {
        didSet { colour() }
    }

    public init(symbol: String, label: String, shortcut: Shortcut? = nil, size: Size = .regular, tone: Tone = .tertiary,
                _ pressed: @escaping () -> Void)
    {
        self.label = label
        self.shortcut = shortcut
        self.size = size
        self.tone = tone
        self.pressed = pressed
        super.init(frame: NSRect(x: 0, y: 0, width: size.side, height: size.side))
        image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)
        symbolConfiguration = .init(pointSize: size.point, weight: .semibold)
        isBordered = false
        bezelStyle = .accessoryBarAction
        imagePosition = .imageOnly
        toolTip = Self.tip(label, shortcut)
        setAccessibilityLabel(label)
        target = self
        action = #selector(press)
        wantsLayer = true
        layer?.cornerRadius = Radius.small
        translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: size.side),
            heightAnchor.constraint(equalToConstant: size.side),
        ])
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

    @objc func press() {
        pressed()
    }

    override public func mouseEntered(with _: NSEvent) {
        hovering = true
    }

    override public func mouseExited(with _: NSEvent) {
        hovering = false
    }

    func colour() {
        guard let roles else { return }
        contentTintColor = (hovering ? Tone.primary : tone).color(roles).nsColor
        layer?.backgroundColor = hovering ? roles.selection.cgColor : nil
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

    public init(_ kind: Kind, radius: CGFloat = Radius.control) {
        self.kind = kind
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = radius
        layer?.borderWidth = Stroke.hairline
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func colour() {
        guard let roles else { return }
        layer?.backgroundColor = kind == .well ? nil : roles.surface.cgColor
        layer?.borderColor = roles.separator.cgColor
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
