import AppKit
import CoreText
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// Where JawUI draws a theme's accent as text it takes the accent's held colour, and the accent's fills
/// the colour the theme wrote; the dim text's words and marks take the secondary text's colour
/// everywhere but the window's ground and the surface, where only the secondary text is held: on your
/// bubbles, a selected row, a selected message's band and a well; the tints JawUI lays over a ground
/// are the ones the hold reads the colours on them on (Owner, 6 Nov); and three small marks are drawn
/// on one ground, not on a tint on a tint.

/// The colours of a line's runs.
func runColours(_ line: CTLine) -> [CGColor] {
    (CTLineGetGlyphRuns(line) as! [CTRun]).compactMap { run in
        (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.ctColour].map { $0 as! CGColor }
    }
}

/// The colour of the first ticks a layout draws.
func tickColour(_ l: Laid) -> CGColor? {
    l.parts.lazy.compactMap { part -> CGColor? in if case let .ticks(_, _, colour, _) = part { colour } else { nil } }.first
}

@MainActor @Test func `the accent drawn as text takes its held colour, and the unread count's badge the accent the theme wrote`() throws {
    var roles = Catalog.jaw.dark
    roles.accentText = RGBA(0xFF00FF)
    #expect(Tone.accent.color(roles) == roles.accentText, "a key, as KeyCap draws one")
    let cell = ChatCell(frame: NSRect(x: 0, y: 0, width: 300, height: 56))
    cell.show(Samples.direct, typists: [], roles: roles, open: false, inCommunity: false, avatar: nil)
    let draft = cell.line(Samples.direct, roles)
    #expect(draft.string.hasPrefix("Draft: ") && draft.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor == roles.accentText.nsColor)
    #expect(cell.badges(Samples.group, roles).first?.fill == roles.accent, "the unread count's badge is the accent's fill")
    let s = LayoutSpec(style: Style(roles: roles), me: ["person:1"])
    let mentions = try #require(Samples.messages.first { $0.spans.contains { $0.t == .mention } })
    let frame = try #require(laid(mentions, s).text?.frame)
    let runs = (CTFrameGetLines(frame) as! [CTLine]).flatMap(runColours)
    #expect(runs.contains(roles.accentText.cgColor) && runs.contains(roles.mention.cgColor) && !runs.contains(roles.accent.cgColor),
            "@Rui in the accent's held colour, @Pedro, a mention of you, in the mention colour")
    var reply = try #require(Samples.messages.first { $0.reply != nil })
    reply.reply = Quote(id: "3EB0000000000002", from: "You", summary: "did you get the photos from Sunday?")
    let quoted = laid(reply, s)
    #expect(quoted.parts.contains { if case let .fill(rect, colour, _) = $0 { rect.width == 3 && colour == roles.accentText.cgColor } else { false } },
            "You in a reply: its bar")
    #expect(quoted.parts.contains { if case let .line(l, _) = $0 { runColours(l) == [roles.accentText.cgColor] } else { false } }, "and its name")
    var context = MessageContext(group: false, firstOfRun: true, showTime: true, day: nil)
    context.sending = ["OUT": Sending(step: .uploading, percent: 40)]
    let going = MessageLayout.lay(outgoing(.document), width: 640, context: context, style: s.style, fonts: s.fonts, me: s.me)
    let rings = going.parts.compactMap { part -> CGColor? in if case let .ring(_, _, _, colour, _) = part { colour } else { nil } }
    #expect(rings.contains(roles.accentText.cgColor) && rings.contains(roles.accent.opacity(0.25).cgColor) && !rings.contains(roles.accent.cgColor),
            "a file of yours on its way, as a download: its progress in the accent drawn as text, its track the accent at 25%")
    let marks = going.parts.compactMap { part -> Data? in if case let .image(image, _) = part { image.dataProvider?.data as Data? } else { nil } }
    let x = try #require(Symbols.image("xmark", 9, roles.accentText)).0.dataProvider?.data as Data?
    #expect(x != nil && marks.contains { $0 == x }, "and its × in the accent drawn as text")
}

