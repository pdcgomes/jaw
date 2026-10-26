import AppKit
import JawThemes

/// The panes drawn: nested split views following the layout, each pane's conversation in its place,
/// the focused one outlined in the accent when there's more than one.
@MainActor
public final class PaneView: NSView, NSSplitViewDelegate {
    /// The pane each split view's first side holds, for writing its new share back.
    var splits: [ObjectIdentifier: (String, PaneLayout.Axis)] = [:]
    public var resized: ((String, PaneLayout.Axis, Double) -> Void)?
    var layout: PaneLayout?

    /// Lays the panes out afresh; the conversations' views move into their places.
    public func show(_ layout: PaneLayout, views: [String: NSView], focused: String?, roles: Roles) {
        self.layout = layout
        for sub in subviews {
            sub.removeFromSuperview()
        }
        splits.removeAll()
        let many = layout.panes.count > 1
        let root = build(layout, views: views, focused: many ? focused : nil, roles: roles)
        root.translatesAutoresizingMaskIntoConstraints = false
        addSubview(root)
        NSLayoutConstraint.activate([
            root.topAnchor.constraint(equalTo: topAnchor),
            root.bottomAnchor.constraint(equalTo: bottomAnchor),
            root.leadingAnchor.constraint(equalTo: leadingAnchor),
            root.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])
        layoutSubtreeIfNeeded()
        place(layout, in: root)
    }

    func build(_ node: PaneLayout, views: [String: NSView], focused: String?, roles: Roles) -> NSView {
        switch node {
        case let .pane(id):
            let frame = PaneFrame()
            if let content = views[id] {
                frame.hold(content)
            }
            frame.outline(id == focused ? roles.accent : nil)
            return frame
        case let .split(axis, _, a, b):
            let split = ThemedSplitView()
            split.divider = roles.separator.nsColor
            split.isVertical = axis == .across
            split.dividerStyle = .thin
            split.delegate = self
            split.addArrangedSubview(build(a, views: views, focused: focused, roles: roles))
            split.addArrangedSubview(build(b, views: views, focused: focused, roles: roles))
            splits[ObjectIdentifier(split)] = (a.panes.last ?? "", axis)
            return split
        }
    }

    /// Puts each divider at its split's share once the views have their sizes.
    func place(_ node: PaneLayout, in view: NSView) {
        guard case let .split(axis, share, a, b) = node, let split = view as? NSSplitView, split.arrangedSubviews.count == 2 else { return }
        let length = axis == .across ? split.bounds.width : split.bounds.height
        if length > 0 { split.setPosition(length * share, ofDividerAt: 0) }
        split.layoutSubtreeIfNeeded()
        place(a, in: split.arrangedSubviews[0])
        place(b, in: split.arrangedSubviews[1])
    }

    public func splitViewDidResizeSubviews(_ notification: Notification) {
        guard let split = notification.object as? NSSplitView, let (pane, axis) = splits[ObjectIdentifier(split)],
              split.arrangedSubviews.count == 2, split.inLiveResize || NSEvent.pressedMouseButtons != 0
        else { return }
        let first = split.arrangedSubviews[0].frame
        let length = axis == .across ? split.bounds.width : split.bounds.height
        guard length > 0 else { return }
        resized?(pane, axis, Double((axis == .across ? first.width : first.height) / length))
    }

    public func splitView(_: NSSplitView, constrainMinCoordinate proposed: CGFloat, ofSubviewAt _: Int) -> CGFloat {
        max(proposed, 240)
    }

    public func splitView(_ split: NSSplitView, constrainMaxCoordinate proposed: CGFloat, ofSubviewAt _: Int) -> CGFloat {
        min(proposed, (split.isVertical ? split.bounds.width : split.bounds.height) - 240)
    }
}

/// A split view whose dividers take the theme's separator colour.
final class ThemedSplitView: NSSplitView {
    var divider: NSColor = .separatorColor

    override var dividerColor: NSColor {
        divider
    }
}

/// One pane's place: its conversation, and the focused pane's thin outline in the accent.
final class PaneFrame: NSView {
    let border = CALayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func hold(_ content: NSView) {
        content.removeFromSuperview()
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.topAnchor.constraint(equalTo: topAnchor),
            content.bottomAnchor.constraint(equalTo: bottomAnchor),
            content.leadingAnchor.constraint(equalTo: leadingAnchor),
            content.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])
    }

    func outline(_ colour: RGBA?) {
        border.removeFromSuperlayer()
        guard let colour else { return }
        border.borderColor = colour.cgColor
        border.borderWidth = 1.5
        border.zPosition = 10
        layer?.addSublayer(border)
        needsLayout = true
    }

    override func layout() {
        super.layout()
        border.frame = bounds
    }
}
