@testable import JawKit
@testable import JawUI
import Testing

/// The poll form: ready once it's a poll jawd takes, trimmed and counted as jawd trims and counts
/// them, saying what's in the way until then; Return going on, adding an option and sending; and a
/// poll jawd turned down opening again as it was.

@MainActor @Test func `a poll is ready with a question and two options, trimmed as jawd trims them`() {
    let form = PollForm(chat: "Lisbon Climbing")
    #expect(form.options == ["", ""] && form.problem == "Ask a question" && form.poll == nil)
    form.question = "  Jantar?  "
    #expect(form.problem == "Give it two options at least")
    form.options = [" Ramen ", "", "Pizza", "  "]
    #expect(form.problem == nil)
    #expect(form.poll?.question == "Jantar?" && form.poll?.options == ["Ramen", "Pizza"] && form.poll?.max == 1,
            "trimmed, the empty options left out")
}

@MainActor @Test func `an option named twice, too many options or too many characters say so`() {
    let form = PollForm(chat: "Lisbon Climbing", question: "Which day?", options: ["Saturday", " Saturday", "Sunday"])
    #expect(form.problem == "Saturday is there twice, and a vote names an option by its name")
    form.options = (1 ... 13).map { "day \($0)" }
    #expect(form.problem == "A poll has 12 options at most")
    form.options = ["a", String(repeating: "é", count: 101)]
    #expect(form.problem == "An option has 100 characters at most")
    form.options = ["a", String(repeating: "é", count: 100)]
    #expect(form.problem == nil, "a hundred is all right")
    form.question = String(repeating: "q", count: 256)
    #expect(form.problem == "A question has 255 characters at most")
    form.question = String(repeating: "👍🏽", count: 128)
    #expect(form.problem == "A question has 255 characters at most", "counted in Unicode scalars, as jawd counts them")
    form.question = String(repeating: "👍🏽", count: 127)
    #expect(form.problem == nil)
}

@MainActor @Test func `several answers let a vote choose every option`() {
    let form = PollForm(chat: "Lisbon Climbing", question: "What do we bring?", options: ["Rope", "Quickdraws", "Crash pads", ""], several: true)
    #expect(form.poll?.max == 3)
}

@MainActor @Test func `Return goes on, adds an option, and sends from an empty one`() {
    let form = PollForm(chat: "Lisbon Climbing")
    #expect(form.returned(in: .question) == .moved && form.focus == .option(0))
    #expect(form.returned(in: .option(0)) == .notReady, "an empty option, and no question yet")
    form.question = "Jantar?"
    form.options[0] = "Ramen"
    #expect(form.returned(in: .option(0)) == .moved && form.focus == .option(1))
    form.options[1] = "Pizza"
    #expect(form.returned(in: .option(1)) == .added && form.options.count == 3 && form.focus == .option(2))
    #expect(form.returned(in: .option(2)) == .send, "Return in the empty one sends")
    form.options = (1 ... 12).map { "day \($0)" }
    #expect(form.returned(in: .option(11)) == .send && form.options.count == 12, "the twelfth sends rather than adding a thirteenth")
    form.add()
    #expect(form.options.count == 12)
    form.focus = .option(11)
    form.remove(at: 11)
    #expect(form.options.count == 11 && form.focus == .option(10))
    let two = PollForm(chat: "Lisbon Climbing")
    two.remove(at: 0)
    #expect(two.options.count == 2, "two stay")
}

@MainActor @Test func `a poll jawd turned down opens again as it was, with jawd's words, until it's changed`() {
    let turned = SendPoll(chat: "g", question: "What do we bring?", options: ["Rope", "Quickdraws"], max: 2, key: "jaw-k")
    let form = PollForm(chat: "Lisbon Climbing", returned: turned, said: "the outbox is full")
    #expect(form.question == "What do we bring?" && form.options == ["Rope", "Quickdraws"] && form.several)
    #expect(form.said == "the outbox is full")
    form.question += " On Saturday"
    #expect(form.said == nil, "jawd's words go once it's changed")
    let one = PollForm(chat: "Lisbon Climbing", returned: SendPoll(chat: "g", question: "?", options: ["a"], key: nil), said: "no")
    #expect(one.options == ["a", ""] && !one.several, "two fields at least")
}
