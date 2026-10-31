import CoreGraphics
import Foundation
import ImageIO
@testable import JawKit
import Testing
import UniformTypeIdentifiers

/// The vCard reader: jaw-proto's cases (`crates/jaw-proto/src/vcard.rs`), each giving the names and
/// numbers the terminal reads; the photos it reads, and those it leaves out; and hostile cards, read
/// in time that grows with their length and no faster, however their lines are folded.

func phone(_ number: String, _ whatsapp: String? = nil) -> ContactCard.Phone {
    ContactCard.Phone(number: number, whatsapp: whatsapp)
}

/// The one card `text` holds.
func one(_ text: String, sourceLocation: SourceLocation = #_sourceLocation) -> VCard.Card {
    let cards = VCard.read(text)
    #expect(cards.count == 1, "\(text.prefix(200).debugDescription) is one card: \(cards.count)", sourceLocation: sourceLocation)
    return cards.first ?? VCard.Card()
}

@Test func `a phone's card gives its name and each number with the WhatsApp account it is`() {
    let card = one("BEGIN:VCARD\r\nVERSION:3.0\r\nN:Lopes;Marta;;Dr.ª;\r\nFN:Dr.ª Marta Lopes\r\nitem1.TEL;waid=351213456789:+351 21 345 6789\r\n"
        + "item1.X-ABLabel:Work\r\nTEL;type=HOME:21 000 00 00\r\nEND:VCARD")
    #expect(card.name == "Dr.ª Marta Lopes")
    #expect(card.phones == [phone("+351 21 345 6789", "+351213456789"), phone("21 000 00 00")])
    #expect(card.phones.map(\.dialled) == ["+351213456789", "21 000 00 00"])
}

@Test func `a card without a formatted name is named by its parts, escaped semicolons and all`() {
    let card = one("BEGIN:VCARD\nN:Costa\\;Silva;Inês;Maria;;\nTEL;CELL;WAID=351914444444:+351 914 444 444\nEND:VCARD")
    #expect(card.name == "Inês Maria Costa;Silva")
    #expect(card.phones == [phone("+351 914 444 444", "+351914444444")])
    #expect(one("BEGIN:VCARD\nFN:  \nN:;;;;\nEND:VCARD").name == nil, "a name of nothing names nothing")
}

@Test func `folded lines, escapes, quoted params and vCard 2.1's quoted-printable read as they mean`() {
    let card = one("BEGIN:VCARD\nFN:Ana\\, a Ribeiro\\nde Lisboa\nTEL;TYPE=\"cell:voice\";waid=351912\n 345678:+351 912\n\t345 678\nEND:VCARD")
    #expect(card.name == "Ana, a Ribeiro\nde Lisboa")
    #expect(card.phones == [phone("+351 912345 678", "+351912345678")])
    let old = one("BEGIN:VCARD\r\nVERSION:2.1\r\nFN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=C3=89lia S=C3=A1=\r\nnchez\r\n"
        + "TEL;CELL;QUOTED-PRINTABLE:+34=20600=20000=20000\r\nTEL;VALUE=uri:tel:+351-21-000-0000\r\nEND:VCARD\r\n")
    #expect(old.name == "Élia Sánchez")
    #expect(old.phones == [phone("+34 600 000 000"), phone("+351-21-000-0000")])
}

@Test func `several cards in one text read one after another, and what isn't a card reads as none`() {
    let both = "\u{FEFF}BEGIN:VCARD\r\nFN:Marta\r\nEND:VCARD\r\nBEGIN:VCARD\r\nFN:Rui\r\nTEL:+351913333333\r\nEND:VCARD\r\n"
    #expect(VCard.read(both).map(\.name) == ["Marta", "Rui"])
    #expect(VCard.read("FN:outside\nBEGIN:VCARD\nFN:Inês").count == 1, "an open card at the end still counts")
    #expect(VCard.read("not a card at all").isEmpty)
    #expect(VCard.read("").isEmpty)
}

