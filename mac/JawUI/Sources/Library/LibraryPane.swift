import CoreGraphics
import JawKit

/// A library as a pane of the layout, as the design has it, "in a pane, like a chat": a slot whose
/// chat is `library` for every chat's or `library:` and a chat's ID for that chat's, kept with the
/// layouts as a chat's slot is, and never looked up as a chat.
public enum LibraryPane {
    public static let every = "library"
    static let prefix = "library:"

    /// A slot's chat for a library of `scope`.
    public static func id(_ scope: LibraryScope) -> String {
        scope.chat.map { prefix + $0 } ?? every
    }

    public static func isLibrary(_ id: String) -> Bool {
        id == every || id.hasPrefix(prefix)
    }

    /// The scope of the library a slot shows, named as `name` names a chat, or nil for a chat's slot.
    public static func scope(of id: String, name: (String) -> String) -> LibraryScope? {
        if id == every { return .every }
        guard id.hasPrefix(prefix) else { return nil }
        let chat = String(id.dropFirst(prefix.count))
        return .chat(id: chat, name: name(chat))
    }

    /// What the layout editor and a pane's words call it: "Library", "Library · Lisbon Climbing".
    public static func name(_ id: String, chat: (String) -> String) -> String? {
        scope(of: id, name: chat).map { scope in
            switch scope {
            case .every: "Library"
            case let .chat(_, name): "Library · \(name)"
            }
        }
    }

    /// Where a library opens.
    public enum Opening: Equatable, Sendable {
        /// The pane that shows it already.
        case shown(Int)
        /// The layout with it in the slot, which takes the keys; `replacing` the chat that was there.
        case into(PaneGrid, Int, replacing: String?)
    }

    /// Where the library `id` opens, as question 1 of the library's plan has it: to the pane that shows
    /// it; beside the focused pane, across, so it sits by the chat it came from; in the focused pane
    /// when that's empty; in its place when there's no room to split; alone with no layout yet.
    public static func open(_ id: String, in grid: PaneGrid?, focused: Int?, size: CGSize) -> Opening {
        guard let grid, let at = focused.flatMap({ grid.slot($0) }) else { return .into(.single(id), 1, replacing: nil) }
        if let shown = grid.slot(showing: id) { return .shown(shown.id) }
        guard let chat = at.chat else { return .into(grid.with(chat: id, in: at.id), at.id, replacing: nil) }
        switch grid.splitting(at.id, .across, chat: id, size: size) {
        case let .success((split, new)): return .into(split, new, replacing: nil)
        case .failure: return .into(grid.with(chat: id, in: at.id), at.id, replacing: chat)
        }
    }

    /// Where the keys go as a library's slot closes: the layout without it and the slot that takes
    /// them; with no other pane, the chat it came from in its place, or an empty pane.
    public static func close(_ id: String, in grid: PaneGrid, cameFrom chat: String?) -> (PaneGrid, Int?) {
        guard let slot = grid.slot(showing: id) else { return (grid, nil) }
        if grid.slots.count > 1 { return grid.closing(slot.id) }
        let back = chat.flatMap { grid.slot(showing: $0) == nil ? $0 : nil }
        return (grid.with(chat: back, in: slot.id), slot.id)
    }
}
