import CoreGraphics
import CoreText
import Foundation
import JawKit
import JawThemes

/// What a message's lines depend on beyond the message: whether its sender's name and its time
/// show, the day above it, and whether a long one is unfolded.
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

    enum Part {
        case fill(CGRect, CGColor, CGFloat)
        case stroke(CGRect, CGColor, CGFloat)
        case frame(CTFrame, CGRect)
        /// A line whose baseline sits at the point, measured from the top.
        case line(CTLine, CGPoint)
    }

    init(height: CGFloat, body: CGRect, parts: [Part], regions: [(CGRect, Region)], text: (CTFrame, CGRect)?) {
        self.height = height
        self.body = body
        self.parts = parts
        self.regions = regions
        self.text = text
    }

    /// Draws into a flipped context, its top at `y`.
    public func draw(in ctx: CGContext, y: CGFloat = 0) {
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
            }
        }
    }

    /// What's at a point, in the layout's own coordinates.
    public func region(at point: CGPoint) -> Region? {
        regions.first { $0.0.contains(point) }?.1
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
            return Laid(height: ceil(y), body: rect, parts: parts, regions: regions, text: nil)
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
            if let kind = Self.kindLine(m) {
                out.append(NSAttributedString(string: kind, attributes: [.ctFont: fonts.italic, .ctColour: roles.secondary.cgColor]))
                if !m.spans.isEmpty { out.append(NSAttributedString(string: "\n", attributes: base)) }
            }
            for span in m.spans {
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

        /// A media message's one line, until Mac-M2 draws it inline.
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
            case .album: return "Album of \(m.album?.items.count ?? 0)"
            case .location: return "Location"
            case .liveLocation: return "Live location"
            case .contact: return "Contact card"
            case .poll: return "Poll · " + (m.poll?.question ?? "vote on the phone")
            case .event: return "Event"
            case .unknown: return "jaw can't show this yet"
            }
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

        func ticks() -> (String, RGBA)? {
            guard m.mine, let status = m.status else { return nil }
            switch status {
            case .pending: return ("◷", roles.tertiary)
            case .sent: return ("✓", roles.tertiary)
            case .delivered: return ("✓✓", roles.tertiary)
            case .read, .played: return ("✓✓", roles.accent)
            case .failed: return ("!", roles.failed)
            }
        }

        /// The time and what's said around it: a star, Edited, your ticks.
        func footer(time: Bool = true) -> CTLine {
            let out = NSMutableAttributedString()
            let quiet: [NSAttributedString.Key: Any] = [.ctFont: fonts.time, .ctColour: roles.secondary.cgColor]
            if m.starred { out.append(NSAttributedString(string: "★ ", attributes: quiet)) }
            if m.edited, !m.deleted { out.append(NSAttributedString(string: "Edited ", attributes: [.ctFont: fonts.smallItalic, .ctColour: roles.secondary.cgColor])) }
            if time { out.append(NSAttributedString(string: Format.clock(m.at), attributes: quiet)) }
            if let (tick, colour) = ticks() {
                out.append(NSAttributedString(string: " " + tick, attributes: [
                    .ctFont: fonts.small, .ctColour: colour.cgColor, .kern: tick == "✓✓" ? -3 : 0,
                ]))
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
            let most = min(width * 0.72, 520) - 2 * metrics.padX
            let p = metrics.padX, q = metrics.padY
            var inner: CGFloat = 0
            let name = (!m.mine && context.group && context.firstOfRun) ? m.from?.name : nil
            let nameLine = name.map { line($0, fonts.name, roles.sender(m.from?.colour ?? 0)) }
            let forwardedLine = m.forwarded > 0
                ? line(m.forwarded >= 5 ? "Forwarded many times" : "Forwarded", fonts.smallItalic, roles.secondary) : nil
            let (frame, size, last, folded) = frame(words(), most: most)
            let foot = footer()
            let footWidth = width(of: foot)
            inner = max(inner, size.width)
            if let nameLine { inner = max(inner, width(of: nameLine)) }
            if let forwardedLine { inner = max(inner, width(of: forwardedLine)) }
            let quoteHeight: CGFloat = m.reply == nil ? 0 : 40
            if m.reply != nil { inner = max(inner, min(most, 220)) }
            // The time sits on the text's last line when there's room, and on a line of its own if not.
            let footInline = last + footWidth + 12 <= most
            if footInline { inner = max(inner, last + footWidth + 12) } else { inner = max(inner, footWidth) }
            inner = min(inner, most)
            var h = q
            if nameLine != nil { h += lineHeight(fonts.name) + 1 }
            if forwardedLine != nil { h += lineHeight(fonts.small) }
            h += quoteHeight > 0 ? quoteHeight + 4 : 0
            let textTop = h
            h += size.height
            if folded { h += lineHeight(fonts.small) + 2 }
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
            let textRect = CGRect(x: x + p, y: y + textTop, width: most, height: size.height)
            parts.append(.frame(frame, textRect))
            text = (frame, textRect)
            cy = y + textTop + size.height
            if folded { cy += showMore(x: x + p, at: cy) }
            let footY = footInline ? cy - 4 - CTFontGetDescent(fonts.body) : cy + ascent(fonts.time)
            parts.append(.line(foot, CGPoint(x: x + bw - p - footWidth, y: footY)))
            y += h
            reactions(x: x + 6, alignRight: m.mine, right: x + bw - 6)
            failedLine(x: x + p, alignRight: m.mine, right: x + bw)
            return Laid(height: ceil(y + 1), body: rect, parts: parts, regions: regions, text: text)
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
            parts.append(.line(foot, CGPoint(x: m.mine ? width - metrics.margin - fw : x, y: y + ascent(fonts.time))))
            y += lineHeight(fonts.time)
            reactions(x: x, alignRight: m.mine, right: x + w)
            return Laid(height: ceil(y + 1), body: rect, parts: parts, regions: regions, text: nil)
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
            let (frame, size, _, folded) = frame(words(), most: most)
            let textRect = CGRect(x: x, y: y, width: most, height: size.height)
            parts.append(.frame(frame, textRect))
            text = (frame, textRect)
            parts.append(.line(foot, CGPoint(x: width - m - footWidth, y: y + ascent(fonts.body))))
            y += size.height
            if folded { y += showMore(x: x, at: y) }
            reactions(x: x, alignRight: false, right: 0)
            failedLine(x: x, alignRight: false, right: 0)
            let body = CGRect(x: x - 4, y: top, width: width - x - m + 8, height: y - top)
            return Laid(height: ceil(y + 1), body: body, parts: parts, regions: regions, text: text)
        }
    }
}
