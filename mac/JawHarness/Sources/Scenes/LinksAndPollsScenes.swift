import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var linkPreviews: HarnessScene {
        HarnessScene(
            id: "link-previews", title: "Link previews", symbol: "link",
            synopsis: "Links' previews under their text, received and sent, with their thumbnails and without, in either look",
            section: .conversation, covers: [ConversationController.self], axes: [.look], width: 680,
        ) { LinkPreviewsScene(conditions: $0) }
    }
}

/// `Samples.links` in Lisbon Climbing, as a pane 640 points wide shows them, their thumbnails drawn by
/// `Samples.pictures` as the app reads jawd's from its folder.
private struct LinkPreviewsScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Links in Lisbon Climbing",
            note: "Each preview sits under its message's text, inside the bubble in bubbles and at the message's indent in lines: Ana's with its thumbnail on the left, Rui's from history without one, so the accent's bar is there instead, yours with its thumbnail and a description cut after two lines with an ellipsis, Inês's with a title and its host alone, and your last with no preview, its link only in the text. Each card's host has no www.; in bubbles the time sits under the card, never on it, and in lines at the right edge, as for any message.",
            roles: conditions.roles,
        ) {
            ControllerSpecimen {
                let c = ConversationController(chat: Samples.group,
                                               spec: LayoutSpec(style: conditions.style, me: ["person:1"], pictures: Samples.pictures),
                                               layouts: Layouts(), place: .newest(100))
                _ = c.view
                Pictures.apply(Samples.update(of: Samples.links), to: c)
                return c
            }
            .frame(width: 640, height: 680)
        }
    }
}
