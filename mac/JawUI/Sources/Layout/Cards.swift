import AppKit
import CoreText
import JawKit
import JawThemes
import Synchronization

/// SF Symbols as images for the message layout, made once for each symbol, size and colour, so
/// layouts made on the layout queue can draw them.
enum Symbols {
    static let made = Mutex<[String: CGImage]>([:])

    /// A symbol at a point size in a colour, at twice its size for the screen, and its size in points:
    /// its other layers in lighter shades of the colour, or, `on` a colour, solid in that one, as a
    /// tick in `colour` on a disc filled with `on`.
    static func image(_ name: String, _ size: CGFloat, _ colour: RGBA, on fill: RGBA? = nil) -> (CGImage, CGSize)? {
        let key = "\(name) \(size) \(colour.hex) \(colour.a)" + (fill.map { " on \($0.hex) \($0.a)" } ?? "")
        let colours = fill.map { NSImage.SymbolConfiguration(paletteColors: [colour.nsColor, $0.nsColor]) }
            ?? NSImage.SymbolConfiguration(hierarchicalColor: colour.nsColor)
        guard let symbol = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: size, weight: .medium).applying(colours))
        else { return nil }
        let points = symbol.size
        if let image = made.withLock({ $0[key] }) { return (image, points) }
        let scale: CGFloat = 2
        guard let ctx = CGContext(data: nil, width: Int(ceil(points.width * scale)), height: Int(ceil(points.height * scale)), bitsPerComponent: 8,
                                  bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        ctx.scaleBy(x: scale, y: scale)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
        symbol.draw(in: CGRect(origin: .zero, size: points))
        NSGraphicsContext.restoreGraphicsState()
        guard let image = ctx.makeImage() else { return nil }
        made.withLock { cache in
            if cache.count > 400 { cache.removeAll() }
            cache[key] = image
        }
        return (image, points)
    }
}

/// What a message shows besides its words, by its kind: a picture's frame in its shape, a voice
/// message's waveform, a file, a place, a contact, a poll, a link's preview, an album. jawd sends
/// what each is (its size, length, name, place, options); a picture's pixels come with jawd's
/// previews, until when its frame says what it is.
extension MessageLayout.Builder {
    /// Whether the kind is drawn as a card, its words then only its caption.
    var hasCard: Bool {
        switch m.kind {
        case .image, .video, .gif, .sticker, .videoNote, .voice, .audio, .document, .location, .liveLocation, .contact, .poll:
            !m.deleted
        // An album media in's jawd sends as counts, its items as messages of their own, is a line.
        case .album: !m.deleted && !(m.album?.items.isEmpty ?? true)
        case .text: !m.deleted && (m.link.map(Self.shows) ?? false)
        case .system, .call, .event, .invite, .unknown: false
        }
    }

    /// Whether the card says all the words do, as a place, a contact or a poll does.
    var cardHasTheWords: Bool {
        [.location, .contact, .poll].contains(m.kind)
    }

    /// Whether the card comes under the words rather than over them: a link's, whose words have the
    /// link it previews.
    var cardUnderText: Bool {
        m.kind == .text
    }

    /// Whether a link's preview has words for a card: a title or a description.
    static func shows(_ link: LinkPreview) -> Bool {
        !(link.title ?? "").isEmpty || !(link.description ?? "").isEmpty
    }

    /// What VoiceOver says of a message's card beyond its words: a link's title and host, or a poll's
    /// options with their tallies and yours, and how many voted.
    static func spoken(_ m: MessageRow) -> String? {
        guard !m.deleted else { return nil }
        switch m.kind {
        case .text:
            guard let link = m.link, shows(link) else { return nil }
            return "link: " + ([link.title, link.host] as [String?]).compactMap(\.self).filter { !$0.isEmpty }.joined(separator: ", ")
        case .poll:
            guard let poll = m.poll else { return nil }
            let options = poll.options.map { o in "\(o.name), \(Format.num(o.votes)) vote\(o.votes == 1 ? "" : "s")\(o.mine ? ", yours" : "")" }
            return "\(pollRule(poll).lowercased()): \(options.joined(separator: "; ")); \(pollVoted(poll))"
        default:
            return nil
        }
    }

