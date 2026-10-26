import CoreGraphics
import Foundation

/// The panes' layout: slots on a grid's lines, as the grid layouts design has it. Lines are
/// fractions of the width (`cols`) and the height (`rows`), from 0 to 1; a slot spans lines by
/// index and holds a chat or none. The slots cover the grid with no holes and no overlaps, every
/// line inside is some slot's edge, and a chat shows in one slot at most (`holds`).
///
/// Splitting and closing are edits to the grid, so the ad hoc splits and a layout drawn in the
/// editor follow the same rules. A value, tested on its own, and kept as JSON.
public struct GridLayout: Hashable, Sendable, Codable {
    public enum Axis: String, Hashable, Sendable, Codable {
        /// Side by side: a column line.
        case across
        /// One above the other: a row line.
        case down
    }

    public enum Direction: Sendable {
        case left, right, up, down
    }

    public enum Refusal: Error, Equatable, Sendable {
        case most, room

        public var said: String {
            switch self {
            case .most: "\(GridLayout.most) panes at most"
            case .room: "no room to split this pane; close one, or a bigger window"
            }
        }
    }

    public struct Slot: Hashable, Sendable, Codable, Identifiable {
        public let id: Int
        public var c0: Int
        public var c1: Int
        public var r0: Int
        public var r1: Int
        public var chat: String?

        public init(id: Int, c0: Int, c1: Int, r0: Int, r1: Int, chat: String? = nil) {
            self.id = id
            self.c0 = c0
            self.c1 = c1
            self.r0 = r0
            self.r1 = r1
            self.chat = chat
        }

        func spans(_ axis: Axis) -> (Int, Int) {
            axis == .across ? (c0, c1) : (r0, r1)
        }

        func across(_ axis: Axis) -> (Int, Int) {
            axis == .across ? (r0, r1) : (c0, c1)
        }
    }

    /// A line inside the grid, by its axis and index.
    public struct Line: Hashable, Sendable {
        public let axis: Axis
        public let index: Int

        public init(axis: Axis, index: Int) {
            self.axis = axis
            self.index = index
        }
    }

    /// A run of a line between slots, where a gutter is drawn and dragged.
    public struct Segment: Hashable, Sendable {
        public let line: Line
        public let rect: CGRect
    }

    /// A rectangle of the editor's cells.
    public struct Cells: Hashable, Sendable {
        public let c0: Int, r0: Int, c1: Int, r1: Int

        public init(c0: Int, r0: Int, c1: Int, r1: Int) {
            self.c0 = min(c0, c1)
            self.c1 = max(c0, c1)
            self.r0 = min(r0, r1)
            self.r1 = max(r0, r1)
        }
    }

    public static let most = 12
    public static let least: CGFloat = 240
    public static let columnsMost = 12
    public static let rowsMost = 8
    public static let columnsNew = 6
    public static let rowsNew = 4

    public private(set) var cols: [Double]
    public private(set) var rows: [Double]
    public private(set) var slots: [Slot]
    /// The next slot's id; ids aren't reused within a layout's life on screen.
    var next: Int

    public init(cols: [Double], rows: [Double], slots: [Slot]) {
        self.cols = cols
        self.rows = rows
        self.slots = slots
        next = (slots.map(\.id).max() ?? 0) + 1
    }

    /// One pane, the whole grid.
    public static func single(_ chat: String?) -> GridLayout {
        GridLayout(cols: [0, 1], rows: [0, 1], slots: [Slot(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: chat)])
    }

    // MARK: Reading it

    /// Top to bottom, then left to right: ⌥⌘1 is the first.
    public var reading: [Slot] {
        slots.sorted { ($0.r0, $0.c0) < ($1.r0, $1.c0) }
    }

    public func slot(_ id: Int) -> Slot? {
        slots.first { $0.id == id }
    }

    public func slot(showing chat: String) -> Slot? {
        slots.first { $0.chat == chat }
    }

    public var chats: [String] {
        reading.compactMap(\.chat)
    }

    func lines(_ axis: Axis) -> [Double] {
        axis == .across ? cols : rows
    }