@Test func `a card reads so many numbers of so much, and no waid that isn't an account's digits`() {
    var many = "BEGIN:VCARD\nFN:Many\n"
    for n in 0 ..< 40 {
        many += String(format: "TEL;waid=35191000%04d:+35191000%04d\n", n, n)
    }
    #expect(one(many + "END:VCARD").phones.count == VCard.numbersMost)
    let long = "BEGIN:VCARD\nFN:\(String(repeating: "n", count: 1000))\nTEL;waid=12x456789:\(String(repeating: "9", count: 500))\n"
        + "TEL;waid=123:+123\nEND:VCARD"
    let card = one(long)
    #expect(card.name?.count == VCard.nameMost)
    #expect(card.phones == [phone(String(repeating: "9", count: VCard.numberMost)), phone("+123")])
    let lots = String(repeating: "BEGIN:VCARD\nFN:x\nEND:VCARD\n", count: VCard.cardsMost + 10)
    #expect(VCard.read(lots).count == VCard.cardsMost)
}

@Test func `a line whose colon comes a KiB in or later is no property, and only its own end breaks it softly`() {
    func padded(_ pad: Int) -> String {
        "BEGIN:VCARD\nTEL;X-PAD=\(String(repeating: "a", count: pad));waid=351912345678:+351 912 345 678\nFN:Ana\nEND:VCARD"
    }
    #expect(one(padded(VCard.headMost - 100)).phones == [phone("+351 912 345 678", "+351912345678")])
    let card = one(padded(VCard.headMost))
    #expect(card.name == "Ana" && card.phones.isEmpty, "the line skipped, the rest read")
    let open = openQuoted(64 * 1024).replacingOccurrences(of: "END:VCARD", with: "FN:Ana\r\nEND:VCARD")
    #expect(one(open).name == "Ana", "a quote left open ends with its line")
    let after = "BEGIN:VCARD\r\nNOTE;ENCODING=QUOTED-PRINTABLE:a==\r\n\r\nFN:Rui\r\nEND:VCARD\r\n"
    #expect(one(after).name == "Rui", "the = an empty line goes on from isn't its own")
}

@Test func `a card's last formatted name names it, and a blank one after it leaves its structured name`() {
    #expect(one("BEGIN:VCARD\nFN:Marta\nFN:Marta Lopes\nEND:VCARD").name == "Marta Lopes")
    #expect(one("BEGIN:VCARD\nN:Lopes;Marta;;;\nFN:Marta\nFN: \nEND:VCARD").name == "Marta Lopes",
            "the last FN, blank, gives way to N")
}

@Test func `the white space around a name, a number, a parameter or a card's mark is Unicode's, as jaw-proto's trim has it`() {
    #expect(one("BEGIN:VCARD\nN:Lopes;Marta;;;\nFN:\u{200B}\nTEL:\u{200B}\nEND:VCARD") == VCard.Card(name: "\u{200B}", phones: [phone("\u{200B}")]),
            "a zero-width space isn't white space, so it names the card, and is a number, as in the terminal")
    #expect(VCard.read("BEGIN:VCARD\nFN:Ana\nEND:VCARD\u{A0}\nFN:Rui\nEND:VCARD").map(\.name) == ["Ana"],
            "a no-break space after END:VCARD still ends the card")
    #expect(VCard.read("\u{A0}BEGIN\u{205F}:\u{2028}vcard\u{3000}\nFN:Ana\nEND:VCARD").map(\.name) == ["Ana"], "nor stops one beginning")
    let card = one("BEGIN:VCARD\n\u{2003}FN\u{3000}:Ana\n\u{85}TEL;waid=\u{A0}351912345678\u{2009}:+351 912 345 678\nEND:VCARD")
    #expect(card.name == "Ana" && card.phones == [phone("+351 912 345 678", "+351912345678")],
            "around a property's name and its parameters' values, past ASCII's")
    #expect(one("BEGIN:VCARD\nFN:\u{200B}Ana\u{200B}\nEND:VCARD").name == "\u{200B}Ana\u{200B}")
    #expect(one("BEGIN:VCARD\nFN:" + String(repeating: " ", count: VCard.valueMost) + "Ana\nN:;Rui;;;\nEND:VCARD").name == "Rui",
            "past the 64 KiB of a value kept as written, a name reads as none here, where the terminal reads it")
}

