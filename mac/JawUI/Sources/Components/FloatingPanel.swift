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
        /// Above a rect on the screen, its right edges lined up: a picker over its button.
        case above(NSRect)
    }

    /// A key, before the panel's views see it; true when it was taken.
    public var keys: ((NSEvent) -> Bool)?
    /// Said once the panel has closed.
    public var closed: (() -> Void)?
    /// Where its size is kept when it can be resized, so it opens at the size it was left.
    let keeps: String?

    /// A panel; `resizable` by its edges, between `least` and the window's size, its size kept in the
    /// preferences under `keeps`.
    public init(resizable: Bool = false, least: NSSize = NSSize(width: 360, height: 240), keeps: String? = nil) {
        self.keeps = keeps
        super.init(contentRect: .zero, styleMask: resizable ? [.borderless, .nonactivatingPanel, .resizable] : [.borderless, .nonactivatingPanel],
                   backing: .buffered, defer: true)
        minSize = least
        if keeps != nil {
            NotificationCenter.default.addObserver(forName: NSWindow.didEndLiveResizeNotification, object: self, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.keepSize() }
            }
        }
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

    /// The size it was left at, if it's kept.
    public var keptSize: NSSize? {
        guard let keeps, let s = UserDefaults.standard.string(forKey: keeps) else { return nil }
        let size = NSSizeFromString(s)
        return size.width > 0 && size.height > 0 ? size : nil
    }

    func keepSize() {
        guard let keeps else { return }
        UserDefaults.standard.set(NSStringFromSize(frame.size), forKey: keeps)
    }

    /// Shows `content` over `window`, at the size it was left at when it's kept, otherwise `size`;
    /// never bigger than the window.
    public func show(_ content: NSView, size: NSSize, over window: NSWindow, at place: Place = .centre) {
        var size = keptSize ?? size
        size.width = min(max(size.width, minSize.width), window.frame.width - Space.xxl)
        size.height = min(max(size.height, minSize.height), window.frame.height - Space.xxl * 2)
        maxSize = NSSize(width: window.frame.width - Space.xxl, height: window.frame.height - Space.xxl * 2)
        content.frame = NSRect(origin: .zero, size: size)
        content.autoresizingMask = [.width, .height]
        contentView = content
        let f = window.frame
        var origin: NSPoint
        switch place {
        case .centre: origin = NSPoint(x: f.midX - size.width / 2, y: f.midY - size.height / 2)
        case .top: origin = NSPoint(x: f.midX - size.width / 2, y: f.maxY - 60 - size.height)
        case let .above(r): origin = NSPoint(x: min(max(r.maxX - size.width, f.minX + Space.s), f.maxX - size.width - Space.s), y: r.maxY + Space.s)
        }
        setFrame(NSRect(x: origin.x.rounded(), y: origin.y.rounded(), width: size.width, height: size.height), display: true)
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
