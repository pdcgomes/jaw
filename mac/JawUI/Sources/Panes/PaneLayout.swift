import CoreGraphics
import Foundation

/// The tree of splits mac-m1 kept its panes as before the grid, as the terminal's `panes.rs` keeps
/// them: read once from the preferences and as the terminal's layout, then drawn as a grid
/// (`PaneGrid.from`).
public indirect enum PaneLayout: Hashable, Sendable, Codable {
    case pane(String)
    case split(Axis, Double, PaneLayout, PaneLayout)

    public enum Axis: String, Hashable, Sendable, Codable {
        /// Side by side.
        case across
        /// One above the other.
        case down
    }

    /// The panes, in reading order.
    public var panes: [String] {
        switch self {
        case let .pane(id): [id]
        case let .split(_, _, a, b): a.panes + b.panes
        }
    }

    /// Each pane's frame in `rect`, splits dividing it by their shares.
    public func frames(in rect: CGRect) -> [String: CGRect] {
        switch self {
        case let .pane(id):
            return [id: rect]
        case let .split(axis, share, a, b):
            let (ra, rb) = axis == .across
                ? rect.divided(atDistance: rect.width * share, from: .minXEdge)
                : rect.divided(atDistance: rect.height * share, from: .minYEdge)
            return a.frames(in: ra).merging(b.frames(in: rb)) { x, _ in x }
        }
    }
}
