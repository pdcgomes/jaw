import AppKit
import JawKit
import JawThemes
import Observation
import SwiftUI

/// What Message on a contact card does, by what's on it, as the terminal's Enter does: one number
/// starts a chat with it, several are offered to choose from, none is said, and several cards open
/// the sheet of them.
public enum CardAction: Equatable, Sendable {
    case message(ContactCard.Phone, name: String)
    case choose([ContactCard.Phone], name: String)
    case nothing
    case cards

    public static func of(_ m: MessageRow) -> CardAction {
        guard m.kind == .contact, !m.deleted, let contact = m.contact else { return .nothing }
        if contact.cards.count > 1 { return .cards }
        let numbers = contact.cards.first.map(numbers) ?? []
        let name = ContactWords.title(contact)
        switch numbers.count {
        case 0: return .nothing
        case 1: return .message(numbers[0], name: name)
        default: return .choose(numbers, name: name)
        }
    }

    /// A card's numbers, each once by the number it dials.
    public static func numbers(_ card: ContactCard) -> [ContactCard.Phone] {
        var out: [ContactCard.Phone] = []
        for phone in card.phones where !out.contains(where: { $0.dialled == phone.dialled }) {
            out.append(phone)
        }
        return out
    }

    /// A number as a menu or the sheet says it: as the card writes it, the WhatsApp account's marked.
    public static func label(_ phone: ContactCard.Phone) -> String {
        phone.whatsapp == nil ? phone.number : phone.number + " · WhatsApp"
    }

    /// What's said for a card without a number, in the terminal's words.
    public static let noNumber = "there's no number on this card to start a chat with"
}

/// Which of a message's cards the sheet has chosen, and what Return on it messages: its number on
/// WhatsApp, or failing that its first.
@MainActor @Observable
public final class ContactCardsSheet {
    public let cards: [ContactCard]
    public private(set) var selected = 0

    public init(cards: [ContactCard]) {
        self.cards = cards
    }

    public func up() {
        selected = max(selected - 1, 0)
    }

    public func down() {
        selected = min(selected + 1, max(cards.count - 1, 0))
    }

    public func choose(_ i: Int) {
        if cards.indices.contains(i) { selected = i }
    }

    /// The chosen card's number to dial and its name, for Return; none when it has no number.
    public func returned() -> (phone: String, name: String)? {
        guard cards.indices.contains(selected) else { return nil }
        let card = cards[selected]
        let numbers = CardAction.numbers(card)
        guard let phone = numbers.first(where: { $0.whatsapp != nil }) ?? numbers.first else { return nil }
        return (phone.dialled, ContactWords.said(card.name) ?? phone.number)
    }
}

/// The sheet of a message's cards, which the design opens for several: each card's picture, name and
/// numbers, the WhatsApp account's marked, and Message, a menu of its numbers when it has several.
/// ↑ and ↓ choose a card, Return messages it, and Esc closes.
public struct ContactCardsView: View {
    let contact: Contact
    let from: String
    let roles: Roles
    let message: (String, String) -> Void
    let done: () -> Void
    @State var sheet: ContactCardsSheet
    @FocusState var focused: Bool

    /// `message` is given the number to dial and the card's name.
    public init(contact: Contact, from: String, roles: Roles, message: @escaping (String, String) -> Void,
                done: @escaping () -> Void)
    {
        self.contact = contact
        self.from = from
        self.roles = roles
        self.message = message
        self.done = done
        _sheet = State(initialValue: ContactCardsSheet(cards: contact.cards))
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.l) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(ContactWords.detail(contact)).textStyle(.heading).tone(.primary, roles)
                Text(from).textStyle(.callout).tone(.secondary, roles)
            }
            if contact.cards.count > Self.unscrolled {
                ScrollViewReader { proxy in
                    ScrollView { rows }
                        .onChange(of: sheet.selected) { proxy.scrollTo(sheet.selected) }
                }
                .frame(height: 400)
            } else {
                rows
            }
            HStack {
                Text("↑↓ choose · ↩ message · esc closes").textStyle(.caption).tone(.tertiary, roles)
                Spacer()
                Button("Done", action: done).keyboardShortcut(.cancelAction)
            }
        }
        .padding(Space.xl)
        .frame(width: 440)
        .background(roles.ground.color)
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onAppear { focused = true }
        .onKeyPress(.upArrow) {
            sheet.up()
            return .handled
        }
        .onKeyPress(.downArrow) {
            sheet.down()
            return .handled
        }
        .onKeyPress(.return) {
            if let (phone, name) = sheet.returned() { message(phone, name) }
            return .handled
        }
    }

    /// The most cards shown without scrolling.
    static let unscrolled = 6

    var rows: some View {
        VStack(spacing: Space.xs) {
            ForEach(Array(contact.cards.enumerated()), id: \.offset) { i, card in
                CardRow(card: card, roles: roles, selected: i == sheet.selected, message: message)
                    .id(i)
                    .contentShape(Rectangle())
                    .onTapGesture { sheet.choose(i) }
            }
        }
    }
}

/// One card in the sheet: its picture, name and numbers, and Message.
struct CardRow: View {
    let card: ContactCard
    let roles: Roles
    let selected: Bool
    let message: (String, String) -> Void

    var name: String {
        ContactWords.said(card.name) ?? "Someone"
    }

    var body: some View {
        HStack(alignment: .center, spacing: Space.m) {
            CardPictureView(card: card, name: name, roles: roles, side: 36)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(name).textStyle(.body).fontWeight(.semibold).tone(.primary, roles).lineLimit(1)
                let numbers = CardAction.numbers(card)
                if numbers.isEmpty {
                    Text("No number").textStyle(.callout).tone(.tertiary, roles)
                }
                ForEach(numbers, id: \.dialled) { phone in
                    Text(CardAction.label(phone)).textStyle(.callout).tone(.secondary, roles).lineLimit(1)
                }
            }
            Spacer(minLength: Space.m)
            messageButton
        }
        .padding(Space.s)
        .card(roles, selected: selected)
    }

    @ViewBuilder var messageButton: some View {
        let numbers = CardAction.numbers(card)
        if numbers.count > 1 {
            Menu("Message") {
                ForEach(numbers, id: \.dialled) { phone in
                    Button(CardAction.label(phone)) { message(phone.dialled, name) }
                }
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
        } else {
            Button("Message") {
                if let phone = numbers.first { message(phone.dialled, name) }
            }
            .disabled(numbers.isEmpty)
        }
    }
}

/// A card's picture as a view: its photo, round, or its initials on its colour, as the card in its
/// message draws it.
public struct CardPictureView: View {
    let card: ContactCard?
    let name: String
    let roles: Roles
    let side: CGFloat

    public init(card: ContactCard?, name: String, roles: Roles, side: CGFloat) {
        self.card = card
        self.name = name
        self.roles = roles
        self.side = side
    }

    public var body: some View {
        if let image = card?.picture?.image {
            Image(decorative: image, scale: CGFloat(image.width) / side)
                .frame(width: side, height: side)
        } else {
            let initials = Avatar.initials(name)
            ZStack {
                Circle().fill(roles.sender(Avatar.colour(ContactWords.colourKey(card, name: name))).opacity(0.85).color)
                if initials.first?.isLetter == true {
                    Text(initials).textStyle(.callout).fontWeight(.semibold)
                        .foregroundStyle((roles.isDark ? roles.ground : RGBA(0xFFFFFF)).color)
                } else {
                    Image(systemName: "person.fill").foregroundStyle((roles.isDark ? roles.ground : RGBA(0xFFFFFF)).color)
                }
            }
            .frame(width: side, height: side)
        }
    }
}
