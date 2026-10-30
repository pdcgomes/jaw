import CoreGraphics
import Foundation
import ImageIO
import Synchronization

/// Where a message's pictures come from, by the paths jawd sends: jawd's folder in the app, and
/// pictures drawn for the purpose in the harness and the tests.
public protocol PictureSource: Sendable {
    /// What tells this source from another, so a layout made with one isn't taken for another's.
    var id: String { get }
    /// The picture at `path` under jawd's root, decoded to fill `pixels` across, or nil when it can't
    /// be read.
    func picture(_ path: String, pixels: Int) -> CGImage?
}

/// The pictures in jawd's folder, decoded with ImageIO on whichever thread asks, the layout queue's
/// mostly, and kept, so a message laid out again at another width doesn't read its file again. A
/// link's thumbnail is a few kilobytes, so at most 64 are kept. A file that can't be read isn't
/// remembered, so one jawd keeps later is read then.
public final class FolderPictures: PictureSource {
    public let root: String
    let kept = Mutex<[String: CGImage]>([:])
    static let most = 64

    public init(root: String) {
        self.root = root
    }

    public var id: String {
        root
    }

    public func picture(_ path: String, pixels: Int) -> CGImage? {
        let key = "\(pixels) \(path)"
        if let image = kept.withLock({ $0[key] }) { return image }
        let url = URL(fileURLWithPath: root).appending(path: path)
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary) else { return nil }
        // Three times across on its long side, so a picture up to three times as wide as it's tall
        // still fills a square without being drawn larger than it was decoded.
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: max(pixels, 1) * 3,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        kept.withLock { kept in
            if kept.count >= Self.most { kept.removeAll() }
            kept[key] = image
        }
        return image
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
