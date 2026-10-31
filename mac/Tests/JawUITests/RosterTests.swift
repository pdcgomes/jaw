import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The roster: under All, grouped by your top folders, then Others, each with who's around listed and
/// the rest folded; under a folder, its people as jawd sorts them, those jawd has no last seen for
/// folded; someone typing in their chat around and listed; faint where jawd isn't following someone;
/// how each is, in words; and more people asked for as it scrolls.

/// The roster's lines as words.
func lineWords(_ lines: [RosterLine]) -> [String] {
    lines.map { line in
        switch line {
        case let .title(text): "\(text):"
        case let .heading(name, around, of): "\(name) \(around) of \(of)"
        case let .person(p, _): p.name
        case let .offline(_, count, open): "\(open ? "▾" : "▸") \(count) offline"
        }
    }
}

/// Family's people, as a roster of Family holds them.
var familyPeople: [RosterRow] {
    Samples.people.filter { !Samples.tree.subtree("folder:1").isDisjoint(with: $0.folders) }
}

@MainActor
final class RosterStub: RosterHost {
    var moved: [Place] = []

    func roster(_: RosterController, move place: Place) {
        moved.append(place)
    }

    var opened: [String] = []
    var asked: [String] = []

    func roster(_: RosterController, open person: RosterRow) {
        opened.append(person.id)
    }

    func roster(_: RosterController, menuItemsFor person: RosterRow) -> [NSMenuItem] {
        asked.append(person.id)
        return [NSMenuItem(title: person.chat == nil ? "Start Chat" : "Open Chat", action: nil, keyEquivalent: "")]
    }
}

@MainActor
func roster(_ host: RosterStub, folder: String = "all", people: [RosterRow] = Samples.people, total: UInt64 = 400) -> RosterController {
    let r = RosterController(folder: folder)
    r.host = host
    r.tree = Samples.tree
    r.now = { Samples.now }
    r.roles = Catalog.jaw.roles(dark: true)
    _ = r.view
    r.apply(Update(seq: 1, upsert: people, total: total))
    return r
}

@Test func `under All the roster is grouped by your top folders, then Others, each with who's around and the rest folded`() {
    #expect(lineWords(RosterLines.lines(Samples.people, folder: "all", tree: Samples.tree, open: []))
        == ["People:", "Family 1 of 5", "Ana Ribeiro", "▸ 4 offline", "Climbing 2 of 3", "Marta", "Rui Costa", "▸ 1 offline",
            "Work 0 of 1", "▸ 1 offline", "Others 0 of 1", "▸ 1 offline"])
}

@Test func `a fold opened lists its people after it, as jawd sorts them`() {
    let lines = RosterLines.lines(Samples.people, folder: "all", tree: Samples.tree, open: ["folder:1"])
    #expect(Array(lineWords(lines)[1 ... 7]) == ["Family 1 of 5", "Ana Ribeiro", "▾ 4 offline", "Mãe", "Inês Gomes", "Avó Lurdes", "Sofia Martins"])
}

@Test func `under a folder its people are listed as jawd sorts them, and those jawd has no last seen for are folded`() {
    #expect(lineWords(RosterLines.lines(familyPeople, folder: "folder:1", tree: Samples.tree, open: []))
        == ["People in Family:", "Ana Ribeiro", "Mãe", "Inês Gomes", "▸ 2 offline"])
    #expect(lineWords(RosterLines.lines([], folder: "groups", tree: Samples.tree, open: [])) == ["People in Groups:"], "Groups holds nobody")
}

@Test func `someone typing in their chat is around and listed, never folded, though jawd has no last seen for them`() {
    let typing: Set<String> = ["person:7"]
    #expect(Array(lineWords(RosterLines.lines(Samples.people, folder: "all", tree: Samples.tree, open: [], typing: typing))[1 ... 4])
        == ["Family 2 of 5", "Ana Ribeiro", "Avó Lurdes", "▸ 3 offline"], "Avó Lurdes hides her presence, and her chat says she's typing")
    #expect(lineWords(RosterLines.lines(familyPeople, folder: "folder:1", tree: Samples.tree, open: [], typing: typing))
        == ["People in Family:", "Ana Ribeiro", "Mãe", "Inês Gomes", "Avó Lurdes", "▸ 1 offline"], "where jawd sorts her, after those last seen")
}