    /// Each slot's frame in `rect`, in a top-down space, edges from the lines so neighbours meet.
    public func frames(in rect: CGRect) -> [Int: CGRect] {
        var out: [Int: CGRect] = [:]
        for s in slots {
            out[s.id] = frame(s, in: rect)
        }
        return out
    }

    func frame(_ s: Slot, in rect: CGRect) -> CGRect {
        let x0 = rect.minX + cols[s.c0] * rect.width, x1 = rect.minX + cols[s.c1] * rect.width
        let y0 = rect.minY + rows[s.r0] * rect.height, y1 = rect.minY + rows[s.r1] * rect.height
        return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }

    /// The runs of lines between slots, for gutters: a column line from the top of the first slot
    /// beside it to the bottom of the last, wherever slots meet along it.
    public func lines(in rect: CGRect) -> [Segment] {
        var out: [Segment] = []
        for axis in [Axis.across, .down] {
            let lines = lines(axis)
            guard lines.count > 2 else { continue }
            for k in 1 ..< lines.count - 1 {
                let spans = slots.filter { $0.spans(axis).1 == k }.map { $0.across(axis) }.sorted { $0.0 < $1.0 }
                var runs: [(Int, Int)] = []
                for span in spans {
                    if let last = runs.last, last.1 == span.0 { runs[runs.count - 1].1 = span.1 } else { runs.append(span) }
                }
                let other = axis == .across ? rows : cols
                for (a, b) in runs {
                    let at = (axis == .across ? rect.minX + lines[k] * rect.width : rect.minY + lines[k] * rect.height)
                    let r = axis == .across
                        ? CGRect(x: at, y: rect.minY + other[a] * rect.height, width: 0, height: (other[b] - other[a]) * rect.height)
                        : CGRect(x: rect.minX + other[a] * rect.width, y: at, width: (other[b] - other[a]) * rect.width, height: 0)
                    out.append(Segment(line: Line(axis: axis, index: k), rect: r))
                }
            }
        }
        return out
    }

    /// The slot next to `id` that way, the nearest whose middle lies beyond its edge, as tmux finds
    /// it; frames in a top-down space.
    public func neighbour(of id: Int, _ direction: Direction) -> Int? {
        let unit = CGRect(x: 0, y: 0, width: 1000, height: 1000)
        let all = frames(in: unit)
        guard let from = all[id] else { return nil }
        let beyond: (CGRect) -> Bool = switch direction {
        case .left: { $0.maxX <= from.minX + 0.5 }
        case .right: { $0.minX >= from.maxX - 0.5 }
        case .up: { $0.maxY <= from.minY + 0.5 }
        case .down: { $0.minY >= from.maxY - 0.5 }
        }
        let overlaps: (CGRect) -> Bool = switch direction {
        case .left, .right: { $0.minY < from.maxY && $0.maxY > from.minY }
        case .up, .down: { $0.minX < from.maxX && $0.maxX > from.minX }
        }
        let distance = { (r: CGRect) in hypot(r.midX - from.midX, r.midY - from.midY) }
        return all.filter { $0.key != id && beyond($0.value) && overlaps($0.value) }
            .min { (distance($0.value), $0.key) < (distance($1.value), $1.key) }?.key
    }

    /// What always holds of a layout on screen.
    public var holds: Bool {
        coversAll && slots.count <= Self.most && Set(chats).count == chats.count && everyLineUsed
    }

    /// The slots cover every cell between the lines once, and the lines are in order.
    public var coversAll: Bool {
        guard cols.first == 0, cols.last == 1, rows.first == 0, rows.last == 1,
              zip(cols, cols.dropFirst()).allSatisfy({ $0 < $1 }), zip(rows, rows.dropFirst()).allSatisfy({ $0 < $1 })
        else { return false }
        var seen = Array(repeating: 0, count: (cols.count - 1) * (rows.count - 1))
        for s in slots {
            guard s.c0 >= 0, s.c0 < s.c1, s.c1 < cols.count, s.r0 >= 0, s.r0 < s.r1, s.r1 < rows.count else { return false }
            for r in s.r0 ..< s.r1 {
                for c in s.c0 ..< s.c1 {
                    seen[r * (cols.count - 1) + c] += 1
                }
            }
        }
        return seen.allSatisfy { $0 == 1 }
    }

