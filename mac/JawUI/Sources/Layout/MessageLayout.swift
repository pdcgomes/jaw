import CoreGraphics
import CoreText
import Foundation
import JawKit
import JawThemes

/// How far a file of yours on its way has got, as jawd's last job for it said: its copy being made,
/// or uploading, a whole percent, so a message lays out at most so many ways.
public struct Sending: Sendable, Hashable {
    public enum Step: Sendable, Hashable {
        case making, uploading
    }

    public var step: Step
    public var percent: Int

    public init(step: Step, percent: Int) {
        self.step = step
        self.percent = percent
    }

    public init(_ job: Job) {
        step = job.kind == .upload ? .uploading : .making
        percent = min(max(Int((job.progress * 100).rounded()), 0), 100)
    }

    /// As the terminal says it.
    public var words: String {
        "\(step == .uploading ? "uploading" : "making it ready") \(percent)%"
    }
}

extension MessageRow {
    /// Yours, sent from here and in the outbox, waiting or failed: its file is jawd's own copy, made
    /// or being made, never one to download, which its row reads as before its copy is made, having
    /// no state. A forward's file isn't uploaded, so a forward isn't.
    var fromOutbox: Bool {
        mine && forwarded == 0 && (status == .pending || status == .failed)
    }

    /// Yours from the outbox, waiting to send: its file being made or uploaded, as jawd's jobs say.
    var onItsWay: Bool {
        mine && forwarded == 0 && status == .pending
    }
}

/// What a message's lines depend on beyond the message: whether its sender's name and its time
/// show, the day above it, whether a long one is unfolded, and how far its files of yours on their
/// way have got.
public struct MessageContext: Sendable, Hashable {
    public var group: Bool
    /// The sender changed, or five minutes passed: a run starts here.
    public var firstOfRun: Bool
    /// Lines show the time when the minute or the sender changes.
    public var showTime: Bool
    /// The day separator above it, when its day isn't the one before's.
    public var day: String?
    public var unfolded: Bool
    /// How many unread messages start here, for the "2 new" rule above the first of them.
    public var newAbove: Int
    /// Its files on their way, by message: its own, or its album's items'.
    public var sending: [String: Sending] = [:]

    public init(group: Bool, firstOfRun: Bool, showTime: Bool, day: String?, unfolded: Bool = false,
                newAbove: Int = 0)
    {
        self.group = group
        self.firstOfRun = firstOfRun
        self.showTime = showTime
        self.day = day
        self.unfolded = unfolded
        self.newAbove = newAbove
    }

    /// The contexts of a window's messages, oldest first, as the terminal decides names and times.
    public static func of(_ rows: [MessageRow], group: Bool, unfolded: Set<String>, now: Int64) -> [MessageContext] {
        var out: [MessageContext] = []
        out.reserveCapacity(rows.count)
        var previous: MessageRow?
        for m in rows {
            let sameSender = previous.map { $0.mine == m.mine && $0.from?.id == m.from?.id } ?? false
            let close = previous.map { m.at - $0.at < 5 * 60000 } ?? false
            let sameDay = previous.map { Format.sameDay($0.at, m.at) } ?? false
            let lines = previous.map { $0.kind == .system || $0.kind == .call } ?? false
            let firstOfRun = !(sameSender && close && sameDay) || lines
            let showTime = firstOfRun || !Format.sameMinute(previous!.at, m.at)
            out.append(MessageContext(group: group, firstOfRun: firstOfRun, showTime: showTime,
                                      day: sameDay ? nil : Format.day(m.at, now: now),
                                      unfolded: unfolded.contains(m.id)))
            previous = m
        }
        return out
    }
}

/// Where a click on a message lands.
public enum Region: Sendable, Hashable {
    case reaction(String)
    case retry
    case showMore
    case quote(String)
    /// A card that opens something: a place in Maps, a link's page.
    case open(String)
    /// A poll's option, by name, which a click votes for.
    case vote(String)
    /// A picture or a file: a click looks at it, or asks for it when it isn't here; `item` is an
    /// album's.
    case media(item: Int?)
    /// The ring over a picture or a file coming: a click stops it.
    case stop(item: Int?)
    /// A contact card: Message, or for several cards, the sheet of them.
    case card
}

/// A message laid out at a width: its height, and what to draw, already measured.
///
/// Core Text's frames and lines are immutable once made, so a layout made on the layout queue is
/// drawn on the main thread.
public final class Laid: @unchecked Sendable {
    public let height: CGFloat
    /// The message's body (its bubble, or its text and what's around it), for the hover's buttons
    /// and the selection.
    public let body: CGRect
    let parts: [Part]
    let regions: [(CGRect, Region)]
    let text: (frame: CTFrame, rect: CGRect)?
    /// The pictures it shows, each drawn from what's decoded of it, over `parts` and under `over`.
    public let pictures: [PictureSlot]
    /// What's drawn over the pictures: a video's ▶ and length, an album's "+3", a download's ring.
    let over: [Part]

