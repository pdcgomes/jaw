import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// A right click on a message opens the message's menu, which ends with the pane's splits; one on the
/// header, on the transcript's blank space (beside, between and under the messages) or on a system
/// or call line opens the pane's whole menu. A right click gives the pane the keys first, so the menu
/// acts on it; making a menu, as the snapshot's log does, leaves the keys where they are. The right
/// clicks go through AppKit's hit test in an offscreen window, then up the responders, as a right
/// mouse-down goes, to the first view with a menu for it. VoiceOver's Show Menu, which has no
/// pointer, opens a message's menu under its bubble.

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

/// A pane showing `messages` in an offscreen window, laid out and drawn.
@MainActor
func shown(_ messages: [MessageRow], _ host: PaneHost) async -> (ConversationController, NSWindow) {
    let c = pane(host)
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 480, height: 700), styleMask: [.titled], backing: .buffered,
                          defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = c.view
    c.view.layoutSubtreeIfNeeded()
    await withCheckedContinuation { done in
        c.drawn = { done.resume() }
        c.apply(Update(seq: 1, upsert: messages, total: UInt64(messages.count), older: false, newer: 0,
                       moved: Moved(place: .newest(100), newer: 0)))
    }
    c.view.layoutSubtreeIfNeeded()
    return (c, window)
}

/// The view of the message at row `i`.
@MainActor
func messageView(_ c: ConversationController, _ i: Int) throws -> MessageView {
    try #require(c.table.view(atColumn: 0, row: i, makeIfNecessary: true) as? MessageView)
}

/// The menu a right click at `point`, in the window, opens: AppKit's hit test finds the view under
/// it, and the click goes up the responders to the first view with a menu for it.
@MainActor
func rightClick(_ window: NSWindow, at point: NSPoint) throws -> NSMenu? {
    let event = try #require(NSEvent.mouseEvent(with: .rightMouseDown, location: point, modifierFlags: [], timestamp: 0,
                                                windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                                                clickCount: 1, pressure: 1))
    var responder: NSResponder? = window.contentView?.superview?.hitTest(point)
    while let r = responder {
        if let view = r as? NSView, let menu = view.menu(for: event) { return menu }
        responder = r.nextResponder
    }
    return nil
}

@MainActor
@Test func `a message's menu ends with the pane's splits, after a separator`() throws {
    let host = PaneHost()
    let c = pane(host)
    let text = try #require(Samples.messages.first { $0.kind == .text && !$0.deleted })
    let menu = try #require(c.menu(for: text))
    #expect(menu.items.first?.title == "Reply")
    #expect(menu.items.last?.title == "Split Across")
    #expect(menu.items.dropLast().last?.isSeparatorItem == true, "after a separator")
    #expect(host.focused == 0, "making it moves no focus")
}

@MainActor
@Test func `with nothing to end with, a message's menu ends with its own items, not a separator`() throws {
    let host = PaneHost()
    let c = pane(host)
    c.messageMenuTail = { [] }
    let text = try #require(Samples.messages.first { $0.kind == .text && !$0.deleted })
    let menu = try #require(c.menu(for: text))
    #expect(menu.items.last?.title == "Info")
    #expect(menu.items.last?.isSeparatorItem == false)
    #expect(host.focused == 0, "making it moves no focus")
}

@MainActor
@Test func `a right click on a message opens its menu, and one beside it or between two messages the pane's`() async throws {
    let host = PaneHost()
    let ana = try #require(Samples.messages.first { $0.id.hasSuffix("04") })
    let rui = try #require(Samples.messages.first { $0.id.hasSuffix("05") })
    let (c, window) = await shown([ana, rui], host)
    let first = try messageView(c, 0), second = try messageView(c, 1)
    let a = try #require(first.laid).body, b = try #require(second.laid).body

    let on = first.convert(NSPoint(x: a.midX, y: a.midY), to: nil)
    let menu = try rightClick(window, at: on)
    #expect(menu?.items.first?.title == "Reply", "on a message, its menu")
    #expect(menu?.items.last?.title == "Split Across")

    try #require(a.maxX < first.bounds.maxX - 40, "blank space beside the message")
    let beside = first.convert(NSPoint(x: (a.maxX + first.bounds.maxX) / 2, y: a.midY), to: nil)
    #expect(try rightClick(window, at: beside)?.items.map(\.title) == ["Pane"], "beside it, the pane's")

    let x = min(a.midX, b.midX)
    let above = first.convert(NSPoint(x: x, y: a.maxY), to: nil), below = second.convert(NSPoint(x: x, y: b.minY), to: nil)
    try #require(above.y - below.y > 4, "a gap between the two")
    let between = NSPoint(x: x, y: (above.y + below.y) / 2)
    #expect(try rightClick(window, at: between)?.items.map(\.title) == ["Pane"], "between two messages, the pane's")
    #expect(host.focused == 3, "each right click gives the pane the keys, once")
}

