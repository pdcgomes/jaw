import AppKit
import CoreText
import JawKit
import JawThemes

/// What a library element draws, from its value, in its own coordinates from its top left: what's
/// under its picture, the picture's slot, and what's over it. A cell draws it, and the tests' pictures
/// are drawn from the same parts.
struct LibraryDrawn {
    var under: [Laid.Part] = []
    var slot: PictureSlot?
    var over: [Laid.Part] = []

    /// Draws into a flipped context, the picture from `picture` when it has one.
    func draw(in ctx: CGContext, picture: CGImage?) {
        Laid.draw(under, in: ctx, y: 0)
        if let slot, let picture { Laid.draw(picture, in: slot, ctx: ctx, y: 0) }
        Laid.draw(over, in: ctx, y: 0)
    }
}

/// How each of the library's elements is drawn: a heading with its count, a tile with its picture,
/// a video's length and its download's state, a row for a voice message, a file or a link, and the
/// words said where there's nothing.
enum LibraryDrawing {
    /// What every element is drawn with: the style, when it's drawn, and whether the pane is one
    /// chat's, so an item needn't say where it was shared.
    struct Context {
        let style: Style
        let now: Int64
        let tz: TimeZone
        let scoped: Bool

        var roles: Roles {
            style.roles
        }

        var metrics: LibraryLayout.Metrics {
            LibraryLayout.Metrics(style)
        }
    }

    static func drawn(_ what: LibraryLaid.Element.What, item: LibraryRow?, size: CGSize, selected: Bool, _ c: Context) -> LibraryDrawn {
        switch what {
        case let .heading(title, count, kind): heading(title, count: count, clickable: kind != nil, size: size, c)
        case .tile: item.map { tile($0, size: size, selected: selected, c) } ?? LibraryDrawn()
        case .row: item.map { row($0, size: size, selected: selected, c) } ?? LibraryDrawn()
        case let .said(words): said(words, size: size, c)
        }
    }

    // MARK: Text

    static func font(_ style: TextStyle, emphasised: Bool = false) -> CTFont {
        (emphasised ? style.emphasised : style.nsFont) as CTFont
    }

    static func line(_ s: String, _ font: CTFont, _ colour: RGBA) -> CTLine {
        CTLineCreateWithAttributedString(NSAttributedString(string: s, attributes: [.ctFont: font, .ctColour: colour.cgColor]))
    }

