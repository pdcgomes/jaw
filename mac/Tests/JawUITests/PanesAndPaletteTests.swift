import Foundation
@testable import JawKit
@testable import JawUI
import Testing

/// The panes' rules, as the terminal's follow them, and the palette's: actions ranked as jawd ranks
/// names, Tab narrowing and stepping back, reactions by name, forwarding to five chats at most.

@Test func `splitting, closing and swapping keep the panes in their places`() {
    var layout = PaneLayout.pane("ana")
    layout = layout.splitting("ana", .across, with: "rui")
    layout = layout.splitting("rui", .down, with: "mum")
    #expect(layout.panes == ["ana", "rui", "mum"])
    #expect(layout.neighbour(of: "ana", .right) != nil && layout.neighbour(of: "ana", .left) == nil)
    #expect(layout.neighbour(of: "rui", .down) == "mum" && layout.neighbour(of: "mum", .up) == "rui")
    #expect(layout.neighbour(of: "mum", .left) == "ana")
    let swapped = layout.swapping("ana", "mum")
    #expect(swapped.panes == ["mum", "rui", "ana"])
    let closed = layout.removing("rui")
    #expect(closed == .split(.across, 0.5, .pane("ana"), .pane("mum")), "its sibling takes its place")
    #expect(PaneLayout.pane("ana").removing("ana") == nil, "the last pane has nothing left")
    #expect(layout.replacing("ana", with: "sofia").panes == ["sofia", "rui", "mum"])
}

@Test func `a split's share moves within its bounds and is kept as it was`() throws {
    let layout = PaneLayout.pane("ana").splitting("ana", .across, with: "rui")
    let wide = layout.resizing("ana", .across, to: 0.95)
    #expect(wide == .split(.across, 0.8, .pane("ana"), .pane("rui")))
    let frames = wide.frames(in: CGRect(x: 0, y: 0, width: 1000, height: 600))
    #expect(frames["ana"]?.width == 800 && frames["rui"]?.minX == 800)
    let kept = try JSONDecoder().decode(PaneLayout.self, from: JSONEncoder().encode(wide))
    #expect(kept == wide, "kept in the preferences and read back the same")
}

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
