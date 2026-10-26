import JawThemes
import SwiftUI

public extension View {
    /// Text in the type scale.
    func textStyle(_ style: TextStyle) -> some View {
        font(style.font)
    }

    /// A colour by what the text is.
    func tone(_ tone: Tone, _ roles: Roles) -> some View {
        foregroundStyle(tone.color(roles).color)
    }

    /// A row's or a card's ground: the selection's colour when picked, the surface's otherwise.
    func card(_ roles: Roles, selected: Bool = false, radius: CGFloat = Radius.control) -> some View {
        background(RoundedRectangle(cornerRadius: radius).fill((selected ? roles.selection : roles.surface).color))
    }

    /// A panel's ground and outline: the palette, the glance.
    func panel(_ roles: Roles) -> some View {
        background(roles.ground.color)
            .clipShape(RoundedRectangle(cornerRadius: Radius.panel))
            .overlay(RoundedRectangle(cornerRadius: Radius.panel).stroke(roles.separator.color, lineWidth: Stroke.hairline))
    }
}

public extension Text {
    /// Text in the type scale, still `Text`, so styled runs can be joined.
    func textStyle(_ style: TextStyle) -> Text {
        font(style.font)
    }

    func tone(_ tone: Tone, _ roles: Roles) -> Text {
        foregroundStyle(tone.color(roles).color)
    }
}

/// A key as it's shown beside what it does: ⌘K.
public struct KeyCap: View {
    let text: String
    let roles: Roles
    let tone: Tone

    public init(_ shortcut: Shortcut, roles: Roles, tone: Tone = .accent) {
        text = shortcut.display
        self.roles = roles
        self.tone = tone
    }

    public init(_ text: String, roles: Roles, tone: Tone = .accent) {
        self.text = text
        self.roles = roles
        self.tone = tone
    }

    public var body: some View {
        Text(text).textStyle(.key).tone(tone, roles)
    }
}
