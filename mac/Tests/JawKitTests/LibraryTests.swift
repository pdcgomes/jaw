import Foundation
@testable import JawKit
import Testing

/// The library as JawKit reads and asks for it: the protocol's fixtures, the window model, a library
/// window against a fake jawd, moved and watched again whole with its filters, and against a real
/// jawd's made-up account.

@Test func `library.watch is written as its fixture, with every filter`() throws {
    let filter = LibraryFilter(kind: .photo, chat: "120363000000000002@g.us", person: "person:4", since: 1_785_542_400_000, text: "praia")
    let watch = LibraryWatch(filter: filter, around: .top, before: 0, after: 100)
    #expect(try written(id: 60, "library.watch", watch) == JSONValue.parse(fixture("library.watch.request.json")))
    let all = try #require(try written(id: 1, "library.watch", LibraryWatch(filter: LibraryFilter(), around: .top, before: 0, after: 24))["params"])
    #expect(all["kind"]?.string == "all" && all["text"] == nil && all["chat"] == nil, "every kind, and no filter left in: \(all)")
}

@Test func `library.shown is written as its fixture`() throws {
    let shown = LibraryShown(items: ["3EB0A1B2C3D4E5F6", "3EB0C7D8E9F0A1B2", "3EB0D4E5F6A7B8C9"])
    #expect(try written(id: 61, "library.shown", shown) == JSONValue.parse(fixture("library.shown.request.json")))
    #expect(LibraryShown.most == 200)
}

@Test func `a library window's answer reads as its items, the photo here with its previews`() throws {
    guard case let .library(id, watched) = try incoming("library.watch.result.json", .library) else {
        Issue.record("not a library window")
        return
    }
    #expect(id == 60 && watched.watch == "w7" && watched.total == 6 && !watched.older && watched.note == nil)
    #expect(watched.counts == LibraryCounts(photo: 6, video: 1, voice: 0, file: 0, link: 1))
    let photo = try #require(watched.rows.first)
    #expect(photo.kind == .photo && photo.message == "3EB0A1B2C3D4E5F6" && photo.chatName == "Família Gomes")
    #expect(photo.from == Sender(id: "person:4", name: "Inês", colour: 4) && !photo.mine)
    #expect(photo.caption == [Span(.text, "na "), Span(.mark, "praia")])
    let media = try #require(photo.media)
    #expect(media.state == .local && media.isHere && media.path == "media/files/9d/9d6c20c8.jpg")
    #expect(media.preview(256) == "media/previews/9d6c20c8.256.jpg" && media.preview(768) == "media/previews/9d6c20c8.768.jpg")
    #expect(media.thumbnail == "media/previews/9d6c20c8.instant.jpg" && media.w == 4032 && media.h == 3024)
}

@Test func `the library push brings a file, a voice message and a link, and takes an item away`() throws {
    guard case let .libraryPush(push) = try incoming("library.push.json", .nothing) else {
        Issue.record("not a library push")
        return
    }
    #expect(push.watch == "w8" && push.total == 9 && push.older == nil && push.remove == ["3EB0A1B2C3D4E5F6"])
    #expect(push.counts?.link == 2 && push.counts?.shared == 11)
    let kinds = push.upsert.map(\.kind)
    #expect(kinds == [.file, .voice, .link])
    let file = push.upsert[0]
    #expect(file.mine && file.from == nil)
    #expect(file.file == LibraryFile(name: "orcamento-obras-cozinha.pdf", type: "PDF", bytes: 2_400_000, pages: 3))
    #expect(push.upsert[1].voice?.seconds == 14 && push.upsert[1].voice?.waveform == [], "a voice item's waveform waits for P14")
    let link = push.upsert[2]
    #expect(link.id == "3EB0E7F8A9B0C1D2~0" && link.message == "3EB0E7F8A9B0C1D2" && link.sort.hasSuffix("~99999"))
    #expect(link.link?.url == "https://www.ipma.pt/pt/otempo/prev.localidade.hora/" && link.link?.host == "www.ipma.pt")
    #expect(link.link?.title == "Previsão para Sintra · IPMA" && link.link?.description?.hasPrefix("Céu pouco nublado") == true)
}

