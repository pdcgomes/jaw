import Foundation
import JawThemes

extension Fixtures {
    /// Omarchy's themes as jawd serves them: each `protocol/themes/<id>.toml` in the harness's bundle,
    /// read as jaw-proto reads it, in the order of their IDs. A file that can't be read stops the
    /// harness, saying which.
    static let omarchy: [OmarchyTheme] = (Bundle.main.urls(forResourcesWithExtension: "toml", subdirectory: nil) ?? [])
        .map { url in
            let id = url.deletingPathExtension().lastPathComponent
            guard let text = try? String(contentsOf: url, encoding: .utf8), let theme = OmarchyTheme(id: id, file: text) else {
                fatalError("protocol/themes/\(id).toml doesn't read as jaw-proto reads it")
            }
            return theme
        }
        .sorted { $0.id < $1.id }

    /// Every theme there is to choose, as the gallery lists them, Neutral in macOS 27.0.1's colours
    /// (`SystemAccent.blue`), so the harness's pictures don't depend on the Mac's colours.
    static let themes = Catalog.themes(omarchy: omarchy, accent: .blue)
}
