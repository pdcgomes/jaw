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

extension HarnessScene {
    static var contactSheet: HarnessScene {
        HarnessScene(
            id: "contact-sheet", title: "A message's contact cards", symbol: "person.2.crop.square.stack",
            synopsis: "The sheet several cards open: each card's picture, name and numbers, and Message",
            section: .overlays, covers: [ContactCardsView.self], width: 480,
        ) { ContactSheetScene(conditions: $0) }
    }
}

extension HarnessScene {
    static var contactForm: HarnessScene {
        HarnessScene(
            id: "contact-form", title: "The contact form", symbol: "person.crop.circle.badge.plus",
            synopsis: "The box's Contact…: as it opens, with two cards to send, with one in the way, and turned down by jawd",
            section: .overlays, covers: [ContactFormView.self], width: 480,
        ) { ContactFormScene(conditions: $0) }
    }
}

/// The contact form to Ana, as it opens, ready, in the way and turned down.
private struct ContactFormScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            band("As it opens", "Nothing on it yet: Send a Contact, To Ana, and how Add a Contact… picks one; the system's picker shows under Add a Contact… at once, and isn't drawn here. Send is greyed, and nothing is said in the way.",
                 ContactForm(chat: "ana", to: "Ana"))
            band("Two cards", "Send 2 Contacts: Marta Lopes with both her numbers on a line and Inês Gomes with hers, each with initials on a colour of their own, as their message will draw them, no photo being sent; the first chosen, its row in the selection's colour, with × at the end of each; the keys said under them, and Send ready.",
                 ContactForm(chat: "ana", to: "Ana", cards: Samples.formCards))
            band("In the way", "A card past the 256 KiB jawd takes in one message, the two cards taking 301 KiB together, said in red under them, and Send greyed until it's taken out.",
                 ContactForm(chat: "ana", to: "Ana", cards: [Samples.formCards[0], Samples.formCard("Clínica Sorriso", ["+351 21 000 11 22"], note: 300 * 1024)]))
            band("Turned down", "jawd's words, after Not sent:, in red, the form opened again as it was, both cards on it and Send ready to try again.",
                 ContactForm(chat: "ana", to: "Ana", cards: Samples.formCards, said: "40 wait to send, 2 failed; it can go once some have"))
        }
    }

    func band(_ title: String, _ note: String, _ form: ContactForm) -> some View {
        SpecimenGroup(title: title, note: note, roles: conditions.roles) {
            ContactFormView(form: form, roles: conditions.roles, pick: { _ in }, send: {}, cancel: {})
        }
    }
}

/// The sheet of Rui's five cards, and of Inês's two.
private struct ContactSheetScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Five cards, and two",
            note: "Each card in a row of its own: Tiago's and Mãe's photos and the others' initials on their colours, their names, each number with WhatsApp beside the account's, and Message; the first card chosen, its row in the selection's colour; ↑↓, ↩ and esc said at the foot beside Done.",
            roles: conditions.roles,
        ) {
            ForEach(Array(Samples.contacts.filter { ($0.contact?.cards.count ?? 0) > 1 }.reversed().enumerated()), id: \.offset) { _, m in
                if let contact = m.contact {
                    ContactCardsView(contact: contact, from: "From \(m.from?.name ?? "someone")", roles: conditions.roles,
                                     message: { _, _ in }, done: {})
                }
            }
        }
    }
}

/// The group's contact cards from `Samples`, as a pane 640 points wide shows them.
private struct ContactCardsScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Cards, one and several",
            note: "Rui's card of Marta Lopes with her photo, round, her two numbers and Message across its foot; Ana's of Inês Gomes, her initials on her colour; two cards and five as their names and how many, three pictures overlapping, initials only on the front one and the person symbol behind it where a card has no photo, with View All; a card without a number, saying it's a contact card, with no foot; a long name and three numbers, cut with an ellipsis; yours on its way; and a deleted one. Every card is the same height for the same kind whether it has a photo or not, and in lines it sits at the message's indent.",
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
