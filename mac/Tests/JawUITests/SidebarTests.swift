import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The sidebar draws jawd's folder tree: the built-in folders, then Folders, yours nested, then Smart
/// Folders, each with its counts, an @ and its ⌘ digit; picking a row picks its folder.

@MainActor
final class SideHost: SidebarHost {
    var picked: [String] = []

    func sidebar(_: SidebarController, pick folder: String) {
        picked.append(folder)
    }

    var named: [(String, SidebarController.Naming)] = []

    func sidebar(_: SidebarController, name: String, as naming: SidebarController.Naming) {
        named.append((name, naming))
    }

    var menusFor: [String?] = []

    func sidebar(_: SidebarController, menuFor folder: String?) -> [NSMenuItem] {
        menusFor.append(folder)
        return [NSMenuItem(title: folder ?? "New Folder…", action: nil, keyEquivalent: "")]
    }

    var did: [(FolderAct, String)] = []

    func sidebar(_: SidebarController, do act: FolderAct, name: String) {
        did.append((act, name))
    }
}

@MainActor
func sidebar(_ host: SideHost? = nil) -> SidebarController {
    let s = SidebarController()
    s.host = host
    s.roles = Catalog.jaw.roles(dark: true)
    _ = s.view
    s.view.frame = NSRect(x: 0, y: 0, width: 220, height: 600)
    return s
}

@MainActor
@Test func `before jawd serves folders, the sidebar has All and Archived`() {
    let s = sidebar()
    #expect(s.said == ["All", "Archived"] && s.picked == "all")
}

@MainActor
@Test func `the tree comes under its headings, yours nested, with counts, an @ and the ⌘ digits from the slots`() {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    #expect(s.said == ["All @ 12", "Unread @ 12", "Groups @ 5", "Communities 1", "Archived", "Folders:", "Family ⌘1 @ 3 ▸", "Climbing ⌘3",
                       "Friends 2", "Work ▸", "Smart Folders:", "Waiting on me 2", "Mentions me @ 1", "Muted, unread"])
    s.expand("folder:1")
    #expect(Array(s.said[6 ... 7]) == ["Family ⌘1 @ 3 ▾", "  Cousins 1"])
}

@MainActor
@Test func `picking a row picks its folder, and the app's own pick selects it without asking again`() {
    let host = SideHost()
    let s = sidebar(host)
    s.apply(Samples.folderUpdate)
    #expect(s.pick(row: 7))
    #expect(host.picked == ["folder:3"] && s.picked == "folder:3")
    #expect(!s.pick(row: 5), "a heading isn't a folder")
    s.select("folder:2")
    #expect(s.picked == "folder:2" && host.picked == ["folder:3"], "not asked for again")
    #expect(s.said[6] == "Family ⌘1 @ 3 ▾", "the folder it's in opened to show it")
}

@MainActor
@Test func `the picked folder is selected whenever its row is in sight`() {
    let host = SideHost()
    let s = sidebar(host)
    s.apply(Samples.folderUpdate)
    let selected = { (s.outline.item(atRow: s.outline.selectedRow) as? SidebarController.Item)?.id }
    s.select("folder:2")
    #expect(selected() == "folder:2", "the app's pick selects its row")
    s.outline.deselectAll(nil)
    #expect(selected() == "folder:2" && host.picked.isEmpty, "a click on nothing leaves it selected")
    s.outline.collapseItem(s.items["folder:1"])
    #expect(selected() == nil && s.picked == "folder:2" && host.picked.isEmpty, "closing Family picks nothing")
    s.outline.expandItem(s.items["folder:1"])
    #expect(selected() == "folder:2", "opened again, Cousins is selected again")
}

