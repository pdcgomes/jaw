import AppKit
import Contacts
@testable import JawKit
import JawThemes
@testable import JawUI
import SwiftUI
import Testing

/// The contact form: a contact's card as Contacts exports it, without its photo, named as Contacts
/// names it; what's in the way of sending, counted as jawd counts; choosing and taking cards out; a
/// form jawd turns down opening again as it was; the card it sends drawn as yours in both looks once
/// jawd's row has it; and the form in each of its states drawn as its reference, dark and light.

/// A made-up contact, which has every key a vCard needs, as one Contacts fetches for it does.
func madeUp(_ given: String, _ family: String, numbers: [String], email: String? = nil, photo: Bool = false) -> CNMutableContact {
    let c = CNMutableContact()
    c.givenName = given
    c.familyName = family
    c.phoneNumbers = numbers.enumerated().map { i, n in
        CNLabeledValue(label: i == 0 ? CNLabelPhoneNumberMobile : CNLabelWork, value: CNPhoneNumber(stringValue: n))
    }
    if let email { c.emailAddresses = [CNLabeledValue(label: CNLabelWork, value: email as NSString)] }
    if photo { c.imageData = Samples.photo(0x7DCFFF) }
    return c
}

let refusedWords = "40 wait to send, 2 failed; it can go once some have"

@Test func `a contact's card is one vCard 3.0 of its name, numbers and email, without its photo, read as its message will draw it`() throws {
    let marta = madeUp("Marta", "Lopes", numbers: ["+351 213 456 789", "21 000 00 00"], email: "marta@sorriso.pt", photo: true)
    #expect(marta.imageData != nil)
    let card = try #require(ContactForm.card(marta))
    #expect(card.sent.name == "Marta Lopes")
    let vcard = card.sent.vcard
    #expect(vcard.hasPrefix("BEGIN:VCARD") && vcard.trimmingCharacters(in: .whitespacesAndNewlines).hasSuffix("END:VCARD"))
    #expect(vcard.components(separatedBy: "BEGIN:VCARD").count == 2, "one card")
    #expect(vcard.contains("VERSION:3.0") && vcard.contains("marta@sorriso.pt"))
    #expect(!vcard.uppercased().contains("PHOTO"), "its photo left out")
    #expect(card.shown.name == "Marta Lopes" && card.shown.phones.map(\.number) == ["+351 213 456 789", "21 000 00 00"])
    #expect(card.shown.picture == nil && card.name == "Marta Lopes")
    #expect(SendContact.refusal([card.sent]) == nil, "jawd takes it")
}

@Test func `a contact with only an organisation is named by it, one with only a number by its number, and one with nothing makes no card`() {
    let clinic = CNMutableContact()
    clinic.contactType = .organization
    clinic.organizationName = "Clínica Sorriso"
    clinic.phoneNumbers = [CNLabeledValue(label: CNLabelWork, value: CNPhoneNumber(stringValue: "+351 21 000 11 22"))]
    #expect(ContactForm.card(clinic)?.sent.name == "Clínica Sorriso")
    let number = CNMutableContact()
    number.phoneNumbers = [CNLabeledValue(label: CNLabelPhoneNumberMobile, value: CNPhoneNumber(stringValue: "+351 912 000 000"))]
    #expect(ContactForm.card(number)?.sent.name == "+351 912 000 000")
    #expect(ContactForm.made(CNMutableContact()) == .unadded("that contact has no name, number or email to send"))
}

@MainActor @Test func `what a contact picked makes lands on the form: its card, or why there's none`() {
    let form = ContactForm(chat: "ana", to: "Ana")
    form.land(.card(Samples.formCards[0]), pickedAfter: form.closings)
    #expect(form.cards == [Samples.formCards[0]] && form.warning == nil)
    form.land(.unadded("that contact has no name, number or email to send"), pickedAfter: form.closings)
    #expect(form.warning == "that contact has no name, number or email to send" && form.cards.count == 1)
    form.land(.unfetched, pickedAfter: form.closings)
    #expect(form.warning == "Contacts gave Jaw less of that contact than its card needs")
    #expect(!["that contact has no name, number or email to send", form.warning].contains(ContactForm.unwritten),
            "Contacts failing to write a card says so in words of its own")
}

