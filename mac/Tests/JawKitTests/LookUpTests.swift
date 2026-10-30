import Foundation
@testable import JawKit
import Testing

/// Chats looked up outside the caller's windows: a window of one row around each, in all and then
/// in archived, let go as soon as jawd answers, one chat at a time, so a lookup holds one of jawd's
/// 32 windows at most.

/// A jawd whose all holds ana and whose archived holds old, which has no chat gone and no window
/// left for busy; each watch has an id of its own.
func listing(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    guard method == "chats.watch", let folder = params["folder"]?.string, let around = params["around"]?.string, around != "top" else {
        return ordinary(method, params, id)
    }
    switch around {
    case "gone":
        return ["{\"id\":\(id),\"error\":{\"code\":3,\"message\":\"jawd has no chat gone\"}}"]
    case "busy":
        return ["{\"id\":\(id),\"error\":{\"code\":5,\"message\":\"a connection can watch at most 32 windows at once, so one has to be unwatched first\",\"data\":{\"limit\":\"windows\",\"most\":32}}}"]
    default:
        let holds = (folder, around) == ("all", "ana") || (folder, around) == ("archived", "old")
        let rows = holds ? FakeJawd.chat(around, v: 7, sort: "0~5", name: around) : ""
        return ["{\"id\":\(id),\"result\":{\"watch\":\"w\(id)\",\"seq\":10,\"total\":5,\"rows\":[\(rows)]}}"]
    }
}

/// What the lookups found, in the order they answered.
func lookedUp(_ all: [JawUpdate]) -> [(String, Looked)] {
    all.compactMap { if case let .looked(chat, found) = $0 { (chat, found) } else { nil } }
}

/// The windows the fake was asked for, in order: each watch's folder, chat and sides, and each unwatch.
func windows(_ fake: FakeJawd) -> [String] {
    fake.requests.compactMap { r -> String? in
        switch r.method {
        case "chats.watch":
            "\(r.params["folder"]?.string ?? "?") \(r.params["around"]?.string ?? "?") \(r.params["before"]?.int ?? -1)/\(r.params["after"]?.int ?? -1)"
        case "unwatch": "unwatch"
        default: nil
        }
    }
}

@MainActor
@Suite(.serialized) struct LookUpTests {
    @Test func `a chat outside the list is found in all, then in archived, and one jawd has none of is gone`() async throws {
        let fake = try FakeJawd(script: listing)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        jawd.lookUp(["ana", "old", "gone"])
        let found = lookedUp(await drain(jawd) { lookedUp($0).count == 3 })
        #expect(found.map { $0.0 } == ["ana", "old", "gone"], "in turn")
        if case let .row(row)? = found.first?.1 { #expect(row.id == "ana") } else { Issue.record("ana wasn't found: \(found)") }
        if case let .row(row)? = found.dropFirst().first?.1 { #expect(row.id == "old") } else { Issue.record("old wasn't found in archived: \(found)") }
        #expect(found.last?.1 == .gone)
        #expect(windows(fake) == ["all ana 0/0", "unwatch", "all old 0/0", "unwatch", "archived old 0/0", "unwatch", "all gone 0/0"],
                "one window at a time, each let go once it answered, and archived not asked about a chat jawd has none of")
    }

    @Test func `a window jawd refuses is said in its words, and the next chat is looked up`() async throws {
        let fake = try FakeJawd(script: listing)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        jawd.lookUp(["busy", "ana"])
        let found = lookedUp(await drain(jawd) { lookedUp($0).count == 2 })
        #expect(found.first?.0 == "busy")
        #expect(found.first?.1 == .refused("a connection can watch at most 32 windows at once, so one has to be unwatched first"))
        if case let .row(row)? = found.last?.1 { #expect(row.id == "ana") } else { Issue.record("ana wasn't looked up after: \(found)") }
    }

    @Test func `asking again replaces the chats still to look up, and the one under way still answers`() async throws {
        let fake = try FakeJawd(script: listing)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        jawd.lookUp(["ana", "old"])
        jawd.lookUp(["gone"])
        let found = lookedUp(await drain(jawd) { lookedUp($0).count == 2 })
        #expect(found.map { $0.0 } == ["ana", "gone"])
        #expect(!windows(fake).contains { $0.contains(" old ") }, "old was never looked for")
    }
}
