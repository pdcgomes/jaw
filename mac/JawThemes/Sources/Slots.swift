import Foundation

/// The theme in each slot, by ID: the one Jaw draws while the Mac is dark, and the one while it's
/// light. Both hold Jaw's Tokyo Night at first.
public struct Slots: Sendable, Hashable {
    public var dark: String
    public var light: String

    public init(dark: String = Catalog.jaw.id, light: String = Catalog.jaw.id) {
        self.dark = dark
        self.light = light
    }

    public subscript(_ slot: Slot) -> String {
        get { slot.isDark ? dark : light }
        set {
            if slot.isDark { dark = newValue } else { light = newValue }
        }
    }

    /// The slots once `theme` is chosen for `slot`: one with both halves fills both, each with its own
    /// half, unless `only`, as ⌥-click is, which fills `slot` alone; one with a single half fills the
    /// slot of its mode, so a slot never holds a theme of the other mode.
    public func choosing(_ theme: Theme, for slot: Slot, only: Bool = false) -> Slots {
        var next = self
        switch (theme.dark != nil, theme.light != nil) {
        case (true, true) where only:
            next[slot] = theme.id
        case (true, true):
            next.dark = theme.id
            next.light = theme.id
        case (true, false):
            next.dark = theme.id
        case (false, true):
            next.light = theme.id
        case (false, false):
            break
        }
        return next
    }
}
