import AppKit
import JawUI
import SwiftUI

/// The scenes by section on the left, the conditions under them, and the stage on the right. The
/// conditions live here rather than in the stage, so moving to another scene keeps what you were
/// judging the last one in.
@MainActor
struct HarnessRootView: View {
    private let catalog = HarnessCatalog.shared
    @State private var selection: String? = HarnessLaunch.current.scene ?? HarnessCatalog.shared.scenes.first?.id
    @State private var settings = StageSettings(launch: HarnessLaunch.current)
    @State private var columns: NavigationSplitViewVisibility = HarnessLaunch.current.stageOnly ? .detailOnly : .automatic

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            List(selection: $selection) {
                ForEach(catalog.sections) { section in
                    Section {
                        ForEach(catalog.scenes(in: section)) { scene in
                            Label(scene.title, systemImage: scene.symbol).tag(scene.id)
                        }
                    } header: {
                        Label(section.rawValue, systemImage: section.symbol)
                    }
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) { ConditionsControls(conditions: $settings.conditions) }
            .navigationSplitViewColumnWidth(min: 220, ideal: 250, max: 320)
        } detail: {
            if let scene = catalog.scene(id: selection) {
                Stage(scene: scene, settings: $settings)
            } else {
                ContentUnavailableView("Choose a scene", systemImage: "square.grid.2x2")
            }
        }
        .preferredColorScheme(settings.conditions.dark ? .dark : .light)
        // AppKit's views take the app's appearance, not SwiftUI's colour scheme.
        .onChange(of: settings.conditions.dark, initial: true) { _, dark in
            NSApp.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        }
        .onChange(of: settings.conditions.reduceMotion, initial: true) { _, reduced in
            Motion.reduced = { reduced }
        }
        .task { await HarnessWindow.size(HarnessLaunch.current.window) }
    }
}

/// `--window`: the harness's window sized as asked once it's on screen, and kept out of its saved
/// frame, since a scripted size isn't the owner's.
enum HarnessWindow {
    @MainActor
    static func size(_ size: HarnessLaunch.Size?) async {
        guard let size else { return }
        var window: NSWindow?
        for _ in 0 ..< 50 {
            window = NSApp.windows.first { $0.isVisible && $0.canBecomeMain }
            if window != nil { break }
            try? await Task.sleep(for: .milliseconds(100))
        }
        guard let window, let area = (window.screen ?? NSScreen.main)?.visibleFrame else { return }
        window.setFrameAutosaveName("")
        window.setFrame(NSRect(x: area.midX - size.width / 2, y: area.midY - size.height / 2, width: size.width,
                               height: size.height), display: true)
    }
}
