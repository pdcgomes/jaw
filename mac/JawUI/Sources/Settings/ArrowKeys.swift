import AppKit
import SwiftUI

/// The gallery's arrow keys, from an AppKit view behind the pane that takes the window's keys as the
/// pane opens and again after any click in it, so the arrows work with no click first and whatever
/// the Mac's keyboard navigation is, where a SwiftUI view would need focus it may not be given. As
/// the pane opens it takes them only from the window or a view in the pane, so a view beside the pane
/// in the same window, as the harness's scene list, keeps them until a click in the pane.
/// `moved` is told each arrow and says whether a card was chosen; every other key goes on.
struct ArrowKeys: NSViewRepresentable {
    /// Counted up by each click in the pane and each choice in it, so the keys come back to the gallery.
    let clicked: Int
    let moved: (Gallery.Arrow) -> Bool

    func makeNSView(context _: Context) -> ArrowCatcher {
        ArrowCatcher()
    }

    func updateNSView(_ view: ArrowCatcher, context _: Context) {
        view.moved = moved
        if view.clicked != clicked {
            view.clicked = clicked
            view.takeKeys()
        }
    }
}

/// An invisible view that holds its window's keys for the gallery's arrows, passing every other key,
/// and every arrow with a modifier, on up the responder chain.
final class ArrowCatcher: NSView {
    var moved: (Gallery.Arrow) -> Bool = { _ in false }
    var clicked = 0

    override var acceptsFirstResponder: Bool {
        true
    }

    /// It's there for the keys alone, so clicks go through it to the pane.
    override func hitTest(_: NSPoint) -> NSView? {
        nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        takeKeys(opening: true)
    }

    /// Becomes its window's first responder once AppKit has placed the pane: after a click in the pane,
    /// whatever had the keys; as the pane opens, only from the window itself or a view in the pane's own
    /// hosting view, the nearest above it, which in Settings is the tab's and in the harness the stage's.
    func takeKeys(opening: Bool = false) {
        DispatchQueue.main.async { [weak self] in
            guard let self, let window = self.window, window.firstResponder !== self else { return }
            if opening, let holder = window.firstResponder as? NSView,
               let host = sequence(first: self as NSView, next: { $0.superview }).first(where: { $0 is any Hosting }),
               !holder.isDescendant(of: host)
            {
                return
            }
            window.makeFirstResponder(self)
        }
    }

    override func keyDown(with event: NSEvent) {
        let arrows: [UInt16: Gallery.Arrow] = [123: .left, 124: .right, 125: .down, 126: .up]
        if event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty, let arrow = arrows[event.keyCode], moved(arrow) {
            return
        }
        super.keyDown(with: event)
    }
}

/// SwiftUI's hosting view, whatever it hosts: what the catcher looks for above it.
private protocol Hosting {}

extension NSHostingView: Hosting {}
