import AppKit
import JawThemes
import SwiftUI

/// Settings › Appearance: dark, light or the system's, and the theme in each slot.
public struct AppearancePane: View {
    @Bindable var preferences: Preferences

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    public var body: some View {
        Form {
            Section {
                Picker("Appearance", selection: $preferences.appearance) {
                    Text("The system's").tag(Appearance.system)
                    Text("Dark").tag(Appearance.dark)
                    Text("Light").tag(Appearance.light)
                }
                .pickerStyle(.inline)
                ForEach(Slot.allCases, id: \.self) { slot in
                    let listed = preferences.themes.filter { $0.roles(slot) != nil }
                    Picker(slot.title, selection: pick(slot)) {
                        ForEach(listed) { theme in
                            Text("\(theme.name(slot)), \(theme.source.title)").tag(theme.id)
                        }
                        if !listed.contains(where: { $0.id == preferences.slots[slot] }) {
                            Text("\(preferences.slots[slot]), drawn as \(Catalog.jawTheme.name(slot))").tag(preferences.slots[slot])
                        }
                    }
                }
            } footer: {
                Text("Jaw draws the dark theme while the Mac is dark, and the light one while it's light.")
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.appearance.size.width, height: SettingsPane.appearance.size.height)
    }

    /// A slot's menu: choosing a theme fills the slots as a click on its card will (`Slots.choosing`).
    func pick(_ slot: Slot) -> Binding<String> {
        Binding(
            get: { preferences.slots[slot] },
            set: { id in
                if let theme = preferences.themes.first(where: { $0.id == id }) {
                    preferences.slots = preferences.slots.choosing(theme, for: slot)
                }
            },
        )
    }
}

/// Settings › Messages: bubbles or lines, comfortable or compact, the text size, the font, and
/// whether photos and videos go in HD.
public struct MessagesPane: View {
    @Bindable var preferences: Preferences

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    public var body: some View {
        Form {
            Section {
                Picker("Messages look like", selection: $preferences.look) {
                    ForEach(Look.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
            } footer: {
                Text(preferences.look == .bubbles
                    ? "Theirs on the left, yours on the right."
                    : "The terminal's layout: names on the left, yours from a third of the way across.")
            }
            Section {
                Picker("Spacing", selection: $preferences.density) {
                    ForEach(Density.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                LabeledContent("Text size") {
                    HStack {
                        Slider(value: $preferences.textSize, in: Preferences.textSizes, step: 1)
                        Text("\(Int(preferences.textSize)) pt").monospacedDigit().frame(width: 40, alignment: .trailing)
                    }
                }
                Picker("Font", selection: $preferences.font) {
                    ForEach(MessageFont.allCases, id: \.self) { Text($0.title).tag($0) }
                }
            }
            Section {
                Toggle("Send photos and videos in HD", isOn: $preferences.sendHD)
            } footer: {
                Text("A sharper copy of each goes as well, as WhatsApp's HD sends it. Tab in the box switches what's there; ⌥ as you click Send sends HD.")
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.messages.size.width, height: SettingsPane.messages.size.height)
    }
}

/// Settings › Panes: how much the panes without the keys are dimmed, and the focused pane's outline.
public struct PanesPane: View {
    @Bindable var preferences: Preferences
    @Environment(\.colorScheme) private var scheme

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    /// The theme's accent in the appearance Settings is drawn in, the outline's colour unless one's picked.
    var accent: RGBA {
        preferences.style(dark: scheme == .dark).roles.accent
    }

    public var body: some View {
        Form {
            Section {
                LabeledContent("Dim the other panes") {
                    HStack {
                        Slider(value: $preferences.paneDim, in: Preferences.paneDims)
                        Text("\(Int((preferences.paneDim * 100).rounded())) %").monospacedDigit().frame(width: 40, alignment: .trailing)
                    }
                }
                Toggle("Outline the focused pane", isOn: $preferences.paneOutline)
                LabeledContent("Outline colour") {
                    HStack {
                        ColorPicker("", selection: Binding(
                            get: { (preferences.paneOutlineColour ?? accent).color },
                            set: { preferences.paneOutlineColour = RGBA(NSColor($0)) },
                        ), supportsOpacity: false)
                            .labelsHidden()
                        Button("The theme's accent") { preferences.paneOutlineColour = nil }
                            .disabled(preferences.paneOutlineColour == nil)
                    }
                }
                .disabled(!preferences.paneOutline)
            } footer: {
                Text("With more than one pane, the one with the keys is outlined inside its edge, and the others dimmed, as iTerm does.")
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.panes.size.width, height: SettingsPane.panes.size.height)
    }
}

extension RGBA {
    /// From a colour picked, in sRGB.
    init(_ colour: NSColor) {
        let c = colour.usingColorSpace(.sRGB) ?? colour
        self.init(r: Double(c.redComponent), g: Double(c.greenComponent), b: Double(c.blueComponent))
    }
}
