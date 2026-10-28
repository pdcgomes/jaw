import AppKit
@testable import JawKit
@testable import JawUI
import Testing

/// The palette's rules: actions ranked as jawd ranks
/// names, Tab narrowing and stepping back, reactions by name, forwarding to five chats at most, and
/// ⌘↩ for a new pane, which the palette and the glance take before the menu bar.

@Test func `actions are matched as jawd matches names`() {
    #expect(PaletteModel.score("sta", "Start a chat") == 100, "the start")
    #expect(PaletteModel.score("sac", "Start a chat") == 90, "initials")
    #expect(PaletteModel.score("chat st", "Start a chat") == 80, "word starts")
    #expect(PaletteModel.score("art", "Start a chat") == 40, "a part of a word")
    #expect(PaletteModel.score("xyz", "Start a chat") == nil)
    #expect(PaletteModel.score("mute", "Mute this chat for a week") == 100)
    #expect(PaletteModel.score("arq", "Arquivar") == 100 && PaletteModel.score("ines", "Inês") == 100, "accents aside")
}

@MainActor
@Test func `a verb typed puts the actions first, and the rest after the names`() {
    let model = PaletteModel()
    model.actions = { [PaletteAction("Mute this chat always") {}, PaletteAction("Start a chat") {}] }
    model.query = "mute"
    #expect(model.items.first?.group == .actions, "mute is an action")
    model.query = "an"
    let groups = model.items.map(\.group)
    #expect(!groups.contains(.actions) || groups.first != .actions, "a name isn't pushed down by actions")
}

@MainActor
@Test func `Tab steps through the kinds found and back to all of them`() throws {
    let model = PaletteModel()
    var asked: [[FindKind]] = []
    model.find = { _, _, kinds, _, _ in asked.append(kinds) }
    model.reset()
    let json = #"{"results":[],"counts":{"chat":2,"message":40}}"#
    model.answered(id: 1, try JSONDecoder().decode(Found.self, from: Data(json.utf8)))
    model.tab()
    #expect(model.kind == .chat && asked.last == [.chat])
    model.tab()
    #expect(model.kind == .message)
    model.tab()
    #expect(model.kind == nil, "and round to all")
    model.tab(back: true)
    #expect(model.kind == .message)
    #expect(model.backspace() && model.kind == nil, "backspace on nothing widens it")
}

@MainActor
@Test func `reactions are the quick six, or emoji by name`() {
    let model = PaletteModel()
    var reacted: String?
    model.chose = { if case let .emoji(_, e) = $0 { reacted = e } }
    model.reset(.react(message: "m1", summary: "hi"))
    #expect(model.items.count == 7)
    #expect(model.digit(2) && reacted == "❤️")
    model.query = "thumbs"
    #expect(model.items.contains { $0.title.map(\.s).joined().hasPrefix("👍") })
}

@MainActor
@Test func `⌘↩ puts a chat in a pane of its own, before the menu bar sees it, and does what ↩ does with anything else`() throws {
    let panel = PalettePanel()
    let model = panel.model
    var chosen: [String] = []
    model.chose = { pick in
        switch pick {
        case let .newPane(row): chosen.append("new pane \(row.id)")
        case let .chat(row): chosen.append("chat \(row.id)")
        case let .action(a): chosen.append("action \(a.name)")
        default: chosen.append("something else")
        }
    }
    model.actions = { [PaletteAction("Start a chat") {}] }
    model.reset()
    let json = #"{"results":[{"kind":"chat","score":1,"text":[],"row":{"id":"c1","v":1,"sort":"1","kind":"direct","name":"Ana"}}],"counts":{"chat":1}}"#
    model.answered(id: 1, try JSONDecoder().decode(Found.self, from: Data(json.utf8)))
    #expect(try panel.performKeyEquivalent(with: returnKey(.command)))
    model.pick()
    #expect(chosen == ["new pane c1", "chat c1"])
    model.index = try #require(model.items.firstIndex { $0.group == .actions })
    model.pick(newPane: true)
    #expect(chosen.last == "action Start a chat", "an action is run as ↩ runs it")
    #expect(try !panel.performKeyEquivalent(with: returnKey([.command, .shift])), "⇧⌘↩ is the menu bar's")
    #expect(chosen.count == 3)
}

@MainActor
@Test func `a floating panel's keys come before the menu bar's, as the glance's ⌘↩`() throws {
    let panel = FloatingPanel()
    var seen: [UInt16] = []
    panel.keys = { event in
        seen.append(event.keyCode)
        return event.keyCode == 36
    }
    #expect(try panel.performKeyEquivalent(with: returnKey(.command)))
    let palette = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command, timestamp: 0, windowNumber: 0,
                                                context: nil, characters: "k", charactersIgnoringModifiers: "k", isARepeat: false, keyCode: 40))
    #expect(!panel.performKeyEquivalent(with: palette), "a key it doesn't take goes on to the menu bar")
    #expect(seen == [36, 40])
}

@MainActor
@Test func `forwarding picks five chats at most with Tab`() throws {
    let model = PaletteModel()
    var forwarded: [String] = []
    model.chose = { if case let .forward(_, _, to) = $0 { forwarded = to } }
    model.reset(.forward(messages: ["m1"], from: "ana", summary: "hi"))
    #expect(model.kind == .chat)
    let rows = (1 ... 7).map { n in
        #"{"kind":"chat","score":1,"text":[],"row":{"id":"c\#(n)","v":1,"sort":"\#(n)","kind":"direct","name":"Chat \#(n)"}}"#
    }
    let json = #"{"results":[\#(rows.joined(separator: ","))],"counts":{"chat":7}}"#
    model.answered(id: 1, try JSONDecoder().decode(Found.self, from: Data(json.utf8)))
    for _ in 0 ..< 7 {
        model.index = model.picked.count
        model.tab()
    }
    #expect(model.picked.count == PaletteModel.most)
    model.pick()
    #expect(forwarded.count == 5 && forwarded.first == "c1")
}

@MainActor
@Test func `the selection stays on its row as rows come in above it, and ↑ and ↓ reach every row`() {
    let model = PaletteModel()
    var names = ["Mute this chat always", "Start a chat", "Search messages"]
    model.actions = { names.map { PaletteAction($0) {} } }
    model.move(1)
    model.move(1)
    #expect(model.items[model.index].id == "action:Search messages")
    names.insert("Go to unread", at: 0)
    #expect(model.items[model.index].id == "action:Search messages", "rows arriving above don't move it")
    #expect(model.index == 3)
    var seen: Set<String> = []
    for _ in 0 ..< model.items.count {
        model.move(-1)
        seen.insert(model.items[model.index].id)
    }
    #expect(seen.count == model.items.count, "↑ goes through every row, the top ones too")
    model.query = "x"
    #expect(model.selected == nil && model.index == 0, "typing goes back to the top")
}
