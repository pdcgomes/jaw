import AppKit
import CoreGraphics
import CoreText
@testable import JawKit
import JawThemes
@testable import JawUI
import SwiftUI
import Testing

/// A contact card in its message: its picture, name and numbers, Message or View All across its
/// foot and the whole card its region, the same height with a photo or without, in both looks; its
/// words for VoiceOver and for copying; a hostile card laid out from what JawKit read of it; and the
/// samples drawn as their references.

let cardContext = MessageContext(group: true, firstOfRun: true, showTime: true, day: nil)

func cardBuilder(_ m: MessageRow, _ look: Look = .bubbles) -> MessageLayout.Builder {
    let s = spec(look, dark: true)
    return MessageLayout.Builder(m: m, width: 640, context: cardContext, style: s.style, fonts: s.fonts, me: s.me)
}

func cardLaid(_ m: MessageRow, _ look: Look = .bubbles) -> Laid {
    let s = spec(look, dark: true)
    return MessageLayout.lay(m, width: 640, context: cardContext, style: s.style, fonts: s.fonts, me: s.me)
}

/// The sample card numbered `n`.
func sampleCard(_ n: Int) throws -> MessageRow {
    try #require(Samples.contacts.first { $0.id == String(format: "3EB0C0%010d", n) })
}

/// Where a laid-out message's card is, by its region.
func cardRect(_ l: Laid) -> CGRect? {
    l.regions.first { $0.1 == .card }?.0
}

func images(_ l: Laid) -> [CGRect] {
    l.parts.compactMap { if case let .image(_, r) = $0 { r } else { nil } }
}

@Test func `the samples' cards read as jawd sends them, photos and all`() throws {
    #expect(Samples.contacts.count == 8)
    #expect(try sampleCard(1).contact?.cards.first?.picture != nil && sampleCard(2).contact?.cards.first?.picture == nil)
    let five = try #require(sampleCard(4).contact)
    #expect(five.cards.count == 5 && five.cards.map { $0.picture != nil } == [true, false, true, false, false])
}

@Test func `a card has its photo, its name and numbers, and Message across its foot, the whole card its region`() throws {
    let marta = try sampleCard(1)
    let l = cardLaid(marta)
    let r = try #require(cardRect(l))
    let b = cardBuilder(marta)
    #expect(r.size == b.contactSize(most: 640) && r.height == b.cardTop + b.cardFoot)
    #expect(images(l).contains(CGRect(x: r.minX + Space.m, y: r.minY + Space.s, width: 40, height: 40)), "the photo in its circle")
    let contact = try #require(marta.contact)
    #expect(ContactWords.title(contact) == "Marta Lopes")
    #expect(ContactWords.detail(contact) == "+351 21 345 6789 · 21 000 00 00")
    #expect(ContactWords.action(contact) == "Message")

    let ines = try sampleCard(2)
    let other = cardLaid(ines)
    let s = try #require(cardRect(other))
    #expect(s.height == r.height, "the same height without a photo")
    #expect(!images(other).contains { $0.intersects(CGRect(x: s.minX + Space.m, y: s.minY + Space.s, width: 40, height: 40)) },
            "her initials, drawn as text on her colour")
    #expect(other.parts.contains { if case let .fill(c, _, radius) = $0 { c.width == 40 && radius == 20 } else { false } })
}

@Test func `several cards show up to three pictures overlapping, their names and how many, with View All`() throws {
    let two = try sampleCard(3), five = try sampleCard(4)
    #expect(try ContactWords.title(#require(two.contact)) == "Tiago Martins, Sofia Costa")
    #expect(try ContactWords.detail(#require(two.contact)) == "2 contacts")
    #expect(try ContactWords.action(#require(five.contact)) == "View All")
    let l = cardLaid(five)
    let r = try #require(cardRect(l))
    let one = try #require(cardRect(cardLaid(sampleCard(1))))
    #expect(r.height == one.height, "as tall as one card")
    let circles = images(l).filter { $0.width == 40 }
    #expect(circles.map(\.minX).sorted() == [r.minX + Space.m, r.minX + Space.m + 40], "Tiago's photo and Mãe's, the first and third of three")
    let rings = l.parts.filter { if case let .fill(c, _, _) = $0 { c.width == 44 } else { false } }
    #expect(rings.count == 3, "each picture of several ringed, three of five")

    let both = cardLaid(two)
    let t = try #require(cardRect(both))
    let front = CGRect(x: t.minX + Space.m, y: t.minY + Space.s, width: 40, height: 40), behind = front.offsetBy(dx: 20, dy: 0)
    let initials = both.parts.filter { if case let .line(_, p) = $0 { front.union(behind).contains(p) } else { false } }
    #expect(initials.count == 1, "Tiago's initials on the front picture, and none behind it")
    #expect(images(both).contains { abs($0.midX - behind.midX) < 1 && $0.width < 40 }, "Sofia's the person symbol")
}

