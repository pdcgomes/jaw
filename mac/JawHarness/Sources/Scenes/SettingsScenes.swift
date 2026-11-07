import AppKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var settingsPanes: HarnessScene {
        HarnessScene(
            id: "settings-panes", title: "Settings' panes", symbol: "gearshape",
            synopsis: "Appearance, Messages and Panes at the window's width, each at its own height, as the toolbar's tabs show them",
            section: .settings, covers: [AppearancePane.self, MessagesPane.self, PanesPane.self, SlotTileView.self],
            width: SettingsPane.appearance.size.width,
        ) { SettingsPanesScene(conditions: $0) }
    }

    @MainActor static var themeGallery: HarnessScene {
        HarnessScene(
            id: "theme-gallery", title: "Theme gallery", symbol: "square.grid.3x3",
            synopsis: "Every theme's card for the slot of the appearance, as the gallery draws them: each should read, and none look like another",
            section: .settings, covers: [GalleryGrid.self, ThemeCard.self, ThemeChat.self],
            width: ThemeChat.size.width * 3 + Space.m * 2,
        ) { ThemeGalleryScene(conditions: $0) }
    }

    static var themeStates: HarnessScene {
        HarnessScene(
            id: "theme-states", title: "Theme states", symbol: "checkmark.circle",
            synopsis: "A card at rest, under the pointer and filling its slot; the slots' tiles; and the Omarchy section while jawd starts, can't be reached and is older",
            section: .settings, covers: [ThemeCard.self, SlotTileView.self, GallerySection.self],
            width: SettingsPane.appearance.size.width,
        ) { ThemeStatesScene(conditions: $0) }
    }
}

/// Settings' preferences for a scene: the harness's own, emptied once a launch so each starts from the
/// app's defaults; the scene's look, density, text size, font and appearance; its theme in the slot of
/// its appearance and Jaw's in the other; Omarchy's themes as jawd serves them; and Neutral in macOS
/// 27.0.1's colours (`SystemAccent.blue`), so the pictures don't depend on the Mac's colours.
@MainActor
private func scenePreferences(_ conditions: Conditions) -> Preferences {
    let preferences = Preferences(defaults: SettingsDefaults.shared, accent: .blue)
    preferences.look = conditions.look
    preferences.density = conditions.density
    preferences.textSize = conditions.textSize
    preferences.font = conditions.font
    preferences.appearance = conditions.dark ? .dark : .light
    preferences.served = Fixtures.omarchy
    preferences.slots = conditions.dark ? Slots(dark: conditions.theme) : Slots(light: conditions.theme)
    return preferences
}

/// The harness's own defaults for Settings, emptied as they're made, the first time a scene makes
/// preferences in a launch.
@MainActor
private enum SettingsDefaults {
    static let shared: UserDefaults = {
        let suite = "\(Bundle.main.bundleIdentifier ?? "JawHarness").settings"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        return defaults
    }()
}

/// Settings' three panes as the toolbar's tabs show them, each at its size on the window's colour.
private struct SettingsPanesScene: View {
    let roles: Roles
    @State private var preferences: Preferences

    init(conditions: Conditions) {
        roles = conditions.roles
        _preferences = State(initialValue: scenePreferences(conditions))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "Appearance",
                note: "The appearance in a row; the slots, the one in use outlined and saying so, the other saying when it would be, each its theme's card at half size with its name and source; then the gallery's first sections. Nothing should be cut at the right edge.",
                roles: roles,
            ) { pane(AppearancePane(preferences: preferences)) }
            SpecimenGroup(
                title: "Messages",
                note: "Bubbles or lines, then spacing, text size and font, then whether photos and videos go in HD, each explanation its section's footer.",
                roles: roles,
            ) { pane(MessagesPane(preferences: preferences)) }
            SpecimenGroup(
                title: "Panes",
                note: "How much the other panes are dimmed, and the focused pane's outline, its colour the theme's accent until one's picked.",
                roles: roles,
            ) { pane(PanesPane(preferences: preferences)) }
        }
    }

    /// A pane on the window's colour, as the window holds it.
    func pane(_ view: some View) -> some View {
        view.background(Color(nsColor: .windowBackgroundColor)).clipShape(RoundedRectangle(cornerRadius: Radius.panel))
    }
}

/// Every theme's card for the slot of the scene's appearance, three across at 144 points, as the
/// gallery draws them, the scene's theme ticked.
private struct ThemeGalleryScene: View {
    let conditions: Conditions

