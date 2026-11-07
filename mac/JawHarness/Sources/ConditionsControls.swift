import JawThemes
import JawUI
import SwiftUI

/// The conditions every scene is drawn in, under the scene list, where they hold from scene to scene:
/// the theme, dark or light, the look, the density, the text size, the font and Reduce Motion.
struct ConditionsControls: View {
    @Binding var conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: Space.m) {
            Picker("Theme", selection: $conditions.theme) {
                ForEach(ThemeSource.allCases, id: \.self) { source in
                    Section(source.title) {
                        ForEach(Fixtures.themes.filter { $0.source == source }) { Text($0.name).tag($0.id) }
                    }
                }
            }
            Picker("Appearance", selection: $conditions.dark) {
                Text("Dark").tag(true)
                Text("Light").tag(false)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            Picker("Look", selection: $conditions.look) {
                ForEach(Look.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            Picker("Density", selection: $conditions.density) {
                ForEach(Density.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            HStack(spacing: Space.m) {
                Text("Text").textStyle(.callout)
                Slider(value: $conditions.textSize, in: Preferences.textSizes, step: 1)
                Text("\(Int(conditions.textSize)) pt").textStyle(.caption).monospacedDigit()
            }
            Picker("Font", selection: $conditions.font) {
                ForEach(MessageFont.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            Toggle("Reduce Motion", isOn: $conditions.reduceMotion)
        }
        .padding(Space.l)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }
}
