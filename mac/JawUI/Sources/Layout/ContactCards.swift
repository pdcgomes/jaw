import CoreGraphics
import CoreText
import JawKit
import JawThemes

/// A contact message's cards in words, as its card says them, VoiceOver reads them and copying
/// gives them, from what JawKit read of each card as its row arrived.
public enum ContactWords {
    /// What the card is called: one card by the name its message gives it, failing that its own;
    /// several by their names.
    public static func title(_ c: Contact) -> String {
        guard c.cards.count > 1 else { return said(c.name) ?? said(c.cards.first?.name) ?? "Contact" }
        let names = c.cards.compactMap { said($0.name) }
        return names.isEmpty ? count(c) : names.joined(separator: ", ")
    }

    /// The most names the card of several shows on its line, and the most of each, in Unicode
    /// scalars: more than its line has room for, so a message of many, however long their names,
    /// costs its layout about what one of five does.
    static let namesShown = 5, nameShown = 64

    /// What the card in its message is called: as `title`, but of several only the first names, each
    /// cut short, with an ellipsis where any is left out.
    public static func titleShown(_ c: Contact) -> String {
        guard c.cards.count > 1 else { return title(c) }
        let names = Array(c.cards.lazy.compactMap { said($0.name) }.prefix(namesShown + 1))
        guard !names.isEmpty else { return count(c) }
        let shown = names.prefix(namesShown).map { name in
            name.unicodeScalars.count > nameShown ? String(String.UnicodeScalarView(name.unicodeScalars.prefix(nameShown))) + "…" : name
        }
        return (shown + (names.count > namesShown ? ["…"] : [])).joined(separator: ", ")
    }

    /// The line under it: one card's numbers as the card writes them; for several, what the message
    /// calls them, or how many they are.
    public static func detail(_ c: Contact) -> String {
        guard c.cards.count > 1 else {
            let numbers = c.cards.first?.phones.map(\.number) ?? []
            return numbers.isEmpty ? "Contact card" : numbers.joined(separator: " · ")
        }
        return said(c.name) ?? count(c)
    }

    /// What a click on the card does, as its foot says it: Message for a card with a number, View
    /// All for several, and nothing for a card without a number.
    public static func action(_ c: Contact) -> String? {
        if c.cards.count > 1 { return "View All" }
        return c.cards.first?.phones.isEmpty == false ? "Message" : nil
    }

    /// The cards as copying gives them, as the terminal draws them: one card's name, then its
    /// numbers; several's name, then each card's name and numbers, a line each.
    public static func lines(_ c: Contact) -> String {
        let numbers = { (card: ContactCard) in card.phones.map(\.number).joined(separator: ", ") }
        guard c.cards.count > 1 else {
            let first = c.cards.first.map(numbers) ?? ""
            return first.isEmpty ? title(c) : title(c) + "\n" + first
        }
        let each = c.cards.map { card in
            [said(card.name) ?? "someone", numbers(card)].filter { !$0.isEmpty }.joined(separator: " · ")
        }
        return ([said(c.name) ?? count(c)] + each).joined(separator: "\n")
    }

    /// What VoiceOver reads of it: a card's name and numbers, or how many and their names.
    public static func spoken(_ c: Contact) -> String {
        guard c.cards.count > 1 else { return "Contact card, " + title(c) + (action(c) == nil ? "" : ", " + detail(c)) }
        return count(c) + ": " + title(c)
    }

    static func count(_ c: Contact) -> String {
        "\(c.cards.count) contacts"
    }

    /// What a card's colour goes by, as a chat's does: the chat of its WhatsApp account, or its name.
    static func colourKey(_ card: ContactCard?, name: String) -> String {
        card?.phones.first { $0.whatsapp != nil }?.whatsapp.map { String($0.dropFirst()) + "@s.whatsapp.net" } ?? name
    }

    static func said(_ text: String?) -> String? {
        text.flatMap { $0.allSatisfy(\.isWhitespace) ? nil : $0 }
    }
}

