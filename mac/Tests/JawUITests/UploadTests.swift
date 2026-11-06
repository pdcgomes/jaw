import AppKit
@testable import JawKit
@testable import JawUI
import Testing

/// A file of yours on its way: how far it has got, from jawd's jobs, drawn where a download's
/// progress is, as a ring with × to stop it, and said as the terminal says it; never drawn as a file
/// to download; and Stop Sending, its state's act.

/// A file of yours waiting to send, before its copy is made: no state and no path, as jawd's answer
/// has it, which reads as remote.
func outgoing(_ kind: MessageKind = .image, id: String = "OUT", status: MessageStatus = .pending) -> MessageRow {
    var m = MessageRow(id: id, v: 1, sort: "1", chat: "ana", at: Samples.now, from: nil, kind: kind, spans: [])
    m.mine = true
    m.status = status
    m.media = kind == .document ? Media(bytes: 48000, name: "notes.pdf", mime: "application/pdf") : Media(w: 1600, h: 1200, mime: "image/jpeg")
    return m
}

/// An album of yours waiting to send, its four photos' messages its items.
func outgoingAlbum() -> MessageRow {
    var m = album(4)
    m.mine = true
    m.status = .pending
    for i in m.album!.items.indices {
        m.album!.items[i].media = Media(w: 1600, h: 1200, mime: "image/jpeg")
    }
    return m
}

func laid(_ m: MessageRow, sending: [String: Sending]) -> Laid {
    let s = spec(.bubbles, dark: true)
    var context = MessageContext(group: false, firstOfRun: true, showTime: true, day: nil)
    context.sending = sending
    return MessageLayout.lay(m, width: 640, context: context, style: s.style, fonts: s.fonts, me: s.me)
}

@Test func `a job is how far a copy has got, a whole percent, said as the terminal says it`() {
    let making = Sending(Job(id: "transcode:7", kind: .transcode, message: "OUT", progress: 0.404))
    let uploading = Sending(Job(id: "upload:9", kind: .upload, message: "OUT", variant: .sd, progress: 0.7))
    #expect(making == Sending(step: .making, percent: 40) && making.words == "making it ready 40%")
    #expect(uploading == Sending(step: .uploading, percent: 70) && uploading.words == "uploading 70%")
    #expect(Sending(Job(id: "upload:9", kind: .upload, message: "OUT", progress: 1.3)).percent == 100)
}

@Test func `a photo of yours on its way draws a ring with × to stop it, and one with no job yet draws neither, nor the ↓ of one to download`() {
    let photo = outgoing()
    let sending = laid(photo, sending: ["OUT": Sending(step: .uploading, percent: 70)])
    #expect(rings(sending) == [1, 0.7] && stops(sending) == [.stop(item: nil)])
    #expect(sending.regions.first?.1 == .stop(item: nil), "the ring is found before the picture under it")
    let waiting = laid(photo, sending: [:])
    #expect(rings(waiting).isEmpty && stops(waiting).isEmpty)
    #expect(waiting.height == sending.height, "how far it's got changes no height")
    var sent = photo
    sent.status = .sent
    #expect(rings(laid(sent, sending: ["OUT": Sending(step: .uploading, percent: 70)])).isEmpty, "once it's sent, a job left over draws nothing")
    var failed = photo
    failed.status = .failed
    #expect(rings(laid(failed, sending: [:])).isEmpty && stops(laid(failed, sending: [:])).isEmpty)
    #expect(laid(failed, sending: [:]).regions.contains { $0.1 == .retry }, "failed, it says so and offers Retry")
}

@Test func `a file's circle fills as it goes, and an album's tile on its way has its own ring`() {
    let doc = laid(outgoing(.document), sending: ["OUT": Sending(step: .making, percent: 40)])
    #expect(rings(doc) == [1, 0.4] && stops(doc) == [.stop(item: nil)])
    #expect(stops(laid(outgoing(.document), sending: [:])).isEmpty)

    let album = outgoingAlbum()
    let second = album.album!.items[1].id
    let tiles = laid(album, sending: [second: Sending(step: .uploading, percent: 50)])
    #expect(rings(tiles) == [1, 0.5] && stops(tiles) == [.stop(item: 1)], "the second tile, stopped by itself")
}

@Test func `VoiceOver says how far a file of yours has got, and nothing of downloading it`() {
    let media = outgoing().media!
    #expect(MessageView.said(media, sending: Sending(step: .uploading, percent: 70), outgoing: true) == "uploading 70%")
    #expect(MessageView.said(media, sending: nil, outgoing: true) == nil)
    #expect(MessageView.said(media, sending: nil, outgoing: false) == "not downloaded", "someone else's says it's to download")
}

@Test func `a pending file of yours offers Stop Sending, a failed one nothing of its file's state, and someone else's its download`() throws {
    let pending = try #require(MediaOffer.of(outgoing()))
    #expect(pending.sending && pending.stateAct! == (.stop, "Stop Sending") && pending.statePhrase == "Stop sending this file")
    #expect(pending.menu.last.map { $0.title } == "Stop Sending")
    let failed = try #require(MediaOffer.of(outgoing(status: .failed)))
    #expect(!failed.sending && failed.stateAct == nil && failed.statePhrase == nil)
    var theirs = outgoing()
    theirs.mine = false
    theirs.status = nil
    #expect(try #require(MediaOffer.of(theirs)).stateAct! == (.fetch, "Download"))
    let tile = try #require(MediaOffer.of(outgoingAlbum(), item: 2))
    #expect(tile.sending && tile.stateAct! == (.stop, "Stop Sending"), "an album's item of yours too")
}

