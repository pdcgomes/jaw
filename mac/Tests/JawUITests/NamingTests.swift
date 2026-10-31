import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// New Folder and Rename open a field in place in the tree: Return, or a click elsewhere, asks for the
/// name typed, once while jawd answers, unless it's empty or unchanged; jawd's refusal is said under
/// the field, which stays open, and doesn't take back keys that have gone elsewhere; Esc drops it; a
/// push from jawd leaves it as it is, and closes it once the folder it names, or the one it's made in,
/// is gone; once jawd has made the folder its row is there, picked when it was made from the sidebar.

/// A sidebar with the sample tree in an offscreen window, so its field can take the keys.
@MainActor
func namingSidebar(_ host: SideHost) -> (SidebarController, NSWindow) {
    let s = SidebarController()
    s.host = host
    s.roles = Catalog.jaw.roles(dark: true)
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 220, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = s.view
    s.apply(Samples.folderUpdate)
    s.view.layoutSubtreeIfNeeded()
    return (s, window)
}

/// The field's editor, which has the keys while a name is typed.
@MainActor
func fieldEditor(_ window: NSWindow) throws -> NSTextView {
    try #require(window.firstResponder as? NSTextView)
}

/// A key pressed with nothing held.
@MainActor
func plainKey(_ window: NSWindow, _ characters: String, _ code: UInt16) throws -> NSEvent {
    try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                  context: nil, characters: characters, charactersIgnoringModifiers: characters, isARepeat: false,
                                  keyCode: code))
}

@MainActor
@Test func `New Folder at the top opens a field at the end of Folders, and Return asks for what's typed, once while jawd answers`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: nil)
    #expect(s.said[10] == "[]")
    try fieldEditor(window).insertText("Parents", replacementRange: NSRange(location: NSNotFound, length: 0))
    #expect(s.said[10] == "[Parents]")
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    #expect(host.named.map(\.0) == ["Parents"], "asked once while jawd answers")
    #expect(host.named.first?.1.parent == nil && host.named.first?.1.renaming == nil)
    s.named(FolderRow(id: "folder:7", v: 70, name: "Parents", position: 4))
    #expect(s.naming == nil && s.said[10] == "Parents")
    #expect(s.picked == "folder:7" && host.picked == ["folder:7"], "made from the sidebar, it's picked, as Notes picks a new folder")
    #expect(s.roster.tree == s.tree, "the roster names it too, as its people are watched before jawd's push brings its row")
}

@MainActor
@Test func `a folder made from a chat's Add to Folder isn't picked, so the list keeps what it showed`() {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: nil, filing: Filed(Samples.direct), text: "Parents")
    s.named(FolderRow(id: "folder:7", v: 70, name: "Parents", position: 4))
    #expect(s.naming == nil && s.picked == "all" && host.picked.isEmpty)
}

@MainActor
@Test func `New Folder in Family opens Family and puts the field after Cousins`() {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { withExtendedLifetime(host) { window.close() } }
    s.beginNaming(parent: "folder:1")
    #expect(Array(s.said[6 ... 8]) == ["Family ⌘1 @ 3 ▾", "  Cousins 1", "  []"])
}

@MainActor
@Test func `a new folder at the top opens Folders, closed by hand, to show its field`() {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { withExtendedLifetime(host) { window.close() } }
    s.outline.collapseItem(s.item(SidebarController.foldersHeading))
    #expect(s.said[5] == "Folders:" && s.said[6] == "Smart Folders:")
    s.beginNaming(parent: nil)
    #expect(s.said[10] == "[]" && window.firstResponder is NSTextView)
}

@MainActor
@Test func `a name jawd turns down is said under the field, which keeps the keys and what's typed, and Return asks again`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: "folder:1")
    try fieldEditor(window).insertText("Cousins", replacementRange: NSRange(location: NSNotFound, length: 0))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    s.notNamed("a folder there is called Cousins already")
    #expect(s.said[8] == "  [Cousins] · a folder there is called Cousins already")
    #expect(window.firstResponder is NSTextView, "the field still has the keys")
    #expect(s.outline.rect(ofRow: 8).height == NamingCell.refusedHeight, "room under the field for two lines of jawd's words")
    s.view.layoutSubtreeIfNeeded()
    let cell = try #require(s.outline.view(atColumn: 0, row: 8, makeIfNecessary: false) as? NamingCell)
    #expect(cell.bounds.contains(cell.convert(cell.refused.bounds, from: cell.refused)), "jawd's words whole in the row")
    try fieldEditor(window).insertText(" 2", replacementRange: NSRange(location: NSNotFound, length: 0))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    #expect(host.named.map(\.0) == ["Cousins", "Cousins 2"])
}

