import CoreGraphics
import Foundation

/// The pictures a window draws: the image queue, the cache of what it's decoded, and where the files
/// are. A slot draws from the best preview its pixels want once that's decoded at its size, from
/// another size of it meanwhile, then from its blurred thumbnail, and otherwise from nothing, the
/// layout's well showing where it will be.
@MainActor
public final class MessagePictures {
    public let cache: PictureCache
    public let queue: PictureQueue

    public init(files: PictureFiles = .none, cache: PictureCache = PictureCache(), mode: PictureQueue.Mode = .queued(PictureQueue.most)) {
        self.cache = cache
        queue = PictureQueue(cache: cache, mode: mode)
        queue.files = files
    }

    /// jawd's folder, from hello, or the harness's pictures.
    public var files: PictureFiles {
        get { queue.files }
        set { queue.files = newValue }
    }

    /// A file of jawd's by the path a row names it by, once jawd has said where its folder is.
    public func url(_ path: String) -> URL? {
        guard case let .folder(root) = files else { return nil }
        return path.hasPrefix("/") ? URL(fileURLWithPath: path) : URL(fileURLWithPath: root).appending(path: path)
    }

    /// What a slot can be drawn with now.
    public struct Drawable {
        public let image: CGImage
        /// A preview rather than the blurred thumbnail.
        public let isPreview: Bool
        /// At the slot's own pixels, rather than scaled from another size meanwhile.
        public let exact: Bool
    }

    public func drawable(_ slot: PictureSlot, scale: CGFloat) -> Drawable? {
        let (previews, thumbnail) = slot.keys(scale)
        for key in previews {
            guard let image = cache.image(key) else { continue }
            // Its thumbnail isn't needed once its preview is drawn; it's made again if that goes.
            if let thumbnail { cache.drop(thumbnail) }
            return Drawable(image: image, isPreview: true, exact: true)
        }
        for key in previews {
            if let image = cache.another(key) { return Drawable(image: image, isPreview: true, exact: false) }
        }
        guard let thumbnail else { return nil }
        if let image = cache.image(thumbnail) { return Drawable(image: image, isPreview: false, exact: true) }
        return cache.another(thumbnail).map { Drawable(image: $0, isPreview: false, exact: false) }
    }

    /// Asks for what the slot lacks at `scale`, for `token`: its best preview that can be read, and,
    /// while no preview of it can be drawn, its blurred thumbnail. `came` runs on the main thread as
    /// each comes; nothing is asked twice, nor anything while the cache is closed.
    public func ask(_ slot: PictureSlot, scale: CGFloat, for token: PictureToken, came: @escaping @MainActor @Sendable () -> Void) {
        guard !cache.isClosed else { return }
        let (previews, thumbnail) = slot.keys(scale)
        let best = previews.first { !queue.failed($0) }
        let previewDrawn = drawable(slot, scale: scale)?.isPreview == true
        if let thumbnail, !previewDrawn, !cache.has(thumbnail), !queue.failed(thumbnail) {
            queue.ask(thumbnail, for: token) { _ in came() }
        }
        if let best, !cache.has(best) {
            queue.ask(best, for: token) { _ in came() }
        }
    }

    public func letGo(_ token: PictureToken) {
        queue.letGo(token)
    }

    public func token() -> PictureToken {
        queue.token()
    }
}
