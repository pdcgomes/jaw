import AppKit
import SwiftUI

/// Jaw Harness: the app's components and screens in the states worth reviewing, drawn from made-up
/// rows and the protocol's fixtures, as Redlamp's and omoi-station's harnesses draw theirs. It links
/// the app's own frameworks and asks no jawd for anything.
@main
struct HarnessApp: App {
    @NSApplicationDelegateAdaptor(HarnessDelegate.self) private var delegate

    var body: some Scene {
        // One window: the appearance and Reduce Motion it sets are the app's, not a window's.
        Window("Jaw Harness", id: "harness") {
            HarnessRootView()
                .frame(minWidth: 900, minHeight: 600)
        }
        .defaultSize(width: 1400, height: 900)
    }
}

/// Draws what `--snapshot` or `--sweep` asks for once the app has started, then quits; a failure says
/// why on standard error and quits with status 1, so a script stops on it.
final class HarnessDelegate: NSObject, NSApplicationDelegate {
    @MainActor
    func applicationDidFinishLaunching(_: Notification) {
        let launch = HarnessLaunch.current
        guard launch.snapshot != nil || launch.sweep != nil else { return }
        Task { @MainActor in
            do {
                if let snapshot = launch.snapshot {
                    try await Pictures.snapshot(launch, catalog: HarnessCatalog.shared, to: URL(fileURLWithPath: snapshot))
                }
                if let folder = launch.sweep {
                    let manifest = try await Pictures.sweep(HarnessCatalog.shared, conditions: launch.conditions,
                                                            to: URL(fileURLWithPath: folder))
                    let pictures = manifest.scenes.reduce(0) { $0 + $1.pictures.count }
                    print("swept \(manifest.scenes.count) scenes into \(pictures) pictures in \(folder)")
                }
                NSApp.terminate(nil)
            } catch {
                FileHandle.standardError.write(Data("jaw-harness: \(error)\n".utf8))
                exit(1)
            }
        }
    }
}
