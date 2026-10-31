import CoreGraphics
import Darwin
import Foundation
import ImageIO

/// A contact card as it's drawn: the name it gives, its numbers in the order they come, and its
/// photo, when it carries one, as a small round picture.
public struct ContactCard: Sendable, Hashable {
    public var name: String?
    public var phones: [Phone]
    public var picture: CardPicture?

    /// One of a card's numbers as the card writes it, and, when the card says it's on WhatsApp (the
    /// phones mark it `waid`), the number of that account in international form: `+` and its digits.
    public struct Phone: Sendable, Hashable {
        public var number: String
        public var whatsapp: String?

        public init(number: String, whatsapp: String? = nil) {
            self.number = number
            self.whatsapp = whatsapp
        }

        /// The number to start a chat with: its WhatsApp account's, or the one written.
        public var dialled: String {
            whatsapp ?? number
        }
    }

    public init(name: String?, phones: [Phone] = [], picture: CardPicture? = nil) {
        self.name = name
        self.phones = phones
        self.picture = picture
    }

    /// The cards a vCard holds, as `VCard.read` reads them, the photos of the first
    /// `CardPicture.most` made into their pictures in order, as long as their pixels come to no more
    /// than `CardPicture.pixelsMost`; the rest's photos aren't even kept as they're read.
    public static func read(_ vcard: String) -> [ContactCard] {
        var pixels = CardPicture.pixelsMost
        return VCard.read(vcard, photos: CardPicture.most).map { card in
            ContactCard(name: card.name, phones: card.phones, picture: card.photo.flatMap { CardPicture(photo: $0, pixels: &pixels) })
        }
    }
}

/// A card's photo as a round picture `side` pixels across, premultiplied RGBA in sRGB, made once
/// as the card is read, so drawing it decodes nothing.
public struct CardPicture: Sendable, Hashable {
    /// How many pixels across: a 40-point circle at twice its size, and some.
    public static let side = 96
    /// The most pixels a side of a photo read, and the most cards of one message whose photos are.
    public static let widest = 2048
    public static let most = 8
    /// The most pixels a message's photos come to together: one photo at its widest. A PNG is
    /// decoded whole, and the row is read on the client's only reading thread each time jawd sends
    /// it, so eight photos cost it no more than one does.
    public static let pixelsMost = widest * widest

    public let side: Int
    public let pixels: Data

    /// The photo's picture: a JPEG or a PNG of at most `widest` pixels a side, filling the circle from
    /// its centre, when it has no more pixels than are left of `pixels`, which it takes from them;
    /// none for anything else.
    public init?(photo: Data, pixels left: inout Int) {
        let quiet = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithData(photo as CFData, quiet),
              let type = CGImageSourceGetType(source) as String?, ["public.jpeg", "public.png"].contains(type),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, quiet) as? [String: Any],
              let w = properties[kCGImagePropertyPixelWidth as String] as? Int, let h = properties[kCGImagePropertyPixelHeight as String] as? Int,
              w > 0, h > 0, w <= Self.widest, h <= Self.widest, w * h <= left
        else { return nil }
        left -= w * h
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Self.side * 2,
            kCGImageSourceShouldCacheImmediately: true,
        ] as CFDictionary
        guard let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else { return nil }
        let side = Self.side
        var pixels = Data(count: side * side * 4)
        let drawn = pixels.withUnsafeMutableBytes { raw -> Bool in
            guard let ctx = CGContext(data: raw.baseAddress, width: side, height: side, bitsPerComponent: 8, bytesPerRow: side * 4,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return false }
            let full = CGRect(x: 0, y: 0, width: side, height: side)
            ctx.addEllipse(in: full)
            ctx.clip()
            let scale = CGFloat(side) / CGFloat(min(thumbnail.width, thumbnail.height))
            let size = CGSize(width: CGFloat(thumbnail.width) * scale, height: CGFloat(thumbnail.height) * scale)
            ctx.interpolationQuality = .high
            ctx.draw(thumbnail, in: CGRect(x: (full.width - size.width) / 2, y: (full.height - size.height) / 2,
                                           width: size.width, height: size.height))
            return true
        }
        guard drawn else { return nil }
        self.side = side
        self.pixels = pixels
    }

    /// The picture to draw, made from its pixels as they are.
    public var image: CGImage? {
        guard let provider = CGDataProvider(data: pixels as CFData) else { return nil }
        return CGImage(width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: side * 4,
                       space: CGColorSpace(name: CGColorSpace.sRGB)!,
                       bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                       provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }
}

