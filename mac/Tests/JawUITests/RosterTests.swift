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