@MainActor @Test func `a contact picked lands only on the form still open as it was picked, not once it's sent or cancelled, nor when it opens again`() throws {
    let form = ContactForm(chat: "ana", to: "Ana", cards: [Samples.formCards[0]])
    let picked = form.closings
    form.closed()
    form.land(.card(Samples.formCards[1]), pickedAfter: picked)
    form.land(.unadded("Jaw isn't allowed to see your contacts"), pickedAfter: picked)
    #expect(form.cards == [Samples.formCards[0]] && form.unadded == nil, "made after Send or Cancel, it's let go")
    let sent = ContactFormsSent()
    sent.going(form, key: "jaw-1")
    let again = try #require(sent.refused("jaw-1", why: refusedWords))
    again.land(.card(Samples.formCards[1]), pickedAfter: picked)
    #expect(again.cards == [Samples.formCards[0]], "turned down, it opens again with the cards sent, not one picked before")
    again.land(.card(Samples.formCards[1]), pickedAfter: again.closings)
    #expect(again.cards == Samples.formCards, "and one picked since lands")
}

@MainActor @Test func `the form says what's in the way as jawd counts it, and is ready only with cards to send`() {
    let form = ContactForm(chat: "ana", to: "Ana")
    #expect(!form.ready && form.problem == nil && form.warning == nil && form.picksAtOnce && form.title == "Send a Contact")
    form.add(Samples.formCards[0])
    #expect(form.ready && form.title == "Send a Contact" && !form.picksAtOnce)
    form.add(Samples.formCards[1])
    #expect(form.ready && form.title == "Send 2 Contacts" && form.sending == Samples.formCards.map(\.sent))

    let many = ContactForm(chat: "ana", to: "Ana", cards: (1 ... 65).map { Samples.formCard("Contact \($0)", ["+351 910 000 \(100 + $0)"]) })
    #expect(!many.ready && many.problem == "one message shares 64 cards at most: take 1 out")
    many.remove(64)
    #expect(many.ready && many.problem == nil)

    let big = ContactForm(chat: "ana", to: "Ana", cards: [Samples.formCards[0], Samples.formCard("Clínica Sorriso", ["+351 21 000 11 22"], note: 300 * 1024)])
    #expect(!big.ready && big.problem == "these cards take 301 KiB together, and one message takes 256 KiB at most")
    #expect(big.warning == big.problem)

    let long = ContactForm(chat: "ana", to: "Ana", cards: [Samples.formCard(String(repeating: "Maria ", count: 43) + "Sá", ["+351 912 345 678"])])
    #expect(!long.ready && long.problem == "Maria Maria Maria Maria…'s name is 260 characters, and a card's is 256 at most")
}

@MainActor @Test func `a card the form has already isn't added again, and adding or taking one out clears jawd's words`() {
    let form = ContactForm(chat: "ana", to: "Ana", cards: [Samples.formCards[0]], said: refusedWords)
    #expect(form.warning == "Not sent: " + refusedWords && form.ready && !form.picksAtOnce)
    form.add(Samples.formCards[0])
    #expect(form.cards.count == 1 && form.warning == "Marta Lopes is on the form already")
    form.add(Samples.formCards[1])
    #expect(form.cards.count == 2 && form.said == nil && form.unadded == nil && form.warning == nil && form.chosen == 1)
    let again = ContactForm(chat: "ana", to: "Ana", cards: Samples.formCards, said: refusedWords)
    again.remove(0)
    #expect(again.said == nil && again.cards == [Samples.formCards[1]])
}

@MainActor @Test func `↑ and ↓ choose a card, and ⌫ takes the chosen one out`() {
    let form = ContactForm(chat: "ana", to: "Ana", cards: Samples.formCards + [Samples.formCard("Rui Costa", ["+351 914 000 000"])])
    #expect(form.chosen == 0)
    form.up()
    #expect(form.chosen == 0, "no further than the first")
    for _ in 0 ..< 4 {
        form.down()
    }
    #expect(form.chosen == 2, "no further than the last")
    form.up()
    form.remove(form.chosen)
    #expect(form.cards.map(\.name) == ["Marta Lopes", "Rui Costa"] && form.chosen == 1)
    form.remove(form.chosen)
    #expect(form.chosen == 0)
    form.remove(form.chosen)
    #expect(form.cards.isEmpty && form.chosen == 0 && !form.ready)
    form.remove(form.chosen)
    #expect(form.cards.isEmpty, "nothing to take out")
}