/// Contact cards as the protocol carries them: a contact row's `vcard` holds its card, or its cards
/// one after another, as several contacts export to one file. This is jaw-proto's reader
/// (`crates/jaw-proto/src/vcard.rs`) in Swift, so a card reads here as it does in the terminal: a
/// card is what someone else's phone wrote, and can be as long as a line jawd sends, so only what's
/// drawn is read, and only so much of it, in one pass whose cost grows with the card's length and no
/// faster, however its lines are folded. It parts from jaw-proto's in one bound of its own: of a
/// name, a number or BEGIN's or END's value it keeps the first `valueMost` bytes as written, where
/// jaw-proto keeps them all, so one written after 64 KiB of white space, which no phone writes,
/// reads as none here and as itself in the terminal. Beyond jaw-proto's, it reads a card's photo
/// when the card carries it inline.
enum VCard {
    /// The most cards read from one vCard, and numbers from one card.
    static let cardsMost = 256
    static let numbersMost = 16
    /// The longest name and number kept, in characters.
    static let nameMost = 256
    static let numberMost = 64
    /// How far into a line its colon is looked for, in bytes. The colon ends its property's name and
    /// parameters, and a line without one that soon is no property, however long it goes on.
    static let headMost = 1024
    /// The most of a name's, a number's or a card's mark kept as written, in bytes: far past any
    /// name or number a card means, so what goes on past it is skipped rather than kept.
    static let valueMost = 64 * 1024
    /// The most of a photo kept as written, in bytes; a card's larger photo is left out.
    static let photoMost = 512 * 1024

    /// A card as it's read: the name it gives, its numbers, and its photo's bytes.
    struct Card: Equatable {
        var name: String?
        var phones: [ContactCard.Phone] = []
        var photo: Data?
    }

    /// The cards `text` holds, each from BEGIN:VCARD to END:VCARD, in order, `cardsMost` at most,
    /// with the photos of the first `photos`; the rest's are skipped as any property that isn't
    /// drawn is. What's outside them is left out, and a card the text ends inside still counts.
    static func read(_ text: String, photos: Int = 0) -> [Card] {
        var text = text
        return text.withUTF8 { bytes in
            var reader = Reader(bytes)
            return reader.cards(most: cardsMost, photos: photos)
        }
    }

    /// The properties a card is read for; the rest are skipped.
    enum Name {
        case begin, end, formatted, structured, phone, photo, other
    }

    /// The text's lines, read one after another: each physical line, from one `\n` to the next, its
    /// `\r` taken off, and each logical line, a physical one with those that go on from it.
    struct Reader {
        let bytes: UnsafeBufferPointer<UInt8>
        /// Where the next physical line starts, or nil past the last.
        var next: Int?
        /// The logical line being read: what's kept of it, its head once read, its kind.
        var line = Line()
        /// Whether the next line's photo is kept: the open card's first, when its photo is wanted.
        var keepsPhoto = false

        init(_ bytes: UnsafeBufferPointer<UInt8>) {
            self.bytes = bytes
            var at = 0
            while at + 2 < bytes.count, bytes[at] == 0xEF, bytes[at + 1] == 0xBB, bytes[at + 2] == 0xBF {
                at += 3
            }
            next = at
        }

        /// The first `most` cards, read no further than the last one's end, with the photos of the
        /// first `photos`.
        mutating func cards(most: Int, photos: Int) -> [Card] {
            var cards: [Card] = []
            var open: Reading?
            while skipBlank(), logical() {
                guard line.colon != nil else { continue }
                let begins = line.marks(.begin)
                if begins || line.marks(.end) {
                    if let card = open { cards.append(card.card()) }
                    open = nil
                    if cards.count >= most { return cards }
                    open = begins ? Reading() : nil
                } else {
                    open?.take(line)
                }
                keepsPhoto = open.map { !$0.photographed } == true && cards.count < photos
            }
            if let card = open { cards.append(card.card()) }
            return cards
        }

        /// Passes over blank lines, which say nothing, unless one goes on into a folded line after it,
        /// so a card of blank lines is read as fast as its bytes are looked at. Always true.
        mutating func skipBlank() -> Bool {
            while var at = next, at < bytes.count {
                if bytes[at] == 0x0D, at + 1 < bytes.count, bytes[at + 1] == 0x0A { at += 1 }
                guard bytes[at] == 0x0A else { return true }
                if at + 1 < bytes.count, bytes[at + 1] == UInt8(ascii: " ") || bytes[at + 1] == UInt8(ascii: "\t") { return true }
                next = at + 1
            }
            return true
        }

