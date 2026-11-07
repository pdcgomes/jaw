import AppKit

/// Settings' panes, in the toolbar's order: each its name, the SF Symbol its tab shows over it, and
/// its size, 500 points wide as Redlamp's are and as tall as its form, which the window takes as its
/// tab is chosen.
public enum SettingsPane: String, CaseIterable, Sendable {
    case appearance, messages, panes

    public var title: String {
        switch self {
        case .appearance: "Appearance"
        case .messages: "Messages"
        case .panes: "Panes"
        }
    }

    /// Appearance's is Redlamp's; Panes' is Split Across's, in the Panes menu.
    public var symbol: String {
        switch self {
        case .appearance: "paintpalette"
        case .messages: "text.bubble"
        case .panes: "rectangle.split.2x1"
        }
    }

    public var size: NSSize {
        switch self {
        case .appearance: NSSize(width: 500, height: 640)
        case .messages: NSSize(width: 500, height: 360)
        case .panes: NSSize(width: 500, height: 220)
        }
    }
}
