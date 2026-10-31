import AppKit
import CoreServices
@testable import JawKit
@testable import JawUI
import Testing

/// What a file offers in each state, in the menu's words and with its keys; the name a copy takes,
/// as jawd names one; the clone a file is looked at and opened from, which nothing done to it lets
/// reach jawd's file; and the keys, the clicks and the menu reaching the host as the act they mean.

func titles(_ offer: MediaOffer?) -> [String] {
    offer?.menu.map(\.title) ?? []
}

@Test func `a file here is looked at, opened, saved and shown in Finder, and one away offers what its state lets you do`() {
    let here = MediaOffer.of(state("D15"))
    #expect(titles(here) == ["Quick Look", "Open", "Save to Downloads", "Show in Finder"])
    #expect(here?.menu.map(\.key) == ["Space", "⌘↩", "s", nil])
    #expect(titles(MediaOffer.of(state("D11"))).last == "Download")
    #expect(titles(MediaOffer.of(state("D3"))).last == "Try Again")
    #expect(titles(MediaOffer.of(state("D2"))).last == "Stop Downloading")
    #expect(titles(MediaOffer.of(state("D5"))).last == "Ask the Phone Again")
    #expect(titles(MediaOffer.of(state("D6"))).last == "Ask the Phone Again")
    #expect(titles(MediaOffer.of(state("D4"))).last == "Show in Finder", "waiting for the phone: nothing more to ask")
    let hd = MediaOffer.of(Samples.pictures.first { $0.id == "P4" }!)
    #expect(titles(hd).contains("Save HD Copy to Downloads"))
    #expect(MediaOffer.of(state("D2"))?.stateAct?.0 == .stop && MediaOffer.of(state("D5"))?.stateAct?.0 == .fetch)
    #expect(["D11", "D3", "D2", "D5", "D6", "D4", "D15"].map { MediaOffer.of(state($0))?.statePhrase } == [
        "Download this file", "Try downloading this file again", "Stop downloading this file", "Ask the phone for this file again",
        "Ask the phone for this file again", nil, nil,
    ], "the palette's words for the same")
}

@Test func `text, a voice message, one deleted and one to view once have no file to offer`() {
    #expect(MediaOffer.of(Samples.messages[1]) == nil, "text")
    #expect(MediaOffer.of(Samples.kinds.first { $0.kind == .voice }!) == nil, "jawd plays voice messages")
    var gone = state("D15")
    gone.deleted = true
    #expect(MediaOffer.of(gone) == nil)
    var once = state("D1")
    once.viewOnce = true
    #expect(MediaOffer.of(once) == nil)
    #expect(MediaOffer.of(Samples.pictures.first { $0.id == "P15" }!) == nil, "an album of counts: its items are messages of their own")
}

@Test func `an album offers each item as the message it is, and looks at all of them from the one clicked`() {
    let album = state("D10")
    let all = MediaOffer.all(album)
    #expect(all.map(\.message) == ["t0", "t1", "t2", "t3"] && all.map(\.item) == [0, 1, 2, 3])
    #expect(all.map(\.kind) == [.image, .image, .video, .image] && all.map(\.isHere) == [true, false, false, false])
    #expect(MediaOffer.of(album, item: 2)?.message == "t2" && MediaOffer.of(album)?.message == "t0")
}

@Test func `a copy is named as jawd names a saved one`() {
    #expect(MediaOffer.of(state("D15"))?.name == "Sintra topo.pdf")
    #expect(MediaOffer.named(Media(name: "a/b\\c.pdf"), id: "M") == "a_b_c.pdf")
    #expect(MediaOffer.named(Media(name: ".hidden", path: "media/files/ab.pdf"), id: "M") == "M.pdf", "never a hidden file")
    #expect(MediaOffer.named(Media(path: "media/files/ab12.jpg"), id: "3EB0") == "3EB0.jpg", "a photo by its ID")
    #expect(MediaOffer.named(Media(mime: "video/mp4"), id: "V") == "V.mp4", "by its type before it's here")
    #expect(MediaOffer.named(Media(), id: "X") == "X.bin")
}