@Test func `a card without a number says it's a contact card and has no foot, and a deleted one has no card`() throws {
    let school = try sampleCard(5)
    let contact = try #require(school.contact)
    #expect(ContactWords.detail(contact) == "Contact card" && ContactWords.action(contact) == nil)
    #expect(cardRect(cardLaid(school))?.height == cardBuilder(school).cardTop)
    let deleted = try sampleCard(8)
    #expect(cardRect(cardLaid(deleted)) == nil)
    #expect(cardBuilder(deleted).words().string == "This message was deleted")
}

@Test func `a long name and many numbers are cut to the card, in both looks`() throws {
    let long = try sampleCard(6)
    for look in Look.allCases {
        let l = cardLaid(long, look)
        let r = try #require(cardRect(l))
        for case let .line(line, point) in l.parts where r.contains(point) {
            let w = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
            #expect(point.x + w <= r.maxX - Space.m + 1, "\(look): a line ends at \(point.x + w), the card at \(r.maxX)")
        }
    }
}

@Test func `in lines a card sits at the message's indent, and yours a third of the way across`() throws {
    let theirs = try #require(cardRect(cardLaid(sampleCard(1), .lines)))
    #expect(theirs.minX == 16)
    let yours = try #require(cardRect(cardLaid(sampleCard(7), .lines)))
    #expect(yours.minX == floor(640 / 3))
}

@MainActor @Test func `VoiceOver reads a card as its name and numbers, and several as how many and their names`() throws {
    #expect(try ContactWords.spoken(#require(sampleCard(1).contact)) == "Contact card, Marta Lopes, +351 21 345 6789 · 21 000 00 00")
    #expect(try ContactWords.spoken(#require(sampleCard(3).contact)) == "2 contacts: Tiago Martins, Sofia Costa")
    #expect(try ContactWords.spoken(#require(sampleCard(5).contact)) == "Contact card, Escola de Escalada")
    let view = MessageView(frame: .zero)
    view.message = try sampleCard(1)
    #expect(view.accessibilityLabel()?.hasPrefix("Rui, Contact card, Marta Lopes, +351 21 345 6789") == true)
}

@Test func `copying a card gives its lines as the terminal draws them`() throws {
    #expect(try ContactWords.lines(#require(sampleCard(1).contact)) == "Marta Lopes\n+351 21 345 6789, 21 000 00 00")
    #expect(try ContactWords.lines(#require(sampleCard(3).contact)) == "2 contacts\nTiago Martins · +351 916 666 666\nSofia Costa · +351 917 777 777")
    #expect(try ContactWords.lines(#require(sampleCard(5).contact)) == "Escola de Escalada")
}

/// The fastest of three runs of `run`.
func fastest(_ run: () -> Void) -> Duration {
    let clock = ContinuousClock()
    return (0 ..< 3).map { _ in clock.measure(run) }.min()!
}

