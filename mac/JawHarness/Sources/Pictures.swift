import AppKit
import CryptoKit
import JawKit
import JawUI
import SwiftUI

/// Scenes drawn into pictures by their own views, off the screen, so no Screen Recording is needed:
/// one for `--snapshot`, and every scene in each of its variants for `--sweep`.
@MainActor
enum Pictures {
    /// Pictures are drawn at 2×, as a Retina screen shows them.
    static let scale: CGFloat = 2
    /// How long a scene is given before it's drawn, for what it does once it's made that says nothing
    /// when it's done: an empty conversation's Mimic, two turns after it's made, and what AppKit draws
    /// once it finds the window isn't key, such as the sidebar's folders in grey. A conversation given
    /// rows is waited for after it, until it has drawn them.
    static let settle = Duration.milliseconds(400)
    /// How long a picture's conversations have to draw their rows, from when it's made.
    nonisolated static let limit = Duration.seconds(10)

    enum PictureError: Error, CustomStringConvertible {
        case noScene(String)
        case cantDraw(String)
        case notASweep(String)
        case undrawn(String, Int)

        var description: String {
            switch self {
            case let .noScene(id): "there's no scene called \(id)"
            case let .cantDraw(id): "\(id) couldn't be drawn"
            case let .notASweep(path): "\(path) has files a sweep didn't make in it; sweep into an empty folder, or one a sweep made"
            case let .undrawn(id, waiting):
                "\(id) wasn't drawn: \(waiting) of its conversations hadn't drawn their rows after \(Pictures.limit.components.seconds) s"
            }
        }
    }

    /// A conversation given rows, held weakly, so one SwiftUI made and let go isn't waited for, and
    /// whether it has drawn them.
    @MainActor
    private final class Arrival {
        weak var conversation: ConversationController?
        var drawn = false

        init(_ conversation: ConversationController) {
            self.conversation = conversation
        }
    }

    /// The conversations given rows since the picture being drawn was made.
    private static var arrivals: [Arrival] = []

    /// Gives `conversation` `update` on the next turn, as jawd's rows come once a chat is open. If the
    /// update holds rows, the picture being drawn waits until the conversation has drawn them, which
    /// the first `drawn` after the update says; it takes `drawn`, which nothing else sets on a
    /// conversation the harness makes.
    static func apply(_ update: Update<MessageRow>, to conversation: ConversationController) {
        let arrival = update.upsert.isEmpty ? nil : Arrival(conversation)
        if let arrival {
            arrivals.removeAll { $0.conversation == nil || $0.conversation === conversation }
            arrivals.append(arrival)
        }
        DispatchQueue.main.async {
            if let arrival { conversation.drawn = { arrival.drawn = true } }
            conversation.apply(update)
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
    /// to do what it does after it's made and each conversation in it that was given rows has drawn
    /// them; one that hasn't drawn them within `limit` fails the picture. Once it's drawn, it closes
    /// the window and lets the window's content go, as AppKit can keep a window it has shown after
    /// it's closed. It holds the app-wide `Motion.reduced`, and the list of conversations given rows,
    /// for as long as it draws, so draws mustn't overlap. The window doesn't animate as it comes and
    /// goes: off every screen that animation never ends, and each would hold one of GCD's threads,
    /// which lay a conversation's rows out, for good.
    static func draw(_ scene: HarnessScene, _ conditions: Conditions) async throws -> Drawn {
        let reduced = Motion.reduced
        Motion.reduced = { conditions.reduceMotion }
        defer { Motion.reduced = reduced }
        let deadline = ContinuousClock.now + limit
        arrivals = []
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
        window.animationBehavior = .none
        window.appearance = NSAppearance(named: conditions.dark ? .darkAqua : .aqua)
        window.contentView = host
        window.orderFront(nil)
        defer {
            window.close()
            window.contentView = nil
        }
        try await Task.sleep(for: settle)
        try await rowsDrawn(scene, in: window, by: deadline)
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
        hideScrollers(in: host)
        host.cacheDisplay(in: bounds, to: rep)
        guard let png = rep.representation(using: .png, properties: [:]), let pixels = rep.bitmapData else {
            throw PictureError.cantDraw(scene.id)
        }
        let digest = SHA256.hash(data: Data(bytes: pixels, count: rep.bytesPerRow * rep.pixelsHigh))
        let hash = String(digest.map { String(format: "%02x", $0) }.joined().prefix(16))
        return Drawn(png: png, points: bounds.size, pixels: [rep.pixelsWide, rep.pixelsHigh], hash: hash, notes: taker.notes)
    }

    /// Waits until each conversation given rows that's still alive and in `window` has drawn them; if
    /// one hasn't drawn them by `deadline`, `scene` fails. It lays the window out before each look, as
    /// the picture is laid out before it's drawn, so a conversation SwiftUI hasn't put in the window
    /// yet is waited for too.
    private static func rowsDrawn(_ scene: HarnessScene, in window: NSWindow, by deadline: ContinuousClock.Instant) async throws {
        func undrawn() -> Int {
            window.layoutIfNeeded()
            return arrivals.filter { !$0.drawn && $0.conversation?.viewIfLoaded?.window === window }.count
        }
        var waiting = undrawn()
        while waiting > 0 {
            guard ContinuousClock.now < deadline else { throw PictureError.undrawn(scene.id, waiting) }
            try await Task.sleep(for: .milliseconds(10))
            waiting = undrawn()
        }
    }

    /// At rest the app shows no scroller: `Scrolling.style` makes each a `ThinScroller`, overlaid, which
    /// shows only while its view scrolls. A picture is of the window at rest, however lately it scrolled.
    private static func hideScrollers(in view: NSView) {
        for v in view.subviews {
            if v is ThinScroller { v.isHidden = true } else { hideScrollers(in: v) }
        }
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
    /// `manifest.json` saying what each is. It draws the scenes in the sidebar's order, section by
    /// section, so the manifest and the review canvas's tabs follow the sidebar whatever order the
    /// scenes are registered in. It draws them into the hidden folder `SweepFolder` makes beside
    /// `folder`, `.<name>.sweeping`, and puts that in `folder`'s place only once every picture and the
    /// manifest are written, so a sweep that's stopped or fails leaves the last whole one where it
    /// was, and the next sweep clears what it left. It replaces only a missing or empty folder, or one
    /// a sweep made, as `SweepFolder` checks before it draws and again before it replaces it; one with
    /// anything else in it is left as it is, and the sweep fails.
    @discardableResult
    static func sweep(_ catalog: HarnessCatalog, conditions: Conditions, to folder: URL) async throws -> Manifest {
        let folder = folder.standardizedFileURL
        try SweepFolder.check(folder)
        let sweeping = try SweepFolder.sweeping(beside: folder)
        var scenes: [Manifest.Scene] = []
        for scene in catalog.sections.flatMap({ catalog.scenes(in: $0) }) {
            try FileManager.default.createDirectory(at: sweeping.appending(path: scene.id), withIntermediateDirectories: true)
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
        try SweepFolder.replace(folder, with: sweeping)
        return manifest
    }
}

/// The notes a scene's bands handed up as SwiftUI laid it out.
@MainActor
private final class NoteTaker {
    var notes: [Note] = []
}
