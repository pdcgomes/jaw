import Darwin
import Foundation
@testable import JawKit
import Synchronization

/// A jawd for tests: it listens on a Unix socket of its own, hands each request to the test's
/// script, and sends what the script answers. A test pushes, drops the connection, or stops
/// answering, to see what the client does.
final class FakeJawd: Sendable {
    typealias Script = @Sendable (_ method: String, _ params: JSONValue, _ id: UInt64) -> [String]

    let folder: String
    let socket: String
    let listener: Int32
    let script: Script
    let state = Mutex(Inside())

    struct Inside {
        var connection: Int32 = -1
        var connections = 0
        var requests: [(method: String, params: JSONValue)] = []
        var stopped = false
    }

    init(script: @escaping Script) throws {
        folder = "/tmp/jawk-\(UInt32.random(in: 0 ... .max))"
        try FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
        socket = folder + "/jaw.sock"
        self.script = script
        listener = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(socket.utf8)
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(listener, $0, size) }
        }
        guard bound == 0, listen(listener, 8) == 0 else {
            throw POSIXError(.EADDRINUSE)
        }
        let thread = Thread { [self] in accepting() }
        thread.start()
    }

    /// The answer `hello` gets.
    static func welcome(_ id: UInt64, seq: Int = 100) -> String {
        """
        {"id":\(id),"result":{"protocol":2,"jawd":"0.1.0","root":"/tmp","seq":\(seq),"account":{"state":"linked",\
        "me":{"id":"person:1","name":"Pedro","phone":"+351 912 000 000"}},"connection":{"state":"online"}}}
        """
    }

    static func chat(_ id: String, v: Int, sort: String, name: String) -> String {
        """
        {"id":"\(id)","v":\(v),"sort":"\(sort)","kind":"direct","name":"\(name)","avatar":null,"last":null,\
        "unread":0,"mentions":0,"pinned":false,"muted_until":null,"archived":false,"marked_unread":false,\
        "draft":null,"typing":[],"community":null}
        """
    }

    static func message(_ id: String, v: Int, sort: String, text: String) -> String {
        """
        {"id":"\(id)","v":\(v),"sort":"\(sort)","chat":"ana","at":1,"mine":false,"kind":"text",\
        "spans":[{"t":"text","s":"\(text)"}],"reply":null,"forwarded":0,"reactions":[],"status":null,\
        "edited":false,"deleted":false,"expires_at":null,"view_once":false,"starred":false}
        """
    }

    func accepting() {
        while true {
            let fd = accept(listener, nil, nil)
            if fd < 0 { return }
            var on: Int32 = 1
            setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
            state.withLock { s in
                s.connection = fd
                s.connections += 1
            }
            serving(fd)
        }
    }

    func serving(_ fd: Int32) {
        var pending = Data()
        var buffer = [UInt8](repeating: 0, count: 65536)
        while true {
            let n = buffer.withUnsafeMutableBytes { Darwin.read(fd, $0.baseAddress!, 65536) }
            if n <= 0 { break }
            pending.append(contentsOf: buffer[0 ..< n])
            while let newline = pending.firstIndex(of: 0x0A) {
                let line = pending[pending.startIndex ..< newline]
                pending.removeSubrange(pending.startIndex ... newline)
                guard let value = try? JSONValue.parse(Data(line)), let method = value["method"]?.string,
                      let id = value["id"]?.int
                else { continue }
                let params = value["params"] ?? .null
                let stopped = state.withLock { s in
                    s.requests.append((method, params))
                    return s.stopped
                }
                if stopped { continue }
                for answer in script(method, params, UInt64(id)) {
                    send(answer, on: fd)
                }
            }
        }
        state.withLock { s in
            if s.connection == fd { s.connection = -1 }
        }
        Darwin.close(fd)
    }

    func send(_ line: String, on fd: Int32? = nil) {
        let fd = fd ?? state.withLock { $0.connection }
        guard fd >= 0 else { return }
        _ = Client.writeAll(fd, Data((line + "\n").utf8))
    }

    /// Drops the current connection, as a jawd that stops does.
    func drop() {
        let fd = state.withLock { $0.connection }
        if fd >= 0 { shutdown(fd, SHUT_RDWR) }
    }

    /// Takes requests from here on without answering them.
    func stopAnswering() {
        state.withLock { $0.stopped = true }
    }

    var requests: [(method: String, params: JSONValue)] {
        state.withLock { $0.requests }
    }

    var connections: Int {
        state.withLock { $0.connections }
    }

    func stop() {
        Darwin.close(listener)
        drop()
        try? FileManager.default.removeItem(atPath: folder)
    }
}

/// Drains a client until `done` says what it has is enough, or a few seconds pass.
@MainActor
func drain(_ jawd: Jawd, seconds: Double = 5, until done: ([JawUpdate]) -> Bool) async -> [JawUpdate] {
    var all: [JawUpdate] = []
    let until = Date().addingTimeInterval(seconds)
    while Date() < until {
        all.append(contentsOf: jawd.drain())
        if done(all) { return all }
        try? await Task.sleep(for: .milliseconds(5))
    }
    return all
}

func requests(_ fake: FakeJawd, _ method: String) -> [JSONValue] {
    fake.requests.filter { $0.method == method }.map(\.params)
}

/// Waits until `condition` holds or a few seconds pass; says whether it held.
@MainActor
func eventually(seconds: Double = 5, _ condition: () -> Bool) async -> Bool {
    let until = Date().addingTimeInterval(seconds)
    while Date() < until {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(5))
    }
    return condition()
}