    static func width(_ line: CTLine) -> CGFloat {
        CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) - CGFloat(CTLineGetTrailingWhitespaceWidth(line))
    }

    /// `line` cut short at its end to fit `most`.
    static func fit(_ line: CTLine, _ most: CGFloat, _ font: CTFont, _ colour: RGBA) -> CTLine {
        guard width(line) > most else { return line }
        return CTLineCreateTruncatedLine(line, Double(max(most, 1)), .end, Self.line("…", font, colour)) ?? line
    }

    /// The baseline that centres a line of `font` on `mid`.
    static func baseline(_ font: CTFont, mid: CGFloat) -> CGFloat {
        (mid + (CTFontGetAscent(font) - CTFontGetDescent(font)) / 2).rounded()
    }

    /// Who shared it, and where when the pane is every chat's: "Ana in Lisbon Climbing", "You".
    static func who(_ item: LibraryRow, _ c: Context) -> String {
        let name = item.mine ? "You" : item.from?.name ?? ""
        guard !c.scoped, !item.chatName.isEmpty else { return name }
        // A one-to-one chat is the person's, so their name says where.
        if !item.mine, item.chat.hasSuffix("@s.whatsapp.net") || item.chat.hasSuffix("@lid") { return name.isEmpty ? item.chatName : name }
        return name.isEmpty ? item.chatName : "\(name) in \(item.chatName)"
    }

    static func when(_ item: LibraryRow, _ c: Context) -> String {
        Format.when(item.at, now: c.now, tz: c.tz)
    }

    /// An item as VoiceOver says it, in the words it's drawn with: what it is, its name, title or
    /// length, how its file is when it isn't here, a file's type, size and pages or a link's host,
    /// who shared it where, and when.
    static func spoken(_ item: LibraryRow, _ c: Context) -> String {
        let f = item.file, l = item.link
        let what: [String?] = switch item.kind {
        case .photo: ["Photo", item.media.flatMap(MessageView.said)]
        case .video: ["Video", item.media?.seconds.map(Format.length), item.media.flatMap(MessageView.said)]
        case .voice: ["Voice message", item.voice?.seconds.map(Format.length)]
        case .file: [f?.name.isEmpty == false ? f?.name : "a file", coming(f), f?.type, f?.bytes.map(Self.size), f?.pages.map(Self.pages)]
        case .link: [l?.title.flatMap { $0.isEmpty ? nil : $0 } ?? l?.url ?? "Link", l?.host]
        case .other: ["Item"]
        }
        return (what + [who(item, c), when(item, c)]).compactMap(\.self).filter { !$0.isEmpty }.joined(separator: ", ")
    }

    /// A file's size, as its row says it: "2.4 MB".
    static func size(_ bytes: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }

    /// A document's pages, as its row says them: "12 pages".
    static func pages(_ n: Int) -> String {
        "\(n) page\(n == 1 ? "" : "s")"
    }

    // MARK: Elements

    static func heading(_ title: String, count: String?, clickable: Bool, size: CGSize, _ c: Context) -> LibraryDrawn {
        var d = LibraryDrawn()
        let r = c.roles
        let bold = font(.callout, emphasised: true), quiet = font(.callout)
        let base = (size.height - Space.s).rounded() - CTFontGetDescent(bold).rounded()
        let label = line(title, bold, r.secondary)
        var x = width(label)
        d.under.append(.line(label, CGPoint(x: 0, y: base)))
        if let count {
            let n = line(" · \(count)", quiet, r.tertiary)
            d.under.append(.line(n, CGPoint(x: x, y: base)))
            x += width(n)
        }
        if clickable, let (image, s) = Symbols.image("chevron.right", 9, r.tertiary) {
            d.under.append(.image(image, CGRect(x: (x + Space.xs).rounded(), y: (base - CTFontGetAscent(bold) / 2 - s.height / 2 - 1).rounded(),
                                                width: s.width, height: s.height)))
            x += Space.xs + s.width
        }
        let ruleX = x + Space.m
        if ruleX < size.width {
            d.under.append(.fill(CGRect(x: ruleX, y: (base - CTFontGetAscent(bold) / 2).rounded(), width: size.width - ruleX, height: Stroke.hairline),
                                 r.separator.opacity(0.7).cgColor, 0))
        }
        return d
    }

    static func said(_ words: String, size: CGSize, _ c: Context) -> LibraryDrawn {
        let f = font(.callout)
        let l = fit(line(words, f, c.roles.tertiary), size.width, f, c.roles.tertiary)
        return LibraryDrawn(under: [.line(l, CGPoint(x: ((size.width - width(l)) / 2).rounded(), y: baseline(f, mid: size.height / 2)))])
    }

    static func well(_ c: Context) -> RGBA {
        c.roles.text.opacity(c.roles.isDark ? 0.07 : 0.06)
    }

    /// A photo's or a video's tile: its well, its picture, filled from the centre, from jawd's
    /// previews, a video's length over it, how its file is when it isn't here, and the selection's
    /// ring.
    static func tile(_ item: LibraryRow, size: CGSize, selected: Bool, _ c: Context) -> LibraryDrawn {
        var d = LibraryDrawn()
        let r = c.roles, radius = c.metrics.radius
        let rect = CGRect(origin: .zero, size: size)
        d.under.append(.fill(rect, well(c).cgColor, radius))
        let source = PictureSource(item.media)
        if source.isEmpty {
            symbol(item.kind == .video ? "play.rectangle" : "photo", 20, r.tertiary, centredIn: rect, into: &d.under)
        } else {
            d.slot = PictureSlot(rect: rect, radius: radius, fit: .fill, source: source, message: item.id)
        }
        if let badge = badge(item) { pill(badge, in: rect, c, into: &d.over) }
        if let media = item.media, media.state != .local { state(media, in: rect, c, into: &d.over) }
        if selected {
            d.over.append(.stroke(rect.insetBy(dx: 1, dy: 1), r.accent.cgColor, max(radius - 1, 0)))
            d.over.append(.stroke(rect.insetBy(dx: 2, dy: 2), r.accent.cgColor, max(radius - 2, 0)))
        }
        return d
    }

    /// What a tile says over its foot: a video's ▶ and its length.
    static func badge(_ item: LibraryRow) -> String? {
        guard item.kind == .video else { return nil }
        return (["▶", item.media?.seconds.map(Format.length)] as [String?]).compactMap(\.self).joined(separator: " ")
    }

    /// A pill of quiet words over a tile's foot: a video's length.
    static func pill(_ words: String, in r: CGRect, _ c: Context, into parts: inout [Laid.Part]) {
        let f = font(.micro)
        let l = line(words, f, c.roles.text)
        let h = ceil(CTFontGetAscent(f) + CTFontGetDescent(f)) + 4
        let w = min(width(l) + 10, r.width - 8)
        let pill = CGRect(x: r.minX + 4, y: r.maxY - 4 - h, width: w, height: h)
        parts.append(.fill(pill, c.roles.ground.opacity(0.72).cgColor, h / 2))
        parts.append(.line(fit(l, w - 10, f, c.roles.text), CGPoint(x: pill.minX + 5, y: baseline(f, mid: pill.midY))))
    }

    /// How a tile's file is when it isn't here, in its corner: ↓ to download, a ring while it comes,
    /// the phone while it's asked, and what to try again once it couldn't come.
    static func state(_ media: Media, in r: CGRect, _ c: Context, into parts: inout [Laid.Part]) {
        let side: CGFloat = 22
        let circle = CGRect(x: r.maxX - 4 - side, y: r.maxY - 4 - side, width: side, height: side)
        // In the light look a disc as clear as the dark one's lets the picture through until its ring
        // reads as grey on grey.
        parts.append(.fill(circle, c.roles.ground.opacity(c.roles.isDark ? 0.66 : 0.9).cgColor, side / 2))
        switch media.state {
        case .downloading:
            let centre = CGPoint(x: circle.midX, y: circle.midY)
            parts.append(.ring(centre, side / 2 - 3, 1, c.roles.text.opacity(0.25).cgColor, 2))
            parts.append(.ring(centre, side / 2 - 3, max(media.progress ?? 0, 0.02), c.roles.text.cgColor, 2))
        case .remote where media.error == nil:
            symbol("arrow.down", 9, c.roles.text, centredIn: circle, into: &parts)
        case .waitingForPhone:
            symbol("iphone", 9, c.roles.text, centredIn: circle, into: &parts)
        case .gone:
            symbol("exclamationmark", 9, c.roles.text, centredIn: circle, into: &parts)
        case .remote, .expired:
            symbol("arrow.clockwise", 9, c.roles.text, centredIn: circle, into: &parts)
        case .local:
            break
        }
    }

    static func symbol(_ name: String, _ size: CGFloat, _ colour: RGBA, on fill: RGBA? = nil, centredIn r: CGRect, into parts: inout [Laid.Part]) {
        guard let (image, s) = Symbols.image(name, size, colour, on: fill) else { return }
        parts.append(.image(image, CGRect(x: (r.midX - s.width / 2).rounded(), y: (r.midY - s.height / 2).rounded(), width: s.width, height: s.height)))
    }

    /// A voice message's, a file's or a link's row: on a card in bubbles, between hairlines in lines,
    /// with the kind's mark at its left, its words, and who shared it, where and when.
    static func row(_ item: LibraryRow, size: CGSize, selected: Bool, _ c: Context) -> LibraryDrawn {
        var d = LibraryDrawn()
        let r = c.roles
        let rect = CGRect(origin: .zero, size: size)
        if c.style.look == .bubbles {
            d.under.append(.fill(rect, (selected ? r.selection : r.surface).cgColor, Radius.control))
        } else {
            if selected { d.under.append(.fill(rect, r.selection.cgColor, Radius.small)) }
            d.under.append(.fill(CGRect(x: 0, y: size.height - Stroke.hairline, width: size.width, height: Stroke.hairline),
                                 r.separator.opacity(0.6).cgColor, 0))
        }
        let icon = CGRect(x: Space.m, y: ((size.height - 30) / 2).rounded(), width: 30, height: 30)
        let x = icon.maxX + Space.m + 2, right = size.width - Space.m - 2
        switch item.kind {
        case .voice: voice(item, icon: icon, x: x, right: right, size: size, c, into: &d)
        case .file: file(item, icon: icon, x: x, right: right, size: size, c, into: &d)
        case .link: link(item, icon: icon, x: x, right: right, size: size, c, into: &d)
        case .photo, .video, .other: break
        }
        return d
    }

    /// Two lines: a title, with when at its right, over details.
    static func twoLines(_ title: String, _ details: CTLine, x: CGFloat, right: CGFloat, size: CGSize, _ item: LibraryRow, _ c: Context,
                         into d: inout LibraryDrawn)
    {
        let r = c.roles
        let name = font(.body), small = font(.caption)
        let when = line(Self.when(item, c), small, r.tertiary)
        let top = baseline(name, mid: size.height / 2 - 8), second = baseline(small, mid: size.height / 2 + 9)
        d.under.append(.line(when, CGPoint(x: right - width(when), y: top)))
        d.under.append(.line(fit(line(title, name, r.text), right - x - width(when) - Space.m, name, r.text), CGPoint(x: x, y: top)))
        d.under.append(.line(fit(details, right - x, small, r.secondary), CGPoint(x: x, y: second)))
    }

    /// Words of the caption the window's text matched, its marks in the accent, then who: what a row
    /// says under its title while a search finds it.
    static func marked(_ item: LibraryRow, then tail: String, _ c: Context) -> CTLine? {
        guard item.caption.contains(where: { $0.t == .mark }) else { return nil }
        let small = font(.caption), bold = font(.caption, emphasised: true)
        let out = NSMutableAttributedString()
        for span in item.caption {
            let mark = span.t == .mark
            out.append(NSAttributedString(string: span.s, attributes: [.ctFont: mark ? bold : small,
                                                                       .ctColour: (mark ? c.roles.accent : c.roles.secondary).cgColor]))
        }
        out.append(NSAttributedString(string: " · " + tail, attributes: [.ctFont: small, .ctColour: c.roles.secondary.cgColor]))
        return CTLineCreateWithAttributedString(out)
    }

    static func file(_ item: LibraryRow, icon: CGRect, x: CGFloat, right: CGFloat, size: CGSize, _ c: Context, into d: inout LibraryDrawn) {
        let r = c.roles
        let f = item.file
        d.under.append(.fill(icon, r.accent.opacity(0.15).cgColor, Radius.small + 2))
        symbol(Self.fileSymbol(f), 13, r.accent, centredIn: icon, into: &d.under)
        let said = [Self.coming(f), f?.type, f?.bytes.map(Self.size), f?.pages.map(Self.pages), who(item, c)].compactMap(\.self).filter { !$0.isEmpty }
        let details = marked(item, then: who(item, c), c) ?? line(said.joined(separator: " · "), font(.caption), r.secondary)
        twoLines(f?.name.isEmpty == false ? f!.name : "a file", details, x: x, right: right, size: size, item, c, into: &d)
    }

    /// What a file not here says first: that it's coming, or why it can't, once jawd says how its copy
    /// is (P14); until then, that it isn't downloaded.
    static func coming(_ f: LibraryFile?) -> String? {
        guard let f, f.path == nil else { return nil }
        switch f.state {
        case .downloading?: return "downloading" + (f.progress.map { " \(Int(($0 * 100).rounded()))%" } ?? "")
        case .waitingForPhone?: return "waiting for your phone"
        case .expired?, .gone?: return f.error ?? "not here any more"
        case .remote?, .local?, nil: return f.error ?? "not downloaded"
        }
    }

    static func fileSymbol(_ f: LibraryFile?) -> String {
        switch f?.type?.uppercased() ?? "" {
        case "PDF": "doc.richtext"
        case "ZIP", "RAR", "7Z", "TAR", "GZ": "doc.zipper"
        case "MP3", "M4A", "OGG", "AAC", "WAV", "OPUS", "FLAC": "music.note"
        case "JPG", "JPEG", "PNG", "HEIC", "GIF", "WEBP": "photo"
        case "MP4", "MOV", "M4V": "film"
        case "GPX", "KML": "map"
        default: "doc.text"
        }
    }

    static func link(_ item: LibraryRow, icon: CGRect, x: CGFloat, right: CGFloat, size: CGSize, _ c: Context, into d: inout LibraryDrawn) {
        let r = c.roles
        let l = item.link
        d.under.append(.fill(icon, well(c).cgColor, Radius.small + 2))
        symbol("link", 13, r.accent, centredIn: icon, into: &d.under)
        if let preview = l?.preview {
            d.slot = PictureSlot(rect: icon, radius: Radius.small + 2, fit: .fill, source: PictureSource(small: preview, jpeg: true), message: item.id)
        }
        let host = l?.host.isEmpty == false ? l!.host : l?.url ?? ""
        let small = font(.caption)
        let details: CTLine
        if let marks = marked(item, then: who(item, c), c) {
            details = marks
        } else {
            let out = NSMutableAttributedString(string: host, attributes: [.ctFont: small, .ctColour: r.accent.cgColor])
            out.append(NSAttributedString(string: " · " + who(item, c), attributes: [.ctFont: small, .ctColour: r.secondary.cgColor]))
            details = CTLineCreateWithAttributedString(out)
        }
        let title = l?.title.flatMap { $0.isEmpty ? nil : $0 } ?? l?.url ?? ""
        twoLines(title, details, x: x, right: right, size: size, item, c, into: &d)
    }

    /// One line: ▶ in the accent, the waveform's bars, the length, then who, where and when at the
    /// right. Until jawd sends a voice item's waveform (P14) its bars are even.
    static func voice(_ item: LibraryRow, icon: CGRect, x: CGFloat, right: CGFloat, size: CGSize, _ c: Context, into d: inout LibraryDrawn) {
        let r = c.roles
        d.under.append(.fill(icon, r.accent.cgColor, icon.width / 2))
        symbol("play.fill", 11, r.isDark ? r.ground : RGBA(0xFFFFFF), centredIn: icon.offsetBy(dx: 1, dy: 0), into: &d.under)
        let small = font(.caption), time = font(.caption)
        let mid = size.height / 2
        let tail = line(who(item, c) + " · " + when(item, c), small, r.secondary)
        let bars = min(max((right - x) * 0.4, 48), 160)
        let length = line(item.voice?.seconds.map(Format.length) ?? "", time, r.secondary)
        let wave = item.voice?.waveform ?? []
        let count = Int(bars / 4)
        for i in 0 ..< count {
            let level = wave.isEmpty ? 0.3 : Double(wave[i * wave.count / max(count, 1)]) / 100
            let h = max(3, (CGFloat(level) * 20).rounded())
            d.under.append(.fill(CGRect(x: x + CGFloat(i) * 4, y: (mid - h / 2).rounded(), width: 2, height: h),
                                 r.secondary.opacity(wave.isEmpty ? 0.35 : 0.7).cgColor, 1))
        }
        let lx = x + bars + Space.s
        d.under.append(.line(length, CGPoint(x: lx, y: baseline(time, mid: mid))))
        let tailX = lx + width(length) + Space.l
        let fitted = fit(tail, right - tailX, small, r.secondary)
        d.under.append(.line(fitted, CGPoint(x: right - width(fitted), y: baseline(small, mid: mid))))
    }
}