@Test func `a card read from a hostile vCard is laid out from what was read, in a fraction of the time reading it takes`() throws {
    // One card with a note of 4 MiB broken softly after each character; and as many cards as are
    // read, each named in 128 skin-toned thumbs, 256 scalars, the longest name read, with as many
    // numbers as are read.
    let soft = "BEGIN:VCARD\r\nVERSION:2.1\r\nFN:Ana Ribeiro\r\nTEL;waid=351912345678:+351 912 345 678\r\nNOTE;ENCODING=QUOTED-PRINTABLE:"
        + String(repeating: "x=\r\n", count: 1 << 20) + "x\r\nEND:VCARD\r\n"
    let thumbs = String(repeating: "👍🏽", count: 128)
    let named = (0 ..< 256).map { i in
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:\(thumbs)\r\n"
            + (0 ..< 16).map { String(format: "TEL;type=CELL;waid=3519%04d%04d:+351 9%04d %04d\r\n", i, $0, i, $0) }.joined() + "END:VCARD\r\n"
    }.joined()
    let one = try #require(Samples.contactRow(9, at: Samples.now, from: Samples.ana, name: "Ana Ribeiro", vcard: soft))
    #expect(one.contact?.cards.first?.phones.first?.dialled == "+351912345678", "the card read as its row was")
    let many = try #require(Samples.contactRow(10, at: Samples.now, from: Samples.ana, name: "256 contacts", vcard: named))
    let contact = try #require(many.contact)
    #expect(contact.cards.count == 256 && contact.cards.allSatisfy { $0.name == thumbs && $0.phones.count == 16 }, "every card read, whole")
    let cut = String(repeating: "👍🏽", count: 32) + "…"
    #expect(ContactWords.titleShown(contact) == Array(repeating: cut, count: 5).joined(separator: ", ") + ", …",
            "five names on its line, each cut to 64 scalars")
    #expect(ContactWords.spoken(contact).components(separatedBy: thumbs).count == 257, "VoiceOver still reads every name")
    #expect(ContactWords.lines(contact).split(separator: "\n").count == 257, "and copying gives every card")
    for (shape, row, vcard) in [("a note broken softly", one, soft), ("256 cards named in thumbs", many, named)] {
        // A layout that read the vCard would take as long as reading it, and one that typeset every
        // name longer, on any Mac and in any build.
        let (took, reading) = (fastest { _ = cardLaid(row) }, fastest { _ = ContactCard.read(vcard) })
        #expect(took < reading / 4, "\(shape): laid out in \(took); reading the card takes \(reading)")
    }
}

// MARK: - Message, and the sheet of several

@Test func `Message on a card with one number starts a chat with it, several are offered, none is said, and several cards open their sheet`() throws {
    #expect(try CardAction.of(sampleCard(2)) == .message(ContactCard.Phone(number: "+351 913 002 210", whatsapp: "+351913002210"),
                                                          name: "Inês Gomes"))
    #expect(try CardAction.of(sampleCard(1)) == .choose([ContactCard.Phone(number: "+351 21 345 6789", whatsapp: "+351213456789"),
                                                         ContactCard.Phone(number: "21 000 00 00")], name: "Marta Lopes"))
    #expect(try CardAction.of(sampleCard(5)) == .nothing)
    #expect(try CardAction.of(sampleCard(3)) == .cards)
    #expect(try CardAction.of(sampleCard(8)) == .nothing, "a deleted card")
    let twice = ContactCard(name: "Ana", phones: [ContactCard.Phone(number: "+351 912 345 678", whatsapp: "+351912345678"),
                                                  ContactCard.Phone(number: "+351912345678")])
    #expect(CardAction.numbers(twice).count == 1, "each number once, by the number it dials")
}

