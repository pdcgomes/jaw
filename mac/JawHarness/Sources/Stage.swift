import AppKit
import JawThemes
import JawUI
import SwiftUI

/// What the stage puts behind a scene: a component is judged on the surface it ships on.
enum StageSurface: String, CaseIterable, Identifiable {
    /// The window's ground: the conversation, the panes.
    case ground = "Ground"
    /// The sidebar's, the list's, and their bubbles'.
    case surface = "Surface"
    /// The system's window colour, which sheets sit on.
    case window = "Window"

    var id: String {
        rawValue
    }

    func color(_ roles: Roles) -> Color {
        switch self {
        case .ground: roles.ground.color
        case .surface: roles.surface.color
        case .window: Color(nsColor: .windowBackgroundColor)
        }
    }
}

/// The stage's settings, kept by the root view so they hold from scene to scene.
struct StageSettings {
    var conditions: Conditions
    var surface = StageSurface.ground
    var inspectorShown: Bool

    init(launch: HarnessLaunch) {
        conditions = launch.conditions
        inspectorShown = !launch.stageOnly
    }
}

/// A scene drawn on its surface in its conditions, `width` points wide and as tall as it needs, with
/// the stage's margin around it: as the stage shows it, and as a picture is drawn.
struct SceneCanvas: View {
    let scene: HarnessScene
    let conditions: Conditions
    var surface = StageSurface.ground

    var body: some View {
        scene.content(conditions)
            .frame(width: scene.width, alignment: .topLeading)
            .padding(Space.xxl)
            .background(surface.color(conditions.roles))
            .environment(\.colorScheme, conditions.dark ? .dark : .light)
    }
}

/// One scene on the stage: on the chosen surface, in the conditions, with its inspector.
struct Stage: View {
    let scene: HarnessScene
    @Binding var settings: StageSettings

    var body: some View {
        ScrollView([.horizontal, .vertical]) {
            SceneCanvas(scene: scene, conditions: settings.conditions, surface: settings.surface)
        }
        .defaultScrollAnchor(.topLeading, for: .alignment)
        .background(settings.surface.color(settings.conditions.roles))
        // AppKit views take their colours when they're made, so other conditions make them again.
        .id(StageIdentity(scene: scene.id, conditions: settings.conditions))
        .navigationTitle(scene.title)
        .navigationSubtitle(scene.synopsis)
        .inspector(isPresented: inspectorShown) {
            scene.inspector?()
                .inspectorColumnWidth(min: 260, ideal: 300, max: 420)
        }
        .toolbar {
            ToolbarItem {
                Picker("Surface", selection: $settings.surface) {
                    ForEach(StageSurface.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .help("The surface behind the scene")
            }
            if scene.inspector != nil {
                ToolbarItem {
                    Toggle(isOn: $settings.inspectorShown) {
                        Label("Inspector", systemImage: "sidebar.right")
                    }
                    .help("The scene's knobs")
                }
            }
        }
    }

    /// Only a scene with an inspector opens one, so no empty pane shows beside the others.
    private var inspectorShown: Binding<Bool> {
        let settings = $settings
        let hasInspector = scene.inspector != nil
        return Binding(get: { settings.wrappedValue.inspectorShown && hasInspector },
                       set: { settings.wrappedValue.inspectorShown = $0 })
    }
}

private struct StageIdentity: Hashable {
    let scene: String
    let conditions: Conditions
}