    enum Part {
        case fill(CGRect, CGColor, CGFloat)
        case stroke(CGRect, CGColor, CGFloat)
        case frame(CTFrame, CGRect)
        /// A line whose baseline sits at the point, measured from the top.
        case line(CTLine, CGPoint)
        /// Your ticks, so many from the point on their baseline, at a text size.
        case ticks(Int, CGPoint, CGColor, CGFloat)
        /// A picture, or a symbol made into one, in a rect measured from the top.
        case image(CGImage, CGRect)
        /// A ring around a centre, of a radius, filled from the top clockwise to a share of it, at a
        /// line's width.
        case ring(CGPoint, CGFloat, CGFloat, CGColor, CGFloat)
    }

    init(height: CGFloat, body: CGRect, parts: [Part], regions: [(CGRect, Region)], text: (CTFrame, CGRect)?,
         pictures: [PictureSlot] = [], over: [Part] = [])
    {
        self.height = height
        self.body = body
        self.parts = parts
        self.regions = regions
        self.text = text
        self.pictures = pictures
        self.over = over
    }

    /// Draws into a flipped context, its top at `y`, each picture from `picture` when it has one.
    public func draw(in ctx: CGContext, y: CGFloat = 0, picture: (PictureSlot) -> CGImage? = { _ in nil }) {
        Self.draw(parts, in: ctx, y: y)
        for slot in pictures {
            if let image = picture(slot) { Self.draw(image, in: slot, ctx: ctx, y: y) }
        }
        Self.draw(over, in: ctx, y: y)
    }

    /// A picture in its slot, clipped to its corners, filling it.
    static func draw(_ image: CGImage, in slot: PictureSlot, ctx: CGContext, y: CGFloat) {
        let rect = slot.rect.offsetBy(dx: 0, dy: y)
        ctx.saveGState()
        ctx.addPath(CGPath(roundedRect: rect, cornerWidth: min(slot.radius, rect.width / 2), cornerHeight: min(slot.radius, rect.height / 2),
                           transform: nil))
        ctx.clip()
        ctx.translateBy(x: rect.minX, y: rect.maxY)
        ctx.scaleBy(x: 1, y: -1)
        ctx.draw(image, in: CGRect(origin: .zero, size: rect.size))
        ctx.restoreGState()
    }

    static func draw(_ parts: [Part], in ctx: CGContext, y: CGFloat) {
        for part in parts {
            switch part {
            case let .fill(rect, colour, radius):
                ctx.setFillColor(colour)
                ctx.addPath(CGPath(roundedRect: rect.offsetBy(dx: 0, dy: y), cornerWidth: min(radius, rect.width / 2),
                                   cornerHeight: min(radius, rect.height / 2), transform: nil))
                ctx.fillPath()
            case let .stroke(rect, colour, radius):
                ctx.setStrokeColor(colour)
                ctx.setLineWidth(1)
                ctx.addPath(CGPath(roundedRect: rect.offsetBy(dx: 0, dy: y).insetBy(dx: 0.5, dy: 0.5),
                                   cornerWidth: radius, cornerHeight: radius, transform: nil))
                ctx.strokePath()
            case let .frame(frame, rect):
                Decorations.draw(frame, in: rect.offsetBy(dx: 0, dy: y), ctx)
            case let .line(line, point):
                ctx.saveGState()
                ctx.translateBy(x: point.x, y: point.y + y)
                ctx.scaleBy(x: 1, y: -1)
                ctx.textPosition = .zero
                CTLineDraw(line, ctx)
                ctx.restoreGState()
            case let .ticks(count, point, colour, size):
                Ticks.draw(count, x: point.x, baseline: point.y + y, size: size, colour: colour, in: ctx)
            case let .image(image, rect):
                ctx.saveGState()
                ctx.translateBy(x: rect.minX, y: rect.maxY + y)
                ctx.scaleBy(x: 1, y: -1)
                ctx.draw(image, in: CGRect(origin: .zero, size: rect.size))
                ctx.restoreGState()
            case let .ring(centre, radius, share, colour, width):
                guard share > 0 else { break }
                ctx.saveGState()
                ctx.setStrokeColor(colour)
                ctx.setLineWidth(width)
                ctx.setLineCap(.round)
                let top = -CGFloat.pi / 2
                ctx.addArc(center: CGPoint(x: centre.x, y: centre.y + y), radius: radius, startAngle: top,
                           endAngle: top + 2 * .pi * min(share, 1), clockwise: false)
                ctx.strokePath()
                ctx.restoreGState()
            }
        }
    }

    /// What's at a point, in the layout's own coordinates.
    public func region(at point: CGPoint) -> Region? {
        regions.first { $0.0.contains(point) }?.1
    }

    /// Whether a point, in the layout's own coordinates, is on the message itself, its body or one of
    /// its regions, where a click acts on it; the rest of its row is the transcript's blank space.
    public func contains(_ point: CGPoint) -> Bool {
        body.contains(point) || region(at: point) != nil
    }

