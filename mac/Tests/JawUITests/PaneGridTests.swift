import CoreGraphics
import Foundation
@testable import JawUI
import Testing

/// The grid's rules: slots on lines that cover the grid with no holes, split by adding a line,
/// closed by giving the space to a neighbour sharing a whole edge, and the editor's cells.

let room = CGSize(width: 1200, height: 800)
let square = CGRect(x: 0, y: 0, width: 1000, height: 1000)

/// A slot's columns and rows, to compare.
func span(_ s: PaneGrid.Slot?) -> [Int] {
    s.map { [$0.c0, $0.c1, $0.r0, $0.r1] } ?? []
}

func split(_ g: PaneGrid, _ id: Int, _ axis: PaneGrid.Axis, _ chat: String?) throws -> (PaneGrid, Int) {
    try g.splitting(id, axis, chat: chat, size: room).get()
}

/// Work: a big slot on the left, Ana above Lisbon and an empty slot on the right.
func work() -> PaneGrid {
    PaneGrid(cols: [0, 0.5, 0.75, 1], rows: [0, 0.5, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 2, chat: "team"),
        .init(id: 2, c0: 1, c1: 3, r0: 0, r1: 1, chat: "ana"),
        .init(id: 3, c0: 1, c1: 2, r0: 1, r1: 2, chat: "lisbon"),
        .init(id: 4, c0: 2, c1: 3, r0: 1, r1: 2, chat: nil),
    ])
}

/// Four slots turning round the middle, where no two share a whole edge.
func pinwheel() -> PaneGrid {
    PaneGrid(cols: [0, 1.0 / 3, 2.0 / 3, 1], rows: [0, 1.0 / 3, 2.0 / 3, 1], slots: [
        .init(id: 1, c0: 0, c1: 2, r0: 0, r1: 1, chat: "a"),
        .init(id: 2, c0: 2, c1: 3, r0: 0, r1: 2, chat: "b"),
        .init(id: 3, c0: 1, c1: 3, r0: 2, r1: 3, chat: "c"),
        .init(id: 4, c0: 0, c1: 1, r0: 1, r1: 3, chat: "d"),
        .init(id: 5, c0: 1, c1: 2, r0: 1, r1: 2, chat: "e"),
    ])
}

@Test func `a split adds a line through the slot, and the slots it crosses keep their places`() throws {
    let (across, new) = try split(.single("ana"), 1, .across, "rui")
    #expect(across.cols == [0, 0.5, 1] && across.slots.count == 2 && across.holds)
    #expect(across.slot(new)?.chat == "rui" && across.reading.map(\.chat) == ["ana", "rui"])
    let (down, low) = try split(across, 1, .down, "mum")
    #expect(down.rows == [0, 0.5, 1] && down.holds)
    #expect(span(down.slot(new)) == [1, 2, 0, 2], "rui spans both rows now")
    let before = across.frames(in: square)[new]
    #expect(down.frames(in: square)[new] == before, "and keeps its frame")
    #expect(down.neighbour(of: low, .up) == 1 && down.neighbour(of: low, .right) == new)
}

@Test func `a slot spanning lines splits at the one nearest its middle, adding none`() throws {
    let (g, new) = try split(work(), 2, .across, "rui")
    #expect(g.cols == work().cols, "ana spanned 0.5 to 1, with 0.75 inside")
    #expect(span(g.slot(2)) == [1, 2, 0, 1] && span(g.slot(new)) == [2, 3, 0, 1])
    #expect(g.holds)
}

@Test func `a split is refused under 240 points and past 12 panes`() throws {
    let narrow = PaneGrid.single("ana")
    #expect(throws: PaneGrid.Refusal.room) { try narrow.splitting(1, .across, chat: nil, size: CGSize(width: 470, height: 800)).get() }
    var g = PaneGrid.single("0")
    for i in 1 ..< PaneGrid.most {
        let id = g.reading.last!.id
        g = try g.splitting(id, i % 2 == 0 ? .across : .down, chat: "\(i)", size: CGSize(width: 1_000_000, height: 1_000_000)).get().0
    }
    #expect(g.slots.count == PaneGrid.most && g.holds)
    #expect(throws: PaneGrid.Refusal.most) { try g.splitting(1, .across, chat: nil, size: CGSize(width: 1_000_000, height: 1_000_000)).get() }
    #expect(PaneGrid.Refusal.room.said == "no room to split this pane; close one, or a bigger window")
}

