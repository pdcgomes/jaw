import AppKit
import JawThemes

/// The panes drawn: each slot of the grid in its place, set by hand in `layout()` so the panes
/// follow a live resize in the same frame, with a hairline gutter between them that's dragged as a
/// whole line. The focused pane has the accent's outline when there's more than one; an empty pane
/// waits for a chat. Slots keep their views from one change to the next, so nothing is rebuilt.
@MainActor
public final class GridView: NSView {
    public private(set) var grid: PaneGrid?
    var frames: [Int: SlotFrame] = [:]
    var focused: Int?
    var roles: Roles?
    /// A line dragged to a position (0 to 1), and whether the drag is over.
    public var moved: ((PaneGrid.Line, Double, Bool) -> Void)?
    /// A pane's close button.
    public var close: ((Int) -> Void)?
    /// A pane clicked, or an empty pane's prompt.
    public var pick: ((Int) -> Void)?
    /// The pane commands for a pane's context menu.
    public var paneMenu: ((Int) -> NSMenu?)?
    var dragging: PaneGrid.Line?
    /// A chat dropped on a pane: on its middle to put it there, on an edge to split that way.
    public var dropped: ((String, Int, DropZone) -> Void)?
    let dropHint = NSView()

    /// Where a dropped chat goes in a pane.
    public enum DropZone: Equatable, Sendable {
        case middle, left, right, top, bottom
    }

    /// The zone of `point` in a pane's `rect`: a band along each edge, a quarter of the pane deep
    /// and at most 120 points, and the middle; in a top-down space.
    public nonisolated static func zone(_ point: CGPoint, in rect: CGRect) -> DropZone? {
        guard rect.contains(point) else { return nil }
        let dx = min(rect.width / 4, 120), dy = min(rect.height / 4, 120)
        let edges: [(DropZone, CGFloat)] = [
            (.left, point.x - rect.minX), (.right, rect.maxX - point.x), (.top, point.y - rect.minY), (.bottom, rect.maxY - point.y),
        ]
        let near = edges.filter { zone, d in d < ((zone == .left || zone == .right) ? dx : dy) }
        return near.min { $0.1 < $1.1 }?.0 ?? .middle
    }

    /// The part of a pane a zone shows as it's dragged over.
    nonisolated static func hint(_ zone: DropZone, in r: CGRect) -> CGRect {
        switch zone {
        case .middle: r
        case .left: CGRect(x: r.minX, y: r.minY, width: r.width / 2, height: r.height)
        case .right: CGRect(x: r.midX, y: r.minY, width: r.width / 2, height: r.height)
        case .top: CGRect(x: r.minX, y: r.minY, width: r.width, height: r.height / 2)
        case .bottom: CGRect(x: r.minX, y: r.midY, width: r.width, height: r.height / 2)
        }
    }
    /// How near a gutter the pointer takes it, either side.
    static let grip: CGFloat = 4

    override public var isFlipped: Bool {
        true
    }

    override public init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        registerForDraggedTypes([.jawChat])
        dropHint.wantsLayer = true
        dropHint.layer?.cornerRadius = Radius.control
        dropHint.layer?.borderWidth = Stroke.focus
        dropHint.isHidden = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Shows `grid`, each slot holding its chat's view, or the empty pane, the focused one as
    /// `look` says.
    public func show(_ grid: PaneGrid, views: [Int: NSView], focused: Int?, roles: Roles, look: PaneLook = PaneLook()) {
        self.grid = grid
        self.focused = focused
        self.roles = roles
        layer?.backgroundColor = roles.separator.mixed(with: roles.ground, 0.55).cgColor
        let many = grid.slots.count > 1
        for slot in grid.slots {
            let frame = frames[slot.id] ?? {
                let f = SlotFrame(id: slot.id)
                f.closing = { [weak self] in self?.close?(slot.id) }
                f.picked = { [weak self] in self?.pick?(slot.id) }
                f.wantsMenu = { [weak self] in self?.paneMenu?(slot.id) }
                addSubview(f)
                frames[slot.id] = f
                return f
            }()
            frame.show(views[slot.id], roles: roles, focused: slot.id == focused, many: many, look: look)
        }
        for (id, frame) in frames where grid.slot(id) == nil {
            frame.removeFromSuperview()
            frames[id] = nil
        }
        dropHint.layer?.backgroundColor = roles.accent.opacity(0.15).cgColor
        dropHint.layer?.borderColor = roles.accent.cgColor
        if dropHint.superview == nil { addSubview(dropHint) }
        needsLayout = true
        window?.invalidateCursorRects(for: self)
    }

    // MARK: Chats dropped