    /// What VoiceOver says a message is, before its card: that it was deleted, or its kind's line and
    /// its words. A poll's kind's line has its question and its card the options, so its words, which
    /// jawd's rows give as the question and the options, a line each, aren't said again; a contact
    /// card is said as its name and numbers, and several as how many and their names.
    static func said(_ m: MessageRow) -> String {
        if m.deleted { return "This message was deleted" }
        if m.kind == .poll, m.poll != nil { return kindLine(m) ?? "" }
        if m.kind == .contact, let contact = m.contact { return ContactWords.spoken(contact) }
        return (kindLine(m).map { "\($0). " } ?? "") + m.plain
    }

    /// The card's size at most `most` wide.
    func cardSize(most: CGFloat) -> CGSize? {
        guard hasCard else { return nil }
        let w = min(most, 280)
        switch m.kind {
        case .image, .video, .gif, .sticker, .videoNote: return pictureSize(most: most)
        case .voice, .audio: return CGSize(width: min(most, 260), height: 40)
        case .document: return CGSize(width: w, height: 44)
        case .contact: return contactSize(most: most)
        case .location, .liveLocation: return CGSize(width: w, height: 52)
        case .poll: return pollSize(most: most)
        case .album: return albumSize(most: most)
        case .text: return linkSize(most: most)
        case .system, .call, .event, .invite, .unknown: return nil
        }
    }

    /// A poll's card: its question on three lines at most, what a vote may choose, each option on a
    /// row of its own, and how many voted. Its height comes from the question and the options alone,
    /// so tallies that change move nothing.
    func pollSize(most: CGFloat) -> CGSize {
        let w = min(most, 300)
        guard let poll = m.poll else { return CGSize(width: w, height: lineHeight(fonts.name)) }
        let question = CGFloat(max(questionLines(poll, width: w).count, 1)) * lineHeight(fonts.name)
        let options = CGFloat(poll.options.count) * optionHeight
        return CGSize(width: w, height: ceil(question + 2 + lineHeight(fonts.small) + 8 + options + lineHeight(fonts.small)))
    }

    /// An option's row: its name with its tick and count, its bar under it, and the space before the next.
    var optionHeight: CGFloat {
        lineHeight(fonts.body) + 3 + 4 + 8
    }

    func questionLines(_ poll: Poll, width: CGFloat) -> [CTLine] {
        wrapped(poll.question, fonts.name, roles.text, width: width, most: 3)
    }

    /// What a vote in the poll may choose, under its question.
    static func pollRule(_ poll: Poll) -> String {
        poll.max <= 1 ? "Choose one" : poll.max >= poll.options.count ? "Choose any" : "Choose up to \(poll.max)"
    }

    /// How many voted, under the options.
    static func pollVoted(_ poll: Poll) -> String {
        poll.voters > 0 ? "\(Format.num(poll.voters)) voted" : "No votes yet"
    }

    /// How much of its bar an option fills: its share of those who voted, or of the most-chosen
    /// option's votes when a jawd says no more than the tallies.
    static func pollShare(_ option: Poll.Option, in poll: Poll) -> CGFloat {
        let total = max(poll.voters, poll.options.map(\.votes).max() ?? 0)
        return total > 0 ? min(CGFloat(option.votes) / CGFloat(total), 1) : 0
    }

    /// A link's thumbnail, square: jawd keeps one 140 pixels on its long side.
    static let thumbnail: CGFloat = 70

    /// A link's card: its thumbnail on the left when it has one, and beside it its title, two lines of
    /// its description at most and its host. Its height comes from the row alone, so a thumbnail read
    /// late moves nothing.
    func linkSize(most: CGFloat) -> CGSize {
        let w = min(most, 320)
        let words = linkWordsHeight(width: linkTextWidth(w)) + 16
        return CGSize(width: w, height: ceil(m.link?.preview == nil ? words : max(words, Self.thumbnail + 12)))
    }

    /// Where a link's words start in its card: after its thumbnail, or after the accent's bar.
    var linkTextInset: CGFloat {
        m.link?.preview == nil ? 12 : 6 + Self.thumbnail + 10
    }

    func linkTextWidth(_ cardWidth: CGFloat) -> CGFloat {
        cardWidth - linkTextInset - 10
    }

    /// The height of a link's words `width` wide: its title, its description's lines and its host.
    func linkWordsHeight(width: CGFloat) -> CGFloat {
        guard let link = m.link else { return 0 }
        let title = (link.title ?? "").isEmpty ? 0 : lineHeight(fonts.name)
        return title + CGFloat(descriptionLines(link, width: width).count) * lineHeight(fonts.small) + lineHeight(fonts.small)
    }