@Test func `closing gives the space to the neighbour sharing its whole edge, and drops lines no one uses`() throws {
    let (two, rui) = try split(.single("ana"), 1, .across, "rui")
    let (closed, grew) = two.closing(rui)
    #expect(grew == 1 && closed == .single("ana"), "back to one pane, the line gone")
    let (g, took) = work().closing(3)
    #expect(took == 4, "the empty slot beside Lisbon shares its whole edge")
    #expect(span(g.slot(4)) == [1, 2, 1, 2] && g.cols == [0, 0.5, 1] && g.holds, "the 0.75 line went with it")
    let (left, before) = work().closing(4)
    #expect(before == 3, "the one before it in reading order first")
    #expect(left.holds)
}

@Test func `a slot no neighbour can take stays, empty`() {
    let (g, grew) = pinwheel().closing(5)
    #expect(grew == nil && g.slot(5)?.chat == nil && g.slots.count == 5 && g.holds)
    #expect(pinwheel().holds)
    let (once, _) = PaneGrid.single("ana").closing(1)
    #expect(once == .single("ana"), "the last pane stays")
}

@Test func `a chat shows in one slot at most, and swapping exchanges chats only`() {
    let g = work().with(chat: "ana", in: 4)
    #expect(g.slot(4)?.chat == "ana" && g.slot(2)?.chat == nil)
    #expect(g.slot(showing: "ana")?.id == 4)
    let swapped = work().swapping(1, 3)
    #expect(swapped.slot(1)?.chat == "lisbon" && swapped.slot(3)?.chat == "team")
    #expect(swapped.frames(in: square) == work().frames(in: square))
}

@Test func `neighbours are found by direction, from the slot's middle`() {
    let g = work()
    #expect(g.neighbour(of: 1, .right) == 3, "Lisbon's middle is nearer the team's than Ana's")
    #expect(g.neighbour(of: 3, .left) == 1 && g.neighbour(of: 3, .up) == 2 && g.neighbour(of: 3, .right) == 4)
    #expect(g.neighbour(of: 1, .left) == nil && g.neighbour(of: 4, .down) == nil)
}

@Test func `a line moves every slot along it, and stops at 240 points`() throws {
    let g = work()
    let moved = g.moving(.init(axis: .across, index: 1), to: 0.4, size: room)
    #expect(moved.cols[1] == 0.4 && moved.holds)
    let far = g.moving(.init(axis: .across, index: 1), to: 0.9, size: room)
    #expect(abs(far.cols[1] - (0.75 - 240.0 / 1200)) < 1e-9, "Lisbon keeps 240 points")
    let low = g.moving(.init(axis: .across, index: 1), to: 0.01, size: room)
    #expect(abs(low.cols[1] - 0.2) < 1e-9, "the team keeps 240 points")
    #expect(g.lines(in: square).count == 3, "two column lines' segments and one row line's")
}

@Test func `a tree of splits reads as the same panes in the same places`() {
    let tree = PaneLayout.split(.across, 0.6, .pane("ana"), .split(.down, 0.5, .pane("rui"), .pane("mum")))
    let g = PaneGrid.from(tree)
    #expect(g.holds && g.slots.count == 3)
    let frames = tree.frames(in: square)
    for slot in g.slots {
        #expect(g.frames(in: square)[slot.id] == frames[slot.chat!])
    }
}

