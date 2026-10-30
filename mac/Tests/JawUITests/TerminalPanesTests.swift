import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The terminal's panes read as a grid: its splits as the grid's lines, its panes as slots numbered
/// in the tree's order, its pane with the keys as the slot with the focus, and what can't open said;
/// and a slot whose chat won't have a pane saying why.

let protocolFixtures = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .appending(path: "protocol/fixtures")

/// A fixture's answer, as `{"id": …, "result": …}`.
struct Answered<R: Decodable>: Decodable {
    let result: R
}

@Test func `the fixture's two panes open side by side, the keys on the second, and the pane jawd left out is said`() throws {
    let data = try Data(contentsOf: protocolFixtures.appending(path: "layout.get.result.json"))
    let kept = try JSONDecoder().decode(Answered<KeptLayout>.self, from: data).result
    let terminal = try TerminalPanes.read(kept).get()
    #expect(terminal.grid == PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "351912345678@s.whatsapp.net"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "120363012345678901@g.us"),
    ]))
    #expect(terminal.focus == 2, "the tree counts its panes from 0, the grid its slots from 1")
    #expect(terminal.said == "the terminal's 2 panes, and 1 more that jawd left out")
}

@Test func `a split inside a split opens as one pane beside two, each where the tree draws it`() throws {
    let tree = SplitTree.split(.across, first: 500, .chat("ana"), .split(.down, first: 600, .chat("climbing"), .chat("rui")))
    let terminal = try TerminalPanes.read(KeptLayout(interface: "jaw", v: 1, layout: SplitLayout(focused: 2, tree: tree))).get()
    #expect(terminal.grid.cols == [0, 0.5, 1] && terminal.grid.rows == [0, 0.6, 1] && terminal.grid.holds)
    #expect(terminal.grid.reading.map(\.chat) == ["ana", "climbing", "rui"])
    #expect(span(terminal.grid.slot(1)) == [0, 1, 0, 2], "ana takes both rows")
    #expect(terminal.focus == 3 && terminal.grid.slot(3)?.chat == "rui")
    #expect(terminal.said == "the terminal's 3 panes")
    let past = try TerminalPanes.read(KeptLayout(interface: "jaw", v: 1, layout: SplitLayout(focused: 3, tree: tree))).get()
    #expect(past.focus == 3, "keys past the last pane go to the last slot")
    let before = try TerminalPanes.read(KeptLayout(interface: "jaw", v: 1, layout: SplitLayout(focused: -1, tree: tree))).get()
    #expect(before.focus == 1, "and keys before the first to the first")
}

@Test func `what can't open says why: nothing kept, every pane's chat gone, more panes than the grid holds, or no grid`() {
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 0)) == .failure(.nothing))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, dropped: ["351911111111@s.whatsapp.net", "351922222222@s.whatsapp.net"]))
        == .failure(.gone(2)))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5)) == .failure(.unreadable), "kept in a format this Jaw doesn't read")
    var many = SplitTree.chat("0")
    for i in 1 ... PaneGrid.most {
        many = .split(.across, first: 500, many, .chat("\(i)"))
    }
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: many))) == .failure(.most(13)))
    let twice = SplitTree.split(.down, first: 500, .chat("ana"), .chat("ana"))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: twice))) == .failure(.unreadable))
    let none = SplitTree.split(.across, first: 0, .chat("ana"), .chat("rui"))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: none))) == .failure(.unreadable),
        "jawd sends shares of 1 to 999 thousandths, and one of none or all leaves a slot no width")
    let all = SplitTree.split(.across, first: 1000, .chat("ana"), .chat("rui"))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: all))) == .failure(.unreadable))
    let less = SplitTree.split(.across, first: -1, .chat("ana"), .chat("rui"))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: less))) == .failure(.unreadable),
        "nor one past either end")
    let more = SplitTree.split(.across, first: 1001, .chat("ana"), .chat("rui"))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: more))) == .failure(.unreadable))
    #expect(TerminalPanes.Refusal.nothing.said == "the terminal hasn't kept any panes")
    #expect(TerminalPanes.Refusal.gone(1).said == "the terminal's pane showed a chat that's gone")
    #expect(TerminalPanes.Refusal.gone(2).said == "the terminal's 2 panes showed chats that are gone")
    #expect(TerminalPanes.Refusal.most(13).said == "the terminal has 13 panes, and Jaw shows 12 at most")
    #expect(TerminalPanes.Refusal.unreadable.said == "the terminal's panes don't make a layout this Jaw can show")
}

@MainActor
@Test func `a slot whose chat won't have a pane says why, that it's gone or jawd's words, and one waiting says how to give it one`() throws {
    let (two, second) = try split(.single("ana"), 1, .across, "old")
    let (grid, third) = try split(two, second, .down, "busy")
    let refused = "a connection can watch at most 32 windows at once, so one has to be unwatched first"
    let unopened = ["old": GridView.gone, "busy": refused]
    let roles = Catalog.jaw.roles(dark: true)
    let view = GridView(frame: NSRect(x: 0, y: 0, width: 1000, height: 600))
    view.show(grid, views: [:], unopened: unopened, focused: 1, roles: roles)
    #expect(view.frames[second]?.empty.prompt.stringValue == "this chat isn't here any more")
    #expect(view.frames[second]?.empty.accessibilityLabel() == "An empty pane, this chat isn't here any more: ⌘K or drag a chat here")
    #expect(view.frames[third]?.empty.prompt.stringValue == refused, "jawd's words for the window it refused")
    #expect(view.frames[1]?.empty.prompt.stringValue == "⌘K or drag a chat here", "ana's pane only waits for its row")
    view.show(grid.with(chat: "mum", in: second), views: [:], unopened: unopened, focused: second, roles: roles)
    #expect(view.frames[second]?.empty.prompt.stringValue == "⌘K or drag a chat here", "another chat put in its place says nothing of the one before")
}

@MainActor
@Test func `the words for a slot whose chat won't have a pane travel in the window's panes, and are drawn again when they change`() {
    let grid = PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "ana"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "old"),
    ])
    let pane = NSView()
    let c = content(width: 1180)
    c.show(WindowContent.Panes(grid: grid, views: [1: pane], unopened: ["old": GridView.gone], focused: 1, box: nil, look: PaneLook()),
           roles: Catalog.jaw.dark)
    #expect(c.detail.panes.frames[2]?.empty.prompt.stringValue == "this chat isn't here any more")
    c.show(WindowContent.Panes(grid: grid, views: [1: pane], focused: 1, box: nil, look: PaneLook()), roles: Catalog.jaw.dark)
    #expect(c.detail.panes.frames[2]?.empty.prompt.stringValue == "⌘K or drag a chat here", "the same panes with no words for it")
}