@MainActor @Test func `a form jawd turns down opens again as it was with its words, and one it takes is let go`() {
    let sent = ContactFormsSent()
    let form = ContactForm(chat: "ana", to: "Ana", cards: Samples.formCards)
    form.unadded = "Inês Gomes is on the form already"
    sent.going(form, key: "jaw-1")
    let again = sent.refused("jaw-1", why: refusedWords)
    #expect(again === form && again?.cards == Samples.formCards && again?.chat == "ana")
    #expect(again?.warning == "Not sent: " + refusedWords && again?.picksAtOnce == false && again?.ready == true)
    #expect(sent.refused("jaw-1", why: "again") == nil, "once")
    sent.going(form, key: "jaw-2")
    sent.taken("jaw-2")
    #expect(sent.refused("jaw-2", why: "late") == nil, "taken by jawd")
}

@Test func `a card the form sends is drawn as yours with the name and numbers the form showed, once jawd's row has it, in both looks`() throws {
    let card = Samples.formCards[0]
    let row = try #require(Samples.contactRow(20, at: Samples.now, mine: true, status: "pending", name: card.sent.name, vcard: card.sent.vcard))
    let contact = try #require(row.contact)
    #expect(contact.cards == [card.shown])
    #expect(ContactWords.title(contact) == card.name && ContactWords.detail(contact) == "+351 213 456 789 · 21 000 00 00")
    for look in Look.allCases {
        let b = cardBuilder(row, look)
        let r = try #require(cardRect(cardLaid(row, look)), "\(look)")
        #expect(r.height == b.cardTop + b.cardFoot, "\(look): Message across its foot")
        #expect(look == .lines ? r.minX == floor(640 / 3) : r.minX > 640 / 3, "\(look): where yours go")
    }
    let both = try #require(Samples.contactRow(21, at: Samples.now, mine: true, name: "2 contacts", vcard: Samples.formCards.map(\.sent.vcard).joined()))
    #expect(both.contact?.cards == Samples.formCards.map(\.shown))
    #expect(try ContactWords.title(#require(both.contact)) == "Marta Lopes, Inês Gomes")
    #expect(try ContactWords.action(#require(both.contact)) == "View All")
}

@MainActor @Test func `the form draws as its reference in each of its states, dark and light`() async throws {
    for dark in [true, false] {
        let drawn = try await renderForm(dark: dark)
        try compare(#require(drawn), "contact-form-\(dark ? "dark" : "light")")
    }
}

/// The form as it opens, with two cards, with one in the way and turned down by jawd, one under
/// another, each as its sheet draws it, at 2x, off the screen.
@MainActor
func renderForm(dark: Bool) async throws -> CGImage? {
    let roles = Catalog.jaw.roles(dark: dark)
    let forms = [
        ContactForm(chat: "ana", to: "Ana"),
        ContactForm(chat: "ana", to: "Ana", cards: Samples.formCards),
        ContactForm(chat: "ana", to: "Ana", cards: [Samples.formCards[0], Samples.formCard("Clínica Sorriso", ["+351 21 000 11 22"], note: 300 * 1024)]),
        ContactForm(chat: "ana", to: "Ana", cards: Samples.formCards, said: refusedWords),
    ]
    let host = NSHostingView(rootView: VStack(spacing: 0) {
        ForEach(forms.indices, id: \.self) { i in
            ContactFormView(form: forms[i], roles: roles, pick: { _ in }, send: {}, cancel: {})
        }
    })
    let window = NSWindow(contentRect: NSRect(origin: NSPoint(x: -30000, y: -30000), size: host.fittingSize), styleMask: [.borderless],
                          backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.animationBehavior = .none
    window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
    window.contentView = host
    window.orderFront(nil)
    defer {
        window.close()
        window.contentView = nil
    }
    try await Task.sleep(for: .milliseconds(400))
    host.layoutSubtreeIfNeeded()
    let bounds = host.bounds
    let (wide, high) = (Int(bounds.width * 2), Int(bounds.height * 2))
    guard wide > 0, high > 0,
          let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: wide, pixelsHigh: high, bitsPerSample: 8, samplesPerPixel: 4,
                                     hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: wide * 4, bitsPerPixel: 32)
    else { return nil }
    rep.size = bounds.size
    host.cacheDisplay(in: bounds, to: rep)
    return rep.cgImage
}
