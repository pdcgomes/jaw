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