    func descriptionLines(_ link: LinkPreview, width: CGFloat) -> [CTLine] {
        guard let d = link.description, !d.isEmpty else { return [] }
        return wrapped(d, fonts.small, roles.secondary, width: width, most: 2)
    }

    /// `s` as one line, its runs of spaces and newlines each a space, wrapped `width` wide onto `most`
    /// lines at most, the last cut with an ellipsis when there's more.
    func wrapped(_ s: String, _ font: CTFont, _ colour: RGBA, width: CGFloat, most: Int) -> [CTLine] {
        let one = s.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        let attributed = NSAttributedString(string: one, attributes: [.ctFont: font, .ctColour: colour.cgColor])
        let setter = CTFramesetterCreateWithAttributedString(attributed)
        let path = CGPath(rect: CGRect(x: 0, y: 0, width: max(width, 10), height: 100_000), transform: nil)
        let lines = CTFrameGetLines(CTFramesetterCreateFrame(setter, CFRange(location: 0, length: 0), path, nil)) as! [CTLine]
        guard lines.count > most, most > 0 else { return lines }
        let rest = CTLineGetStringRange(lines[most - 1]).location
        return Array(lines.prefix(most - 1)) + [truncated((one as NSString).substring(from: rest), font, colour, width)]
    }

    /// A picture's place in its shape, from the row's width and height before any pixel: a photo, a
    /// video's or a GIF's poster inside 320 by 400 points and 120 at least, cropped from the centre
    /// when its shape is further out than that; a sticker inside 160; a video message a 192-point
    /// circle.
    func pictureSize(most: CGFloat) -> CGSize {
        let aspect = m.media.flatMap { m in m.w.flatMap { w in m.h.flatMap { h in w > 0 && h > 0 ? CGFloat(w) / CGFloat(h) : nil } } }
        switch m.kind {
        case .videoNote:
            let side = min(192, most)
            return CGSize(width: side, height: side)
        case .sticker:
            let side = min(160, most), a = aspect ?? 1
            return a >= 1 ? CGSize(width: side, height: max((side / a).rounded(), 1)) : CGSize(width: max((side * a).rounded(), 1), height: side)
        default:
            let a = aspect ?? 4 / 3
            let widest = min(most, 320)
            var w = widest, h = w / a
            if h > 400 { h = 400; w = h * a }
            w = max(w, min(120, widest))
            h = min(max(h, 120), 400)
            return CGSize(width: w.rounded(), height: h.rounded())
        }
    }

    /// An album's place, fixed by how many items it has: two side by side, three as two across and
    /// one wide below, four or more two by two, the tiles square and 2 points apart, 320 wide.
    func albumSize(most: CGFloat) -> CGSize {
        let w = min(most, 320), side = ((w - 2) / 2).rounded(.down)
        return CGSize(width: w, height: (m.album?.items.count ?? 0) <= 2 ? side : 2 * side + 2)
    }

    /// The album's tiles in `r`: at most four, the fourth saying how many more there are.
    static func albumTiles(_ r: CGRect, count: Int) -> [CGRect] {
        let side = ((r.width - 2) / 2).rounded(.down), right = r.width - side - 2
        let left = CGRect(x: r.minX, y: r.minY, width: side, height: side)
        let beside = CGRect(x: r.minX + side + 2, y: r.minY, width: right, height: side)
        switch count {
        case ...0: return []
        case 1: return [r]
        case 2: return [left, beside]
        case 3: return [left, beside, CGRect(x: r.minX, y: r.minY + side + 2, width: r.width, height: side)]
        default:
            return [left, beside, left.offsetBy(dx: 0, dy: side + 2),
                    CGRect(x: beside.minX, y: beside.minY + side + 2, width: right, height: side)]
        }
    }

    /// Draws the card in `r`.
    mutating func drawCard(_ r: CGRect) {
        switch m.kind {
        case .image, .video, .gif, .sticker, .videoNote: picture(r)
        case .voice: voice(r)
        case .audio:
            voice(r)
            regions.append((r, .media(item: nil)))
        case .document: file(r)
        case .location, .liveLocation: place(r)
        case .contact: contact(r)
        case .poll: poll(r)
        case .album: album(r)
        case .text: linkPreview(r)
        case .system, .call, .event, .invite, .unknown: break
        }
    }

    // MARK: The pieces

    var well: RGBA {
        roles.text.opacity(roles.isDark ? 0.07 : 0.06)
    }

    /// A part under the pictures, or over them while `drawingOver` is set.
    mutating func put(_ part: Laid.Part) {
        if drawingOver { over.append(part) } else { parts.append(part) }
    }

