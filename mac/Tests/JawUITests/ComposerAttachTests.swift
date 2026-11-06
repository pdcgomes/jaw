import AppKit
@testable import JawKit
@testable import JawUI
import Testing

/// The box with files in it: the strip above the field, what Return, Send, Tab and Esc do with
/// them, the 30 it holds at most, files turned down coming back, attaching while editing, and what a
/// paste brings.

@MainActor
final class BoxStub: ComposerDelegate {
    var files: [(files: [AttachedFile], caption: String, hd: Bool, key: String, reply: String?)] = []
    var texts: [String] = []
    var drafts: [String] = []

    func composer(_: ComposerController, send text: String, key _: String, reply _: String?, mentions _: [String]) {
        texts.append(text)
    }

    func composer(_: ComposerController, send files: [AttachedFile], caption: String, hd: Bool, key: String, reply: String?) {
        self.files.append((files, caption, hd, key, reply))
    }

    func composer(_: ComposerController, edit _: String, text _: String) {}
    func composer(_: ComposerController, draft text: String) {
        drafts.append(text)
    }

    func composer(_: ComposerController, typing _: TypingState) {}
    func composerStepIntoMessages(_: ComposerController) {}
    func composer(_: ComposerController, key _: NSEvent) -> Bool {
        false
    }

    func composerScroll(_: ComposerController, pages _: Int) {}
}

/// A box for Ana's chat, laid out and empty, its draft left out, its sends going to the stub.
@MainActor
func box() -> (ComposerController, BoxStub) {
    let c = ComposerController(chat: Samples.direct)
    let stub = BoxStub()
    c.delegate = stub
    c.view.frame = NSRect(x: 0, y: 0, width: 640, height: 200)
    c.roles = spec(.bubbles, dark: true).style.roles
    type("", in: c)
    return (c, stub)
}

@MainActor
func type(_ text: String, in c: ComposerController) {
    c.textView.string = text
    c.changed()
}

@MainActor @Test func `attaching shows the strip and Send, and Return sends the files with the text as their caption, the reply and a key, in SD`() throws {
    let (c, stub) = box()
    #expect(c.strip.isHidden && c.sendButton.isHidden)
    let ana = try #require(Samples.messages.first { !$0.mine && $0.kind == .text })
    c.reply(to: ana)
    c.attach([file("lake.jpg"), file("notes.pdf", kind: .document)])
    #expect(!c.strip.isHidden && !c.sendButton.isHidden, "Send shows with files and no words")
    #expect(c.strip.tiles.map(\.name) == ["lake.jpg", "notes.pdf"])
    type("the lake on Sunday", in: c)
    #expect(c.key(key("\r", code: 36)))
    let sent = try #require(stub.files.first)
    #expect(stub.files.count == 1 && stub.texts.isEmpty, "the files go, not the words alone")
    #expect(sent.files.map(\.name) == ["lake.jpg", "notes.pdf"] && sent.caption == "the lake on Sunday" && !sent.hd)
    #expect(sent.reply == ana.id && sent.key.hasPrefix("jaw-"))
    #expect(c.attachments.isEmpty && c.textView.string.isEmpty && c.strip.isHidden && c.mode == .writing)
    c.flushDraft()
    #expect(stub.drafts.isEmpty, "the caption isn't kept as a draft after it went")
}

@MainActor @Test func `Return sends files with no words, and with neither, nothing`() {
    let (c, stub) = box()
    _ = c.key(key("\r", code: 36))
    #expect(stub.files.isEmpty && stub.texts.isEmpty, "an empty box sends nothing")
    c.attach([file("lake.jpg")])
    #expect(c.key(key("\r", code: 36)))
    #expect(stub.files.map(\.caption) == [""] && stub.texts.isEmpty)
}

@MainActor @Test func `Tab switches SD and HD while there's a photo or a video, ⌥ on Send sends HD, and the default is the setting's`() throws {
    let (c, stub) = box()
    c.attach([file("notes.pdf", kind: .document)])
    #expect(!c.key(key("\t", code: 48)) && !c.attachments.hd, "a document alone has no SD or HD, and Tab is the field's")
    #expect(c.strip.quality.isHidden)
    c.attach([file("lake.jpg")])
    #expect(!c.strip.quality.isHidden && c.strip.quality.title == "SD")
    #expect(c.key(key("\t", code: 48)) && c.attachments.hd && c.strip.quality.title == "HD")
    #expect(c.key(key("\t", code: 48)) && !c.attachments.hd)
    c.send(hd: true)
    #expect(stub.files.map(\.hd) == [true], "⌥ held as Send is clicked")

    c.hdByDefault = true
    c.attach([file("swim.mov")])
    #expect(c.attachments.hd && c.strip.quality.title == "HD")
}

