import AppKit
import Foundation
import ServiceManagement

/// Who runs jawd. A release build registers the jawd inside it as the one LaunchAgent, under the
/// label `jaw agent install` uses, so there can't be two agents, and jawd's `jaw.lock` refuses a
/// second jawd whoever starts it. A debug build, and `--demo`, run a jawd of their own on a folder
/// of their own instead, and never touch the agent.
@MainActor
final class Agent {
    static let label = "dev.badcoffee.jaw.jawd"
    static let plist = "\(label).plist"

    let launch: Launch
    /// The jawd this app started, for `--demo`, stopped when the app quits.
    var demo: Process?
    var started: Date?

    init(launch: Launch) {
        self.launch = launch
    }

    /// The jawd beside the app, or, building from source, the one cargo built.
    static var jawd: String? {
        let helpers = Bundle.main.bundleURL.appending(path: "Contents/Helpers/jawd").path
        if FileManager.default.isExecutableFile(atPath: helpers) { return helpers }
        if let env = ProcessInfo.processInfo.environment["JAWD"], FileManager.default.isExecutableFile(atPath: env) {
            return env
        }
        #if DEBUG
            let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
                .deletingLastPathComponent().deletingLastPathComponent()
            for profile in ["debug", "release"] {
                let built = repo.appending(path: "target/\(profile)/jawd").path
                if FileManager.default.isExecutableFile(atPath: built) { return built }
            }
        #endif
        return nil
    }

    static func answers(_ socket: String) -> Bool {
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        defer { Darwin.close(fd) }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(socket.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: addr.sun_path) else { return false }
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        return withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, size) }
        } == 0
    }

    /// Makes sure a jawd will answer on the app's folder; says why not when it can't.
    func ensure() -> String? {
        if Self.answers(launch.socket) { return nil }
        if launch.demo { return startDemo() }
        // jawd at login is registered only when the owner asks, with jaw › jawd at Login…: doing it
        // by itself on his Mac would replace his jawd with a build in progress.
        return "nothing answers on \(launch.socket): Jaw › jawd at Login… starts Jaw's own, jaw agent install the terminal's, and --demo a made-up account"
    }

    /// `jawd --engine demo` on the demo folder: a made-up account, with no WhatsApp behind it.
    func startDemo() -> String? {
        guard let jawd = Self.jawd else { return "there's no jawd to run: make build builds it" }
        try? FileManager.default.createDirectory(atPath: launch.home, withIntermediateDirectories: true,
                                                 attributes: [.posixPermissions: 0o700])
        let log = (launch.home as NSString).appendingPathComponent("jawd.log")
        FileManager.default.createFile(atPath: log, contents: nil)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: jawd)
        process.arguments = ["--engine", "demo", "--no-notify"]
        var env = ProcessInfo.processInfo.environment
        env["JAW_HOME"] = launch.home
        process.environment = env
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle(forWritingAtPath: log)
        do {
            try process.run()
        } catch {
            return "can't start jawd: \(error.localizedDescription)"
        }
        demo = process
        started = Date()
        return nil
    }

    /// Whether the app started a jawd that's still starting, as the terminal waits 15 s for one.
    var starting: Bool {
        guard let started else { return false }
        return Date().timeIntervalSince(started) < 15 && (demo?.isRunning ?? true)
    }

    /// Why the jawd the app started stopped, if it did.
    var stopped: String? {
        guard let demo, !demo.isRunning else { return nil }
        let log = (try? String(contentsOfFile: (launch.home as NSString).appendingPathComponent("jawd.log"), encoding: .utf8)) ?? ""
        let last = log.split(separator: "\n").last.map { ": \($0)" } ?? ""
        return "jawd stopped as it started (status \(demo.terminationStatus))\(last)"
    }

    /// Stops the jawd the app started, giving it a few seconds to go, so its folder is still by the
    /// time the app has quit: a snapshot's folder is removed then.
    func stop() {
        guard let demo, demo.isRunning else { return }
        demo.terminate()
        let until = Date().addingTimeInterval(5)
        while demo.isRunning, Date() < until {
            usleep(10000)
        }
    }

    // MARK: - The LaunchAgent

    var service: SMAppService {
        SMAppService.agent(plistName: Self.plist)
    }

    /// The plist `jaw agent install` writes, if the terminal installed its own agent.
    static var terminalAgent: String {
        (NSHomeDirectory() as NSString).appendingPathComponent("Library/LaunchAgents/\(plist)")
    }

    /// Registers the bundled jawd as the agent, unless the terminal's agent is installed, which the
    /// app offers to take over and never does by itself.
    func registerAgent() -> String? {
        if FileManager.default.fileExists(atPath: Self.terminalAgent) {
            return "jaw agent install's agent is installed but jawd isn't answering; jaw agent status says more"
        }
        switch service.status {
        case .enabled:
            return nil
        case .requiresApproval:
            return "jawd needs your approval in System Settings › General › Login Items"
        default:
            do {
                try service.register()
                started = Date()
                return nil
            } catch {
                return "can't register jawd to start at login: \(error.localizedDescription)"
            }
        }
    }

    /// Takes over the terminal's agent, on the owner's click: stops it, removes its plist, and
    /// registers the app's own under the same label. A newer jawd than the app's own is kept, since an
    /// older jawd refuses a newer jaw.db.
    func takeOver(running: String?, own: String?) -> String? {
        if let running, let own, Self.newer(running, than: own) {
            return "the jawd running (\(running)) is newer than this app's (\(own)), so it stays"
        }
        let uid = getuid()
        let bootout = Process()
        bootout.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        bootout.arguments = ["bootout", "gui/\(uid)/\(Self.label)"]
        try? bootout.run()
        bootout.waitUntilExit()
        try? FileManager.default.removeItem(atPath: Self.terminalAgent)
        return registerAgent()
    }

    /// Whether version `a` is newer than `b`, as git describe gives them: by the release's dotted
    /// numbers, then the commits since it, so 0.2.0-3-g4d2c1e9 is newer than 0.2.0 and older than
    /// 0.2.1. Before the first tag a build says its commit's hash, which has no order, and neither is
    /// newer, as when every jawd said 0.1.0.
    static func newer(_ a: String, than b: String) -> Bool {
        guard let a = order(a), let b = order(b) else { return false }
        return b.lexicographicallyPrecedes(a)
    }

    /// A version's release numbers and the commits since it: [0, 2, 0] for 0.2.0, [0, 2, 0, 3] for
    /// 0.2.0-3-g4d2c1e9, and nil for a commit's hash or anything else.
    static func order(_ version: String) -> [Int]? {
        let parts = version.split(separator: "-", omittingEmptySubsequences: false)
        let release = parts[0].split(separator: ".", omittingEmptySubsequences: false).map { Int($0) }
        guard release.count == 3, !release.contains(nil) else { return nil }
        let numbers = release.compactMap(\.self)
        switch parts.count {
        case 1: return numbers
        case 3 where parts[2].hasPrefix("g"): return Int(parts[1]).map { numbers + [$0] }
        default: return nil
        }
    }
}
