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
    /// How near a gutter the pointer takes it, either side.
    static let grip: CGFloat = 4

    override public var isFlipped: Bool {
        true
    }

    override public init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Shows `grid`, each slot holding its chat's view, or the empty pane.
    public func show(_ grid: PaneGrid, views: [Int: NSView], focused: Int?, roles: Roles) {
        self.grid = grid
        self.focused = focused
        self.roles = roles
        layer?.backgroundColor = roles.separator.cgColor
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
            frame.show(views[slot.id], roles: roles, focused: many && slot.id == focused, closable: many)
        }
        for (id, frame) in frames where grid.slot(id) == nil {
            frame.removeFromSuperview()
            frames[id] = nil
        }
        needsLayout = true
        window?.invalidateCursorRects(for: self)
    }

    override public func layout() {
        super.layout()
        guard let grid else { return }
        let half = Stroke.hairline / 2
        for (id, rect) in grid.frames(in: bounds) {
            frames[id]?.frame = rect.insetBy(dx: half, dy: half).integral
        }
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
        addSubview(closeButton)
        NSLayoutConstraint.activate([
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.l),
            closeButton.centerYAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor, constant: Space.xxl),
        ])
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ view: NSView?, roles: Roles, focused: Bool, closable: Bool) {
        let shown = view ?? empty
        if content !== shown {
            content?.removeFromSuperview()
            shown.removeFromSuperview()
            shown.translatesAutoresizingMaskIntoConstraints = false
            addSubview(shown, positioned: .below, relativeTo: closeButton)
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
        layer?.borderColor = focused ? roles.accent.cgColor : nil
        layer?.borderWidth = focused ? Stroke.focus : 0
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
            well.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor, constant: Space.l),
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