/// A contact card in its message, as the Mac design draws one: its picture, the name it goes by and
/// its numbers, with Message across its foot; several cards their pictures overlapping, their names
/// and how many, with View All. Its size comes from the row alone, so a picture or none moves nothing.
extension MessageLayout.Builder {
    /// A card's picture, a circle, in points.
    static let cardPicture: CGFloat = 40
    /// How far each picture after the first is drawn from the one before, for several.
    static let cardStep: CGFloat = 20
    /// The most pictures a card of several draws.
    static let cardPictures = 3

    var cardTop: CGFloat {
        Space.s + Self.cardPicture + Space.s
    }

    var cardFoot: CGFloat {
        Stroke.hairline + lineHeight(fonts.body) + Space.l
    }

    func contactSize(most: CGFloat) -> CGSize {
        let foot = m.contact.flatMap(ContactWords.action) == nil ? 0 : cardFoot
        return CGSize(width: min(most, 280), height: cardTop + foot)
    }

    mutating func contact(_ r: CGRect) {
        let contact = m.contact ?? Contact(name: nil, cards: [])
        parts.append(.fill(r, well.cgColor, Radius.control))
        let shown: [ContactCard?] = contact.cards.count > 1 ? Array(contact.cards.prefix(Self.cardPictures)) : [contact.cards.first]
        let behind = well.over(style.look == .bubbles ? (m.mine ? roles.yours : roles.surface) : roles.ground)
        for (i, card) in shown.enumerated().reversed() {
            let circle = CGRect(x: r.minX + Space.m + CGFloat(i) * Self.cardStep, y: r.minY + Space.s,
                                width: Self.cardPicture, height: Self.cardPicture)
            if shown.count > 1 { parts.append(.fill(circle.insetBy(dx: -2, dy: -2), behind.cgColor, Self.cardPicture / 2 + 2)) }
            picture(of: card, title: shown.count > 1 ? nil : ContactWords.title(contact), in: circle)
        }
        let x = r.minX + Space.m + Self.cardPicture + CGFloat(shown.count - 1) * Self.cardStep + Space.m + 2
        let most = r.maxX - Space.m - x
        let words = lineHeight(fonts.name) + 2 + lineHeight(fonts.small)
        let top = r.minY + ((cardTop - words) / 2).rounded()
        text(ContactWords.titleShown(contact), fonts.name, roles.text, x: x, top: top, most: most)
        text(ContactWords.detail(contact), fonts.small, roles.secondary, x: x, top: top + lineHeight(fonts.name) + 2, most: most)
        if let action = ContactWords.action(contact) {
            let rule = r.minY + cardTop
            parts.append(.fill(CGRect(x: r.minX, y: rule, width: r.width, height: Stroke.hairline), roles.separator.cgColor, 0))
            let label = line(action, fonts.body, roles.accent)
            let w = width(of: label)
            parts.append(.line(label, CGPoint(x: (r.midX - w / 2).rounded(), y: rule + Stroke.hairline + Space.l / 2 + ascent(fonts.body))))
        }
        regions.append((r, .card))
    }

    /// A card's picture in `circle`: its photo, or its initials on its colour, as a chat's avatar
    /// without a picture shows them, or the person symbol when it has no name.
    mutating func picture(of card: ContactCard?, title: String?, in circle: CGRect) {
        if let image = card?.picture?.image {
            parts.append(.image(image, circle))
            return
        }
        let name = title ?? card?.name ?? ""
        let initials = Avatar.initials(name)
        let colour = roles.sender(Avatar.colour(ContactWords.colourKey(card, name: name)))
        parts.append(.fill(circle, colour.opacity(0.85).cgColor, circle.width / 2))
        guard !initials.isEmpty, initials.first?.isLetter == true else {
            symbol("person.fill", 18, roles.isDark ? roles.ground : RGBA(0xFFFFFF), centredIn: circle)
            return
        }
        let label = line(initials, fonts.initials, roles.isDark ? roles.ground : RGBA(0xFFFFFF))
        let w = width(of: label)
        parts.append(.line(label, CGPoint(x: (circle.midX - w / 2).rounded(),
                                          y: (circle.midY + (ascent(fonts.initials) - CTFontGetDescent(fonts.initials)) / 2).rounded())))
    }
}
