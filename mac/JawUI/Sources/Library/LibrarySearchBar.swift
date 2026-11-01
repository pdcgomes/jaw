import AppKit
import CoreText
import JawKit
import JawThemes

/// The library's search line: a field for words, after the tokens taken for `from:`, `in:`, `since:`
/// and `until:`, each with an × that takes it away, on the box's field surface.
final class LibrarySearchBar: NSView {
    let surface = Surface(.field, radius: Radius.field)
    let glass = NSImageView()
    let field = NSTextField()
    var chips: [TokenChip] = []
    /// A token's × clicked, by its place.
    var removed: ((Int) -> Void)?
    var roles: Roles? {
        didSet { colour() }
    }

    var tokens: [LibraryToken] = [] {
        didSet {
            guard tokens != oldValue else { return }
            for chip in chips {
                chip.removeFromSuperview()
            }
            chips = tokens.enumerated().map { i, token in
                let chip = TokenChip(token.said)
                chip.removed = { [weak self] in self?.removed?(i) }
                chip.roles = roles
                addSubview(chip)
                return chip
            }
            needsLayout = true
        }
    }

    static let placeholder = "captions, file names, link titles · from:  in:  since:  until:"

    override init(frame: NSRect) {
        super.init(frame: frame)
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = TextStyle.body.nsFont
        field.lineBreakMode = .byTruncatingTail
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.setAccessibilityLabel("Search the library")
        glass.imageScaling = .scaleNone
        for v in [surface, glass, field] as [NSView] {
            addSubview(v)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var isFlipped: Bool {
        true
    }

    override func layout() {
        super.layout()
        let inner = bounds.insetBy(dx: Space.l, dy: Space.xxs)
        surface.frame = inner
        let mid = inner.midY
        glass.frame = CGRect(x: inner.minX + Space.m, y: (mid - 8).rounded(), width: 16, height: 16)
        var x = glass.frame.maxX + Space.s
        for chip in chips {
            let size = chip.intrinsicContentSize
            chip.frame = CGRect(x: x, y: (mid - size.height / 2).rounded(), width: min(size.width, max(inner.maxX - x - 40, 40)), height: size.height)
            x = chip.frame.maxX + Space.xs
        }
        let h = ceil(field.intrinsicContentSize.height)
        field.frame = CGRect(x: x, y: (mid - h / 2).rounded(), width: max(inner.maxX - Space.m - x, 20), height: h)
    }

    func colour() {
        guard let roles else { return }
        surface.roles = roles
        field.textColor = roles.text.nsColor
        field.placeholderAttributedString = NSAttributedString(string: Self.placeholder, attributes: [
            .font: TextStyle.body.nsFont, .foregroundColor: roles.tertiary.nsColor,
        ])
        glass.image = NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 12, weight: .medium)
                .applying(.init(hierarchicalColor: roles.tertiary.nsColor)))
        for chip in chips {
            chip.roles = roles
        }
    }
}

/// A token on the search line: what it filters by, and an × that takes it away.
final class TokenChip: NSView {
    let said: String
    var removed: (() -> Void)?
    var roles: Roles? {
        didSet { needsDisplay = true }
    }

    init(_ said: String) {
        self.said = said
        super.init(frame: .zero)
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel("\(said), remove")
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var isFlipped: Bool {
        true
    }

    static let height: CGFloat = 20
    static let cross: CGFloat = 14

    var font: CTFont {
        TextStyle.callout.nsFont as CTFont
    }

    override var intrinsicContentSize: NSSize {
        let w = LibraryDrawing.width(LibraryDrawing.line(said, font, RGBA(0)))
        return NSSize(width: ceil(w + Space.s * 2 + Self.cross + Space.xxs), height: Self.height)
    }

    override func draw(_: NSRect) {
        guard let roles, let ctx = NSGraphicsContext.current?.cgContext else { return }
        var parts: [Laid.Part] = [.fill(bounds, roles.selection.cgColor, Radius.small)]
        let room = bounds.width - Space.s * 2 - Self.cross - Space.xxs
        let l = LibraryDrawing.fit(LibraryDrawing.line(said, font, roles.text), room, font, roles.text)
        parts.append(.line(l, CGPoint(x: Space.s, y: LibraryDrawing.baseline(font, mid: bounds.midY))))
        LibraryDrawing.symbol("xmark", 8, roles.secondary, centredIn: crossRect, into: &parts)
        Laid.draw(parts, in: ctx, y: 0)
    }

    var crossRect: CGRect {
        CGRect(x: bounds.maxX - Space.s - Self.cross + 2, y: (bounds.midY - Self.cross / 2).rounded(), width: Self.cross, height: Self.cross)
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        if crossRect.insetBy(dx: -4, dy: -4).contains(p) { removed?() }
    }

    override func accessibilityPerformPress() -> Bool {
        removed?()
        return true
    }
}
