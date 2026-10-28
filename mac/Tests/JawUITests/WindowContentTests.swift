import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The window's content folds by its own width, as the window's resize asks: the sidebar below 960
/// points, the chat list too below 740 while a chat is open; focus mode folds both and puts them back.

@MainActor
func content(width: CGFloat) -> WindowContent {
    let c = WindowContent(sidebar: SidebarController(), chatList: ChatListController(folder: "all", roles: Catalog.jaw.dark),
                          detail: DetailController(), bar: BottomBar(commands: { [] }))
    c.view.frame = NSRect(x: 0, y: 0, width: width, height: 700)
    c.view.layoutSubtreeIfNeeded()
    return c
}

@MainActor @Test func `the window's content folds by its own width, and focus mode folds both and puts them back`() {
    let before = Motion.reduced
    defer { Motion.reduced = before }
    Motion.reduced = { true }
    let wide = content(width: 1180)
    wide.fold(chatOpen: true)
    #expect(!wide.sidebarItem.isCollapsed && !wide.listItem.isCollapsed)
    let mid = content(width: 940)
    mid.fold(chatOpen: true)
    #expect(mid.sidebarItem.isCollapsed && !mid.listItem.isCollapsed)
    let narrow = content(width: 720)
    narrow.fold(chatOpen: false)
    #expect(narrow.sidebarItem.isCollapsed && !narrow.listItem.isCollapsed, "the list stays while no chat is open")
    narrow.fold(chatOpen: true)
    #expect(narrow.sidebarItem.isCollapsed && narrow.listItem.isCollapsed)
    wide.focusMode(true)
    #expect(wide.inFocusMode && wide.sidebarItem.isCollapsed && wide.listItem.isCollapsed)
    wide.fold(chatOpen: true)
    #expect(wide.sidebarItem.isCollapsed, "focus mode keeps them folded whatever the width")
    wide.focusMode(false)
    #expect(!wide.inFocusMode && !wide.sidebarItem.isCollapsed && !wide.listItem.isCollapsed, "and puts them back as they were")
}