@Test func `how someone is: typing in the online colour while their chat says so, online, a time ago, and nothing when they hide it`() {
    let now = Samples.now
    #expect(RosterLines.said(Presence(state: .seen, at: now - 60000, known: false), typing: true, now: now) == RosterLines.Said(text: "typing…", online: true),
            "their chat says they're typing or recording, whatever jawd last knew of their presence")
    #expect(RosterLines.said(Presence(state: .online), now: now) == RosterLines.Said(text: "online", online: false))
    #expect(RosterLines.said(Presence(state: .seen, at: now - 12 * 60000), now: now).text == "12 min ago")
    #expect(RosterLines.said(Presence(state: .hidden), now: now).text.isEmpty)
}

@MainActor
@Test func `the roster draws its lines, typing from the chat and never faint, faint where jawd isn't following someone, and opens a fold`() {
    let host = RosterStub()
    let r = roster(host)
    defer { withExtendedLifetime(host) {} }
    #expect(Array(r.said.prefix(4)) == ["People:", "Family 1 of 5", "Ana Ribeiro online", "▸ 4 offline"])
    r.typing = ["person:1"]
    #expect(r.said[2] == "Ana Ribeiro typing…", "her chat says she's typing")
    r.toggle("folder:1")
    #expect(r.said[3] == "▾ 4 offline" && r.said[4] == "Mãe 12 min ago (faint)")
    r.typing = ["person:1", "person:2"]
    #expect(Array(r.said[1 ... 4]) == ["Family 2 of 5", "Ana Ribeiro typing…", "Mãe typing…", "▾ 3 offline"],
            "typing shows only while jawd hears it, so it's never faint")
}

@MainActor
@Test func `the roster asks for more only once the last person it holds is in sight, and never past the 1,000 jawd keeps`() throws {
    let host = RosterStub()
    let r = roster(host)
    r.grow(visibleTo: r.said.count)
    #expect(host.moved.isEmpty, "Sofia, the last it holds, is folded under Family's 4 offline")
    r.toggle("folder:1")
    let sofia = try #require(r.said.firstIndex { $0.hasPrefix("Sofia Martins") })
    r.grow(visibleTo: sofia)
    #expect(host.moved.isEmpty, "the lines in sight end just before her")
    r.grow(visibleTo: sofia + 1)
    #expect(host.moved == [.top(210)], "200 more than it holds")

    let many = (1 ... 900).map { i in
        RosterRow(id: "person:\(100 + i)", v: 1, sort: "0~\(100_000 + i)", name: "Someone \(i)", presence: Presence(state: .online),
                  folders: ["folder:1"])
    }
    let full = RosterStub()
    let big = roster(full, folder: "folder:1", people: many, total: 5000)
    big.grow(visibleTo: big.said.count)
    #expect(full.moved == [.top(1000)], "up to the 1,000 jawd keeps, not 1,100")
    var held = Update<RosterRow>(seq: 2, total: 5000)
    held.moved = Moved(place: .top(1000), newer: 0)
    big.apply(held)
    big.grow(visibleTo: big.said.count)
    #expect(full.moved == [.top(1000)], "nothing more once it's there")
}

@Test func `someone's menu opens or starts their chat, opens it in a new pane, and files them, with their chat's slot when they have one`() {
    let ana = Samples.people[0], marta = Samples.people[1]
    #expect(RosterMenu.entries(ana, tree: Samples.tree, list: "all").map(head)
        == ["Open Chat", "person.pane", "—", "Add to Folder", "—", "Favourite"])
    #expect(RosterMenu.entries(marta, tree: Samples.tree, list: "all").map(head) == ["Start Chat", "person.pane", "—", "Add to Folder"])
    #expect(RosterMenu.entries(ana, tree: Samples.tree, list: "folder:1").map(head)
        == ["Open Chat", "person.pane", "—", "Add to Folder", "Move to", "unfile", "—", "Favourite"])
    #expect(RosterMenu.entries(ana, tree: FolderTree(), list: "all").map(head) == ["Open Chat", "person.pane"])
}

