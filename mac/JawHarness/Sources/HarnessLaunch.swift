import Foundation
import JawThemes
import JawUI

/// What the harness was launched to do, from its command line:
///
/// - `--scene <id>` opens a scene.
/// - `--theme jaw|omarchy:<id>|redlamp:<id>`, `--appearance dark|light`, `--look bubbles|lines`,
///   `--density comfortable|compact`, `--text-size <11…17>`, `--font system|mono` and `--reduce-motion`
///   set the conditions.
/// - `--window <width>x<height>` sizes the window, in points.
/// - `--stage-only` hides the scene list and the inspector.
/// - `--snapshot <png>` draws the scene into a picture, and quits.
/// - `--sweep <folder>` draws every scene in each of its variants, with `manifest.json`, and quits.
///
/// A value it doesn't know leaves what it sets as it was.
struct HarnessLaunch: Equatable {
    struct Size: Hashable, Sendable {
        var width: CGFloat
        var height: CGFloat
    }

    var scene: String?
    var conditions = Conditions()
    var window: Size?
    var stageOnly = false
    var snapshot: String?
    var sweep: String?

    @MainActor static let current = parse(CommandLine.arguments)

    @MainActor
    static func parse(_ arguments: [String]) -> HarnessLaunch {
        func value(_ flag: String) -> String? {
            guard let i = arguments.firstIndex(of: flag), i + 1 < arguments.count, !arguments[i + 1].hasPrefix("--") else { return nil }
            return arguments[i + 1]
        }
        var launch = HarnessLaunch()
        launch.scene = value("--scene")
        if let theme = value("--theme"), Fixtures.themes.contains(where: { $0.id == theme }) {
            launch.conditions.theme = theme
        }
        switch value("--appearance") {
        case "dark": launch.conditions.dark = true
        case "light": launch.conditions.dark = false
        default: break
        }
        if let look = value("--look").flatMap(Look.init(rawValue:)) {
            launch.conditions.look = look
        }
        if let density = value("--density").flatMap(Density.init(rawValue:)) {
            launch.conditions.density = density
        }
        if let size = value("--text-size").flatMap(Double.init), size.isFinite {
            let sizes = Preferences.textSizes
            launch.conditions.textSize = min(max(CGFloat(size.rounded()), sizes.lowerBound), sizes.upperBound)
        }
        if let font = value("--font").flatMap(MessageFont.init(rawValue:)) {
            launch.conditions.font = font
        }
        launch.conditions.reduceMotion = arguments.contains("--reduce-motion")
        if let parts = value("--window")?.split(separator: "x", omittingEmptySubsequences: false), parts.count == 2,
           let width = Double(parts[0]), let height = Double(parts[1]), width.isFinite, height.isFinite, width > 0, height > 0
        {
            launch.window = Size(width: width, height: height)
        }
        launch.stageOnly = arguments.contains("--stage-only")
        launch.snapshot = value("--snapshot")
        launch.sweep = value("--sweep")
        return launch
    }
}
