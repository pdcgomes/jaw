import AppKit
@testable import JawUI
import Testing

/// A conversation's header under the window's buttons, as a pane at the window's top left is with the
/// sidebar and the chat list folded away: it starts after them, and elsewhere keeps its usual inset.

/// The group's conversation, 640 by 480 points and in no window, laid out twice, as a window lays it
/// out: the header's inset moves after the pass that finds the buttons.
@MainActor
func paneLaidOut(windowButtons: (() -> NSRect?)? = nil) -> ConversationController {
    let c = ConversationController(chat: Samples.group, spec: spec(.bubbles, dark: true), layouts: Layouts(), place: .newest(100))
    if let windowButtons { c.windowButtons = windowButtons }
    c.view.frame = NSRect(x: 0, y: 0, width: 640, height: 480)
    c.view.layoutSubtreeIfNeeded()
    c.view.layoutSubtreeIfNeeded()
    return c
}

@MainActor @Test func `a header under the window's buttons starts after them, and with none given and no window keeps its inset`() {
    let zoom = NSRect(x: 55, y: 480 - 9 - 14, width: 14, height: 14)
    let under = paneLaidOut(windowButtons: { zoom })
    #expect(under.header.avatar.frame.minX == zoom.maxX + Space.xs + Space.m, "the zoom button's right edge, the buttons' margin and the header's own")
    let alone = paneLaidOut()
    #expect(alone.header.avatar.frame.minX == HeaderView.inset)
}
