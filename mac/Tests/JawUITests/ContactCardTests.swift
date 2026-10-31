import AppKit
import CoreGraphics
import CoreText
@testable import JawKit
import JawThemes
@testable import JawUI
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

@Test func `a card read from a hostile vCard is laid out from what was read, in no time`() throws {
    let vcard = "BEGIN:VCARD\r\nVERSION:2.1\r\nFN:Ana Ribeiro\r\nTEL;waid=351912345678:+351 912 345 678\r\nNOTE;ENCODING=QUOTED-PRINTABLE:"
        + String(repeating: "x=\r\n", count: 1 << 20) + "x\r\nEND:VCARD\r\n"
    let row = try #require(Samples.contactRow(9, at: Samples.now, from: Samples.ana, name: "Ana Ribeiro", vcard: vcard))
    #expect(row.contact?.cards.first?.phones.first?.dialled == "+351912345678", "the card read as its row was")
    let clock = ContinuousClock()
    let took = (0 ..< 3).map { _ in clock.measure { _ = cardLaid(row) } }.min()!
    #expect(took < .milliseconds(20), "laid out in \(took)")
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
