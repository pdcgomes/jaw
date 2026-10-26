import CoreGraphics
import Foundation
@testable import JawUI
import Testing

/// Named layouts: chats saved at once, a changed shape kept as an edit until saved or reverted,
/// names unique, and the unnamed layout before any is saved.

func book(_ grid: PaneGrid, named name: String? = "Work") throws -> LayoutBook {
    var b = LayoutBook()
    b.update(grid)
    if let name { try b.saveAs(name) }
    return b
}

@Test func `before any layout is saved, the panes are the unnamed layout`() {
    var b = LayoutBook()
    b.update(.single("ana"))
    #expect(b.current == nil && b.grid == .single("ana") && !b.isEdited)
}

@Test func `a changed shape is an edit, and going back to the saved shape clears it`() throws {
    var b = try book(work())
    let (split, _) = try work().splitting(1, .down, chat: "rui", size: CGSize(width: 2000, height: 2000)).get()
    b.update(split)
    #expect(b.isEdited && b.grid == split && b.layouts[0].grid == work())
    b.update(work())
    #expect(!b.isEdited, "the same shape as saved")
}

@Test func `a chat put in a pane is saved at once, edited or not`() throws {
    var b = try book(work())
    b.update(work().with(chat: "mum", in: 4))
    #expect(!b.isEdited && b.layouts[0].grid.slot(4)?.chat == "mum")
    let (split, new) = try work().splitting(1, .down, chat: "rui", size: CGSize(width: 2000, height: 2000)).get()
    b.update(split.with(chat: "sofia", in: 3))
    #expect(b.isEdited && b.layouts[0].grid.slot(3)?.chat == "sofia", "the saved layout's pane takes it too")
    #expect(b.layouts[0].grid.slot(new) == nil, "the new pane is the edit's alone")
}

@Test func `save keeps the edit, revert drops it`() throws {
    var b = try book(work())
    let (closed, _) = work().closing(3)
    b.update(closed)
    b.revert()
    #expect(!b.isEdited && b.grid == work())
    b.update(closed)
    b.save()
    #expect(!b.isEdited && b.grid == closed && b.layouts[0].grid == closed)
}

@Test func `names are unique, kept trimmed, and there are 30 at most`() throws {
    var b = try book(work())
    #expect(throws: LayoutBook.Problem.taken) { try b.saveAs(" work ") }
    #expect(throws: LayoutBook.Problem.noName) { try b.saveAs("  ") }
    try b.saveAs(" Family ")
    #expect(b.names == ["Work", "Family"] && b.current == "Family")
    try b.rename("Family", to: "Home")
    #expect(b.current == "Home")
    #expect(throws: LayoutBook.Problem.taken) { try b.rename("Home", to: "Work") }
    for i in 3 ... LayoutBook.most {
        try b.saveAs("L\(i)")
    }
    #expect(throws: LayoutBook.Problem.most) { try b.saveAs("one more") }
}

@Test func `switching keeps each layout's edit, and deleting the one on screen keeps its panes`() throws {
    var b = try book(work())
    try b.saveAs("Family")
    b.update(.single("mum"))
    #expect(b.isEdited)
    b.switchTo("Work")
    #expect(b.grid == work() && !b.isEdited)
    b.switchTo("Family")
    #expect(b.grid == .single("mum") && b.isEdited, "Family's edit waited")
    b.delete("Family")
    #expect(b.current == nil && b.grid == .single("mum") && b.names == ["Work"])
}

@Test func `the book is kept as version 1 JSON, and a newer version isn't read`() throws {
    var b = try book(work())
    b.focus = 3
    let data = try JSONEncoder().encode(b)
    let read = try JSONDecoder().decode(LayoutBook.self, from: data)
    #expect(read == b && read.focus == 3)
    let newer = try #require(String(data: data, encoding: .utf8)?.replacingOccurrences(of: "\"v\":1", with: "\"v\":2"))
    #expect(throws: LayoutBook.Problem.version) { try JSONDecoder().decode(LayoutBook.self, from: Data(newer.utf8)) }
}
