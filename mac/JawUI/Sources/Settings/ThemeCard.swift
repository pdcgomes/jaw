import AppKit
import JawThemes
import SwiftUI

/// A theme's half as a small chat, drawn from its roles as the chat list and a conversation draw them:
/// on the ground, an unread chat's row on the surface with a separator under it, their bubble on the
/// surface with a mention of you, and yours, all of it inside the card's edge by `Space.s` or more.
/// 104 points tall and as wide as it's given, 144 in the design; a slot's tile draws it at half size.
public struct ThemeChat: View {
    public static let size = CGSize(width: 144, height: 104)
    let roles: Roles

    public init(_ roles: Roles) {
        self.roles = roles
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            VStack(spacing: 0) {
                row
                Rectangle().fill(roles.separator.color).frame(height: Stroke.hairline)
            }
            theirs
            yours.frame(maxWidth: .infinity, alignment: .trailing)
        }
        .padding(Space.s)
        .frame(maxWidth: .infinity, minHeight: Self.size.height, maxHeight: Self.size.height, alignment: .top)
        .background(roles.ground.color)
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .accessibilityHidden(true)
    }

    /// Lisbon Climbing's row, unread, as the chat list draws one: the avatar in a sender's colour with
    /// the online dot, the name with the time in the accent drawn as text after it, and Rui's line with
    /// the count on the accent after it, so in a card 144 points wide only the name is cut.
    var row: some View {
        HStack(spacing: Space.xs) {
            Circle().fill(roles.sender(2).color).frame(width: 16, height: 16)
                .overlay(alignment: .bottomTrailing) { Circle().fill(roles.online.color).frame(width: 6, height: 6) }
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 0) {
                    Text("Lisbon Climbing").bold().foregroundStyle(roles.text.color)
                    Spacer(minLength: Space.xs)
                    Text("13:58").foregroundStyle(roles.accentText.color).fixedSize()
                }
                HStack(spacing: 0) {
                    Text("Rui: Sunday at 9?").foregroundStyle(roles.secondary.color)
                    Spacer(minLength: Space.xs)
                    Text("2").bold().foregroundStyle(roles.onFill.color)
                        .padding(.horizontal, Space.xs)
                        .background(Capsule().fill(roles.accent.color))
                        .fixedSize()
                }
            }
        }
        .textStyle(.micro)
        .lineLimit(1)
        .padding(.horizontal, Space.xs)
        .padding(.vertical, Space.xxs)
        .background(RoundedRectangle(cornerRadius: Radius.small).fill(roles.surface.color))
    }

    /// Their bubble on the surface: Ana's name in another sender's colour over a line with @Pedro, a
    /// mention of you, in the mention colour on a tint of it, as `MessageLayout` draws one.
    var theirs: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Ana").bold().foregroundStyle(roles.sender(0).color)
            HStack(spacing: 0) {
                Text("@Pedro").bold().foregroundStyle(roles.mention.color).background(roles.mentionTint.color)
                Text(" bring the rope").foregroundStyle(roles.text.color)
            }
        }
        .textStyle(.micro)
        .lineLimit(1)
        .padding(Space.xs)
        .background(RoundedRectangle(cornerRadius: Radius.small).fill(roles.surface.color))
    }

    /// Yours: "On it" in your bubble, its time in the secondary text's colour, as the conversation draws your bubble's, and its read ticks in the link colour.
    var yours: some View {
        HStack(spacing: Space.xxs) {
            Text("On it").foregroundStyle(roles.text.color)
            Text("13:59").foregroundStyle(roles.secondary.color)
            Text("✓✓").foregroundStyle(roles.link.color)
        }
        .textStyle(.micro)
        .padding(Space.xs)
        .background(RoundedRectangle(cornerRadius: Radius.small).fill(roles.yours.color))
    }
}

/// A card in the gallery: the half the slot chosen for would take, as a small chat, that half's name
/// under it in the system's label colour, with a tick before it in the system's accent and the accent
/// around the card when it's that slot's theme, as the wireframes draw it, so nothing covers the chat,
/// and a ring under the pointer. VoiceOver reads it as its label: the half's name, its section and the
/// slots it fills.
public struct ThemeCard: View {
    let theme: Theme
    let slot: Slot
    let roles: Roles
    let ticked: Bool
    let hovered: Bool
    let label: String
    @State private var pointer = false

    /// `hovered` draws it as under the pointer, for the harness's pictures.
    public init(theme: Theme, slot: Slot, roles: Roles, ticked: Bool, hovered: Bool = false, label: String) {
        self.theme = theme
        self.slot = slot
        self.roles = roles
        self.ticked = ticked
        self.hovered = hovered
        self.label = label
    }

    public var body: some View {
        VStack(spacing: Space.xs) {
            ThemeChat(roles)
                .overlay {
                    RoundedRectangle(cornerRadius: Radius.control)
                        .strokeBorder(ring, lineWidth: ticked || hovered || pointer ? Stroke.focus : Stroke.hairline)
                }
            HStack(alignment: .firstTextBaseline, spacing: Space.xxs) {
                if ticked {
                    Image(systemName: "checkmark").fontWeight(.semibold).foregroundStyle(Color.accentColor)
                }
                Text(theme.name(slot)).lineLimit(1)
            }
            .font(.callout)
        }
        .contentShape(Rectangle())
        .onHover { pointer = $0 }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityAddTraits(ticked ? [.isButton, .isSelected] : .isButton)
    }

