import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var palette: HarnessScene {
        HarnessScene(
            id: "palette", title: "Palette", symbol: "command",
            synopsis: "The palette over find's answer to \"ar\" from the protocol's fixture",
            section: .overlays, covers: [PaletteView.self], width: 700,
        ) { PaletteScene(roles: $0.roles) }
    }
}

/// The palette with "ar" typed and jawd's answer from `protocol/fixtures/find.result.json`.
private struct PaletteScene: View {
    let roles: Roles
    @State private var model: PaletteModel = {
        let model = PaletteModel()
        model.query = "ar"
        model.answered(id: 1, Fixtures.found)
        return model
    }()

    var body: some View {
        SpecimenGroup(
            title: "\"ar\" typed",
            note: "A chat, a person, a folder and a message, under the strip of counts (all, chats 1, people 2, folders 1, messages 214). The letters that matched are marked in each, each kind has its heading, and the first row is selected.",
            roles: roles,
        ) {
            PaletteView(model: model, roles: roles) {}
        }
    }
}
