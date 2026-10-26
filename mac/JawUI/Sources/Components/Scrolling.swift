import AppKit

/// A thin scroller that shows only while scrolling and fades away, as macOS's overlay scrollers do,
/// even when the system is set to show them always: a rounded knob in the text's colour, faint, with
/// no track, wider while the pointer is over it.
@MainActor
public final class ThinScroller: NSScroller {
    override public class var isCompatibleWithOverlayScrollers: Bool {
        true
    }

    override public func drawKnobSlot(in _: NSRect, highlight _: Bool) {}

    override public func drawKnob() {
        let slot = rect(for: .knob)
        guard slot.height > 0, slot.width > 0 else { return }
        let thin = controlSize == .small || knobStyle == .default
        let width: CGFloat = thin ? 5 : 7
        let r = NSRect(x: slot.maxX - width - 2, y: slot.minY + 2, width: width, height: max(slot.height - 4, width))
        NSColor.labelColor.withAlphaComponent(0.28).setFill()
        NSBezierPath(roundedRect: r, xRadius: width / 2, yRadius: width / 2).fill()
    }
}

/// Every scroll view's scrollers, made thin and overlaid, kept so when the system's setting changes.
@MainActor
public enum Scrolling {
    static let views = NSHashTable<NSScrollView>.weakObjects()
    static var watching = false

    public static func style(_ scroll: NSScrollView) {
        scroll.verticalScroller = ThinScroller()
        scroll.horizontalScroller = ThinScroller()
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        views.add(scroll)
        guard !watching else { return }
        watching = true
        NotificationCenter.default.addObserver(forName: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated {
                for v in views.allObjects {
                    v.scrollerStyle = .overlay
                }
            }
        }
    }
}
