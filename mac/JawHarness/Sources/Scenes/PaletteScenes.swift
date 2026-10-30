import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var palette: HarnessScene {
        HarnessScene(
            id: "palette", title: "Palette", symbol: "command",
            synopsis: "The palette over find's answer to \"ar\" from the protocol's fixture, and over links found",
            section: .overlays, covers: [PaletteView.self], width: 700,
        ) { PaletteScene(roles: $0.roles) }
    }
}

/// The palette with "ar" typed and jawd's answer from `protocol/fixtures/find.result.json`.
private struct PaletteScene: View {
    let roles: Roles
    @State private var model = Fixtures.palette()
    @State private var links = Fixtures.paletteLinks()

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "\"ar\" typed",
                note: "A chat, a person, a folder and a message, under the strip of counts (all, chats 1, people 2, folders 1, messages 214). The letters that matched are marked in each, each kind has its heading, and the first row is selected. The field selects what's in it when it opens with a query, so \"ar\" shows selected, where typing would leave the caret after it.",
                roles: roles,
            ) {
                PaletteView(model: model, roles: roles) {}
            }
            SpecimenGroup(
                title: "\"sintra\" typed, links found",
                note: "A chat, then the links jawd found under their own heading, each its title and host in jawd's words with what matched marked, and who sent it, where and when beside it; Enter opens it in the browser, and ⌘G goes to its message, as for a message found.",
                roles: roles,
            ) {
                PaletteView(model: links, roles: roles) {}
            }
        }
    }
}