@MainActor
@Test func `closing the folder that holds the picked one with its triangle picks nothing, so the chat list stays where it is`() throws {
    let host = SideHost()
    let s = sidebar(host)
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.select("folder:2")
    s.view.layoutSubtreeIfNeeded()
    window.makeKeyAndOrderFront(nil)
    window.makeFirstResponder(s.outline)
    let family = try #require(s.outline.rowView(atRow: 6, makeIfNecessary: true))
    let triangle = try #require(family.subviews.first { $0.identifier == NSOutlineView.disclosureButtonIdentifier } as? NSButton)
    triangle.performClick(nil)
    #expect(s.said[6] == "Family ⌘1 @ 3 ▸", "closed")
    #expect(host.picked.isEmpty && s.picked == "folder:2" && s.selected == nil)
}

@MainActor
@Test func `a heading closed by hand stays closed, and a folder the app picks under a closed heading opens it`() {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    s.outline.collapseItem(s.items[SidebarController.smartHeading])
    s.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:8", v: 62, name: "Parents", position: 4)]))
    s.roles = Catalog.jaw.roles(dark: false)
    #expect(s.said.last == "Smart Folders:", "a push that lays the tree out again and a restyle leave it closed")
    s.outline.collapseItem(s.items[SidebarController.foldersHeading])
    s.select("folder:2")
    #expect(Array(s.said[5 ... 7]) == ["Folders:", "Family ⌘1 @ 3 ▾", "  Cousins 1"] && s.said.last == "Smart Folders:")
}

@MainActor
@Test func `what's left open and what's closed is kept from launch to launch under the key the app gives`() {
    let key = "jaw.tests.sidebar.\(UUID().uuidString)"
    defer { UserDefaults.standard.removeObject(forKey: key) }
    let before = sidebar()
    before.keepsOpen = key
    before.apply(Samples.folderUpdate)
    before.expand("folder:1")
    before.outline.collapseItem(before.items[SidebarController.smartHeading])
    let after = sidebar()
    after.keepsOpen = key
    after.apply(Samples.folderUpdate)
    #expect(Array(after.said[6 ... 7]) == ["Family ⌘1 @ 3 ▾", "  Cousins 1"], "Family left open")
    #expect(after.said.last == "Smart Folders:", "Smart Folders left closed")
}

@MainActor
@Test func `counts that change draw their rows again, and a folder that comes lays the tree out again, keeping what's open`() {
    let s = sidebar()
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.expand("folder:1")
    s.view.layoutSubtreeIfNeeded()
    // `said` reads the tree, so the drawn cell is what shows the row was drawn again.
    let cousins = { (s.outline.view(atColumn: 0, row: 7, makeIfNecessary: true) as? FolderCell)?.count.stringValue }
    #expect(cousins() == "1" && s.outline.rowView(atRow: 7, makeIfNecessary: false) != nil, "Cousins' row is on screen")
    s.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:2", v: 50, parent: "folder:1", name: "Cousins", position: 0, unread: 4)]))
    #expect(cousins() == "4", "the row on screen drawn again, not only the tree changed")
    s.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:8", v: 62, parent: "folder:1", name: "Parents", position: 1)]))
    #expect(Array(s.said[6 ... 8]) == ["Family ⌘1 @ 3 ▾", "  Cousins 4", "  Parents"])
}

@MainActor
@Test func `the digits, @s and counts end at the row's right, however long the name and however deep the folder`() {
    let s = sidebar()
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.expand("folder:1")
    s.view.layoutSubtreeIfNeeded()
    let ends = (0 ..< s.outline.numberOfRows).compactMap { row -> CGFloat? in
        guard let cell = s.outline.view(atColumn: 0, row: row, makeIfNecessary: true) as? FolderCell else { return nil }
        cell.layoutSubtreeIfNeeded()
        guard let last = [cell.count, cell.mention, cell.digit].first(where: { !$0.isHidden }) else { return nil }
        return s.outline.convert(last.bounds, from: last).maxX
    }
    #expect(ends.count == 10 && Set(ends).count == 1, "\(ends)")
}