    /// The link under a point, if the text has one there.
    public func link(at point: CGPoint) -> String? {
        guard let (frame, rect) = text, rect.contains(point) else { return nil }
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = [CGPoint](repeating: .zero, count: lines.count)
        CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
        let local = CGPoint(x: point.x - rect.minX, y: rect.height - (point.y - rect.minY))
        for (i, line) in lines.enumerated() {
            var ascent: CGFloat = 0, descent: CGFloat = 0
            CTLineGetTypographicBounds(line, &ascent, &descent, nil)
            guard local.y >= origins[i].y - descent, local.y <= origins[i].y + ascent else { continue }
            let index = CTLineGetStringIndexForPosition(line, CGPoint(x: local.x - origins[i].x, y: 0))
            let string = CTFrameGetVisibleStringRange(frame)
            guard index != kCFNotFound, index < string.location + string.length else { return nil }
            let runs = CTLineGetGlyphRuns(line) as! [CTRun]
            for run in runs {
                let range = CTRunGetStringRange(run)
                guard index >= range.location, index < range.location + range.length else { continue }
                let attributes = CTRunGetAttributes(run) as NSDictionary
                return attributes[NSAttributedString.Key.jawLink] as? String
            }
        }
        return nil
    }
}

extension NSAttributedString.Key {
    static let jawLink = NSAttributedString.Key("jaw.link")
    static let jawStrike = NSAttributedString.Key("jaw.strike")
    static let jawBackground = NSAttributedString.Key("jaw.background")
    static let ctFont = NSAttributedString.Key(kCTFontAttributeName as String)
    static let ctColour = NSAttributedString.Key(kCTForegroundColorAttributeName as String)
    static let ctUnderline = NSAttributedString.Key(kCTUnderlineStyleAttributeName as String)
}

/// What Core Text doesn't draw itself: a run's background and its strike.
enum Decorations {
    static func draw(_ frame: CTFrame, in rect: CGRect, _ ctx: CGContext) {
        ctx.saveGState()
        ctx.translateBy(x: rect.minX, y: rect.maxY)
        ctx.scaleBy(x: 1, y: -1)
        ctx.textMatrix = .identity
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = [CGPoint](repeating: .zero, count: lines.count)
        CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
        var strikes: [(CGRect, CGColor)] = []
        for (i, line) in lines.enumerated() {
            for run in CTLineGetGlyphRuns(line) as! [CTRun] {
                let attributes = CTRunGetAttributes(run) as NSDictionary
                let background = attributes[NSAttributedString.Key.jawBackground]
                let strike = attributes[NSAttributedString.Key.jawStrike] != nil
                guard background != nil || strike else { continue }
                var ascent: CGFloat = 0, descent: CGFloat = 0
                let width = CTRunGetTypographicBounds(run, CFRange(location: 0, length: 0), &ascent, &descent, nil)
                var position = CGPoint.zero
                CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
                let x = origins[i].x + position.x
                if let background {
                    ctx.setFillColor(background as! CGColor)
                    ctx.addPath(CGPath(roundedRect: CGRect(x: x - 2, y: origins[i].y - descent - 1, width: width + 4,
                                                           height: ascent + descent + 2),
                                       cornerWidth: 3, cornerHeight: 3, transform: nil))
                    ctx.fillPath()
                }
                if strike {
                    let colour = (attributes[NSAttributedString.Key.ctColour] as! CGColor?) ?? CGColor(gray: 0.5, alpha: 1)
                    strikes.append((CGRect(x: x, y: origins[i].y + ascent * 0.32, width: width, height: 1), colour))
                }
            }
        }
        CTFrameDraw(frame, ctx)
        for (line, colour) in strikes {
            ctx.setFillColor(colour)
            ctx.fill(line)
        }
        ctx.restoreGState()
    }
}

/// Lays out messages in either look, as the design draws them.
public enum MessageLayout {
    struct Metrics {
        let margin: CGFloat
        let padX: CGFloat
        let padY: CGFloat
        let radius: CGFloat
        let runGap: CGFloat
        let gap: CGFloat
        let dayHeight: CGFloat

        init(_ style: Style) {
            let c: CGFloat = style.compact ? 0.5 : 1
            margin = 16
            padX = 10
            padY = style.compact ? 4 : 6
            radius = 14
            runGap = 10 * c
            gap = 2 * c
            dayHeight = style.compact ? 26 : 34
        }
    }

    /// The most lines a message shows before it folds, until it's opened.
    public static let fold = 40

    public static func lay(_ m: MessageRow, width: CGFloat, context: MessageContext, style: Style, fonts: FontSet,
                           me: Set<String>) -> Laid
    {
        var b = Builder(m: m, width: max(width, 160), context: context, style: style, fonts: fonts, me: me)
        return b.build()
    }

    struct Builder {
        let m: MessageRow
        let width: CGFloat
        let context: MessageContext
        let style: Style
        let fonts: FontSet
        let me: Set<String>
        let metrics: Metrics
        let roles: Roles
        var parts: [Laid.Part] = []
        var regions: [(CGRect, Region)] = []
        var text: (CTFrame, CGRect)?
        var slots: [PictureSlot] = []
        var over: [Laid.Part] = []
        /// Whether the card's pieces are drawn over its pictures rather than under them.
        var drawingOver = false
        var y: CGFloat = 0

