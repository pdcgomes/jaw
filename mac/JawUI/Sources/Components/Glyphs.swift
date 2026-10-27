import AppKit
import JawKit
import JawThemes

/// Your ticks, drawn as two strokes each rather than the ✓ character, which comes from a fallback
/// font and squeezes when two are kerned together.
public enum Ticks {
    /// How wide `count` ticks are at a text size.
    public static func width(_ count: Int, size: CGFloat) -> CGFloat {
        guard count > 0 else { return 0 }
        let w = size * 0.72
        return w + CGFloat(count - 1) * w * 0.42
    }

    /// Draws `count` ticks, their bottom on `baseline`, from `x`, in a top-down context.
    public static func draw(_ count: Int, x: CGFloat, baseline: CGFloat, size: CGFloat, colour: CGColor, in ctx: CGContext) {
        let w = size * 0.72, h = size * 0.58
        ctx.saveGState()
        ctx.setStrokeColor(colour)
        ctx.setLineWidth(max(1.1, size * 0.1))
        ctx.setLineCap(.round)
        ctx.setLineJoin(.round)
        for i in 0 ..< count {
            let x0 = x + CGFloat(i) * w * 0.42
            if i == 0 {
                ctx.move(to: CGPoint(x: x0, y: baseline - h * 0.48))
                ctx.addLine(to: CGPoint(x: x0 + w * 0.34, y: baseline - h * 0.08))
            } else {
                ctx.move(to: CGPoint(x: x0 + w * 0.2, y: baseline - h * 0.2))
                ctx.addLine(to: CGPoint(x: x0 + w * 0.34, y: baseline - h * 0.08))
            }
            ctx.addLine(to: CGPoint(x: x0 + w, y: baseline - h))
        }
        ctx.strokePath()
        ctx.restoreGState()
    }

    /// How many ticks a status has, and in which colour: one sent, two delivered, two in the accent
    /// read; nil for the states said in words or symbols.
    public static func of(_ status: MessageStatus?, _ roles: Roles) -> (Int, RGBA)? {
        switch status {
        case .sent: (1, roles.tertiary)
        case .delivered: (2, roles.tertiary)
        case .read, .played: (2, roles.accent)
        case .pending, .failed, nil: nil
        }
    }
}

/// A chat's picture: jawd's avatar, or its initials on its colour.
public enum Avatar {
    public static func draw(_ name: String, id: String, image: NSImage?, in rect: NSRect, roles: Roles) {
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(ovalIn: rect).addClip()
        if let image {
            image.draw(in: rect)
        } else {
            roles.sender(colour(id)).opacity(0.85).nsColor.setFill()
            rect.fill()
            let s = NSAttributedString(string: initials(name), attributes: [
                .font: NSFont.systemFont(ofSize: (rect.height * 0.4).rounded(), weight: .semibold),
                .foregroundColor: (roles.isDark ? roles.ground : RGBA(0xFFFFFF)).nsColor,
            ])
            let size = s.size()
            s.draw(at: NSPoint(x: rect.midX - size.width / 2, y: rect.midY - size.height / 2))
        }
        NSGraphicsContext.restoreGraphicsState()
    }

    /// Up to two letters: a name's first words' first letters, or its first character.
    public static func initials(_ name: String) -> String {
        let words = name.split(separator: " ").filter { $0.first?.isLetter ?? false }
        let letters = words.prefix(2).compactMap(\.first).map(String.init).joined()
        return letters.isEmpty ? String(name.prefix(1)) : letters.uppercased()
    }

    /// A chat's colour among the senders', the same each time.
    public static func colour(_ id: String) -> Int {
        id.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0xFFFF } % 8
    }
}

/// An avatar as a view, for the header.
@MainActor
public final class AvatarView: NSView {
    var name = ""
    var id = ""
    var image: NSImage?
    var roles: Roles?

    public func show(_ name: String, id: String, image: NSImage?, roles: Roles) {
        self.name = name
        self.id = id
        self.image = image
        self.roles = roles
        needsDisplay = true
    }

    override public func draw(_: NSRect) {
        guard let roles else { return }
        Avatar.draw(name, id: id, image: image, in: bounds, roles: roles)
    }
}

/// A chat named in a chip, its picture beside its name in the accent: the box's "to whom".
@MainActor
public final class ChatChip: NSView {
    let avatar = AvatarView()
    let name = Labels.make("", .callout)
    public var roles: Roles? {
        didSet { needsDisplay = true }
    }

    public static let height: CGFloat = 22

    override public init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
        name.font = TextStyle.callout.emphasised
        for v in [avatar, name] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        let side = Self.height - Space.xs * 2 + 2
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: Self.height),
            avatar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs - 1),
            avatar.centerYAnchor.constraint(equalTo: centerYAnchor),
            avatar.widthAnchor.constraint(equalToConstant: side),
            avatar.heightAnchor.constraint(equalToConstant: side),
            name.leadingAnchor.constraint(equalTo: avatar.trailingAnchor, constant: Space.xs + 1),
            name.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.m),
            name.centerYAnchor.constraint(equalTo: centerYAnchor),
            widthAnchor.constraint(lessThanOrEqualToConstant: 200),
        ])
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func show(_ chat: ChatRow, image: NSImage?, roles: Roles) {
        self.roles = roles
        name.stringValue = chat.name
        name.textColor = roles.accent.nsColor
        avatar.show(chat.name, id: chat.id, image: image, roles: roles)
        toolTip = "Writing to \(chat.name)"
        setAccessibilityLabel("Writing to \(chat.name)")
        needsDisplay = true
    }

    override public var wantsUpdateLayer: Bool {
        true
    }

    override public func updateLayer() {
        guard let roles, let layer else { return }
        layer.cornerRadius = Self.height / 2
        layer.backgroundColor = roles.accent.opacity(roles.isDark ? 0.16 : 0.12).cgColor
    }
}