@Test func `a forward of yours on its way, which jawd doesn't upload, is drawn and offered as any file, its download among it`() throws {
    var forward = outgoing()
    forward.forwarded = 1
    let offer = try #require(MediaOffer.of(forward))
    #expect(!offer.sending && !offer.outbox && offer.stateAct! == (.fetch, "Download"))
    #expect(laid(forward, sending: ["OUT": Sending(step: .uploading, percent: 70)]).regions.contains { $0.1 == .media(item: nil) })
    #expect(rings(laid(forward, sending: ["OUT": Sending(step: .uploading, percent: 70)])).isEmpty, "no job of jawd's is a forward's")
}

/// A pane of `rows`, once they're in its table.
@MainActor
func pane(_ rows: [MessageRow], layouts: Layouts = Layouts()) async -> ConversationController {
    let c = ConversationController(chat: Samples.direct, spec: spec(.bubbles, dark: true), layouts: layouts, place: .newest(100))
    c.view.frame = NSRect(x: 0, y: 0, width: 640, height: 900)
    await withCheckedContinuation { done in
        c.drawn = { done.resume() }
        c.apply(Update(seq: 1, upsert: rows, total: UInt64(rows.count), older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0)))
    }
    return c
}

@MainActor @Test func `an HD copy's jobs, which name its SD message, are left out, and a job for a file stopped isn't kept`() async throws {
    let c = await pane([outgoing(id: "OUT1")])
    c.took(Job(id: "upload:1", kind: .upload, message: "OUT1", variant: .sd, progress: 0.3))
    c.took(Job(id: "transcode:2", kind: .transcode, message: "OUT1", variant: .hd, progress: 0.8))
    #expect(c.sending["OUT1"] == Sending(step: .uploading, percent: 30), "the SD copy's progress, not the HD one's")
    var stopped = outgoing(id: "OUT1", status: .failed)
    stopped.v = 2
    c.apply(Update(seq: 2, upsert: [stopped], total: 1, older: false, newer: 0, moved: nil))
    c.took(Job(id: "upload:1", kind: .upload, message: "OUT1", variant: .sd, progress: 0.4))
    #expect(c.sending.isEmpty, "a job that comes after it's stopped is left out, so Send Again starts from nothing")
}

@MainActor @Test func `each step of a file's progress takes its message's place in the layouts kept, rather than adding one`() async throws {
    let layouts = Layouts()
    let c = await pane([outgoing(id: "OUT1")], layouts: layouts)
    let kept = layouts.count
    for step in 1 ... 19 {
        c.took(Job(id: "upload:1", kind: .upload, message: "OUT1", progress: Double(step) / 20))
    }
    // Every layout asked for made, then each one's completion on the main queue run.
    layouts.queue.sync(flags: .barrier) {}
    try await Task.sleep(for: .milliseconds(200))
    #expect(layouts.count == kept, "\(layouts.count) layouts kept for one message, from \(kept)")
}

@MainActor @Test func `a pane puts each job in its row's context, lays out only that row again, and forgets a copy once it's uploaded`() async throws {
    let c = ConversationController(chat: Samples.direct, spec: spec(.bubbles, dark: true), layouts: Layouts(), place: .newest(100))
    c.view.frame = NSRect(x: 0, y: 0, width: 640, height: 900)
    let photo = outgoing(id: "OUT1"), album = outgoingAlbum()
    await withCheckedContinuation { done in
        c.drawn = { done.resume() }
        c.apply(Update(seq: 1, upsert: [photo, album], total: 2, older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0)))
    }
    let i = try #require(c.rows.firstIndex { $0.id == "OUT1" }), a = try #require(c.rows.firstIndex { $0.id == album.id })
    let item = album.album!.items[3].id
    c.took(Job(id: "transcode:1", kind: .transcode, message: "OUT1", progress: 0.25))
    c.took(Job(id: "upload:2", kind: .upload, message: item, progress: 0.6))
    c.took(Job(id: "upload:3", kind: .upload, message: "elsewhere", progress: 0.5))
    #expect(c.contexts[i].sending == ["OUT1": Sending(step: .making, percent: 25)])
    #expect(c.contexts[a].sending == [item: Sending(step: .uploading, percent: 60)], "an album's item, in the album's row")
    #expect(c.sending.keys.sorted() == ["OUT1", item].sorted(), "a job for a message the pane doesn't hold is left out")
    c.took(Job(id: "download:4", kind: .download, message: "OUT1", progress: 0.9))
    #expect(c.contexts[i].sending == ["OUT1": Sending(step: .making, percent: 25)], "a download's job isn't a file on its way")
    c.took(Job(id: "upload:5", kind: .upload, message: "OUT1", progress: 1))
    #expect(c.contexts[i].sending.isEmpty && c.sending["OUT1"] == nil, "uploaded: what's left is its tick")
}
