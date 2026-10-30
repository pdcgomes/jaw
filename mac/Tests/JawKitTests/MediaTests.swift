import Foundation
@testable import JawKit
import Testing

/// Media in, as the client reads and asks it: a media row in each state jawd gives a file, an album
/// as the protocol has it and as media in's jawd sends one, and `media.fetch`, `media.save` and
/// `media.cancel` asked of a fake jawd and answered or refused.

/// A message row of `kind` with `media` as jawd sends it, in Ana's chat.
func mediaRow(_ kind: String, _ media: String, id: String = "3EB0A1B2C3D4E5F6") throws -> MessageRow {
    try JSONDecoder().decode(MessageRow.self, from: Data("""
    {"id":"\(id)","v":81302,"sort":"1792145060000~000000000123","chat":"351912345678@s.whatsapp.net",
     "at":1792145060000,"from":{"id":"person:12","name":"Ana","colour":4},"mine":false,"kind":"\(kind)","spans":[],
     "reply":null,"forwarded":0,"reactions":[],"status":null,"edited":false,"deleted":false,"expires_at":null,
     "view_once":false,"starred":false,"media":\(media)}
    """.utf8))
}

@Test func `a photo downloading reads as the protocol's example has it`() throws {
    let row = try mediaRow("image", """
    {"w":4032,"h":3024,"bytes":3480211,"mime":"image/jpeg","state":"downloading","progress":0.4,
     "thumbnail":"media/previews/9d6c20c8.instant.jpg","hd":true}
    """)
    let media = try #require(row.media)
    #expect(media.state == .downloading && media.progress == 0.4 && !media.isHere)
    #expect(media.w == 4032 && media.h == 3024 && media.bytes == 3_480_211 && media.hd)
    #expect(media.thumbnail == "media/previews/9d6c20c8.instant.jpg" && media.previews == nil)
}

