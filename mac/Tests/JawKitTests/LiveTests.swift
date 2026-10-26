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
}
