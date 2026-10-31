import Foundation
@testable import JawKit
import Synchronization
import Testing

/// Who's typing, as the client keeps it against a fake jawd: each answer says it for every chat of
/// its window, a push for its chat while a window shows it, and it's forgotten as a chat leaves every
/// window and when the connection drops, as jaw-client's `client.rs` keeps it.

func typist(_ id: String, _ name: String, _ state: String = "typing") -> String {
    #"{"id":"\#(id)","name":"\#(name)","state":"\#(state)"}"#
}

/// A chat list's answer with `chats` as its rows, newest first, and `typing` as its typing map.
func chatList(_ id: UInt64, _ chats: [String], typing: String) -> String {
    let rows = chats.enumerated().map { FakeJawd.chat($0.element, v: 3, sort: "0~\(9 - $0.offset)", name: $0.element) }
    return #"{"id":\#(id),"result":{"watch":"w1","seq":10,"total":\#(chats.count),"rows":[\#(rows.joined(separator: ","))],"typing":\#(typing)}}"#
}

/// A conversation's answer with no rows, saying who's typing in it as `typing`.
func conversation(_ id: UInt64, typing: String) -> String {
    #"{"id":\#(id),"result":{"watch":"w2","seq":10,"total":0,"older":false,"newer":0,"rows":[],"typing":\#(typing)}}"#
}

func typingPush(_ chat: String, _ who: String...) -> String {
    #"{"method":"typing","params":{"chat":"\#(chat)","who":[\#(who.joined(separator: ","))]}}"#
}

/// The newest of who's typing that `updates` handed over, if they handed it over.
func typing(_ updates: [JawUpdate]) -> Typists? {
    updates.reversed().lazy.compactMap { if case let .typing(t) = $0 { t } else { nil } }.first
}

/// How many times a fake's script has been asked something, kept across its calls.
final class Count: Sendable {
    let n = Mutex(0)

    func next() -> Int {
        n.withLock { n in
            n += 1
            return n
        }
    }
}

/// The watches a fake hasn't answered yet, their request ids by chat, kept across its calls.
final class Unanswered: Sendable {
    let ids = Mutex<[String: UInt64]>([:])

    func hold(_ chat: String, _ id: UInt64) {
        ids.withLock { $0[chat] = id }
    }

    func id(_ chat: String) -> UInt64? {
        ids.withLock { $0[chat] }
    }
}

let anaTyping = Typist(id: "person:2", name: "Ana")
let ruiRecording = Typist(id: "person:3", name: "Rui", state: .recording)
let zeTyping = Typist(id: "person:4", name: "Zé")