        init(m: MessageRow, width: CGFloat, context: MessageContext, style: Style, fonts: FontSet, me: Set<String>) {
            self.m = m
            self.width = width
            self.context = context
            self.style = style
            self.fonts = fonts
            self.me = me
            metrics = Metrics(style)
            roles = style.roles
        }

        mutating func build() -> Laid {
            if let day = context.day { dayLine(day) }
            if context.newAbove > 0 { newRule(context.newAbove) }
            y += context.firstOfRun ? metrics.runGap : metrics.gap
            if m.kind == .system || m.kind == .call {
                return systemLine()
            }
            return style.look == .bubbles ? bubble() : lines()
        }

        // MARK: Pieces

        func line(_ s: String, _ font: CTFont, _ colour: RGBA, kern: CGFloat = 0) -> CTLine {
            let attributed = NSAttributedString(string: s, attributes: [
                .ctFont: font, .ctColour: colour.cgColor, .kern: kern,
            ])
            return CTLineCreateWithAttributedString(attributed)
        }

        func width(of line: CTLine) -> CGFloat {
            CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) - CGFloat(CTLineGetTrailingWhitespaceWidth(line))
        }

        func ascent(_ font: CTFont) -> CGFloat {
            CTFontGetAscent(font)
        }

        func lineHeight(_ font: CTFont) -> CGFloat {
            ceil(CTFontGetAscent(font) + CTFontGetDescent(font) + CTFontGetLeading(font))
        }

        func truncated(_ s: String, _ font: CTFont, _ colour: RGBA, _ most: CGFloat) -> CTLine {
            let full = line(s, font, colour)
            let ellipsis = line("…", font, colour)
            return CTLineCreateTruncatedLine(full, Double(max(most, 10)), .end, ellipsis) ?? full
        }

        mutating func dayLine(_ day: String) {
            let label = line(day, fonts.small, roles.secondary)
            let w = width(of: label)
            let h = metrics.dayHeight
            let mid = y + h / 2
            if style.look == .bubbles {
                let pill = CGRect(x: (width - w) / 2 - 10, y: mid - 10, width: w + 20, height: 20)
                parts.append(.fill(pill, roles.surface.cgColor, 10))
            } else {
                let rule = roles.separator.cgColor
                parts.append(.fill(CGRect(x: metrics.margin, y: mid, width: (width - w) / 2 - metrics.margin - 10, height: 1), rule, 0))
                parts.append(.fill(CGRect(x: (width + w) / 2 + 10, y: mid, width: (width - w) / 2 - metrics.margin - 10, height: 1), rule, 0))
            }
            parts.append(.line(label, CGPoint(x: (width - w) / 2, y: mid + ascent(fonts.small) / 2 - 1)))
            y += h
        }

        /// "2 new" across the conversation, above the first unread message.
        mutating func newRule(_ n: Int) {
            let label = line("\(Format.num(n)) new", fonts.small, roles.mention)
            let w = width(of: label)
            let mid = y + 12
            parts.append(.fill(CGRect(x: metrics.margin, y: mid, width: width - 2 * metrics.margin - w - 10, height: 1),
                               roles.mention.opacity(0.6).cgColor, 0))
            parts.append(.line(label, CGPoint(x: width - metrics.margin - w, y: mid + ascent(fonts.small) / 2 - 1)))
            y += 24
        }

        mutating func systemLine() -> Laid {
            let s = m.system ?? m.plain
            let label = truncated(s, fonts.small, roles.secondary, width - 2 * metrics.margin - 24)
            let w = width(of: label)
            let rect = CGRect(x: (width - w) / 2 - 10, y: y, width: w + 20, height: 22)
            if style.look == .bubbles { parts.append(.fill(rect, roles.surface.cgColor, 11)) }
            parts.append(.line(label, CGPoint(x: (width - w) / 2, y: y + 15)))
            y += 22
            return Laid(height: ceil(y), body: rect, parts: parts, regions: regions, text: nil, pictures: slots, over: over)
        }