@MainActor @Test func `a click on a card starts its chat, pops its numbers up where it was clicked, says it has none, or asks for the sheet`() async throws {
    let host = PaneHost()
    let rows = try [sampleCard(2), sampleCard(1), sampleCard(3), sampleCard(5)]
    let (c, window) = await shown(rows, host)
    defer { window.close() }
    var popped: [(NSMenu, NSPoint)] = []
    c.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    var said: [String] = []
    c.notice = { said.append($0) }
    func click(_ m: MessageRow) throws -> CGPoint {
        let view = try messageView(c, #require(c.rows.firstIndex { $0.id == m.id }))
        let r = try #require(view.laid.flatMap(cardRect))
        let point = CGPoint(x: r.midX, y: r.midY)
        c.clicked(view, point, 1)
        return point
    }
    _ = try click(rows[0])
    #expect(host.started == ["+351913002210 Inês Gomes"])
    let at = try click(rows[1])
    #expect(popped.map(\.1) == [at], "where it was clicked")
    #expect(popped.first?.0.items.map(\.title) == ["+351 21 345 6789 · WhatsApp", "21 000 00 00"])
    (popped.first?.0.items.last as? ClosureItem)?.run()
    #expect(host.started.last == "21 000 00 00 Marta Lopes")
    _ = try click(rows[2])
    #expect(host.sheets == [rows[2].id])
    _ = try click(rows[3])
    #expect(said == [CardAction.noNumber])
}

@MainActor @Test func `Enter on a card does what a click on it does, its numbers popped up under it`() async throws {
    let host = PaneHost()
    let rows = try [sampleCard(2), sampleCard(1)]
    let (c, window) = await shown(rows, host)
    defer { window.close() }
    var popped: [(NSMenu, NSView)] = []
    c.popUp.run = { menu, _, view in popped.append((menu, view)) }
    c.select(rows[0].id)
    c.act("\n")
    #expect(host.started == ["+351913002210 Inês Gomes"])
    c.select(rows[1].id)
    c.act("\n")
    #expect(popped.count == 1 && popped.first?.1 === c.table && popped.first?.0.items.count == 2)
}

@MainActor @Test func `a number on WhatsApp as an account whose digits aren't its own says which account Message opens`() {
    #expect(CardAction.label(ContactCard.Phone(number: "+351 21 345 6789", whatsapp: "+351213456789")) == "+351 21 345 6789 · WhatsApp")
    let other = ContactCard.Phone(number: "+351 912 345 678", whatsapp: "+447700900123")
    #expect(CardAction.label(other) == "+351 912 345 678 · WhatsApp +447700900123")
    #expect(CardAction.label(ContactCard.Phone(number: "912 345 678", whatsapp: "+351912345678")) == "912 345 678 · WhatsApp +351912345678",
            "written without its country, the account says it")
    #expect(CardAction.label(ContactCard.Phone(number: "21 000 00 00")) == "21 000 00 00")
    let menu = pane(PaneHost()).numbersMenu([other, ContactCard.Phone(number: "21 000 00 00")], name: "Ana")
    #expect(menu.items.map(\.title) == ["+351 912 345 678 · WhatsApp +447700900123", "21 000 00 00"])
}

@MainActor @Test func `a card's menu starts with Message, its numbers when it has several, and several cards' with View Contacts`() throws {
    let c = pane(PaneHost())
    func titles(_ m: MessageRow) throws -> [String] {
        try #require(c.menu(for: m)).items.prefix(3).map { $0.isSeparatorItem ? "—" : $0.title }
    }
    #expect(try titles(sampleCard(2)) == ["Message Inês Gomes", "—", "Reply"])
    let marta = try #require(c.menu(for: sampleCard(1)))
    #expect(marta.items.first?.title == "Message Marta Lopes")
    #expect(marta.items.first?.submenu?.items.map(\.title) == ["+351 21 345 6789 · WhatsApp", "21 000 00 00"])
    #expect(try titles(sampleCard(3)) == ["View Contacts…", "—", "Reply"])
    #expect(try titles(sampleCard(5)).first == "Reply", "a card without a number starts as any message's")
    #expect(try ConversationController.copied(sampleCard(1)) == "Marta Lopes\n+351 21 345 6789, 21 000 00 00", "copied as it's drawn")
}

@MainActor @Test func `the sheet of several chooses a card with the arrows, and Return messages its number on WhatsApp`() throws {
    let five = try #require(sampleCard(4).contact)
    let sheet = ContactCardsSheet(cards: five.cards)
    sheet.up()
    #expect(sheet.selected == 0)
    #expect(sheet.returned()?.phone == "+351916666666" && sheet.returned()?.name == "Tiago Martins")
    for _ in 0 ..< 6 {
        sheet.down()
    }
    #expect(sheet.selected == 4, "no further than the last")
    sheet.choose(1)
    #expect(sheet.returned()?.phone == "+351 917 777 777", "Sofia's number, which isn't on WhatsApp")
    #expect(ContactCardsSheet(cards: [ContactCard(name: "Escola")]).returned() == nil, "a card without a number messages no one")
}

