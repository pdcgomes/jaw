import CoreGraphics
import Foundation

/// The panes' layout, as the terminal's `panes.rs` keeps it: a pane, by an id, or a split of two
/// layouts with the first side's share. A value, so its rules are tested on their own, and kept in
/// the app's preferences by the chats the panes show until jawd keeps an interface's state (P1).
public indirect enum PaneLayout: Hashable, Sendable, Codable {
    case pane(String)
    case split(Axis, Double, PaneLayout, PaneLayout)

    public enum Axis: String, Hashable, Sendable, Codable {
        /// Side by side.
        case across
        /// One above the other.
        case down
    }

    public enum Direction: Sendable {
        case left, right, up, down
    }

    /// The most panes: each is a messages window, and jawd allows 32 a connection.
    public static let most = 12
    /// The least share a split's side keeps, and the most.
    public static let shares: ClosedRange<Double> = 0.2 ... 0.8

    /// The panes, in reading order.
    public var panes: [String] {
        switch self {
        case let .pane(id): [id]
        case let .split(_, _, a, b): a.panes + b.panes
        }
    }

    public func contains(_ id: String) -> Bool {
        panes.contains(id)
    }

    /// Splits pane `id` along `axis`, the new pane after it.
    public func splitting(_ id: String, _ axis: Axis, with new: String) -> PaneLayout {
        switch self {
        case let .pane(p) where p == id: .split(axis, 0.5, .pane(p), .pane(new))
        case .pane: self
        case let .split(ax, share, a, b): .split(ax, share, a.splitting(id, axis, with: new), b.splitting(id, axis, with: new))
        }
    }

    /// Without pane `id`, its sibling taking its place; nil when it was the only pane.
    public func removing(_ id: String) -> PaneLayout? {
        switch self {
        case let .pane(p): p == id ? nil : self
        case let .split(ax, share, a, b):
            switch (a.removing(id), b.removing(id)) {
            case let (a?, b?): .split(ax, share, a, b)
            case let (a?, nil): a
            case let (nil, b?): b
            case (nil, nil): nil
            }
        }
    }

    /// With panes `x` and `y` changing places.
    public func swapping(_ x: String, _ y: String) -> PaneLayout {
        switch self {
        case let .pane(p): .pane(p == x ? y : p == y ? x : p)
        case let .split(ax, share, a, b): .split(ax, share, a.swapping(x, y), b.swapping(x, y))
        }
    }

    /// With pane `old` showing as `new`, in its place.
    public func replacing(_ old: String, with new: String) -> PaneLayout {
        switch self {
        case let .pane(p): .pane(p == old ? new : p)
        case let .split(ax, share, a, b): .split(ax, share, a.replacing(old, with: new), b.replacing(old, with: new))
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

    /// The pane next to `id` that way, the nearest whose middle lies beyond its edge, as tmux finds
    /// it. Frames are in a top-down space.
    public func neighbour(of id: String, _ direction: Direction, in rect: CGRect = CGRect(x: 0, y: 0, width: 1000, height: 1000)) -> String? {
        let all = frames(in: rect)
        guard let from = all[id] else { return nil }
        let beyond: (CGRect) -> Bool = switch direction {
        case .left: { $0.maxX <= from.minX + 0.5 }
        case .right: { $0.minX >= from.maxX - 0.5 }
        case .up: { $0.maxY <= from.minY + 0.5 }
        case .down: { $0.minY >= from.maxY - 0.5 }
        }
        let overlaps: (CGRect) -> Bool = switch direction {
        case .left, .right: { $0.minY < from.maxY && $0.maxY > from.minY }
        case .up, .down: { $0.minX < from.maxX && $0.maxX > from.minX }
        }
        let distance = { (r: CGRect) in hypot(r.midX - from.midX, r.midY - from.midY) }
        return all.filter { $0.key != id && beyond($0.value) && overlaps($0.value) }.min { distance($0.value) < distance($1.value) }?.key
    }

    /// With the split nearest above pane `id` along `axis` given a new share.
    public func resizing(_ id: String, _ axis: Axis, to share: Double) -> PaneLayout {
        switch self {
        case .pane: self
        case let .split(ax, s, a, b):
            if ax == axis, a.contains(id) || b.contains(id), !(a.hasSplit(axis, holding: id) || b.hasSplit(axis, holding: id)) {
                .split(ax, min(max(share, Self.shares.lowerBound), Self.shares.upperBound), a, b)
            } else {
                .split(ax, s, a.resizing(id, axis, to: share), b.resizing(id, axis, to: share))
            }
        }
    }

    func hasSplit(_ axis: Axis, holding id: String) -> Bool {
        switch self {
        case .pane: false
        case let .split(ax, _, a, b): (ax == axis && (a.contains(id) || b.contains(id))) || a.hasSplit(axis, holding: id) || b.hasSplit(axis, holding: id)
        }
    }
}
