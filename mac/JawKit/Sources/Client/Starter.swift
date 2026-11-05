import Foundation

/// Who starts jawd for the app, and what the window says of jawd meanwhile, as the terminal's
/// `wire.rs` has them. When nothing answers at launch, a release starts the jawd inside it, in a
/// process group of its own so it goes on when the app quits, and `--demo` the demo's, which the app
/// stops. A jawd the app started has 15 s to answer, and one that stopped as it started says why from
/// its log, unless another jawd held the folder, when it's started again a second later while nothing
/// answers, whether or not the window is drawing. On an account's folder, jaw's own or one holding a
/// `session.db`, only a Jaw built from origin/main or a release's tag starts one, and `--demo` none.
/// On jaw's own folder such a Jaw then asks for the agent an earlier Jaw registered to be
/// unregistered, never waiting on the answer, unless the terminal's agent holds the label, which it
/// names with what removes it. The app carries out its steps and tells it what happens.
public struct Starter: Sendable {
    /// What the app knows at launch.
    public struct Here: Equatable, Sendable {
        /// A build from source, which starts no jawd but `--demo`'s.
        public var debug: Bool
        /// `--demo`: a made-up account, on a jawd the app starts and stops.
        public var demo: Bool
        /// jaw's folder, which holds the socket and the log.
        public var home: String
        /// Whether `home` is jaw's own folder, which holds the owner's account.
        public var jawsOwn: Bool
        /// Whether `home` holds an engine's `session.db`, as another account's folder does.
        public var account: Bool
        public var build: Build
        /// The jawd to start: the one inside the app for a release, the one `--demo` finds for it.
        public var jawd: String?
        /// The app's environment, which the jawd it starts gets with `JAW_HOME` set to `home`.
        public var environment: [String: String]
        /// Whether `jaw agent install`'s plist is in `~/Library/LaunchAgents`.
        public var terminalAgent: Bool

        public init(debug: Bool, demo: Bool, home: String, jawsOwn: Bool, account: Bool, build: Build, jawd: String?,
                    environment: [String: String], terminalAgent: Bool)
        {
            self.debug = debug
            self.demo = demo
            self.home = home
            self.jawsOwn = jawsOwn
            self.account = account
            self.build = build
            self.jawd = jawd
            self.environment = environment
            self.terminalAgent = terminalAgent
        }
    }

    /// What the app does for it.
    public enum Step: Equatable, Sendable {
        /// Ask `SMAppService` to unregister the agent an earlier Jaw registered, then tell `unregistered`.
        case unregister
        /// Start this jawd, then tell `exited` when it goes, or `didntStart` if it can't be started.
        case start(Spawn)
        /// Wait this long, whatever the window does, then tell `waited`.
        case wait(TimeInterval)
        /// Say this in the status line.
        case say(String, warn: Bool)
    }

    /// What the window says of jawd until it answers.
    public enum Shown: Equatable, Sendable {
        /// Nothing yet: a jawd may answer.
        case waiting
        /// A jawd the app started is starting.
        case starting
        /// It didn't start, or the app won't start one here, and why.
        case failed(String)
        /// Nothing the app started is on its way: jaw-client's words, as it tries again.
        case unreachable
    }

    /// How long a jawd the app started has to answer, as the terminal waits (`PATIENCE` in `wire.rs`).
    public static let patience: TimeInterval = 15
    /// How long after a jawd stopped because another held the folder it's started again, on the app's
    /// own timer, since jaw-client's failed tries are drained only at the window's display frames.
    public static let again: TimeInterval = 1
    /// What jawd says when another jawd holds the folder's `jaw.lock` (`lock` in crates/jawd/src/main.rs).
    public static let held = "another jawd is running on "

    public let here: Here
    /// When the app first started a jawd that hasn't answered yet.
    var since: Date?
    /// Whether the jawd it started last is running.
    var running = false
    /// Whether it's to be started again once the app has waited, after it stopped because another
    /// jawd held the folder.
    var restarting = false
    /// Why it didn't start, or why the app won't start one here, until a jawd answers.
    var failed: String?
    /// Whether `SMAppService` has yet to answer for an earlier Jaw's agent.
    var unregistering = false

    public init(_ here: Here) {
        self.here = here
    }

    /// At launch, once the app knows whether a jawd answers: what it does, a jawd to start coming before
    /// anything asked of `SMAppService`, and what the window says until jawd's first word.
    public mutating func launched(answers: Bool, now: Date) -> ([Step], Shown) {
        var steps: [Step] = []
        if here.jawsOwn, here.terminalAgent {
            let starts = refusal == nil ? ", and Jaw starts jawd as it opens" : ""
            steps.append(.say("jaw agent install's LaunchAgent still starts jawd at login: jaw agent uninstall removes it\(starts)", warn: true))
        }
        steps += decide(answers: answers, now: now)
        if here.jawsOwn, !here.terminalAgent, refusal == nil {
            unregistering = true
            steps.append(.unregister)
        }
        if let failed { return (steps, .failed(failed)) }
        return (steps, running ? .starting : .waiting)
    }