@Test func `a kind jawd adds later reads as other, an item that isn't jaw's protocol is left out, and P14's fields read once sent`() throws {
    let line = #"""
    {"method":"library","params":{"watch":"w9","seq":2,"upsert":[
    {"id":"S1","v":1,"sort":"9~0000000","kind":"sticker","message":"S1","chat":"c","chat_name":"C","mine":true,"at":9,"caption":[]},
    {"id":"X1","sort":"8~0000000","kind":"photo"},
    {"id":"V1","v":1,"sort":"7~0000000","kind":"voice","message":"V1","chat":"c","chat_name":"C","mine":false,"at":7,"caption":[],"voice":{"seconds":4.0,"waveform":[0,50,100],"played":true}},
    {"id":"F1","v":1,"sort":"6~0000000","kind":"file","message":"F1","chat":"c","chat_name":"C","mine":false,"at":6,"caption":[],"file":{"name":"topo.pdf","state":"downloading","progress":0.4}},
    {"id":"L1~0","v":1,"sort":"5~0000000~99999","kind":"link","message":"L1","chat":"c","chat_name":"C","mine":false,"at":5,"caption":[],"link":{"url":"https://a.pt","host":"a.pt","preview":"media/links/a.jpg"}}],
    "counts":{"photo":1,"video":0,"voice":1,"file":1}}}
    """#.replacingOccurrences(of: "\n", with: "")
    guard case let .libraryPush(push) = pushed(line) else {
        Issue.record("not a library push")
        return
    }
    #expect(push.upsert.map(\.id) == ["S1", "V1", "F1", "L1~0"] && push.unread == 1, "the item with no v is left out and counted")
    #expect(push.upsert[0].kind == .other)
    #expect(push.upsert[1].voice == LibraryVoice(seconds: 4, waveform: [0, 50, 100], played: true))
    #expect(push.upsert[2].file?.state == .downloading && push.upsert[2].file?.progress == 0.4)
    #expect(push.upsert[3].link?.preview == "media/links/a.jpg")
    #expect(push.counts?.link == nil && push.counts?[.link] == 0, "a jawd from before links leaves their count out")
}

@Test func `a library window keeps its items newest first, and its counts, total, older and note until they change`() throws {
    var window = Library<LibraryRow>(place: .top(240))
    func item(_ id: String, _ t: Int, v: UInt64 = 1) -> LibraryRow {
        LibraryRow(id: id, v: v, sort: "\(t)~0000000", kind: .photo, chat: "c", chatName: "C", at: Int64(t))
    }
    window.apply(Update(seq: 1, upsert: [item("a", 1), item("c", 3), item("b", 2)], total: 3, older: true,
                        moved: Moved(place: .top(240), newer: 0), counts: LibraryCounts(photo: 3), note: .some("links come later")))
    #expect(window.list.map(\.id) == ["c", "b", "a"] && window.total == 3 && window.older && window.loaded)
    #expect(window.counts?.photo == 3 && window.note == "links come later")
    window.apply(Update(seq: 2, remove: ["b"], upsert: [item("d", 4), item("a", 1, v: 2)]))
    #expect(window.list.map(\.id) == ["d", "c", "a"] && window.get("a")?.v == 2)
    #expect(window.total == 3 && window.older && window.counts?.photo == 3 && window.note == "links come later",
            "a push that doesn't say them leaves them")
    window.apply(Update(seq: 3, total: 4, older: false, counts: LibraryCounts(photo: 4), note: .some(nil)))
    #expect(window.total == 4 && !window.older && window.counts?.photo == 4 && window.note == nil, "an answer with no note clears it")
    #expect(window.seq == 3)
}

@Test func `a library window's updates merged between frames end as applying each`() {
    func item(_ id: String, _ t: Int, v: UInt64 = 1) -> LibraryRow {
        LibraryRow(id: id, v: v, sort: "\(t)~0000000", kind: .file, chat: "c", chatName: "C", at: Int64(t))
    }
    let updates: [Update<LibraryRow>] = [
        Update(seq: 1, upsert: [item("a", 1), item("b", 2)], total: 2, older: false, moved: Moved(place: .top(10), newer: 0),
               counts: LibraryCounts(file: 2), note: .some(nil)),
        Update(seq: 2, upsert: [item("c", 3)], total: 3, counts: LibraryCounts(file: 3)),
        Update(seq: 3, remove: ["a", "c"], upsert: [item("b", 2, v: 5)], total: 1),
    ]
    var each = Library<LibraryRow>(place: .top(10))
    for u in updates {
        each.apply(u)
    }
    var merged = Merged<LibraryRow>()
    var ids: Set<String> = []
    for u in updates {
        merged.add(u, ids: &ids)
    }
    var once = Library<LibraryRow>(place: .top(10))
    once.apply(merged.update())
    #expect(once.list.map { "\($0.id)@\($0.v)" } == each.list.map { "\($0.id)@\($0.v)" } && once.list.map(\.id) == ["b"])
    #expect(once.total == each.total && once.counts == each.counts && once.note == each.note && once.seq == 3)
    #expect(merged.counts == (0, 1), "the item that came and went leaves nothing behind")
}

/// A script that answers library windows as jawd would for a small account: two photos, and
/// `watch.move` on the window as a page of older ones.
func library(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    func item(_ n: Int) -> String {
        #"{"id":"P\#(n)","v":\#(n),"sort":"\#(String(format: "%04d", 1000 - n))~0000000","kind":"photo","message":"P\#(n)","chat":"ana","chat_name":"Ana","mine":false,"at":\#(1000 - n),"caption":[],"media":{"w":10,"h":10}}"#
    }
    switch method {
    case "library.watch":
        return [#"{"id":\#(id),"result":{"watch":"w5","seq":10,"total":4,"older":true,"counts":{"photo":4,"video":0,"voice":0,"file":0,"link":0},"rows":[\#(item(1)),\#(item(2))]}}"#]
    case "watch.move":
        return [#"{"id":\#(id),"result":{"watch":"w5","seq":11,"total":4,"older":false,"counts":{"photo":4,"video":0,"voice":0,"file":0,"link":0},"rows":[\#(item(3)),\#(item(4))]}}"#]
    default:
        return ordinary(method, params, id)
    }
}

@MainActor
@Suite(.serialized) struct LibraryClientTests {
    @Test func `a library window is watched with its filter, pushed to, and moved with the same filter`() async throws {
        let fake = try FakeJawd(script: library)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let filter = LibraryFilter(kind: .photo, chat: "ana", person: "me", text: "lago")
        let w = jawd.watchLibrary(filter, .top(240))
        var window = Library<LibraryRow>(place: .top(240))
        func take(_ all: [JawUpdate]) {
            for case let .library(id, update) in all where id == w {
                window.apply(update)
            }
        }
        take(await drain(jawd) { all in all.contains { if case .library = $0 { true } else { false } } })
        #expect(window.list.map(\.id) == ["P1", "P2"] && window.older && window.counts?.photo == 4)
        let asked = try #require(requests(fake, "library.watch").first)
        #expect(asked["kind"]?.string == "photo" && asked["chat"]?.string == "ana" && asked["person"]?.string == "me")
        #expect(asked["text"]?.string == "lago" && asked["around"]?.string == "top" && asked["after"]?.int == 240 && asked["have"] == nil)

        fake.send(#"{"method":"library","params":{"watch":"w5","seq":12,"total":5,"counts":{"photo":5,"video":0,"voice":0,"file":0,"link":0},"upsert":[{"id":"P0","v":20,"sort":"1001~0000000","kind":"photo","message":"P0","chat":"ana","chat_name":"Ana","mine":true,"at":1001,"caption":[]}]}}"#)
        fake.send(#"{"method":"library","params":{"watch":"w5","seq":13,"remove":["P2"]}}"#)
        try await Task.sleep(for: .milliseconds(100))
        let pushed = jawd.drain()
        #expect(pushed.count { if case .library = $0 { true } else { false } } == 1, "two pushes between frames come as one update")
        take(pushed)
        #expect(window.list.map(\.id) == ["P0", "P1"] && window.total == 5 && window.counts?.photo == 5)

        jawd.move(w, .around("P1", before: 1, after: 2))
        take(await drain(jawd) { all in all.contains { if case let .library(_, u) = $0 { u.moved != nil } else { false } } })
        let move = try #require(requests(fake, "watch.move").first)
        #expect(move["watch"]?.string == "w5" && move["around"]?.string == "P1" && move["before"]?.int == 1 && move["after"]?.int == 2)
        #expect(window.place == .around("P1", before: 1, after: 2) && !window.older)
        #expect(window.list.map(\.id) == ["P0", "P1", "P3", "P4"])
    }

    @Test func `after a drop or a resync a library window is watched again whole, with its filters and place`() async throws {
        let fake = try FakeJawd(script: library)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = jawd.watchLibrary(LibraryFilter(kind: .link, since: 5, until: 9), .top(240))
        _ = await drain(jawd) { all in all.contains { if case .library = $0 { true } else { false } } }
        fake.drop()
        _ = await drain(jawd) { all in helloed(all) == 1 && all.contains { if case .library = $0 { true } else { false } } }
        let watches = requests(fake, "library.watch")
        #expect(watches.count == 2)
        #expect(watches.last?["kind"]?.string == "link" && watches.last?["since"]?.int == 5 && watches.last?["until"]?.int == 9)
        #expect(watches.last?["have"] == nil && watches.last?["around"]?.string == "top", "whole, as jawd watches a library")

        fake.send(#"{"method":"resync","params":{"watch":"w5"}}"#)
        #expect(await eventually { requests(fake, "library.watch").count == 3 })
        #expect(jawd.client.stats.resyncs == 1)
    }

    @Test func `the items on screen are named to jawd, 200 at most`() async throws {
        let fake = try FakeJawd(script: library)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.shown((0 ..< 250).map { "P\($0)" })
        #expect(await eventually { requests(fake, "library.shown").count == 1 })
        let items = requests(fake, "library.shown").first?["items"]?.array?.compactMap(\.string)
        #expect(items?.count == 200 && items?.first == "P0" && items?.last == "P199", "the first 200, in their order")
    }

    @Test func `once a library window is unwatched, its pushes are let go`() async throws {
        let fake = try FakeJawd(script: library)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let w = jawd.watchLibrary(LibraryFilter(), .top(24))
        _ = await drain(jawd) { all in all.contains { if case .library = $0 { true } else { false } } }
        jawd.unwatch(w)
        #expect(await eventually { requests(fake, "unwatch").count == 1 })
        fake.send(#"{"method":"library","params":{"watch":"w5","seq":14,"remove":["P1"]}}"#)
        try await Task.sleep(for: .milliseconds(100))
        #expect(!jawd.drain().contains { if case .library = $0 { true } else { false } })
        #expect(requests(fake, "unwatch").first?["watch"]?.string == "w5")
    }
}
