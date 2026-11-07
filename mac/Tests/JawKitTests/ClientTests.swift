import Foundation
@testable import JawKit
import Testing

/// The client against a fake jawd on a real socket: hello, windows and their pushes, watching
/// again with `have` after a drop or a resync, what a dropped connection fails, the latest find,
/// each chat started answered once and in order, and the interface's half: keys asked again,
/// drafts owed.

/// A script that answers hello, watches and anything else as jawd would for a small account.
func ordinary(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    switch method {
    case "hello":
        return [FakeJawd.welcome(id)]
    case "chats.watch":
        let rows = [FakeJawd.chat("ana", v: 3, sort: "0~2", name: "Ana"), FakeJawd.chat("rui", v: 2, sort: "0~1", name: "Rui")]
        let have = Set(params["have"]?.array?.compactMap(\.string) ?? [])
        let remove = have.subtracting(["ana", "rui"]).map { "\"\($0)\"" }.joined(separator: ",")
        return ["{\"id\":\(id),\"result\":{\"watch\":\"w1\",\"seq\":10,\"total\":2,\"rows\":[\(rows.joined(separator: ","))],\"remove\":[\(remove)]}}"]
    case "messages.watch":
        let rows = [FakeJawd.message("m1", v: 5, sort: "1", text: "hi"), FakeJawd.message("m2", v: 6, sort: "2", text: "there")]
        return ["{\"id\":\(id),\"result\":{\"watch\":\"w2\",\"seq\":10,\"total\":2,\"older\":false,\"newer\":0,\"rows\":[\(rows.joined(separator: ","))]}}"]
    case "send.text", "send.poll":
        let key = params["key"]?.string ?? ""
        return ["{\"id\":\(id),\"result\":{\"message\":\"3EB0\(key.suffix(4))\"}}"]
    default:
        return ["{\"id\":\(id),\"result\":{}}"]
    }
}

@MainActor
func connected(_ fake: FakeJawd) -> Jawd {
    let jawd = Jawd(socket: fake.socket, client: "jaw-mac-test", version: "0", focus: true) { _ in }
    jawd.start()
    return jawd
}

func helloed(_ all: [JawUpdate]) -> Int {
    all.count { if case .hello = $0 { true } else { false } }
}

@MainActor
@Suite(.serialized) struct ClientTests {
    @Test func `hello, then a window's rows, then its pushes merged into one update`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let list = jawd.watchChats("all", .top(60))
        var chats = Chats<ChatRow>(folder: "all", place: .top(60))
        let first = await drain(jawd) { all in all.contains { if case .chats = $0 { true } else { false } } }
        #expect(helloed(first) == 1)
        for case let .chats(w, update) in first where w == list {
            chats.apply(update)
        }
        #expect(chats.list.map(\.id) == ["ana", "rui"] && chats.total == 2)
        #expect(requests(fake, "hello").first?["client"]?.string == "jaw-mac-test")

