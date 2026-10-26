import AppKit

/// A panel over the window that takes the keys, closes on Esc or a click elsewhere, and stays
/// with the window: the glance, centred in it. Keys it's given first go to `keys` before any view,
/// so a list's arrows work whatever has the focus inside.
@MainActor
public final class FloatingPanel: NSPanel {
    public enum Place: Sendable {
        /// In the middle of the window.
        case centre
        /// Near the top of the window, as the palette sits.
        case top
    }

    /// A key, before the panel's views see it; true when it was taken.
    public var keys: ((NSEvent) -> Bool)?
    /// Said once the panel has closed.
    public var closed: (() -> Void)?

    public init() {
        super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        isFloatingPanel = true
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true
        hidesOnDeactivate = true
        isReleasedWhenClosed = false
    }

    override public var canBecomeKey: Bool {
        true
    }

    /// Shows `content` at `size` over `window`.
    public func show(_ content: NSView, size: NSSize, over window: NSWindow, at place: Place = .centre) {
        content.frame = NSRect(origin: .zero, size: size)
        contentView = content
        let f = window.frame
        let y = place == .centre ? f.midY - size.height / 2 : f.maxY - 60 - size.height
        setFrame(NSRect(x: (f.midX - size.width / 2).rounded(), y: y.rounded(), width: size.width, height: size.height), display: true)
        if parent !== window { window.addChildWindow(self, ordered: .above) }
        makeKeyAndOrderFront(nil)
    }

    override public func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, keys?(event) == true { return }
        super.sendEvent(event)
    }

    override public func resignKey() {
        super.resignKey()
        close()
    }

    override public func close() {
        let wasOpen = isVisible
        parent?.removeChildWindow(self)
        super.close()
        if wasOpen { closed?() }
    }

    override public func cancelOperation(_: Any?) {
        close()
    }
}
