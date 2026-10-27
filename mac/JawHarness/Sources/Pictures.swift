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
        case notASweep(String)

        var description: String {
            switch self {
            case let .noScene(id): "there's no scene called \(id)"
            case let .cantDraw(id): "\(id) couldn't be drawn"
            case let .notASweep(path): "\(path) has files a sweep didn't make in it; sweep into an empty folder, or one a sweep made"
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
    /// to do what it does after it's made. It holds the app-wide `Motion.reduced` for as long as it
    /// draws, so draws mustn't overlap.
    static func draw(_ scene: HarnessScene, _ conditions: Conditions) async throws -> Drawn {
        let reduced = Motion.reduced
        Motion.reduced = { conditions.reduceMotion }
        defer { Motion.reduced = reduced }
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
        let wide = Int((bounds.width * scale).rounded())
        guard bounds.width >= 1, bounds.height >= 1,
              let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: wide,
                                         pixelsHigh: Int((bounds.height * scale).rounded()), bitsPerSample: 8,
                                         samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                         bytesPerRow: wide * 4, bitsPerPixel: 32)
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

    /// What a sweep drew, as `manifest.json` has it: each scene with its notes and its pictures.
    struct Manifest: Codable, Equatable {
        struct Picture: Codable, Equatable {
            let variant: String
            /// From the sweep's folder: `<scene>/<variant>.png`.
            let file: String
            let points: [Double]
            let pixels: [Int]
            let hash: String
        }

        struct Scene: Codable, Equatable {
            let id: String
            let section: String
            let title: String
            let synopsis: String
            let notes: [Note]
            let pictures: [Picture]
        }

        let made: String
        let scenes: [Scene]
    }

    /// `--sweep`: every scene in each of its variants, as `<scene>/<variant>.png` under `folder`, with
    /// `manifest.json` saying what each is. It draws them into a hidden folder beside `folder`,
    /// `.<name>.sweeping`, made afresh each time, and only once every picture and the manifest are
    /// written does it put that in `folder`'s place, through `replaceItemAt` where there's an old one,
    /// so no moment leaves neither, and nothing is left beside it. A sweep that's stopped or fails
    /// leaves the last whole one where it was, and the next sweep clears what it left. It replaces
    /// only a missing or empty folder, or one a sweep made, which it checks before it draws and again
    /// before it replaces it; one with anything else in it is left as it is, and the sweep fails.
    @discardableResult
    static func sweep(_ catalog: HarnessCatalog, conditions: Conditions, to folder: URL) async throws -> Manifest {
        let fm = FileManager.default
        let folder = folder.standardizedFileURL
        try checkReplaceable(folder)
        let sweeping = folder.deletingLastPathComponent().appending(path: ".\(folder.lastPathComponent).sweeping")
        if fm.fileExists(atPath: sweeping.path) { try fm.removeItem(at: sweeping) }
        try fm.createDirectory(at: sweeping, withIntermediateDirectories: true)
        var scenes: [Manifest.Scene] = []
        for scene in catalog.scenes {
            try fm.createDirectory(at: sweeping.appending(path: scene.id), withIntermediateDirectories: true)
            var pictures: [Manifest.Picture] = []
            var notes: [Note] = []
            for variant in scene.variants {
                let drawn = try await draw(scene, variant.applied(to: conditions))
                let file = "\(scene.id)/\(variant.name).png"
                try drawn.png.write(to: sweeping.appending(path: file))
                pictures.append(Manifest.Picture(variant: variant.name, file: file,
                                                 points: [Double(drawn.points.width), Double(drawn.points.height)],
                                                 pixels: drawn.pixels, hash: drawn.hash))
                if notes.isEmpty { notes = drawn.notes }
            }
            scenes.append(Manifest.Scene(id: scene.id, section: scene.section.rawValue, title: scene.title,
                                         synopsis: scene.synopsis, notes: notes, pictures: pictures))
        }
        let manifest = Manifest(made: ISO8601DateFormatter().string(from: Date()), scenes: scenes)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(manifest).write(to: sweeping.appending(path: "manifest.json"))
        try checkReplaceable(folder)
        if fm.fileExists(atPath: folder.path) {
            _ = try fm.replaceItemAt(folder, withItemAt: sweeping)
            if fm.fileExists(atPath: sweeping.path) { try fm.removeItem(at: sweeping) }
        } else {
            try fm.moveItem(at: sweeping, to: folder)
        }
        return manifest
    }

    /// Refuses `folder` unless a sweep may replace it: it's missing or empty, or it's one a sweep made,
    /// whose `manifest.json` decodes as a manifest and which holds nothing else but a folder for each
    /// of the scenes it lists, each with only that scene's pictures in it. Only the `.DS_Store` the
    /// Finder drops anywhere is overlooked, in it or in a scene's folder; any other hidden entry, such
    /// as a `.git`, counts. So a mistyped path can't lose what's there.
    private static func checkReplaceable(_ folder: URL) throws {
        let fm = FileManager.default
        func isFolder(_ url: URL) -> Bool {
            var directory: ObjCBool = false
            return fm.fileExists(atPath: url.path, isDirectory: &directory) && directory.boolValue
        }
        func entries(_ url: URL) throws -> [String] {
            try fm.contentsOfDirectory(atPath: url.path).filter { $0 != ".DS_Store" }
        }
        guard fm.fileExists(atPath: folder.path) else { return }
        guard isFolder(folder) else { throw PictureError.notASweep(folder.path) }
        let inside = try entries(folder)
        guard !inside.isEmpty else { return }
        let listed = try? JSONDecoder().decode(Manifest.self, from: Data(contentsOf: folder.appending(path: "manifest.json")))
        guard let manifest = listed else { throw PictureError.notASweep(folder.path) }
        let scenes = Set(manifest.scenes.map(\.id))
        let pictures = Set(manifest.scenes.flatMap(\.pictures).map(\.file))
        for entry in inside where entry != "manifest.json" {
            let scene = folder.appending(path: entry)
            guard scenes.contains(entry), isFolder(scene),
                  try entries(scene).allSatisfy({ pictures.contains("\(entry)/\($0)") && !isFolder(scene.appending(path: $0)) })
            else { throw PictureError.notASweep(folder.path) }
        }
    }
}

/// The notes a scene's bands handed up as SwiftUI laid it out.
@MainActor
private final class NoteTaker {
    var notes: [Note] = []
}
