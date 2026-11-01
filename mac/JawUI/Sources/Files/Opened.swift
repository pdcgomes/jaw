import CoreServices
import Darwin
import Foundation

/// The copies a file is looked at, opened and dragged out from: a clone of jawd's file in the app's
/// own temporary folder, named as `media.save` names a copy. jawd keeps a file once for every message
/// that has it, and an app that edits what it opens saves into it, so nothing jaw hands another app
/// is jawd's own file. On APFS a clone costs neither space nor time and leaves jawd's file as it is
/// whatever is done to it; where one can't be made, as on another volume, it's a copy.
///
/// Each is quarantined as a message's attachment, so macOS checks what someone sent before it runs,
/// as it checks a download; one looked at or opened is read-only, so an app that edits it offers to
/// duplicate it rather than saving into a file that goes when Jaw quits. Each run keeps its own in a
/// folder of its own, emptied as it quits, and the folders of runs that have gone are emptied as one
/// starts, so a run beside another never takes its files away.
public enum Opened {
    /// Every run's folders.
    public static let root = FileManager.default.temporaryDirectory.appending(path: "dev.badcoffee.jaw.opened", directoryHint: .isDirectory)
    /// This run's, named by its process and when that started.
    public static let folder = root.appending(path: run(getpid()) ?? "\(getpid())", directoryHint: .isDirectory)

