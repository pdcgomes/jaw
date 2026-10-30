import AppKit
import CoreText
import JawKit
import JawThemes
import Synchronization

/// SF Symbols as images for the message layout, made once for each symbol, size and colour, so
/// layouts made on the layout queue can draw them.
enum Symbols {
    static let made = Mutex<[String: CGImage]>([:])

    /// A symbol at a point size in a colour, at twice its size for the screen, and its size in points.
    static func image(_ name: String, _ size: CGFloat, _ colour: RGBA) -> (CGImage, CGSize)? {
        let key = "\(name) \(size) \(colour.hex) \(colour.a)"
        guard let symbol = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: size, weight: .medium).applying(.init(hierarchicalColor: colour.nsColor)))
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
        case .image, .video, .gif, .sticker, .videoNote, .voice, .audio, .document, .location, .liveLocation, .contact, .poll, .album:
            !m.deleted
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

    /// What VoiceOver says of a message's card beyond its words: a link's title and host.
    static func spoken(_ m: MessageRow) -> String? {
        guard !m.deleted, m.kind == .text, let link = m.link, shows(link) else { return nil }
        return "link: " + ([link.title, link.host] as [String?]).compactMap(\.self).filter { !$0.isEmpty }.joined(separator: ", ")
    }

    /// The card's size at most `most` wide.
    func cardSize(most: CGFloat) -> CGSize? {
        guard hasCard else { return nil }
        let w = min(most, 280)
        switch m.kind {
        case .image, .video, .gif, .sticker, .videoNote: return pictureSize(most: most)
        case .voice, .audio: return CGSize(width: min(most, 260), height: 40)
        case .document, .contact: return CGSize(width: w, height: 44)
        case .location, .liveLocation: return CGSize(width: w, height: 52)
        case .poll:
            let options = CGFloat(max(m.poll?.options.count ?? 0, 1))
            return CGSize(width: min(most, 300), height: lineHeight(fonts.name) + 6 + options * (lineHeight(fonts.body) + 9) + lineHeight(fonts.small) + 4)
        case .album:
            let n = min(m.album?.items.count ?? 1, 4)
            let side = (min(most, 260) - 4) / 2
            return CGSize(width: min(most, 260), height: CGFloat((n + 1) / 2) * (side + 4) - 4)
        case .text: return linkSize(most: most)
        case .system, .call, .event, .invite, .unknown: return nil
        }
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

    /// A picture's frame in its shape, from its width and height.
    func pictureSize(most: CGFloat) -> CGSize {
        switch m.kind {
        case .videoNote: return CGSize(width: 160, height: 160)
        case .sticker: return CGSize(width: 140, height: 140)
        default:
            let aspect = m.media.flatMap { m in m.w.flatMap { w in m.h.map { CGFloat(w) / CGFloat(max($0, 1)) } } } ?? 4 / 3
            let widest = min(most, 280)
            var w = widest, h = w / aspect
            if h > 320 { h = 320; w = h * aspect }
            if h < 90 { h = 90; w = min(widest, h * aspect) }
            return CGSize(width: max(w.rounded(), 120), height: h.rounded())
        }
    }

    /// Draws the card in `r`.
    mutating func drawCard(_ r: CGRect) {
        switch m.kind {
        case .image, .video, .gif, .sticker, .videoNote: picture(r)
        case .voice, .audio: voice(r)
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

    mutating func symbol(_ name: String, _ size: CGFloat, _ colour: RGBA, centredIn r: CGRect) {
        guard let (image, s) = Symbols.image(name, size, colour) else { return }
        parts.append(.image(image, CGRect(x: (r.midX - s.width / 2).rounded(), y: (r.midY - s.height / 2).rounded(), width: s.width, height: s.height)))
    }

    /// A line of text with its baseline placed so the line sits at `top`.
    mutating func text(_ s: String, _ font: CTFont, _ colour: RGBA, x: CGFloat, top: CGFloat, most: CGFloat) {
        let l = truncated(s, font, colour, most)
        parts.append(.line(l, CGPoint(x: x, y: top + ascent(font))))
    }

    static func bytes(_ n: Int64?) -> String? {
        n.map { ByteCountFormatter.string(fromByteCount: $0, countStyle: .file) }
    }

    /// What a picture is, said over its frame until jawd sends its preview: "Video · 0:42 · 12 MB".
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

    mutating func picture(_ r: CGRect) {
        let round = m.kind == .videoNote ? r.width / 2 : Radius.field
        parts.append(.fill(r, well.cgColor, round))
        let icon = switch m.kind {
        case .video, .gif: "play.circle.fill"
        case .videoNote: "video.fill"
        case .sticker: "face.smiling"
        default: m.viewOnce ? "1.circle" : "photo"
        }
        symbol(icon, m.kind == .video || m.kind == .gif ? 34 : 26, roles.secondary, centredIn: r)
        let label = line(pictureLabel, fonts.small, roles.text)
        let lw = width(of: label)
        let lh = lineHeight(fonts.small) + 4
        let pill = CGRect(x: r.minX + 8, y: r.maxY - 8 - lh, width: min(lw + 12, r.width - 16), height: lh)
        if m.kind != .videoNote {
            parts.append(.fill(pill, roles.ground.opacity(0.7).cgColor, lh / 2))
            text(pictureLabel, fonts.small, roles.text, x: pill.minX + 6, top: pill.minY + 2, most: pill.width - 12)
        }
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
        text(name, fonts.name, roles.text, x: x, top: r.minY + 4, most: r.maxX - x)
        let pages = m.media?.pages.map { "\($0) page\($0 == 1 ? "" : "s")" }
        let detail = ([ext.isEmpty ? nil : ext, Self.bytes(m.media?.bytes), pages] as [String?]).compactMap(\.self).joined(separator: " · ")
        text(detail.isEmpty ? "a file" : detail, fonts.small, roles.secondary, x: x, top: r.minY + 6 + lineHeight(fonts.name), most: r.maxX - x)
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

    mutating func contact(_ r: CGRect) {
        symbol("person.crop.circle.fill", 28, roles.secondary, centredIn: CGRect(x: r.minX, y: r.minY, width: 36, height: r.height))
        let x = r.minX + 44
        text(m.contact?.name ?? "Contact", fonts.name, roles.text, x: x, top: r.minY + 4, most: r.maxX - x)
        text("Contact card · open it on your phone", fonts.small, roles.secondary, x: x, top: r.minY + 6 + lineHeight(fonts.name), most: r.maxX - x)
    }

    mutating func poll(_ r: CGRect) {
        guard let poll = m.poll else { return }
        var top = r.minY
        text(poll.question, fonts.name, roles.text, x: r.minX, top: top, most: r.width)
        top += lineHeight(fonts.name) + 6
        let most = max(poll.options.map(\.votes).max() ?? 0, 1)
        for option in poll.options {
            let count = line("\(option.votes)", fonts.time, roles.secondary)
            let cw = width(of: count)
            text(option.name, fonts.body, roles.text, x: r.minX, top: top, most: r.width - cw - 8)
            parts.append(.line(count, CGPoint(x: r.maxX - cw, y: top + ascent(fonts.body))))
            top += lineHeight(fonts.body) + 2
            let track = CGRect(x: r.minX, y: top, width: r.width, height: 4)
            parts.append(.fill(track, well.cgColor, 2))
            let share = CGFloat(option.votes) / CGFloat(most)
            if option.votes > 0 {
                parts.append(.fill(CGRect(x: track.minX, y: track.minY, width: max(4, track.width * share), height: 4),
                                   (option.mine ? roles.accent : roles.secondary.opacity(0.6)).cgColor, 2))
            }
            top += 7
        }
        let rule = poll.max > 1 ? "choose up to \(poll.max)" : "choose one"
        text("Poll · \(rule) · vote on your phone", fonts.small, roles.tertiary, x: r.minX, top: top + 2, most: r.width)
    }

    mutating func album(_ r: CGRect) {
        let n = m.album?.items.count ?? 1
        let side = (r.width - 4) / 2
        for i in 0 ..< min(n, 4) {
            let tile = CGRect(x: r.minX + CGFloat(i % 2) * (side + 4), y: r.minY + CGFloat(i / 2) * (side + 4), width: side, height: side)
            parts.append(.fill(tile, well.cgColor, Radius.control))
            if i == 3, n > 4 {
                let more = line("+\(n - 3)", fonts.name, roles.text)
                let w = width(of: more)
                parts.append(.line(more, CGPoint(x: tile.midX - w / 2, y: tile.midY + ascent(fonts.name) / 2 - 1)))
            } else {
                symbol(m.album?.items[i].kind == "video" ? "play.circle.fill" : "photo", 20, roles.secondary, centredIn: tile)
            }
        }
    }

    mutating func linkPreview(_ r: CGRect) {
        guard let link = m.link else { return }
        parts.append(.fill(r, well.cgColor, Radius.control))
        if let path = link.preview {
            let thumb = CGRect(x: r.minX + 6, y: (r.midY - Self.thumbnail / 2).rounded(), width: Self.thumbnail, height: Self.thumbnail)
            if let image = pictures?.picture(path, pixels: Int(Self.thumbnail * 2)) {
                parts.append(.picture(image, thumb, Radius.small))
            } else {
                parts.append(.fill(thumb, well.cgColor, Radius.small))
                symbol("link", 18, roles.secondary, centredIn: thumb)
            }
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
