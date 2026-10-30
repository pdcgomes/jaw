import Foundation
@testable import JawKit
import Testing

/// Polls and votes against a fake jawd on a real socket: a poll goes as a message does, with the key
/// made for it, asked again with that key after a lost answer and handed back whole when jawd turns
/// it down; a vote goes as an action does, with a key of its own.

@MainActor
@Suite(.serialized) struct PollClientTests {
    static let poll = SendPoll(chat: "ana", question: "Jantar?", options: ["Ramen", "Pizza"], max: 1, key: "jaw-poll")

    func send(_ jawd: Jawd) {
        jawd.sendPoll(chat: "ana", question: "Jantar?", options: ["Ramen", "Pizza"], max: 1, key: "jaw-poll")
    }

    @Test func `a poll goes with its key and comes back as the box's message does`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        send(jawd)
        let all = await drain(jawd) { all in all.contains { if case .sent = $0 { true } else { false } } }
        #expect(all.contains { if case let .sent(key, message, _) = $0 { key == "jaw-poll" && message == "3EB0poll" } else { false } })
        #expect(requests(fake, "send.poll") == [.object([
            "chat": .string("ana"), "question": .string("Jantar?"), "options": .array([.string("Ramen"), .string("Pizza")]),
            "max": .number(1), "key": .string("jaw-poll"),
        ])])
    }

    @Test func `a poll whose answer was lost is asked again with its key once jawd is back`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.stopAnswering()
        send(jawd)
        #expect(await eventually { requests(fake, "send.poll").count == 1 })
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let after = await drain(jawd) { all in all.contains { if case .sent = $0 { true } else { false } } }
        #expect(after.contains { if case let .unsure(chat, key) = $0 { chat == "ana" && key == "jaw-poll" } else { false } },
                "said to be unsure while jawd was away")
        #expect(after.contains { if case let .sent(key, _, _) = $0 { key == "jaw-poll" } else { false } }, "then sent, under the same key")
        let sends = requests(fake, "send.poll")
        #expect(sends.count == 2 && sends.allSatisfy { $0["key"]?.string == "jaw-poll" })
    }

    @Test func `a poll lost for longer than jawd keeps keys comes back whole`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.stopAnswering()
        send(jawd)
        #expect(await eventually { requests(fake, "send.poll").count == 1 })
        jawd.now = { Date().addingTimeInterval(21 * 60 * 60) }
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let after = await drain(jawd) { all in all.contains { if case .pollUnsent = $0 { true } else { false } } }
        #expect(after.contains { if case let .pollUnsent(chat, key, poll, why) = $0 {
            chat == "ana" && key == "jaw-poll" && poll == Self.poll && why.hasPrefix("jawd was away too long")
        } else { false } })
        #expect(requests(fake, "send.poll").count == 1, "and isn't asked again")
    }

    @Test func `a poll jawd turns down comes back whole, with jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "send.poll" else { return ordinary(method, params, id) }
            return [#"{"id":\#(id),"error":{"code":5,"message":"a poll has 12 options at most","data":{"limit":"options","most":12}}}"#]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        send(jawd)
        let all = await drain(jawd) { all in all.contains { if case .pollUnsent = $0 { true } else { false } } }
        #expect(all.contains { if case let .pollUnsent(chat, key, poll, why) = $0 {
            chat == "ana" && key == "jaw-poll" && poll == Self.poll && why == "a poll has 12 options at most"
        } else { false } })
        #expect(!all.contains { if case .unsure = $0 { true } else { false } })
    }

    @Test func `a vote goes with a key of its own and is asked again with it once jawd is back`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.stopAnswering()
        let key = jawd.vote(chat: "g", message: "3EB0DEMO00000005", options: ["Pizza"])
        #expect(key.hasPrefix("jaw-") && key.count == 36)
        #expect(await eventually { requests(fake, "poll.vote").count == 1 })
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let after = await drain(jawd) { all in all.contains { if case let .acted(k, _) = $0 { k == key } else { false } } }
        #expect(after.contains { if case let .unsure(chat, k) = $0 { chat == "g" && k == key } else { false } })
        let votes = requests(fake, "poll.vote")
        #expect(votes.count == 2 && votes.allSatisfy { $0 == votes[0] })
        #expect(votes.first == .object([
            "message": .string("3EB0DEMO00000005"), "options": .array([.string("Pizza")]), "chat": .string("g"), "key": .string(key),
        ]))
        let none = jawd.vote(chat: "g", message: "3EB0DEMO00000005", options: [])
        #expect(await eventually { requests(fake, "poll.vote").last?["key"]?.string == none })
        #expect(requests(fake, "poll.vote").last?["options"] == .array([]), "no options takes the vote back")
    }

    @Test func `a vote jawd turns down says why in its words, and one it doesn't serve says it comes later`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "poll.vote" else { return ordinary(method, params, id) }
            return (params["options"]?.array?.count ?? 0) > 1
                ? [#"{"id":\#(id),"error":{"code":4,"message":"this poll takes one choice, so a vote chooses one option at most"}}"#]
                : [#"{"id":\#(id),"error":{"code":-32601,"message":"no such method"}}"#]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        let two = jawd.vote(chat: "g", message: "p", options: ["Ramen", "Pizza"])
        let one = jawd.vote(chat: "g", message: "p", options: ["Ramen"])
        let all = await drain(jawd) { all in all.count { if case .acted = $0 { true } else { false } } == 2 }
        var said: [String: String] = [:]
        for case let .acted(key, words) in all {
            said[key] = words
        }
        #expect(said[two] == "this poll takes one choice, so a vote chooses one option at most")
        #expect(said[one] == "voting comes with the next step")
    }
}
