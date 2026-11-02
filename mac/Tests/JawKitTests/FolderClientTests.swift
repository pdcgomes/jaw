import Foundation
@testable import JawKit
import Synchronization
import Testing

/// The client watches jawd's folder tree and a roster, and asks for what the folder items do: the
/// tree whole with its slots, its pushes merged, watched again whole with the folders held, and never
/// moved; a roster like a chat list; a jawd without folders refusing the tree; a deleted folder's list
/// failing once it's watched again, after the top it falls back to if it was around a chat; Move to's
/// two requests in their order, finished after a drop, and what's said when it ends in both folders
/// or its old one went; a name's answer, with the folder made or without one gone at once, and its
/// refusal; and, once jawd has done each, what's said and what a filing did to the item's folders.

/// A script that answers as jawd's folders do: the tree, a roster and a made folder from the protocol's
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

func filings(_ all: [JawUpdate]) -> [Filing] {
    all.compactMap { if case let .filed(filing) = $0 { filing } else { nil } }
}

/// Waits until jawd has everything asked before now: requests go out in order on the one socket, so
/// once a marker asked after them arrives, anything they'd have sent with them has arrived too.
@MainActor
func settled(_ jawd: Jawd, _ fake: FakeJawd) async -> Bool {
    let before = requests(fake, "favourite.set").count
    jawd.favouriteSet(slot: 9, target: nil, said: nil)
    return await eventually { requests(fake, "favourite.set").count == before + 1 }
}

