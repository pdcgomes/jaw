import Foundation
import JawKit

/// A filter set on the library's search line, shown as a token, as the terminal's chips.
public enum LibraryToken: Sendable, Hashable {
    /// A sender: `person:` and a number, or `me`.
    case from(id: String, name: String)
    case `in`(id: String, name: String)
    /// From the start of a day or a month.
    case since(at: Int64, label: String)
    /// To the end of a day or a month, both included.
    case until(at: Int64, label: String)

    /// As its token says it: "from: Ana".
    public var said: String {
        switch self {
        case let .from(_, name): "from: \(name)"
        case let .in(_, name): "in: \(name)"
        case let .since(_, label): "since: \(label)"
        case let .until(_, label): "until: \(label)"
        }
    }

    /// The filter it is: one token of each at most.
    var filter: String {
        switch self {
        case .from: "from:"
        case .in: "in:"
        case .since: "since:"
        case .until: "until:"
        }
    }
}

/// What a filter being typed could become, as its offer says it.
public struct LibraryOffer: Sendable, Hashable {
    public let token: LibraryToken
    public let said: String
}

/// The library's search line, as the terminal's: words, which match captions, file names and link
/// titles, and tokens for `from:`, `in:`, `since:` and `until:`, each typed and taken from what it
/// could become. It's a value: the pane gives it what's typed and asks it what to ask jawd.
public struct LibrarySearch: Sendable, Hashable {
    public static let filters = ["from:", "in:", "since:", "until:"]
    /// The most offers shown at once.
    public static let most = 6

    /// The words in the field, a filter being typed among them.
    public var text = ""
    public var tokens: [LibraryToken] = []
    /// The text Esc put the offers away for, until it changes.
    public var dismissed: String?

    public init(text: String = "", tokens: [LibraryToken] = []) {
        self.text = text
        self.tokens = tokens
    }

    /// The filter being typed at the end of `before`, the field's text up to the caret: where it starts
    /// in it, its name, and what follows the colon. One name can have several words: the last filter
    /// whose value has three words at most is still being typed.
    public static func typing(_ before: String) -> (start: Int, name: String, rest: String)? {
        let chars = Array(before)
        let start = (chars.lastIndex(of: " ").map { $0 + 1 }) ?? 0
        let word = String(chars[start...])
        for name in filters where word.hasPrefix(name) {
            return (start, name, String(word.dropFirst(name.count)))
        }
        for name in filters {
            guard let at = before.range(of: name, options: .backwards) else { continue }
            let i = before.distance(from: before.startIndex, to: at.lowerBound)
            guard i == 0 || chars[i - 1] == " " else { continue }
            let rest = String(before[at.upperBound...])
            if !rest.isEmpty, rest.split(separator: " ", omittingEmptySubsequences: false).count <= 3 {
                return (i, name, rest)
            }
        }
        return nil
    }

    /// The words to ask jawd for: what's typed, but a filter still being typed at its end.
    public func words(before: String? = nil) -> String {
        let before = before ?? text
        guard let (start, _, _) = Self.typing(before) else { return text.trimmingCharacters(in: .whitespaces) }
        return String(Array(text).prefix(start)).trimmingCharacters(in: .whitespaces)
    }

    /// Whether anything narrows what's shown beyond its kind and chat: words, a person or a time.
    public func capped(before: String? = nil) -> Bool {
        !words(before: before).isEmpty || tokens.contains { if case .in = $0 { false } else { true } }
    }

    /// Whether anything narrows what's shown beyond its kind: words or a token.
    public var filtered: Bool {
        !words().isEmpty || !tokens.isEmpty
    }

    /// What the filter being typed could become, best first, six at most: you, and the people given,
    /// for `from:`; the chats given for `in:`; the terminal's days and months, or a date typed, for
    /// `since:` and `until:`. Nothing once Esc put them away, until the text changes.
    public func offers(before: String, people: [(id: String, name: String)], chats: [(id: String, name: String)], now: Int64,
                       tz: TimeZone = .current) -> [LibraryOffer]
    {
        guard let (_, name, rest) = Self.typing(before), dismissed != text else { return [] }
        let typed = rest.trimmingCharacters(in: .whitespaces)
        var out: [LibraryOffer] = []
        switch name {
        case "from:":
            var seen: Set<String> = []
            for p in [(id: "me", name: "you")] + people where seen.insert(p.id).inserted && Self.matches(typed, p.name) {
                out.append(LibraryOffer(token: .from(id: p.id, name: p.name), said: p.name))
            }
        case "in:":
            for c in chats where Self.matches(typed, c.name) {
                out.append(LibraryOffer(token: .in(id: c.id, name: c.name), said: c.name))
            }
        default:
            let since = name == "since:"
            for (label, start, end) in Self.dates(typed, now: now, tz: tz) {
                out.append(LibraryOffer(token: since ? .since(at: start, label: label) : .until(at: end, label: label), said: label))
            }
        }
        return Array(out.prefix(Self.most))
    }