        /// The message's words: its spans, or what its kind is, as one line before its caption.
        func words() -> NSAttributedString {
            let out = NSMutableAttributedString()
            let base: [NSAttributedString.Key: Any] = [.ctFont: fonts.body, .ctColour: roles.text.cgColor]
            if m.deleted {
                let s = m.mine ? "You deleted this message" : "This message was deleted"
                out.append(NSAttributedString(string: s, attributes: [.ctFont: fonts.italic, .ctColour: roles.secondary.cgColor]))
                return out
            }
            if !hasCard, let kind = Self.kindLine(m) {
                out.append(NSAttributedString(string: kind, attributes: [.ctFont: fonts.italic, .ctColour: roles.secondary.cgColor]))
                if !m.spans.isEmpty { out.append(NSAttributedString(string: "\n", attributes: base)) }
            }
            if hasCard, cardHasTheWords { return out }
            // A document's name is in its card; its words are its caption.
            let spans = m.kind == .document && !m.deleted
                ? m.spans.filter { $0.s != m.media?.name && $0.s != "\n" + (m.media?.name ?? "") }
                : m.spans
            for span in spans {
                var a = base
                switch span.t {
                case .bold: a[.ctFont] = fonts.bold
                case .italic: a[.ctFont] = fonts.italic
                case .strike:
                    a[.jawStrike] = true
                    a[.ctColour] = roles.secondary.cgColor
                case .mono:
                    a[.ctFont] = fonts.mono
                    a[.jawBackground] = roles.text.opacity(0.08).cgColor
                case .quote:
                    a[.ctFont] = fonts.italic
                    a[.ctColour] = roles.secondary.cgColor
                case .link:
                    a[.ctColour] = roles.accent.cgColor
                    a[.ctUnderline] = CTUnderlineStyle.single.rawValue
                    a[.jawLink] = span.url ?? span.s
                case .mention:
                    let mine = span.person.map(me.contains) ?? false
                    a[.ctFont] = fonts.bold
                    a[.ctColour] = (mine ? roles.mention : roles.accent).cgColor
                    if mine { a[.jawBackground] = roles.mention.opacity(0.16).cgColor }
                case .text, .list, .mark, .unknown: break
                }
                out.append(NSAttributedString(string: span.s, attributes: a))
            }
            if out.length == 0, !m.deleted, m.kind == .text {
                out.append(NSAttributedString(string: " ", attributes: base))
            }
            return out
        }

        /// A message's kind in one line: what a message with no card of its own shows, an album of counts
        /// among them, and what VoiceOver, a reply's quote and Delete's question say for one with no text.
        static func kindLine(_ m: MessageRow) -> String? {
            let seconds = m.media?.seconds ?? m.voice?.seconds
            let length = seconds.map { " · " + Format.length($0) } ?? ""
            switch m.kind {
            case .text, .system, .call: return nil
            case .image: return m.viewOnce ? "Photo, view once · open it on your phone" : "Photo"
            case .video: return "Video\(length)"
            case .gif: return "GIF"
            case .videoNote: return "Video message\(length)"
            case .voice: return "Voice message\(length)"
            case .audio: return "Audio\(length)"
            case .document: return "Document · " + (m.media?.name ?? "a file")
            case .sticker: return "Sticker"
            case .album: return albumLine(m.album)
            case .location: return "Location"
            case .liveLocation: return "Live location"
            case .contact: return "Contact card"
            case .poll: return m.poll.map { "Poll · " + $0.question } ?? "Poll"
            case .event: return "Event"
            case .invite: return invitation(m.invite)
            case .unknown: return "Jaw can't show this yet"
            }
        }

        /// An album's photos and videos, counted: "Album · 3 photos and 1 video".
        static func albumLine(_ album: Album?) -> String {
            let items = album?.items ?? []
            let photos = items.isEmpty ? album?.photos ?? 0 : items.count { $0.kind != "video" }
            let videos = items.isEmpty ? album?.videos ?? 0 : items.count { $0.kind == "video" }
            let counted = [(photos, "photo"), (videos, "video")].filter { $0.0 > 0 }.map { "\($0.0) \($0.1)\($0.0 == 1 ? "" : "s")" }
            return counted.isEmpty ? "Album" : "Album · " + counted.joined(separator: " and ")
        }

        /// What an invitation is to, the group's or channel's name, and until when. Its caption, under
        /// it, is the sentence WhatsApp writes for it ("Invitation to follow the channel, 'Jaw news'").
        static func invitation(_ invite: Invite?) -> String {
            let what = invite?.admin == true ? "Channel admin invitation"
                : invite?.channel != nil ? "Channel invitation" : "Group invitation"
            let name = invite?.name.flatMap { $0.isEmpty ? nil : $0 }
            let until = invite?.expires.flatMap { $0 > 0 ? "until " + Format.until($0) : nil }
            return ([what, name, until] as [String?]).compactMap(\.self).joined(separator: " · ")
        }

        /// Whether the message is one to three emoji alone, drawn large without a bubble.
        func emojiOnly() -> Bool {
            guard m.kind == .text, !m.deleted, m.reply == nil else { return false }
            let s = m.plain.trimmingCharacters(in: .whitespaces)
            guard !s.isEmpty, s.count <= 3 else { return false }
            return s.allSatisfy { c in c.unicodeScalars.first.map { $0.properties.isEmojiPresentation } ?? false }
        }

        /// The text as a frame at most `most` wide: the frame, its size, and its last line's width.
        mutating func frame(_ attributed: NSAttributedString, most: CGFloat) -> (CTFrame, CGSize, CGFloat, Bool) {
            let setter = CTFramesetterCreateWithAttributedString(attributed)
            var range = CFRange(location: 0, length: 0)
            var folded = false
            let full = CTFramesetterSuggestFrameSizeWithConstraints(setter, range, nil,
                                                                    CGSize(width: most, height: .greatestFiniteMagnitude), nil)
            var size = CGSize(width: ceil(full.width), height: ceil(full.height))
            var made = CTFramesetterCreateFrame(setter, range, CGPath(rect: CGRect(origin: .zero, size: CGSize(width: most, height: size.height + 2)), transform: nil), nil)
            var lines = CTFrameGetLines(made) as! [CTLine]
            if lines.count > MessageLayout.fold, !context.unfolded {
                let end = CTLineGetStringRange(lines[MessageLayout.fold - 1])
                range = CFRange(location: 0, length: end.location + end.length)
                let cut = CTFramesetterSuggestFrameSizeWithConstraints(setter, range, nil,
                                                                       CGSize(width: most, height: .greatestFiniteMagnitude), nil)
                size = CGSize(width: ceil(cut.width), height: ceil(cut.height))
                made = CTFramesetterCreateFrame(setter, range, CGPath(rect: CGRect(origin: .zero, size: CGSize(width: most, height: size.height + 2)), transform: nil), nil)
                lines = CTFrameGetLines(made) as! [CTLine]
                folded = true
            }
            let widest = lines.map { width(of: $0) }.max() ?? 0
            size.width = ceil(min(max(widest, 1), most))
            size.height += 2
            let last = lines.last.map { width(of: $0) } ?? 0
            return (made, size, last, folded)
        }