    mutating func symbol(_ name: String, _ size: CGFloat, _ colour: RGBA, on fill: RGBA? = nil, centredIn r: CGRect) {
        guard let (image, s) = Symbols.image(name, size, colour, on: fill) else { return }
        put(.image(image, CGRect(x: (r.midX - s.width / 2).rounded(), y: (r.midY - s.height / 2).rounded(), width: s.width, height: s.height)))
    }

    /// A line of text with its baseline placed so the line sits at `top`.
    mutating func text(_ s: String, _ font: CTFont, _ colour: RGBA, x: CGFloat, top: CGFloat, most: CGFloat) {
        let l = truncated(s, font, colour, most)
        put(.line(l, CGPoint(x: x, y: top + ascent(font))))
    }

    /// A pill of quiet words over a picture's corner: a poster's length and size, "GIF", "HD".
    mutating func pill(_ words: String, in r: CGRect, top: Bool = false, right: Bool = false) {
        let label = line(words, fonts.small, roles.text)
        let lh = lineHeight(fonts.small) + 4
        let w = min(width(of: label) + 12, r.width - 16)
        let pill = CGRect(x: right ? r.maxX - 8 - w : r.minX + 8, y: top ? r.minY + 8 : r.maxY - 8 - lh, width: w, height: lh)
        put(.fill(pill, roles.ground.opacity(0.72).cgColor, lh / 2))
        text(words, fonts.small, roles.text, x: pill.minX + 6, top: pill.minY + 2, most: pill.width - 12)
    }

    /// ▶ on a scrim, at the middle of a video's or a GIF's poster.
    mutating func play(in r: CGRect, size: CGFloat = 44) {
        let circle = CGRect(x: (r.midX - size / 2).rounded(), y: (r.midY - size / 2).rounded(), width: size, height: size)
        put(.fill(circle, roles.ground.opacity(0.6).cgColor, size / 2))
        symbol("play.fill", size * 0.38, roles.text, centredIn: circle.offsetBy(dx: size * 0.04, dy: 0))
    }

    static func bytes(_ n: Int64?) -> String? {
        n.map { ByteCountFormatter.string(fromByteCount: $0, countStyle: .file) }
    }

    /// What a picture is, said over its frame while there's nothing to draw it from: "Video · 0:42 ·
    /// 12 MB".
    var pictureLabel: String {
        let what = switch m.kind {
        case .video: "Video"
        case .gif: "GIF"
        case .videoNote: "Video message"
        case .sticker: "Sticker"
        default: m.viewOnce ? "Photo, view once" : "Photo"
        }
        return ([what, m.media?.seconds.map(Format.length), Self.bytes(m.media?.bytes)] as [String?]).compactMap(\.self).joined(separator: " · ")
    }

    /// A poster's length and size, or "GIF".
    var posterLabel: String? {
        switch m.kind {
        case .gif: return "GIF"
        case .video: return ([m.media?.seconds.map(Format.length), Self.bytes(m.media?.bytes)] as [String?]).compactMap(\.self).joined(separator: " · ")
        case .videoNote: return m.media?.seconds.map(Format.length)
        default: return nil
        }
    }

    /// A photo, a sticker, or a video's, a GIF's or a video message's poster, in its place: the well
    /// where it will be, its slot, drawn from jawd's previews by the cell, and over it what it is.
    mutating func picture(_ r: CGRect) {
        let round = m.kind == .videoNote ? r.width / 2 : m.kind == .sticker ? 0 : Radius.field
        let source = m.viewOnce ? PictureSource() : PictureSource(m.media)
        if m.kind != .sticker || source.isEmpty { parts.append(.fill(r, well.cgColor, round)) }
        drawingOver = true
        defer { drawingOver = false }
        if source.isEmpty {
            let icon = switch m.kind {
            case .video, .gif: "play.circle.fill"
            case .videoNote: "video.fill"
            case .sticker: "face.smiling"
            default: m.viewOnce ? "1.circle" : "photo"
            }
            let words = m.media.flatMap { Self.comingWords($0) }
            if words == nil || m.media?.state == .local {
                symbol(icon, m.kind == .video || m.kind == .gif ? 34 : 26, roles.secondary, centredIn: r)
                if m.kind != .videoNote { pill(pictureLabel, in: r) }
            }
        } else {
            slots.append(PictureSlot(rect: r, radius: round, fit: m.kind == .sticker ? .fit : .fill, source: source, message: m.id))
            let here = m.media.map { $0.state == .local } ?? true
            if [.video, .gif, .videoNote].contains(m.kind), here { play(in: r, size: m.kind == .videoNote ? 40 : 44) }
            if let label = posterLabel, !label.isEmpty {
                if m.kind == .videoNote {
                    let l = line(label, fonts.small, roles.text)
                    let lh = lineHeight(fonts.small) + 4, w = width(of: l) + 12
                    let pill = CGRect(x: (r.midX - w / 2).rounded(), y: r.maxY - 14 - lh, width: w, height: lh)
                    put(.fill(pill, roles.ground.opacity(0.72).cgColor, lh / 2))
                    text(label, fonts.small, roles.text, x: pill.minX + 6, top: pill.minY + 2, most: w)
                } else {
                    pill(label, in: r)
                }
            }
            if m.media?.hd == true, m.kind == .image { pill("HD", in: r, top: true, right: true) }
        }
        if !m.viewOnce {
            coming(r, m.media, item: nil, small: m.kind == .sticker || m.kind == .videoNote)
            regions.append((r, .media(item: nil)))
        }
    }

