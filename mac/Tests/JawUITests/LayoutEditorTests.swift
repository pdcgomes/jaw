import SwiftUI
@testable import JawUI
import Testing

/// The layout editor's model: the cursor and selection stay on the grid, merging and splitting
/// work as the grid's rules do, presets keep the chats, the grid can't shrink under its panes, and
/// Save says why it can't go.

@MainActor
func editor(_ mode: LayoutEditorModel.Mode = .edit, grid: PaneGrid? = work(), name: String? = "Work") -> LayoutEditorModel {
    LayoutEditorModel(mode: mode, grid: grid, name: name, taken: ["Work", "Family"]) { $0.capitalized }
}

@MainActor
@Test func `the cursor stays on the grid and ⇧ grows a selection from where it was`() {
    let e = editor()
    #expect(e.columns == 6 && e.rows == 4)
    e.move(-1, -1, extend: false)
    #expect(e.cursor == .init(0, 0))
    e.move(10, 10, extend: false)
    #expect(e.cursor == .init(5, 3))
    e.move(-2, 0, extend: true)
    #expect(e.selection == PaneGrid.Cells(c0: 3, r0: 3, c1: 5, r1: 3))
    e.move(0, -1, extend: false)
    #expect(e.anchor == nil && e.selection == PaneGrid.Cells(c0: 3, r0: 2, c1: 3, r1: 2))
}

@MainActor
@Test func `M merges what's chosen, S splits a pane back, and a merge of one pane says how`() {
    let e = editor()
    e.cursor = .init(3, 2)
    e.anchor = .init(5, 2)
    e.merge()
    #expect(e.grid.slots.count == 3 && e.said == nil)
    e.cursor = .init(0, 0)
    e.merge()
    #expect(e.said?.hasPrefix("choose cells of more than one pane") == true)
    _ = e.key(.init("s"), characters: "s", modifiers: [])
    #expect(e.said == PaneGrid.Refusal.most.said, "the team's pane is 3 by 4 cells, too many")
    e.setColumns(1)
    #expect(e.columns == 2, "two columns are what the panes need")
}

@MainActor
@Test func `a preset keeps the chats on screen, in reading order`() {
    let e = editor()
    _ = e.key(.init("4"), characters: "4", modifiers: [])
    #expect(e.result.slots.count == 4 && e.result.chats == ["team", "ana", "lisbon"])
    #expect(e.label(e.result.reading[0]) == "Team" && e.label(e.result.reading[3]) == "empty")
}

@MainActor
@Test func `the grid's size moves with ⌥ and the arrows, within its bounds`() {
    let e = editor()
    _ = e.key(.rightArrow, characters: "", modifiers: .option)
    _ = e.key(.downArrow, characters: "", modifiers: .option)
    #expect(e.columns == 7 && e.rows == 5 && e.result.chats == work().chats)
    for _ in 0 ..< 20 {
        e.setColumns(e.columns + 1)
    }
    #expect(e.columns == PaneGrid.columnsMost)
}

@MainActor
@Test func `Save says why it can't go`() {
    let save = editor(.saveAs)
    #expect(save.notReady == "a layout needs a name" && save.name.isEmpty)
    save.name = "family"
    #expect(save.notReady == "there's a layout by that name already")
    save.name = "Home"
    #expect(save.notReady == nil)
    let edit = editor()
    #expect(edit.notReady == nil, "its own name is fine")
    let fresh = editor(.saveAs, grid: nil, name: nil)
    #expect(fresh.result.slots.count == 2, "a new layout starts as two panes")
}
