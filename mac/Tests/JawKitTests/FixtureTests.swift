import Foundation
@testable import JawKit
import Testing

/// Every file in protocol/fixtures decodes into JawKit's types, and each request JawKit writes
/// equals its fixture, so the Swift client keeps in step with jaw-proto's.

let fixtures = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .appending(path: "protocol/fixtures")

func fixture(_ name: String) throws -> Data {
    try Data(contentsOf: fixtures.appending(path: name))
}

/// Decodes a reply line as a client would, expecting what `want` says it was asked for.
func incoming(_ name: String, _ want: Want) throws -> Incoming {
    let decoder = JSONDecoder()
    decoder.userInfo[.want] = WantLookup { _ in want }
    return IncomingLine.decode(try fixture(name), with: decoder)
}

/// A request as JawKit writes it, as JSON to compare.
func written(id: UInt64, _ method: String, _ params: (some Encodable & Sendable)?) throws -> JSONValue {
    var line = Lines.encode(id: id, method: method, params: params)
    line.removeLast()
    return try JSONValue.parse(line)
}

@Test func `every result and push fixture decodes`() throws {
    let names = try FileManager.default.contentsOfDirectory(atPath: fixtures.path).filter { $0.hasSuffix(".json") }
    #expect(names.count > 40)
    for name in names where !name.contains(".request.") {
        let method = name.components(separatedBy: ".").dropLast(2).joined(separator: ".")
        let want: Want = switch method {
        case "hello": .hello
        case "find": .find
        case "chats.watch": .chats
        case "messages.watch", "watch.move": .messages
        case "send.text": .reply(.sent)
        case "chat.start": .reply(.chat)
        case "message.info": .reply(.info)
        case "chat.info": .reply(.chatInfo)
        case "status": .reply(.status)
        default: .reply(.json)
        }
        let value = try incoming(name, want)
        switch value {
        case .skip, .bad: Issue.record("\(name) doesn't decode: \(value)")
        case let .push(method, _) where ["chats", "messages", "resync", "connection", "notice", "link"].contains(method):
            Issue.record("\(name) decodes only as an untyped push")
        default: break
        }
    }
}

@Test func `rows decode as the protocol draws them`() throws {
    guard case let .messages(_, watched) = try incoming("messages.watch.result.json", .messages) else {
        Issue.record("not a messages window")
        return
    }
    #expect(watched.rows.count == 2 && watched.remove == ["3EB0C431F2A9B8E7D6"] && watched.older)
    let text = watched.rows[0]
    #expect(text.spans.map(\.t) == [.text, .bold, .text])
    #expect(text.plain == "did you get the photos from Sunday?")
    #expect(text.from == Sender(id: "person:12", name: "Ana", colour: 4))
    #expect(text.reactions == [Reaction(emoji: "❤", n: 2, mine: true)])
    let voice = watched.rows[1]
    #expect(voice.kind == .voice && voice.mine && voice.status == .read)
    #expect(voice.reply == Quote(id: "3EB0A1B2C3D4E5F6", from: "Ana", summary: "did you get the photos from Sunday?"))
    #expect(voice.voice?.waveform.count == 8)

    guard case let .chats(_, chats) = try incoming("chats.watch.result.json", .chats) else {
        Issue.record("not a chat list")
        return
    }
    #expect(chats.total == 312 && chats.rows.map(\.kind) == [.direct, .group])
    #expect(chats.rows[1].typing == ["person:7"] && chats.rows[1].draft == "see you" && chats.rows[1].mentions == 1)

    guard case let .found(_, found) = try incoming("find.result.json", .find) else {
        Issue.record("not find's answer")
        return
    }
    #expect(found.counts[.message] == 214)
    #expect(found.results.map(\.kind) == [.chat, .person, .folder, .message])
    if case .other = found.results[3].row { Issue.record("a message find found reads as a message") }

    guard case let .connection(c) = try incoming("connection.push.json", .nothing) else {
        Issue.record("not a connection push")
        return
    }
    #expect(c.state == .catchingUp && c.done == 1204 && c.total == 5000)

    guard case let .error(_, error) = try incoming("error.cancelled.json", .find) else {
        Issue.record("not an error")
        return
    }
    #expect(error.code == ErrorCode.cancelled)
}

