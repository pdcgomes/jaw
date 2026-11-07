import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// A poll's card: its question, what a vote may choose, each option with its tick, count and bar,
/// and how many voted, in both looks; and what VoiceOver says of it.

func samplePoll(_ id: String) -> MessageRow {
    Samples.polls.first { $0.id == id }!
}

func builder(_ m: MessageRow, _ s: LayoutSpec = spec(.bubbles, dark: true)) -> MessageLayout.Builder {
    MessageLayout.Builder(m: m, width: 640, context: MessageContext(group: true, firstOfRun: true, showTime: true, day: nil),
                          style: s.style, fonts: s.fonts, me: s.me)
}

func pollOptions(_ names: String...) -> [Poll.Option] {
    names.map { Poll.Option(name: $0) }
}

@Test func `a poll says what a vote may choose and how many voted`() {
    typealias B = MessageLayout.Builder
    #expect(B.pollRule(Poll(question: "?", options: pollOptions("a", "b"), max: 1)) == "Choose one")
    #expect(B.pollRule(Poll(question: "?", options: pollOptions("a", "b", "c", "d"), max: 2)) == "Choose up to 2")
    #expect(B.pollRule(Poll(question: "?", options: pollOptions("a", "b", "c"), max: 3)) == "Choose any")
    #expect(B.pollVoted(Poll(question: "?", options: pollOptions("a", "b"))) == "No votes yet")
    #expect(B.pollVoted(Poll(question: "?", options: pollOptions("a", "b"), voters: 1)) == "1 voted")
    #expect(B.pollVoted(Poll(question: "?", options: pollOptions("a", "b"), voters: 1204)) == "1,204 voted")
}

@Test func `an option's bar is its share of those who voted`() throws {
    typealias B = MessageLayout.Builder
    let which = try #require(samplePoll("P1").poll)
    #expect(B.pollShare(which.options[0], in: which) == 0.75 && B.pollShare(which.options[1], in: which) == 0.25)
    let bring = try #require(samplePoll("P2").poll)
    #expect(B.pollShare(bring.options[1], in: bring) == 1, "everyone chose Quickdraws, of several")
    let none = try #require(samplePoll("P3").poll)
    #expect(none.options.allSatisfy { B.pollShare($0, in: none) == 0 })
    let older = Poll(question: "?", options: [Poll.Option(name: "a", votes: 2), Poll.Option(name: "b", votes: 1)])
    #expect(B.pollShare(older.options[1], in: older) == 0.5, "against the most chosen when jawd says nothing of the voters")
}

@Test func `a poll's card grows with its options and a long question, and not with its tallies`() throws {
    let which = builder(samplePoll("P1"))
    let card = try #require(which.cardSize(most: 400))
    #expect(card.width == 300)
    var more = samplePoll("P1")
    more.poll?.options.append(Poll.Option(name: "Monday"))
    let moreCard = try #require(builder(more).cardSize(most: 400))
    #expect(moreCard.height == card.height + which.optionHeight)
    var tallied = samplePoll("P1")
    tallied.poll?.options[1].votes = 12
    tallied.poll?.voters = 15
    #expect(builder(tallied).cardSize(most: 400) == card, "the tallies change, nothing moves")
    let long = builder(samplePoll("P5"))
    let longPoll = try #require(samplePoll("P5").poll)
    #expect(long.questionLines(longPoll, width: 300).count > 1, "a long question wraps")
    #expect(builder(samplePoll("P5")).questionLines(Poll(question: String(repeating: "where? ", count: 60), options: []), width: 300).count == 3,
            "on three lines at most")
}

@Test func `a poll's words are its card's, so its question isn't written twice`() {
    let b = builder(samplePoll("P1"))
    #expect(b.hasCard && b.cardHasTheWords && b.words().length == 0)
    var deleted = samplePoll("P1")
    deleted.deleted = true
    #expect(!builder(deleted).hasCard, "a deleted poll is said deleted, with no card")
}

@Test func `VoiceOver reads a poll's options, their tallies and yours, and how many voted`() {
    #expect(MessageLayout.Builder.spoken(samplePoll("P1")) == "choose one: Saturday, 3 votes, yours; Sunday, 1 vote; 4 voted")
    #expect(MessageLayout.Builder.spoken(samplePoll("P3")) == "choose one: Ramen, 0 votes; Tasca do Chico, 0 votes; Pizza, 0 votes; No votes yet")
}

