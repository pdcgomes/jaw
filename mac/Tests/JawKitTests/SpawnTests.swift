import Darwin
import Foundation
@testable import JawKit
import Testing

/// The exits the main queue was told of, by PID.
@MainActor
final class Exits {
    var seen: [pid_t: Int32] = [:]
}

/// A folder of the test's own, which it removes.
func spawnFolder() -> String {
    "/tmp/jaws-\(UInt32.random(in: 0 ... .max))"
}

@MainActor @Test func `a spawn in a group of its own leads it, appends to its log in a folder made for it, and its exit is told`() async throws {
    let folder = spawnFolder()
    defer { try? FileManager.default.removeItem(atPath: folder) }
    let log = folder + "/home/jawd.log"
    let exits = Exits()
    func sh(_ script: String, ownGroup: Bool) -> Spawn {
        Spawn(program: "/bin/sh", arguments: ["-c", script], environment: ["PATH": "/usr/bin:/bin"], log: log, ownGroup: ownGroup)
    }
    let first = try sh("echo before >&2; exit 3", ownGroup: true).run { pid, status in exits.seen[pid] = status }
    #expect(await eventually { exits.seen[first] != nil })
    #expect(exits.seen[first].map(Spawn.said) == "exit status: 3")
    let mode = try FileManager.default.attributesOfItem(atPath: folder + "/home")[.posixPermissions] as? Int
    #expect(mode == 0o700, "the log's folder is made, readable only by you")
    let leader = try sh("echo after >&2; exec sleep 30", ownGroup: true).run { pid, status in exits.seen[pid] = status }
    #expect(await eventually { (try? String(contentsOfFile: log, encoding: .utf8)) == "before\nafter\n" }, "each start appends to the log")
    #expect(getpgid(leader) == leader && getpgid(leader) != getpgrp(), "it leads a process group of its own")
    kill(leader, SIGTERM)
    #expect(await eventually { exits.seen[leader] != nil })
    #expect(exits.seen[leader].map(Spawn.said) == "signal: 15 (SIGTERM)")
    let child = try sh("exec sleep 30", ownGroup: false).run { pid, status in exits.seen[pid] = status }
    #expect(getpgid(child) == getpgrp(), "without one, it's in the app's group")
    kill(child, SIGKILL)
    #expect(await eventually { exits.seen[child] != nil })
    #expect(exits.seen[child].map(Spawn.said) == "signal: 9 (SIGKILL)")
}

@MainActor @Test func `a spawn has none of the app's descriptors but the three it's given`() async throws {
    let folder = spawnFolder()
    defer { try? FileManager.default.removeItem(atPath: folder) }
    let log = folder + "/jawd.log"
    let held = Darwin.open("/dev/null", O_RDONLY)
    let high = fcntl(held, F_DUPFD, 40)
    defer {
        close(high)
        close(held)
    }
    #expect(high >= 40, "a descriptor of the app's numbered above the three, as another test's socket can be")
    let exits = Exits()
    let pid = try Spawn(program: "/bin/sh", arguments: ["-c", "ls /dev/fd >&2"], environment: ["PATH": "/usr/bin:/bin"], log: log,
                        ownGroup: true).run { pid, status in exits.seen[pid] = status }
    #expect(await eventually { exits.seen[pid] != nil })
    #expect(exits.seen[pid] == 0)
    let listed = try String(contentsOfFile: log, encoding: .utf8).split(separator: "\n").map(String.init)
    #expect(listed.contains("2") && !listed.contains(String(high)), "the descriptors it had: \(listed)")
}

@MainActor @Test func `a spawn's signals are at their defaults and unblocked, whatever the app ignores or blocks`() async throws {
    let folder = spawnFolder()
    defer { try? FileManager.default.removeItem(atPath: folder) }
    var blocked = sigset_t()
    sigemptyset(&blocked)
    sigaddset(&blocked, SIGUSR2)
    signal(SIGUSR1, SIG_IGN)
    pthread_sigmask(SIG_BLOCK, &blocked, nil)
    defer {
        pthread_sigmask(SIG_UNBLOCK, &blocked, nil)
        signal(SIGUSR1, SIG_DFL)
    }
    let exits = Exits()
    func sleeper() throws -> pid_t {
        try Spawn(program: "/bin/sleep", arguments: ["30"], environment: [:], log: folder + "/jawd.log", ownGroup: true)
            .run { pid, status in exits.seen[pid] = status }
    }
    let ignored = try sleeper()
    let masked = try sleeper()
    defer {
        for pid in [ignored, masked] where exits.seen[pid] == nil {
            kill(pid, SIGKILL)
        }
    }
    kill(ignored, SIGUSR1)
    kill(masked, SIGUSR2)
    #expect(await eventually { exits.seen[ignored] != nil && exits.seen[masked] != nil })
    #expect(exits.seen[ignored].map(Spawn.said) == "signal: 30 (SIGUSR1)", "a signal the app ignores is at its default")
    #expect(exits.seen[masked].map(Spawn.said) == "signal: 31 (SIGUSR2)", "a signal the app blocks is unblocked")
}

@Test func `a log's last words are its last line that says something, without jawd's name, from its last 4 KB, and a wait status is said in the terminal's words`() throws {
    let folder = spawnFolder()
    try FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: folder) }
    let log = folder + "/jawd.log"
    #expect(Spawn.lastWords(log) == "", "no log, no words")
    try "jawd: stopped in 40 ms\njawd: another jawd is running on /x\n\n  \n".write(toFile: log, atomically: true, encoding: .utf8)
    #expect(Spawn.lastWords(log) == "another jawd is running on /x")
    try (String(repeating: "x", count: 10000) + "\nthe engine said nothing\n").write(toFile: log, atomically: true, encoding: .utf8)
    #expect(Spawn.lastWords(log) == "the engine said nothing")
    try ("x\n" + String(repeating: "y", count: 5000) + "\n").write(toFile: log, atomically: true, encoding: .utf8)
    #expect(Spawn.lastWords(log) == String(repeating: "y", count: 4095), "only the last 4 KB are read, however long the log")
    #expect(Spawn.said(1 << 8) == "exit status: 1" && Spawn.said(9) == "signal: 9 (SIGKILL)")
    #expect(Spawn.said(6 | 0x80) == "signal: 6 (SIGABRT) (core dumped)")
}
