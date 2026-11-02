import AppKit
import Darwin
import JawKit
import JawUI

/// `--bench library`: a fling through the library's grid, against the design's "Scrolling with
/// photos". Once jawd demo has the lake's previews, as `--bench pictures` waits for them, the bench
/// clones them as the previews of 3,000 photos an hour apart, each picture files of its own so each
/// is decoded, and shows them in a library pane of photos in a window 900 points square, seven tiles
/// across, drawing from the app's pictures. It answers the pane as jawd would, 4 ms after each move,
/// with what came into the window and what left it, one move at a time and only the latest, as
/// JawKit's client sends them, and flings at 6,000 points a second to the oldest and back, timing
/// every turn of the main run loop and each move's landing, then says the footprint after the fling.
/// The clones are removed before it says what it found.
@MainActor
enum BenchLibrary {
    static let photos = 3000
    static let speed: CGFloat = 6000
    static let answer: TimeInterval = 0.004

    static func run(_ model: AppModel) {
        model.firstRows = {
            guard let chat = model.window?.chatList.all.first(where: { $0.id == BenchPictures.ana }) else { return Bench.finish(["error": 4]) }
            model.open(chat, opened: false)
            waitForLake(model, tries: 0)
        }
    }

    /// Waits for jawd to have the lake here with both previews and its thumbnail.
    static func waitForLake(_ model: AppModel, tries: Int) {
        let media = model.panes[BenchPictures.ana]?.window.get(BenchPictures.lake)?.media
        guard let media, let small = media.preview(256).flatMap({ model.pictures.url($0) }),
              let large = media.preview(768).flatMap({ model.pictures.url($0) }), let tiny = media.thumbnail.flatMap({ model.pictures.url($0) })
        else {
            if tries > 300 { return Bench.finish(["error": 5]) }
            return DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { waitForLake(model, tries: tries + 1) }
        }
        // In the run's own folder, which is emptied as it quits, even when its deadline ends it.
        let folder = Opened.folder.appending(path: "bench-library", directoryHint: .isDirectory)
        do {
            show(try made(in: folder, from: (small, large, tiny)), model: model, folder: folder)
        } catch {
            try? FileManager.default.removeItem(at: folder)
            Bench.finish(["error": 6])
        }
    }

    /// The photos, newest first, an hour apart, each with clones of the lake's previews and thumbnail.
    static func made(in folder: URL, from lake: (URL, URL, URL)) throws -> [LibraryRow] {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return try (0 ..< photos).map { i in
            var paths: [String] = []
            for (file, size) in [(lake.0, "256"), (lake.1, "768"), (lake.2, "instant")] {
                let to = folder.appending(path: "l\(i).\(size).jpg")
                guard clonefile(file.path, to.path, 0) == 0 else { throw POSIXError(.EIO) }
                paths.append(to.path)
            }
            let at = Samples.now - Int64(i) * 3_600_000
            return LibraryRow(id: String(format: "BENCH%04d", i), v: 1, sort: "\(at)~" + String(format: "%07d", i), kind: .photo, chat: "bench",
                              chatName: "Pictures", at: at,
                              media: Media(w: 4032, h: 3024, bytes: 2_400_000, mime: "image/jpeg", previews: ["256": paths[0], "768": paths[1]],
                                           thumbnail: paths[2], path: paths[1], state: .local))
        }
    }

    /// jawd's side of the pane: every photo, and the pane's window of them, watched and moved as the
    /// pane asks, one at a time and only the latest asked while one is on its way.
    final class Served: LibraryHost {
        let all: [LibraryRow]
        let index: [String: Int]
        var held: Set<String> = []
        var asking: Place?
        var next: Place?
        var seq: UInt64 = 0
        var landings: [Double] = []
        var landed: (() -> Void)?
        /// The pane, held for as long as the bench, since its window holds only its view.
        var pane: LibraryController?

        init(_ all: [LibraryRow]) {
            self.all = all
            index = Dictionary(uniqueKeysWithValues: all.enumerated().map { ($1.id, $0) })
        }

        func library(_: LibraryController, watch _: LibraryFilter, _ place: Place) {
            held = []
            asking = nil
            ask(place)
        }

        func library(_: LibraryController, move place: Place) {
            ask(place)
        }

        func ask(_ place: Place) {
            guard asking == nil else {
                next = place
                return
            }
            asking = place
            DispatchQueue.main.asyncAfter(deadline: .now() + answer) { [weak self] in
                MainActor.assumeIsolated { self?.land(place) }
            }
        }

        func land(_ place: Place) {
            let window = Self.window(all, index, place)
            let ids = Set(window.map(\.id))
            seq += 1
            let update = Update(seq: seq, remove: held.subtracting(ids).sorted(), upsert: window.filter { !held.contains($0.id) },
                                total: UInt64(all.count), older: window.last?.id != all.last?.id, moved: Moved(place: place, newer: 0),
                                counts: LibraryCounts(photo: all.count), note: .some(nil))
            held = ids
            let began = CACurrentMediaTime()
            pane?.apply(update)
            landings.append((CACurrentMediaTime() - began) * 1000)
            asking = nil
            landed?()
            if let waiting = next {
                next = nil
                if waiting != place { ask(waiting) }
            }
        }