@MainActor @Test func `VoiceOver says a poll's question and options once, whatever its words hold`() async throws {
    var rows = Samples.polls
    let bring = try #require(rows.firstIndex { $0.id == "P2" })
    rows[bring].spans = [Span(.text, "What do we bring?\nRope\nQuickdraws\nCrash pads")]
    let none = try #require(rows.firstIndex { $0.id == "P3" })
    rows[none].spans = []
    let (c, window) = await shown(rows, PaneHost())
    defer { window.close() }
    func label(_ id: String) throws -> String {
        let i = try #require(c.rows.firstIndex { $0.id == id })
        return try #require(try messageView(c, i).accessibilityLabel())
    }
    let which = try label("P1")
    #expect(which.hasPrefix("Inês, Poll · Which day?, choose one: Saturday, 3 votes, yours; Sunday, 1 vote; 4 voted, "), "\(which)")
    let brought = try label("P2")
    #expect(brought.hasPrefix("Rui, Poll · What do we bring?, choose any: Rope, 2 votes; Quickdraws, 3 votes; Crash pads, 1 vote; 3 voted, "),
            "the words as jawd's rows give them, the question and the options a line each: \(brought)")
    #expect(brought.components(separatedBy: "Quickdraws").count == 2 && brought.components(separatedBy: "bring?").count == 2)
    #expect(try label("P3").hasPrefix("Ana, Poll · Dinner after?, choose one: Ramen, 0 votes;"), "no words at all")
    var deleted = samplePoll("P1")
    deleted.deleted = true
    #expect(MessageLayout.Builder.said(deleted) == "This message was deleted" && MessageLayout.Builder.spoken(deleted) == nil)
}

// MARK: - Voting

@Test func `a vote replaces yours in a poll of one, your choice again takes it back, and several toggle within the most`() throws {
    let which = try #require(samplePoll("P1").poll)
    #expect(Voting.choosing("Sunday", in: which) == .vote(["Sunday"]), "another replaces yours")
    #expect(Voting.choosing("Saturday", in: which) == .vote([]), "yours again takes it back")
    let none = try #require(samplePoll("P3").poll)
    #expect(Voting.choosing("Pizza", in: none) == .vote(["Pizza"]))
    var some = try #require(samplePoll("P2").poll)
    #expect(Voting.choosing("Crash pads", in: some) == .vote(["Crash pads"]))
    some.options[0].mine = true
    #expect(Voting.choosing("Crash pads", in: some) == .vote(["Rope", "Crash pads"]), "added, in the poll's order")
    #expect(Voting.choosing("Rope", in: some) == .vote([]), "taken out")
    let start = try #require(samplePoll("P5").poll)
    #expect(Voting.choosing("Peninha, if the wind has dropped", in: start) == .tooMany(2), "past the most")
    #expect(Voting.choosing("The boulders below the palace", in: start) == .vote(["Cabo da Roca, on the slabs above the lighthouse"]))
    #expect(Voting.tooMany(2) == "this poll takes 2 at most, so take one back first")
}

@Test func `each of a poll's options is a region a click votes in, and a deleted poll has none`() {
    let votes = laid(samplePoll("P1"), spec(.bubbles, dark: true)).regions.compactMap { if case let .vote(name) = $0.1 { name } else { nil } }
    #expect(votes == ["Saturday", "Sunday"])
    var deleted = samplePoll("P1")
    deleted.deleted = true
    #expect(!laid(deleted, spec(.bubbles, dark: true)).regions.contains { if case .vote = $0.1 { true } else { false } })
    for look in Look.allCases {
        let l = laid(samplePoll("P1"), spec(look, dark: false))
        let rects = l.regions.compactMap { if case .vote = $0.1 { $0.0 } else { nil } }
        #expect(rects.count == 2 && rects[0].maxY <= rects[1].minY, "\(look): one over the other")
        #expect(rects.allSatisfy { l.contains(CGPoint(x: $0.midX, y: $0.midY)) }, "\(look): on the message, where a click acts")
    }
}