    func target(_ info: any NSDraggingInfo) -> (Int, DropZone, CGRect)? {
        guard let grid else { return nil }
        let p = convert(info.draggingLocation, from: nil)
        for (id, rect) in grid.frames(in: bounds) {
            if let zone = Self.zone(p, in: rect) { return (id, zone, rect) }
        }
        return nil
    }

    override public func draggingEntered(_ info: any NSDraggingInfo) -> NSDragOperation {
        draggingUpdated(info)
    }

    override public func draggingUpdated(_ info: any NSDraggingInfo) -> NSDragOperation {
        guard let (_, zone, rect) = target(info) else {
            dropHint.isHidden = true
            return []
        }
        dropHint.frame = Self.hint(zone, in: rect).insetBy(dx: Space.xs, dy: Space.xs)
        addSubview(dropHint, positioned: .above, relativeTo: nil)
        dropHint.isHidden = false
        return .move
    }

    override public func draggingExited(_: (any NSDraggingInfo)?) {
        dropHint.isHidden = true
    }

    override public func performDragOperation(_ info: any NSDraggingInfo) -> Bool {
        dropHint.isHidden = true
        guard let (id, zone, _) = target(info), let chat = info.draggingPasteboard.string(forType: .jawChat) else { return false }
        dropped?(chat, id, zone)
        return true
    }

    override public func layout() {
        super.layout()
        guard let grid else { return }
        for (id, rect) in grid.frames(in: bounds) {
            frames[id]?.frame = Self.place(rect, in: bounds)
        }
    }

    /// A pane's frame: on whole points, a hairline short of each edge it shares with another pane,
    /// so the gutter shows the grid's background, the theme's separator.
    nonisolated static func place(_ rect: CGRect, in bounds: CGRect) -> CGRect {
        var r = CGRect(x: rect.minX.rounded(), y: rect.minY.rounded(), width: 0, height: 0)
        r.size.width = rect.maxX.rounded() - r.minX
        r.size.height = rect.maxY.rounded() - r.minY
        if r.maxX < bounds.maxX - 0.5 { r.size.width -= Stroke.hairline }
        if r.maxY < bounds.maxY - 0.5 { r.size.height -= Stroke.hairline }
        return r
    }

    override public func resetCursorRects() {
        guard let grid else { return }
        for segment in grid.lines(in: bounds) {
            let r = segment.rect.insetBy(dx: -Self.grip, dy: -Self.grip)
            addCursorRect(r, cursor: segment.line.axis == .across ? .resizeLeftRight : .resizeUpDown)
        }
    }

    /// The gutter under a point, if it's within the grip.
    func line(at point: NSPoint) -> PaneGrid.Line? {
        grid?.lines(in: bounds).first { $0.rect.insetBy(dx: -Self.grip, dy: -Self.grip).contains(point) }?.line
    }

    override public func hitTest(_ point: NSPoint) -> NSView? {
        let local = superview.map { convert(point, from: $0) } ?? point
        if line(at: local) != nil { return self }
        return super.hitTest(point)
    }

    override public func mouseDown(with event: NSEvent) {
        dragging = line(at: convert(event.locationInWindow, from: nil))
    }

    override public func mouseDragged(with event: NSEvent) {
        drag(event, done: false)
    }

    override public func mouseUp(with event: NSEvent) {
        drag(event, done: true)
        dragging = nil
    }

    func drag(_ event: NSEvent, done: Bool) {
        guard let line = dragging, bounds.width > 0, bounds.height > 0 else { return }
        let p = convert(event.locationInWindow, from: nil)
        let position = line.axis == .across ? p.x / bounds.width : p.y / bounds.height
        moved?(line, Double(position), done)
        if let grid = grid?.moving(line, to: Double(position), size: bounds.size) {
            self.grid = grid
            needsLayout = true
            layoutSubtreeIfNeeded()
            if done { window?.invalidateCursorRects(for: self) }
        }
    }
}

/// One slot's place: its chat's view or the empty pane, the focused outline as its own layer's
/// border (resized with it in the same frame), and a close button over the header while the
/// pointer is on it.
@MainActor
final class SlotFrame: NSView {
    let id: Int
    var content: NSView?
    let empty = EmptyPane()
    let shade = Shade()
    lazy var closeButton = IconButton(symbol: "xmark", label: "Close Pane", shortcut: Shortcut("w"), size: .small) { [weak self] in
        self?.closing?()
    }

    var closing: (() -> Void)?
    var picked: (() -> Void)?
    var wantsMenu: (() -> NSMenu?)?
    var closable = false

