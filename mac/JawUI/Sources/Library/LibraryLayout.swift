import CoreGraphics
import Foundation
import JawKit

/// What a library pane shows of its window, laid out at a width: headings, tiles and rows, each an
/// item of the grid with its frame, in the order they're drawn from the top, so where everything
/// goes is known from the window's items before any picture.
public struct LibraryLaid: Sendable, Hashable {
    public struct Element: Sendable, Hashable {
        public enum What: Sendable, Hashable {
            /// A month's heading, or a shelf's with its count and the kind a click on it goes to.
            case heading(String, count: String?, kind: LibraryKind?)
            /// A photo's or a video's tile, by its item's id.
            case tile(String)
            /// A voice message's, a file's or a link's row, by its item's id.
            case row(String)
            /// What's said where there's nothing to show: nothing shared, nothing matching, jawd's
            /// note, or why it refused the window.
            case said(String)
        }

        public let what: What
        public let frame: CGRect

        /// The item it draws, for a tile or a row.
        public var item: String? {
            switch what {
            case let .tile(id), let .row(id): id
            case .heading, .said: nil
            }
        }
    }

    public let elements: [Element]
    public let width: CGFloat
    public let height: CGFloat
    /// How many tiles a line holds, and their side.
    public let columns: Int
    public let side: CGFloat

    public static let empty = LibraryLaid(elements: [], width: 0, height: 0, columns: 1, side: LibraryLayout.most)

    /// The elements whose frames meet `rect`, by their index; the elements run down the page.
    public func indices(in rect: CGRect) -> Range<Int> {
        let first = Self.firstEnding(after: rect.minY, elements)
        var last = first
        while last < elements.count, elements[last].frame.minY <= rect.maxY {
            last += 1
        }
        return first ..< last
    }

    /// The first element whose foot is below `y`. Every line's elements share a top and a foot, and
    /// the lines run down the page, so the feet only grow.
    static func firstEnding(after y: CGFloat, _ elements: [Element]) -> Int {
        var lo = 0, hi = elements.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if elements[mid].frame.maxY < y { lo = mid + 1 } else { hi = mid }
        }
        return lo
    }

    /// Where the item `id` is drawn.
    public func element(of id: String) -> Int? {
        elements.firstIndex { $0.item == id }
    }
}

/// Lays out a library window's items as the design draws them: a kind's newest first under their
/// months, photos and videos as a grid of tiles and the rest as rows; every kind's as a shelf of
/// each, headed by the kind and its count. A tile is as wide as fits from 96 to 128 points, so at
/// 2x it's 256 pixels at most and takes jawd's 256 preview.
public enum LibraryLayout {
    public static let least: CGFloat = 96
    public static let most: CGFloat = 128
    public static let gap: CGFloat = 2

    /// The spaces and sizes a look and a density give the library.
    public struct Metrics: Sendable, Hashable {
        public let margin: CGFloat
        public let heading: CGFloat
        /// Between a heading's items and the next heading.
        public let section: CGFloat
        public let row: CGFloat
        /// Between rows: a card's gap in bubbles, none in lines, where a hairline parts them.
        public let rowGap: CGFloat
        /// A tile's corners.
        public let radius: CGFloat

        public init(_ style: Style) {
            let compact = style.density == .compact
            margin = Space.l
            heading = compact ? 24 : 30
            section = compact ? Space.s : Space.l
            row = compact ? 38 : 46
            rowGap = style.look == .bubbles ? (compact ? Space.xxs : Space.xs) : 0
            radius = style.look == .bubbles ? Radius.small + 2 : Radius.small / 2
        }
    }

    /// How many tiles fit across `width`, and their side, whole points: as many as leave each at most
    /// `most` points, fewer when that would make them less than `least`; a pane too narrow for two
    /// has one, at most `most` across.
    public static func columns(_ width: CGFloat, margin: CGFloat) -> (count: Int, side: CGFloat) {
        let room = max(width - 2 * margin, 1)
        var count = max(Int(((room + gap) / (most + gap)).rounded(.up)), 1)
        func side(_ n: Int) -> CGFloat {
            floor((room - CGFloat(n - 1) * gap) / CGFloat(n))
        }
        while count > 1, side(count) < least {
            count -= 1
        }
        return (count, min(side(count), most))
    }