        /// What's said for your message's state in text: pending and failed; ticks are drawn.
        func stateMark() -> (String, RGBA)? {
            guard m.mine, let status = m.status else { return nil }
            switch status {
            case .pending: return ("◷", roles.tertiary)
            case .failed: return ("!", roles.failed)
            case .sent, .delivered, .read, .played: return nil
            }
        }

        var tickSize: CGFloat {
            CTFontGetSize(fonts.time) + 1
        }

        /// A foot line drawn at a point, with the ticks drawn in the room it leaves for them.
        func footParts(_ foot: CTLine, at point: CGPoint) -> [Laid.Part] {
            var out: [Laid.Part] = [.line(foot, point)]
            if m.mine, let (count, colour) = Ticks.of(m.status, roles) {
                let w = Ticks.width(count, size: tickSize)
                out.append(.ticks(count, CGPoint(x: point.x + width(of: foot) - w, y: point.y), colour.cgColor, tickSize))
            }
            return out
        }

        /// The time and what's said around it: a star, Edited, your ticks.
        func footer(time: Bool = true) -> CTLine {
            let out = NSMutableAttributedString()
            let quiet: [NSAttributedString.Key: Any] = [.ctFont: fonts.time, .ctColour: roles.secondary.cgColor]
            if m.starred { out.append(NSAttributedString(string: "★ ", attributes: quiet)) }
            if m.edited, !m.deleted { out.append(NSAttributedString(string: "Edited ", attributes: [.ctFont: fonts.smallItalic, .ctColour: roles.secondary.cgColor])) }
            if time { out.append(NSAttributedString(string: Format.clock(m.at), attributes: quiet)) }
            if let (mark, colour) = stateMark() {
                out.append(NSAttributedString(string: " " + mark, attributes: [.ctFont: fonts.small, .ctColour: colour.cgColor]))
            } else if m.mine, let (count, _) = Ticks.of(m.status, roles) {
                // Room for the ticks: an invisible x, kerned to their width, since trailing spaces
                // aren't measured.
                let x = NSAttributedString(string: "x", attributes: [.ctFont: fonts.time])
                let xWidth = width(of: CTLineCreateWithAttributedString(x))
                out.append(NSAttributedString(string: " ", attributes: [.ctFont: fonts.time]))
                out.append(NSAttributedString(string: "x", attributes: [.ctFont: fonts.time, .ctColour: CGColor.clear,
                                                                        .kern: Ticks.width(count, size: tickSize) - xWidth]))
            }
            return CTLineCreateWithAttributedString(out)
        }

        mutating func reactions(x: CGFloat, alignRight: Bool, right: CGFloat) {
            guard !m.reactions.isEmpty, !m.deleted else { return }
            var chips: [(CTLine, CGFloat, Reaction)] = []
            for r in m.reactions {
                let label = line(r.n > 1 ? "\(r.emoji) \(r.n)" : r.emoji, fonts.reaction, roles.text)
                chips.append((label, width(of: label) + 14, r))
            }
            let total = chips.reduce(0) { $0 + $1.1 + 4 } - 4
            var cx = alignRight ? right - total : x
            let h: CGFloat = 22
            y += 2
            for (label, w, r) in chips {
                let rect = CGRect(x: cx, y: y, width: w, height: h)
                parts.append(.fill(rect, (r.mine ? roles.selection : roles.surface).cgColor, h / 2))
                if r.mine { parts.append(.stroke(rect, roles.accent.cgColor, h / 2)) }
                parts.append(.line(label, CGPoint(x: cx + 7, y: y + 15)))
                regions.append((rect, .reaction(r.emoji)))
                cx += w + 4
            }
            y += h
        }

        mutating func failedLine(x: CGFloat, alignRight: Bool, right: CGFloat) {
            guard m.mine, m.status == .failed, !m.deleted else { return }
            let label = line("Not sent · Retry", fonts.small, roles.failed)
            let w = width(of: label)
            let lx = alignRight ? right - w : x
            y += 3
            parts.append(.line(label, CGPoint(x: lx, y: y + ascent(fonts.small))))
            regions.append((CGRect(x: lx, y: y, width: w, height: lineHeight(fonts.small)), .retry))
            y += lineHeight(fonts.small)
        }