    init(id: Int) {
        self.id = id
        super.init(frame: .zero)
        wantsLayer = true
        empty.picked = { [weak self] in self?.picked?() }
        closeButton.isHidden = true
        shade.translatesAutoresizingMaskIntoConstraints = false
        addSubview(shade)
        addSubview(closeButton)
        NSLayoutConstraint.activate([
            shade.topAnchor.constraint(equalTo: topAnchor),
            shade.bottomAnchor.constraint(equalTo: bottomAnchor),
            shade.leadingAnchor.constraint(equalTo: leadingAnchor),
            shade.trailingAnchor.constraint(equalTo: trailingAnchor),
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.l),
            closeButton.centerYAnchor.constraint(equalTo: topAnchor, constant: ConversationHeader.height / 2),
        ])
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ view: NSView?, roles: Roles, focused: Bool, many: Bool, look: PaneLook) {
        let closable = many
        let shown = view ?? empty
        if content !== shown || shown.superview !== self {
            // A view another pane took in this same pass is that pane's now.
            if let content, content.superview === self { content.removeFromSuperview() }
            shown.removeFromSuperview()
            shown.translatesAutoresizingMaskIntoConstraints = false
            addSubview(shown, positioned: .below, relativeTo: shade)
            NSLayoutConstraint.activate([
                shown.topAnchor.constraint(equalTo: topAnchor),
                shown.bottomAnchor.constraint(equalTo: bottomAnchor),
                shown.leadingAnchor.constraint(equalTo: leadingAnchor),
                shown.trailingAnchor.constraint(equalTo: trailingAnchor),
            ])
            content = shown
        }
        empty.roles = roles
        closeButton.roles = roles
        self.closable = closable
        if !closable { closeButton.isHidden = true }
        shade.show(roles: roles, focused: focused, many: many, look: look)
    }

    override func mouseEntered(with _: NSEvent) {
        closeButton.isHidden = !closable
    }

    override func mouseExited(with _: NSEvent) {
        closeButton.isHidden = true
    }

    override func menu(for _: NSEvent) -> NSMenu? {
        wantsMenu?()
    }
}

/// A pane with no chat: it says how to give it one, and clicking it gives it the keys.
@MainActor
final class EmptyPane: NSView {
    let well = Surface(.well, radius: Radius.field)
    let prompt = Labels.make("⌘K or drag a chat here", .callout)
    var picked: (() -> Void)?
    var roles: Roles? {
        didSet { colour() }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        for v in [well, prompt] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        NSLayoutConstraint.activate([
            well.topAnchor.constraint(equalTo: topAnchor, constant: Space.l),
            well.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.l),
            well.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.l),
            well.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Space.l),
            prompt.centerXAnchor.constraint(equalTo: well.centerXAnchor),
            prompt.centerYAnchor.constraint(equalTo: well.centerYAnchor),
        ])
        setAccessibilityRole(.group)
        setAccessibilityLabel("An empty pane: ⌘K or drag a chat here")
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func colour() {
        guard let roles else { return }
        layer?.backgroundColor = roles.ground.cgColor
        well.roles = roles
        Labels.style(prompt, .tertiary, roles: roles)
    }

    override func mouseDown(with _: NSEvent) {
        picked?()
    }
}

/// How the panes say which has the keys, as iTerm's settings have it: the others dimmed, and the
/// focused one outlined inside its edge, both optional.
public struct PaneLook: Hashable, Sendable {
    /// How much the panes without the keys are dimmed, from 0 to 0.6.
    public var dim: Double
    /// The outline's colour, or nil for none.
    public var outline: RGBA?

    public init(dim: Double = 0.18, outline: RGBA? = nil) {
        self.dim = dim
        self.outline = outline
    }
}

/// Over a pane and letting the pointer through: the dimming of a pane without the keys, and the
/// focused pane's outline, drawn inside it with rounded corners.
@MainActor
final class Shade: NSView {
    var roles: Roles?
    var focused = false
    var many = false
    var look = PaneLook()

    func show(roles: Roles, focused: Bool, many: Bool, look: PaneLook) {
        guard roles != self.roles || focused != self.focused || many != self.many || look != self.look else { return }
        self.roles = roles
        self.focused = focused
        self.many = many
        self.look = look
        needsDisplay = true
    }

    override func hitTest(_: NSPoint) -> NSView? {
        nil
    }

    override func draw(_: NSRect) {
        guard let roles, many else { return }
        if !focused, look.dim > 0 {
            roles.ground.opacity(look.dim).nsColor.setFill()
            bounds.fill()
        }
        if focused, let outline = look.outline {
            let r = bounds.insetBy(dx: Space.xs, dy: Space.xs)
            let path = NSBezierPath(roundedRect: r, xRadius: Radius.field, yRadius: Radius.field)
            path.lineWidth = Stroke.hairline
            outline.opacity(0.7).nsColor.setStroke()
            path.stroke()
        }
    }
}
