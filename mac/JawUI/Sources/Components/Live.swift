import AppKit
import QuartzCore

/// A view whose animation shows a live state, as someone typing. It plays while whoever shows it
/// allows it (its pane is the one you're reading, and it's on screen), in a key window on screen,
/// with motion allowed, and shows its still form otherwise, which is also what it draws for a
/// snapshot. Its animations run on the render server, so the main thread does nothing while it
/// plays, and keep time from `since`, so being taken out of the window and put back, as a table's
/// reload does, doesn't start them again.
class LiveStateView: NSView {
    var allowed = false {
        didSet { update() }
    }

    var since = CACurrentMediaTime() {
        didSet { if playing { play() } }
    }

    private(set) var playing = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
        NSWorkspace.shared.notificationCenter.addObserver(self, selector: #selector(windowChanged),
                                                          name: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification,
                                                          object: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var wantsUpdateLayer: Bool {
        false
    }

    static let windowChanges = [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification,
                                NSWindow.didChangeOcclusionStateNotification]

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        guard let window else { return }
        for name in Self.windowChanges {
            NotificationCenter.default.removeObserver(self, name: name, object: window)
        }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let window {
            for name in Self.windowChanges {
                NotificationCenter.default.addObserver(self, selector: #selector(windowChanged), name: name, object: window)
            }
        }
        update()
    }

    @objc func windowChanged(_: Notification) {
        update()
    }

    /// Whether it plays: allowed, in a window that's key and on screen, with motion not reduced.
    static func plays(allowed: Bool, key: Bool, visible: Bool, reduced: Bool) -> Bool {
        allowed && key && visible && !reduced
    }

    func update() {
        let should = Self.plays(allowed: allowed, key: window?.isKeyWindow ?? false,
                                visible: window?.occlusionState.contains(.visible) ?? false, reduced: Motion.reduced())
        guard should != playing else { return }
        if should { play() } else { stop() }
    }

    /// A subclass puts its animations on its layers, then calls this.
    func play() {
        show(playing: true)
    }

    /// A subclass takes its animations off, then calls this.
    func stop() {
        show(playing: false)
    }

    /// What's drawn changes with the layers, in the same pass.
    func show(playing now: Bool) {
        playing = now
        needsDisplay = true
        if window != nil { displayIfNeeded() }
    }

    /// When an animation `after` seconds into the loop begins, in a layer's time.
    func begin(_ layer: CALayer, after offset: CFTimeInterval = 0) -> CFTimeInterval {
        layer.convertTime(since, from: nil) + offset
    }
}
