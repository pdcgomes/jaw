import Accelerate
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Where the pictures' files are read from: jawd's folder, which the rows' paths are relative to,
/// or pictures held in memory by path, as the harness and the tests draw their own.
public enum PictureFiles: Sendable, Hashable {
    case folder(String)
    case memory([String: Data])
    /// Before jawd has said where its folder is.
    case none

    func source(_ path: String) -> CGImageSource? {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        switch self {
        case let .folder(root):
            let whole = path.hasPrefix("/") ? path : (root as NSString).appendingPathComponent(path)
            return CGImageSourceCreateWithURL(URL(fileURLWithPath: whole) as CFURL, options)
        case let .memory(files):
            return files[path].flatMap { CGImageSourceCreateWithData($0 as CFData, options) }
        case .none:
            return nil
        }
    }
}

/// Decoding, off the main thread: a preview read with ImageIO at no more than the pixels it's drawn
/// at, then drawn into a bitmap of exactly those pixels in the colours its key asks for, so the cell
/// that draws it copies it rather than scaling or converting it; and a thumbnail blurred to stand in
/// for its preview.
enum Decode {
    /// The picture `key` names, from `files`, or nil when it can't be read, or isn't a JPEG where only
    /// one is read.
    static func picture(_ key: PictureKey, from files: PictureFiles) -> CGImage? {
        guard let source = files.source(key.path), !key.jpeg || CGImageSourceGetType(source) as String? == UTType.jpeg.identifier,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              var sw = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue,
              var sh = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue, sw > 0, sh > 0
        else { return nil }
        if let orientation = (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue, orientation >= 5 {
            swap(&sw, &sh)
        }
        let (tw, th) = (Double(key.width), Double(key.height))
        let most = longSide(sw, sh, key)
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: most,
        ] as CFDictionary
        guard let decoded = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else { return nil }
        guard let ctx = CGContext(data: nil, width: key.width, height: key.height, bitsPerComponent: 8, bytesPerRow: 0, space: key.space.space,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        ctx.interpolationQuality = .high
        let (dw, dh) = (Double(decoded.width), Double(decoded.height))
        let s = key.fit == .fill ? max(tw / dw, th / dh) : min(tw / dw, th / dh)
        let drawn = CGRect(x: (tw - dw * s) / 2, y: (th - dh * s) / 2, width: dw * s, height: dh * s)
        ctx.draw(decoded, in: drawn)
        if key.blurred { blur(ctx, key) }
        return ctx.makeImage()
    }

    /// The long side ImageIO decodes a picture `sw` by `sh` at for `key`: no larger than the file, nor
    /// than its place needs, its short side filling the place, or all of it fitting; and a picture
    /// more than four times as long as its place, which shows only its middle, at four times.
    static func longSide(_ sw: Double, _ sh: Double, _ key: PictureKey) -> Int {
        let (tw, th) = (Double(key.width), Double(key.height))
        let scale = key.fit == .fill ? max(tw / sw, th / sh) : min(tw / sw, th / sh)
        let most = max(1, Int((max(sw, sh) * min(scale, 1)).rounded(.up)))
        return min(most, 4 * max(key.width, key.height))
    }

    /// Blurs a bitmap in place, a tent's width about a twentieth of its shorter side, three times
    /// over, which is close to a Gaussian's and comes out the same on every run.
    static func blur(_ ctx: CGContext, _ key: PictureKey) {
        guard let data = ctx.data else { return }
        let side = UInt32(max(3, (min(key.width, key.height) / 20) | 1))
        var buffer = vImage_Buffer(data: data, height: vImagePixelCount(key.height), width: vImagePixelCount(key.width),
                                   rowBytes: ctx.bytesPerRow)
        guard let scratch = malloc(ctx.bytesPerRow * key.height) else { return }
        defer { free(scratch) }
        var other = vImage_Buffer(data: scratch, height: buffer.height, width: buffer.width, rowBytes: ctx.bytesPerRow)
        for _ in 0 ..< 3 {
            vImageTentConvolve_ARGB8888(&buffer, &other, nil, 0, 0, side, side, nil, vImage_Flags(kvImageEdgeExtend))
            swap(&buffer.data, &other.data)
        }
        // After three passes the blurred pixels are in what started as the scratch buffer.
        if buffer.data != data { memcpy(data, buffer.data, ctx.bytesPerRow * key.height) }
    }

    /// A picture's bytes in memory, for the bench's and the tests' accounting.
    static func bytes(_ image: CGImage) -> Int {
        image.bytesPerRow * image.height
    }
}