@Test func `each request JawKit writes equals its fixture`() throws {
    func same(_ name: String, _ written: JSONValue) throws {
        #expect(written == (try JSONValue.parse(fixture(name))), "\(name)")
    }
    try same("hello.request.json",
             written(id: 1, "hello", Hello(client: "jaw", version: "0.1.0", focus: true, since: 81200)))
    try same("chats.watch.request.json", written(id: 2, "chats.watch", ChatsWatch(
        folder: "all", around: .top, before: 0, after: 60, since: nil, have: [],
    )))
    try same("messages.watch.request.json", written(id: 3, "messages.watch", MessagesWatch(
        chat: "351912345678@s.whatsapp.net", around: .newest, before: 80, after: 0, since: 81200,
        have: ["3EB0A1B2C3D4E5F6", "3EB0C431F2A9B8E7D6"],
    )))
    try same("watch.move.request.json", written(id: 4, "watch.move", WatchMove(
        watch: "w2", around: .row("3EB0A1B2C3D4E5F6"), before: 40, after: 40,
    )))
    try same("unwatch.request.json", written(id: 5, "unwatch", WatchRef(watch: "w1")))
    try same("send.text.request.json", written(id: 7, "send.text", SendText(
        chat: "351912345678@s.whatsapp.net", text: "on my way", key: "jaw-5f1c9a7e",
    )))
    try same("send.retry.request.json", written(id: 8, "send.retry", MessageRef(message: "3EB0C431F2A9B8E7D61A2B")))
    try same("chat.start.request.json", written(id: 9, "chat.start", ChatStart(phone: "+351 913 333 333")))
    try same("draft.set.request.json", written(id: 10, "draft.set", DraftSet(chat: "120363012345678901@g.us", text: "see you")))
    try same("read.request.json", written(id: 11, "read", ReadUpTo(chat: "351912345678@s.whatsapp.net", upTo: "3EB0A1B2C3D4E5F6")))
    try same("find.request.json", written(id: 12, "find", Find(query: "ar", kinds: [.chat, .person, .folder, .message], limit: 10)))
    try same("history.more.request.json", written(id: 12, "history.more", ChatRef(chat: "351912345678@s.whatsapp.net")))
    try same("typing.request.json", written(id: 12, "typing", Typing(chat: "351912345678@s.whatsapp.net", state: .composing)))
    try same("chat.opened.request.json", written(id: 13, "chat.opened", ChatRef(chat: "351912345678@s.whatsapp.net")))
    try same("focus.request.json", written(id: 14, "focus", Focus(focused: false)))
    try same("react.request.json", written(id: 41, "react", React(
        message: "3EB0A1B2C3D4E5F6", emoji: "❤️", chat: "351912345678@s.whatsapp.net", key: "jaw-7a2c90d1",
    )))
    try same("edit.request.json", written(id: 42, "edit", Edit(message: "3EB0C431F2A9B8E7D61A2B", text: "afinal chego às 21h")))
    try same("delete.request.json", written(id: 43, "delete", Delete(message: "3EB0C431F2A9B8E7D61A2B", for: .everyone)))
    try same("forward.request.json", written(id: 44, "forward", Forward(
        messages: ["3EB0A1B2C3D4E5F6"], to: ["351913000031@s.whatsapp.net"], key: "jaw-0c51e2aa",
    )))
    try same("message.info.request.json", written(id: 45, "message.info", MessageRef(message: "3EB0C431F2A9B8E7D61A2B")))
    try same("chat.info.request.json", written(id: 46, "chat.info", ChatRef(chat: "120363000000000002@g.us")))
    try same("status.request.json", written(id: 2, "status", NoParams?.none))
}

@Test func `keys are jaw and 32 hex digits, each its own`() {
    let keys = (0 ..< 100).map { _ in Jawd.key() }
    #expect(Set(keys).count == 100)
    for key in keys {
        #expect(key.hasPrefix("jaw-") && key.count == 36 && key.dropFirst(4).allSatisfy(\.isHexDigit))
    }
}

@Test func `a row that isn't the protocol leaves out only itself`() throws {
    var chats = try JSONValue.parse(try fixture("chats.watch.result.json"))
    guard case var .object(line) = chats, case var .object(result) = line["result"],
          case var .array(rows) = result["rows"], !rows.isEmpty else {
        Issue.record("chats.watch.result.json has no rows")
        return
    }
    rows.append(.object(["jid": .string("broken@s.whatsapp.net")]))
    result["rows"] = .array(rows)
    line["result"] = .object(result)
    chats = .object(line)

    let decoder = JSONDecoder()
    decoder.userInfo[.want] = WantLookup { _ in .chats }
    let data = try JSONEncoder().encode(chats)
    guard case let .chats(_, watched) = IncomingLine.decode(data, with: decoder) else {
        Issue.record("the window sank with its bad row")
        return
    }
    #expect(watched.rows.count == rows.count - 1)
    #expect(watched.unread == 1)
    #expect(watched.why != nil)
}
