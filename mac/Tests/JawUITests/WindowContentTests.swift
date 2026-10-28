import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The window's content folds by its own width, as the window's resize asks: the sidebar below 960
/// points, the chat list too below 740 while a chat is open; focus mode folds both and puts them back.
/// A still screen is laid out folded at once, as the content is under Reduce Motion, and the chats that
/// need you leave out those a pane shows.

@MainActor
func content(width: CGFloat, laidOut: Bool = true) -> WindowContent {
    let c = WindowContent(sidebar: SidebarController(), chatList: ChatListController(folder: "all", roles: Catalog.jaw.dark),
                          detail: DetailController(), bar: BottomBar(commands: { [] }))
    c.view.frame = NSRect(x: 0, y: 0, width: width, height: 700)
    if laidOut { c.view.layoutSubtreeIfNeeded() }
    return c
}

@MainActor @Test func `the window's content folds by its own width, and focus mode folds both and puts them back`() {
    let wide = content(width: 1180)
    wide.fold(chatOpen: true, animated: false)
    #expect(!wide.sidebarItem.isCollapsed && !wide.listItem.isCollapsed)
    let mid = content(width: 940)
    mid.fold(chatOpen: true, animated: false)
    #expect(mid.sidebarItem.isCollapsed && !mid.listItem.isCollapsed)
    let narrow = content(width: 720)
    narrow.fold(chatOpen: false, animated: false)
    #expect(narrow.sidebarItem.isCollapsed && !narrow.listItem.isCollapsed, "the list stays while no chat is open")
    narrow.fold(chatOpen: true, animated: false)
    #expect(narrow.sidebarItem.isCollapsed && narrow.listItem.isCollapsed)
    wide.focusMode(true, animated: false)
    #expect(wide.inFocusMode && wide.sidebarItem.isCollapsed && wide.listItem.isCollapsed)
    wide.fold(chatOpen: true, animated: false)
    #expect(wide.sidebarItem.isCollapsed && wide.listItem.isCollapsed, "focus mode keeps them folded whatever the width")
    wide.focusMode(false, animated: false)
    #expect(!wide.inFocusMode && !wide.sidebarItem.isCollapsed && !wide.listItem.isCollapsed, "and puts them back as they were")
}

@MainActor @Test func `at 960 points the sidebar stays and at 959 it folds, and with a chat open at 740 the chat list stays and at 739 it folds`() {
    let edges: [(CGFloat, Bool, Bool)] = [(960, false, false), (959, true, false), (740, true, false), (739, true, true)]
    for (width, sidebar, list) in edges {
        let c = content(width: width)
        c.fold(chatOpen: true, animated: false)
        #expect(c.sidebarItem.isCollapsed == sidebar && c.listItem.isCollapsed == list, "at \(width) points")
    }
}

@MainActor @Test func `focus mode puts back what was folded before it, the sidebar folded at 940 and the chat list out`() {
    let c = content(width: 940)
    c.fold(chatOpen: true, animated: false)
    c.focusMode(true, animated: false)
    #expect(c.inFocusMode && c.sidebarItem.isCollapsed && c.listItem.isCollapsed)
    c.focusMode(false, animated: false)
    #expect(!c.inFocusMode && c.sidebarItem.isCollapsed && !c.listItem.isCollapsed)
}

@MainActor @Test func `a still screen is laid out folded at once, the panes' place taking the width`() {
    let before = Motion.reduced
    defer { Motion.reduced = before }
    Motion.reduced = { false }
    let c = content(width: 720)
    c.fold(chatOpen: true, animated: false)
    c.view.layoutSubtreeIfNeeded()
    #expect(c.sidebar.view.isHiddenOrHasHiddenAncestor && c.chatList.view.isHiddenOrHasHiddenAncestor, "AppKit hides a folded item's view")
    #expect(c.detail.view.frame.width == 720)
}

@MainActor @Test func `a still screen in focus mode is laid out folded at once, folded before the content's first layout`() {
    let before = Motion.reduced
    defer { Motion.reduced = before }
    Motion.reduced = { false }
    let c = content(width: 1180, laidOut: false)
    c.focusMode(true, animated: false)
    c.view.layoutSubtreeIfNeeded()
    #expect(c.sidebar.view.isHiddenOrHasHiddenAncestor && c.chatList.view.isHiddenOrHasHiddenAncestor, "AppKit hides a folded item's view")
    #expect(c.detail.view.frame.width == 1180)
}

@MainActor @Test func `under Reduce Motion the content is laid out folded at once too`() {
    let before = Motion.reduced
    defer { Motion.reduced = before }
    Motion.reduced = { true }
    let c = content(width: 720)
    c.fold(chatOpen: true)
    c.view.layoutSubtreeIfNeeded()
    #expect(c.sidebar.view.isHiddenOrHasHiddenAncestor && c.chatList.view.isHiddenOrHasHiddenAncestor, "AppKit hides a folded item's view")
    #expect(c.detail.view.frame.width == 720)
}

@MainActor @Test func `the chats that need you leave out those a pane shows, but not one whose pane has no view yet`() {
    let shown = ChatRow(id: "ana", v: 1, sort: "2", name: "Ana", unread: 2)
    let waiting = ChatRow(id: "rui", v: 1, sort: "1", name: "Rui", unread: 1)
    let grid = PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: shown.id),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: waiting.id),
    ])
    let c = content(width: 1180)
    c.show(Situation(reach: .up, connection: Connection(state: .online)), chats: [shown, waiting], focused: waiting,
           panes: WindowContent.Panes(grid: grid, views: [1: NSView()], focused: 2, box: nil, look: PaneLook()), roles: Catalog.jaw.dark,
           now: Date(), actions: WindowContent.Actions())
    #expect(c.bar.status.rootView.needs.map(\.id) == [waiting.id])
}
