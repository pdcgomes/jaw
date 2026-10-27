import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// A right click anywhere in a pane offers its splits: after a message's own actions, or as the
/// pane's whole menu on its header, around its messages and on its system lines. The pane takes
/// the keys first, so the menu acts on it.

@MainActor
final class PaneHost: ConversationHost {
    var focused = 0

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
    func conversationFocused(_: ConversationController) { focused += 1 }
    func conversation(_: ConversationController, open _: String) {}
    func conversationIsRead(_: ConversationController) -> Bool { false }
}

@MainActor
func pane(_ host: PaneHost) -> ConversationController {
    let c = ConversationController(chat: Samples.group, spec: spec(.bubbles, dark: true), layouts: Layouts(),
                                   place: ConversationController.place(for: Samples.group))
    c.host = host
    c.paneMenu = {
        let menu = NSMenu()
        menu.addItem(withTitle: "Pane", action: nil, keyEquivalent: "")
        return menu
    }
    c.messageMenuTail = { [NSMenuItem(title: "Split Across", action: nil, keyEquivalent: "d")] }
    return c
}

@MainActor
@Test func `a message's menu ends with the pane's splits, the pane taking the keys first`() throws {
    let host = PaneHost()
    let c = pane(host)
    let text = try #require(Samples.messages.first { $0.kind == .text && !$0.deleted })
    let menu = try #require(c.menu(for: text))
    #expect(menu.items.first?.title == "Reply")
    #expect(menu.items.last?.title == "Split Across")
    #expect(menu.items.dropLast().last?.isSeparatorItem == true, "after a separator")
    #expect(host.focused == 1)
}

@MainActor
@Test func `a system line, the header and the space around the messages open the pane's menu`() throws {
    let host = PaneHost()
    let c = pane(host)
    _ = c.view
    let system = try #require(Samples.messages.first { $0.kind == .system })
    #expect(c.menu(for: system)?.items.map(\.title) == ["Pane"])
    let click = try #require(NSEvent.mouseEvent(with: .rightMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
    #expect(c.header.menu(for: click)?.items.map(\.title) == ["Pane"])
    #expect(c.table.menu(for: click)?.items.map(\.title) == ["Pane"])
    #expect(host.focused == 3, "each takes the keys first")
}
