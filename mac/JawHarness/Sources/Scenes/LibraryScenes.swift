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
