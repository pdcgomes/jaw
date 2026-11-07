import AppKit
import JawThemes
import SwiftUI

/// What a piece of text or a symbol is, so its colour comes from the theme's roles.
public enum Tone: Sendable {
    case primary, secondary, tertiary, accent, warn

    public func color(_ roles: Roles) -> RGBA {
        switch self {
        case .primary: roles.text
        case .secondary: roles.secondary
        case .tertiary: roles.tertiary
        case .accent: roles.accentText
        case .warn: roles.failed
        }
    }
}

public extension RGBA {
    /// The colour for SwiftUI.
    var color: Color {
        Color(nsColor: nsColor)
    }
}