        /// The next physical line, without its `\r`.
        mutating func physical() -> Range<Int>? {
            guard let start = next else { return nil }
            var end = bytes.count
            if start < bytes.count, let base = bytes.baseAddress,
               let found = memchr(base + start, 0x0A, bytes.count - start)
            {
                end = base.distance(to: found.assumingMemoryBound(to: UInt8.self))
            }
            next = end < bytes.count ? end + 1 : nil
            let stop = end > start && bytes[end - 1] == 0x0D ? end - 1 : end
            return start ..< stop
        }

        /// Reads the next logical line into `line`: a line beginning with a space or a tab goes on
        /// from the one before, and so does the line after one of quoted-printable text ending in `=`.
        /// Each piece is read once: its head as far as `headMost`, and of the rest only what its kind
        /// keeps, so a line costs what its pieces' lengths do, however many it was folded into. False
        /// past the last line.
        mutating func logical() -> Bool {
            guard var piece = physical() else { return false }
            line.reset()
            line.keepsPhoto = keepsPhoto
            while true {
                take(piece)
                let soft = line.quotedPrintable && !piece.isEmpty && bytes[piece.upperBound - 1] == UInt8(ascii: "=")
                guard let after = next else { return true }
                if soft {
                    if !line.full, line.kept.last == UInt8(ascii: "=") { line.kept.removeLast() }
                    piece = physical()!
                } else if after < bytes.count, bytes[after] == UInt8(ascii: " ") || bytes[after] == UInt8(ascii: "\t") {
                    let folded = physical()!
                    piece = folded.lowerBound + 1 ..< folded.upperBound
                } else {
                    return true
                }
            }
        }

        /// Keeps as much of `piece` as the line keeps, reading its head as it comes.
        mutating func take(_ piece: Range<Int>) {
            var from = piece.lowerBound
            while from < piece.upperBound, !line.full {
                let room = line.room
                guard room > 0 else {
                    line.full = true
                    break
                }
                let taken = min(room, piece.upperBound - from)
                line.kept.append(contentsOf: UnsafeBufferPointer(rebasing: bytes[from ..< from + taken]))
                from += taken
                if line.colon == nil { line.readHead() }
            }
            if from < piece.upperBound, line.colon != nil { line.cut = true }
        }
    }

    /// A logical line as it's read: its head, the property's name and parameters up to the colon,
    /// read once; and as much of its value as its kind keeps.
    struct Line {
        var kept: [UInt8] = []
        /// Where the head's colon is in `kept`, once it's come.
        var colon: Int?
        /// How far the head's been looked at for its colon, and whether that's inside a quote.
        var scanned = 0
        var quoted = false
        /// No colon came in its first `headMost` bytes, so it's no property.
        var endless = false
        var name = Name.other
        /// Where its parameters start in `kept`: its name's `;`, or the colon when it has none.
        var params = 0
        var quotedPrintable = false
        /// It keeps no more, and what's past what it keeps is skipped.
        var full = false
        /// Its value went on past what's kept.
        var cut = false
        /// A photo on it is the card's, kept; otherwise it's skipped, as a property that isn't drawn.
        var keepsPhoto = false

        mutating func reset() {
            kept.removeAll(keepingCapacity: true)
            colon = nil
            scanned = 0
            quoted = false
            endless = false
            name = .other
            params = 0
            quotedPrintable = false
            full = false
            cut = false
            keepsPhoto = false
        }

        /// How many more bytes it keeps: up to `headMost` while its head is read, then as many of its
        /// value as its kind keeps.
        var room: Int {
            if endless { return 0 }
            guard let colon else { return VCard.headMost - kept.count }
            let most = switch name {
            case .begin, .end, .formatted, .structured, .phone: VCard.valueMost
            case .photo: VCard.photoMost
            case .other: 0
            }
            return colon + 1 + most - kept.count
        }

        /// Looks for the head's colon outside quotes in what's come since the last look, no further
        /// than `headMost` bytes in; once it's found, reads the property's name and parameters.
        mutating func readHead() {
            let end = min(kept.count, VCard.headMost)
            var at = scanned
            while at < end {
                let byte = kept[at]
                if byte == UInt8(ascii: "\"") { quoted.toggle() }
                if byte == UInt8(ascii: ":"), !quoted {
                    colon = at
                    ended()
                    return
                }
                at += 1
            }
            scanned = end
            if end == VCard.headMost { endless = true }
        }

