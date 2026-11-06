import AppKit
import JawThemes
import SwiftUI

/// Settings, in a sheet over the window: Appearance, Messages and Panes, and Done.
public struct SettingsView: View {
    @Bindable var preferences: Preferences
    /// The theme's accent, the outline's colour unless one's picked.
    let accent: RGBA
    let done: () -> Void
    @State var section = 0

    public init(preferences: Preferences, accent: RGBA, done: @escaping () -> Void) {
        self.preferences = preferences
        self.accent = accent
        self.done = done
    }

    public var body: some View {
        VStack(spacing: 0) {
            // Sections chosen with a segmented control inside the sheet: a tab view draws its tabs
            // across its top edge, half above it, where a sheet has nothing to hold them.
            Picker("Section", selection: $section) {
                Text("Appearance").tag(0)
                Text("Messages").tag(1)
                Text("Panes").tag(2)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 330)
            .padding(.top, Space.xl)
            tabs
            HStack {
                Spacer()
                Button("Done", action: done).keyboardShortcut(.defaultAction)
            }
            .padding(Space.l)
        }
        .focusEffectDisabled()
    }

    var tabs: some View {
        Group {
            switch section {
        case 0:
            Form {
                Picker("Appearance", selection: $preferences.appearance) {
                    Text("The system's").tag(Appearance.system)
                    Text("Dark").tag(Appearance.dark)
                    Text("Light").tag(Appearance.light)
                }
                .pickerStyle(.inline)
                Picker("Theme", selection: $preferences.theme) {
                    ForEach(Catalog.families) { family in
                        Text(family.name == "jaw" ? "Jaw (Tokyo Night)" : family.name).tag(family.id)
                    }
                }
                Text("Redlamp's themes come in a later version.").font(.footnote).foregroundStyle(.secondary)
            }
            .formStyle(.grouped)
        case 1:
            Form {
                Picker("Messages look like", selection: $preferences.look) {
                    ForEach(Look.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                Text(preferences.look == .bubbles
                    ? "Theirs on the left, yours on the right."
                    : "The terminal's layout: names on the left, yours from a third of the way across.")
                    .font(.footnote).foregroundStyle(.secondary)
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
                Toggle("Send photos and videos in HD", isOn: $preferences.sendHD)
                Text("A sharper copy of each goes as well, as WhatsApp's HD sends it. Tab in the box switches what's there; ⌥ as you click Send sends HD.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            .formStyle(.grouped)
        default:
            Form {
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
                            set: { preferences.paneOutlineColour = RGBA(NSColor($0)) }
                        ), supportsOpacity: false)
                        .labelsHidden()
                        Button("The theme's accent") { preferences.paneOutlineColour = nil }
                            .disabled(preferences.paneOutlineColour == nil)
                    }
                }
                .disabled(!preferences.paneOutline)
                Text("With more than one pane, the one with the keys is outlined inside its edge, and the others dimmed, as iTerm does.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            .formStyle(.grouped)
            }
        }
        .frame(width: 480, height: 360)
    }
}

extension RGBA {
    /// From a colour picked, in sRGB.
    init(_ colour: NSColor) {
        let c = colour.usingColorSpace(.sRGB) ?? colour
        self.init(r: Double(c.redComponent), g: Double(c.greenComponent), b: Double(c.blueComponent))
    }
}