        /// "Show more" under a folded message, at `top`; returns the height it takes.
        mutating func showMore(x: CGFloat, at top: CGFloat) -> CGFloat {
            let label = line("Show more", fonts.small, roles.accent)
            parts.append(.line(label, CGPoint(x: x, y: top + ascent(fonts.small))))
            regions.append((CGRect(x: x, y: top, width: width(of: label), height: lineHeight(fonts.small)), .showMore))
            return lineHeight(fonts.small) + 2
        }

        // MARK: Bubbles

        mutating func bubble() -> Laid {
            if emojiOnly() { return bigEmoji() }
            if m.kind == .sticker || m.kind == .videoNote, !m.deleted, m.reply == nil { return alone() }
            let most = min(width * 0.72, 520) - 2 * metrics.padX
            let p = metrics.padX, q = metrics.padY
            var inner: CGFloat = 0
            let name = (!m.mine && context.group && context.firstOfRun) ? m.from?.name : nil
            let nameLine = name.map { line($0, fonts.name, roles.sender(m.from?.colour ?? 0)) }
            let forwardedLine = m.forwarded > 0
                ? line(m.forwarded >= 5 ? "Forwarded many times" : "Forwarded", fonts.smallItalic, roles.secondary) : nil
            let card = cardSize(most: most)
            let said = words()
            let hasWords = said.length > 0
            let (frame, size, last, folded) = frame(said, most: most)
            let foot = footer()
            let footWidth = width(of: foot)
            inner = max(inner, hasWords ? size.width : 0)
            if let card { inner = max(inner, card.width) }
            if let nameLine { inner = max(inner, width(of: nameLine)) }
            if let forwardedLine { inner = max(inner, width(of: forwardedLine)) }
            let quoteHeight: CGFloat = m.reply == nil ? 0 : 40
            if m.reply != nil { inner = max(inner, min(most, 220)) }
            let under = card != nil && cardUnderText
            // The time sits on the text's last line when there's room, and on a line of its own if not.
            let footInline = !under && hasWords && last + footWidth + 12 <= most
            if footInline { inner = max(inner, last + footWidth + 12) } else { inner = max(inner, footWidth) }
            inner = min(inner, most)
            var h = q
            if nameLine != nil { h += lineHeight(fonts.name) + 1 }
            if forwardedLine != nil { h += lineHeight(fonts.small) }
            h += quoteHeight > 0 ? quoteHeight + 4 : 0
            let cardTop = h
            if let card, !under { h += card.height + (hasWords ? 6 : 4) }
            let textTop = h
            h += hasWords ? size.height : 0
            if folded { h += lineHeight(fonts.small) + 2 }
            if let card, under { h += 6 + card.height + 2 }
            if !footInline { h += lineHeight(fonts.time) }
            h += q
            let bw = inner + 2 * p
            let x = m.mine ? width - metrics.margin - bw : metrics.margin
            let rect = CGRect(x: x, y: y, width: bw, height: h)
            parts.append(.fill(rect, (m.mine ? roles.yours : roles.surface).cgColor, metrics.radius))
            var cy = y + q
            if let nameLine {
                parts.append(.line(nameLine, CGPoint(x: x + p, y: cy + ascent(fonts.name))))
                cy += lineHeight(fonts.name) + 1
            }
            if let forwardedLine {
                parts.append(.line(forwardedLine, CGPoint(x: x + p, y: cy + ascent(fonts.small))))
                cy += lineHeight(fonts.small)
            }
            if let reply = m.reply {
                quote(reply, CGRect(x: x + p - 4, y: cy, width: inner + 8, height: quoteHeight))
            }
            if let card, !under { drawCard(CGRect(x: x + p, y: y + cardTop, width: card.width, height: card.height)) }
            let textRect = CGRect(x: x + p, y: y + textTop, width: most, height: hasWords ? size.height : 0)
            if hasWords {
                parts.append(.frame(frame, textRect))
                text = (frame, textRect)
            }
            cy = y + textTop + (hasWords ? size.height : 0)
            if folded { cy += showMore(x: x + p, at: cy) }
            if let card, under {
                drawCard(CGRect(x: x + p, y: cy + 6, width: card.width, height: card.height))
                cy += 6 + card.height + 2
            }
            let footY = footInline ? cy - 4 - CTFontGetDescent(fonts.body) : cy + ascent(fonts.time)
            parts.append(contentsOf: footParts(foot, at: CGPoint(x: x + bw - p - footWidth, y: footY)))
            y += h
            reactions(x: x + 6, alignRight: m.mine, right: x + bw - 6)
            failedLine(x: x + p, alignRight: m.mine, right: x + bw)
            return Laid(height: ceil(y + 1), body: rect, parts: parts, regions: regions, text: text, pictures: slots, over: over)
        }

