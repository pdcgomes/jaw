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

@Test func `every poll draws as its reference, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let image = try #require(render(spec(look, dark: dark), Samples.polls, typing: false))
            try compare(image, "polls-\(look.rawValue)-\(dark ? "dark" : "light")")
        }
    }
}