    /// What a file's state says over its picture, and what a click there does: ↓ and its size while
    /// it's remote; a ring filling as it comes, with × to stop it; why it can't come, in jawd's
    /// words, with "Try again" or "Ask the phone again"; "Waiting for your phone"; nothing once it's
    /// here. `small` is a tile's or a sticker's room, which says what's done without why.
    mutating func coming(_ r: CGRect, _ media: Media?, item: Int?, small: Bool) {
        guard let media, media.state != .local else { return }
        let side: CGFloat = small ? 36 : 44
        let circle = CGRect(x: (r.midX - side / 2).rounded(), y: (r.midY - side / 2).rounded(), width: side, height: side)
        put(.fill(circle, roles.ground.opacity(0.6).cgColor, side / 2))
        switch media.state {
        case .downloading:
            let centre = CGPoint(x: circle.midX, y: circle.midY), radius = side / 2 - 4
            put(.ring(centre, radius, 1, roles.text.opacity(0.25).cgColor, 3))
            put(.ring(centre, radius, max(media.progress ?? 0, 0.02), roles.text.cgColor, 3))
            symbol("xmark", side * 0.3, roles.text, centredIn: circle)
            regions.append((circle, .stop(item: item)))
        case .remote where media.error == nil:
            symbol("arrow.down", side * 0.38, roles.text, centredIn: circle)
            if let bytes = Self.bytes(media.bytes), !small, m.kind == .image || m.kind == .album { pill(bytes, in: r) }
        case .local:
            break
        default:
            symbol(media.state == .waitingForPhone ? "iphone" : media.state == .gone ? "exclamationmark" : "arrow.clockwise",
                   side * 0.38, roles.text, centredIn: circle)
            if let words = Self.comingWords(media, short: small) { band(words, in: r) }
        }
    }

    /// Why a file isn't here and what can be done, as its card says it: jawd's words, then the act.
    static func comingWords(_ media: Media, short: Bool = false) -> (why: String?, act: String?)? {
        switch media.state {
        case .local, .downloading: return nil
        case .remote where media.error == nil: return nil
        case .remote: return (short ? nil : media.error, "Try again")
        case .waitingForPhone: return ("Waiting for your phone", nil)
        case .expired: return (short ? nil : media.error ?? "WhatsApp no longer has this file", "Ask the phone again")
        case .gone: return (short ? nil : media.error ?? "your phone no longer has this file", "Ask the phone again")
        }
    }

    /// A band of words across a picture's foot, on a scrim: why, quiet, then what can be done, in the
    /// accent.
    mutating func band(_ words: (why: String?, act: String?), in r: CGRect) {
        let lh = lineHeight(fonts.small)
        let band = CGRect(x: r.minX, y: r.maxY - lh - 10, width: r.width, height: lh + 10)
        put(.fill(band, roles.ground.opacity(0.78).cgColor, 0))
        let said = whyAndAct(words, colour: roles.text, most: band.width - 16)
        let w = width(of: said)
        put(.line(said, CGPoint(x: (band.midX - w / 2).rounded(), y: band.minY + 5 + ascent(fonts.small))))
    }