@MainActor
@Suite(.serialized) struct TypingClientTests {
    @Test func `a chat list's answer says who's typing in its chats, and pushes change it for the chats it shows`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "chats.watch" else { return ordinary(method, params, id) }
            return [chatList(id, ["ana", "rui", "ze"], typing: #"{"ana":[\#(typist("person:2", "Ana"))],"rui":[\#(typist("person:3", "Rui", "recording"))]}"#)]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = jawd.watchChats("all", .top(60))
        let first = await drain(jawd) { typing($0) != nil }
        #expect(typing(first) == ["ana": [anaTyping], "rui": [ruiRecording]], "Ana typing and Rui recording, as the answer says")

        fake.send(typingPush("ze", typist("person:4", "Zé")))
        fake.send(typingPush("rui"))
        fake.send(typingPush("elsewhere", typist("person:9", "Xavier")))
        let pushed = await drain(jawd) { typing($0) == ["ana": [anaTyping], "ze": [zeTyping]] }
        #expect(typing(pushed) == ["ana": [anaTyping], "ze": [zeTyping]],
                "Zé starts, Rui stops, and a chat no window shows isn't kept: \(String(describing: typing(pushed)))")

        fake.send(typingPush("ze", typist("person:4", "Zé")))
        fake.send(#"{"method":"chats","params":{"watch":"w1","seq":11,"upsert":[\#(FakeJawd.chat("rui", v: 11, sort: "0~9", name: "rui"))]}}"#)
        let same = await drain(jawd) { all in all.contains { if case .chats = $0 { true } else { false } } }
        #expect(typing(same) == nil, "what changed nothing hands nothing over")

        fake.send(#"{"method":"chats","params":{"watch":"w1","seq":12,"remove":["ana"]}}"#)
        let gone = await drain(jawd) { typing($0) != nil }
        #expect(typing(gone) == ["ze": [zeTyping]], "Ana's chat left the list, and her typing with it")
    }

    @Test func `a conversation's answer says who's typing in its chat, a move's says it for its window, and unwatching forgets`() async throws {
        let fake = try FakeJawd { method, params, id in
            switch method {
            case "chats.watch":
                [chatList(id, ["ana", "rui"], typing: #"{"ana":[\#(typist("person:2", "Ana"))]}"#)]
            case "messages.watch":
                [conversation(id, typing: #"{"rui":[\#(typist("person:3", "Rui", "recording"))]}"#)]
            case "watch.move":
                [conversation(id, typing: "{}")]
            default:
                ordinary(method, params, id)
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let list = jawd.watchChats("all", .top(60))
        let chat = jawd.watchMessages("rui", .newest(80))
        let both = await drain(jawd) { typing($0) == ["ana": [anaTyping], "rui": [ruiRecording]] }
        #expect(typing(both) == ["ana": [anaTyping], "rui": [ruiRecording]], "the conversation's answer says Rui's recording")

        jawd.unwatch(list)
        let unwatched = await drain(jawd) { typing($0) != nil }
        #expect(typing(unwatched) == ["rui": [ruiRecording]], "Ana's chat is in no window now; Rui's still is, in its conversation")

        fake.send(typingPush("ana", typist("person:2", "Ana")))
        jawd.move(chat, .around("m1", before: 40, after: 40))
        let after = await drain(jawd) { all in !requests(fake, "watch.move").isEmpty && typing(all) != nil }
        #expect(typing(after) == [:], "the move's answer leaves Rui out, so he's stopped, and Ana's chat isn't shown")
    }

    @Test func `a drop forgets who's typing at once, and watching again says how it is now`() async throws {
        let watches = Count()
        let fake = try FakeJawd { method, params, id in
            guard method == "chats.watch" else { return ordinary(method, params, id) }
            let who = switch watches.next() {
            case 1: #"{"ana":[\#(typist("person:2", "Ana"))]}"#
            case 2: #"{"rui":[\#(typist("person:3", "Rui", "recording"))]}"#
            default: "{}"
            }
            return [chatList(id, ["ana", "rui"], typing: who)]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = jawd.watchChats("all", .top(60))
        let first = await drain(jawd) { typing($0) != nil }
        #expect(typing(first) == ["ana": [anaTyping]])

        // Unanswered once it's back, so what the drop forgot shows before anything is said again.
        fake.stopAnswering()
        fake.drop()
        let dropped = await drain(jawd) { all in all.contains { if case .unreachable = $0 { true } else { false } } && typing(all) != nil }
        #expect(typing(dropped) == [:], "nobody's typing while jawd can't say otherwise")

        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let again = await drain(jawd) { typing($0) != nil }
        #expect(typing(again) == ["rui": [ruiRecording]], "the answer to watching again says how it is now")

        fake.send(#"{"method":"resync","params":{"watch":"w1"}}"#)
        let resynced = await drain(jawd) { typing($0) != nil }
        #expect(typing(resynced) == [:], "and so does the answer to watching again after a resync")
    }

    @Test func `a chat list's move whose answer takes out a chat someone's typing in forgets them`() async throws {
        let fake = try FakeJawd { method, params, id in
            switch method {
            case "chats.watch":
                return [chatList(id, ["ana", "rui"], typing: #"{"ana":[\#(typist("person:2", "Ana"))]}"#)]
            case "watch.move":
                let ze = FakeJawd.chat("ze", v: 4, sort: "0~1", name: "ze")
                return [#"{"id":\#(id),"result":{"watch":"w1","seq":11,"total":3,"rows":[\#(ze)],"remove":["ana"],"#
                    + #""typing":{"ze":[\#(typist("person:4", "Zé"))]}}}"#]
            default:
                return ordinary(method, params, id)
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let list = jawd.watchChats("all", .top(60))
        let first = await drain(jawd) { typing($0) != nil }
        #expect(typing(first) == ["ana": [anaTyping]])

        jawd.move(list, .around("rui", before: 1, after: 1))
        let moved = await drain(jawd) { all in !requests(fake, "watch.move").isEmpty && typing(all) != nil }
        #expect(typing(moved) == ["ze": [zeTyping]], "Ana's chat left the list with the move, and her typing with it; Zé's came in")
    }

    @Test func `conversations let go while their watches are in flight forget their typing at once, and their answer or failure changes nothing`() async throws {
        let held = Unanswered()
        let fake = try FakeJawd { method, params, id in
            switch method {
            case "chats.watch":
                return [chatList(id, ["ana"], typing: "{}")]
            case "messages.watch":
                held.hold(params["chat"]?.string ?? "", id)
                return []
            default:
                return ordinary(method, params, id)
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = jawd.watchChats("all", .top(60))
        let ze = jawd.watchMessages("ze", .newest(80)), rui = jawd.watchMessages("rui", .newest(80))
        #expect(await eventually { held.id("ze") != nil && held.id("rui") != nil }, "both watched, neither answered")
        fake.send(typingPush("ze", typist("person:4", "Zé")))
        fake.send(typingPush("rui", typist("person:3", "Rui", "recording")))
        let both = await drain(jawd) { typing($0) == ["ze": [zeTyping], "rui": [ruiRecording]] }
        #expect(typing(both) == ["ze": [zeTyping], "rui": [ruiRecording]], "a conversation shows its chat while its watch is in flight")

        jawd.unwatch(ze)
        jawd.unwatch(rui)
        let letGo = await drain(jawd) { typing($0) != nil }
        #expect(typing(letGo) == [:], "forgotten as they're let go, before either is answered")

        let zeID = try #require(held.id("ze")), ruiID = try #require(held.id("rui"))
        fake.send(conversation(zeID, typing: #"{"ze":[\#(typist("person:4", "Zé"))]}"#))
        fake.send(#"{"id":\#(ruiID),"error":{"code":3,"message":"jawd has no chat rui"}}"#)
        fake.send(typingPush("ze", typist("person:4", "Zé")))
        #expect(await eventually { requests(fake, "unwatch").map { $0["watch"]?.string } == ["w2"] }, "the answer's watch let go of")
        let after = await drain(jawd, seconds: 0.5) { _ in false }
        #expect(typing(after) == nil, "the answer's typing, and a push for its chat, change nothing")
        #expect(!after.contains { if case .windowFailed = $0 { true } else { false } }, "nor is the failure said")
    }
}