    var everyLineUsed: Bool {
        (1 ..< max(cols.count - 1, 1)).allSatisfy { k in slots.contains { $0.c0 == k || $0.c1 == k } }
            && (1 ..< max(rows.count - 1, 1)).allSatisfy { k in slots.contains { $0.r0 == k || $0.r1 == k } }
    }

    /// The layout without its chats, to tell a change of shape from a change of chats.
    public var shape: GridLayout {
        var g = self
        for i in g.slots.indices {
            g.slots[i].chat = nil
        }
        return g
    }

    // MARK: Editing it

    /// Splits slot `id` along `axis`: at the line inside it nearest its middle when it spans
    /// several, or at a new line through its middle. The new slot holds `chat` and comes after.
    public func splitting(_ id: Int, _ axis: Axis, chat: String?, size: CGSize) -> Result<(GridLayout, Int), Refusal> {
        guard let s = slot(id) else { return .failure(.room) }
        guard slots.count < Self.most else { return .failure(.most) }
        var g = self
        var lines = lines(axis)
        let (a0, a1) = s.spans(axis)
        let length = Double(axis == .across ? size.width : size.height)
        let least = Double(Self.least)
        let k: Int
        if a1 - a0 > 1 {
            let mid = (lines[a0] + lines[a1]) / 2
            k = (a0 + 1 ..< a1).min { abs(lines[$0] - mid) < abs(lines[$1] - mid) }!
            guard (lines[k] - lines[a0]) * length >= least, (lines[a1] - lines[k]) * length >= least else { return .failure(.room) }
        } else {
            guard (lines[a1] - lines[a0]) * length >= 2 * least else { return .failure(.room) }
            k = a0 + 1
            lines.insert((lines[a0] + lines[a1]) / 2, at: k)
            for i in g.slots.indices {
                if axis == .across {
                    if g.slots[i].c0 >= k { g.slots[i].c0 += 1 }
                    if g.slots[i].c1 >= k { g.slots[i].c1 += 1 }
                } else {
                    if g.slots[i].r0 >= k { g.slots[i].r0 += 1 }
                    if g.slots[i].r1 >= k { g.slots[i].r1 += 1 }
                }
            }
            if axis == .across { g.cols = lines } else { g.rows = lines }
        }
        let at = g.slots.firstIndex { $0.id == id }!
        var second = g.slots[at]
        second = Slot(id: g.next, c0: second.c0, c1: second.c1, r0: second.r0, r1: second.r1, chat: chat)
        if axis == .across {
            g.slots[at].c1 = k
            second.c0 = k
        } else {
            g.slots[at].r1 = k
            second.r0 = k
        }
        g.slots.insert(second, at: at + 1)
        g.next += 1
        return .success((g, second.id))
    }

    /// Closes slot `id`: the neighbour sharing its whole edge grows into its place, the one before
    /// it in reading order first, and lines no slot uses go; it returns the slot that grew. With no
    /// such neighbour (a pinwheel) the slot stays, empty. The last slot stays as it is.
    public func closing(_ id: Int) -> (GridLayout, Int?) {
        guard let s = slot(id), slots.count > 1 else { return (self, nil) }
        let sharing = slots.filter { n in
            n.id != id && ((n.r0 == s.r0 && n.r1 == s.r1 && (n.c1 == s.c0 || n.c0 == s.c1))
                || (n.c0 == s.c0 && n.c1 == s.c1 && (n.r1 == s.r0 || n.r0 == s.r1)))
        }
        let order = reading.map(\.id)
        let at = order.firstIndex(of: id) ?? 0
        let place = { (n: Slot) in order.firstIndex(of: n.id) ?? 0 }
        let before = sharing.filter { place($0) < at }.max { place($0) < place($1) }
        let after = sharing.filter { place($0) > at }.min { place($0) < place($1) }
        var g = self
        guard let n = before ?? after else {
            if let i = g.slots.firstIndex(where: { $0.id == id }) { g.slots[i].chat = nil }
            return (g, nil)
        }
        g.slots.removeAll { $0.id == id }
        if let i = g.slots.firstIndex(where: { $0.id == n.id }) {
            g.slots[i].c0 = min(n.c0, s.c0)
            g.slots[i].c1 = max(n.c1, s.c1)
            g.slots[i].r0 = min(n.r0, s.r0)
            g.slots[i].r1 = max(n.r1, s.r1)
        }
        return (g.pruned(), n.id)
    }

