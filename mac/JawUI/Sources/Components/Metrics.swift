import AppKit
import QuartzCore

/// Spacing, on a 2-point step.
public enum Space {
    public static let xxs: CGFloat = 2
    public static let xs: CGFloat = 4
    public static let s: CGFloat = 6
    public static let m: CGFloat = 8
    public static let l: CGFloat = 12
    public static let xl: CGFloat = 16
    public static let xxl: CGFloat = 24
}

/// Corner radii, by what's rounded.
public enum Radius {
    /// Chips, cells of a grid.
    public static let small: CGFloat = 4
    /// Buttons, rows, popovers' items.
    public static let control: CGFloat = 8
    /// The box, fields.
    public static let field: CGFloat = 10
    /// Panels: the palette, the glance.
    public static let panel: CGFloat = 14
}

/// Line widths.
public enum Stroke {
    /// Separators, borders at rest.
    public static let hairline: CGFloat = 1
    /// The focused pane's outline.
    public static let focus: CGFloat = 1.5
}

/// How long things move: 120 ms for a control, 180 ms for a pane or a panel; nothing moves under
/// Reduce Motion, and nothing follows a live resize.
@MainActor
public enum Motion {
    public static let quick: TimeInterval = 0.12
    public static let standard: TimeInterval = 0.18

    /// Whether to move at all, as the system's Reduce Motion says; tests set it.
    public static var reduced: () -> Bool = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }

    /// Runs `changes` animated over `duration`, or at once when motion is reduced.
    public static func run(_ duration: TimeInterval = standard, _ changes: @escaping @MainActor () -> Void) {
        guard !reduced() else { return changes() }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = duration
            context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            context.allowsImplicitAnimation = true
            changes()
        }
    }
}