/// A scratch folder of the test's own, removed after it.
func scratchFolder() throws -> URL {
    let scratch = FileManager.default.temporaryDirectory.appending(path: "jaw-mac-media-tests-\(UInt32.random(in: 0 ... .max))", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    return scratch
}

func attributes(_ url: URL) throws -> [FileAttributeKey: Any] {
    try FileManager.default.attributesOfItem(atPath: url.path)
}

/// A file's quarantine, as its type and the agent that set it.
func quarantine(_ url: URL) throws -> (type: String?, agent: String?) {
    let q = try url.resourceValues(forKeys: [.quarantinePropertiesKey]).quarantineProperties
    return (q?[kLSQuarantineTypeKey as String] as? String, q?[kLSQuarantineAgentNameKey as String] as? String)
}

@Test func `a file is looked at and opened from a read-only clone of it, quarantined as a message's attachment, which nothing done to it lets reach jawd's`() throws {
    let scratch = try scratchFolder()
    defer { try? FileManager.default.removeItem(at: scratch) }
    let jawds = scratch.appending(path: "media/files")
    try FileManager.default.createDirectory(at: jawds, withIntermediateDirectories: true)
    let file = jawds.appending(path: "ab12.pdf")
    try Data("jawd's file".utf8).write(to: file)
    let opened = scratch.appending(path: "opened")

    let clone = try Opened.copy(of: file, named: "Sintra topo.pdf", for: "3EB0/A1", in: opened)
    #expect(clone.lastPathComponent == "Sintra topo.pdf" && clone.deletingLastPathComponent().lastPathComponent == "3EB0_A1")
    let marked = try quarantine(clone)
    #expect(marked.type == kLSQuarantineTypeInstantMessageAttachment as String && marked.agent == "Jaw")
    #expect(try quarantine(file).type == nil, "jawd's own file isn't touched")
    #expect(try attributes(clone)[.posixPermissions] as? Int == 0o444)
    #expect(throws: (any Error).self, "an app can't save into it") { try Data("edited in another app".utf8).write(to: clone) }
    #expect(try Data(contentsOf: file) == Data("jawd's file".utf8), "jawd's file is as it was")
    let again = try Opened.copy(of: file, named: "Sintra topo.pdf", for: "3EB0/A1", in: opened)
    #expect(try again == clone && attributes(again)[.systemFileNumber] as? Int == attributes(clone)[.systemFileNumber] as? Int,
            "looked at again, the same clone, so an app showing it keeps it")
    let hd = jawds.appending(path: "cd34.pdf")
    try Data("jawd's HD copy".utf8).write(to: hd)
    let better = try Opened.copy(of: hd, named: "Sintra topo.pdf", for: "3EB0/A1", in: opened)
    #expect(try better == clone && Data(contentsOf: better) == Data("jawd's HD copy".utf8), "another file under its name is cloned again")

    let dragged = try #require(Opened.clone(of: file, named: "Sintra topo.pdf", for: "3EB0/A1", in: opened), "a clone at once, for a drag")
    #expect(dragged != clone, "beside the one looked at, which a drag never takes away")
    #expect(try quarantine(dragged).type == kLSQuarantineTypeInstantMessageAttachment as String)
    #expect(try (attributes(dragged)[.posixPermissions] as? Int ?? 0) & 0o200 != 0, "writable, as the Finder's copy of it will be")
    #expect(throws: (any Error).self) { try Opened.copy(of: jawds.appending(path: "missing.pdf"), named: "m.pdf", for: "M", in: opened) }
    Opened.leave(opened)
    #expect(!FileManager.default.fileExists(atPath: opened.path), "emptied as the run quits, read-only clones and all")
}

@Test func `each run keeps its clones in a folder of its own, and one starting empties only those of runs that have gone`() throws {
    let root = try scratchFolder()
    defer { try? FileManager.default.removeItem(at: root) }
    let mine = try #require(Opened.run(getpid()))
    #expect(Opened.folder.lastPathComponent == mine && mine.hasPrefix("\(getpid())-"))
    let gone = "\(getpid())-1", earlier = "999999-12345", old = "3EB0DEMO00000008"
    for name in [mine, gone, earlier, old] {
        try FileManager.default.createDirectory(at: root.appending(path: name).appending(path: "open"), withIntermediateDirectories: true)
    }
    Opened.sweep(root)
    #expect(try FileManager.default.contentsOfDirectory(atPath: root.path) == [mine],
            "a run with this process's ID that started at another time has gone, as has one with no process, and an earlier build's clones")
}