    /// Why, cut short at its end to leave room, then what can be done, always whole, in the accent.
    func whyAndAct(_ words: (why: String?, act: String?), colour: RGBA, most: CGFloat) -> CTLine {
        let act = words.act.map { NSAttributedString(string: $0, attributes: [.ctFont: fonts.small, .ctColour: roles.accent.cgColor]) }
        let dot = NSAttributedString(string: " · ", attributes: [.ctFont: fonts.small, .ctColour: roles.secondary.cgColor])
        let room = most - (act.map { width(of: CTLineCreateWithAttributedString($0)) + width(of: CTLineCreateWithAttributedString(dot)) } ?? 0)
        let why = words.why.map { Self.fitted($0, room) { width(of: line($0, fonts.small, colour)) } } ?? ""
        let out = NSMutableAttributedString(string: why, attributes: [.ctFont: fonts.small, .ctColour: colour.cgColor])
        if let act {
            if !why.isEmpty { out.append(dot) }
            out.append(act)
        }
        return CTLineCreateWithAttributedString(out)
    }

    /// `s` as it fits in `room`: whole, or cut at its end with an ellipsis, or nothing.
    static func fitted(_ s: String, _ room: CGFloat, measure: (String) -> CGFloat) -> String {
        guard measure(s) > room else { return s }
        var chars = Array(s)
        while !chars.isEmpty, measure(String(chars) + "…") > room {
            chars.removeLast()
        }
        let cut = String(chars).trimmingCharacters(in: .whitespaces)
        return cut.count < 4 ? "" : cut + "…"
    }

    /// A sticker or a video message on its own, as WhatsApp draws them: no bubble, the time under
    /// it. One that quotes a message has a bubble for the quote.
    mutating func alone() -> Laid {
        let size = pictureSize(most: width - 2 * metrics.margin)
        var top = y
        if !m.mine, context.group, context.firstOfRun, let from = m.from {
            parts.append(.line(line(from.name, fonts.name, roles.sender(from.colour)), CGPoint(x: metrics.margin, y: top + ascent(fonts.name))))
            top += lineHeight(fonts.name) + 2
        }
        let x = m.mine ? width - metrics.margin - size.width : metrics.margin
        let r = CGRect(x: x, y: top, width: size.width, height: size.height)
        drawCard(r)
        y = r.maxY + 2
        let foot = footer()
        let fw = width(of: foot)
        parts.append(contentsOf: footParts(foot, at: CGPoint(x: m.mine ? width - metrics.margin - fw : x, y: y + ascent(fonts.time))))
        y += lineHeight(fonts.time)
        reactions(x: x, alignRight: m.mine, right: r.maxX)
        failedLine(x: x, alignRight: m.mine, right: r.maxX)
        return Laid(height: ceil(y + 1), body: r, parts: parts, regions: regions, text: nil, pictures: slots, over: over)
    }

    mutating func voice(_ r: CGRect) {
        let circle = CGRect(x: r.minX, y: r.midY - 16, width: 32, height: 32)
        parts.append(.fill(circle, roles.accent.cgColor, 16))
        symbol(m.kind == .voice ? "play.fill" : "music.note", 12, roles.isDark ? roles.ground : RGBA(0xFFFFFF), centredIn: circle)
        let seconds = m.media?.seconds ?? m.voice?.seconds
        let length = line(seconds.map(Format.length) ?? "", fonts.time, roles.secondary)
        let lw = width(of: length)
        parts.append(.line(length, CGPoint(x: r.maxX - lw, y: r.midY + ascent(fonts.time) / 2 - 1)))
        let left = circle.maxX + 8, right = r.maxX - lw - 8
        let count = max(Int((right - left) / 4), 0)
        let wave = m.voice?.waveform ?? []
        let seed = m.id.unicodeScalars.reduce(UInt32(7)) { $0 &* 31 &+ $1.value }
        for i in 0 ..< count {
            let level: Double = wave.isEmpty
                ? 0.25 + 0.5 * Double((seed &+ UInt32(i) &* 2_654_435_761) >> 27) / 31
                : Double(wave[i * wave.count / max(count, 1)]) / 100
            let h = max(3, CGFloat(level) * 22)
            let bar = CGRect(x: left + CGFloat(i) * 4, y: r.midY - h / 2, width: 2, height: h)
            parts.append(.fill(bar, roles.secondary.opacity(wave.isEmpty ? 0.35 : 0.7).cgColor, 1))
        }
    }