@Test func `names and numbers in another case, in a group, or with a BOM before them read as theirs`() {
    let card = one("begin:vcard\n\u{FEFF}fn:Ana\nGROUP.tel;WaId=351912345678:+351 912 345 678\nend:vcard")
    #expect(card.name == "Ana" && card.phones == [phone("+351 912 345 678", "+351912345678")])
    #expect(VCard.read("BEGIN:VCALENDAR\nFN:Ana\nEND:VCALENDAR").isEmpty, "a calendar isn't a card")
    #expect(one("BEGIN:VCARD\nFN:Ana\nBEGıN:VCARD\nEND:VCARD").name == "Ana", "only ASCII letters match in any case")
}

// MARK: - Photos

/// A picture `w` by `h` pixels in `type`, red on its left half and blue on its right.
func photo(_ w: Int, _ h: Int, _ type: UTType = .jpeg) throws -> Data {
    let ctx = try #require(CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                                     space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    ctx.setFillColor(CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1))
    ctx.fill(CGRect(x: 0, y: 0, width: w / 2, height: h))
    ctx.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 1, alpha: 1))
    ctx.fill(CGRect(x: w / 2, y: 0, width: w - w / 2, height: h))
    let image = try #require(ctx.makeImage())
    let data = NSMutableData()
    let destination = try #require(CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, image, nil)
    #expect(CGImageDestinationFinalize(destination))
    return data as Data
}

/// `data` in base64, folded at 75 columns as Contacts folds a photo.
func folded(_ data: Data) -> String {
    let base64 = data.base64EncodedString()
    var out = ""
    var at = base64.startIndex
    while at < base64.endIndex {
        let end = base64.index(at, offsetBy: 75, limitedBy: base64.endIndex) ?? base64.endIndex
        out += (out.isEmpty ? "" : "\r\n ") + base64[at ..< end]
        at = end
    }
    return out
}

/// A card's pixel at (x, y), from the top left, as RGBA.
func pixel(_ picture: CardPicture, _ x: Int, _ y: Int) -> [UInt8] {
    let at = (y * picture.side + x) * 4
    return Array(picture.pixels[at ..< at + 4])
}

@Test func `a card's photo, inline in 2.1, 3.0 or 4.0, is read into a round picture`() throws {
    let jpeg = try photo(200, 120), png = try photo(64, 64, .png)
    let three = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Marta Lopes\r\nPHOTO;ENCODING=b;TYPE=JPEG:\(folded(jpeg))\r\nTEL:+351 213 456 789\r\nEND:VCARD\r\n"
    let card = try #require(ContactCard.read(three).first)
    #expect(card.name == "Marta Lopes" && card.phones == [phone("+351 213 456 789")], "what's after the photo is read too")
    let picture = try #require(card.picture)
    #expect(picture.side == CardPicture.side && picture.pixels.count == CardPicture.side * CardPicture.side * 4)
    #expect(pixel(picture, 0, 0)[3] == 0, "outside the circle, nothing")
    let left = pixel(picture, 20, 48), right = pixel(picture, 76, 48)
    #expect(left[0] > 200 && left[2] < 60 && right[2] > 200 && right[0] < 60, "the photo fills it from its centre: \(left) \(right)")
    #expect(picture.image?.width == CardPicture.side)

    let four = "BEGIN:VCARD\nVERSION:4.0\nFN:Rui\nPHOTO:data:image/png;base64,\(png.base64EncodedString())\nEND:VCARD"
    #expect(ContactCard.read(four).first?.picture != nil)
    let old = "BEGIN:VCARD\r\nVERSION:2.1\r\nFN:Inês\r\nPHOTO;ENCODING=BASE64;JPEG:\(folded(jpeg))\r\n\r\nEND:VCARD\r\n"
    #expect(ContactCard.read(old).first?.picture != nil)
    let bare = "BEGIN:VCARD\r\nVERSION:2.1\r\nFN:Inês\r\nPHOTO;JPEG;BASE64:\(jpeg.base64EncodedString())\r\nEND:VCARD\r\n"
    #expect(ContactCard.read(bare).first?.picture != nil)
}