        /// The photos `place` holds: from the newest, or `before` newer and `after` older around one.
        static func window(_ all: [LibraryRow], _ index: [String: Int], _ place: Place) -> ArraySlice<LibraryRow> {
            guard case let .row(id) = place.around, let i = index[id] else { return all.prefix(Int(place.after)) }
            return all[max(i - Int(place.before), 0) ..< min(i + Int(place.after) + 1, all.count)]
        }
    }

    /// The pane, watched from the top; the fling once its first screen's pictures have had a second.
    static func show(_ rows: [LibraryRow], model: AppModel, folder: URL) {
        let c = LibraryController(scope: .every, style: model.style, kind: .photo)
        c.pictures = model.pictures
        let served = Served(rows)
        served.pane = c
        c.host = served
        let window = NSWindow(contentRect: NSRect(x: 80, y: 80, width: 900, height: 900), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = c.view
        window.makeKeyAndOrderFront(nil)
        served.landed = {
            served.landed = nil
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                MainActor.assumeIsolated { fling(c, served: served, model: model, window: window, folder: folder) }
            }
        }
        c.ask()
    }

    /// The fling, then what it found.
    static func fling(_ c: LibraryController, served: Served, model: AppModel, window: NSWindow, folder: URL) {
        let landed = served.landings.count
        measure(c) { turns, seconds in
            found(turns, seconds, served, from: landed, model: model, window: window, folder: folder)
        }
    }

    /// Flings from the newest to the oldest and back at `speed`, on a timer at 120 Hz, timing each
    /// turn of the main run loop meanwhile, from its waking to its waiting again.
    static func measure(_ c: LibraryController, done: @escaping @MainActor ([Double], Double) -> Void) {
        final class Run: @unchecked Sendable {
            var turns: [Double] = []
            var began: Double = 0
            var down = true
            var last = CACurrentMediaTime()
            var observers: [CFRunLoopObserver] = []
            var timer: Timer?
            weak var pane: LibraryController?
        }
        let run = Run()
        run.pane = c
        let woke = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.afterWaiting.rawValue, true, CFIndex.min) { _, _ in
            run.began = CACurrentMediaTime()
        }
        let waits = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, true, CFIndex.max) { _, _ in
            guard run.began > 0 else { return }
            run.turns.append((CACurrentMediaTime() - run.began) * 1000)
            run.began = 0
        }
        run.observers = [woke, waits].compactMap(\.self)
        for observer in run.observers {
            CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        }
        let start = run.last
        run.timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 120, repeats: true) { _ in
            MainActor.assumeIsolated {
                guard let c = run.pane else { return }
                let now = CACurrentMediaTime()
                let g = c.gridShown
                let bottom = max(g.height - g.shown, 0)
                let y = min(max(g.top + (run.down ? 1 : -1) * speed * CGFloat(now - run.last), 0), bottom)
                run.last = now
                if run.down, g.oldest, y >= bottom { run.down = false }
                if !run.down, g.newest, y <= 0 {
                    run.timer?.invalidate()
                    for observer in run.observers {
                        CFRunLoopRemoveObserver(CFRunLoopGetMain(), observer, .commonModes)
                    }
                    return done(run.turns, now - start)
                }
                c.scrollGrid(to: y)
            }
        }
    }

    /// What the fling found, with the footprint and the decoded pictures after it; the pane is let go
    /// of only once its window has let go of its view, which would otherwise lay out without it.
    static func found(_ turns: [Double], _ seconds: Double, _ served: Served, from landed: Int, model: AppModel, window: NSWindow,
                      folder: URL)
    {
        let sorted = turns.sorted(), over = turns.filter { $0 > 1000.0 / 120 }, moves = served.landings[landed...].sorted()
        let found = BenchPictures.Found()
        found.said = ["photos": Double(photos), "fling_ms": Bench.ms(seconds), "turns": Double(turns.count)]
        found.said["p50_ms"] = BenchPictures.percentile(sorted, 0.5)
        found.said["p99_ms"] = BenchPictures.percentile(sorted, 0.99)
        found.said["max_ms"] = sorted.last.map { Bench.ms($0 / 1000) } ?? 0
        found.said["over_frame"] = Double(over.count)
        found.said["hitch_ms_per_s"] = Bench.ms(over.reduce(0) { $0 + $1 - 1000.0 / 120 } / 1000 / seconds)
        found.said["moves"] = Double(moves.count)
        found.said["landing_p50_ms"] = BenchPictures.percentile(moves, 0.5)
        found.said["landing_max_ms"] = moves.last.map { Bench.ms($0 / 1000) } ?? 0
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            MainActor.assumeIsolated {
                found.said["footprint_mb"] = Double(Bench.footprint()) / 1_048_576
                found.said["decoded_peak_mb"] = Double(model.pictures.cache.peak) / 1_048_576
                found.said["decoded_now_mb"] = Double(model.pictures.cache.bytes) / 1_048_576
                let counts = model.pictures.queue.counts
                found.said["decoded"] = Double(counts.decoded)
                found.said["dropped"] = Double(counts.dropped)
                found.said["load"] = BenchPictures.load()
                window.contentView = nil
                window.close()
                served.pane = nil
                try? FileManager.default.removeItem(at: folder)
                Bench.finish(found.said)
            }
        }
    }
}
