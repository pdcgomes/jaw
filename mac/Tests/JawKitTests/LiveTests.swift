import Darwin
import Foundation
@testable import JawKit
import Testing

/// JawKit against a real jawd: `jawd --engine demo` on a made-up account in a temporary folder,
/// never the owner's. It watches, sends with a key, and is killed mid-stream and served again from
/// the same jaw.db, as a jawd that crashed and started again would; the windows must end as a fresh
/// watch leaves them, as `crates/jaw-client/tests/jawd.rs` checks the Rust client.

let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
let builtJawd = ["debug", "release"].map { repo.appending(path: "target/\($0)/jawd").path }
    .first { FileManager.default.isExecutableFile(atPath: $0) }

/// A jawd for a test, on a folder of its own, killed when the test ends.
final class DemoJawd {
    let home: String
    var process: Process?

    init() throws {
        home = "/tmp/jawl-\(UInt32.random(in: 0 ... .max))"
        try FileManager.default.createDirectory(atPath: home, withIntermediateDirectories: true)
        try start()
    }

    var socket: String {
        home + "/jaw.sock"
    }

    func start() throws {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: builtJawd!)
        p.arguments = ["--engine", "demo", "--no-notify"]
        p.environment = ProcessInfo.processInfo.environment.merging(["JAW_HOME": home]) { $1 }
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        try p.run()
        process = p
    }

    func kill() {
        guard let p = process, p.isRunning else { return }
        Darwin.kill(p.processIdentifier, SIGKILL)
        p.waitUntilExit()
        try? FileManager.default.removeItem(atPath: socket)
    }

    func stop() {
        if let p = process, p.isRunning {
            p.terminate()
            p.waitUntilExit()
        }
        try? FileManager.default.removeItem(atPath: home)
    }
}

/// A client's windows, kept as an interface keeps them.
@MainActor
final class Held {
    let jawd: Jawd
    var chats = Chats<ChatRow>(folder: "all", place: .top(60))
    var messages: Messages<MessageRow>?
    var list: WindowID
    var open: WindowID?
    var updates: [JawUpdate] = []

    init(_ socket: String) {
        jawd = Jawd(socket: socket, client: "jaw-mac-live", version: "0", focus: false) { _ in }
        jawd.start()
        list = jawd.watchChats("all", .top(60))
    }

    func watch(_ chat: String) {
        messages = Messages(chat: chat, place: .newest(80))
        open = jawd.watchMessages(chat, .newest(80))
    }

    /// Drains until `done` holds or `seconds` pass; says whether it held.
    func until(seconds: Double = 20, _ done: (Held) -> Bool) async -> Bool {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end {
            for u in jawd.drain() {
                updates.append(u)
                switch u {
                case let .chats(w, update) where w == list: chats.apply(update)
                case let .messages(w, update) where w == open: messages?.apply(update)
                default: break
                }
            }
            if done(self) { return true }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return done(self)
    }
}

/// Keeps `params` as the terminal's panes, as jaw does: a connection that says hello as jaw, then
/// layout.set. Says whether jawd kept them.
func keepTerminalPanes(_ socket: String, _ params: JSONValue) async -> Bool {
    let terminal = Client(config: .init(socket: socket, client: "jaw", version: "0"), wake: { _ in })
    terminal.start()
    defer { terminal.close() }
    var asked: UInt64?
    let end = Date().addingTimeInterval(10)
    while Date() < end {
        for event in terminal.drain().events {
            switch event {
            case .connected where asked == nil:
                if case let .success(id) = terminal.request("layout.set", params) { asked = id }
            case let .reply(id, .success(.json(answer))) where id == asked:
                return answer["kept"]?.bool == true
            default:
                break
            }
        }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return false
}

@MainActor
@Suite(.serialized, .enabled(if: builtJawd != nil, "cargo build -p jawd builds the jawd this runs"))
struct LiveTests {
    @Test func `windows on a real jawd end as a fresh watch leaves them, after jawd is killed mid-stream`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let held = Held(demo.socket)
        defer { held.jawd.close() }
        #expect(await held.until { !$0.chats.isEmpty && $0.chats.list.contains { $0.last != nil } }, "the demo's chats come")
        let chat = try #require(held.chats.list.first { $0.last != nil })
        held.watch(chat.id)
        #expect(await held.until { $0.messages?.isEmpty == false }, "and a chat's messages")

        held.jawd.sendText(chat: chat.id, text: "on my way", key: "jaw-live-0001")
        #expect(await held.until { h in h.updates.contains { if case let .sent(key, _, _) = $0 { key == "jaw-live-0001" } else { false } } },
                "sent, under its key")
        #expect(await held.until { $0.messages?.list.contains { $0.mine && $0.plain == "on my way" } == true },
                "and its row in the window")