    mutating func file(_ r: CGRect) {
        let icon = CGRect(x: r.minX, y: r.minY + 2, width: 32, height: 40)
        parts.append(.fill(icon, roles.accent.opacity(0.15).cgColor, 6))
        let mime = m.media?.mime ?? ""
        let name = m.media?.name ?? "Document"
        let ext = (name as NSString).pathExtension.uppercased()
        let symbolName = mime.contains("pdf") ? "doc.richtext" : mime.contains("zip") ? "doc.zipper" : mime.hasPrefix("image") ? "photo"
            : mime.hasPrefix("audio") ? "music.note" : mime.hasPrefix("video") ? "film" : "doc.text"
        symbol(symbolName, 15, roles.accent, centredIn: icon)
        let x = icon.maxX + 10
        let state = m.media?.state ?? .local
        let action = CGRect(x: r.maxX - 28, y: r.midY - 14, width: 28, height: 28)
        let right = state == .local ? r.maxX : action.minX - 8
        text(name, fonts.name, roles.text, x: x, top: r.minY + 4, most: right - x)
        let pages = m.media?.pages.map { "\($0) page\($0 == 1 ? "" : "s")" }
        let detail = ([ext.isEmpty ? nil : ext, Self.bytes(m.media?.bytes), pages] as [String?]).compactMap(\.self).joined(separator: " · ")
        let second = r.minY + 6 + lineHeight(fonts.name)
        if let media = m.media, let words = Self.comingWords(media) {
            parts.append(.line(whyAndAct(words, colour: roles.secondary, most: right - x), CGPoint(x: x, y: second + ascent(fonts.small))))
        } else {
            text(detail.isEmpty ? "a file" : detail, fonts.small, roles.secondary, x: x, top: second, most: right - x)
        }
        if state != .local {
            parts.append(.fill(action, roles.accent.opacity(0.15).cgColor, 14))
            switch state {
            case .downloading:
                let centre = CGPoint(x: action.midX, y: action.midY)
                parts.append(.ring(centre, 11, 1, roles.accent.opacity(0.25).cgColor, 2.5))
                parts.append(.ring(centre, 11, max(m.media?.progress ?? 0, 0.02), roles.accent.cgColor, 2.5))
                symbol("xmark", 9, roles.accent, centredIn: action)
                regions.append((action, .stop(item: nil)))
            case .remote where m.media?.error == nil:
                symbol("arrow.down", 11, roles.accent, centredIn: action)
            default:
                symbol(state == .waitingForPhone ? "iphone" : state == .gone ? "exclamationmark" : "arrow.clockwise", 11, roles.accent, centredIn: action)
            }
        }
        regions.append((r, .media(item: nil)))
    }

    mutating func place(_ r: CGRect) {
        let circle = CGRect(x: r.minX, y: r.midY - 18, width: 36, height: 36)
        parts.append(.fill(circle, roles.accent.opacity(0.15).cgColor, 18))
        symbol(m.kind == .liveLocation ? "location.fill" : "mappin.and.ellipse", 15, roles.accent, centredIn: circle)
        let x = circle.maxX + 10
        let l = m.location
        let coords = l.flatMap { l in l.lat.flatMap { lat in l.lon.map { String(format: "%.4f, %.4f", lat, $0) } } }
        let title = m.kind == .liveLocation ? "Live location" : l?.name ?? l?.address ?? "Location"
        let detail = m.kind == .liveLocation ? coords : (l?.name != nil ? l?.address : nil) ?? coords
        text(title, fonts.name, roles.text, x: x, top: r.minY + 7, most: r.maxX - x)
        if let detail { text(detail, fonts.small, roles.secondary, x: x, top: r.minY + 9 + lineHeight(fonts.name), most: r.maxX - x) }
        if let l, let lat = l.lat, let lon = l.lon {
            let q = (l.name ?? "").addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
            regions.append((r, .open("https://maps.apple.com/?ll=\(lat),\(lon)&q=\(q)")))
        } else if let url = l?.url {
            regions.append((r, .open(url)))
        }
    }