@MainActor
@Test func `on each level every name starts at the same place, whatever its symbol, and your folders and the smart ones start where All does`() {
    let s = sidebar()
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.expand("folder:1")
    s.view.layoutSubtreeIfNeeded()
    var starts: [Int: Set<CGFloat>] = [:]
    for row in 0 ..< s.outline.numberOfRows {
        guard let cell = s.outline.view(atColumn: 0, row: row, makeIfNecessary: true) as? FolderCell else { continue }
        cell.layoutSubtreeIfNeeded()
        starts[s.outline.level(forRow: row), default: []].insert(s.outline.convert(cell.name.bounds, from: cell.name).minX)
    }
    #expect(starts.keys.sorted() == [0, 1, 2] && starts.values.allSatisfy { $0.count == 1 }, "\(starts)")
    #expect(starts[0] == starts[1], "under their headings, in line with All")
}

@MainActor
@Test func `on the selection of a tree with the keys a row is drawn in the selected text's colour, and in the theme's once the keys go`() throws {
    let s = sidebar()
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.select("folder:1")
    s.view.layoutSubtreeIfNeeded()
    let family = try #require(s.outline.view(atColumn: 0, row: 6, makeIfNecessary: true) as? FolderCell)
    // What AppKit gives a selected row's cell while its list has the keys in the key window.
    family.backgroundStyle = .emphasized
    #expect([family.name, family.digit, family.mention, family.count].allSatisfy { $0.textColor == .alternateSelectedControlTextColor })
    #expect(family.symbol.contentTintColor == .alternateSelectedControlTextColor)
    family.backgroundStyle = .normal
    let roles = Catalog.jaw.roles(dark: true)
    #expect([family.name, family.digit, family.count].allSatisfy { $0.textColor == roles.text.nsColor }, "the selection's quiet grey under it")
    #expect(family.mention.textColor == roles.mention.nsColor && family.symbol.contentTintColor == roles.text.nsColor)
    let climbing = try #require(s.outline.view(atColumn: 0, row: 7, makeIfNecessary: true) as? FolderCell)
    #expect(climbing.name.textColor == roles.text.nsColor && climbing.digit.textColor == roles.tertiary.nsColor)
    #expect(climbing.symbol.contentTintColor == roles.secondary.nsColor)
}

/// A colour as AppKit draws it in the dark or the light look, in sRGB.
@MainActor
func drawn(_ colour: NSColor?, dark: Bool) -> RGBA {
    var out = RGBA(0)
    NSAppearance(named: dark ? .darkAqua : .aqua)?.performAsCurrentDrawingAppearance {
        if let c = colour?.usingColorSpace(.sRGB) {
            out = RGBA(r: c.redComponent, g: c.greenComponent, b: c.blueComponent, a: c.alphaComponent)
        }
    }
    return out
}

@MainActor
@Test(arguments: [true, false])
func `the picked folder's row reads on the selection under it, with the keys and without, in either look`(dark: Bool) throws {
    let s = sidebar()
    s.roles = Catalog.jaw.roles(dark: dark)
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.select("folder:1")
    s.view.layoutSubtreeIfNeeded()
    let row = try #require(s.outline.rowView(atRow: 6, makeIfNecessary: true))
    let family = try #require(s.outline.view(atColumn: 0, row: 6, makeIfNecessary: true) as? FolderCell)
    #expect(row.isSelected)
    func reads(_ ink: NSColor?, on fill: NSColor, _ least: Double, _ what: String) {
        let ratio = drawn(ink, dark: dark).contrast(with: drawn(fill, dark: dark))
        #expect(ratio >= least, "\(what) reads at \(ratio):1 on \(fill)")
    }
    // Without the keys, as while you type: the theme's colours on AppKit's quiet grey.
    row.isEmphasized = false
    family.backgroundStyle = .normal
    for (what, label) in [("the name", family.name), ("the ⌘ digit", family.digit), ("the count", family.count)] {
        reads(label.textColor, on: .unemphasizedSelectedContentBackgroundColor, 4.5, what)
    }
    reads(family.mention.textColor, on: .unemphasizedSelectedContentBackgroundColor, 3, "the @")
    reads(family.symbol.contentTintColor, on: .unemphasizedSelectedContentBackgroundColor, 3, "the symbol")
    // With the keys: the selected text's colour on the accent.
    row.isEmphasized = true
    family.backgroundStyle = .emphasized
    for (what, label) in [("the name", family.name), ("the ⌘ digit", family.digit), ("the @", family.mention), ("the count", family.count)] {
        reads(label.textColor, on: .selectedContentBackgroundColor, 4.5, what)
    }
    reads(family.symbol.contentTintColor, on: .selectedContentBackgroundColor, 3, "the symbol")
}