        /// The head is read: its name without its group (`item1.TEL` is `TEL`), in any case, and
        /// whether a parameter says its value is quoted-printable. Read in place, so a line costs no
        /// more than its head's length, whatever it is.
        mutating func ended() {
            let colon = colon!
            params = kept[0 ..< colon].firstIndex(of: UInt8(ascii: ";")) ?? colon
            let start = (kept[0 ..< params].lastIndex(of: UInt8(ascii: ".")) ?? -1) + 1
            var named = VCard.trimmed(kept[start ..< params])
            while named.starts(with: VCard.bom) {
                named = named.dropFirst(VCard.bom.count)
            }
            name = switch named.count {
            case 1 where VCard.lower(named[named.startIndex]) == UInt8(ascii: "n"): .structured
            case 2 where VCard.same(named, VCard.fn): .formatted
            case 3 where VCard.same(named, VCard.tel): .phone
            case 3 where VCard.same(named, VCard.endWord): .end
            case 5 where VCard.same(named, VCard.beginWord): .begin
            case 5 where VCard.same(named, VCard.photoWord): keepsPhoto ? .photo : .other
            default: .other
            }
            quotedPrintable = params < colon && has("ENCODING", "QUOTED-PRINTABLE")
        }

        /// Each of its parameters, a name and a value, trimmed and its value's quotes taken off; a bare
        /// one, as vCard 2.1 writes `CELL`, has no name.
        func each(_ body: (ArraySlice<UInt8>, ArraySlice<UInt8>) -> Void) {
            guard let colon, params < colon else { return }
            var at = params + 1
            while at <= colon {
                let end = kept[at ..< colon].firstIndex(of: UInt8(ascii: ";")) ?? colon
                let param = kept[at ..< end]
                if let equals = param.firstIndex(of: UInt8(ascii: "=")) {
                    var value = VCard.trimmed(param[(equals + 1)...])
                    while value.first == UInt8(ascii: "\"") { value = value.dropFirst() }
                    while value.last == UInt8(ascii: "\"") { value = value.dropLast() }
                    body(VCard.trimmed(param[..<equals]), value)
                } else {
                    body(param[param.startIndex ..< param.startIndex], VCard.trimmed(param))
                }
                at = end + 1
            }
        }

        /// The first parameter named `key`, in any case.
        func param(_ key: String) -> String? {
            var found: String?
            each { name, value in
                if found == nil, VCard.same(name, key) { found = String(decoding: value, as: UTF8.self) }
            }
            return found
        }

        /// Whether a parameter is `value`, bare or named `key`, in any case.
        func has(_ key: String, _ value: String) -> Bool {
            var found = false
            each { name, given in
                if name.isEmpty || VCard.same(name, key), VCard.same(given, value) { found = true }
            }
            return found
        }

        /// Its value as kept, bytes after the colon.
        var value: ArraySlice<UInt8> {
            kept[(colon! + 1)...]
        }

        /// Whether it begins or ends a card, as `what` says.
        func marks(_ what: Name) -> Bool {
            name == what && !cut && VCard.same(VCard.trimmed(value), "VCARD")
        }

        /// Its value with any quoted-printable encoding undone, vCard's escapes still in it.
        var text: String {
            quotedPrintable ? VCard.quotedPrintable(value) : String(decoding: value, as: UTF8.self)
        }
    }

    /// A card being read: its formatted name and its structured one as written, the last of each it
    /// gives, read once it ends; its numbers so far; and its first photo.
    struct Reading {
        var formatted: Written?
        var structured: Written?
        var phones: [ContactCard.Phone] = []
        var photo: Data?
        var photographed = false

        /// A property's value as written, and whether it's quoted-printable, kept until the card ends.
        struct Written {
            var value: [UInt8]
            var quotedPrintable: Bool

            var text: String {
                quotedPrintable ? VCard.quotedPrintable(value[...]) : String(decoding: value, as: UTF8.self)
            }
        }

        mutating func take(_ line: Line) {
            switch line.name {
            case .formatted: Self.keep(&formatted, line)
            case .structured: Self.keep(&structured, line)
            case .phone where phones.count < VCard.numbersMost: phone(line)
            case .photo where !photographed:
                photographed = true
                photo = VCard.photo(line)
            default: break
            }
        }

