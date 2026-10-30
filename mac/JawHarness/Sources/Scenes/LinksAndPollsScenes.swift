import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var polls: HarnessScene {
        HarnessScene(
            id: "polls", title: "Polls", symbol: "chart.bar.xaxis",
            synopsis: "Polls with their options, tallies and how many voted, yours ticked, in either look",
            section: .conversation, covers: [ConversationController.self], axes: [.look], width: 680,
        ) { PollsScene(conditions: $0) }
    }

    static var linkPreviews: HarnessScene {
        HarnessScene(
            id: "link-previews", title: "Link previews", symbol: "link",
            synopsis: "Links' previews under their text, received and sent, with their thumbnails and without, in either look",
            section: .conversation, covers: [ConversationController.self], axes: [.look], width: 680,
        ) { LinkPreviewsScene(conditions: $0) }
    }
}

/// `Samples.polls` in Lisbon Climbing, as a pane 640 points wide shows them.
private struct PollsScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Polls in Lisbon Climbing",
            note: "Each poll's question, on three lines at most, then what a vote may choose (Choose one, Choose any, Choose up to 2), its options each with a tick, a circle for one choice and a square for several, filled in the accent where you chose, the count on the right and a bar for the option's share of those who voted, and how many voted at the foot: Inês's with your Saturday, Rui's of any with Quickdraws' bar full, Ana's with no votes yet and no bars, yours still on its way with its clock, and Rui's of up to two with its long question wrapped and its long options cut with an ellipsis.",
            roles: conditions.roles,
        ) {
            ControllerSpecimen {
                let c = ConversationController(chat: Samples.group,
                                               spec: LayoutSpec(style: conditions.style, me: ["person:1"], pictures: Samples.pictures),
                                               layouts: Layouts(), place: .newest(100))
                _ = c.view
                Pictures.apply(Samples.update(of: Samples.polls), to: c)
                return c
            }
            .frame(width: 640, height: 1220)
        }
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
