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

@MainActor @Test func `each option keeps its number as others come and go, and the field of one taken out writes nothing`() {
    let form = PollForm(chat: "Lisbon Climbing", question: "Jantar?", options: ["Ramen", "Pizza", "Tasca"])
    #expect(form.ids == [0, 1, 2])
    form.focus = .option(2)
    form.remove(at: 2)
    #expect(form.ids == [0, 1] && form.options == ["Ramen", "Pizza"] && form.focus == .option(1), "the third, being typed in, taken out")
    form.write("Tasca do Chico", to: 2)
    #expect(form.options == ["Ramen", "Pizza"] && form.words(of: 2).isEmpty, "its field's last write goes nowhere")
    form.add()
    #expect(form.ids == [0, 1, 3], "a new option has a number of its own")
    form.write("Sushi", to: 3)
    form.remove(at: 0)
    form.write("Ramen", to: 0)
    #expect(form.options == ["Pizza", "Sushi"] && form.words(of: 1) == "Pizza", "the first taken out, the others keep their words")
    form.options = ["a", "b", "c", "d"]
    #expect(form.ids == [1, 3, 4, 5], "options set whole keep the numbers they had, and new ones get theirs")
    form.options = ["a", "b"]
    #expect(form.ids == [1, 3])
    let middle = PollForm(chat: "Lisbon Climbing", options: ["Saturday", "Sunday", "Monday"])
    middle.focus = .option(1)
    middle.remove(at: 1)
    #expect(middle.options == ["Saturday", "Monday"] && middle.focus == .option(2), "the keys go to the option in its place, by its number")
    let turned = PollForm(chat: "Lisbon Climbing", returned: SendPoll(chat: "g", question: "?", options: ["Rope", "Pads"], key: nil), said: "no")
    turned.write("Rope", to: turned.ids[0])
    #expect(turned.said == "no", "a field writing what it already had changes nothing")
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
