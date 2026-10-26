import Darwin
import Foundation

/// How the app was started: which jaw folder it shows, whether it runs a made-up account, and
/// whether it's measuring itself.
struct Launch {
    /// jaw's folder: `--home`, `JAW_HOME`, or the platform's, as the terminal resolves it.
    let home: String
    /// A made-up account in a folder of its own, served by a jawd the app starts.
    let demo: Bool
    /// What `--bench` measures, if anything.
    let bench: String?
    /// `--snapshot <path>`: draws the window into a picture once a chat is open, and quits.
    let snapshot: String?

    static let platformHome = (NSHomeDirectory() as NSString).appendingPathComponent("Library/Application Support/jaw")
    static let demoHome = (NSTemporaryDirectory() as NSString).appendingPathComponent("jaw-mac-demo")

    static func parse(_ arguments: [String], environment: [String: String] = ProcessInfo.processInfo.environment) -> Launch {
        var home: String?
        var demo = false
        var bench: String?
        var snapshot: String?
        var i = 1
        while i < arguments.count {
            switch arguments[i] {
            case "--home" where i + 1 < arguments.count:
                home = arguments[i + 1]
                i += 1
            case "--demo":
                demo = true
            case "--snapshot" where i + 1 < arguments.count:
                snapshot = arguments[i + 1]
                i += 1
            case "--bench" where i + 1 < arguments.count:
                bench = arguments[i + 1]
                i += 1
            default:
                break
            }
            i += 1
        }
        home = home ?? environment["JAW_HOME"].flatMap { $0.isEmpty ? nil : $0 }
        #if DEBUG
            // A debug build never opens the owner's folder unless it's told to: its jawd could be a
            // branch's, which might migrate jaw.db to a schema main doesn't know.
            if home == nil { demo = true }
        #endif
        if demo, home == nil { home = demoHome }
        return Launch(home: home ?? platformHome, demo: demo, bench: bench, snapshot: snapshot)
    }

    var socket: String {
        (home as NSString).appendingPathComponent("jaw.sock")
    }

    /// When this process started, by the kernel's clock, for the launch bench.
    static func processStart() -> Date? {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var mib: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        guard sysctl(&mib, 4, &info, &size, nil, 0) == 0 else { return nil }
        let t = info.kp_proc.p_starttime
        return Date(timeIntervalSince1970: Double(t.tv_sec) + Double(t.tv_usec) / 1e6)
    }
}