    /// With `chat` in slot `id`, and out of any other slot that showed it.
    public func with(chat: String?, in id: Int) -> GridLayout {
        var g = self
        for i in g.slots.indices {
            if g.slots[i].id == id {
                g.slots[i].chat = chat
            } else if chat != nil, g.slots[i].chat == chat {
                g.slots[i].chat = nil
            }
        }
        return g
    }

    /// With the chats of slots `a` and `b` exchanged; the shape stays.
    public func swapping(_ a: Int, _ b: Int) -> GridLayout {
        guard let x = slot(a), let y = slot(b) else { return self }
        var g = self
        for i in g.slots.indices {
            if g.slots[i].id == a { g.slots[i].chat = y.chat }
            if g.slots[i].id == b { g.slots[i].chat = x.chat }
        }
        return g
    }

    /// With `line` moved to `position`, every slot along it resizing, none under 240 points.
    public func moving(_ line: Line, to position: Double, size: CGSize) -> GridLayout {
        let lines = lines(line.axis)
        let k = line.index
        guard k > 0, k < lines.count - 1 else { return self }
        let length = Double(line.axis == .across ? size.width : size.height)
        let least = length > 0 ? Double(Self.least) / length : 0
        let ending = slots.filter { $0.spans(line.axis).1 == k }.map { lines[$0.spans(line.axis).0] + least }
        let starting = slots.filter { $0.spans(line.axis).0 == k }.map { lines[$0.spans(line.axis).1] - least }
        let low = ending.max() ?? lines[k - 1], high = starting.min() ?? lines[k + 1]
        guard low <= high else { return self }
        var g = self
        let to = min(max(position, low), high)
        if line.axis == .across { g.cols[k] = to } else { g.rows[k] = to }
        return g
    }

    /// Drops lines no slot's edge uses.
    public func pruned() -> GridLayout {
        var g = self
        for axis in [Axis.across, .down] {
            var k = g.lines(axis).count - 2
            while k >= 1 {
                if !g.slots.contains(where: { $0.spans(axis).0 == k || $0.spans(axis).1 == k }) {
                    if axis == .across { g.cols.remove(at: k) } else { g.rows.remove(at: k) }
                    for i in g.slots.indices {
                        if axis == .across {
                            if g.slots[i].c0 > k { g.slots[i].c0 -= 1 }
                            if g.slots[i].c1 > k { g.slots[i].c1 -= 1 }
                        } else {
                            if g.slots[i].r0 > k { g.slots[i].r0 -= 1 }
                            if g.slots[i].r1 > k { g.slots[i].r1 -= 1 }
                        }
                    }
                }
                k -= 1
            }
        }
        return g
    }

    // MARK: A tree of splits

    /// The layout a tree of splits draws, its panes' chats in their slots.
    public static func from(_ tree: PaneLayout) -> GridLayout {
        let unit = CGRect(x: 0, y: 0, width: 1, height: 1)
        let frames = tree.frames(in: unit)
        let xs = Set(frames.values.flatMap { [Double($0.minX), Double($0.maxX)] }).sorted()
        let ys = Set(frames.values.flatMap { [Double($0.minY), Double($0.maxY)] }).sorted()
        var slots: [Slot] = []
        for (i, id) in tree.panes.enumerated() {
            guard let f = frames[id] else { continue }
            slots.append(Slot(id: i + 1, c0: xs.firstIndex(of: Double(f.minX))!, c1: xs.firstIndex(of: Double(f.maxX))!,
                              r0: ys.firstIndex(of: Double(f.minY))!, r1: ys.firstIndex(of: Double(f.maxY))!, chat: id))
        }
        return GridLayout(cols: xs, rows: ys, slots: slots)
    }

    // MARK: The editor's cells