@MainActor @Test func `the dim text's marks take the secondary text's colour on your bubbles and a selected row, and the dim text's own on the ground and the surface`() throws {
    let roles = Catalog.jaw.dark
    #expect(Ticks.of(.delivered, roles)?.1 == roles.tertiary && Ticks.of(.sent, roles, on: .yours)?.1 == roles.secondary)
    #expect(Ticks.of(.read, roles, on: .selection)?.1 == roles.link, "read ticks in the link colour wherever they're drawn")
    var direct = Samples.direct
    direct.draft = nil
    direct.last?.status = .delivered
    let cell = ChatCell(frame: NSRect(x: 0, y: 0, width: 300, height: 56))
    cell.show(direct, typists: [], roles: roles, open: false, inCommunity: false, avatar: nil)
    #expect(cell.ticks(direct, roles)?.1 == roles.tertiary, "on the chat list's surface")
    cell.isSelected = true
    #expect(cell.ticks(direct, roles)?.1 == roles.secondary, "on a selected row")
    direct.last?.status = .pending
    let pending = cell.line(direct, roles)
    #expect(pending.string.hasPrefix("◷ ") && pending.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor == roles.secondary.nsColor)
    let sent = try #require(Samples.messages.first { $0.mine && $0.status == .sent })
    #expect(tickColour(laid(sent, look: .bubbles)) == roles.secondary.cgColor, "in your bubble")
    #expect(tickColour(laid(sent, look: .lines)) == roles.tertiary.cgColor, "on the window's ground, in lines")
    let emoji = try #require(Samples.messages.first { $0.mine && $0.status == .delivered && $0.kind == .text })
    #expect(tickColour(laid(emoji, look: .bubbles)) == roles.tertiary.cgColor, "emoji alone, under no bubble")
}

@MainActor @Test func `a message selected in lines has its dim marks drawn in the secondary text's colour on its band, as the conversation tells its layout`() async throws {
    let roles = Catalog.jaw.dark
    let sent = try #require(Samples.messages.first { $0.mine && $0.status == .sent })
    let lines = spec(.lines, dark: true)
    func ticks(selected: Bool) -> CGColor? {
        var context = MessageContext(group: false, firstOfRun: true, showTime: true, day: nil)
        context.selected = selected
        return tickColour(MessageLayout.lay(sent, width: 640, context: context, style: lines.style, fonts: lines.fonts, me: lines.me))
    }
    #expect(ticks(selected: false) == roles.tertiary.cgColor && ticks(selected: true) == roles.secondary.cgColor)
    let host = PaneHost()
    let (c, window) = await shown(Samples.messages, host)
    defer { window.close() }
    c.spec = lines
    let i = try #require(c.rows.firstIndex { $0.id == sent.id })
    c.select(sent.id)
    #expect(c.contexts[i].selected, "selected in lines: its layout is told")
    #expect(try messageView(c, i).laid.flatMap(tickColour) == roles.secondary.cgColor, "and its ticks are drawn on its band")
    c.select(nil)
    #expect(!c.contexts[i].selected)
    c.spec = spec(.bubbles, dark: true)
    c.select(sent.id)
    #expect(!c.contexts[i].selected, "in bubbles the band is behind the bubble, whose ground its words keep")
}

