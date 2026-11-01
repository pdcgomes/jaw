import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var libraryGrid: HarnessScene {
        HarnessScene(
            id: "library-grid", title: "Library grid", symbol: "square.grid.3x3",
            synopsis: "The library's photos and videos as a grid of jawd's 256-pixel previews under their months, newest first, one selected",
            section: .panes, covers: [LibraryController.self, LibraryLayout.self], axes: [.look], width: 560,
        ) { LibraryGridScene(conditions: $0) }
    }
}

extension HarnessScene {
    static var libraryRows: HarnessScene {
        HarnessScene(
            id: "library-rows", title: "Library rows", symbol: "list.bullet",
            synopsis: "Files, links and voice messages as the library's rows, each with who shared it, where and when, one file selected",
            section: .panes, covers: [LibraryController.self, LibraryLayout.self], axes: [.look], width: 560,
        ) { LibraryRowsScene(conditions: $0) }
    }

    static var libraryAll: HarnessScene {
        HarnessScene(
            id: "library-all", title: "Library, every kind", symbol: "square.stack",
            synopsis: "Every kind's window as a shelf of each: two lines of photos and videos, three files, three links and two voice messages, each headed by its kind and count",
            section: .panes, covers: [LibraryController.self, LibraryLayout.self], axes: [.look], width: 560,
        ) { LibraryAllScene(conditions: $0) }
    }
}

private struct LibraryRowsScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Files",
            note: "Each file's type in its symbol, its name, its type, size and pages, who shared it and in which chat, and when at its right; one not here says so first; a name too long for its row cut short, its date whole; the first selected. In bubbles each row is a card, in lines they're parted by hairlines.",
            roles: conditions.roles,
        ) {
            LibrarySpecimen(conditions: conditions, kind: .file, selected: "LF0", height: 340)
        }
        SpecimenGroup(
            title: "Links",
            note: "Each link's title, or its URL where it had none, its host in the accent, who and when; until jawd sends a link's thumbnail (P14) its well shows the link's mark.",
            roles: conditions.roles,
        ) {
            LibrarySpecimen(conditions: conditions, kind: .link, height: 290)
        }
        SpecimenGroup(
            title: "Voice messages",
            note: "▶ in the accent, the bars, the length, then who, where and when at the right: even bars until jawd sends a voice item's waveform (P14), as Inês's is; your own, with one, follows it.",
            roles: conditions.roles,
        ) {
            LibrarySpecimen(conditions: conditions, kind: .voice, height: 200)
        }
    }
}

private struct LibraryAllScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Every kind",
            note: "Photos and videos together, newest first, in two lines of tiles; then three files, three links and two voice messages, each shelf headed by its kind, jawd's count and ›, which goes to that kind. A shelf with nothing isn't drawn.",
            roles: conditions.roles,
        ) {
            LibrarySpecimen(conditions: conditions, kind: nil, height: 880)
        }
    }
}

/// A library pane of the made-up items, at `kind`, as tall as `height`, its pictures decoded as it
/// draws them, the item `selected` selected.
struct LibrarySpecimen: View {
    let conditions: Conditions
    var scope: LibraryScope = .every
    let kind: LibraryKind?
    var selected: String?
    let height: CGFloat

    var body: some View {
        ControllerSpecimen {
            let c = LibraryController(scope: scope, style: conditions.style, kind: kind)
            c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
            _ = c.view
            c.apply(Samples.libraryUpdate(of: Samples.library, kind: kind))
            if let selected { c.select(selected) }
            return c
        }
        .frame(width: 560, height: height)
    }
}

private struct LibraryGridScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Photos",
            note: "Tiles as wide as fit from 96 to 128 points, 2 apart, each filled from the centre from its 256 preview, under October, September and August 2026, newest first; one still coming shows its ring, one with nothing yet its well and its mark; the second selected, ringed in the accent. Nothing stretched, no tile overlapping its neighbour or its heading.",
            roles: conditions.roles,
        ) {
            LibrarySpecimen(conditions: conditions, kind: .photo, selected: "LP1", height: 720)
        }
        SpecimenGroup(
            title: "Videos",
            note: "Each video's poster with ▶ and its length on a pill at its foot: 0:42, 3:04 and a video message's 0:14.",
            roles: conditions.roles,
        ) {
            LibrarySpecimen(conditions: conditions, kind: .video, height: 360)
        }
    }
}
