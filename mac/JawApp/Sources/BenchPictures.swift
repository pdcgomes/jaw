import AppKit
import Darwin
import JawKit
import JawUI

/// `--bench pictures`: a fling through a chat of pictures, against the design's "Scrolling with
/// photos". jawd demo has one photo, the lake in Ana's chat, which jawd downloads and previews as it
/// would a camera's; once its previews are made, the bench clones them into a folder of its own as
/// the previews of 120 messages, each picture a file of its own so each is decoded, and shows them
/// as a conversation of photos, albums, stickers and posters, flung through at 6,000 points a
/// second, down and up again, in each look. Every turn of the main run loop is timed, as Redlamp's
/// MainThreadMonitor times them, with the decoded pictures' most bytes and the footprint on a screen
/// of photos. The clones are removed before it says what it found.
@MainActor
enum BenchPictures {
    static let lake = "3EB0DEMO00000008"
    static let ana = "351912345678@s.whatsapp.net"
    static let rows = 120
    static let speed: CGFloat = 6000

    static func run(_ model: AppModel) {
        model.firstRows = {
            guard let chat = model.window?.chatList.all.first(where: { $0.id == ana }) else { return Bench.finish(["error": 4]) }
            model.open(chat, opened: false)
            waitForLake(model, tries: 0)
        }
    }

    /// Waits for jawd to have the lake here with both previews and its thumbnail.
    static func waitForLake(_ model: AppModel, tries: Int) {
        let media = model.panes[ana]?.window.get(lake)?.media
        guard let media, media.preview(256) != nil, media.preview(768) != nil, let thumbnail = media.thumbnail,
              let small = model.pictures.url(media.preview(256)!), let large = model.pictures.url(media.preview(768)!),
              let tiny = model.pictures.url(thumbnail)
        else {
            if tries > 300 { return Bench.finish(["error": 5]) }
            return DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { waitForLake(model, tries: tries + 1) }
        }
        let folder = FileManager.default.temporaryDirectory.appending(path: "jaw-bench-pictures-\(getpid())", directoryHint: .isDirectory)
        do {
            let chat = try made(in: folder, from: (small, large, tiny))
            fling(chat, model: model, folder: folder)
        } catch {
            try? FileManager.default.removeItem(at: folder)
            Bench.finish(["error": 6])
        }
    }

    /// The chat's rows, their previews clones of the lake's, each picture its own files.
    static func made(in folder: URL, from lake: (URL, URL, URL)) throws -> [MessageRow] {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var n = 0
        func picture() throws -> Media {
            n += 1
            var paths: [String] = []
            for (file, size) in [(lake.0, "256"), (lake.1, "768"), (lake.2, "instant")] {
                let to = folder.appending(path: "p\(n).\(size).jpg")
                guard clonefile(file.path, to.path, 0) == 0 else { throw POSIXError(.EIO) }
                paths.append(to.path)
            }
            let shapes = [(4032, 3024), (3024, 4032), (4000, 1500), (2000, 2000), (1600, 1200)]
            let (w, h) = shapes[n % shapes.count]
            return Media(w: w, h: h, bytes: 2_400_000, mime: "image/jpeg", previews: ["256": paths[0], "768": paths[1]], thumbnail: paths[2],
                         path: paths[1], state: .local)
        }
        let at = Samples.now - Int64(rows) * 60000
        var out: [MessageRow] = []
        for i in 0 ..< rows {
            let kinds: [MessageKind] = [.image, .album, .image, .sticker, .video, .image, .gif, .videoNote]
            let kind = kinds[i % kinds.count]
            var m = MessageRow(id: "BENCH\(i)", v: 1, sort: String(format: "%013d~%06d", at + Int64(i) * 60000, i), chat: "bench",
                               at: at + Int64(i) * 60000, from: i % 3 == 0 ? nil : Sender(id: "person:4", name: "Ana", colour: 4),
                               mine: i % 3 == 0, kind: kind, spans: i % 4 == 1 ? [Span(.text, "at the lake, again")] : [])
            if kind == .album {
                var items: [Album.Item] = []
                for j in 0 ..< 4 {
                    items.append(Album.Item(id: "BENCH\(i)-\(j)", kind: "image", media: try picture()))
                }
                m.album = Album(items: items)
            } else {
                m.media = try picture()
                if kind == .video || kind == .videoNote { m.media?.seconds = 42 }
            }
            out.append(m)
        }
        return out
    }

