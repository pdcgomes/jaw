import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var contactCards: HarnessScene {
        HarnessScene(
            id: "contact-cards", title: "Contact cards", symbol: "person.crop.circle",
            synopsis: "Contact cards in their bubbles, one or several, with a photo or initials, in either look",
            section: .conversation, covers: [ConversationController.self], axes: [.look], width: 680,
        ) { ContactCardsScene(conditions: $0) }
    }
}

/// The group's contact cards from `Samples`, as a pane 640 points wide shows them.
private struct ContactCardsScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Cards, one and several",
            note: "Rui's card of Marta Lopes with her photo, round, her two numbers and Message across its foot; Ana's of Inês Gomes, her initials on her colour; two cards and five as their names and how many, three pictures overlapping, with View All; a card without a number, saying it's a contact card, with no foot; a long name and three numbers, cut with an ellipsis; yours on its way; and a deleted one. Every card is the same height for the same kind whether it has a photo or not, and in lines it sits at the message's indent.",
            roles: conditions.roles,
        ) {
            ControllerSpecimen {
                let c = ConversationController(chat: Samples.group, spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                               layouts: Layouts(), place: .newest(100))
                c.typing = Samples.typing[Samples.group.id] ?? []
                _ = c.view
                c.members = Fixtures.members
                Pictures.apply(Samples.contactsUpdate, to: c)
                return c
            }
            .frame(width: 640, height: 1180)
        }
    }
}
