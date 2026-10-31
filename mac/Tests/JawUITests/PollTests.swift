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
                          style: s.style, fonts: s.fonts, me: s.me, pictures: s.pictures)
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

@MainActor @Test func `a poll's menu has Vote with its options, yours ticked, on their digits, and Take Back My Vote`() async throws {
    let host = PaneHost()
    let (c, window) = await shown(Samples.polls, host)
    defer { window.close() }
    let i = try #require(c.rows.firstIndex { $0.id == "P1" })
    let vote = try #require(c.menu(forRow: i)?.items.first { $0.title == "Vote" }?.submenu)
    #expect(vote.items.map(\.title) == ["Saturday", "Sunday", "", "Take Back My Vote"])
    #expect(vote.items[0].state == .on && vote.items[1].state == .off)
    #expect(vote.items[0].keyEquivalent == "1" && vote.items[1].keyEquivalent == "2" && vote.items[0].keyEquivalentModifierMask.isEmpty)
    (vote.items[1] as? ClosureItem)?.run()
    #expect(host.votes.last?.options == ["Sunday"])
    (vote.items[3] as? ClosureItem)?.run()
    #expect(host.votes.last?.options == [], "none takes it back")
    let none = try #require(c.rows.firstIndex { $0.id == "P3" })
    #expect(c.menu(forRow: none)?.items.first { $0.title == "Vote" }?.submenu?.items.map(\.title) == ["Ramen", "Tasca do Chico", "Pizza"],
            "no Take Back without a vote")
    #expect(c.voteMenu(for: sampleLink("L1")) == nil && c.newestPoll?.id == "P5")
}

@Test func `every poll draws as its reference, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let image = try #require(render(spec(look, dark: dark), Samples.polls, typing: false))
            try compare(image, "polls-\(look.rawValue)-\(dark ? "dark" : "light")")
        }
    }
}