@MainActor
@Test func `a right click on the buttons a hovered message shows opens the message's menu`() async throws {
    let host = PaneHost()
    let ana = try #require(Samples.messages.first { $0.id.hasSuffix("04") })
    let (c, window) = await shown([ana], host)
    let view = try messageView(c, 0)
    c.hovered(view, true)
    try #require(c.hover.superview === view)
    let bar = c.hover.frame
    let menu = try rightClick(window, at: view.convert(NSPoint(x: bar.midX, y: bar.midY), to: nil))
    #expect(menu?.items.first?.title == "Reply")
    #expect(host.focused == 1)
}

@MainActor
@Test func `VoiceOver's Show Menu opens a short message's menu under its bubble, and with no menu says so`() async throws {
    let host = PaneHost()
    let mine = try #require(Samples.messages.first { $0.id.hasSuffix("14") })
    let call = try #require(Samples.messages.first { $0.kind == .call })
    let (c, _) = await shown([call, mine], host)
    c.paneMenu = { nil }
    let view = try messageView(c, #require(c.rows.firstIndex { $0.id == mine.id }))
    let body = try #require(view.laid).body
    try #require(!body.contains(NSPoint(x: view.bounds.midX, y: view.bounds.midY)), "a bubble clear of its row's middle")
    var popped: [(NSMenu, NSPoint)] = []
    view.popUp = { menu, point, _ in popped.append((menu, point)) }
    #expect(view.accessibilityPerformShowMenu())
    #expect(popped.first?.0.items.first?.title == "Reply", "the message's menu")
    #expect(popped.first?.1 == NSPoint(x: body.minX, y: body.maxY), "under its bubble")
    #expect(host.focused == 1, "the pane takes the keys first, as for a right click")

    let line = try messageView(c, #require(c.rows.firstIndex { $0.id == call.id }))
    line.popUp = { menu, point, _ in popped.append((menu, point)) }
    #expect(!line.accessibilityPerformShowMenu(), "a call line has the pane's menu, and here there's none")
    #expect(popped.count == 1)
}

@MainActor
@Test func `a right click under the last message, on a call line or on the header opens the pane's menu`() async throws {
    let host = PaneHost()
    let rui = try #require(Samples.messages.first { $0.id.hasSuffix("05") })
    let call = try #require(Samples.messages.first { $0.kind == .call })
    let (c, window) = await shown([rui, call], host)
    let line = try messageView(c, 1)
    let pill = try #require(line.laid).body
    let onLine = line.convert(NSPoint(x: pill.midX, y: pill.midY), to: nil)
    #expect(try rightClick(window, at: onLine)?.items.map(\.title) == ["Pane"], "on a call line")

    let end = line.convert(NSPoint(x: pill.midX, y: line.bounds.maxY), to: nil)
    try #require(end.y > 100, "room under the last message")
    #expect(try rightClick(window, at: NSPoint(x: end.x, y: 40))?.items.map(\.title) == ["Pane"], "under the last message")

    let name = c.header.name
    let onName = name.convert(NSPoint(x: 4, y: name.bounds.midY), to: nil)
    #expect(try rightClick(window, at: onName)?.items.map(\.title) == ["Pane"], "on the header")
    #expect(host.focused == 3, "each right click gives the pane the keys, once")
}

@MainActor
@Test func `making a menu, as the snapshot's log does, moves no focus, and off the rows there's none`() throws {
    let host = PaneHost()
    let c = pane(host)
    c.apply(Samples.update)
    let text = try #require(c.rows.firstIndex { $0.kind == .text && !$0.deleted })
    let system = try #require(c.rows.firstIndex { $0.kind == .system })
    let call = try #require(c.rows.firstIndex { $0.kind == .call })
    #expect(c.menu(forRow: text)?.items.first?.title == "Reply")
    #expect(c.menu(forRow: system)?.items.map(\.title) == ["Pane"], "a system line's is the pane's")
    #expect(c.menu(forRow: call)?.items.map(\.title) == ["Pane"], "a call line's too")
    #expect(c.menu(forRow: -1) == nil)
    #expect(c.menu(forRow: c.rows.count) == nil)
    #expect(host.focused == 0, "making a menu moves no focus")
}
