import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var conversation: HarnessScene {
        HarnessScene(
            id: "conversation", title: "The conversation", symbol: "bubble.left.and.bubble.right",
            synopsis: "A group's newest messages as the conversation draws them, in either look, comfortable or compact",
            section: .conversation, covers: [ConversationController.self], axes: [.look, .density], width: 680,
        ) { ConversationScene(conditions: $0) }
    }
}

/// Lisbon Climbing's messages from `Samples`, opened at the newest, as a pane 640 points wide shows them,
/// tall enough to reach back past the cards to your edited, pending and failed messages.
private struct ConversationScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Lisbon Climbing, at the newest",
            note: "Your messages pending, sent, edited and failed with Retry, a photo with reactions, and a card for each kind jawd sends: a video, a document, a place, a poll, a contact, a link's preview, a voice message and an album of five; after the newest, Rui's typing bubble, its three dots still, as in a window that isn't key, and the dots alone in lines. Runs of one sender sit closer than messages between senders; in lines, yours start a third of the way across; no card overlaps the text around it.",
            roles: conditions.roles,
        ) {
            ControllerSpecimen {
                let c = ConversationController(chat: Samples.group, spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                               layouts: Layouts(), place: .newest(100))
                _ = c.view
                c.members = Fixtures.members
                DispatchQueue.main.async { c.apply(Samples.update) }
                return c
            }
            .frame(width: 640, height: 1900)
        }
    }
}
