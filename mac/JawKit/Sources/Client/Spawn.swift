import Darwin
import Foundation

/// A program to start as jawd is started, as the terminal's `start_jawd` starts one
/// (`crates/jaw/src/wire.rs`): its arguments and environment, the log its standard error is
/// appended to, and whether it runs in a process group of its own, so it isn't stopped with the app.
public struct Spawn: Equatable, Sendable {
    public var program: String
    public var arguments: [String]
    public var environment: [String: String]
    /// Where its standard error goes, appended; the folder is made if it isn't there, readable only by you.
    public var log: String
    public var ownGroup: Bool

    public init(program: String, arguments: [String], environment: [String: String], log: String, ownGroup: Bool) {
        self.program = program
        self.arguments = arguments
        self.environment = environment
        self.log = log
        self.ownGroup = ownGroup
    }

    /// Starts it with nothing on its standard input and output, its standard error appended to the
    /// log, none of the app's other descriptors, and every signal at its default and unblocked;
    /// `exited` is told its PID and wait status on the main queue once it has gone, however long after.
    public func run(exited: @escaping @MainActor @Sendable (pid_t, Int32) -> Void) throws -> pid_t {
        try FileManager.default.createDirectory(atPath: (log as NSString).deletingLastPathComponent,
                                                withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        var actions: posix_spawn_file_actions_t?
        posix_spawn_file_actions_init(&actions)
        defer { posix_spawn_file_actions_destroy(&actions) }
        posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0)
        posix_spawn_file_actions_addopen(&actions, 1, "/dev/null", O_WRONLY, 0)
        posix_spawn_file_actions_addopen(&actions, 2, log, O_WRONLY | O_CREAT | O_APPEND, 0o600)
        var attributes: posix_spawnattr_t?
        posix_spawnattr_init(&attributes)
        defer { posix_spawnattr_destroy(&attributes) }
        var flags = POSIX_SPAWN_CLOEXEC_DEFAULT | POSIX_SPAWN_SETSIGDEF | POSIX_SPAWN_SETSIGMASK
        if ownGroup {
            flags |= POSIX_SPAWN_SETPGROUP
            posix_spawnattr_setpgroup(&attributes, 0)
        }
        posix_spawnattr_setflags(&attributes, Int16(flags))
        var unblocked = sigset_t()
        sigemptyset(&unblocked)
        posix_spawnattr_setsigmask(&attributes, &unblocked)
        var defaults = sigset_t()
        sigfillset(&defaults)
        sigdelset(&defaults, SIGKILL)
        sigdelset(&defaults, SIGSTOP)
        posix_spawnattr_setsigdefault(&attributes, &defaults)
        let argv = ([program] + arguments).map { strdup($0) } + [nil]
        let envp = environment.map { strdup("\($0.key)=\($0.value)") } + [nil]
        defer {
            for string in argv + envp {
                free(string)
            }
        }
        var pid: pid_t = 0
        let failed = posix_spawn(&pid, program, &actions, &attributes, argv, envp)
        guard failed == 0 else { throw POSIXError(POSIXErrorCode(rawValue: failed) ?? .EINVAL) }
        let started = pid
        Thread.detachNewThread {
            var status: Int32 = 0
            while waitpid(started, &status, 0) == -1, errno == EINTR {}
            let ended = status
            DispatchQueue.main.async { MainActor.assumeIsolated { exited(started, ended) } }
        }
        return started
    }

    /// A wait status in the terminal's words, which are Rust's `ExitStatus`'s: "exit status: 1", or
    /// "signal: 9 (SIGKILL)" for one a signal stopped, with " (core dumped)" when it left a core.
    public static func said(_ status: Int32) -> String {
        let signal = status & 0x7F
        guard signal != 0 else { return "exit status: \((status >> 8) & 0xFF)" }
        let names = ["HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "EMT", "FPE", "KILL", "BUS", "SEGV", "SYS", "PIPE", "ALRM", "TERM", "URG",
                     "STOP", "TSTP", "CONT", "CHLD", "TTIN", "TTOU", "IO", "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "INFO", "USR1", "USR2"]
        let name = Int(signal) <= names.count ? " (SIG\(names[Int(signal) - 1]))" : ""
        return "signal: \(signal)\(name)" + ((status & 0x80) != 0 ? " (core dumped)" : "")
    }

    /// A log's last line that says something, without jawd's "jawd: ", read from its last 4 KB: why a
    /// jawd that stopped as it started did, which jawd says on its last line.
    public static func lastWords(_ log: String) -> String {
        guard let file = FileHandle(forReadingAtPath: log) else { return "" }
        defer { try? file.close() }
        let end = (try? file.seekToEnd()) ?? 0
        try? file.seek(toOffset: end > 4096 ? end - 4096 : 0)
        let text = String(decoding: (try? file.readToEnd()) ?? Data(), as: UTF8.self)
        let last = text.split(separator: "\n").reversed().map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
        return last.hasPrefix("jawd: ") ? String(last.dropFirst(6)) : last
    }
}