@MainActor
final class FileHost: ConversationHost {
    var acts: [(MediaOffer.Act, [String], Int)] = []

    func conversation(_: ConversationController, move _: Place) {}
    func conversation(_: ConversationController, send _: String, key _: String, reply _: String?, mentions _: [String]) {}
    func conversation(_: ConversationController, react _: String, emoji _: String) {}
    func conversation(_: ConversationController, edit _: String, text _: String) {}
    func conversation(_: ConversationController, delete _: String, everyone _: Bool) {}
    func conversation(_: ConversationController, info _: String) {}
    func conversation(_: ConversationController, retry _: String) {}
    func conversation(_: ConversationController, read _: String) {}
    func conversation(_: ConversationController, typing _: TypingState) {}
    func conversation(_: ConversationController, draft _: String) {}
    func conversationWantsHistory(_: ConversationController) {}
    func conversation(_: ConversationController, forward _: MessageRow) {}
    func conversation(_: ConversationController, reactTo _: MessageRow) {}
    func conversationFocused(_: ConversationController) {}
    func conversation(_: ConversationController, open _: String) {}
    func conversation(_: ConversationController, vote _: String, options _: [String]) {}
    func conversationIsRead(_: ConversationController) -> Bool { false }

    func conversation(_: ConversationController, media act: MediaOffer.Act, _ offers: [MediaOffer], at: Int, of _: MessageRow) {
        acts.append((act, offers.map(\.message), at))
    }
}

/// `row` at version `v`, its file (or its album's item `item`'s) in `state`, with `error`, and here
/// once it has `path`.
func asked(_ row: MessageRow, v: UInt64, _ state: MediaState, error: String? = nil, path: String? = nil, item: Int? = nil) -> MessageRow {
    var r = row
    r.v = v
    func change(_ media: inout Media?) {
        media?.state = state
        media?.error = error
        media?.path = path
    }
    if let item { change(&r.album!.items[item].media) } else { change(&r.media) }
    return r
}

@Test func `a file looked at before it's here opens once it comes, and stops waiting only when jawd gives up after it was asked for`() throws {
    // Expired, with jawd's words: asked again, jawd clears them and leaves it expired until its job runs.
    let expired = state("D5")
    let offer = try #require(MediaOffer.of(expired))
    let pending = PendingOpen(.look, offer, of: expired)
    let v = expired.v
    #expect(pending.next(asked(expired, v: v, .expired, error: expired.media?.error)) == .wait, "a row from before the ask, still in flight")
    #expect(pending.next(asked(expired, v: v + 1, .expired)) == .wait, "asked again: its words gone, still expired")
    #expect(pending.next(asked(expired, v: v + 2, .waitingForPhone)) == .wait)
    #expect(pending.next(asked(expired, v: v + 3, .downloading)) == .wait)
    let here = asked(expired, v: v + 4, .local, path: "media/files/wall.jpg")
    #expect(pending.next(here) == .open(MediaOffer.all(here), at: 0))
    #expect(pending.next(asked(expired, v: v + 2, .expired, error: "your phone didn't answer")) == .stop, "given up on again")
    #expect(pending.next(asked(expired, v: v + 1, .gone, error: "your phone no longer has this file")) == .stop)
    #expect(pending.next(asked(expired, v: v + 1, .remote, error: "the download failed")) == .stop)
    #expect(pending.next(state("D1")) == .wait, "another message's row")
    var deleted = asked(expired, v: v + 1, .expired)
    deleted.deleted = true
    #expect(pending.next(deleted) == .stop, "nothing to open any more")

    // An album's item, by its own message, in its album's row.
    let album = state("D10")
    let tile = try #require(MediaOffer.of(album, item: 3))
    let waiting = PendingOpen(.open, tile, of: album)
    #expect(waiting.row == "D10" && waiting.message == "t3")
    #expect(waiting.next(asked(album, v: album.v + 1, .expired, item: 3)) == .wait)
    let came = asked(album, v: album.v + 2, .local, path: "media/files/rope.jpg", item: 3)
    #expect(waiting.next(came) == .open(MediaOffer.all(came), at: 3))
    #expect(waiting.next(asked(album, v: album.v + 2, .gone, error: "gone", item: 3)) == .stop)
}

