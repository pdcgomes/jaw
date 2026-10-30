import JawKit

/// The terminal's panes as jawd keeps them for it, read as a grid to open in place of the unsaved
/// panes: its tree's splits as the grid's lines, its panes as slots numbered 1, 2, … in the tree's
/// order, and its pane with the keys as the slot with the focus. The terminal makes six panes at
/// most and the grid holds twelve, so a tree with more was kept by something else under its name.
public struct TerminalPanes: Equatable, Sendable {
    /// The terminal's name in hello, which jawd keeps its panes under.
    public static let interface = "jaw"

    public let grid: PaneGrid
    public let focus: Int
    /// What's said once they're open: how many, and how many more jawd left out.
    public let said: String

    /// Why the terminal's panes don't open, as it's said.
    public enum Refusal: Error, Equatable, Sendable {
        /// The terminal hasn't kept any.
        case nothing
        /// Every pane's chat is gone, this many.
        case gone(Int)
        /// More panes than the grid holds.
        case most(Int)
        /// Kept in a format this Jaw doesn't read, or a tree that makes no grid.
        case unreadable

        public var said: String {
            switch self {
            case .nothing: "the terminal hasn't kept any panes"
            case let .gone(n): n == 1 ? "the terminal's pane showed a chat that's gone" : "the terminal's \(n) panes showed chats that are gone"
            case let .most(n): "the terminal has \(n) panes, and Jaw shows \(PaneGrid.most) at most"
            case .unreadable: "the terminal's panes don't make a layout this Jaw can show"
            }
        }
    }

    /// The panes `kept` holds, or why they don't open.
    public static func read(_ kept: KeptLayout) -> Result<TerminalPanes, Refusal> {
        guard kept.v > 0 else { return .failure(.nothing) }
        guard let layout = kept.layout else { return .failure(kept.dropped.isEmpty ? .unreadable : .gone(kept.dropped.count)) }
        let tree = PaneLayout(layout.tree)
        let count = tree.panes.count
        guard count <= PaneGrid.most else { return .failure(.most(count)) }
        let grid = PaneGrid.from(tree)
        guard grid.holds else { return .failure(.unreadable) }
        let focus = min(max(layout.focused, 0), count - 1) + 1
        return .success(TerminalPanes(grid: grid, focus: focus, said: saying(count, left: kept.dropped.count)))
    }

    static func saying(_ count: Int, left: Int) -> String {
        let panes = count == 1 ? "the terminal's pane" : "the terminal's \(count) panes"
        return left == 0 ? panes : "\(panes), and \(left) more that jawd left out"
    }
}

extension PaneLayout {
    /// A tree of splits as jawd keeps it, each split's share turned from thousandths to a fraction.
    public init(_ tree: SplitTree) {
        switch tree {
        case let .chat(id):
            self = .pane(id)
        case let .split(how, first, a, b):
            self = .split(how == .across ? .across : .down, Double(first) / 1000, PaneLayout(a), PaneLayout(b))
        }
    }
}
