import Foundation

/// The folder a sweep writes into, and the rules that keep a sweep from losing anything there: it
/// draws into a hidden folder beside it and puts that in its place only once it's whole, and it
/// replaces only a missing or empty folder, or one a sweep made. `mac/scripts/take-sweep`, which
/// brings the build Mac's sweep back here, knows a folder a sweep made by the same rule, and the two
/// must agree.
enum SweepFolder {
    /// What the check reads of a sweep's `manifest.json`: the scenes it lists and their pictures'
    /// files, and nothing else, so a field a later phase adds to `Pictures.Manifest` doesn't lock an
    /// earlier sweep's folder out.
    private struct Listing: Decodable {
        struct Scene: Decodable {
            struct Picture: Decodable {
                let file: String
            }

            let id: String
            let pictures: [Picture]
        }

        let scenes: [Scene]
    }

    /// Refuses `folder` unless a sweep may replace it: it's missing or empty, or it's one a sweep made,
    /// whose `manifest.json` reads as a listing of scenes and their pictures, and which holds nothing
    /// else but a folder for each of the scenes it lists, each with only that scene's pictures in it.
    /// Only the `.DS_Store` the Finder drops anywhere is overlooked, in it or in a scene's folder; any
    /// other hidden entry, such as a `.git`, counts. So a mistyped path can't lose what's there.
    static func check(_ folder: URL) throws {
        let fm = FileManager.default
        func isFolder(_ url: URL) -> Bool {
            var directory: ObjCBool = false
            return fm.fileExists(atPath: url.path, isDirectory: &directory) && directory.boolValue
        }
        func entries(_ url: URL) throws -> [String] {
            try fm.contentsOfDirectory(atPath: url.path).filter { $0 != ".DS_Store" }
        }
        guard fm.fileExists(atPath: folder.path) else { return }
        guard isFolder(folder) else { throw Pictures.PictureError.notASweep(folder.path) }
        let inside = try entries(folder)
        guard !inside.isEmpty else { return }
        let read = try? JSONDecoder().decode(Listing.self, from: Data(contentsOf: folder.appending(path: "manifest.json")))
        guard let listing = read else { throw Pictures.PictureError.notASweep(folder.path) }
        let scenes = Set(listing.scenes.map(\.id))
        let pictures = Set(listing.scenes.flatMap(\.pictures).map(\.file))
        for entry in inside where entry != "manifest.json" {
            let scene = folder.appending(path: entry)
            guard scenes.contains(entry), isFolder(scene),
                  try entries(scene).allSatisfy({ pictures.contains("\(entry)/\($0)") && !isFolder(scene.appending(path: $0)) })
            else { throw Pictures.PictureError.notASweep(folder.path) }
        }
    }

    /// The hidden folder beside `folder` that a sweep into it draws into, `.<name>.sweeping`, made
    /// afresh: one an earlier sweep left there is removed first, since it's a sweep's own by its name.
    static func sweeping(beside folder: URL) throws -> URL {
        let fm = FileManager.default
        let sweeping = folder.deletingLastPathComponent().appending(path: ".\(folder.lastPathComponent).sweeping")
        if fm.fileExists(atPath: sweeping.path) { try fm.removeItem(at: sweeping) }
        try fm.createDirectory(at: sweeping, withIntermediateDirectories: true)
        return sweeping
    }

    /// Puts the whole sweep in `sweeping` in `folder`'s place once `folder` is checked again, since it
    /// may have changed while the pictures were drawn: through `replaceItemAt` where there's an old
    /// one, so no moment leaves neither, and `moveItem` where there isn't. Nothing is left beside it.
    static func replace(_ folder: URL, with sweeping: URL) throws {
        let fm = FileManager.default
        try check(folder)
        if fm.fileExists(atPath: folder.path) {
            _ = try fm.replaceItemAt(folder, withItemAt: sweeping)
            if fm.fileExists(atPath: sweeping.path) { try fm.removeItem(at: sweeping) }
        } else {
            try fm.moveItem(at: sweeping, to: folder)
        }
    }
}
