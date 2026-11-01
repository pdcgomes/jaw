import AppKit
import CoreGraphics
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The library as a pane: its slot's chat, where it opens among the panes and where the keys go as
/// it closes, and the chat header's button that opens a chat's.

@Test func `a library's slot is library, or library: and a chat's ID, never a chat, and named for the editor`() {
    #expect(LibraryPane.id(.every) == "library" && LibraryPane.id(.chat(id: "ana@s", name: "Ana")) == "library:ana@s")
    #expect(LibraryPane.isLibrary("library") && LibraryPane.isLibrary("library:ana@s") && !LibraryPane.isLibrary("ana@s"))
    #expect(!LibraryPane.isLibrary("librarian@s.whatsapp.net"))
    #expect(LibraryPane.scope(of: "library:ana@s") { _ in "Ana Ribeiro" } == .chat(id: "ana@s", name: "Ana Ribeiro"))
    #expect(LibraryPane.scope(of: "library") { _ in "" } == .every && LibraryPane.scope(of: "ana@s") { _ in "" } == nil)
    #expect(LibraryPane.name("library") { _ in "" } == "Library" && LibraryPane.name("library:g") { _ in "Lisbon Climbing" } == "Library · Lisbon Climbing")
    #expect(LibraryPane.name("ana@s") { _ in "Ana" } == nil)
}

@Test func `a library opens beside the focused pane, in an empty one, in its place with no room, and goes to the pane that shows it`() {
    let size = CGSize(width: 1200, height: 800)
    #expect(LibraryPane.open("library", in: nil, focused: nil, size: size) == .into(.single("library"), 1, replacing: nil), "with no panes, alone")
    let one = PaneGrid.single("ana")
    guard case let .into(beside, slot, replacing) = LibraryPane.open("library:ana", in: one, focused: 1, size: size) else {
        Issue.record("not opened")
        return
    }
    #expect(beside.slots.count == 2 && beside.slot(slot)?.chat == "library:ana" && beside.slot(showing: "ana") != nil && replacing == nil)
    #expect(beside.frames(in: CGRect(origin: .zero, size: size))[slot]!.minX > 0, "across, to the chat's right")
    #expect(LibraryPane.open("library:ana", in: beside, focused: 1, size: size) == .shown(slot), "again: to its pane")
    #expect(LibraryPane.open("library", in: .single(nil), focused: 1, size: size) == .into(.single("library"), 1, replacing: nil),
            "an empty pane takes it")
    let tight = CGSize(width: 300, height: 300)
    #expect(LibraryPane.open("library", in: one, focused: 1, size: tight) == .into(.single("library"), 1, replacing: "ana"),
            "with no room to split, it takes the focused pane's place")
}

@Test func `as a library closes the keys go beside it, or the chat it came from takes its place`() {
    let size = CGSize(width: 1200, height: 800)
    guard case let .into(two, _, _) = LibraryPane.open("library:ana", in: .single("ana"), focused: 1, size: size) else { return }
    let (back, next) = LibraryPane.close("library:ana", in: two, cameFrom: "ana")
    #expect(back.slots.count == 1 && back.slots[0].chat == "ana" && next == back.slots[0].id, "the chat beside it grows into its place")
    let (alone, at) = LibraryPane.close("library", in: .single("library"), cameFrom: "ana")
    #expect(alone.slots.map(\.chat) == ["ana"] && at == 1, "alone, the chat it came from comes back")
    let (empty, _) = LibraryPane.close("library", in: .single("library"), cameFrom: nil)
    #expect(empty.slots.map(\.chat) == [nil], "or an empty pane, waiting for one")
    #expect(LibraryPane.close("library", in: .single("ana"), cameFrom: nil).0 == .single("ana"), "a layout without it is left as it is")
}

@MainActor @Test func `a chat's header has the library's button, with its key, shown while the pointer is over it`() {
    final class Host: ConversationHost {
        var asked = 0
        func conversation(_: ConversationController, move _: Place) {}
        func conversation(_: ConversationController, send _: String, key _: String, reply _: String?, mentions _: [String]) {}
        func conversation(_: ConversationController, react _: String, emoji _: String) {}
        func conversation(_: ConversationController, edit _: String, text _: String) {}
        func conversation(_: ConversationController, delete _: String, everyone _: Bool) {}
        func conversation(_: ConversationController, info _: String) {}
        func conversation(_: ConversationController, retry _: String) {}
        func conversation(_: ConversationController, read _: String) {}
        func conversation(_: ConversationController, typing _: TypingState) {}
        func conversation(_: ConversationController, draft _: String) {}
        func conversationWantsHistory(_: ConversationController) {}
        func conversation(_: ConversationController, forward _: MessageRow) {}
        func conversation(_: ConversationController, reactTo _: MessageRow) {}
        func conversationFocused(_: ConversationController) {}
        func conversation(_: ConversationController, open _: String) {}
        func conversation(_: ConversationController, vote _: String, options _: [String]) {}
        func conversationIsRead(_: ConversationController) -> Bool { false }
        func conversationLibrary(_: ConversationController) { asked += 1 }
    }
    let host = Host()
    let c = ConversationController(chat: Samples.group, spec: spec(.bubbles, dark: true), layouts: Layouts(), place: .newest(10))
    c.host = host
    _ = c.view
    let button = c.header.library
    #expect(button.label == "This Chat's Library" && button.shortcut == Shortcut("l", [.command, .option]) && button.toolTip == "This Chat's Library (⌥⌘L)")
    #expect(button.isHidden, "until the pointer is over the header")
    c.header.mouseEntered(with: NSEvent())
    #expect(!button.isHidden)
    c.header.mouseExited(with: NSEvent())
    #expect(button.isHidden)
    button.click()
    #expect(host.asked == 1, "it asks for the chat's library")
}