@MainActor
@Test func `Esc drops the field, and a click elsewhere keeps the name typed, as Finder does, unless it's empty or unchanged`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: nil)
    try fieldEditor(window).insertText("Temp", replacementRange: NSRange(location: NSNotFound, length: 0))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.cancelOperation(_:)))
    #expect(s.naming == nil && !s.said.contains("[Temp]") && host.named.isEmpty)
    s.beginNaming(parent: nil)
    window.makeFirstResponder(s.outline)
    #expect(s.naming == nil && host.named.isEmpty, "an empty name given up is dropped")
    s.beginRenaming("folder:3")
    window.makeFirstResponder(s.outline)
    #expect(s.naming == nil && host.named.isEmpty, "and an unchanged one")
    s.beginNaming(parent: nil)
    try fieldEditor(window).insertText("Parents", replacementRange: NSRange(location: NSNotFound, length: 0))
    window.makeFirstResponder(s.outline)
    #expect(host.named.map(\.0) == ["Parents"], "a click elsewhere asks for what's typed")
    #expect(s.naming?.asked == true && s.said.contains("[Parents]"), "the row waits for jawd's answer")
}

@MainActor
@Test func `a name a click elsewhere kept that jawd turns down is said without taking the keys back, and Esc or another click away drops it`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: "folder:1")
    try fieldEditor(window).insertText("Cousins", replacementRange: NSRange(location: NSNotFound, length: 0))
    window.makeFirstResponder(s.outline)
    s.notNamed("a folder there is called Cousins already")
    #expect(window.firstResponder === s.outline, "the keys stay where they went")
    #expect(s.said[8] == "  [Cousins] · a folder there is called Cousins already")
    s.outline.keyDown(with: try plainKey(window, "\u{1b}", 53))
    #expect(s.naming == nil && !s.said.contains { $0.contains("[Cousins]") }, "Esc in the tree drops it")
    s.beginNaming(parent: "folder:1")
    try fieldEditor(window).insertText("Cousins", replacementRange: NSRange(location: NSNotFound, length: 0))
    window.makeFirstResponder(s.outline)
    s.notNamed("a folder there is called Cousins already")
    s.focusField()
    window.makeFirstResponder(s.outline)
    #expect(s.naming == nil && host.named.count == 2, "given up again once jawd has turned it down: dropped, not asked again")
}

@MainActor
@Test func `a push while a name is typed leaves the field with the keys, what's typed and the caret where it was`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginRenaming("folder:3")
    try fieldEditor(window).insertText("Climbing club", replacementRange: NSRange(location: NSNotFound, length: 0))
    s.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:3", v: 50, name: "Climbing", position: 1, unread: 4, count: 8, favourite: 3)]))
    #expect(window.firstResponder is NSTextView && s.said[7] == "[Climbing club]", "the renamed folder's counts changed")
    s.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:8", v: 62, name: "Neighbours", position: 4)]))
    #expect(try fieldEditor(window).string == "Climbing club" && s.said[7] == "[Climbing club]", "another interface made a folder")
    #expect(try fieldEditor(window).selectedRange() == NSRange(location: 13, length: 0), "the caret where it was")
    #expect(s.naming?.renaming == "folder:3" && host.named.isEmpty, "nothing asked of jawd")
}

@MainActor
@Test func `the field closes when another interface deletes the folder it renames, and Return on the tree renames again`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginRenaming("folder:3")
    try fieldEditor(window).insertText("Climbing club", replacementRange: NSRange(location: NSNotFound, length: 0))
    s.apply(Update(seq: 301, remove: ["folder:3"]))
    #expect(s.naming == nil && !s.said.contains { $0.hasPrefix("[") }, "Climbing went, and its field with it")
    s.select("folder:4")
    window.makeFirstResponder(s.outline)
    s.outline.keyDown(with: try plainKey(window, "\r", 36))
    #expect(s.naming?.renaming == "folder:4", "Return opens Friends' name, where it did nothing until Esc")
    #expect(host.named.isEmpty, "nothing asked of jawd for the folder that went")
}

@MainActor
@Test func `the field closes when another interface deletes the folder a new one is being made in, and not for another folder`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: "folder:1")
    try fieldEditor(window).insertText("Parents", replacementRange: NSRange(location: NSNotFound, length: 0))
    s.apply(Update(seq: 301, remove: ["folder:5", "folder:6"]))
    #expect(s.naming?.text == "Parents" && s.said.contains("  [Parents]"), "Work went, which the new folder isn't in")
    s.apply(Update(seq: 302, remove: ["folder:1"], upsert: [FolderRow(id: "folder:2", v: 63, name: "Cousins", position: 0, unread: 1)]))
    #expect(s.naming == nil && !s.said.contains { $0.contains("[") }, "Family went, so there's nowhere to make it")
    #expect(host.named.isEmpty)
}

@MainActor
@Test func `Return on a folder of yours opens its name to change, and on a built-in one does nothing`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    let ret = try plainKey(window, "\r", 36)
    s.select("folder:3")
    window.makeFirstResponder(s.outline)
    s.outline.keyDown(with: ret)
    #expect(s.naming?.renaming == "folder:3" && s.said[7] == "[Climbing]")
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    #expect(host.named.map(\.0) == ["Climbing"] && host.named.first?.1.renaming == "folder:3")
    s.named(nil)
    #expect(s.naming == nil && s.said[7] == "Climbing ⌘3")
    s.select("all")
    window.makeFirstResponder(s.outline)
    s.outline.keyDown(with: ret)
    #expect(s.naming == nil, "the built-in folders keep their names")
}