/// A pane of the download states' samples, once their rows are in its table.
@MainActor
func filesPane(_ host: FileHost) async -> ConversationController {
    let c = ConversationController(chat: Samples.direct, spec: spec(.bubbles, dark: true), layouts: Layouts(), place: .newest(100))
    c.host = host
    c.view.frame = NSRect(x: 0, y: 0, width: 640, height: 900)
    await withCheckedContinuation { done in
        c.drawn = { done.resume() }
        c.apply(Update(seq: 1, upsert: Samples.downloadStates, total: 15, older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0)))
    }
    return c
}

@MainActor
func key(_ characters: String, code: UInt16, _ modifiers: NSEvent.ModifierFlags = []) -> NSEvent {
    NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0, windowNumber: 0, context: nil,
                     characters: characters, charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code)!
}

@MainActor @Test func `Space, Enter and a click look, ⌘↩ opens, s saves, and the ring stops, each as the act it means`() async {
    let host = FileHost()
    let c = await filesPane(host)
    c.select("D15")
    #expect(c.selectionKey(key(" ", code: 49)))
    #expect(c.selectionKey(key("\r", code: 36)))
    #expect(c.selectionKey(key("\r", code: 36, .command)))
    #expect(c.selectionKey(key("s", code: 1)))
    #expect(host.acts.map(\.0) == [.look, .look, .open, .save])
    #expect(host.acts.allSatisfy { $0.1 == ["D15"] && $0.2 == 0 })

    host.acts.removeAll()
    c.select("D10")
    c.media(.look, c.window.get("D10")!, item: 2)
    #expect(host.acts.first.map { $0.0 == .look && $0.1 == ["t0", "t1", "t2", "t3"] && $0.2 == 2 } == true, "an album from the tile clicked")
    c.select("D2")
    c.media(.stop, c.window.get("D2")!, item: nil)
    #expect(host.acts.last?.0 == .stop)

    c.select("D15")
    let menu = c.menu(forRow: c.rows.firstIndex { $0.id == "D15" }!)
    let words = menu?.items.map(\.title) ?? []
    #expect(words.contains("Quick Look") && words.contains("Save to Downloads") && words.contains("Show in Finder"))
    #expect(menu?.items.first { $0.title == "Quick Look" }?.keyEquivalent == " ")
    #expect(menu?.items.first { $0.title == "Open" }.map { $0.keyEquivalent == "\r" && $0.keyEquivalentModifierMask == .command } == true)
    #expect(!words.contains("Stop Downloading") && !words.contains("Download"), "it's here")
    let coming = c.menu(forRow: c.rows.firstIndex { $0.id == "D2" }!)?.items.map(\.title) ?? []
    #expect(coming.contains("Stop Downloading"))
}

@MainActor @Test func `a file not here drags out as a promise, kept by saving it where it's dropped`() async throws {
    let offer = try #require(MediaOffer.of(state("D11")))
    final class Asked: @unchecked Sendable {
        var to: [URL] = []
        var kept: [String] = []
    }
    let asked = Asked()
    let promise = Dragged(offer) { url, done in
        asked.to.append(url)
        done(nil)
    }
    #expect(promise.fileType == "com.adobe.pdf")
    #expect(promise.filePromiseProvider(promise, fileNameForType: promise.fileType) == "Sintra topo.pdf")
    let drop = URL(fileURLWithPath: "/Users/me/Desktop/Sintra topo.pdf")
    promise.filePromiseProvider(promise, writePromiseTo: drop) { error in asked.kept.append(error == nil ? "kept" : "broken") }
    #expect(await eventually { asked.kept == ["kept"] }, "kept once jawd has saved it")
    #expect(asked.to == [drop], "into the folder it's dropped in, by the name the drop gives it")
    #expect(NotSaved("jawd couldn't download the file").localizedDescription == "jawd couldn't download the file")
}