@MainActor @Test func `Esc drops the reply first, then the files`() throws {
    let (c, _) = box()
    let ana = try #require(Samples.messages.first { !$0.mine && $0.kind == .text })
    c.reply(to: ana)
    c.attach([file("lake.jpg")])
    #expect(c.key(key("\u{1b}", code: 53)) && c.mode == .writing && c.attachments.files.count == 1)
    #expect(c.key(key("\u{1b}", code: 53)) && c.attachments.isEmpty && c.strip.isHidden)
    #expect(!c.key(key("\u{1b}", code: 53)), "nothing left to drop")
}

@MainActor @Test func `a 31st file is left out, and the bar says 30 at most`() {
    let (c, _) = box()
    c.attach((0 ..< 31).map { file("photo \($0).jpg") })
    #expect(c.attachments.files.count == 30)
    #expect(!c.bar.isHidden && c.bar.stringValue == "30 files at most at once: 1 left out")
    type("and this", in: c)
    c.textDidChange(Notification(name: NSText.didChangeNotification))
    #expect(c.bar.isHidden, "gone once the box changes")
}

@MainActor @Test func `files jawd turns down come back before those attached since, the words in the field, and the bar says why`() {
    let (c, stub) = box()
    c.attach([file("lake.jpg"), file("swim.mov")])
    type("the lake", in: c)
    c.send()
    #expect(stub.files.count == 1 && c.attachments.isEmpty)
    c.attach([file("notes.pdf", kind: .document)])
    c.unsent(files: stub.files[0].files, caption: "the lake", why: "swim.mov can't go as image")
    #expect(c.attachments.files.map(\.name) == ["lake.jpg", "swim.mov", "notes.pdf"])
    #expect(c.textView.string == "the lake" && c.bar.stringValue == "Not sent: swim.mov can't go as image" && !c.strip.isHidden)
}

@MainActor @Test func `attaching while editing is turned down, and the edit stays`() throws {
    let (c, stub) = box()
    let mine = try #require(Samples.messages.first { $0.mine && $0.kind == .text })
    c.edit(mine)
    c.attach([file("lake.jpg")])
    #expect(c.attachments.isEmpty && c.strip.isHidden)
    #expect(c.bar.stringValue == "files go with a new message: Esc stops editing first")
    if case .editing = c.mode {} else { Issue.record("still editing") }
    #expect(stub.files.isEmpty)
}

@MainActor @Test func `the strip's × takes its file out, and the last one out hides the strip`() {
    let (c, _) = box()
    c.attach([file("lake.jpg"), file("swim.mov")])
    c.strip.removed?(0)
    #expect(c.attachments.files.map(\.name) == ["swim.mov"] && c.strip.tiles.map(\.name) == ["swim.mov"])
    c.strip.removed?(0)
    #expect(c.attachments.isEmpty && c.strip.isHidden && c.sendButton.isHidden)
}

@MainActor @Test func `pasting files attaches them, an image attaches a PNG written in the box's folder, and text is the field's`() throws {
    let (c, _) = box()
    let board = NSPasteboard(name: .init("jaw-test-\(UUID().uuidString)"))
    defer { board.releaseGlobally() }
    try inFolder { folder in
        c.attachedFolder = folder
        let lake = folder.appending(path: "lake.jpg")
        try Data([1]).write(to: lake)
        board.clearContents()
        board.writeObjects([lake as NSURL])
        board.setString("lake.jpg", forType: .string)
        #expect(c.attachPasted(from: board) && c.attachments.files.map(\.name) == ["lake.jpg"], "the Finder's names as text too")

        board.clearContents()
        board.writeObjects([NSImage(size: NSSize(width: 2, height: 2), flipped: false) { r in
            NSColor.blue.setFill()
            r.fill()
            return true
        }])
        #expect(c.attachPasted(from: board))
        #expect(c.attachments.files.count == 2 && c.attachments.files[1].written && c.attachments.files[1].url.path.hasPrefix(folder.path))

        board.clearContents()
        board.setString("cells as text", forType: .string)
        board.writeObjects([NSImage(size: NSSize(width: 2, height: 2), flipped: false) { _ in true }])
        #expect(!c.attachPasted(from: board), "words and a picture of them, as a spreadsheet's cells copied, paste as words")
        #expect(c.attachments.files.count == 2)
    }
}
