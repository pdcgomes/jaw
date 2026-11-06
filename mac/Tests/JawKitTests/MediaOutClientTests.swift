import Foundation
@testable import JawKit
import Testing

/// Files through the client, against a fake jawd on a real socket: sent with their key, the caption on
/// the first alone, and back as jawd's rows; asked again with that key after a lost answer; handed back
/// whole, in jawd's words, when jawd turns them down; and how far each copy has got, as jawd's `job`
/// pushes say it.

let lake = SendMedia.File(path: "/Users/me/Pictures/lake.jpg", caption: "the lake on Sunday")
let swim = SendMedia.File(path: "/Users/me/Movies/swim.mov")
let notes = SendMedia.File(path: "/Users/me/Documents/notes.pdf", kind: .document)

/// A script that answers `send.media` as jawd does, with its fixture's messages and rows.
func attaching(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    guard method == "send.media" else { return ordinary(method, params, id) }
    return [FakeJawd.answer("send.media.result.json", id: id)]
}

func mediaSent(_ all: [JawUpdate]) -> [(key: String, messages: [String], rows: [MessageRow])] {
    all.compactMap { if case let .mediaSent(key, messages, rows) = $0 { (key, messages, rows) } else { nil } }
}

func mediaUnsent(_ all: [JawUpdate]) -> [(chat: String, key: String, files: [SendMedia.File], why: String)] {
    all.compactMap { if case let .mediaUnsent(chat, key, files, why) = $0 { (chat, key, files, why) } else { nil } }
}