    /// A run's folder's name, its process's ID and when it started in microseconds; nil when no
    /// process has that ID.
    static func run(_ pid: pid_t) -> String? {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var mib: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, pid]
        guard sysctl(&mib, 4, &info, &size, nil, 0) == 0, size > 0, info.kp_proc.p_pid == pid else { return nil }
        let t = info.kp_proc.p_starttime
        return "\(pid)-\(Int64(t.tv_sec) * 1_000_000 + Int64(t.tv_usec))"
    }

    /// A clone of `file` named `name`, read-only and quarantined, in a folder of its own for
    /// `message`, so two messages' files of one name don't meet; the one made before, when it's of
    /// the same file, so an app showing it keeps it. It can take as long as a copy, so it's made off
    /// the main thread.
    public static func copy(of file: URL, named name: String, for message: String, in folder: URL = Opened.folder) throws -> URL {
        let target = try place(message, name, in: folder.appending(path: "open", directoryHint: .isDirectory))
        if same(target, as: file) { return target }
        try? FileManager.default.removeItem(at: target)
        if clonefile(file.path, target.path, 0) != 0 {
            guard errno == EXDEV || errno == ENOTSUP else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
            try FileManager.default.copyItem(at: file, to: target)
        }
        try handOver(target, readOnly: true)
        return target
    }

    /// A clone made at once, as a drag starting needs, nil where it would take a copy: quarantined,
    /// and left writable, as the Finder keeps a file's mode on the copy it makes of a drop. An app
    /// dropped on that opens the file where it is, as Preview's Dock icon does, edits this clone
    /// itself, and what it saves is lost when Jaw quits or the file is dragged again.
    public static func clone(of file: URL, named name: String, for message: String, in folder: URL = Opened.folder) -> URL? {
        guard let target = try? place(message, name, in: folder.appending(path: "drag", directoryHint: .isDirectory)) else { return nil }
        if same(target, as: file) { return target }
        try? FileManager.default.removeItem(at: target)
        guard clonefile(file.path, target.path, 0) == 0, (try? handOver(target, readOnly: false)) != nil else { return nil }
        return target
    }

    /// A folder of its own in this run's, for jawd to save a promised file into before it's put
    /// where it was dropped.
    public static func promised(in folder: URL = Opened.folder) throws -> URL {
        let made = folder.appending(path: "promised", directoryHint: .isDirectory).appending(path: UUID().uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: made, withIntermediateDirectories: true)
        return made
    }

    /// Puts `file` at `url`, or beside it under its name with a number when that's taken, as jawd
    /// names a copy, never over a file: moved when both are on one volume, otherwise copied whole,
    /// synced, and `file` removed. Says where it is.
    public static func put(_ file: URL, at url: URL) throws -> URL {
        for candidate in named(url) {
            if renamex_np(file.path, candidate.path, UInt32(RENAME_EXCL)) == 0 { return candidate }
            switch errno {
            case EEXIST: continue
            case EXDEV: return try copied(file, to: url)
            default: throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
            }
        }
        throw CocoaError(.fileWriteFileExists)
    }

    /// `put`'s copy onto another volume: whole under the first name that's free, synced, then `file`
    /// removed.
    static func copied(_ file: URL, to url: URL) throws -> URL {
        for candidate in named(url) {
            do {
                try FileManager.default.copyItem(at: file, to: candidate)
            } catch CocoaError.fileWriteFileExists {
                continue
            } catch {
                try? FileManager.default.removeItem(at: candidate)
                throw error
            }
            let fd = open(candidate.path, O_RDONLY)
            if fd >= 0 {
                _ = fcntl(fd, F_FULLFSYNC)
                close(fd)
            }
            try? FileManager.default.removeItem(at: file)
            return candidate
        }
        throw CocoaError(.fileWriteFileExists)
    }

    /// `url`, then its name with a number before its extension, as jawd names a copy where one's
    /// taken: `topo (1).pdf`.
    static func named(_ url: URL) -> [URL] {
        let folder = url.deletingLastPathComponent(), name = url.lastPathComponent
        let (stem, ext) = name.lastIndex(of: ".").map { $0 == name.startIndex ? (name, "") : (String(name[..<$0]), String(name[$0...])) }
            ?? (name, "")
        return (0 ..< 1000).map { n in folder.appending(path: n == 0 ? name : "\(stem) (\(n))\(ext)") }
    }

    /// Where `message`'s copy named `name` goes in `folder`, its folder made.
    static func place(_ message: String, _ name: String, in folder: URL) throws -> URL {
        let safe = message.map { $0.isLetter || $0.isNumber ? String($0) : "_" }.joined()
        let dir = folder.appending(path: safe, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appending(path: name)
    }

    /// Whether `copy` was made of `file` as it is: the same size and time, which a clone and a copy
    /// keep.
    static func same(_ copy: URL, as file: URL) -> Bool {
        guard let a = try? FileManager.default.attributesOfItem(atPath: copy.path),
              let b = try? FileManager.default.attributesOfItem(atPath: file.path) else { return false }
        return a[.size] as? Int == b[.size] as? Int && a[.modificationDate] as? Date == b[.modificationDate] as? Date
    }

    /// Quarantines `url` as a message's attachment Jaw received, and makes it read-only when asked:
    /// the quarantine is set first, as it can't be on a file that can't be written.
    public static func handOver(_ url: URL, readOnly: Bool) throws {
        var values = URLResourceValues()
        values.quarantineProperties = [kLSQuarantineTypeKey as String: kLSQuarantineTypeInstantMessageAttachment as String,
                                       kLSQuarantineAgentNameKey as String: "Jaw"]
        var marked = url
        try marked.setResourceValues(values)
        if readOnly { try FileManager.default.setAttributes([.posixPermissions: 0o444], ofItemAtPath: url.path) }
    }

    /// Empties the folders of runs that have gone, as a run starts; a run still going keeps its own.
    public static func sweep(_ root: URL = Opened.root) {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: root.path) else { return }
        for name in names where !running(name) {
            try? FileManager.default.removeItem(at: root.appending(path: name))
        }
    }

    /// Whether the run a folder's named for is still going: its process, started when it says.
    static func running(_ name: String) -> Bool {
        guard let pid = name.split(separator: "-").first.flatMap({ pid_t($0) }) else { return false }
        return run(pid) == name
    }

    /// Empties this run's folder, as it quits.
    public static func leave(_ folder: URL = Opened.folder) {
        try? FileManager.default.removeItem(at: folder)
    }
}
