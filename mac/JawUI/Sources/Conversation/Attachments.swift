import AppKit
import JawKit
import UniformTypeIdentifiers

/// A file waiting in the box's strip to be sent: where it is, what it goes as (nil for what it is,
/// as jawd decides; a document for one picked with Document…), and whether the app wrote it, for an
/// image pasted or dropped that wasn't a file, to go once jawd has its own copy.
public struct AttachedFile: Hashable, Sendable {
    public let url: URL
    public let kind: MediaKind?
    public let written: Bool

    public init(url: URL, kind: MediaKind? = nil, written: Bool = false) {
        self.url = url
        self.kind = kind
        self.written = written
    }

    public var name: String {
        url.lastPathComponent
    }

    /// What it is, by its name's extension.
    var type: UTType? {
        UTType(filenameExtension: url.pathExtension)
    }

    /// A photo or a video, which SD or HD is for; a GIF goes as a loop, and a document as it is.
    public var isVisual: Bool {
        guard kind != .document, let type else { return false }
        return type.conforms(to: .movie) || type.conforms(to: .image) && !type.conforms(to: .gif)
    }

    /// A video, a GIF or an audio file, which jawd makes with ffmpeg; a document goes as it is.
    public var needsFFmpeg: Bool {
        guard kind != .document, let type else { return false }
        return type.conforms(to: .movie) || type.conforms(to: .gif) || type.conforms(to: .audio)
    }
}

/// What's attached to a box: its files in the order they came, each once and as many as one
/// `send.media` takes, and whether its photos and videos go in HD.
public struct Attachments: Hashable, Sendable {
    public private(set) var files: [AttachedFile] = []
    public var hd: Bool

    public init(hd: Bool = false) {
        self.hd = hd
    }

    public var isEmpty: Bool {
        files.isEmpty
    }

    /// Adds `new` after what's there, a file already there not again, as many as fit; says how many
    /// were left out for want of room.
    @discardableResult
    public mutating func add(_ new: [AttachedFile]) -> Int {
        var left = 0
        for a in new where !files.contains(where: { $0.url.standardizedFileURL == a.url.standardizedFileURL }) {
            if files.count < SendMedia.filesMost { files.append(a) } else { left += 1 }
        }
        return left
    }

    public mutating func remove(at i: Int) {
        guard files.indices.contains(i) else { return }
        files.remove(at: i)
    }

    public mutating func removeAll() {
        files.removeAll()
    }

    /// Whether SD or HD shows: there's a photo or a video among them.
    public var showsQuality: Bool {
        files.contains(where: \.isVisual)
    }

    public var needsFFmpeg: Bool {
        files.contains(where: \.needsFFmpeg)
    }

    /// The files as `send.media` takes them: what's typed is the first one's caption, its end
    /// trimmed, and none when it's blank.
    public func params(caption: String) -> [SendMedia.File] {
        let trimmed = String(caption.reversed().drop(while: \.isWhitespace).reversed())
        return files.enumerated().map { i, a in
            SendMedia.File(path: a.url.path, kind: a.kind, caption: i == 0 && !trimmed.isEmpty ? trimmed : nil)
        }
    }
}

/// What a paste or a drop brings, read as files.
public enum Pasted {
    /// What a paste brings: files from the Finder, as they are; or words, which are the field's, so
    /// nil, though a picture of them comes too, as a spreadsheet's copied cells bring one; or an
    /// image that isn't a file, as a screenshot copied, written as a PNG into `folder`.
    public static func attachments(from pasteboard: NSPasteboard, into folder: URL) -> [AttachedFile]? {
        if let files = files(pasteboard) { return files }
        if hasText(pasteboard) { return nil }
        return image(pasteboard, into: folder).map { [$0] }
    }

    /// The files it holds, folders left out; nil when it holds none.
    public static func files(_ pasteboard: NSPasteboard) -> [AttachedFile]? {
        let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        let files = urls.filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == false }
        return files.isEmpty ? nil : files.map { AttachedFile(url: $0) }
    }

    public static func hasText(_ pasteboard: NSPasteboard) -> Bool {
        pasteboard.availableType(from: [.string]) != nil
    }

    /// Its image, written as a PNG into `folder`; nil when it has none.
    public static func image(_ pasteboard: NSPasteboard, into folder: URL) -> AttachedFile? {
        guard pasteboard.availableType(from: [.png, .tiff]) != nil, let png = png(pasteboard),
              let written = try? write(png, named: "Pasted image.png", into: folder)
        else { return nil }
        return AttachedFile(url: written, written: true)
    }

    /// The pasteboard's image as PNG: as it is when it's one, or made from its TIFF.
    static func png(_ pasteboard: NSPasteboard) -> Data? {
        if let png = pasteboard.data(forType: .png) { return png }
        guard let tiff = pasteboard.data(forType: .tiff), let rep = NSBitmapImageRep(data: tiff) else { return nil }
        return rep.representation(using: .png, properties: [:])
    }

    /// Writes `data` as `name` in a folder of its own inside `folder`, so no two writes share a name.
    public static func write(_ data: Data, named name: String, into folder: URL) throws -> URL {
        let own = folder.appending(path: UUID().uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: own, withIntermediateDirectories: true)
        let url = own.appending(path: name)
        try data.write(to: url)
        return url
    }
}