@Test func `a photo too large, too wide, not a JPEG or a PNG, or at a URL is left out, and the card read`() throws {
    func picture(_ photoLine: String) -> CardPicture?? {
        let card = ContactCard.read("BEGIN:VCARD\r\nFN:Ana\r\n\(photoLine)\r\nTEL:+351912345678\r\nEND:VCARD\r\n").first
        #expect(card?.name == "Ana" && card?.phones == [phone("+351912345678")], "\(photoLine.prefix(60))")
        return card.map(\.picture)
    }
    let big = Data(repeating: 0xAB, count: VCard.photoMost)
    #expect(picture("PHOTO;ENCODING=b;TYPE=JPEG:\(folded(big))") == .some(nil), "past 512 KiB as written")
    #expect(try picture("PHOTO;ENCODING=b;TYPE=PNG:\(folded(photo(CardPicture.widest + 1, 8, .png)))") == .some(nil), "too wide")
    #expect(try picture("PHOTO;ENCODING=b;TYPE=GIF:\(folded(photo(16, 16, .gif)))") == .some(nil), "a GIF")
    #expect(picture("PHOTO;ENCODING=b;TYPE=JPEG:\(Data("hello, not a picture".utf8).base64EncodedString())") == .some(nil))
    #expect(picture("PHOTO;VALUE=uri:https://example.com/ana.jpg") == .some(nil), "never fetched")
    #expect(picture("PHOTO:data:image/jpeg,%FF%D8") == .some(nil), "a data URI that isn't base64")
}

/// `jpeg` made `size` bytes long by comments after its start, as a camera's notes make a photo
/// long without anything more to decode.
func commented(_ jpeg: Data, to size: Int) -> Data {
    var out = Data(jpeg.prefix(2))
    var room = size - jpeg.count
    while room > 4 {
        let n = min(room - 4, 0xFFFD)
        out.append(contentsOf: [0xFF, 0xFE, UInt8((n + 2) >> 8), UInt8((n + 2) & 0xFF)])
        out.append(Data(repeating: 0x20, count: n))
        room -= n + 4
    }
    return out + jpeg.dropFirst(2)
}

@Test func `only the first eight cards' photos are kept, so a message of many costs no more`() throws {
    let jpeg = try photo(32, 32)
    let card = "BEGIN:VCARD\r\nFN:Climber\r\nPHOTO;ENCODING=b;TYPE=JPEG:\(folded(jpeg))\r\nEND:VCARD\r\n"
    let ten = String(repeating: card, count: CardPicture.most + 2)
    let cards = ContactCard.read(ten)
    #expect(cards.count == CardPicture.most + 2)
    #expect(cards.map { $0.picture != nil } == Array(repeating: true, count: CardPicture.most) + [false, false])
    #expect(VCard.read(ten, photos: CardPicture.most).map { $0.photo != nil } == Array(repeating: true, count: CardPicture.most) + [false, false],
            "the rest's photos skipped as they're read")

    // What photos add to reading a message, each half as long as a photo kept can be: as much for
    // 30 cards as for the 8 whose photos are kept. A photo in a note costs only its length.
    let long = commented(jpeg, to: VCard.photoMost * 3 / 8)
    let photographed = "BEGIN:VCARD\r\nFN:Climber\r\nPHOTO;ENCODING=b;TYPE=JPEG:\(folded(long))\r\nEND:VCARD\r\n"
    #expect(ContactCard.read(photographed).first?.picture != nil, "a photo of \(long.count) bytes, read")
    func photos(_ n: Int) -> Duration {
        let message = String(repeating: photographed, count: n)
        return fastest(message) - fastest(message.replacingOccurrences(of: "PHOTO;", with: "NOTE;"))
    }
    let (eight, thirty) = (photos(CardPicture.most), photos(30))
    #expect(thirty < eight * 2, "photos add \(eight) to 8 cards and \(thirty) to 30")
}

// MARK: - Hostile cards

/// A nameless card of about `size` bytes whose note is quoted-printable broken softly after each
/// character, its lines `x=`.
func softBroken(_ size: Int) -> String {
    "BEGIN:VCARD\r\nVERSION:2.1\r\nNOTE;ENCODING=QUOTED-PRINTABLE:" + String(repeating: "x=\r\n", count: size / 4) + "x\r\nEND:VCARD\r\n"
}

/// A nameless card of about `size` bytes whose note leaves a parameter's quote open and goes on in
/// folded lines, each ending in `=`.
func openQuoted(_ size: Int) -> String {
    "BEGIN:VCARD\r\nVERSION:3.0\r\nNOTE;X-OPEN=\"" + String(repeating: "\r\n x=", count: size / 5) + "\r\nEND:VCARD\r\n"
}