@MainActor
@Test func `Return or a double click opens someone's chat, a right click asks for their menu, and they drag as themselves`() throws {
    let host = RosterStub()
    let r = roster(host)
    r.table.selectRowIndexes([2], byExtendingSelection: false)
    r.openSelected()
    #expect(host.opened == ["person:1"], "Ana Ribeiro, on the roster's third line")
    #expect(r.menuItems(forRow: 2).map(\.title) == ["Open Chat"] && r.menuItems(forRow: 1).isEmpty, "a heading has no menu")
    #expect(host.asked == ["person:1"])
    let item = try #require(r.tableView(r.table, pasteboardWriterForRow: 2) as? NSPasteboardItem)
    #expect(item.string(forType: .jawPerson) == "person:1" && item.string(forType: .string) == "Ana Ribeiro")
    #expect(r.tableView(r.table, pasteboardWriterForRow: 1) == nil)
}

@MainActor
@Test func `VoiceOver reads someone's row once, from the row itself, saying a faint row's state is what was last known`() throws {
    let host = RosterStub()
    let r = roster(host)
    let window = NSWindow(contentViewController: r)
    defer { withExtendedLifetime(window) {} }
    var lurdes = try #require(Samples.people.first { $0.id == "person:7" })
    lurdes.v = 2
    lurdes.presence.known = false
    r.apply(Update(seq: 2, upsert: [lurdes]))
    r.toggle("folder:1")
    r.view.layoutSubtreeIfNeeded()
    #expect(r.said[2] == "Ana Ribeiro online" && r.said[4] == "Mãe 12 min ago (faint)" && r.said[6] == "Avó Lurdes (faint)")
    let cell = { (row: Int) in r.table.view(atColumn: 0, row: row, makeIfNecessary: true) as? PersonCell }
    let ana = try #require(cell(2))
    #expect(ana.isAccessibilityElement() && ana.accessibilityRole() == .button && ana.accessibilityLabel() == "Ana Ribeiro, online")
    let parts = NSAccessibility.unignoredChildren(from: ana.accessibilityChildren() ?? [])
    #expect(parts.isEmpty, "nothing in it read again: \(parts)")
    #expect(try #require(cell(4)).accessibilityLabel() == "Mãe, 12 min ago, last known", "jawd isn't following her now")
    #expect(try #require(cell(6)).accessibilityLabel() == "Avó Lurdes", "she hides when she was last seen, so nothing is said of it")
}

@MainActor
@Test func `a fold is selected and opened from the keyboard: Return or Space opens or closes it, → opens it and ← closes it`() throws {
    let host = RosterStub()
    let r = roster(host)
    let window = NSWindow(contentViewController: r)
    defer { withExtendedLifetime(window) {} }
    #expect(r.tableView(r.table, shouldSelectRow: 3) && r.tableView(r.table, shouldSelectRow: 2), "Family's fold, and Ana")
    #expect(!r.tableView(r.table, shouldSelectRow: 0) && !r.tableView(r.table, shouldSelectRow: 1), "not the title, nor a heading")
    r.table.selectRowIndexes([3], byExtendingSelection: false)
    let press = { (code: UInt16) in r.table.keyDown(with: try plainKey(window, "", code)) }
    try press(124)
    #expect(r.said[3] == "▾ 4 offline" && r.said[4] == "Mãe 12 min ago (faint)", "→ opens it")
    try press(124)
    #expect(r.said[3] == "▾ 4 offline", "and leaves it open")
    try press(123)
    #expect(r.said[3] == "▸ 4 offline", "← closes it")
    try press(36)
    #expect(r.said[3] == "▾ 4 offline", "Return opens it")
    try press(49)
    #expect(r.said[3] == "▸ 4 offline" && r.table.selectedRow == 3, "Space closes it, which stays selected")
    r.table.selectRowIndexes([2], byExtendingSelection: false)
    try press(36)
    try press(49)
    #expect(host.opened == ["person:1"] && r.said[3] == "▸ 4 offline", "Return on Ana opens her chat, and Space does nothing")
}

