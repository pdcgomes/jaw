import Foundation
@testable import JawKit
import Synchronization
import Testing

/// The client watches jawd's folder tree and a roster, and asks for what the folder items do: the
/// tree whole with its slots, its pushes merged, watched again whole with the folders held; a roster
/// like a chat list; a jawd without folders refusing the tree; a deleted folder's list failing once
/// it's watched again; Move to's two requests in their order, finished after a drop, and what's said
/// when it ends in both folders; a name's answer, with the folder made or without one gone at once,
/// and its refusal; and what's said once jawd has done each.

/// A script that answers as jawd's folders branch does: the tree, a roster and a made folder from its
/// fixtures, a roster's move as its watch, and every other request done.
func foldering(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    switch method {
    case "folders.watch": [FakeJawd.answer("folders.watch.result.json", id: id)]
    case "roster.watch", "watch.move": [FakeJawd.answer("roster.watch.result.json", id: id)]
    case "folder.create": [FakeJawd.answer("folder.create.result.json", id: id)]
    default: ordinary(method, params, id)
    }
}

/// A request's id, held back unanswered until the test answers it.
final class HeldBack: Sendable {
    let id = Mutex<UInt64?>(nil)
}

/// How many times the fake was asked for something; a class, since a script can't hold a `Mutex`.
final class Counted: Sendable {
    let n = Mutex(0)

    /// Counts one more, and says how many there have been.
    func next() -> Int {
        n.withLock { n in
            n += 1
            return n
        }
    }
}

func trees(_ all: [JawUpdate], _ window: WindowID) -> [Update<FolderRow>] {
    all.compactMap { if case let .folders(w, u) = $0, w == window { u } else { nil } }
}

func rosters(_ all: [JawUpdate], _ window: WindowID) -> [Update<RosterRow>] {
    all.compactMap { if case let .roster(w, u) = $0, w == window { u } else { nil } }
}

func notices(_ all: [JawUpdate]) -> [String] {
    all.compactMap { if case let .notice(text, _) = $0 { text } else { nil } }
}

/// Waits until jawd has everything asked before now: requests go out in order on the one socket, so
/// once a marker asked after them arrives, anything they'd have sent with them has arrived too.
@MainActor
func settled(_ jawd: Jawd, _ fake: FakeJawd) async -> Bool {
    let before = requests(fake, "favourite.set").count
    jawd.favouriteSet(slot: 9, target: nil, said: nil)
    return await eventually { requests(fake, "favourite.set").count == before + 1 }
}

@MainActor
@Suite(.serialized) struct FolderClientTests {
    @Test func `the tree comes whole with its slots, and a push changes a row and the slots`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        let first = await drain(jawd) { !trees($0, window).isEmpty }
        let tree = try #require(trees(first, window).first)
        #expect(Set(tree.upsert.map(\.id)) == ["all", "unread", "folder:9", "folder:10", "smart:7"])
        #expect(tree.favourites?.map(\.slot) == [1, 2, 3])
        #expect(requests(fake, "folders.watch").first == .object([:]), "watched with nothing held")

