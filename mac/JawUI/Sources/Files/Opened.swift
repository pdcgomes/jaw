import Darwin
import Foundation

/// The copies a file is looked at, opened and dragged out from: a clone of jawd's file in the app's
/// own temporary folder, named as `media.save` names a copy. jawd keeps a file once for every message
/// that has it, and an app that edits what it opens saves into it, so nothing jaw hands another app
/// is jawd's own file. On APFS a clone costs neither space nor time and leaves jawd's file as it is
/// whatever is done to it; where one can't be made, as on another volume, it's a copy.
public enum Opened {
    /// The folder the clones are made in, emptied at launch of what an earlier run left.
    public static let folder = FileManager.default.temporaryDirectory.appending(path: "dev.badcoffee.jaw.opened", directoryHint: .isDirectory)

    /// A fresh clone of `file` named `name`, in a folder of its own for `message`, so two messages'
    /// files of one name don't meet. It can take as long as a copy, so it's made off the main thread.
    public static func copy(of file: URL, named name: String, for message: String, in folder: URL = Opened.folder) throws -> URL {
        let safe = message.map { $0.isLetter || $0.isNumber ? String($0) : "_" }.joined()
        let dir = folder.appending(path: safe, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let target = dir.appending(path: name)
        if FileManager.default.fileExists(atPath: target.path) { try FileManager.default.removeItem(at: target) }
        if clonefile(file.path, target.path, 0) == 0 { return target }
        guard errno == EXDEV || errno == ENOTSUP || errno == EEXIST else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        try FileManager.default.copyItem(at: file, to: target)
        return target
    }

    /// A clone, when one can be made at once, as a drag starting needs; nil where it would take a copy.
    public static func clone(of file: URL, named name: String, for message: String, in folder: URL = Opened.folder) -> URL? {
        let safe = message.map { $0.isLetter || $0.isNumber ? String($0) : "_" }.joined()
        let dir = folder.appending(path: safe, directoryHint: .isDirectory)
        guard (try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)) != nil else { return nil }
        let target = dir.appending(path: name)
        try? FileManager.default.removeItem(at: target)
        return clonefile(file.path, target.path, 0) == 0 ? target : nil
    }

    /// Lets go of every clone an earlier run made.
    public static func empty(_ folder: URL = Opened.folder) {
        try? FileManager.default.removeItem(at: folder)
    }
}