/// A card of about `size` bytes with a photo, its base64 folded at 75 columns.
func withPhoto(_ size: Int) -> String {
    "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Marta Lopes\r\nPHOTO;ENCODING=b;TYPE=JPEG:" + String(repeating: String(repeating: "A", count: 75) + "\r\n ", count: size / 78)
        + "A==\r\nEND:VCARD\r\n"
}

/// About `size` bytes of `piece` again and again inside one card.
func repeated(_ piece: String, _ size: Int) -> String {
    "BEGIN:VCARD\r\n" + String(repeating: piece, count: size / piece.utf8.count) + "END:VCARD\r\n"
}

/// Each shape of hostile card the reader is measured on, of about `size` bytes.
func hostile(_ size: Int) -> [(String, String)] {
    [
        ("quoted-printable broken softly", softBroken(size)),
        ("a quote left open", openQuoted(size)),
        ("a photo folded at 75 columns", withPhoto(size)),
        ("a note folded after each character", "BEGIN:VCARD\r\nNOTE:" + String(repeating: "\r\n x", count: size / 4) + "\r\nEND:VCARD\r\n"),
        ("blank lines", repeated("\n", size)),
        ("lines of a property each", repeated("A:\r\n", size)),
        ("formatted names", repeated("FN:a\r\n", size)),
        ("numbers", repeated("TEL:1\r\n", size)),
        ("cards one after another", String(repeating: "BEGIN:VCARD\r\nEND:VCARD\r\n", count: size / 24)),
    ]
}

/// The fastest of three reads of `card`.
func fastest(_ card: String) -> Duration {
    let clock = ContinuousClock()
    return (0 ..< 3).map { _ in clock.measure { _ = ContactCard.read(card) } }.min()!
}

@Test func `a hostile card takes as long as its length, however its lines are folded`() {
    // Each line looked at once, four times the card takes four times as long; a reader that looks at
    // the whole of a line again for each line folded into it takes sixteen times, and seconds. The
    // bounds are a debug build's, on the build Mac under load; a release build reads several times
    // faster.
    let small = hostile(256 * 1024), large = hostile(1024 * 1024)
    for ((shape, a), (_, b)) in zip(small, large) {
        let (once, four) = (fastest(a), fastest(b))
        #expect(four < once * 8 + .milliseconds(5), "\(shape): 256 KiB in \(once), 1 MiB in \(four)")
        #expect(four < .milliseconds(400), "\(shape) of 1 MiB read in \(four)")
    }
}

@Test func `eight photos of 2,048 pixels a side cost a row about what one does, the pixels a row decodes shared in order`() throws {
    // A PNG is decoded whole, so pixels are what a photo costs. A row decodes those of one photo at
    // its widest; the bubble's pictures come first, in the cards' order.
    let png = try photo(CardPicture.widest, CardPicture.widest, .png)
    let card = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Climber\r\nPHOTO;ENCODING=b;TYPE=PNG:\(folded(png))\r\nEND:VCARD\r\n"
    let eight = String(repeating: card, count: 8)
    #expect(ContactCard.read(eight).map { $0.picture != nil } == [true] + Array(repeating: false, count: 7), "the first takes them all")
    let (one, all) = (fastest(card), fastest(eight))
    #expect(all < one * 2, "one card read in \(one), eight in \(all)")

    let quarter = try photo(CardPicture.widest / 2, CardPicture.widest / 2, .png)
    let card4 = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Climber\r\nPHOTO;ENCODING=b;TYPE=PNG:\(folded(quarter))\r\nEND:VCARD\r\n"
    #expect(ContactCard.read(String(repeating: card4, count: 5)).map { $0.picture != nil } == [true, true, true, true, false],
            "four of half as wide share them")
}

@Test func `a card as long as a line jawd sends is read in time that grows with its length`() {
    let small = hostile(256 * 1024), large = hostile(15 * 1024 * 1024)
    for ((shape, a), (_, b)) in zip(small, large) {
        let (once, sixty) = (fastest(a), fastest(b))
        #expect(sixty < once * 90 + .milliseconds(50), "\(shape): 256 KiB in \(once), 15 MiB in \(sixty)")
        #expect(sixty < .seconds(5), "\(shape) of 15 MiB read in \(sixty)")
    }
}