@MainActor
@Suite(.serialized) struct MediaOutClientTests {
    @Test func `files go with their key, quality and reply, the caption on the first alone, and come back as jawd's rows`() async throws {
        let fake = try FakeJawd(script: attaching)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.sendMedia(chat: "ana", files: [lake, swim, notes], quality: .hd, reply: "3EB0A1B2C3D4E5F6", key: "jaw-7c2e19a4")
        let all = await drain(jawd) { !mediaSent($0).isEmpty }
        let sent = try #require(mediaSent(all).first)
        #expect(sent.key == "jaw-7c2e19a4")
        #expect(sent.messages == ["3EB0D7A16C2B9F04E1A3C5", "3EB0E2F93B7A1C6D58049F"])
        #expect(sent.rows.map(\.kind) == [.album, .image, .video] && sent.rows.allSatisfy { $0.status == .pending })
        #expect(requests(fake, "send.media") == [.object([
            "chat": .string("ana"), "quality": .string("hd"), "reply": .string("3EB0A1B2C3D4E5F6"), "key": .string("jaw-7c2e19a4"),
            "files": .array([
                .object(["path": .string("/Users/me/Pictures/lake.jpg"), "caption": .string("the lake on Sunday")]),
                .object(["path": .string("/Users/me/Movies/swim.mov")]),
                .object(["path": .string("/Users/me/Documents/notes.pdf"), "kind": .string("document")]),
            ]),
        ])])
    }

    @Test func `files whose answer was lost are asked again with their key, and past a day come back whole`() async throws {
        let fake = try FakeJawd(script: attaching)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.stopAnswering()
        jawd.sendMedia(chat: "ana", files: [lake], quality: .sd, reply: nil, key: "jaw-lost")
        #expect(await eventually { requests(fake, "send.media").count == 1 })
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let after = await drain(jawd) { !mediaSent($0).isEmpty }
        #expect(after.contains { if case let .unsure(chat, key) = $0 { chat == "ana" && key == "jaw-lost" } else { false } },
                "said to be unsure while jawd was away")
        #expect(mediaSent(after).map(\.key) == ["jaw-lost"], "then sent, under the same key")
        #expect(requests(fake, "send.media").map { $0["key"]?.string } == ["jaw-lost", "jaw-lost"])
        #expect(requests(fake, "send.media").map { $0["quality"]?.string } == ["sd", "sd"])

        fake.stopAnswering()
        jawd.sendMedia(chat: "ana", files: [lake, swim], quality: .sd, reply: nil, key: "jaw-old")
        #expect(await eventually { requests(fake, "send.media").count == 3 })
        jawd.now = { Date().addingTimeInterval(21 * 60 * 60) }
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let late = await drain(jawd) { !mediaUnsent($0).isEmpty }
        let back = try #require(mediaUnsent(late).first)
        #expect(back.chat == "ana" && back.key == "jaw-old" && back.files == [lake, swim])
        #expect(back.why == "jawd was away too long to tell whether it went, so look in the chat before sending it again")
        #expect(requests(fake, "send.media").count == 3, "and isn't asked again")
    }

    @Test func `files jawd turns down come back whole in its words, a file named by its name rather than its path`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "send.media" else { return ordinary(method, params, id) }
            return switch params["key"]?.string {
            case "jaw-kind":
                [#"{"id":\#(id),"error":{"code":-32602,"message":"/Users/me/Movies/swim.mov can't go as image","data":{"param":"files","file":1,"path":"/Users/me/Movies/swim.mov"}}}"#]
            case "jaw-many":
                [#"{"id":\#(id),"error":{"code":5,"message":"jaw sends 30 files at most at once","data":{"limit":"files","most":30}}}"#]
            case "jaw-full":
                [#"{"id":\#(id),"error":{"code":5,"message":"40 messages of yours are waiting to send, 2 of them failed; jawd takes more once some have gone or been retried","data":{"limit":"outbox","waiting":40,"failed":2}}}"#]
            case "jaw-chat":
                [#"{"id":\#(id),"error":{"code":3,"message":"jawd has no chat ana; chat.start finds or makes one"}}"#]
            default:
                [#"{"id":\#(id),"error":{"code":-32601,"message":"unknown method: send.media"}}"#]
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        for key in ["jaw-kind", "jaw-many", "jaw-full", "jaw-chat", "jaw-before"] {
            jawd.sendMedia(chat: "ana", files: [lake, swim], quality: .sd, reply: nil, key: key)
        }
        let all = await drain(jawd) { mediaUnsent($0).count == 5 }
        let said = mediaUnsent(all).filter { $0.chat == "ana" && $0.files == [lake, swim] }.map { "\($0.key): \($0.why)" }
        #expect(said == [
            "jaw-kind: swim.mov can't go as image",
            "jaw-many: jaw sends 30 files at most at once",
            "jaw-full: 40 wait to send, 2 failed; it can go once some have",
            "jaw-chat: jawd has no chat ana; chat.start finds or makes one",
            "jaw-before: sending files comes with the next step",
        ])

        let away = Jawd(socket: fake.socket + ".nobody", client: "jaw-mac-test", version: "0", focus: true) { _ in }
        away.start()
        defer { away.close() }
        away.sendMedia(chat: "ana", files: [lake], quality: .sd, reply: nil, key: "jaw-away")
        let back = await drain(away) { !mediaUnsent($0).isEmpty }
        #expect(mediaUnsent(back).map(\.why) == ["jawd isn't connected; jaw is trying to reach it, so try again in a moment"])
    }

    @Test func `each copy's progress comes as a job, in the order jawd sends them`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.send(#"{"method":"job","params":{"id":"transcode:7","kind":"transcode","message":"3EB0V","variant":"sd","progress":0.5}}"#)
        fake.send(try FakeJawd.pushed("job.push.json"))
        fake.send(#"{"method":"job","params":{"id":"thumb:9","kind":"thumbnails","message":"3EB0V","progress":1}}"#)
        let all = await drain(jawd) { $0.count { if case .job = $0 { true } else { false } } == 3 }
        let jobs = all.compactMap { if case let .job(job) = $0 { job } else { nil } }
        #expect(jobs.map(\.kind) == [.transcode, .upload, .other], "a kind this client doesn't know reads as other")
        #expect(jobs.map(\.message) == ["3EB0V", "3EB0D7A16C2B9F04E1A3C5", "3EB0V"])
        #expect(jobs.map(\.progress) == [0.5, 0.42, 1] && jobs.map(\.variant) == [.sd, .sd, nil])
        #expect(!all.contains { if case .push = $0 { true } else { false } }, "none of them as an untyped push")
    }

    @Test func `status asked quietly is read, not shown, and stopping a file says what it stops`() async throws {
        let fake = try FakeJawd { method, params, id in
            switch method {
            case "status": [FakeJawd.answer("status.result.json", id: id)]
            case "media.cancel": [#"{"id":\#(id),"error":{"code":3,"message":"jawd has no message 3EB0X"}}"#]
            default: ordinary(method, params, id)
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.status(shown: false)
        jawd.cancel(chat: "ana", message: "3EB0X", what: "stopping it")
        let all = await drain(jawd) { all in
            all.contains { if case .statusRead = $0 { true } else { false } } && all.contains { if case .notice = $0 { true } else { false } }
        }
        let read = all.compactMap { if case let .statusRead(s) = $0 { s } else { nil } }.first
        #expect(read?.missing.map(\.what) == ["ffmpeg"])
        #expect(!all.contains { if case .status = $0 { true } else { false } }, "not as the status sheet's")
        #expect(all.contains { if case let .notice(text, _) = $0 { text == "jawd has no message 3EB0X" } else { false } })
        #expect(requests(fake, "media.cancel") == [.object(["message": .string("3EB0X"), "chat": .string("ana")])])
    }
}