    /// The shelves every kind's window shows, in their order: their heading, the kinds on them, how
    /// many lines of tiles or how many rows, and the kind a click on the heading goes to.
    static let shelves: [(title: String, kinds: [LibraryKind], most: Int, to: LibraryKind)] = [
        ("Photos and videos", [.photo, .video], 2, .photo),
        ("Files", [.file], 3, .file),
        ("Links", [.link], 3, .link),
        ("Voice messages", [.voice], 2, .voice),
    ]

    /// `items`, newest first, laid out at `width` for the window's `kind` (nil for every kind's
    /// shelves), with `counts` on the shelves' headings, `capped` when jawd counted only so far, and
    /// `said` in place of items when there are none.
    public static func lay(_ items: [LibraryRow], kind: LibraryKind?, counts: LibraryCounts?, capped: Bool, note: String?,
                           said empty: String?, width: CGFloat, style: Style, tz: TimeZone = .current) -> LibraryLaid
    {
        let m = Metrics(style)
        let (columns, side) = Self.columns(width, margin: m.margin)
        let inner = max(width - 2 * m.margin, 1)
        var out: [LibraryLaid.Element] = []
        var y = m.margin
        func heading(_ title: String, count: String? = nil, to: LibraryKind? = nil) {
            out.append(.init(what: .heading(title, count: count, kind: to), frame: CGRect(x: m.margin, y: y, width: inner, height: m.heading)))
            y += m.heading
        }
        func tiles(_ these: [LibraryRow]) {
            for (i, item) in these.enumerated() {
                let (line, column) = (i / columns, i % columns)
                out.append(.init(what: .tile(item.id), frame: CGRect(x: m.margin + CGFloat(column) * (side + gap),
                                                                     y: y + CGFloat(line) * (side + gap), width: side, height: side)))
            }
            let lines = (these.count + columns - 1) / columns
            y += CGFloat(lines) * side + CGFloat(max(lines - 1, 0)) * gap
        }
        func rows(_ these: [LibraryRow]) {
            for (i, item) in these.enumerated() {
                if i > 0 { y += m.rowGap }
                out.append(.init(what: .row(item.id), frame: CGRect(x: m.margin, y: y, width: inner, height: m.row)))
                y += m.row
            }
        }
        func said(_ words: String) {
            out.append(.init(what: .said(words), frame: CGRect(x: m.margin, y: y, width: inner, height: m.row)))
            y += m.row
        }
        let shown = items.filter { $0.kind != .other }
        if let kind {
            let these = shown.filter { $0.kind == kind }
            var start = 0
            while start < these.count {
                let month = Self.month(these[start].at, tz)
                let end = these[start...].firstIndex { Self.month($0.at, tz) != month } ?? these.count
                if start > 0 { y += m.section }
                heading(month)
                let group = Array(these[start ..< end])
                if kind == .photo || kind == .video { tiles(group) } else { rows(group) }
                start = end
            }
        } else {
            var first = true
            for shelf in shelves {
                let these = Array(shown.filter { shelf.kinds.contains($0.kind) }.prefix(shelf.kinds.contains(.photo) ? shelf.most * columns : shelf.most))
                let quiet = these.isEmpty ? (shelf.kinds == [.link] ? note : nil) : nil
                guard !these.isEmpty || quiet != nil else { continue }
                if !first { y += m.section }
                first = false
                let count = shelf.kinds.reduce(0) { $0 + (counts?[$1] ?? 0) }
                heading(shelf.title, count: Self.counted(count, capped: capped), to: shelf.to)
                if let quiet { said(quiet) } else if shelf.kinds.contains(.photo) { tiles(these) } else { rows(these) }
            }
        }
        if out.isEmpty, let empty { said(empty) }
        return LibraryLaid(elements: out, width: width, height: ceil(y + m.margin), columns: columns, side: side)
    }

    /// A month's heading: October 2026.
    static func month(_ at: Int64, _ tz: TimeZone) -> String {
        Format.formatted(at, "MMMM yyyy", tz)
    }

    /// How many there are, as the strip and the headings say it: `1,000+` where jawd stopped counting.
    public static func counted(_ n: Int, capped: Bool) -> String {
        capped && n >= LibraryCounts.counted ? "\(Format.num(n))+" : Format.num(n)
    }
}