        /// Keeps `line`'s value in place of the last one's, in the same buffer.
        static func keep(_ written: inout Written?, _ line: Line) {
            if written == nil { written = Written(value: [], quotedPrintable: false) }
            written!.value.removeAll(keepingCapacity: true)
            written!.value.append(contentsOf: line.value)
            written!.quotedPrintable = line.quotedPrintable
        }

        mutating func phone(_ line: Line) {
            var written = VCard.trim(VCard.unescaped(line.text))
            if VCard.same(written.utf8.prefix(4), "tel:") {
                written = String(decoding: written.utf8.dropFirst(4), as: UTF8.self)
            }
            let number = VCard.prefix(VCard.trim(written), VCard.numberMost)
            guard !number.isEmpty else { return }
            let whatsapp = line.param("WAID").flatMap { digits in
                (6 ... 20).contains(digits.utf8.count) && digits.utf8.allSatisfy { (0x30 ... 0x39).contains($0) } ? "+" + digits : nil
            }
            phones.append(ContactCard.Phone(number: number, whatsapp: whatsapp))
        }

        /// The card read: named by its formatted name, or failing that its structured one.
        func card() -> Card {
            let named = formatted.flatMap { VCard.said(VCard.unescaped($0.text)) } ?? structured.flatMap { VCard.structured($0.text) }
            return Card(name: named.map { VCard.prefix($0, VCard.nameMost) }, phones: phones, photo: photo)
        }
    }

    /// A photo given inline: base64, as vCard 3.0's `ENCODING=b` and 2.1's `BASE64` give it, or a
    /// `data:` URI with `;base64`, as 4.0's does; none when it's past what's kept, or a URI to fetch,
    /// which is never fetched.
    static func photo(_ line: Line) -> Data? {
        guard !line.cut else { return nil }
        let value = line.value
        var payload = value
        if value.prefix(5).map(lower) == Array("data:".utf8) {
            guard let comma = value.firstIndex(of: UInt8(ascii: ",")),
                  String(decoding: value[value.startIndex ..< comma], as: UTF8.self).lowercased().contains(";base64")
            else { return nil }
            payload = value[(comma + 1)...]
        } else {
            guard line.has("ENCODING", "B") || line.has("ENCODING", "BASE64") else { return nil }
        }
        return Data(base64Encoded: Data(payload), options: .ignoreUnknownCharacters)
    }

    /// Quoted-printable text decoded: each `=` and two hex digits a byte, read as UTF-8.
    static func quotedPrintable(_ text: ArraySlice<UInt8>) -> String {
        func hex(_ byte: UInt8) -> UInt8? {
            switch byte {
            case 0x30 ... 0x39: byte - 0x30
            case 0x41 ... 0x46: byte - 0x41 + 10
            case 0x61 ... 0x66: byte - 0x61 + 10
            default: nil
            }
        }
        var out: [UInt8] = []
        out.reserveCapacity(text.count)
        var at = text.startIndex
        while at < text.endIndex {
            if text[at] == UInt8(ascii: "="), at + 2 < text.endIndex, let high = hex(text[at + 1]), let low = hex(text[at + 2]) {
                out.append(high << 4 | low)
                at += 3
            } else {
                out.append(text[at])
                at += 1
            }
        }
        return String(decoding: out, as: UTF8.self)
    }

    /// A value with vCard's escapes undone: `\n` a new line, and a backslash before anything else that
    /// thing itself.
    static func unescaped(_ value: String) -> String {
        guard value.utf8.contains(UInt8(ascii: "\\")) else { return value }
        var out = String.UnicodeScalarView()
        var scalars = value.unicodeScalars.makeIterator()
        while let c = scalars.next() {
            if c == "\\" {
                switch scalars.next() {
                case "n", "N": out.append("\n")
                case let other?: out.append(other)
                case nil: break
                }
            } else {
                out.append(c)
            }
        }
        return String(out)
    }

