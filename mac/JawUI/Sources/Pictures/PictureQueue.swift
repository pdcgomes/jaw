import CoreGraphics
import Foundation
import Synchronization

/// Whoever asks for pictures, a cell by its own number, so what it asked for is let go when it's
/// scrolled past.
public struct PictureToken: Hashable, Sendable {
    let raw: UInt64
}

/// The image queue: pictures decoded on other threads, four at a time, in the order they were asked
/// for; one asked for by cells that have all let it go before it starts is never decoded, so a
/// fling decodes what it stops on, not what it passed. One being decoded when it's let go is still
/// kept, since it's nearly done, but whoever let it go isn't told. `here` decodes on the asking
/// thread, for pictures that must come out whole: the harness's and the tests'.
public final class PictureQueue: Sendable {
    public enum Mode: Sendable, Hashable {
        case queued(Int)
        case here
    }

    public static let most = 4

    public typealias Done = @MainActor @Sendable (PictureKey) -> Void

    let mode: Mode
    let cache: PictureCache
    let state: Mutex<State>
    let workers = DispatchQueue(label: "dev.badcoffee.jaw.pictures", qos: .userInitiated, attributes: .concurrent)

    struct State {
        var files: PictureFiles = .none
        var jobs: [PictureKey: Job] = [:]
        var waiting: [PictureKey] = []
        var running = 0
        var failed: Set<PictureKey> = []
        var next: UInt64 = 0
        var counts = Counts()
    }

    struct Job {
        var waiters: [PictureToken: [Done]] = [:]
        var started = false
    }

    /// What it has done since it was made, for the tests and the bench.
    public struct Counts: Sendable, Hashable {
        public var decoded = 0
        public var failed = 0
        /// Asked for, then let go of by everyone before it started.
        public var dropped = 0
        /// The most decoding at once.
        public var mostAtOnce = 0
    }

    public init(cache: PictureCache, mode: Mode = .queued(PictureQueue.most)) {
        self.cache = cache
        self.mode = mode
        state = Mutex(State())
    }

    public var files: PictureFiles {
        get { state.withLock { $0.files } }
        set {
            state.withLock { s in
                guard s.files != newValue else { return }
                s.files = newValue
                s.failed.removeAll()
            }
        }
    }

    public func token() -> PictureToken {
        state.withLock { s in
            s.next += 1
            return PictureToken(raw: s.next)
        }
    }

    /// Whether the picture couldn't be read, so its slot draws from the next it has.
    public func failed(_ key: PictureKey) -> Bool {
        state.withLock { $0.failed.contains(key) }
    }

    public var counts: Counts {
        state.withLock { $0.counts }
    }

    /// Whether anything is asked for and not yet decoded.
    public var busy: Bool {
        state.withLock { !$0.jobs.isEmpty }
    }

    /// Asks for `key` for `token`; `done` runs on the main thread once it's decoded, or found unreadable.
    public func ask(_ key: PictureKey, for token: PictureToken, done: @escaping Done) {
        if case .here = mode {
            let files = state.withLock { $0.files }
            finish(key, Decode.picture(key, from: files), started: false)
            MainActor.assumeIsolated { done(key) }
            return
        }
        let asked = state.withLock { s -> Bool in
            guard !s.failed.contains(key) else { return false }
            if s.jobs[key] == nil {
                s.jobs[key] = Job()
                s.waiting.append(key)
            }
            s.jobs[key]!.waiters[token, default: []].append(done)
            return true
        }
        if asked { pump() }
    }

    /// Lets go of what `token` asked for: what hasn't started for anyone else is never decoded.
    public func letGo(_ token: PictureToken) {
        state.withLock { s in
            for key in Array(s.jobs.keys) {
                guard s.jobs[key]!.waiters.removeValue(forKey: token) != nil else { continue }
                if s.jobs[key]!.waiters.isEmpty, !s.jobs[key]!.started {
                    s.jobs[key] = nil
                    s.counts.dropped += 1
                }
            }
            s.waiting.removeAll { s.jobs[$0] == nil }
        }
    }

    /// Starts what's waiting, as far as the workers allow.
    func pump() {
        guard case let .queued(most) = mode else { return }
        while true {
            let next = state.withLock { s -> (PictureKey, PictureFiles)? in
                guard s.running < most, !s.waiting.isEmpty else { return nil }
                let key = s.waiting.removeFirst()
                s.jobs[key]!.started = true
                s.running += 1
                s.counts.mostAtOnce = max(s.counts.mostAtOnce, s.running)
                return (key, s.files)
            }
            guard let (key, files) = next else { return }
            workers.async { [self] in
                let image = Decode.picture(key, from: files)
                let told = finish(key, image, started: true)
                DispatchQueue.main.async {
                    MainActor.assumeIsolated {
                        for done in told {
                            done(key)
                        }
                    }
                }
                pump()
            }
        }
    }

    /// Keeps what was decoded and says who's still waiting for it.
    @discardableResult
    func finish(_ key: PictureKey, _ image: CGImage?, started: Bool) -> [Done] {
        if let image { cache.keep(image, as: key) }
        return state.withLock { s in
            if started { s.running -= 1 }
            if image == nil {
                if s.failed.count > 4096 { s.failed.removeAll() }
                s.failed.insert(key)
                s.counts.failed += 1
            } else {
                s.counts.decoded += 1
            }
            return s.jobs.removeValue(forKey: key)?.waiters.values.flatMap(\.self) ?? []
        }
    }
}