    /// The layout on a grid of `columns` by `rows` cells: each line snapped to the nearest cell's
    /// edge, kept in order; every cell's line is kept, used or not, until `pruned`.
    public func onCells(columns: Int, rows rowCount: Int) -> GridLayout {
        let columns = max(columns, cols.count - 1), rowCount = max(rowCount, rows.count - 1)
        func snap(_ lines: [Double], _ n: Int) -> [Int] {
            var out: [Int] = []
            for (i, x) in lines.enumerated() {
                let left = lines.count - 1 - i
                let floor = (out.last ?? -1) + 1
                out.append(min(max(Int((x * Double(n)).rounded()), floor), n - left))
            }
            return out
        }
        let cx = snap(cols, columns), ry = snap(rows, rowCount)
        var g = GridLayout(cols: (0 ... columns).map { Double($0) / Double(columns) }, rows: (0 ... rowCount).map { Double($0) / Double(rowCount) },
                           slots: slots.map { Slot(id: $0.id, c0: cx[$0.c0], c1: cx[$0.c1], r0: ry[$0.r0], r1: ry[$0.r1], chat: $0.chat) })
        g.next = max(g.next, next)
        return g
    }

    /// The editor's merge: the cells dragged across, grown until they cut no slot, become one slot,
    /// the first slot's in reading order, with the first chat among them.
    public func merging(_ cells: Cells) -> GridLayout {
        var r = cells
        var grew = true
        while grew {
            grew = false
            for s in slots {
                let meets = s.c0 < r.c1 + 1 && s.c1 > r.c0 && s.r0 < r.r1 + 1 && s.r1 > r.r0
                let inside = s.c0 >= r.c0 && s.c1 <= r.c1 + 1 && s.r0 >= r.r0 && s.r1 <= r.r1 + 1
                if meets, !inside {
                    r = Cells(c0: min(r.c0, s.c0), r0: min(r.r0, s.r0), c1: max(r.c1, s.c1 - 1), r1: max(r.r1, s.r1 - 1))
                    grew = true
                }
            }
        }
        let taken = reading.filter { $0.c0 >= r.c0 && $0.c1 <= r.c1 + 1 && $0.r0 >= r.r0 && $0.r1 <= r.r1 + 1 }
        guard let first = taken.first, taken.count > 1 else { return self }
        var g = self
        g.slots.removeAll { s in taken.contains { $0.id == s.id } }
        g.slots.append(Slot(id: first.id, c0: r.c0, c1: r.c1 + 1, r0: r.r0, r1: r.r1 + 1, chat: taken.compactMap(\.chat).first))
        return g
    }

    /// The editor's split: slot `id` back into one slot a cell, the first keeping its id and chat.
    public func intoCells(_ id: Int) -> Result<GridLayout, Refusal> {
        guard let s = slot(id) else { return .success(self) }
        let count = (s.c1 - s.c0) * (s.r1 - s.r0)
        guard slots.count - 1 + count <= Self.most else { return .failure(.most) }
        var g = self
        let at = g.slots.firstIndex { $0.id == id }!
        g.slots.remove(at: at)
        var made: [Slot] = []
        for r in s.r0 ..< s.r1 {
            for c in s.c0 ..< s.c1 {
                let first = made.isEmpty
                made.append(Slot(id: first ? s.id : g.next, c0: c, c1: c + 1, r0: r, r1: r + 1, chat: first ? s.chat : nil))
                if !first { g.next += 1 }
            }
        }
        g.slots.insert(contentsOf: made, at: at)
        return .success(g)
    }

    // MARK: Equality and keeping

    public static func == (a: GridLayout, b: GridLayout) -> Bool {
        a.cols == b.cols && a.rows == b.rows && a.slots == b.slots
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(cols)
        hasher.combine(rows)
        hasher.combine(slots)
    }

    enum Keys: String, CodingKey {
        case cols, rows, slots
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        self.init(cols: try c.decode([Double].self, forKey: .cols), rows: try c.decode([Double].self, forKey: .rows),
                  slots: try c.decode([Slot].self, forKey: .slots))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(cols, forKey: .cols)
        try c.encode(rows, forKey: .rows)
        try c.encode(slots, forKey: .slots)
    }
}