    /// `SMAppService` answered for the earlier Jaw's agent, `removed` when there was one, whose jawd it
    /// has stopped by then: when nothing answers, no jawd of the app's is running or to be started
    /// again, and none failed to start, the app starts its own, which has 15 s from then to answer.
    public mutating func unregistered(removed: Bool, answers: Bool, now: Date) -> [Step] {
        guard unregistering else { return [] }
        unregistering = false
        guard removed else { return [] }
        let said = Step.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)
        guard !running, !restarting, failed == nil else { return [said] }
        since = nil
        return [said] + decide(answers: answers, now: now)
    }

    /// jaw-client couldn't reach jawd: what the window says.
    public func unreachable(now: Date) -> Shown {
        if let failed { return .failed(failed) }
        if let since, running || restarting, now.timeIntervalSince(since) < Self.patience { return .starting }
        return .unreachable
    }

    /// The jawd the app started stopped, with this wait status and these last words in its log: when
    /// another jawd held the folder, the app waits before it's started again.
    public mutating func exited(status: Int32, said: String, now: Date) -> [Step] {
        guard let since else { return [] }
        running = false
        guard said.hasPrefix(Self.held), now.timeIntervalSince(since) < Self.patience else {
            failed = "jawd stopped as it started (\(Spawn.said(status)))" + (said.isEmpty ? "" : ": \(said)")
            return []
        }
        restarting = true
        return [.wait(Self.again)]
    }

    /// The app has waited after a jawd stopped because another held the folder: the jawd to start
    /// again, once, unless a jawd answers now or has said hello since.
    public mutating func waited(answers: Bool, now: Date) -> [Step] {
        guard restarting else { return [] }
        restarting = false
        return answers ? [] : start(now)
    }

    /// The jawd couldn't be started at all, for this reason.
    public mutating func didntStart(_ why: String) {
        running = false
        failed = why
    }

    /// A jawd answered: what becomes of it from now on is jaw-client's to say.
    public mutating func answered() {
        since = nil
        running = false
        restarting = false
        failed = nil
    }

    /// The jawd to start: a release's own, in a process group of its own, or the demo's, which the
    /// app stops, each with its output appended to `jawd.log` beside the socket.
    public var spawn: Spawn? {
        guard let jawd = here.jawd else { return nil }
        var environment = here.environment
        environment["JAW_HOME"] = here.home
        let log = (here.home as NSString).appendingPathComponent("jawd.log")
        if here.demo {
            return Spawn(program: jawd, arguments: ["--engine", "demo", "--no-notify"], environment: environment, log: log, ownGroup: false)
        }
        return Spawn(program: jawd, arguments: [], environment: environment, log: log, ownGroup: true)
    }

    /// Why the app won't start a jawd here, if it won't.
    var refusal: String? {
        let socket = (here.home as NSString).appendingPathComponent("jaw.sock")
        let account = here.jawsOwn || here.account
        if here.demo, account {
            return "--demo runs a made-up account, so not on jaw's own folder or another account's: give it a folder of its own with --home"
        }
        if here.demo { return here.jawd == nil ? "there's no jawd to run: make build builds it" : nil }
        if here.debug {
            return "nothing answers on \(socket), and a build from source starts only --demo's jawd: start one there with jaw, or open Jaw with --demo"
        }
        if account, !here.build.startsOnAccounts { return here.build.refusal }
        if here.jawd == nil { return "there's no jawd inside this Jaw: make mac puts one in its Contents/Helpers" }
        return nil
    }

    mutating func decide(answers: Bool, now: Date) -> [Step] {
        guard !answers else { return [] }
        if let why = refusal {
            failed = why
            return []
        }
        return start(now)
    }

    mutating func start(_ now: Date) -> [Step] {
        guard let spawn else { return [] }
        since = since ?? now
        running = true
        return [.start(spawn)]
    }

    /// Whether two paths are one folder: the same once standardised, symlinks resolved and case
    /// ignored, or, for a folder that's there, the same file on the same volume as the kernel finds it
    /// at each path as written, through symlinks, `..` after one, and firmlinks.
    public static func same(_ a: String, _ b: String) -> Bool {
        func path(_ p: String) -> String {
            URL(fileURLWithPath: p).standardizedFileURL.resolvingSymlinksInPath().path
        }
        if path(a).lowercased() == path(b).lowercased() { return true }
        func identity(_ p: String) -> (dev_t, ino_t)? {
            var info = stat()
            guard stat(p, &info) == 0 else { return nil }
            return (info.st_dev, info.st_ino)
        }
        guard let first = identity(a), let second = identity(b) else { return false }
        return first == second
    }
}
