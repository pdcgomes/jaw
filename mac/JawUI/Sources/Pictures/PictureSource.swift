import CoreGraphics
import JawKit

/// Where a picture is drawn from: jawd's previews, at most 256 and 768 pixels on their long side,
/// and the tiny copy the message brought, which jawd draws at 256 and which is there first; or a
/// link's thumbnail, a preview of 140 pixels jawd keeps as it came.
public struct PictureSource: Sendable, Hashable {
    public var small: String?
    public var large: String?
    public var thumbnail: String?
    /// Only a JPEG is read, as the protocol says a link's thumbnail is: jawd keeps one someone sent as
    /// it came, so bytes of any other kind under its name are refused.
    public var jpeg: Bool

    public init(small: String? = nil, large: String? = nil, thumbnail: String? = nil, jpeg: Bool = false) {
        self.small = small
        self.large = large
        self.thumbnail = thumbnail
        self.jpeg = jpeg
    }

    public init(_ media: Media?) {
        self.init(small: media?.preview(256), large: media?.preview(768), thumbnail: media?.thumbnail)
    }

    /// The previews to try for a picture `pixels` on its long side, best first: jawd's 256-pixel one
    /// for 256 pixels or fewer, the 768 above, as the design has it, then the other. The file itself
    /// is the viewer's alone, so nothing here decodes a full-size photo to draw a list.
    public func previews(for pixels: CGFloat) -> [String] {
        let order = pixels <= 256 ? [small, large] : [large, small]
        return order.compactMap(\.self)
    }

    public var isEmpty: Bool {
        small == nil && large == nil && thumbnail == nil
    }
}

/// How a picture fills its place: cropped from the centre to fill it, as photos and album tiles are,
/// or whole inside it, as a sticker is, its transparency around it.
public enum PictureFit: Sendable, Hashable {
    case fill, fit
}

/// One picture of a message's layout: where it's drawn, how it's clipped, and what it's drawn from.
/// The layout knows its place from the row before any pixel is decoded.
public struct PictureSlot: Sendable, Hashable {
    /// In the layout's coordinates, measured from its top.
    public var rect: CGRect
    /// Its corners' radius; half its width for a video message's circle.
    public var radius: CGFloat
    public var fit: PictureFit
    public var source: PictureSource
    /// The message, and the album's item, it shows.
    public var message: String
    public var item: Int?

    public init(rect: CGRect, radius: CGFloat, fit: PictureFit = .fill, source: PictureSource, message: String, item: Int? = nil) {
        self.rect = rect
        self.radius = radius
        self.fit = fit
        self.source = source
        self.message = message
        self.item = item
    }

    /// Its size in pixels at a display's scale.
    public func pixels(_ scale: CGFloat) -> (width: Int, height: Int) {
        (max(Int((rect.width * scale).rounded()), 1), max(Int((rect.height * scale).rounded()), 1))
    }

    /// The decodes it can be drawn from at `scale`, best first, and its blurred thumbnail, which is
    /// a quarter of its pixels each way, since a blur loses nothing drawn larger and the cache
    /// keeps previews in what that leaves.
    public func keys(_ scale: CGFloat) -> (previews: [PictureKey], thumbnail: PictureKey?) {
        let (w, h) = pixels(scale)
        let previews = source.previews(for: CGFloat(max(w, h))).map {
            PictureKey(path: $0, width: w, height: h, fit: fit, blurred: false, jpeg: source.jpeg)
        }
        return (previews, source.thumbnail.map {
            PictureKey(path: $0, width: max(w / 4, 1), height: max(h / 4, 1), fit: fit, blurred: true, jpeg: source.jpeg)
        })
    }
}

/// A picture decoded at the pixels it's drawn at: its file, its size, how it fills, whether it's a
/// thumbnail blurred to stand in for its preview, and whether only a JPEG is read.
public struct PictureKey: Sendable, Hashable {
    public let path: String
    public let width: Int
    public let height: Int
    public let fit: PictureFit
    public let blurred: Bool
    public let jpeg: Bool

    public init(path: String, width: Int, height: Int, fit: PictureFit, blurred: Bool, jpeg: Bool = false) {
        self.path = path
        self.width = width
        self.height = height
        self.fit = fit
        self.blurred = blurred
        self.jpeg = jpeg
    }

    /// The same picture at any size: what a slot can be drawn from, scaled, while its own size is
    /// being decoded, as in a live resize.
    var family: Family {
        Family(path: path, fit: fit, blurred: blurred)
    }

    struct Family: Hashable {
        let path: String
        let fit: PictureFit
        let blurred: Bool
    }
}