    var slot: Slot {
        conditions.dark ? .dark : .light
    }

    var body: some View {
        let slots = conditions.dark ? Slots(dark: conditions.theme) : Slots(light: conditions.theme)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Gallery(Fixtures.themes, for: slot, omarchyLine: nil).sections) { section in
                SpecimenGroup(title: "\(section.source.title), for \(slot.title)", note: note(section.source), roles: conditions.roles) {
                    GalleryGrid(themes: section.themes, slot: slot, slots: slots) { _, _ in }
                }
            }
        }
    }

    func note(_ source: ThemeSource) -> String {
        switch source {
        case .jaw: "Tokyo Night, or Tokyo Night Day for Light: the app's own, as it was before the slots."
        case .omarchy: "The 18 dark or the 5 light, in jawd's order, each mapped as the terminal maps it and held where it's drawn: no line should be hard to read."
        case .redlamp: "Neutral in macOS 27.0.1's colours, then Redlamp and the twenty from omoi-station, held by the same bands and where they're drawn: no line should be faint, Redlamp's red only its accent, its light half's mention a brown and no sender's name red."
        }
    }
}

/// A card in each of its states, the slots' tiles, and the Omarchy section while jawd's themes aren't
/// in, each as the Appearance pane draws it.
private struct ThemeStatesScene: View {
    let conditions: Conditions

    var slot: Slot {
        conditions.dark ? .dark : .light
    }

    /// The scene's theme and its half for the scene's appearance, or Jaw's where it has no such half.
    var shown: (theme: Theme, roles: Roles) {
        if let theme = Fixtures.themes.first(where: { $0.id == conditions.theme }), let roles = theme.roles(slot) { return (theme, roles) }
        return (Catalog.jawTheme, Catalog.jaw.roles(dark: slot.isDark))
    }

    var body: some View {
        let (theme, roles) = shown
        let slots = conditions.dark ? Slots(dark: theme.id) : Slots(light: theme.id)
        let empty = Slots(dark: "", light: "")
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "A card",
                note: "At rest, under the pointer, and filling the slot: ringed in the separator, then in the secondary label colour, then ringed in the system's accent with a tick before its name.",
                roles: conditions.roles,
            ) {
                HStack(alignment: .top, spacing: Space.m) {
                    ThemeCard(theme: theme, slot: slot, roles: roles, ticked: false, label: Gallery.label(theme, for: slot, empty))
                    ThemeCard(theme: theme, slot: slot, roles: roles, ticked: false, hovered: true, label: Gallery.label(theme, for: slot, empty))
                    ThemeCard(theme: theme, slot: slot, roles: roles, ticked: true, label: Gallery.label(theme, for: slot, slots))
                }
                .frame(width: ThemeChat.size.width * 3 + Space.m * 2)
            }
            SpecimenGroup(
                title: "The slots",
                note: "The slot chosen for outlined in the accent; the one in use says so, and the other when it would be, as Appearance holds one; on the window's colour, set in as the pane sets them, so each is as wide as there and a name wraps where it wraps there.",
                roles: conditions.roles,
            ) {
                HStack(spacing: Space.m) {
                    ForEach(Slot.allCases, id: \.self) { each in
                        SlotTileView(tile: SlotTile(each, slots: slots, themes: Fixtures.themes, served: true,
                                                    appearance: conditions.dark ? .dark : .light, dark: conditions.dark),
                                     roles: Catalog.roles(each, of: slots, omarchy: Fixtures.omarchy, accent: .blue), outlined: each == slot)
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
                .padding(SettingsPane.margin)
                .background(Color(nsColor: .windowBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: Radius.panel))
            }
            SpecimenGroup(
                title: "Omarchy's, while jawd's themes aren't in",
                note: "The slots' own from their copies, here Gruvbox, under \"Omarchy's, from jawd\", and one line in jawd's state where the section's footer goes: starting, not answering, older.",
                roles: conditions.roles,
            ) {
                VStack(spacing: Space.xl) {
                    ForEach([OmarchyState.starting, .unreachable, .older], id: \.self) { state in
                        GallerySection(section: Gallery.Section(source: .omarchy, themes: Fixtures.themes.filter { $0.id == "omarchy:gruvbox" },
                                                                line: state.line),
                                       slot: .dark, slots: Slots(dark: "omarchy:gruvbox")) { _, _ in }
                            .padding(SettingsPane.margin)
                            .background(Color(nsColor: .windowBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: Radius.panel))
                    }
                }
            }
        }
    }
}
