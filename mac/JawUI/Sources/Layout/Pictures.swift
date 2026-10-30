import CoreGraphics
import Foundation
import ImageIO
import Synchronization
import UniformTypeIdentifiers

/// Where a message's pictures come from, by the paths jawd sends: jawd's folder in the app, and
/// pictures drawn for the purpose in the harness and the tests.
public protocol ThumbnailSource: Sendable {
    /// What tells this source from another, so a layout made with one isn't taken for another's.
    var id: String { get }
    /// The picture at `path` under jawd's root, decoded so its short side is `pixels`, to fill a
    /// square that many across, or nil when it can't be read.
    func picture(_ path: String, pixels: Int) -> CGImage?
}

/// The pictures in jawd's folder, decoded with ImageIO on whichever thread asks, the layout queue's
/// mostly, and kept, so a message laid out again at another width, or in another pane, doesn't read
/// its file again: on a live resize every row is laid out again on the main thread. A link's
/// thumbnail decoded for its square takes 78 KB, so the 8 MB kept hold a hundred of them; past that
/// the least recently drawn go first. A file that can't be read isn't remembered, so one jawd keeps
/// later is read then.
public final class FolderPictures: ThumbnailSource {
    public let root: String
    /// The most bytes kept decoded.
    let most: Int
    let kept = Mutex(Kept())

    public init(root: String, most: Int = 8 << 20) {
        self.root = root
        self.most = most
    }

    public var id: String {
        root
    }

    public func picture(_ path: String, pixels: Int) -> CGImage? {
        let key = "\(pixels) \(path)"
        if let image = kept.withLock({ $0.use(key) }) { return image }
        let url = URL(fileURLWithPath: root).appending(path: path)
        // jawd keeps a thumbnail someone sent as it came, so only bytes that read as the JPEG the
        // protocol says these are get decoded.
        let reading: [CFString: Any] = [kCGImageSourceShouldCache: false, kCGImageSourceTypeIdentifierHint: UTType.jpeg.identifier]
        guard let source = CGImageSourceCreateWithURL(url as CFURL, reading as CFDictionary),
              CGImageSourceGetType(source) as String? == UTType.jpeg.identifier,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let w = properties[kCGImagePropertyPixelWidth] as? Int, let h = properties[kCGImagePropertyPixelHeight] as? Int
        else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: Self.longSide(w, h, short: max(pixels, 1)),
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        kept.withLock { $0.keep(image, as: key, most: most) }
        return image
    }

    /// The long side to decode a picture `w` by `h` at, so its short side comes to `short` and it
    /// fills a square that size from its centre: ImageIO never makes one larger than it is, and one
    /// more than four times as long as it's wide is decoded at four times, as its square shows only
    /// its middle.
    static func longSide(_ w: Int, _ h: Int, short: Int) -> Int {
        let long = max(w, h, 1), shortest = max(min(w, h), 1)
        let filling = shortest > short ? (long * short + shortest - 1) / shortest : long
        return min(filling, short * 4)
    }

    /// The pictures decoded, by size and path, with when each was last drawn and the bytes they take.
    struct Kept {
        var pictures: [String: (image: CGImage, bytes: Int, used: UInt64)] = [:]
        var bytes = 0
        var clock: UInt64 = 0

        /// The picture kept as `key`, drawn now.
        mutating func use(_ key: String) -> CGImage? {
            guard let kept = pictures[key] else { return nil }
            clock += 1
            pictures[key]?.used = clock
            return kept.image
        }

        /// Keeps `image` as `key`, the least recently drawn going until all are within `most`; one
        /// larger than `most` by itself isn't kept.
        mutating func keep(_ image: CGImage, as key: String, most: Int) {
            let size = image.bytesPerRow * image.height
            guard size <= most else { return }
            if let replaced = pictures.removeValue(forKey: key) { bytes -= replaced.bytes }
            while bytes + size > most, let oldest = pictures.min(by: { $0.value.used < $1.value.used }) {
                pictures[oldest.key] = nil
                bytes -= oldest.value.bytes
            }
            clock += 1
            pictures[key] = (image, size, clock)
            bytes += size
        }
    }
}

enum Fill {
    /// Where a picture of `size` is drawn to fill `rect` from its centre, keeping its shape.
    static func rect(_ size: CGSize, filling rect: CGRect) -> CGRect {
        guard size.width > 0, size.height > 0 else { return rect }
        let scale = max(rect.width / size.width, rect.height / size.height)
        let w = size.width * scale, h = size.height * scale
        return CGRect(x: rect.midX - w / 2, y: rect.midY - h / 2, width: w, height: h)
    }
}