    /// The fling, in each look, then what it found.
    static func fling(_ rows: [MessageRow], model: AppModel, folder: URL) {
        let chat = ChatRow(id: "bench", v: 1, sort: "1", name: "Pictures")
        let c = ConversationController(chat: chat, spec: model.spec, layouts: model.layouts, place: .newest(UInt32(rows.count)))
        c.pictures = model.pictures
        let window = NSWindow(contentRect: NSRect(x: 80, y: 80, width: 900, height: 900), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = c.view
        window.makeKeyAndOrderFront(nil)
        c.drawn = { MainActor.assumeIsolated { flung(c, rows: rows, model: model, window: window, folder: folder) } }
        c.apply(Update(seq: 1, upsert: rows, total: UInt64(rows.count), older: false, newer: 0,
                       moved: Moved(place: .newest(UInt32(rows.count)), newer: 0)))
    }

    /// What the bench found, as it goes.
    final class Found: @unchecked Sendable {
        var said: [String: Double] = [:]
    }

    /// Once the chat's rows are in its table: the fling in bubbles, then in lines, then the footprint.
    static func flung(_ c: ConversationController, rows: [MessageRow], model: AppModel, window: NSWindow, folder: URL) {
        let found = Found()
        found.said = ["rows": Double(rows.count), "pictures": Double(rows.reduce(0) { $0 + ($1.album?.items.count ?? 1) })]
        look(0, c, model: model, found: found) {
            // A screen of photos, decoded and settled, for the footprint.
            c.scrollTranscript(to: 0)
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                MainActor.assumeIsolated {
                    found.said["footprint_mb"] = Double(Bench.footprint()) / 1_048_576
                    found.said["decoded_peak_mb"] = Double(model.pictures.cache.peak) / 1_048_576
                    found.said["decoded_now_mb"] = Double(model.pictures.cache.bytes) / 1_048_576
                    let counts = model.pictures.queue.counts
                    found.said["decoded"] = Double(counts.decoded)
                    found.said["dropped"] = Double(counts.dropped)
                    found.said["load"] = load()
                    window.orderOut(nil)
                    try? FileManager.default.removeItem(at: folder)
                    Bench.finish(found.said)
                }
            }
        }
    }

    /// Each fling: in bubbles and in lines with the pictures, then in bubbles again with nothing
    /// decoded, their wells alone, to say what the transcript costs before any picture.
    static let looks: [(name: String, look: Look, pictures: Bool)] = [("bubbles", .bubbles, true), ("lines", .lines, true), ("wells", .bubbles, false)]

    /// The fling `i`, and those after it, then `then`.
    static func look(_ i: Int, _ c: ConversationController, model: AppModel, found: Found, then: @escaping @MainActor () -> Void) {
        guard i < looks.count else { return then() }
        var style = model.style
        style.look = looks[i].look
        c.pictures = looks[i].pictures ? model.pictures : MessagePictures()
        c.spec = LayoutSpec(style: style, me: model.me)
        if !looks[i].pictures { c.relayoutForBench() }
        c.scrollTranscript(to: 0)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            MainActor.assumeIsolated {
                let timing = CellTiming()
                CellTiming.now = timing
                measure(c) { turns, seconds in
                    CellTiming.now = nil
                    let name = looks[i].name
                    let draws = timing.drawMs.sorted()
                    found.said["\(name)_draws"] = Double(timing.draws)
                    found.said["\(name)_draw_p99_ms"] = percentile(draws, 0.99)
                    found.said["\(name)_draw_max_ms"] = draws.last.map { Bench.ms($0 / 1000) } ?? 0
                    found.said["\(name)_draw_total_ms"] = Bench.ms(draws.reduce(0, +) / 1000)
                    found.said["\(name)_came"] = Double(timing.came)
                    let sorted = turns.sorted()
                    let over = turns.filter { $0 > 1000.0 / 120 }
                    found.said["\(name)_turns"] = Double(turns.count)
                    found.said["\(name)_p50_ms"] = percentile(sorted, 0.5)
                    found.said["\(name)_p99_ms"] = percentile(sorted, 0.99)
                    found.said["\(name)_max_ms"] = sorted.last.map { Bench.ms($0 / 1000) } ?? 0
                    found.said["\(name)_over_frame"] = Double(over.count)
                    found.said["\(name)_hitch_ms_per_s"] = Bench.ms(over.reduce(0) { $0 + $1 - 1000.0 / 120 } / 1000 / seconds)
                    look(i + 1, c, model: model, found: found, then: then)
                }
            }
        }
    }

    /// Flings from the top to the bottom and back at `speed`, on a timer at 120 Hz, timing each turn
    /// of the main run loop meanwhile, from its waking to its waiting again.
    static func measure(_ c: ConversationController, done: @escaping @MainActor ([Double], Double) -> Void) {
        let bottom = max(c.transcript.height - c.transcript.shown, 0)
        final class Run: @unchecked Sendable {
            var turns: [Double] = []
            var began: Double = 0
            var y: CGFloat = 0
            var down = true
            var last = CACurrentMediaTime()
            var observers: [CFRunLoopObserver] = []
            var timer: Timer?
            weak var conversation: ConversationController?
        }
        let run = Run()
        run.conversation = c
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
                let now = CACurrentMediaTime()
                run.y += (run.down ? 1 : -1) * speed * CGFloat(now - run.last)
                run.last = now
                if run.down, run.y >= bottom { run.down = false }
                if !run.down, run.y <= 0 {
                    run.timer?.invalidate()
                    for observer in run.observers {
                        CFRunLoopRemoveObserver(CFRunLoopGetMain(), observer, .commonModes)
                    }
                    return done(run.turns, now - start)
                }
                run.conversation?.scrollTranscript(to: min(max(run.y, 0), bottom))
            }
        }
    }

    static func percentile(_ sorted: [Double], _ q: Double) -> Double {
        sorted.isEmpty ? 0 : Bench.ms(sorted[min(Int(Double(sorted.count) * q), sorted.count - 1)] / 1000)
    }

    /// The load average over the last minute, which every number here should be read beside.
    static func load() -> Double {
        var loads = [Double](repeating: 0, count: 3)
        return getloadavg(&loads, 3) == 3 ? (loads[0] * 100).rounded() / 100 : -1
    }
}