@Test func `a tree read as a grid has one line where its panes meet, however the arithmetic rounds`() {
    let nested = PaneLayout.split(.across, 0.21, .split(.across, 0.28, .pane("a"), .pane("b")), .pane("c"))
    #expect(PaneGrid.from(nested) == PaneGrid(cols: [0, 0.0588, 0.21, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "a"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "b"),
        .init(id: 3, c0: 2, c1: 3, r0: 0, r1: 1, chat: "c"),
    ]), "b's right edge is the line its parent split at")
    let edge = PaneLayout.split(.across, 0.29, .pane("a"), .split(.across, 0.2, .pane("b"), .pane("c")))
    #expect(PaneGrid.from(edge) == PaneGrid(cols: [0, 0.29, 0.432, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "a"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "b"),
        .init(id: 3, c0: 2, c1: 3, r0: 0, r1: 1, chat: "c"),
    ]), "and c's is the grid's")
    let stacked = PaneLayout.split(.down, 0.5, .split(.across, 0.2, .pane("a"), .split(.across, 0.5, .pane("b"), .pane("c"))),
                                   .split(.across, 0.6, .pane("d"), .pane("e")))
    #expect(PaneGrid.from(stacked) == PaneGrid(cols: [0, 0.2, 0.6, 1], rows: [0, 0.5, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "a"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "b"),
        .init(id: 3, c0: 2, c1: 3, r0: 0, r1: 1, chat: "c"),
        .init(id: 4, c0: 0, c1: 2, r0: 1, r1: 2, chat: "d"),
        .init(id: 5, c0: 2, c1: 3, r0: 1, r1: 2, chat: "e"),
    ]), "a fifth and half the rest is the 0.6 the row below splits at, one line")
    #expect([nested, edge, stacked].allSatisfy { PaneGrid.from($0).holds })
    let shares = stride(from: 200, through: 800, by: 10).map { Double($0) / 1000 }
    var trees: [PaneLayout] = []
    for (outer, inner) in [(PaneLayout.Axis.across, PaneLayout.Axis.across), (.across, .down), (.down, .across), (.down, .down)] {
        for s in shares {
            for t in shares {
                trees.append(.split(outer, s, .pane("a"), .split(inner, t, .pane("b"), .pane("c"))))
                trees.append(.split(outer, s, .split(inner, t, .pane("a"), .pane("b")), .pane("c")))
            }
        }
    }
    let wrong = trees.filter { !PaneGrid.from($0).holds }
    #expect(wrong.isEmpty, "\(wrong.count) of the terminal's \(trees.count) trees of three panes don't hold")
}

@Test func `the editor's cells: lines snap to them, merging takes in what it cuts, splitting back is bounded`() throws {
    let cells = work().onCells(columns: 6, rows: 4)
    #expect(cells.cols.count == 7 && cells.rows.count == 5)
    #expect(span(cells.slot(2)) == [3, 6, 0, 2] && span(cells.slot(3)) == [3, 5, 2, 4], "0.75 snaps to the fifth of six")
    #expect(cells.coversAll && !cells.holds, "every cell is covered, though not every line is used")
    let merged = cells.merging(PaneGrid.Cells(c0: 3, r0: 2, c1: 5, r1: 2))
    #expect(merged.slots.count == 3 && merged.coversAll, "Lisbon and the empty slot become one")
    #expect(merged.slots.contains { $0.chat == "lisbon" && $0.c1 == 6 })
    let apart = try PaneGrid.single("ana").onCells(columns: 2, rows: 2).intoCells(1).get()
    #expect(apart.slots.count == 4 && apart.slot(1)?.chat == "ana")
    #expect(throws: PaneGrid.Refusal.most) { try PaneGrid.single("ana").onCells(columns: 6, rows: 4).intoCells(1).get() }
    #expect(cells.pruned().cols == [0, 0.5, 5.0 / 6, 1] && cells.pruned().holds, "on its own lines again once saved")
}

@Test func `a layout is kept as JSON and read back the same`() throws {
    let kept = try JSONDecoder().decode(PaneGrid.self, from: JSONEncoder().encode(work()))
    #expect(kept == work() && kept.holds)
}

@Test func `a chat dropped near an edge splits that way, and anywhere else takes the pane`() {
    let r = CGRect(x: 100, y: 100, width: 800, height: 400)
    #expect(GridView.zone(CGPoint(x: 500, y: 300), in: r) == .middle)
    #expect(GridView.zone(CGPoint(x: 120, y: 300), in: r) == .left)
    #expect(GridView.zone(CGPoint(x: 890, y: 300), in: r) == .right)
    #expect(GridView.zone(CGPoint(x: 500, y: 110), in: r) == .top)
    #expect(GridView.zone(CGPoint(x: 500, y: 495), in: r) == .bottom)
    #expect(GridView.zone(CGPoint(x: 105, y: 102), in: r) == .top, "the nearer edge wins in a corner")
    #expect(GridView.zone(CGPoint(x: 50, y: 300), in: r) == nil)
    #expect(GridView.zone(CGPoint(x: 290, y: 300), in: r) == .middle, "the band is at most 120 points deep")
}