/// Each option's tick, by the option's name: the symbol drawn left of its name.
func ticks(_ l: Laid) -> [String: CGImage] {
    var out: [String: CGImage] = [:]
    for case let (rect, .vote(name)) in l.regions {
        for case let .image(image, r) in l.parts where r.midX < rect.minX + 16 && r.midY > rect.minY && r.midY < rect.maxY {
            out[name] = image
        }
    }
    return out
}

/// How much of a picture is solid `colour`, give or take what anti-aliasing moves.
func share(of colour: RGBA, in image: CGImage) -> Double {
    let p = pixels(image)
    let want = [colour.r, colour.g, colour.b].map { Int(($0 * 255).rounded()) }
    var n = 0
    for i in stride(from: 0, to: p.count, by: 4) where p[i + 3] >= 250 {
        if abs(Int(p[i]) - want[0]) <= 24, abs(Int(p[i + 1]) - want[1]) <= 24, abs(Int(p[i + 2]) - want[2]) <= 24 { n += 1 }
    }
    return Double(n) / Double(p.count / 4)
}

@Test func `your choice is a tick on a solid disc or square of the accent, and an option you didn't choose an outline`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let s = spec(look, dark: dark)
            let roles = s.style.roles
            let onAccent = roles.onFill
            for (id, yours, other) in [("P1", "Saturday", "Sunday"), ("P5", "Cabo da Roca, on the slabs above the lighthouse", "Peninha, if the wind has dropped")] {
                let drawn = ticks(laid(samplePoll(id), s))
                let mine = try #require(drawn[yours]), theirs = try #require(drawn[other])
                let what = "\(id) in \(look), \(dark ? "dark" : "light")"
                #expect(share(of: roles.accent, in: mine) > 0.3, "\(what): the accent solid, \(share(of: roles.accent, in: mine))")
                #expect(share(of: onAccent, in: mine) > 0.01, "\(what): the tick in the ground's colour or white, \(share(of: onAccent, in: mine))")
                #expect(share(of: roles.accent, in: theirs) == 0, "\(what): an outline, not the accent")
            }
        }
    }
}

@MainActor @Test func `a click on an option asks to vote, a double click's second asks nothing, and one past the most says why`() async throws {
    let host = PaneHost()
    let (c, window) = await shown(Samples.polls, host)
    defer { window.close() }
    var said: [String] = []
    c.notice = { said.append($0) }
    func click(_ id: String, _ option: String, clicks: Int = 1) throws {
        let i = try #require(c.rows.firstIndex { $0.id == id })
        let view = try messageView(c, i)
        let rect = try #require(view.laid?.regions.first { $0.1 == .vote(option) }?.0)
        c.clicked(view, CGPoint(x: rect.midX, y: rect.midY), clicks)
    }
    try click("P1", "Sunday")
    #expect(host.votes.count == 1 && host.votes.first?.message == "P1" && host.votes.first?.options == ["Sunday"])
    try click("P1", "Sunday", clicks: 2)
    #expect(host.votes.count == 1, "a double click's second click votes nothing")
    try click("P5", "Peninha, if the wind has dropped")
    #expect(host.votes.count == 1 && said == ["this poll takes 2 at most, so take one back first"])
}

@MainActor @Test func `a second click before jawd's row for the first builds on it, until a row shows it or jawd says it wasn't done`() async throws {
    let host = PaneHost()
    let (c, window) = await shown(Samples.polls, host)
    defer { window.close() }
    func click(_ id: String, _ option: String) throws {
        let i = try #require(c.rows.firstIndex { $0.id == id })
        let view = try messageView(c, i)
        let rect = try #require(view.laid?.regions.first { $0.1 == .vote(option) }?.0)
        c.clicked(view, CGPoint(x: rect.midX, y: rect.midY), 1)
    }
    /// The poll's row as jawd sends it once it has your vote of `mine`, a version later.
    func voted(_ id: String, _ mine: [String], after: UInt64) -> MessageRow {
        var m = samplePoll(id)
        m.v += after
        for (i, option) in (m.poll?.options ?? []).enumerated() {
            m.poll?.options[i].mine = mine.contains(option.name)
        }
        return m
    }
    try click("P2", "Rope")
    try click("P2", "Crash pads")
    #expect(host.votes.map { $0.options } == [["Rope"], ["Rope", "Crash pads"]], "the second keeps the first, which no row shows yet")
    c.apply(Update(seq: 2, upsert: [voted("P2", ["Rope"], after: 1)]))
    try click("P2", "Quickdraws")
    #expect(host.votes.last?.options == ["Rope", "Quickdraws", "Crash pads"], "a row showing the first alone doesn't show the second")
    c.apply(Update(seq: 3, upsert: [voted("P2", ["Rope", "Quickdraws", "Crash pads"], after: 3)]))
    try click("P2", "Rope")
    #expect(host.votes.last?.options == ["Quickdraws", "Crash pads"], "once a row shows it, the row")
    try click("P1", "Sunday")
    try click("P1", "Sunday")
    #expect(host.votes.suffix(2).map { $0.options } == [["Sunday"], []], "in a poll of one, Sunday again before its row takes it back")
    c.forgetVote(in: "P1")
    try click("P1", "Sunday")
    #expect(host.votes.last?.options == ["Sunday"], "jawd said it wasn't done, so the row again")
}

