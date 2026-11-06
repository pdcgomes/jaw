import AppKit
@testable import JawKit
@testable import JawUI
import Testing

/// What's attached to a box, as a value: its files in the order they came, each once and 30 at most,
/// SD or HD for its photos and videos, whether jawd needs ffmpeg for it, and the files as `send.media`
/// takes them; and what a paste or a drop brings, read as files.

func file(_ name: String, kind: MediaKind? = nil) -> AttachedFile {
    AttachedFile(url: URL(fileURLWithPath: "/Users/me/Pictures/\(name)"), kind: kind)
}

/// A folder of its own for a test's files, removed when `body` is done.
func inFolder<T>(_ body: (URL) throws -> T) throws -> T {
    let folder = FileManager.default.temporaryDirectory.appending(path: "jaw-attachments-test-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: folder) }
    return try body(folder)
}

@Test func `files keep the order they came in, a file already there isn't added twice, and past 30 the rest are left out and counted`() {
    var a = Attachments()
    #expect(a.isEmpty && a.add([file("lake.jpg"), file("swim.mov")]) == 0)
    #expect(a.add([file("swim.mov"), file("notes.pdf", kind: .document)]) == 0, "the second swim.mov is the first")
    #expect(a.files.map(\.name) == ["lake.jpg", "swim.mov", "notes.pdf"])
    #expect(a.add((0 ..< 30).map { file("photo \($0).jpg") }) == 3, "27 fit beside the three, and three are left out")
    #expect(a.files.count == SendMedia.filesMost && a.files.last?.name == "photo 26.jpg")
    a.remove(at: 1)
    #expect(a.files.count == 29 && a.files.map(\.name).prefix(2) == ["lake.jpg", "notes.pdf"])
    a.removeAll()
    #expect(a.isEmpty)
}

@Test func `SD or HD shows only with a photo or a video, and ffmpeg is needed for a video, a GIF or an audio file sent as what it is`() {
    #expect(file("lake.jpg").isVisual && file("IMG_2041.HEIC").isVisual && file("swim.mov").isVisual)
    #expect(!file("dance.gif").isVisual && !file("notes.pdf").isVisual && !file("song.m4a").isVisual)
    #expect(!file("lake.jpg", kind: .document).isVisual, "a photo sent as a document goes as it is")
    #expect(file("swim.mov").needsFFmpeg && file("dance.gif").needsFFmpeg && file("song.m4a").needsFFmpeg)
    #expect(!file("lake.jpg").needsFFmpeg && !file("notes.pdf").needsFFmpeg && !file("swim.mov", kind: .document).needsFFmpeg)

    var a = Attachments()
    a.add([file("notes.pdf", kind: .document), file("song.m4a")])
    #expect(!a.showsQuality && a.needsFFmpeg)
    a.add([file("lake.jpg")])
    #expect(a.showsQuality)
}

@Test func `the caption goes on the first file alone, its end trimmed, none when it's blank, and a document keeps its kind`() {
    var a = Attachments()
    a.add([file("notes.pdf", kind: .document), file("lake.jpg")])
    #expect(a.params(caption: "  the lake\non Sunday \n") == [
        SendMedia.File(path: "/Users/me/Pictures/notes.pdf", kind: .document, caption: "  the lake\non Sunday"),
        SendMedia.File(path: "/Users/me/Pictures/lake.jpg"),
    ])
    #expect(a.params(caption: " \n\t").map(\.caption) == [nil, nil])
}

@Test func `discarding takes only the folder the app made for a file, never a file from the Finder or one beside it`() throws {
    try inFolder { folder in
        let finder = folder.appending(path: "lake.jpg")
        try Data([1]).write(to: finder)
        AttachedFile.discard([AttachedFile(url: finder)])
        #expect(FileManager.default.fileExists(atPath: finder.path), "the Finder's file stays where it is")

        // Two files a drag promised, received into one folder as AppKit asks, each then moved to its own.
        let landing = folder.appending(path: "landing")
        try FileManager.default.createDirectory(at: landing, withIntermediateDirectories: true)
        try Data([2]).write(to: landing.appending(path: "IMG_1.HEIC"))
        try Data([3]).write(to: landing.appending(path: "IMG_2.HEIC"))
        let one = try #require(ComposerController.alone(landing.appending(path: "IMG_1.HEIC"), in: folder))
        let two = try #require(ComposerController.alone(landing.appending(path: "IMG_2.HEIC"), in: folder))
        #expect(one.written && one.folder != two.folder && one.url.deletingLastPathComponent() == one.folder)
        AttachedFile.discard([one])
        #expect(!FileManager.default.fileExists(atPath: one.url.path) && FileManager.default.fileExists(atPath: two.url.path),
                "taking one out leaves the other")
    }
}

@MainActor @Test func `a pasteboard of files gives them, folders left out, one of an image writes it as a PNG, and one of text gives nothing`() throws {
    let board = NSPasteboard(name: .init("jaw-test-\(UUID().uuidString)"))
    defer { board.releaseGlobally() }
    try inFolder { folder in
        let lake = folder.appending(path: "lake.jpg"), notes = folder.appending(path: "notes.pdf"), inside = folder.appending(path: "trip")
        try Data([1]).write(to: lake)
        try Data([2]).write(to: notes)
        try FileManager.default.createDirectory(at: inside, withIntermediateDirectories: true)
        board.clearContents()
        board.writeObjects([lake as NSURL, inside as NSURL, notes as NSURL])
        #expect(Pasted.attachments(from: board, into: folder)?.map(\.name) == ["lake.jpg", "notes.pdf"])
        #expect(Pasted.attachments(from: board, into: folder)?.allSatisfy { !$0.written && $0.kind == nil } == true)

        let red = NSImage(size: NSSize(width: 4, height: 3), flipped: false) { r in
            NSColor.red.setFill()
            r.fill()
            return true
        }
        board.clearContents()
        board.writeObjects([red])
        let pasted = try #require(Pasted.attachments(from: board, into: folder))
        #expect(pasted.count == 1 && pasted[0].written && pasted[0].url.pathExtension == "png")
        #expect(pasted[0].url.path.hasPrefix(folder.path), "written into the folder given")
        let read = try #require(NSBitmapImageRep(data: Data(contentsOf: pasted[0].url)))
        #expect(read.pixelsWide >= 4 && read.pixelsHigh >= 3 && read.pixelsWide * 3 == read.pixelsHigh * 4)

        board.clearContents()
        board.setString("the lake on Sunday", forType: .string)
        #expect(Pasted.attachments(from: board, into: folder) == nil, "text pastes as text")
    }
}
