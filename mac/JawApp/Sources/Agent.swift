import AppKit
import Foundation
import JawKit
import JawUI
import ServiceManagement

/// Who starts jawd, as JawKit's `Starter` decides it: a release starts the jawd inside it when nothing
/// answers, in a process group of its own so it goes on when the app quits, and on jaw's own folder
/// then asks `SMAppService` to unregister the LaunchAgent an earlier Jaw registered; a debug build,
/// and `--demo`, start only the demo's jawd, on a folder of its own, and stop it when the app quits.
/// There's no jawd at login: the owner decided on 29 October that jawd runs while an app does.
@MainActor
final class Agent {
    /// The label an earlier Jaw registered its jawd under, as `jaw agent install` does.
    static let label = "dev.badcoffee.jaw.jawd"

    let launch: Launch
    var starter: Starter
    /// Says something in the status line, as the model does.
    var say: (String, Bool) -> Void = { _, _ in }
    /// The jawd this app started for `--demo`, stopped when the app quits.
    var demo: pid_t?
    /// `JAW_TRACE=1` says each jawd started and stopped, and when `SMAppService` was asked and what it answered.
    let trace = ProcessInfo.processInfo.environment["JAW_TRACE"] != nil

    init(launch: Launch) {
        self.launch = launch
        #if DEBUG
            let debug = true
        #else
            let debug = false
        #endif
        starter = Starter(Starter.Here(
            debug: debug,
            demo: launch.demo,
            home: launch.home,
            jawsOwn: Starter.same(launch.home, Launch.platformHome),
            account: FileManager.default.fileExists(atPath: (launch.home as NSString).appendingPathComponent("session.db")),
            build: Build(info: Bundle.main.infoDictionary ?? [:]),
            jawd: launch.demo ? Self.jawd : Self.helpers,
            environment: ProcessInfo.processInfo.environment,
            terminalAgent: FileManager.default.fileExists(atPath: Self.terminalAgent),
        ))
    }

    /// The jawd inside the app, the only one a release starts by itself.
    static var helpers: String? {
        let inside = Bundle.main.bundleURL.appending(path: "Contents/Helpers/jawd").path
        return FileManager.default.isExecutableFile(atPath: inside) ? inside : nil
    }

    /// The jawd `--demo` runs: the one inside the app, `JAWD`'s, or, building from source, the one
    /// cargo built.
    static var jawd: String? {
        if let helpers = Self.helpers { return helpers }
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

    /// The plist `jaw agent install` writes, if the terminal installed its own agent.
    static var terminalAgent: String {
        (NSHomeDirectory() as NSString).appendingPathComponent("Library/LaunchAgents/\(label).plist")
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

    /// At launch: what the starter does first, and what the window says until jawd's first word.
    func launched() -> Starter.Shown {
        let (steps, shown) = starter.launched(answers: Self.answers(launch.socket), now: Date())
        run(steps)
        return shown
    }

    /// jaw-client couldn't reach jawd: what the window says.
    func unreachable() -> Starter.Shown {
        starter.unreachable(now: Date())
    }

    /// jawd said hello.
    func answered() {
        starter.answered()
    }

    func run(_ steps: [Starter.Step]) {
        for step in steps {
            switch step {
            case let .say(text, warn): say(text, warn)
            case .unregister: unregister()
            case let .start(spawn): start(spawn)
            case let .wait(seconds): DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in self?.waited() }
            }
        }
    }

    /// The wait the starter asked for is over: on the main queue's own timer, so it ends whether or not
    /// the window is drawing.
    func waited() {
        run(starter.waited(answers: Self.answers(launch.socket), now: Date()))
    }

    func start(_ spawn: Spawn) {
        do {
            let pid = try spawn.run { [weak self] pid, status in self?.exited(pid, status, log: spawn.log) }
            if !spawn.ownGroup { demo = pid }
            note("started jawd \(pid)\(spawn.ownGroup ? ", in a process group of its own" : "")")
        } catch {
            let why = "can't start jawd: \(error.localizedDescription)"
            note(why)
            starter.didntStart(why)
        }
    }

    func exited(_ pid: pid_t, _ status: Int32, log: String) {
        if demo == pid { demo = nil }
        let words = Spawn.lastWords(log)
        note("jawd \(pid) stopped (\(Spawn.said(status)))\(words.isEmpty ? "" : ": \(words)")")
        run(starter.exited(status: status, said: words, now: Date()))
    }

    /// Asks `SMAppService` to unregister the agent an earlier Jaw registered, which stops that agent's
    /// jawd before it answers, and tells the starter; an error is there being none it could find.
    /// `SMAppService` calls the handler on libdispatch's default queue, so it's `@Sendable`, never the
    /// main actor's, and hops to the main queue itself.
    func unregister() {
        let service = SMAppService.agent(plistName: "\(Self.label).plist")
        let asked = Date()
        if trace {
            note("unregistering an earlier Jaw's agent at \(AppModel.sinceStart) ms, which SMAppService says is \(Self.said(service.status))")
        }
        service.unregister { @Sendable error in
            let removed = error == nil
            let answer = error.map { "\(($0 as NSError).domain) \(($0 as NSError).code): \($0.localizedDescription)" } ?? "done"
            let took = Int(Date().timeIntervalSince(asked) * 1000)
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self.note("unregistered \(took) ms after asking: \(answer)")
                    self.run(self.starter.unregistered(removed: removed, answers: Self.answers(self.launch.socket), now: Date()))
                }
            }
        }
    }

    /// An agent's status as the trace says it.
    static func said(_ status: SMAppService.Status) -> String {
        switch status {
        case .notRegistered: "not registered"
        case .enabled: "enabled"
        case .requiresApproval: "waiting for approval"
        case .notFound: "not found"
        @unknown default: "status \(status.rawValue)"
        }
    }

    /// With `JAW_TRACE`, says it on standard error.
    func note(_ text: String) {
        if trace { FileHandle.standardError.write(Data("jaw-mac: \(text)\n".utf8)) }
    }

    /// Stops the jawd the app started for `--demo`, once, giving it a few seconds to go, so its folder
    /// is still by the time the app has quit: a snapshot's folder is removed then. A release's jawd
    /// goes on, as the terminal's does.
    func stop() {
        guard let demo else { return }
        self.demo = nil
        kill(demo, SIGTERM)
        let until = Date().addingTimeInterval(5)
        while kill(demo, 0) == 0, Date() < until {
            usleep(10000)
        }
    }
}

extension Situation.Reach {
    /// What the window says of jawd, from what the starter says and jaw-client's words.
    init(_ shown: Starter.Shown, why: String = "", retry: Int = 0) {
        switch shown {
        case .waiting: self = .waiting
        case .starting: self = .starting(failed: nil)
        case let .failed(said): self = .starting(failed: said)
        case .unreachable: self = .unreachable(why: why, retry: retry)
        }
    }
}