    mutating func poll(_ r: CGRect) {
        guard let poll = m.poll else { return }
        var top = r.minY
        for question in questionLines(poll, width: r.width) {
            parts.append(.line(question, CGPoint(x: r.minX, y: top + ascent(fonts.name))))
            top += lineHeight(fonts.name)
        }
        top += 2
        text(Self.pollRule(poll), fonts.small, roles.secondary, x: r.minX, top: top, most: r.width)
        top += lineHeight(fonts.small) + 8
        let several = poll.max > 1
        let x = r.minX + 24
        for option in poll.options {
            regions.append((CGRect(x: r.minX, y: top - 4, width: r.width, height: optionHeight), .vote(option.name)))
            let tick = CGRect(x: r.minX, y: top, width: 16, height: lineHeight(fonts.body))
            if option.mine {
                let onAccent = roles.isDark ? roles.ground : RGBA(0xFFFFFF)
                symbol(several ? "checkmark.square.fill" : "checkmark.circle.fill", 14, onAccent, on: roles.accent, centredIn: tick)
            } else {
                symbol(several ? "square" : "circle", 14, roles.tertiary, centredIn: tick)
            }
            let count = line(Format.num(option.votes), fonts.time, roles.secondary)
            let cw = width(of: count)
            text(option.name, fonts.body, roles.text, x: x, top: top, most: r.maxX - x - cw - 8)
            parts.append(.line(count, CGPoint(x: r.maxX - cw, y: top + ascent(fonts.body))))
            let track = CGRect(x: x, y: top + lineHeight(fonts.body) + 3, width: r.maxX - x, height: 4)
            parts.append(.fill(track, well.cgColor, 2))
            let share = Self.pollShare(option, in: poll)
            if share > 0 {
                parts.append(.fill(CGRect(x: track.minX, y: track.minY, width: max(4, track.width * share), height: 4),
                                   (option.mine ? roles.accent : roles.secondary.opacity(0.6)).cgColor, 2))
            }
            top += optionHeight
        }
        text(Self.pollVoted(poll), fonts.small, roles.tertiary, x: r.minX, top: top, most: r.width)
    }

    /// An album's tiles, each filled and cropped from the centre, a video's with ▶, the fourth saying
    /// how many more there are.
    mutating func album(_ r: CGRect) {
        let items = m.album?.items ?? []
        for (i, tile) in Self.albumTiles(r, count: items.count).enumerated() {
            let item = items[i]
            let source = PictureSource(item.media)
            parts.append(.fill(tile, well.cgColor, Radius.small))
            drawingOver = true
            if source.isEmpty {
                symbol(item.kind == "video" ? "play.circle.fill" : "photo", 20, roles.secondary, centredIn: tile)
            } else {
                slots.append(PictureSlot(rect: tile, radius: Radius.small, source: source, message: m.id, item: i))
            }
            if i == 3, items.count > 4 {
                put(.fill(tile, roles.ground.opacity(0.55).cgColor, Radius.small))
                let more = line("+\(items.count - 3)", fonts.emoji, roles.text)
                let w = width(of: more)
                put(.line(more, CGPoint(x: (tile.midX - w / 2).rounded(), y: (tile.midY + ascent(fonts.emoji) / 2 - 4).rounded())))
            } else if item.kind == "video", !source.isEmpty, (item.media?.state ?? .local) == .local {
                play(in: tile, size: 34)
            }
            if !(i == 3 && items.count > 4) { coming(tile, item.media, item: i, small: true) }
            drawingOver = false
            regions.append((tile, .media(item: i)))
        }
    }

    mutating func linkPreview(_ r: CGRect) {
        guard let link = m.link else { return }
        parts.append(.fill(r, well.cgColor, Radius.control))
        if let path = link.preview {
            // Its well until the image queue has it, and where it can't be read: jawd keeps it as the
            // JPEG it came as, so nothing else is decoded.
            let thumb = CGRect(x: r.minX + 6, y: (r.midY - Self.thumbnail / 2).rounded(), width: Self.thumbnail, height: Self.thumbnail)
            parts.append(.fill(thumb, well.cgColor, Radius.small))
            symbol("link", 18, roles.secondary, centredIn: thumb)
            slots.append(PictureSlot(rect: thumb, radius: Radius.small, source: PictureSource(small: path, jpeg: true), message: m.id))
        } else {
            parts.append(.fill(CGRect(x: r.minX, y: r.minY, width: 3, height: r.height), roles.accent.cgColor, 1.5))
        }
        let x = r.minX + linkTextInset, most = linkTextWidth(r.width)
        var top = r.minY + ((r.height - linkWordsHeight(width: most)) / 2).rounded()
        if let title = link.title, !title.isEmpty {
            text(title, fonts.name, roles.text, x: x, top: top, most: most)
            top += lineHeight(fonts.name)
        }
        for line in descriptionLines(link, width: most) {
            parts.append(.line(line, CGPoint(x: x, y: top + ascent(fonts.small))))
            top += lineHeight(fonts.small)
        }
        text(link.host, fonts.small, roles.tertiary, x: x, top: top, most: most)
        regions.append((r, .open(link.url)))
    }
}