        mutating func quote(_ reply: Quote, _ rect: CGRect) {
            parts.append(.fill(rect, roles.text.opacity(roles.isDark ? 0.06 : 0.05).cgColor, 8))
            let colour = reply.from == "You" ? roles.accent : roles.sender(Self.colourIndex(reply.from))
            parts.append(.fill(CGRect(x: rect.minX, y: rect.minY, width: 3, height: rect.height), colour.cgColor, 1.5))
            let who = truncated(reply.from, fonts.name, colour, rect.width - 16)
            let what = truncated(reply.summary, fonts.small, roles.secondary, rect.width - 16)
            parts.append(.line(who, CGPoint(x: rect.minX + 10, y: rect.minY + 4 + ascent(fonts.name))))
            parts.append(.line(what, CGPoint(x: rect.minX + 10, y: rect.minY + 6 + lineHeight(fonts.name) + ascent(fonts.small))))
            regions.append((rect, .quote(reply.id)))
        }

        /// A quoted sender's colour, steady for a name, since a quote carries the name alone.
        static func colourIndex(_ name: String) -> Int {
            name.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0xFFFF } % 8
        }

        mutating func bigEmoji() -> Laid {
            let big = NSAttributedString(string: m.plain, attributes: [.ctFont: fonts.emoji, .ctColour: roles.text.cgColor])
            let l = CTLineCreateWithAttributedString(big)
            let w = width(of: l)
            let x = m.mine ? width - metrics.margin - w : metrics.margin
            let h = lineHeight(fonts.emoji)
            let rect = CGRect(x: x, y: y, width: w, height: h)
            parts.append(.line(l, CGPoint(x: x, y: y + ascent(fonts.emoji))))
            y += h
            let foot = footer()
            let fw = width(of: foot)
            parts.append(contentsOf: footParts(foot, at: CGPoint(x: m.mine ? width - metrics.margin - fw : x, y: y + ascent(fonts.time))))
            y += lineHeight(fonts.time)
            reactions(x: x, alignRight: m.mine, right: x + w)
            return Laid(height: ceil(y + 1), body: rect, parts: parts, regions: regions, text: nil, pictures: slots, over: over)
        }

        // MARK: Lines

        /// The terminal's layout grown up: others' messages under their name, which starts each run at
        /// the left in their colour; yours from a third of the way across; the time at the right edge
        /// when the minute or the sender changes, your ticks before it.
        mutating func lines() -> Laid {
            let m = metrics.margin
            let foot = footer(time: context.showTime || self.m.edited || self.m.mine)
            let footWidth = width(of: foot)
            let x = self.m.mine ? floor(width / 3) : m
            let most = max(width - x - m - footWidth - 16, 80)
            let top = y
            if !self.m.mine, context.firstOfRun {
                let name = self.m.from?.name ?? "them"
                let label = line(name, fonts.name, roles.sender(self.m.from?.colour ?? 0))
                parts.append(.line(label, CGPoint(x: m, y: y + ascent(fonts.name))))
                y += lineHeight(fonts.name) + 1
            }
            if self.m.forwarded > 0 {
                let label = line(self.m.forwarded >= 5 ? "Forwarded many times" : "Forwarded", fonts.smallItalic, roles.secondary)
                parts.append(.line(label, CGPoint(x: x, y: y + ascent(fonts.small))))
                y += lineHeight(fonts.small)
            }
            if let reply = self.m.reply {
                let colour = reply.from == "You" ? roles.accent : roles.sender(Self.colourIndex(reply.from))
                parts.append(.fill(CGRect(x: x, y: y + 2, width: 2, height: lineHeight(fonts.small)), colour.cgColor, 1))
                let who = line(reply.from, fonts.name, colour)
                let ww = width(of: who) + 6
                parts.append(.line(who, CGPoint(x: x + 8, y: y + 2 + ascent(fonts.small))))
                let what = truncated(reply.summary, fonts.small, roles.secondary, most - ww - 8)
                parts.append(.line(what, CGPoint(x: x + 8 + ww, y: y + 2 + ascent(fonts.small))))
                regions.append((CGRect(x: x, y: y, width: most, height: lineHeight(fonts.small) + 4), .quote(reply.id)))
                y += lineHeight(fonts.small) + 4
            }
            let footY = y + ascent(fonts.body)
            let card = cardSize(most: most)
            if let card, !cardUnderText {
                drawCard(CGRect(x: x, y: y + 2, width: card.width, height: card.height))
                y += card.height + 6
            }
            let said = words()
            let (frame, size, _, folded) = frame(said, most: most)
            if said.length > 0 {
                let textRect = CGRect(x: x, y: y, width: most, height: size.height)
                parts.append(.frame(frame, textRect))
                text = (frame, textRect)
                y += size.height
            }
            parts.append(contentsOf: footParts(foot, at: CGPoint(x: width - m - footWidth, y: footY)))
            if folded { y += showMore(x: x, at: y) }
            if let card, cardUnderText {
                drawCard(CGRect(x: x, y: y + 4, width: card.width, height: card.height))
                y += 4 + card.height + 2
            }
            reactions(x: x, alignRight: false, right: 0)
            failedLine(x: x, alignRight: false, right: 0)
            let body = CGRect(x: x - 4, y: top, width: width - x - m + 8, height: y - top)
            return Laid(height: ceil(y + 1), body: body, parts: parts, regions: regions, text: text, pictures: slots, over: over)
        }
    }
}
