import AppKit
import JawKit
import JawThemes
import JawUI

/// A window as values: jawd's situation, the chats in the list, the chats open in panes (the first
/// with the keys), the layout's name, focus mode and the window's size. The harness makes a
/// `WindowContent` from one as the app makes one from jawd's windows, and shows it as of `Samples.now`.
struct Screen {
    var situation = Situation(reach: .up, linked: true, connection: Connection(state: .online))
    var chats: [ChatRow] = Samples.chats
    var open: [ChatRow] = []
    var layout = "Unsaved Panes"
    var focusMode = false
    var width: CGFloat = 1180
    var height: CGFloat = 780

    /// The open chats side by side on a grid of one row, each in a slot numbered from 1.
    var grid: PaneGrid? {
        guard !open.isEmpty else { return nil }
        let n = Double(open.count)
        return PaneGrid(cols: (0 ... open.count).map { Double($0) / n }, rows: [0, 1],
                        slots: open.enumerated().map { PaneGrid.Slot(id: $0.offset + 1, c0: $0.offset, c1: $0.offset + 1, r0: 0, r1: 1, chat: $0.element.id) })
    }

    /// The window's content made from this screen in `conditions`: the sidebar; the chat list holding
    /// `chats`, the first open one selected; a conversation for each open chat, `Samples`' messages in
    /// the group's, kept alive as the content's children; and the bar, with the layout's name. It's
    /// laid out at its size once it's folded, so each conversation has its pane's width when its rows
    /// come on the next turn, which can be before the window lays the content out: rows that come
    /// before the width are drawn in a different place from one picture to the next.
    @MainActor
    func content(_ conditions: Conditions) -> WindowContent {
        let roles = conditions.roles
        let list = ChatListController(folder: "all", roles: roles)
        let content = WindowContent(sidebar: SidebarController(), chatList: list, detail: DetailController(), bar: BottomBar(commands: { [] }))
        content.view.frame = NSRect(x: 0, y: 0, width: width, height: height)
        content.view.wantsLayer = true
        content.view.layer?.backgroundColor = roles.ground.cgColor
        list.apply(Update(seq: 1, upsert: chats, total: UInt64(chats.count)))
        let spec = LayoutSpec(style: conditions.style, me: ["person:1"])
        let layouts = Layouts()
        var views: [Int: NSView] = [:]
        var box: NSView?
        for (i, chat) in open.enumerated() {
            let c = ConversationController(chat: chat, spec: spec, layouts: layouts, place: .newest(100))
            content.addChild(c)
            _ = c.view
            let rows = chat.id == Samples.group.id ? Samples.update
                : Update(seq: 1, upsert: [], total: 0, older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0))
            if chat.id == Samples.group.id { c.members = Fixtures.members }
            DispatchQueue.main.async { c.apply(rows) }
            views[i + 1] = c.view
            if i == 0 {
                box = c.composer.view
                list.select(chat.id)
            }
        }
        content.bar.restyle(roles)
        content.bar.show(layout: layout, edited: false)
        content.show(situation, chats: chats, focused: open.first,
                     panes: grid.map { WindowContent.Panes(grid: $0, views: views, focused: 1, box: box, look: PaneLook(outline: roles.accent)) },
                     roles: roles, now: Date(timeIntervalSince1970: Double(Samples.now) / 1000), actions: WindowContent.Actions())
        content.fold(chatOpen: !open.isEmpty, animated: false)
        if focusMode { content.focusMode(true, animated: false) }
        content.view.layoutSubtreeIfNeeded()
        return content
    }
}