    /// Whether `typed` names `name`: each of its words starts one of the name's, whatever their case
    /// and accents, as the box completes names.
    static func matches(_ typed: String, _ name: String) -> Bool {
        let words = fold(typed).split(separator: " ")
        guard !words.isEmpty else { return true }
        let parts = fold(name).split(separator: " ")
        return words.allSatisfy { w in parts.contains { $0.hasPrefix(w) } }
    }

    static func fold(_ s: String) -> String {
        s.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
    }

    /// The days and months a `since:` or `until:` could mean, as the terminal offers them: today,
    /// yesterday, a week, a month and a year ago, then the last twelve months; or a date typed as
    /// 2026-08-01. Each with its first and last millisecond, so `until:` takes in the whole of it.
    static func dates(_ typed: String, now: Int64, tz: TimeZone) -> [(String, Int64, Int64)] {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = tz
        let today = calendar.startOfDay(for: Date(timeIntervalSince1970: Double(now) / 1000))
        func ms(_ d: Date) -> Int64 {
            Int64((d.timeIntervalSince1970 * 1000).rounded())
        }
        func day(_ d: Date) -> (Int64, Int64) {
            (ms(d), ms(calendar.date(byAdding: .day, value: 1, to: d)!) - 1)
        }
        let parse = DateFormatter()
        parse.locale = Locale(identifier: "en_GB")
        parse.timeZone = tz
        parse.dateFormat = "yyyy-MM-dd"
        if let d = parse.date(from: typed.trimmingCharacters(in: .whitespaces)) {
            let (a, b) = day(d)
            return [(Format.formatted(a, "d MMM yyyy", tz), a, b)]
        }
        var out: [(String, Int64, Int64)] = []
        for (label, back) in [("today", 0), ("yesterday", 1), ("a week ago", 7), ("a month ago", 30), ("a year ago", 365)] {
            let (a, b) = day(calendar.date(byAdding: .day, value: -back, to: today)!)
            out.append((label, a, b))
        }
        var month = calendar.date(from: calendar.dateComponents([.year, .month], from: today))!
        for _ in 0 ..< 12 {
            let next = calendar.date(byAdding: .month, value: 1, to: month)!
            out.append((Format.formatted(ms(month), "MMMM yyyy", tz), ms(month), ms(next) - 1))
            month = calendar.date(byAdding: .month, value: -1, to: month)!
        }
        let t = fold(typed)
        return out.filter { t.isEmpty || fold($0.0).hasPrefix(t) }
    }

    /// Takes an offer: the filter being typed goes from the text, and its token takes the place of one
    /// of its filter already there.
    public mutating func take(_ offer: LibraryOffer, before: String) {
        guard let (start, _, _) = Self.typing(before) else { return }
        let chars = Array(text)
        let after = Array(before).count
        text = (String(chars.prefix(start)) + String(chars.dropFirst(after))).trimmingCharacters(in: .whitespaces)
        if !text.isEmpty { text += " " }
        tokens.removeAll { $0.filter == offer.token.filter }
        tokens.append(offer.token)
        dismissed = nil
    }

    /// ⌫ in an empty field: the last token goes back; says whether one did.
    @discardableResult
    public mutating func takeBack() -> Bool {
        guard text.isEmpty, !tokens.isEmpty else { return false }
        tokens.removeLast()
        return true
    }

    /// What's asked of jawd: `kind`, `in:`'s chat or the pane's, the sender, the time and the words.
    public func filter(kind: LibraryKind?, chat: String?, before: String? = nil) -> LibraryFilter {
        var f = LibraryFilter(kind: kind, chat: chat, text: words(before: before))
        for token in tokens {
            switch token {
            case let .from(id, _): f.person = id
            case let .in(id, _): f.chat = id
            case let .since(at, _): f.since = at
            case let .until(at, _): f.until = at
            }
        }
        return f
    }
}