/// A jawd that has deleted folder 9 once its list has been watched: every chat list watched after
/// the first is turned down as gone.
func forgetting() throws -> FakeJawd {
    let watched = Counted()
    return try FakeJawd { method, params, id in
        guard method == "chats.watch" else { return ordinary(method, params, id) }
        return watched.next() == 1 ? ordinary(method, params, id)
            : [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 9")]
    }
}

/// The halves of Move to that `Jawd` keeps to ask again once jawd is back.
@MainActor
func moves(_ jawd: Jawd) -> [Jawd.Moving] {
    jawd.again.compactMap { if case let .move(moving) = $0 { moving } else { nil } }
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

        fake.send(try FakeJawd.pushed("folders.push.json"))
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
        fake.send(try FakeJawd.pushed("folders.push.json"))
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

    @Test func `moving the tree asks nothing of jawd, as it's watched whole`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        _ = await drain(jawd) { !trees($0, window).isEmpty }
        jawd.move(window, .top(50))
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "folders.watch").count == 1 && requests(fake, "watch.move").isEmpty,
                "a second watch would leave the first open, its pushes coming twice")
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
        let fake = try forgetting()
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchChats("folder:9", .top(50))
        _ = await drain(jawd) { $0.contains { if case .chats = $0 { true } else { false } } }
        fake.drop()
        let all = await drain(jawd) { $0.contains { if case .windowFailed = $0 { true } else { false } } }
        let failedAt = requests(fake, "chats.watch").count
        let failure = try #require(all.compactMap { if case let .windowFailed(w, f) = $0, w == window { f } else { nil } }.first)
        #expect(failure.code == ErrorCode.notFound)
        #expect(failedAt == 2, "the first watch, and one after the drop, at the top it would fall back to")
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "chats.watch").count == failedAt, "and none after it failed")
    }

    @Test func `a list around a chat whose folder went is asked again once at the top, then fails`() async throws {
        let fake = try forgetting()
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchChats("folder:9", .around("ana", before: 25, after: 25))
        _ = await drain(jawd) { $0.contains { if case .chats = $0 { true } else { false } } }
        fake.drop()
        let all = await drain(jawd) { $0.contains { if case .windowFailed = $0 { true } else { false } } }
        let failedAt = requests(fake, "chats.watch").count
        let failure = try #require(all.compactMap { if case let .windowFailed(w, f) = $0, w == window { f } else { nil } }.first)
        #expect(failure.code == ErrorCode.notFound)
        #expect(requests(fake, "chats.watch").dropFirst().map { $0["around"]?.string } == ["ana", "top"],
                "around the chat it held, then once at the top it falls back to")
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "chats.watch").count == failedAt, "and none after it failed")
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
        fake.send(try FakeJawd.pushed("roster.push.json"))
        for update in rosters(await drain(jawd) { !rosters($0, window).isEmpty }, window) {
            people.apply(update)
        }
        #expect(people.list.map(\.id) == ["person:1", "person:5", "person:4"], "Sofia, online now, before Inês, last seen")
        jawd.move(window, .top(400))
        #expect(await eventually { requests(fake, "watch.move").first?["after"]?.int == 400 })
    }

    @Test func `the tree and a roster show none of the chat list's chats, so typing where no list or pane shows isn't kept, and goes as its chat leaves the list`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "chats.watch" else { return foldering(method, params, id) }
            return [chatList(id, ["ana", "rui"], typing: #"{"ana":[\#(typist("person:2", "Ana"))]}"#)]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let tree = jawd.watchFolders()
        let roster = jawd.watchRoster("folder:9", .top(200))
        _ = jawd.watchChats("all", .top(60))
        let first = await drain(jawd) { all in !trees(all, tree).isEmpty && !rosters(all, roster).isEmpty && typing(all)?["ana"] != nil }
        #expect(typing(first)?["ana"] == [anaTyping], "the chat list's answer says Ana's typing in her chat")

        // Someone types in a chat no list or pane shows, nor anyone's in the roster, then Rui records in one the list shows.
        fake.send(typingPush("elsewhere", typist("person:9", "Xavier")))
        fake.send(typingPush("rui", typist("person:3", "Rui", "recording")))
        let pushed = await drain(jawd) { typing($0)?["rui"] != nil }
        #expect(typing(pushed)?["rui"] == [ruiRecording])
        #expect(typing(pushed)?["elsewhere"] == nil, "neither the tree nor the roster shows a chat the list doesn't: \(String(describing: typing(pushed)))")

        fake.send(#"{"method":"chats","params":{"watch":"w1","seq":11,"remove":["ana"]}}"#)
        let gone = await drain(jawd) { typing($0)?["ana"] == nil && typing($0)?["rui"] != nil }
        #expect(typing(gone)?["ana"] == nil && typing(gone)?["rui"] == [ruiRecording],
                "Ana's chat left the list, and her typing with it, though the tree and the roster are still watched")
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
        #expect(filings(all) == [.moved(item: "351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10")],
                "for a row held for the chat, which jawd's push may not reach")
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
        #expect(filings(all).isEmpty, "nothing filed")
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
        #expect(filings(all) == [.moved(item: "351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10")])
    }

    @Test func `a half lost to a drop is kept for jawd's return if jawd goes again before it's sent, as a second half is`() throws {
        let jawd = Jawd(socket: "/tmp/jawk-never/jaw.sock", client: "jaw-mac-test", version: "0", focus: false) { _ in }
        let move = Jawd.Moving(item: "351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                               said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        #expect(jawd.refused(.moving(move), .unreachable).count == 1 && jawd.again.isEmpty,
                "Move to chosen while jawd is away: said, and left to be chosen again")
        #expect(jawd.refused(.moving(move), .lost).isEmpty)
        let lost = try #require(moves(jawd).first)
        jawd.again.removeAll()
        #expect(jawd.refused(.moving(lost), .unreachable).isEmpty, "asked again once jawd was back, and jawd gone again first")
        var second = move
        second.removing = true
        #expect(jawd.refused(.moving(second), .unreachable).isEmpty, "asked as the first half's answer was read, jawd gone meanwhile")
        #expect(moves(jawd).map(\.removing) == [false, true], "both kept, to be asked once jawd is back")
    }

    @Test func `a move whose taking out jawd refuses says the chat is in both folders now, in jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folder.remove" ? [FakeJawd.refusal(id, code: ErrorCode.internal, #"couldn't write "jaw.db": the disk is full"#)]
                : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        let all = await drain(jawd) { !notices($0).isEmpty }
        #expect(notices(all) == [#"Ana Ribeiro is now in both Cousins and Family: couldn't write "jaw.db": the disk is full"#])
        #expect(filings(all) == [.added(item: "351912345678@s.whatsapp.net", to: "folder:10")], "filed in Cousins, and still in Family")
    }

    @Test func `a move whose old folder went between the halves says it moved, as the chat is in the new one alone`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folder.remove" ? [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 9")] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351913550005@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: nil, both: "Sofia is now in both Cousins and Family")
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        let all = await drain(jawd) { notices($0).contains { $0.hasPrefix("Ana Ribeiro") } }
        #expect(notices(all) == ["Ana Ribeiro moved from Family to Cousins"],
                "taking out what isn't there is done already, so error 3 says Family went: the move's own words, or none")
        #expect(filings(all) == [.moved(item: "351913550005@s.whatsapp.net", from: "folder:9", to: "folder:10"),
                                 .moved(item: "351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10")])
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
        jawd.folderRemove("folder:10", item: "person:4", said: "Inês Gomes taken out of Cousins")
        let all = await drain(jawd) { notices($0).count == 3 }
        #expect(notices(all) == ["Inês Gomes filed in Family", "⌘3 is free", "Inês Gomes taken out of Cousins"])
        #expect(filings(all) == [.added(item: "person:4", to: "folder:9"), .removed(item: "person:4", from: "folder:10")],
                "what each filing did to the item's folders; a slot files nothing")
        #expect(requests(fake, "favourite.set").first == .object(["slot": .number(3), "target": .null]))
    }

    @Test func `a filing says what the item's folders are once jawd has done it`() {
        let ana = "351912345678@s.whatsapp.net"
        #expect(Filing.added(item: ana, to: "folder:10").applied(to: ["folder:9"]) == ["folder:9", "folder:10"])
        #expect(Filing.added(item: ana, to: "folder:9").applied(to: ["folder:9"]) == ["folder:9"], "filed where it was already")
        #expect(Filing.removed(item: ana, from: "folder:9").applied(to: ["folder:9", "folder:4"]) == ["folder:4"])
        #expect(Filing.removed(item: ana, from: "folder:9").applied(to: ["folder:4"]) == ["folder:4"], "taken out where it wasn't")
        #expect(Filing.moved(item: ana, from: "folder:9", to: "folder:10").applied(to: ["folder:9", "folder:4"]) == ["folder:4", "folder:10"])
        #expect([Filing.added(item: ana, to: "x"), .removed(item: "person:4", from: "x"), .moved(item: ana, from: "x", to: "y")].map(\.item)
            == [ana, "person:4", ana])
    }
}