@MainActor @Test func `the tints JawUI lays over a ground are the ones the hold reads the colours drawn on them on, in both halves`() throws {
    func fills(_ l: Laid) -> [CGColor] {
        l.parts.compactMap { part -> CGColor? in if case let .fill(_, colour, _) = part { colour } else { nil } }
    }
    for dark in [true, false] {
        let s = spec(.bubbles, dark: dark)
        let roles = s.style.roles
        let half = dark ? "dark" : "light"
        #expect(fills(laid(sampleLink("L2"), s)).contains(roles.well.cgColor), "\(half): a link card's well")
        let reply = try #require(Samples.messages.first { $0.reply != nil })
        #expect(fills(laid(reply, s)).contains(roles.quoteWell.cgColor), "\(half): a quote's")
        let file = try #require(Samples.messages.first { $0.kind == .document })
        #expect(fills(laid(file, s)).contains(roles.accentTint.cgColor), "\(half): the accent's, behind a file's symbol")
        let mentions = try #require(Samples.messages.first { $0.spans.contains { $0.t == .mention } })
        let frame = try #require(laid(mentions, s).text?.frame)
        let tints = (CTFrameGetLines(frame) as! [CTLine]).flatMap { line in
            (CTLineGetGlyphRuns(line) as! [CTRun]).compactMap { run in
                (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.jawBackground].map { $0 as! CGColor }
            }
        }
        #expect(!tints.isEmpty && tints.allSatisfy { $0 == roles.mentionTint.cgColor }, "\(half): a mention of you, on its own colour's tint")
        let view = MessageView(frame: NSRect(x: 0, y: 0, width: 640, height: 200))
        view.roles = roles
        view.laid = laid(sampleLink("L2"), s)
        view.selected = true
        let l = try #require(view.laid)
        let (w, h) = (640, Int(ceil(l.height)))
        let ctx = try #require(CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                         space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.setFillColor(roles.ground.cgColor)
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
        ctx.translateBy(x: 0, y: CGFloat(h))
        ctx.scaleBy(x: 1, y: -1)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        view.paint(view.bounds)
        NSGraphicsContext.restoreGraphicsState()
        let pixel = try #require(ctx.data).assumingMemoryBound(to: UInt8.self) + (Int(l.body.midY) * w + w - 3) * 4
        let band = roles[.band].hex
        let off = [(pixel[0], band >> 16), (pixel[1], band >> 8), (pixel[2], band)].map { abs(Int($0.0) - Int($0.1 & 0xFF)) }
        #expect(off.allSatisfy { $0 <= 2 }, "\(half): a selected message's band beside their bubble, the selection at 55% over the window's ground, off by \(off)")
    }
}

@MainActor @Test func `a link card's thumbnail waits on the card's own well, its link symbol drawn there, with no well of its own`() throws {
    for look in [Look.bubbles, .lines] {
        let s = spec(look, dark: true)
        let roles = s.style.roles
        for id in ["L1", "L3"] {
            let l = laid(sampleLink(id), s)
            let wells = l.parts.filter { if case let .fill(_, colour, _) = $0 { colour == roles.well.cgColor } else { false } }
            #expect(wells.count == 1, "\(id) in \(look): the card's well alone, the placeholder's symbol on it")
            #expect(l.pictures.count == 1, "\(id) in \(look): the thumbnail's slot, which its picture fills when it comes")
        }
    }
}

@MainActor @Test func `a selected row of the library draws a file's symbol and a link's icon on the selection, without their tints`() throws {
    for look in [Look.bubbles, .lines] {
        let style = libraryStyle(look)
        let roles = style.roles
        func fills(_ d: LibraryDrawn) -> [CGColor] {
            d.under.compactMap { part -> CGColor? in if case let .fill(_, colour, _) = part { colour } else { nil } }
        }
        for (kind, tint) in [(LibraryKind.file, roles.accentTint), (.link, roles.well)] {
            let row = try #require(rows(libraryLaid(kind, style: style)).first)
            #expect(fills(drawn(row, style: style)).contains(tint.cgColor), "\(kind) in \(look): on its tint, not selected")
            let selected = fills(drawn(row, style: style, selected: true))
            #expect(!selected.contains(tint.cgColor) && selected.contains(roles.selection.cgColor),
                    "\(kind) in \(look): selected, on the selection itself")
        }
    }
}

@MainActor @Test func `the dim text drawn on a well takes the secondary text's colour: a link card's host, and a tile's placeholder`() throws {
    let s = spec(.bubbles, dark: true)
    let roles = s.style.roles
    let card = laid(sampleLink("L2"), s)
    let runs = card.parts.flatMap { part -> [CGColor] in if case let .line(l, _) = part { runColours(l) } else { [] } }
    #expect(!runs.contains(roles.tertiary.cgColor),
            "the host, the card's one word in the dim text's colour before, now in the secondary text's: nothing in the card in the dim text's")
    var photo = try #require(Samples.library.first { $0.kind == .photo })
    photo.media = nil
    let c = drawingContext()
    let tile = LibraryDrawing.tile(photo, size: CGSize(width: 120, height: 120), selected: false, c)
    let symbols = tile.under.compactMap { part -> Data? in if case let .image(image, _) = part { image.dataProvider?.data as Data? } else { nil } }
    let secondary = try #require(Symbols.image("photo", 20, c.roles.secondary)).0.dataProvider?.data as Data?
    #expect(secondary != nil && symbols.contains { $0 == secondary }, "a tile with no picture yet: its placeholder in the secondary text's colour, on its well")
}