/// A message of `n` cards, each with `numbers` numbers.
func cards(_ n: Int, numbers: Int) -> Contact {
    Contact(name: "\(n) contacts", cards: (0 ..< n).map { i in
        ContactCard(name: "Card \(i)", phones: (0 ..< numbers).map { ContactCard.Phone(number: String(format: "+351 91%03d %04d", i, $0)) })
    })
}

/// The sheet of `contact`'s cards in a window off the screen, and how long it took to lay out.
@MainActor
func sheetShown(_ contact: Contact, sheet: ContactCardsSheet? = nil) -> (took: Duration, window: NSWindow, host: NSView) {
    let window = NSWindow(contentRect: NSRect(x: -30000, y: -30000, width: 440, height: 600), styleMask: [.borderless],
                          backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.animationBehavior = .none
    window.orderFront(nil)
    let view = ContactCardsView(contact: contact, from: "From Rui", roles: Catalog.jaw.roles(dark: true),
                                sheet: sheet ?? ContactCardsSheet(cards: contact.cards), message: { _, _ in }, done: {})
    var host = NSView()
    let took = ContinuousClock().measure {
        host = NSHostingView(rootView: view)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
    }
    return (took, window, host)
}

@MainActor
func scrollViews(in view: NSView) -> [NSScrollView] {
    if let scroll = view as? NSScrollView { return [scroll] }
    return view.subviews.flatMap { scrollViews(in: $0) }
}

@MainActor @Test func `the sheet of a message of many cards lays out only those in sight, and still scrolls to the one chosen`() async throws {
    func laid(_ n: Int) -> Duration {
        (0 ..< 3).map { _ in
            let shown = sheetShown(cards(n, numbers: 16))
            shown.window.close()
            return shown.took
        }.min()!
    }
    _ = laid(8)
    let (few, many) = (laid(8), laid(256))
    #expect(many < few * 3, "256 cards of 16 numbers laid out in \(many), and 8 in \(few)")

    let contact = cards(256, numbers: 1)
    let sheet = ContactCardsSheet(cards: contact.cards)
    let shown = sheetShown(contact, sheet: sheet)
    defer { shown.window.close() }
    let scroll = try #require(scrollViews(in: shown.host).first, "the sheet's scroll view")
    let top = scroll.documentVisibleRect.minY
    sheet.choose(200)
    try await Task.sleep(for: .milliseconds(500))
    shown.host.layoutSubtreeIfNeeded()
    #expect(scroll.documentVisibleRect.minY > top + 200 * 40, "card 200 scrolled to: from \(top) to \(scroll.documentVisibleRect.minY)")
}

@Test func `every contact card draws as its reference, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let image = try #require(renderCards(spec(look, dark: dark)))
            try compare(image, "contacts-\(look.rawValue)-\(dark ? "dark" : "light")")
        }
    }
}

/// The sample cards drawn as a conversation 640 points wide, at 2x.
func renderCards(_ spec: LayoutSpec) -> CGImage? {
    let rows = Samples.contacts
    let cs = MessageContext.of(rows, group: true, unfolded: [], now: Samples.now)
    let laid = zip(rows, cs).map { MessageLayout.lay($0, width: 640, context: $1, style: spec.style, fonts: spec.fonts, me: spec.me) }
    let height = laid.reduce(0) { $0 + $1.height } + 16
    let scale: CGFloat = 2
    guard let ctx = CGContext(data: nil, width: Int(640 * scale), height: Int(height * scale), bitsPerComponent: 8,
                              bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    ctx.scaleBy(x: scale, y: scale)
    ctx.translateBy(x: 0, y: height)
    ctx.scaleBy(x: 1, y: -1)
    ctx.setFillColor(spec.style.roles.ground.cgColor)
    ctx.fill(CGRect(x: 0, y: 0, width: 640, height: height))
    var y: CGFloat = 8
    for l in laid {
        l.draw(in: ctx, y: y)
        y += l.height
    }
    return ctx.makeImage()
}