@MainActor
@Test func `VoiceOver reads a fold as a disclosure triangle, saying whose it is and whether it's open, and a press opens or closes it`() throws {
    let host = RosterStub()
    let r = roster(host)
    let window = NSWindow(contentViewController: r)
    defer { withExtendedLifetime(window) {} }
    let cell = { (row: Int) throws -> GroupCell in try #require(r.table.view(atColumn: 0, row: row, makeIfNecessary: true) as? GroupCell) }
    let fold = try cell(3)
    #expect(fold.isAccessibilityElement() && fold.accessibilityRole() == .disclosureTriangle)
    #expect(fold.accessibilityLabel() == "4 offline in Family" && (fold.accessibilityValue() as? NSNumber) == 0)
    #expect(NSAccessibility.unignoredChildren(from: fold.accessibilityChildren() ?? []).isEmpty)
    #expect(fold.accessibilityPerformPress() && r.said[3] == "▾ 4 offline")
    let open = try cell(3)
    #expect((open.accessibilityValue() as? NSNumber) == 1, "open now")
    #expect(open.accessibilityPerformPress() && r.said[3] == "▸ 4 offline")
    let heading = try cell(1)
    #expect(!heading.isAccessibilityElement() && !heading.accessibilityPerformPress(), "a heading's words are read as they are")
    let family = roster(host, folder: "folder:1", people: familyPeople)
    #expect(family.said.last == "▸ 2 offline")
    let inFamily = try #require(family.table.view(atColumn: 0, row: family.said.count - 1, makeIfNecessary: true) as? GroupCell)
    #expect(inFamily.accessibilityLabel() == "2 offline", "under a folder, the fold is the folder's")
}

@MainActor
@Test func `the selection stays on its line as people come and move, and goes when its line does`() throws {
    let host = RosterStub()
    let r = roster(host)
    r.table.selectRowIndexes([2], byExtendingSelection: false)
    var mae = try #require(Samples.people.first { $0.id == "person:2" })
    mae.v = 2
    mae.sort = "0~a mãe\u{1}000000000002"
    mae.presence = Presence(state: .online)
    r.apply(Update(seq: 2, upsert: [mae]))
    #expect(Array(r.said[1 ... 3]) == ["Family 2 of 5", "Mãe online", "Ana Ribeiro online"])
    #expect(r.table.selectedRow == 3, "Ana still, a line down")
    r.table.selectRowIndexes([4], byExtendingSelection: false)
    r.typing = ["person:7"]
    #expect(r.said[4] == "Avó Lurdes typing…" && r.said[5] == "▸ 2 offline" && r.table.selectedRow == 5, "Family's fold still")
    r.apply(Update(seq: 3, remove: ["person:1"]))
    r.table.selectRowIndexes([2], byExtendingSelection: false)
    r.apply(Update(seq: 4, remove: ["person:2"]))
    #expect(r.table.selectedRow == -1, "Mãe gone, and nothing else selected in her place")
}

@MainActor
@Test func `VoiceOver's Show Menu on someone's row opens their menu under the row`() throws {
    let host = RosterStub()
    let r = roster(host)
    let cell = try #require(r.table.view(atColumn: 0, row: 2, makeIfNecessary: true) as? PersonCell)
    var popped: [(NSMenu, NSPoint)] = []
    cell.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    #expect(cell.accessibilityPerformShowMenu())
    #expect(popped.first?.0.items.map(\.title) == ["Open Chat"] && popped.first?.1 == NSPoint(x: cell.bounds.minX, y: cell.bounds.minY))
}