@MainActor @Test func `a poll's menu has Vote with its options, yours ticked, and Take Back My Vote, with no digits`() async throws {
    let host = PaneHost()
    let (c, window) = await shown(Samples.polls, host)
    defer { window.close() }
    let i = try #require(c.rows.firstIndex { $0.id == "P1" })
    let vote = try #require(c.menu(forRow: i)?.items.first { $0.title == "Vote" }?.submenu)
    #expect(vote.items.map(\.title) == ["Saturday", "Sunday", "", "Take Back My Vote"])
    #expect(vote.items[0].state == .on && vote.items[1].state == .off)
    #expect(vote.items.allSatisfy { $0.keyEquivalent.isEmpty }, "a selected poll's digits react, as a message's do")
    (vote.items[1] as? ClosureItem)?.run()
    #expect(host.votes.last?.options == ["Sunday"])
    (vote.items[3] as? ClosureItem)?.run()
    #expect(host.votes.last?.options == [], "none takes it back")
    let none = try #require(c.rows.firstIndex { $0.id == "P3" })
    #expect(c.menu(forRow: none)?.items.first { $0.title == "Vote" }?.submenu?.items.map(\.title) == ["Ramen", "Tasca do Chico", "Pizza"],
            "no Take Back without a vote")
    #expect(c.voteMenu(for: sampleLink("L1")) == nil && c.newestPoll?.id == "P5")
}

@MainActor @Test func `Enter on a selected poll opens its Vote menu under it, and an option chosen there votes`() async throws {
    let host = PaneHost()
    let (c, window) = await shown(Samples.polls, host)
    defer { window.close() }
    let i = try #require(c.rows.firstIndex { $0.id == "P5" })
    c.select("P5")
    let cell = try #require(c.table.view(atColumn: 0, row: i, makeIfNecessary: false) as? MessageView, "the newest poll, on screen")
    var popped: [(NSMenu, NSPoint)] = []
    cell.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    let enter = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                              context: nil, characters: "\r", charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 36))
    #expect(c.selectionKey(enter), "Enter is taken")
    let (menu, point) = try #require(popped.first)
    #expect(popped.count == 1 && menu.items.map(\.title) == ["Cabo da Roca, on the slabs above the lighthouse", "Peninha, if the wind has dropped",
                                                             "The boulders below the palace", "Anywhere with shade all day, please", "",
                                                             "Take Back My Vote"])
    #expect(menu.items[0].state == .on && menu.items[1].state == .off && menu.items[2].state == .on)
    #expect(menu.items.allSatisfy { $0.keyEquivalent.isEmpty }, "an open menu takes a digit as the start of a name, so none is shown")
    let body = try #require(cell.laid).body
    #expect(point == NSPoint(x: body.minX, y: body.maxY), "under the poll's bubble")
    (menu.items[2] as? ClosureItem)?.run()
    #expect(host.votes.last?.message == "P5" && host.votes.last?.options == ["Cabo da Roca, on the slabs above the lighthouse"],
            "the third, yours, taken out")
}

@Test func `every poll draws as its reference, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let image = try #require(render(spec(look, dark: dark), Samples.polls, typing: false))
            try compare(image, "polls-\(look.rawValue)-\(dark ? "dark" : "light")")
        }
    }
}
