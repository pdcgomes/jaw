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
        case "send.text", "send.poll": .reply(.sent)
        case "chat.start": .reply(.chat)
        case "message.info": .reply(.info)
        case "chat.info": .reply(.chatInfo)
        case "status": .reply(.status)
        case "layout.get": .reply(.layout)
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
    #expect(chats.rows[1].typing.isEmpty && chats.rows[1].draft == "see you" && chats.rows[1].mentions == 1)

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

@Test func `the panes jawd keeps decode as a tree, with the chats of the panes it left out`() throws {
    guard case let .reply(_, .layout(kept)) = try incoming("layout.get.result.json", .reply(.layout)) else {
        Issue.record("not layout.get's answer")
        return
    }
    #expect(kept == KeptLayout(interface: "jaw", v: 1_792_000_000_123, layout: SplitLayout(focused: 1, tree: .split(
        .across, first: 500, .chat("351912345678@s.whatsapp.net"), .chat("120363012345678901@g.us"),
    )), dropped: ["351919999999@s.whatsapp.net"]))
    let none = try JSONDecoder().decode(KeptLayout.self, from: Data(#"{"interface":"jaw","v":0}"#.utf8))
    #expect(none == KeptLayout(interface: "jaw", v: 0), "nothing kept")
    let book = try JSONDecoder().decode(KeptLayout.self, from: Data(#"{"interface":"jaw-mac","v":5,"layout":{"format":2,"layouts":[]}}"#.utf8))
    #expect(book.v == 5 && book.layout == nil, "panes in a format other than 1's tree are none this Jaw reads")
    let malformed = #"{"interface":"jaw","v":5,"layout":{"format":1,"focused":0,"tree":{"split":{"how":"sideways"}}},"dropped":["351911111111@s.whatsapp.net"]}"#
    #expect(throws: DecodingError.self, "a malformed tree fails the answer rather than reading as panes left out") {
        try JSONDecoder().decode(KeptLayout.self, from: Data(malformed.utf8))
    }
}

@Test func `an invitation decodes with what it invites to and when it runs out`() throws {
    func row(_ kind: String) throws -> MessageRow {
        try JSONDecoder().decode(MessageRow.self, from: Data("""
        {"id":"3EB0A1B2C3D4E5F7","v":81231,"sort":"1792145070000~000000000124","chat":"351912345678@s.whatsapp.net",
         "at":1792145070000,"from":{"id":"person:12","name":"Ana","colour":4},"mine":false,\(kind),"reply":null,
         "forwarded":0,"reactions":[],"status":null,"edited":false,"deleted":false,"expires_at":null,"view_once":false,
         "starred":false}
        """.utf8))
    }
    let group = try row("""
    "kind":"invite","spans":[],"invite":{"group":"120363012345678901@g.us","name":"Lisbon","expires":1792404270000}
    """)
    #expect(group.kind == .invite && group.spans.isEmpty)
    #expect(group.invite?.group == "120363012345678901@g.us" && group.invite?.channel == nil)
    #expect(group.invite?.name == "Lisbon" && group.invite?.expires == 1_792_404_270_000 && group.invite?.admin != true)

    let admin = try row("""
    "kind":"invite","spans":[{"t":"text","s":"help me run it?"}],
    "invite":{"channel":"120363000000000007@newsletter","name":"Jaw news","expires":1792600000000,"admin":true}
    """)
    #expect(admin.invite?.channel == "120363000000000007@newsletter" && admin.invite?.group == nil)
    #expect(admin.invite?.admin == true && admin.invite?.expires == 1_792_600_000_000)
    #expect(admin.plain == "help me run it?", "its caption is the row's spans")

    let unnamed = try row(#""kind":"invite","spans":[],"invite":{"group":null,"name":null,"expires":null}"#)
    #expect(unnamed.kind == .invite && unnamed.invite != nil && unnamed.invite?.name == nil)
}

@Test func `a link's preview and a poll's tallies decode as jawd sends them`() throws {
    guard case let .reply(_, .sent(sent)) = try incoming("send.poll.result.json", .reply(.sent)), let poll = sent.row else {
        Issue.record("not send.poll's answer")
        return
    }
    #expect(sent.message == "3EB0F2A9B8E7D61A2B4C5D" && poll.kind == .poll && poll.mine && poll.status == .pending)
    #expect(poll.poll?.question == "Onde jantamos sexta?" && poll.poll?.max == 1 && poll.poll?.voters == 0)
    #expect(poll.poll?.options.map(\.name) == ["Ramen", "Tasca do Chico", "Pizza"] && poll.poll?.mine == [])

    func row(_ kind: String) throws -> MessageRow {
        try JSONDecoder().decode(MessageRow.self, from: Data("""
        {"id":"3EB0DEMO00000004","v":81240,"sort":"1792145190123~000000000126","chat":"351912345678@s.whatsapp.net",
         "at":1792145190123,"from":{"id":"person:12","name":"Ana","colour":4},"mine":false,\(kind)}
        """.utf8))
    }
    let link = try row("""
    "kind":"text","spans":[{"t":"text","s":"a previsão "},{"t":"link","s":"https://www.ipma.pt/pt/otempo/","url":"https://www.ipma.pt/pt/otempo/"}],
    "link":{"url":"https://www.ipma.pt/pt/otempo/","title":"Previsão para Sintra · IPMA","description":"Céu pouco nublado.","preview":"media/links/3f2a.jpg"}
    """)
    #expect(link.link == LinkPreview(url: "https://www.ipma.pt/pt/otempo/", title: "Previsão para Sintra · IPMA",
                                     description: "Céu pouco nublado.", preview: "media/links/3f2a.jpg"))
    #expect(link.link?.host == "ipma.pt", "the host, without www.")
    let history = try row(#""kind":"text","spans":[],"link":{"url":"https://climb.pt/sintra","title":"Sintra"}"#)
    #expect(history.link?.preview == nil && history.link?.host == "climb.pt", "history's previews come without their thumbnail")
    #expect(LinkPreview(url: "not a link").host == "not a link")
    #expect(LinkPreview(url: "https://müller.de/x").host == "müller.de", "its own letters, not percent escapes")
    #expect(LinkPreview(url: "https://WWW.Example.COM/Path").host == "example.com", "in lower case, its www. dropped then")
    #expect(LinkPreview(url: "https://ana@www.climb.pt:8443/sintra").host == "climb.pt", "without who or the port")
    #expect(LinkPreview(url: "https://wwwx.pt/").host == "wwwx.pt" && LinkPreview(url: "https://www2.ipma.pt/").host == "www2.ipma.pt")

    let voted = try row("""
    "kind":"poll","spans":[{"t":"text","s":"Which day?"}],"poll":{"question":"Which day?","max":2,"voters":4,
    "options":[{"name":"Saturday","votes":3,"mine":true},{"name":"Sunday","votes":2,"mine":true},{"name":"Monday","votes":0,"mine":false}]}
    """)
    #expect(voted.poll?.voters == 4 && voted.poll?.max == 2 && voted.poll?.mine == ["Saturday", "Sunday"])
    let older = try row(#""kind":"poll","spans":[],"poll":{"question":"Jantar?","options":[{"name":"Ramen","votes":1},{"name":"Pizza"}]}"#)
    #expect(older.poll?.voters == 0 && older.poll?.max == 1 && older.poll?.options.map(\.votes) == [1, 0], "what an older jawd leaves out")
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
    try same("send.poll.request.json", written(id: 12, "send.poll", SendPoll(
        chat: "120363012345678901@g.us", question: "Onde jantamos sexta?", options: ["Ramen", "Tasca do Chico", "Pizza"], max: 1,
        key: "jaw-1c2d3e4f",
    )))
    try same("poll.vote.request.json", written(id: 13, "poll.vote", PollVote(
        message: "3EB0DEMO00000005", options: ["Pizza"], chat: "120363012345678901@g.us", key: "jaw-9e8d7c6b",
    )))
    try same("forward.request.json", written(id: 44, "forward", Forward(
        messages: ["3EB0A1B2C3D4E5F6"], to: ["351913000031@s.whatsapp.net"], key: "jaw-0c51e2aa",
    )))
    try same("message.info.request.json", written(id: 45, "message.info", MessageRef(message: "3EB0C431F2A9B8E7D61A2B")))
    try same("chat.info.request.json", written(id: 46, "chat.info", ChatRef(chat: "120363000000000002@g.us")))
    try same("layout.get.request.json", written(id: 30, "layout.get", LayoutGet(interface: "jaw")))
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
