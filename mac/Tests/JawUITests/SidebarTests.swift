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