        fake.send(FakeJawd.pushed("folders.push.json"))
        let pushed = await drain(jawd) { !trees($0, window).isEmpty }
        let update = try #require(trees(pushed, window).first)
        #expect(update.upsert.map(\.unread) == [4])
        #expect(update.favourites == [Favourite(slot: 1, folder: "folder:9")], "the slots whole, as one changed")
    }

    @Test func `pushes between two frames merge into the newest row, though its v is the same, and the newest slots`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        _ = await drain(jawd) { !trees($0, window).isEmpty }
        fake.send(FakeJawd.pushed("folders.push.json"))
        fake.send(#"{"method":"folders","params":{"watch":"w5","seq":81242,"upsert":[{"id":"folder:9","v":81241,"parent":null,"name":"Family","position":0,"smart":null,"unread":5,"mentions":1,"count":6,"favourite":1}]}}"#)
        #expect(await eventually { jawd.client.state.withLock { $0.windows[window]?.seq } == 81242 }, "both pushes read")
        let merged = trees(jawd.drain(), window)
        #expect(merged.count == 1, "one update for the frame")
        #expect(merged.first?.upsert.map(\.unread) == [5], "a folder's counts change without its v, so the later row wins")
        #expect(merged.first?.favourites == [Favourite(slot: 1, folder: "folder:9")], "a push without slots leaves the last ones")
    }

    @Test func `the tree is watched again whole after jawd drops, saying the folders it holds and no since`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        _ = await drain(jawd) { !trees($0, window).isEmpty }
        fake.drop()
        #expect(await eventually { requests(fake, "folders.watch").count == 2 })
        let again = try #require(requests(fake, "folders.watch").last)
        #expect(Set(again["have"]?.array?.compactMap(\.string) ?? []) == ["all", "unread", "folder:9", "folder:10", "smart:7"])
        #expect(again["since"] == nil, "jawd compares the tree row by row, whatever since says")
    }

    @Test func `a jawd without folders refuses the tree with its code, and is asked again once it's back`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folders.watch" ? [FakeJawd.refusal(id, code: ErrorCode.unknownMethod, "jawd doesn't know folders.watch")]
                : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        let all = await drain(jawd) { $0.contains { if case .windowFailed = $0 { true } else { false } } }
        let failure = try #require(all.compactMap { if case let .windowFailed(w, f) = $0, w == window { f } else { nil } }.first)
        #expect(failure.code == ErrorCode.unknownMethod)
        fake.drop()
        #expect(await eventually { requests(fake, "folders.watch").count == 2 }, "a jawd back with folders gives the tree")
    }

    @Test func `a folder's list that jawd no longer has, watched again after a drop, fails rather than asking for ever`() async throws {
        let watched = Counted()
        let fake = try FakeJawd { method, params, id in
            guard method == "chats.watch" else { return ordinary(method, params, id) }
            return watched.next() == 1 ? ordinary(method, params, id)
                : [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 9")]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchChats("folder:9", .top(50))
        _ = await drain(jawd) { $0.contains { if case .chats = $0 { true } else { false } } }
        fake.drop()
        let all = await drain(jawd) { $0.contains { if case .windowFailed = $0 { true } else { false } } }
        let failure = try #require(all.compactMap { if case let .windowFailed(w, f) = $0, w == window { f } else { nil } }.first)
        #expect(failure.code == ErrorCode.notFound)
        #expect(requests(fake, "chats.watch").count <= 3, "the first watch, and at most two after the drop")
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "chats.watch").count <= 3, "and none after it failed")
    }

    @Test func `a roster's people come from the smallest sort up, then its pushes, and it grows when moved`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchRoster("folder:9", .top(200))
        var people = People<RosterRow>(folder: "folder:9", place: .top(200))
        for update in rosters(await drain(jawd) { !rosters($0, window).isEmpty }, window) {
            people.apply(update)
        }
        #expect(people.list.map(\.id) == ["person:1", "person:4", "person:5"] && people.total == 6)
        #expect(requests(fake, "roster.watch").first == (try JSONValue.parse(fixture("roster.watch.request.json")))["params"])
        fake.send(FakeJawd.pushed("roster.push.json"))
        for update in rosters(await drain(jawd) { !rosters($0, window).isEmpty }, window) {
            people.apply(update)
        }
        #expect(people.list.map(\.id) == ["person:1", "person:5", "person:4"], "Sofia, online now, before Inês, last seen")
        jawd.move(window, .top(400))
        #expect(await eventually { requests(fake, "watch.move").first?["after"]?.int == 400 })
    }

    @Test func `Move to files the chat in the other folder, and takes it out of this one only once jawd has it there`() async throws {
        let add = HeldBack()
        let fake = try FakeJawd { method, params, id in
            if method == "folder.add" {
                add.id.withLock { $0 = id }
                return []
            }
            return ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        #expect(await eventually { add.id.withLock { $0 } != nil })
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "folder.add") == [.object(["folder": .string("folder:10"), "item": .string("351912345678@s.whatsapp.net")])])
        #expect(requests(fake, "folder.remove").isEmpty, "nothing taken out before jawd has it in the other folder")
        let id = try #require(add.id.withLock { $0 })
        fake.send("{\"id\":\(id),\"result\":{}}")
        let all = await drain(jawd) { notices($0).contains("Ana Ribeiro moved from Family to Cousins") }
        #expect(requests(fake, "folder.remove") == [.object(["folder": .string("folder:9"), "item": .string("351912345678@s.whatsapp.net")])])
        #expect(notices(all) == ["Ana Ribeiro moved from Family to Cousins"], "said once it's out of the old one")
    }

    @Test func `a move whose filing jawd refuses takes nothing out, and says why in jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folder.add" ? [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 10")] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        let all = await drain(jawd) { !notices($0).isEmpty }
        #expect(notices(all) == ["jawd has no folder 10"])
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "folder.remove").isEmpty, "left where it was, never in neither")
    }

    @Test(arguments: ["folder.add", "folder.remove"])
    func `a move cut off by a drop in either half is finished once jawd is back`(half: String) async throws {
        let asked = Counted()
        let fake = try FakeJawd { method, params, id in
            method == half && asked.next() == 1 ? [] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        var all = await drain(jawd) { _ in requests(fake, half).count == 1 }
        #expect(requests(fake, half).count == 1, "drained as the app drains, so the second half goes once the first is answered")
        fake.drop()
        all += await drain(jawd) { notices($0).contains("Ana Ribeiro moved from Family to Cousins") }
        #expect(requests(fake, half).count == 2, "asked again once jawd is back: adding and removing are idempotent")
        #expect(requests(fake, "folder.remove").last == .object(["folder": .string("folder:9"), "item": .string("351912345678@s.whatsapp.net")]))
        #expect(notices(all).filter { $0.hasPrefix("Ana Ribeiro") } == ["Ana Ribeiro moved from Family to Cousins"])
    }

    @Test func `a move whose taking out jawd refuses says the chat is in both folders now, in jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folder.remove" ? [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 9")] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        let all = await drain(jawd) { !notices($0).isEmpty }
        #expect(notices(all) == ["Ana Ribeiro is now in both Cousins and Family: jawd has no folder 9"])
    }

    @Test func `a name comes back made with its row, renamed with none, or turned down in jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            if method == "folder.create", params["name"]?.string == "Family" {
                return [FakeJawd.refusal(id, code: ErrorCode.notAllowed, "a folder there is called Family already")]
            }
            return foldering(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.folderCreate(name: "Cousins", parent: "folder:9")
        jawd.folderCreate(name: "Family", parent: nil)
        jawd.folderRename("folder:10", name: "Primos")
        let all = await drain(jawd) { all in
            all.count { if case .named = $0 { true } else if case .notNamed = $0 { true } else { false } } == 3
        }
        #expect(all.compactMap { if case let .named(row) = $0 { row?.id ?? "renamed" } else { nil } } == ["folder:10", "renamed"])
        #expect(all.compactMap { if case let .notNamed(why) = $0 { why } else { nil } } == ["a folder there is called Family already"])
        #expect(requests(fake, "folder.create").first == (try JSONValue.parse(fixture("folder.create.request.json")))["params"])
    }

    @Test func `a folder made and gone before jawd read it back is named with no row, not turned down`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folder.create" ? ["{\"id\":\(id),\"result\":{\"folder\":null}}"] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.folderCreate(name: "Cousins", parent: "folder:9")
        let all = await drain(jawd) { all in
            all.contains { if case .named = $0 { true } else if case .notNamed = $0 { true } else { false } }
        }
        #expect(all.compactMap { if case let .named(row) = $0 { row?.id ?? "gone" } else { nil } } == ["gone"],
                "another interface deleted it at once: nothing to pick, and the tree's next push says so")
        #expect(all.compactMap { if case let .notNamed(why) = $0 { why } else { nil } }.isEmpty && notices(all).isEmpty,
                "the request succeeded, so nothing says it failed")
    }

    @Test func `filing and the slots say what was done once jawd has done it, and an emptied slot is sent as null`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.folderAdd("folder:9", item: "person:4", said: "Inês Gomes filed in Family")
        jawd.favouriteSet(slot: 3, target: nil, said: "⌘3 is free")
        let all = await drain(jawd) { notices($0).count == 2 }
        #expect(notices(all) == ["Inês Gomes filed in Family", "⌘3 is free"])
        #expect(requests(fake, "favourite.set").first == .object(["slot": .number(3), "target": .null]))
    }
}