        fake.send("{\"method\":\"chats\",\"params\":{\"watch\":\"w1\",\"seq\":11,\"upsert\":[\(FakeJawd.chat("rui", v: 11, sort: "0~9", name: "Rui"))]}}")
        fake.send("{\"method\":\"chats\",\"params\":{\"watch\":\"w1\",\"seq\":12,\"remove\":[\"ana\"]}}")
        try await Task.sleep(for: .milliseconds(100))
        let pushed = jawd.drain()
        let updates = pushed.compactMap { if case let .chats(w, u) = $0, w == list { u } else { nil } }
        #expect(updates.count == 1, "two pushes between frames come as one update")
        for update in updates {
            chats.apply(update)
        }
        #expect(chats.list.map(\.id) == ["rui"] && chats.seq == 12)
    }

    @Test func `after a drop it says hello again and watches with the rows it holds`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = jawd.watchChats("all", .top(60))
        _ = jawd.watchMessages("ana", .newest(80))
        _ = await drain(jawd) { all in all.count { if case .messages = $0 { true } else { false } } == 1 }
        fake.drop()
        let again = await drain(jawd) { helloed($0) == 1 && $0.contains { if case .messages = $0 { true } else { false } } }
        #expect(again.contains { if case .unreachable = $0 { true } else { false } }, "the drop is said")
        #expect(helloed(again) == 1, "and hello comes again")
        #expect(fake.connections == 2)
        let watches = requests(fake, "messages.watch")
        #expect(watches.count == 2)
        #expect(watches.last?["have"] == .array([.string("m1"), .string("m2")]), "watched again with what it holds")
        #expect(watches.last?["since"]?.int == 10, "from the newest seq it saw")
        #expect(requests(fake, "chats.watch").last?["since"] == nil, "a chat list comes whole")
    }

    @Test func `a resync watches that window again`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = jawd.watchMessages("ana", .newest(80))
        _ = await drain(jawd) { $0.contains { if case .messages = $0 { true } else { false } } }
        fake.send("{\"method\":\"resync\",\"params\":{\"watch\":\"w2\"}}")
        #expect(await eventually { requests(fake, "messages.watch").count == 2 })
        #expect(jawd.client.stats.resyncs == 1)
    }

    @Test func `a message whose answer was lost is asked again with its key once jawd is back`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.stopAnswering()
        jawd.sendText(chat: "ana", text: "on my way", key: "jaw-abcd")
        jawd.draft(chat: "ana", text: "half typ")
        #expect(await eventually { requests(fake, "send.text").count == 1 && requests(fake, "draft.set").count == 1 })
        // The same fake answers again once it's dropped and taken back.
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let after = await drain(jawd) { all in all.contains { if case .sent = $0 { true } else { false } } }
        #expect(after.contains { if case let .unsure(_, key) = $0 { key == "jaw-abcd" } else { false } },
                "said to be unsure while jawd was away")
        #expect(after.contains { if case let .sent(key, _, _) = $0 { key == "jaw-abcd" } else { false } },
                "then sent, under the same key")
        let sends = requests(fake, "send.text")
        #expect(sends.count == 2 && sends.allSatisfy { $0["key"]?.string == "jaw-abcd" })
        #expect(requests(fake, "draft.set").count == 2, "the draft jawd hadn't answered is asked again")
    }

    @Test func `a message lost for longer than jawd keeps keys goes back to its box`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.stopAnswering()
        jawd.sendText(chat: "ana", text: "on my way", key: "jaw-old")
        #expect(await eventually { requests(fake, "send.text").count == 1 })
        jawd.now = { Date().addingTimeInterval(21 * 60 * 60) }
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let after = await drain(jawd) { all in all.contains { if case .unsent = $0 { true } else { false } } }
        #expect(after.contains { if case let .unsent(_, key, text, _) = $0 { key == "jaw-old" && text == "on my way" } else { false } })
        #expect(requests(fake, "send.text").count == 1, "and isn't asked again")
    }

    @Test func `only the latest find is answered`() async throws {
        let fake = try FakeJawd { method, params, id in
            if method == "find" {
                let q = params["query"]?.string ?? ""
                return q == "a"
                    ? ["{\"id\":\(id),\"error\":{\"code\":7,\"message\":\"a newer find replaced this one\"}}"]
                    : ["{\"id\":\(id),\"result\":{\"results\":[],\"counts\":{\"chat\":3}}}"]
            }
            return ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.find(id: 1, query: "a")
        jawd.find(id: 2, query: "an")
        let found = await drain(jawd) { all in all.contains { if case .found = $0 { true } else { false } } }
        let answers = found.compactMap { if case let .found(id, f) = $0 { (id, f.counts[.chat]) } else { nil } }
        #expect(answers.count == 1 && answers[0].0 == 2 && answers[0].1 == 3)
    }

    @Test func `the terminal's panes are asked for by its name and come whole, and a jawd without them says so`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "layout.get" else { return ordinary(method, params, id) }
            return ["{\"id\":\(id),\"result\":{\"interface\":\"jaw\",\"v\":7,\"layout\":{\"format\":1,\"focused\":1,\"tree\":{\"split\":{\"how\":\"down\",\"first\":400,\"a\":{\"chat\":\"ana\"},\"b\":{\"chat\":\"rui\"}}}},\"dropped\":[\"mum\"]}}"]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.layout(of: "jaw", what: "opening the terminal's panes")
        let all = await drain(jawd) { all in all.contains { if case .layout = $0 { true } else { false } } }
        let kept = all.compactMap { if case let .layout(k) = $0 { k } else { nil } }.first
        #expect(kept == KeptLayout(interface: "jaw", v: 7, layout: SplitLayout(focused: 1, tree: .split(.down, first: 400, .chat("ana"), .chat("rui"))),
                                   dropped: ["mum"]))
        #expect(requests(fake, "layout.get") == [.object(["interface": .string("jaw")])])
        let refused = jawd.refused(.layout("opening the terminal's panes"), .jawd(JawError(code: ErrorCode.unknownMethod, message: "?")))
        guard case let .notice(text, tone)? = refused.first else {
            Issue.record("nothing said: \(refused)")
            return
        }
        #expect(text == "opening the terminal's panes comes with the next step" && tone == .warning)
    }

    @Test func `Omarchy's themes come as jawd serves them, and a jawd from before them is told apart from a drop`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "themes" ? [FakeJawd.answer("themes.result.json", id: id)] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.themes()
        let all = await drain(jawd) { all in all.contains { if case .themes = $0 { true } else { false } } }
        let themes: [ServedTheme] = all.compactMap { if case let .themes(t) = $0 { t } else { nil } }.first ?? []
        #expect(themes.map(\.id) == ["catppuccin", "catppuccin-latte"] && themes.map(\.mode) == [.dark, .light])
        #expect(requests(fake, "themes") == [.null], "asked with no params")
        let older = jawd.refused(.themes, .jawd(JawError(code: ErrorCode.unknownMethod, message: "unknown method: themes")))
        let busy = jawd.refused(.themes, .jawd(JawError(code: ErrorCode.limit, message: "jawd is busy")))
        guard case let .noThemes(isOlder, _)? = older.first, case let .noThemes(busyIsOlder, why)? = busy.first else {
            Issue.record("nothing said: \(older), \(busy)")
            return
        }
        #expect(isOlder && !busyIsOlder && why == "jawd is busy")
        #expect(jawd.refused(.themes, .lost).isEmpty && jawd.refused(.themes, .unreachable).isEmpty,
                "a drop says nothing, as the next hello asks again")
    }

    @Test func `what jawd refuses to send says why, the outbox in a few words`() {
        let full = Failure.jawd(JawError(code: ErrorCode.limit, message: "the outbox is full",
                                         data: .object(["limit": .string("outbox"), "waiting": .number(1000), "failed": .number(2)])))
        #expect(Jawd.whyUnsent(full) == "1000 wait to send, 2 failed; it can go once some have")
        #expect(Jawd.whyNot(.jawd(JawError(code: ErrorCode.unknownMethod, message: "?")), "starring") == "starring comes with the next step")
    }

    @Test func `each chat started is answered once, in the order sent: with its chat, jawd's refusal, or the drop`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "chat.start" else { return ordinary(method, params, id) }
            switch params["phone"]?.string ?? params["person"]?.string {
            case "+351 913 333 333":
                return ["{\"id\":\(id),\"result\":{\"chat\":\(FakeJawd.chat("351913333333@s.whatsapp.net", v: 9, sort: "0~9", name: "Rui"))}}"]
            case "person:7": return [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no person:7")]
            default: return []
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        func answers(_ all: [JawUpdate]) -> [String] {
            all.compactMap {
                switch $0 {
                case let .chatStarted(row): row.id
                case let .notStarted(why): "not started: \(why)"
                default: nil
                }
            }
        }
        jawd.startChat(phone: "+351 913 333 333")
        jawd.startChat(person: "person:7")
        jawd.startChat(phone: "+351 914 444 444")
        let two = await drain(jawd) { answers($0).count == 2 }
        #expect(answers(two) == ["351913333333@s.whatsapp.net", "not started: jawd has no person:7"])
        #expect(await eventually { requests(fake, "chat.start").count == 3 })
        fake.drop()
        let third = await drain(jawd) { answers($0).count == 1 }
        #expect(answers(third) == ["not started: the connection to jawd dropped before it answered, so it may not have been done"],
                "the third, which jawd never answered, is said once as the connection drops, and not asked again")
    }
}