@Test func `a file here has its path and both previews, and the HD copy's path once it's here too`() throws {
    let media = try #require(try mediaRow("image", """
    {"w":1600,"h":1200,"bytes":204800,"mime":"image/jpeg","state":"local","path":"media/files/ab12.jpg",
     "previews":{"256":"media/previews/ab12.256.jpg","768":"media/previews/ab12.768.jpg"},
     "thumbnail":"media/previews/ab12.instant.jpg","hd":true,"hd_path":"media/files/cd34.jpg"}
    """).media)
    #expect(media.isHere && media.path == "media/files/ab12.jpg" && media.hdPath == "media/files/cd34.jpg")
    #expect(media.preview(256) == "media/previews/ab12.256.jpg" && media.preview(768) == "media/previews/ab12.768.jpg")
    #expect(media.preview(512) == nil)
}

@Test func `each state that leaves the file away says why in jawd's words`() throws {
    func media(_ fields: String) throws -> Media {
        try #require(try mediaRow("video", "{\"seconds\":42,\"bytes\":12500000,\(fields)}").media)
    }
    let expired = try media(#""state":"expired","error":"WhatsApp's server no longer has it; ask for it to have your phone send it again""#)
    #expect(expired.state == .expired && expired.error?.hasPrefix("WhatsApp's server") == true && !expired.isHere)
    let gone = try media(#""state":"gone","error":"your phone no longer has this file""#)
    #expect(gone.state == .gone && gone.error == "your phone no longer has this file")
    let waiting = try media(#""state":"waiting_for_phone""#)
    #expect(waiting.state == .waitingForPhone && waiting.error == nil)
    let failed = try media(#""state":"remote","error":"the download failed: the network connection was lost""#)
    #expect(failed.state == .remote && failed.error != nil, "a download that failed is remote again, with why")
    let later = try media(#""state":"thawing""#)
    #expect(later.state == .remote, "a state jawd may add later says nothing's here")
}

@Test func `a row from before media in is remote until it has a path`() throws {
    let bare = try #require(try mediaRow("image", #"{"w":800,"h":600}"#).media)
    #expect(bare.state == .remote && !bare.isHere && !bare.hd)
    let here = try #require(try mediaRow("document", #"{"name":"topo.pdf","path":"media/files/ef.pdf"}"#).media)
    #expect(here.state == .local && here.isHere)
}

@Test func `an album has its items' files as the protocol has it, or the counts media in sends`() throws {
    let row = try JSONDecoder().decode(MessageRow.self, from: Data("""
    {"id":"AL1","v":1,"sort":"1","chat":"g","at":1,"kind":"album","spans":[{"t":"text","s":"Sintra"}],
     "album":{"items":[{"id":"a","kind":"image","media":{"w":4000,"h":3000,"state":"local","path":"media/files/a.jpg",
       "previews":{"256":"media/previews/a.256.jpg","768":"media/previews/a.768.jpg"}}},
      {"id":"b","kind":"video","media":{"w":1080,"h":1920,"seconds":12,"state":"remote"}},{"id":"c","kind":"image"}]}}
    """.utf8))
    let album = try #require(row.album)
    #expect(album.count == 3 && album.items.map(\.kind) == ["image", "video", "image"])
    #expect(album.items[0].media?.isHere == true && album.items[1].media?.seconds == 12 && album.items[2].media == nil)
    #expect(row.plain == "Sintra", "the album's caption is its spans")

    let counted = try JSONDecoder().decode(MessageRow.self, from: Data("""
    {"id":"AL2","v":1,"sort":"1","chat":"g","at":1,"kind":"album","spans":[],"album":{"photos":3,"videos":1}}
    """.utf8))
    #expect(counted.album?.items.isEmpty == true && counted.album?.count == 4)
    #expect(counted.album?.photos == 3 && counted.album?.videos == 1)
}

@Test func `media in's requests are written as its fixtures, and media.save's answer reads as where it saved`() throws {
    func same(_ name: String, _ written: JSONValue) throws {
        #expect(written == (try JSONValue.parse(fixture(name))), "\(name)")
    }
    try same("media.fetch.request.json", written(id: 61, "media.fetch", MediaFetch(
        message: "3EB0A1B2C3D4E5F6", chat: "351912345678@s.whatsapp.net", variant: .hd,
    )))
    try same("media.save.request.json", written(id: 62, "media.save", MediaSave(message: "3EB0A1B2C3D4E5F6", to: "~/Downloads")))
    try same("media.cancel.request.json", written(id: 63, "media.cancel", MediaCancel(message: "3EB0A1B2C3D4E5F6")))
    #expect(try written(id: 1, "media.fetch", MediaFetch(message: "m"))["params"] == .object(["message": .string("m")]),
            "the standard copy is jawd's default, so it's left out")

    guard case let .reply(62, .saved(saved)) = try incoming("media.save.result.json", .reply(.saved)) else {
        Issue.record("media.save's answer isn't read as Saved")
        return
    }
    #expect(saved.path == "/Users/me/Downloads/3EB0A1B2C3D4E5F6.jpg")
    guard case .reply(61, _) = try incoming("media.fetch.result.json", .reply(.json)),
          case .reply(63, _) = try incoming("media.cancel.result.json", .reply(.json))
    else {
        Issue.record("media.fetch's and media.cancel's answers aren't read")
        return
    }
}

/// A script that answers as `ordinary` does, and media.save with a path in Downloads, or refuses it
/// for the message `refused`, as jawd refuses one whose download failed.
func saving(refused: String) -> FakeJawd.Script {
    { method, params, id in
        guard method == "media.save" else { return ordinary(method, params, id) }
        let message = params["message"]?.string ?? ""
        if message == refused {
            return [#"{"id":\#(id),"error":{"code":4,"message":"jawd couldn't download the file: your phone no longer has this file"}}"#]
        }
        return [#"{"id":\#(id),"result":{"path":"/Users/me/Downloads/\#(message).jpg"}}"#]
    }
}

@MainActor
@Suite(.serialized) struct MediaClientTests {
    @Test func `fetch and cancel ask jawd once each, naming the chat`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.fetch(chat: "ana", message: "m1")
        jawd.fetch(chat: "ana", message: "m1", variant: .hd)
        jawd.cancel(chat: "ana", message: "m2")
        #expect(await eventually { requests(fake, "media.fetch").count == 2 && requests(fake, "media.cancel").count == 1 })
        #expect(requests(fake, "media.fetch") == [
            .object(["message": .string("m1"), "chat": .string("ana")]),
            .object(["message": .string("m1"), "chat": .string("ana"), "variant": .string("hd")]),
        ])
        #expect(requests(fake, "media.cancel") == [.object(["message": .string("m2"), "chat": .string("ana")])])
        let after = await drain(jawd, seconds: 0.3) { _ in false }
        #expect(!after.contains { if case .notice = $0 { true } else { false } }, "an answered fetch or cancel says nothing")
    }

    @Test func `a save is answered with where it saved, or jawd's words, under the ticket it was asked with`() async throws {
        let fake = try FakeJawd(script: saving(refused: "m2"))
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        let first = jawd.save(chat: "ana", message: "m1", to: "~/Downloads")
        let second = jawd.save(chat: "ana", message: "m2", to: "~/Downloads", variant: .hd)
        #expect(first != second, "each save has a ticket of its own")
        let all = await drain(jawd) { all in all.count { if case .saved = $0 { true } else { false } } == 2 }
        let saved = all.compactMap { u -> (String, String, String?, String?)? in
            if case let .saved(ticket, message, path, why) = u { (ticket, message, path, why) } else { nil }
        }
        #expect(saved.first { $0.0 == first }.map { $0.1 == "m1" && $0.2 == "/Users/me/Downloads/m1.jpg" && $0.3 == nil } == true)
        #expect(saved.first { $0.0 == second }.map {
            $0.1 == "m2" && $0.2 == nil && $0.3 == "jawd couldn't download the file: your phone no longer has this file"
        } == true)
        #expect(requests(fake, "media.save").last == .object([
            "message": .string("m2"), "to": .string("~/Downloads"), "chat": .string("ana"), "variant": .string("hd"),
        ]))
    }

    @Test func `a fetch jawd refuses says so as the message it was for`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "media.fetch" ? [#"{"id":\#(id),"error":{"code":4,"message":"that message was deleted, so jaw doesn't keep its file"}}"#]
                : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.fetch(chat: "ana", message: "m9")
        let all = await drain(jawd) { all in all.contains { if case .notFetched = $0 { true } else { false } } }
        #expect(all.contains {
            if case let .notFetched(message, why) = $0 { message == "m9" && why == "that message was deleted, so jaw doesn't keep its file" } else { false }
        })
    }

    @Test func `a jawd from before media in says downloading and saving come later`() async throws {
        let fake = try FakeJawd { method, params, id in
            method.hasPrefix("media.") ? [#"{"id":\#(id),"error":{"code":-32601,"message":"no such method"}}"#] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.fetch(chat: "ana", message: "m1")
        let ticket = jawd.save(chat: "ana", message: "m1", to: "~/Downloads")
        let all = await drain(jawd) { all in
            all.contains { if case .notFetched = $0 { true } else { false } } && all.contains { if case .saved = $0 { true } else { false } }
        }
        #expect(all.contains { if case let .notFetched(_, why) = $0 { why == "downloading comes with the next step" } else { false } })
        #expect(all.contains { if case let .saved(t, _, path, why) = $0 { t == ticket && path == nil && why == "saving comes with the next step" } else { false } })
    }
}