    var ring: Color {
        if ticked { return .accentColor }
        return hovered || pointer ? Color(nsColor: .secondaryLabelColor) : Color(nsColor: .separatorColor)
    }
}

/// A gallery section's cards, three across in rows of their own, each the half `slot` would take and
/// ticked when it's the slot's theme: a click chooses it, and ⌥-click, ⌥ read from the click itself, or
/// its menu's Use for Dark Only (or Light Only) chooses it for that slot alone; VoiceOver's press and its
/// named action do the same.
/// The rows are laid out whole, not lazily, so the gallery can scroll to any card an arrow reaches.
public struct GalleryGrid: View {
    let themes: [Theme]
    let slot: Slot
    let slots: Slots
    let choose: (Theme, _ only: Bool) -> Void

    public init(themes: [Theme], slot: Slot, slots: Slots, choose: @escaping (Theme, _ only: Bool) -> Void) {
        self.themes = themes
        self.slot = slot
        self.slots = slots
        self.choose = choose
    }

    public var body: some View {
        VStack(spacing: Space.m) {
            ForEach(Array(stride(from: 0, to: themes.count, by: 3)), id: \.self) { start in
                HStack(alignment: .top, spacing: Space.m) {
                    ForEach(themes[start ..< min(start + 3, themes.count)]) { theme in
                        card(theme)
                    }
                    ForEach(0 ..< 3 - min(3, themes.count - start), id: \.self) { _ in
                        Color.clear.frame(maxWidth: .infinity, maxHeight: 0)
                    }
                }
            }
        }
    }

    func card(_ theme: Theme) -> some View {
        ThemeCard(theme: theme, slot: slot, roles: theme.roles(slot) ?? Catalog.jaw.roles(dark: slot.isDark),
                  ticked: slots[slot] == theme.id, label: Gallery.label(theme, for: slot, slots))
            .onTapGesture { choose(theme, false) }
            .highPriorityGesture(TapGesture().modifiers(.option).onEnded { choose(theme, true) })
            .contextMenu {
                Button("Use for \(slot.title) Only") { choose(theme, true) }
            }
            .accessibilityAction { choose(theme, false) }
            .accessibilityAction(named: "Use for \(slot.title) Only") { choose(theme, true) }
            .id(theme.id)
    }
}

/// A gallery section as the Appearance pane draws it: its header, its cards three across, and its
/// footer, which says why while jawd's themes aren't in.
public struct GallerySection: View {
    let section: Gallery.Section
    let slot: Slot
    let slots: Slots
    let choose: (Theme, _ only: Bool) -> Void

    public init(section: Gallery.Section, slot: Slot, slots: Slots, choose: @escaping (Theme, _ only: Bool) -> Void) {
        self.section = section
        self.slot = slot
        self.slots = slots
        self.choose = choose
    }

    public var body: some View {
        SettingsSection(section.header, footer: section.footer) {
            if !section.themes.isEmpty {
                GalleryGrid(themes: section.themes, slot: slot, slots: slots, choose: choose)
            }
        }
    }
}

/// A section as the system's grouped forms draw one, but open rather than boxed, as the wireframes
/// draw the slots and the gallery: its header, a heading VoiceOver moves by, what it holds, and its
/// footer, the header and footer set in from what it holds as a grouped form's are from its box, so
/// they line up with Messages' and Panes'.
public struct SettingsSection<Content: View>: View {
    let title: String
    let footer: String?
    let content: Content

    public init(_ title: String, footer: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.footer = footer
        self.content = content()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            Text(title).font(.headline).accessibilityAddTraits(.isHeader)
                .padding(.horizontal, SettingsPane.indent)
            content
            if let footer {
                Text(footer).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, SettingsPane.indent)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A slot's tile: what it draws, as a card at half size, with the slot's name, its theme's name and
/// source, and whether it's in use or when it would be; outlined in the system's accent while the
/// gallery chooses for it, which VoiceOver hears as its value. It fills the height it's given, from
/// its top, so two side by side in a row as tall as the taller of them match, their names level,
/// whichever says more.
public struct SlotTileView: View {
    let tile: SlotTile
    let roles: Roles
    let outlined: Bool

    public init(tile: SlotTile, roles: Roles, outlined: Bool) {
        self.tile = tile
        self.roles = roles
        self.outlined = outlined
    }

    public var body: some View {
        HStack(alignment: .top, spacing: Space.m) {
            ThemeChat(roles)
                .frame(width: ThemeChat.size.width, height: ThemeChat.size.height)
                .scaleEffect(0.5)
                .frame(width: ThemeChat.size.width / 2, height: ThemeChat.size.height / 2)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(tile.slot.title).font(.headline)
                Text(tile.name).font(.callout)
                if let note = tile.note {
                    Text(note).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(Space.xs)
        .frame(maxWidth: .infinity, minHeight: 64, maxHeight: .infinity, alignment: .topLeading)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(outlined ? Color.accentColor : Color(nsColor: .separatorColor), lineWidth: outlined ? Stroke.focus : Stroke.hairline)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityValue(outlined ? "The gallery shows its themes" : "")
        .accessibilityAddTraits(outlined ? [.isButton, .isSelected] : .isButton)
    }
}