        demo.kill()
        #expect(await held.until(seconds: 5) { h in h.updates.contains { if case .unreachable = $0 { true } else { false } } },
                "the drop is said")
        try demo.start()
        let hellos = { (h: Held) in h.updates.count { if case .hello = $0 { true } else { false } } }
        #expect(await held.until { hellos($0) >= 2 }, "hello again once jawd is back")

        let fresh = Held(demo.socket)
        defer { fresh.jawd.close() }
        fresh.watch(chat.id)
        #expect(await fresh.until { $0.messages?.isEmpty == false && !$0.chats.isEmpty })
        // Both settle on what jawd holds now.
        _ = await held.until(seconds: 2) { _ in false }
        _ = await fresh.until(seconds: 1) { _ in false }
        let rows = { (h: Held) in h.messages?.list.map { "\($0.id)@\($0.v)" } ?? [] }
        #expect(rows(held) == rows(fresh), "the conversation as a fresh watch leaves it")
        #expect(held.chats.list.map(\.id) == fresh.chats.list.map(\.id), "and the chat list")
        #expect(held.jawd.client.stats.connects == 2)
    }

    @Test func `the demo's photo comes here with its previews, and is fetched, saved and cancelled as jawd answers`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let held = Held(demo.socket)
        defer { held.jawd.close() }
        let ana = "351912345678@s.whatsapp.net", lake = "3EB0DEMO00000008"
        #expect(await held.until { $0.chats.list.contains { $0.id == ana } }, "Ana's chat comes")
        held.watch(ana)
        let photo = { (h: Held) in h.messages?.get(lake)?.media }
        #expect(await held.until(seconds: 30) { h in
            photo(h).map { $0.isHere && $0.preview(256) != nil && $0.preview(768) != nil && $0.thumbnail != nil } ?? false
        }, "the lake, here with both previews and its thumbnail: \(String(describing: photo(held)))")
        let media = try #require(photo(held))
        #expect(media.w == 1600 && media.h == 1200 && media.state == .local)
        for path in [media.path, media.preview(256), media.preview(768), media.thumbnail].compactMap(\.self) {
            #expect(FileManager.default.fileExists(atPath: demo.home + "/" + path), "\(path) is under jawd's root")
        }

        held.jawd.fetch(chat: ana, message: lake)
        let folder = demo.home + "-saved"
        try FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: folder) }
        let ticket = held.jawd.save(chat: ana, message: lake, to: folder)
        let saved = { (h: Held) in
            h.updates.lazy.compactMap { u -> (String?, String?)? in
                if case let .saved(t, _, path, why) = u, t == ticket { (path, why) } else { nil }
            }.first
        }
        #expect(await held.until { saved($0) != nil }, "the save is answered")
        let path = try #require(saved(held)?.0, "saved: \(String(describing: saved(held)))")
        #expect(path == folder + "/" + lake + ".jpg", "named by its ID, as a photo has no name")
        #expect(try Data(contentsOf: URL(fileURLWithPath: path)) == Data(contentsOf: URL(fileURLWithPath: demo.home + "/" + media.path!)),
                "the copy is the cache's file, whole")
        held.jawd.cancel(chat: ana, message: lake)
        _ = await held.until(seconds: 1) { _ in false }
        #expect(!held.updates.contains { if case .notFetched = $0 { true } else { false } }, "the fetch was taken")
        #expect(!held.updates.contains { if case let .notice(text, _) = $0 { text.contains("download") } else { false } },
                "and the cancel")
    }

    @Test func `the made-up account's library: each kind's newest, a photo window's previews under jawd's root, and a page by keyset`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let jawd = Jawd(socket: demo.socket, client: "jaw-mac-live", version: "0", focus: false) { _ in }
        jawd.start()
        defer { jawd.close() }
        let every = jawd.watchLibrary(LibraryFilter(), .top(24))
        let photo = jawd.watchLibrary(LibraryFilter(kind: .photo), .top(240))
        var all = Library<LibraryRow>(place: .top(24)), photos = Library<LibraryRow>(place: .top(240))
        var moves = 0
        func until(seconds: Double = 30, _ done: () -> Bool) async -> Bool {
            let end = Date().addingTimeInterval(seconds)
            while Date() < end {
                for case let .library(w, update) in jawd.drain() {
                    if w == every { all.apply(update) }
                    if w == photo {
                        photos.apply(update)
                        if update.moved?.place.around != .top, update.moved != nil { moves += 1 }
                    }
                }
                if done() { return true }
                try? await Task.sleep(for: .milliseconds(10))
            }
            return done()
        }
        let kinds: Set<LibraryKind> = [.photo, .voice, .file, .link]
        #expect(await until { kinds.isSubset(of: Set(all.list.map(\.kind))) && kinds.allSatisfy { (all.counts?[$0] ?? 0) > 0 } },
                "a photo, a voice message, a file and a link: \(all.list.map(\.kind)), \(String(describing: all.counts))")
        #expect(!all.older, "every kind's window isn't paged")
        #expect(all.list.first { $0.kind == .file }?.file?.name == "sintra-saturday.txt")
        #expect(all.list.first { $0.kind == .file }?.file?.type == "TXT")
        #expect(all.list.first { $0.kind == .link }?.link?.host == "www.ipma.pt")

        let lake = "3EB0DEMO00000008"
        #expect(await until { photos.get(lake)?.media.map { $0.isHere && $0.preview(256) != nil && $0.preview(768) != nil } ?? false },
                "the lake comes here with its previews: \(String(describing: photos.get(lake)?.media))")
        let media = try #require(photos.get(lake)?.media)
        for path in [media.path, media.preview(256), media.preview(768), media.thumbnail].compactMap(\.self) {
            #expect(FileManager.default.fileExists(atPath: demo.home + "/" + path), "\(path) is under jawd's root")
        }
        jawd.move(photo, .around(lake, before: 0, after: 120))
        #expect(await until { moves == 1 }, "the page around the lake lands")
        #expect(photos.place == .around(lake, before: 0, after: 120) && photos.list.map(\.id) == [lake] && !photos.older,
                "the lake and what's older, none twice: \(photos.list.map(\.id))")

        guard case let .success(asked) = jawd.client.request("library.shown", LibraryShown(items: [lake, "3EB0NOTANITEM000"])) else {
            Issue.record("library.shown wasn't sent")
            return
        }
        var answer: Result<Reply, Failure>?
        let end = Date().addingTimeInterval(10)
        while answer == nil, Date() < end {
            for case let .reply(id, outcome) in jawd.client.drain().events where id == asked {
                answer = outcome
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        if case .success? = answer {} else { Issue.record("library.shown, an id that names no item passed over: \(String(describing: answer))") }
    }

    @Test func `find answers from a real jawd's index`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let held = Held(demo.socket)
        defer { held.jawd.close() }
        // A chat is named by its number until its contact's name comes.
        let named = { (h: Held) in h.chats.list.first { $0.name.first?.isLetter == true } }
        #expect(await held.until { named($0) != nil }, "chats: \(held.chats.list.map(\.name))")
        let name = try #require(named(held)?.name)
        held.jawd.find(id: 7, query: String(name.prefix(3)))
        let found = await held.until { h in
            h.updates.contains { if case let .found(7, found) = $0 { !found.results.isEmpty } else { false } }
        }
        let answers = held.updates.compactMap { u -> String? in
            switch u {
            case let .found(id, f): "found \(id): \(f.results.count)"
            case let .findFailed(id, _, why): "failed \(id): \(why)"
            default: nil
            }
        }
        #expect(found, "find for \(name.prefix(3)): \(answers)")
    }

    @Test func `the terminal's panes come from a real jawd as it keeps them, and a chat is looked up by its JID`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let held = Held(demo.socket)
        defer { held.jawd.close() }
        let ana = "351912345678@s.whatsapp.net", climbing = "120363012345678901@g.us", unknown = "351919999999@s.whatsapp.net"
        #expect(await held.until { h in [ana, climbing].allSatisfy { id in h.chats.list.contains { $0.id == id } } }, "the demo's chats come")
        let params = try JSONValue.parse(Data("""
        {"v":1,"layout":{"format":1,"focused":2,"tree":{"split":{"how":"across","first":600,"a":{"chat":"\(ana)"},\
        "b":{"split":{"how":"down","first":500,"a":{"chat":"\(climbing)"},"b":{"chat":"\(unknown)"}}}}}}}
        """.utf8))
        #expect(await keepTerminalPanes(demo.socket, params), "the terminal keeps its panes, as jaw does")

        held.jawd.layout(of: "jaw", what: "opening the terminal's panes")
        #expect(await held.until { h in h.updates.contains { if case .layout = $0 { true } else { false } } })
        let kept = held.updates.compactMap { if case let .layout(k) = $0 { k } else { nil } }.first
        #expect(kept?.interface == "jaw" && kept?.v == 1)
        #expect(kept?.layout == SplitLayout(focused: 1, tree: .split(.across, first: 600, .chat(ana), .chat(climbing))),
                "the pane of a chat jawd has none of is left out, its split closed, and the keys go to the pane before it")
        #expect(kept?.dropped == [unknown])

        held.jawd.lookUp([climbing, unknown])
        #expect(await held.until { h in h.updates.count { if case .looked = $0 { true } else { false } } == 2 })
        let found = held.updates.compactMap { if case let .looked(chat, f) = $0 { (chat, f) } else { nil } }
        if case let .row(row)? = found.first?.1 { #expect(row.id == climbing) } else { Issue.record("the group wasn't found: \(found)") }
        #expect(found.last?.0 == unknown && found.last?.1 == .gone, "jawd has no chat by that JID")
    }

    @Test func `the made-up account's people type, and a real jawd says who in its answers and pushes`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        // In front, so jawd tells the made-up WhatsApp you're online, which is when typing is heard.
        let jawd = Jawd(socket: demo.socket, client: "jaw-mac-live", version: "0", focus: true) { _ in }
        jawd.start()
        defer { jawd.close() }
        _ = jawd.watchChats("all", .top(60))
        let ana = "351912345678@s.whatsapp.net", climbing = "120363012345678901@g.us"
        var said: [Typists] = []
        let end = Date().addingTimeInterval(30)
        func heard(_ chat: String, _ state: Typist.State) -> Bool {
            said.contains { $0[chat]?.contains { $0.state == state && $0.id.hasPrefix("person:") && !$0.name.isEmpty } == true }
        }
        while Date() < end, !(heard(ana, .typing) && heard(climbing, .recording)) {
            for case let .typing(t) in jawd.drain() {
                said.append(t)
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(heard(ana, .typing), "Ana typing in her chat: \(said.suffix(3))")
        #expect(heard(climbing, .recording), "someone recording in Lisbon Climbing: \(said.suffix(3))")
    }
}
