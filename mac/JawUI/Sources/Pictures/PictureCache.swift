import CoreGraphics
import Synchronization

/// The decoded pictures, kept by their bytes up to a limit, 64 MB by the design's budget, the one
/// drawn least recently going first; the files stay in jawd's cache, so what goes is decoded again
/// when it's drawn again.
public final class PictureCache: Sendable {
    public static let most = 64 << 20

    public let limit: Int
    let state: Mutex<State>

    struct State {
        var kept: [PictureKey: Kept] = [:]
        /// Each key's sizes, so a slot can be drawn scaled from another while its own is decoded.
        var families: [PictureKey.Family: Set<PictureKey>] = [:]
        var bytes = 0
        var peak = 0
        var tick: UInt64 = 0
    }

    struct Kept {
        let image: CGImage
        let bytes: Int
        var drawn: UInt64
    }

    public init(limit: Int = PictureCache.most) {
        self.limit = limit
        state = Mutex(State())
    }

    /// The picture, if it's kept; drawing it counts as its use.
    public func image(_ key: PictureKey) -> CGImage? {
        state.withLock { s in
            guard s.kept[key] != nil else { return nil }
            s.tick += 1
            s.kept[key]!.drawn = s.tick
            return s.kept[key]!.image
        }
    }

    /// The same picture at another size, for drawing it scaled meanwhile; nothing counts as used.
    public func another(_ key: PictureKey) -> CGImage? {
        state.withLock { s in
            s.families[key.family]?.compactMap { s.kept[$0] }.max { $0.drawn < $1.drawn }?.image
        }
    }

    /// Keeps a decoded picture, letting go of the least recently drawn until the cache is within its
    /// limit again; one larger than the whole limit isn't kept.
    public func keep(_ image: CGImage, as key: PictureKey) {
        let bytes = Decode.bytes(image)
        state.withLock { s in
            guard bytes <= limit else { return }
            if let old = s.kept.removeValue(forKey: key) { s.bytes -= old.bytes }
            s.tick += 1
            s.kept[key] = Kept(image: image, bytes: bytes, drawn: s.tick)
            s.families[key.family, default: []].insert(key)
            s.bytes += bytes
            while s.bytes > limit, let oldest = s.kept.min(by: { $0.value.drawn < $1.value.drawn })?.key {
                s.bytes -= s.kept.removeValue(forKey: oldest)!.bytes
                s.families[oldest.family]?.remove(oldest)
                if s.families[oldest.family]?.isEmpty == true { s.families[oldest.family] = nil }
            }
            s.peak = max(s.peak, s.bytes)
        }
    }

    /// Lets one picture go, as a slot's thumbnail once its preview is drawn.
    public func drop(_ key: PictureKey) {
        state.withLock { s in
            guard let gone = s.kept.removeValue(forKey: key) else { return }
            s.bytes -= gone.bytes
            s.families[key.family]?.remove(key)
            if s.families[key.family]?.isEmpty == true { s.families[key.family] = nil }
        }
    }

    public func has(_ key: PictureKey) -> Bool {
        state.withLock { $0.kept[key] != nil }
    }

    /// Lets every picture go, as the app does when it has no window.
    public func empty() {
        state.withLock { s in
            s.kept.removeAll()
            s.families.removeAll()
            s.bytes = 0
        }
    }

    public var bytes: Int {
        state.withLock { $0.bytes }
    }

    /// The most it has held at once, for the bench.
    public var peak: Int {
        state.withLock { $0.peak }
    }

    public var count: Int {
        state.withLock { $0.kept.count }
    }
}