    /// Text that says something, trimmed.
    static func said(_ text: String) -> String? {
        let trimmed = trim(text)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// A structured name (family, given, additional, prefixes and suffixes, split at the semicolons
    /// that aren't escaped) as a person reads it: prefixes, given, additional, family, suffixes.
    static func structured(_ value: String) -> String? {
        var parts = [String.UnicodeScalarView()]
        var scalars = value.unicodeScalars.makeIterator()
        while let c = scalars.next() {
            switch c {
            case "\\":
                parts[parts.count - 1].append("\\")
                if let escaped = scalars.next() { parts[parts.count - 1].append(escaped) }
            case ";": parts.append(String.UnicodeScalarView())
            default: parts[parts.count - 1].append(c)
            }
        }
        let part = { (i: Int) in i < parts.count ? trim(unescaped(String(parts[i]))) : "" }
        let name = [part(3), part(1), part(2), part(0), part(4)].filter { !$0.isEmpty }
        return name.isEmpty ? nil : name.joined(separator: " ")
    }

    /// `text` without the white space around it, as Rust's `trim` takes it off.
    static func trim(_ text: String) -> String {
        String(text.unicodeScalars.trimmed)
    }

    /// The names of the properties a card is read for, in lower case.
    static let beginWord = Array("begin".utf8), endWord = Array("end".utf8), fn = Array("fn".utf8), tel = Array("tel".utf8),
               photoWord = Array("photo".utf8)
    /// A byte order mark in UTF-8, which jaw-proto takes off the start of the text and of a
    /// property's name.
    static let bom: [UInt8] = [0xEF, 0xBB, 0xBF]

    /// Whether `bytes` are `word`'s, in any case of their ASCII letters only, as Rust's
    /// `eq_ignore_ascii_case` has it.
    static func same(_ bytes: some Collection<UInt8>, _ word: String) -> Bool {
        bytes.count == word.utf8.count && zip(bytes, word.utf8).allSatisfy { a, b in lower(a) == lower(b) }
    }

    /// Whether `bytes` are `word`, a word in lower case, in any case of their ASCII letters: as `same`,
    /// for the names every line's head is matched against.
    static func same(_ bytes: ArraySlice<UInt8>, _ word: [UInt8]) -> Bool {
        guard bytes.count == word.count else { return false }
        var at = bytes.startIndex
        for letter in word {
            if lower(bytes[at]) != letter { return false }
            at += 1
        }
        return true
    }

    static func lower(_ byte: UInt8) -> UInt8 {
        (0x41 ... 0x5A).contains(byte) ? byte | 0x20 : byte
    }

    /// Whether a byte is ASCII white space.
    static func space(_ byte: UInt8) -> Bool {
        byte == 0x20 || (0x09 ... 0x0D).contains(byte)
    }

    /// How many bytes the white space `bytes` begin with takes in UTF-8, or 0 when they don't begin
    /// with any: Unicode's White_Space, as Rust's `trim` has it, ASCII's and the few past it.
    static func spaceFirst(_ bytes: ArraySlice<UInt8>) -> Int {
        guard let first = bytes.first else { return 0 }
        if first < 0x80 { return space(first) ? 1 : 0 }
        var parser = Unicode.UTF8.ForwardParser()
        var each = bytes.makeIterator()
        guard case let .valid(scalar) = parser.parseScalar(from: &each), Unicode.UTF8.decode(scalar).properties.isWhitespace else { return 0 }
        return scalar.count
    }

    /// How many bytes the white space `bytes` end with takes in UTF-8, or 0.
    static func spaceLast(_ bytes: ArraySlice<UInt8>) -> Int {
        guard let last = bytes.last else { return 0 }
        if last < 0x80 { return space(last) ? 1 : 0 }
        var start = bytes.endIndex - 1
        while start > bytes.startIndex, bytes.endIndex - start < 4, bytes[start] & 0xC0 == 0x80 {
            start -= 1
        }
        let n = spaceFirst(bytes[start...])
        return n == bytes.endIndex - start ? n : 0
    }

    /// `bytes` without the white space around them, as Rust's `trim` takes it off.
    static func trimmed(_ bytes: ArraySlice<UInt8>) -> ArraySlice<UInt8> {
        var out = bytes
        var n = spaceFirst(out)
        while n > 0 {
            out = out.dropFirst(n)
            n = spaceFirst(out)
        }
        n = spaceLast(out)
        while n > 0 {
            out = out.dropLast(n)
            n = spaceLast(out)
        }
        return out
    }

    /// The first `most` characters of `text`, counted as jaw-proto counts them, in Unicode scalars.
    static func prefix(_ text: String, _ most: Int) -> String {
        guard text.unicodeScalars.count > most else { return text }
        var scalars = String.UnicodeScalarView()
        scalars.append(contentsOf: text.unicodeScalars.prefix(most))
        return String(scalars)
    }
}
