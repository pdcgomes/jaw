import AppKit
import CryptoKit
import JawUI
import SwiftUI

/// Scenes drawn into pictures by their own views, off the screen, so no Screen Recording is needed:
/// one for `--snapshot`, and every scene in each of its variants for `--sweep`.
@MainActor
enum Pictures {
    /// Pictures are drawn at 2×, as a Retina screen shows them.
    static let scale: CGFloat = 2
    /// How long a scene is given before it's drawn, for what it does once it's made: rows applied on
    /// the next turn and laid out off the main thread.
    static let settle = Duration.milliseconds(400)

    enum PictureError: Error, CustomStringConvertible {
        case noScene(String)
        case cantDraw(String)

        var description: String {
            switch self {
            case let .noScene(id): "there's no scene called \(id)"
            case let .cantDraw(id): "\(id) couldn't be drawn"
            }
        }
    }

    /// A scene drawn: the picture, its size in points and in pixels, a hash of its pixels, which
    /// changes exactly when the picture does, and the notes its bands carried.
    struct Drawn {
        let png: Data
        let points: CGSize
        let pixels: [Int]
        let hash: String
        let notes: [Note]
    }

    /// Draws `scene` in `conditions`, in a window of its own off the screen, once it has had `settle`
    /// to do what it does after it's made.
    static func draw(_ scene: HarnessScene, _ conditions: Conditions) async throws -> Drawn {
        let taker = NoteTaker()
        let canvas = SceneCanvas(scene: scene, conditions: conditions)
            .onPreferenceChange(NotesKey.self) { notes in
                MainActor.assumeIsolated { taker.notes = notes }
            }
        let host = NSHostingView(rootView: AnyView(canvas))
        let size = host.fittingSize
        let window = NSWindow(contentRect: NSRect(origin: NSPoint(x: -30000, y: -30000), size: size), styleMask: [.borderless],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: conditions.dark ? .darkAqua : .aqua)
        window.contentView = host
        window.orderFront(nil)
        defer { window.orderOut(nil) }
        try await Task.sleep(for: settle)
        host.layoutSubtreeIfNeeded()
        let bounds = host.bounds
        guard bounds.width >= 1, bounds.height >= 1,
              let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int((bounds.width * scale).rounded()),
                                         pixelsHigh: Int((bounds.height * scale).rounded()), bitsPerSample: 8,
                                         samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                         bytesPerRow: 0, bitsPerPixel: 0)
        else { throw PictureError.cantDraw(scene.id) }
        rep.size = bounds.size
        host.cacheDisplay(in: bounds, to: rep)
        guard let png = rep.representation(using: .png, properties: [:]), let pixels = rep.bitmapData else {
            throw PictureError.cantDraw(scene.id)
        }
        let digest = SHA256.hash(data: Data(bytes: pixels, count: rep.bytesPerRow * rep.pixelsHigh))
        let hash = String(digest.map { String(format: "%02x", $0) }.joined().prefix(16))
        return Drawn(png: png, points: bounds.size, pixels: [rep.pixelsWide, rep.pixelsHigh], hash: hash, notes: taker.notes)
    }

    /// `--snapshot`: the scene `--scene` names, or the first, in the conditions the flags set.
    static func snapshot(_ launch: HarnessLaunch, catalog: HarnessCatalog, to url: URL) async throws {
        let scene = launch.scene == nil ? catalog.scenes.first : catalog.scene(id: launch.scene)
        guard let scene else { throw PictureError.noScene(launch.scene ?? "(none)") }
        let drawn = try await draw(scene, launch.conditions)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try drawn.png.write(to: url)
    }
}

/// The notes a scene's bands handed up as SwiftUI laid it out.
@MainActor
private final class NoteTaker {
    var notes: [Note] = []
}