@MainActor
@Test func `VoiceOver reads a folder's row once, from the row itself, with its unread total, mentions and favourite`() throws {
    let s = sidebar()
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.view.layoutSubtreeIfNeeded()
    let family = try #require(s.outline.view(atColumn: 0, row: 6, makeIfNecessary: true) as? FolderCell)
    #expect(family.isAccessibilityElement() && family.accessibilityLabel() == "Family, 3 unread, mentions you, favourite 1")
    let parts = NSAccessibility.unignoredChildren(from: family.accessibilityChildren() ?? [])
    #expect(parts.isEmpty, "nothing in it read again: \(parts)")
}

/// A source list of a row, a group row and a row, as AppKit lays one out with nothing saying how tall
/// its rows are but `rowHeight`.
@MainActor
final class PlainSourceList: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate {
    let items = ["a", "heading", "b"].map { NSString(string: $0) }

    func outlineView(_: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        item == nil ? items.count : 0
    }

    func outlineView(_: NSOutlineView, child index: Int, ofItem _: Any?) -> Any {
        items[index]
    }

    func outlineView(_: NSOutlineView, isItemExpandable _: Any) -> Bool {
        false
    }

    func outlineView(_: NSOutlineView, isGroupItem item: Any) -> Bool {
        (item as? NSString) == "heading"
    }

    func outlineView(_: NSOutlineView, viewFor _: NSTableColumn?, item _: Any) -> NSView? {
        NSTableCellView()
    }
}

@MainActor
@Test func `a heading is as tall as AppKit draws a source list's group row, and a folder's row as tall as its others`() {
    let plain = NSOutlineView()
    let column = NSTableColumn(identifier: .init("c"))
    plain.addTableColumn(column)
    plain.outlineTableColumn = column
    plain.style = .sourceList
    plain.rowHeight = 28
    let source = PlainSourceList()
    plain.dataSource = source
    plain.delegate = source
    plain.reloadData()
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    #expect(s.said[5] == "Folders:")
    #expect(s.outline.rect(ofRow: 5).height == plain.rect(ofRow: 1).height, "a heading")
    #expect(s.outline.rect(ofRow: 6).height == plain.rect(ofRow: 0).height, "a folder")
    withExtendedLifetime(source) {}
}

@MainActor
@Test func `a right click on a folder asks for its menu, and on a heading or the space under the rows for the sidebar's`() throws {
    let host = SideHost()
    let s = sidebar(host)
    s.apply(Samples.folderUpdate)
    let menu = try #require(s.outline.menu)
    #expect(menu.delegate === s, "filled for the row right-clicked as it opens")
    #expect(s.menuItems(forRow: 6).map(\.title) == ["folder:1"])
    #expect(s.menuItems(forRow: 5).map(\.title) == ["New Folder…"] && s.menuItems(forRow: -1).map(\.title) == ["New Folder…"])
    #expect(host.menusFor == ["folder:1", nil, nil])
}

@MainActor
@Test func `VoiceOver's Show Menu on a folder's row opens its menu under the row`() throws {
    let host = SideHost()
    let s = sidebar(host)
    s.apply(Samples.folderUpdate)
    let cell = try #require(s.outline.view(atColumn: 0, row: 6, makeIfNecessary: true) as? FolderCell)
    var popped: [(NSMenu, NSPoint)] = []
    cell.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    #expect(cell.accessibilityPerformShowMenu())
    #expect(popped.first?.0.items.map(\.title) == ["folder:1"] && popped.first?.1 == NSPoint(x: cell.bounds.minX, y: cell.bounds.minY))
}

