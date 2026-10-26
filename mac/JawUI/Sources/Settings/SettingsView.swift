import JawThemes
import SwiftUI

/// Settings, first form: Appearance, and Messages.
public struct SettingsView: View {
    @Bindable var preferences: Preferences

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    public var body: some View {
        TabView {
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
            .tabItem { Label("Appearance", systemImage: "paintpalette") }

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
            }
            .formStyle(.grouped)
            .tabItem { Label("Messages", systemImage: "text.bubble") }
        }
        .frame(width: 460, height: 340)
        .focusEffectDisabled()
    }
}