@MainActor
@Test func `the tree has the keys only while its outline is the window's first responder`() {
    let s = sidebar()
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    #expect(!s.hasKeys)
    window.makeFirstResponder(s.outline)
    #expect(s.hasKeys)
}

@MainActor
@Test func `a drop lands onto a folder, between the rows in one or under Folders, and nowhere among the built-in rows`() {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    let onto = s.place(s.item("folder:1"), NSOutlineViewDropOnItemIndex)
    #expect(onto?.target == "folder:1" && onto?.at == nil)
    let between = s.place(s.item(SidebarController.foldersHeading), 2)
    #expect(between != nil && between?.target == nil && between?.at == 2)
    #expect(s.place(s.item(SidebarController.foldersHeading), NSOutlineViewDropOnItemIndex)?.at == 4, "onto Folders: last at the top")
    #expect(s.place(nil, 0) == nil && s.place(s.item(SidebarController.smartHeading), 0) == nil)
}

@MainActor
@Test func `a folder of yours drags as itself, a built-in one doesn't, and a drag says what it carries`() throws {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    let family = try #require(s.outlineView(s.outline, pasteboardWriterForItem: s.item("folder:1")) as? NSPasteboardItem)
    #expect(family.string(forType: .jawFolder) == "folder:1" && family.string(forType: .string) == "Family")
    #expect(s.outlineView(s.outline, pasteboardWriterForItem: s.item("all")) == nil)
    let board = NSPasteboard(name: .init("jaw-test-\(UUID().uuidString)"))
    defer { board.releaseGlobally() }
    board.clearContents()
    let chat = NSPasteboardItem()
    chat.setString("ana@s.whatsapp.net", forType: .jawChat)
    chat.setString("Ana Ribeiro", forType: .string)
    board.writeObjects([chat])
    #expect(SidebarController.dragged(board) == .item("ana@s.whatsapp.net", name: "Ana Ribeiro"))
    board.clearContents()
    board.writeObjects([family])
    #expect(SidebarController.dragged(board) == .folder("folder:1"))
    board.clearContents()
    let community = NSPasteboardItem()
    community.setString("120363000000000000@g.us", forType: .jawCommunity)
    community.setString("Bairro Alto", forType: .string)
    board.writeObjects([community])
    #expect(SidebarController.dragged(board) == .item("120363000000000000@g.us", name: "Bairro Alto"), "a community files as a chat does")
    board.clearContents()
    let person = NSPasteboardItem()
    person.setString("person:4", forType: .jawPerson)
    person.setString("Inês Gomes", forType: .string)
    board.writeObjects([person])
    #expect(SidebarController.dragged(board) == .item("person:4", name: "Inês Gomes"), "someone from the roster, by their person id")
}

@MainActor
@Test func `a chat dropped near a row's edge goes onto the folder under the pointer, and a folder dropped there goes between the rows`() {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    let chat = FolderTree.Dragged.item("ana@s.whatsapp.net", name: "Ana")
    #expect(s.dropTarget(chat, row: 6)?.id == "folder:1", "Family's row")
    #expect(s.dropTarget(chat, row: 0) == nil && s.dropTarget(chat, row: 5) == nil, "not All, nor the Folders heading")
    #expect(s.dropTarget(.folder("folder:3"), row: 6) == nil, "a folder near a row's edge goes between the rows, as AppKit proposes")
}

@MainActor
@Test func `the tree takes chats, communities, people and folders dragged onto it`() {
    let s = sidebar()
    #expect(Set(s.outline.registeredDraggedTypes).isSuperset(of: [.jawChat, .jawCommunity, .jawPerson, .jawFolder]))
}
