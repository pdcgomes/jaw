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

    /// The app's title bar: how tall it is, and where its buttons are from the window's top left, the
    /// window's three and the sidebar's beside them.
    struct Bar {
        let height: CGFloat
        let buttons: [(kind: NSWindow.ButtonType, frame: NSRect)]
        let sidebar: NSRect

        /// `frame`, from the top left of a view `height` points tall, in that view's own coordinates,
        /// whose y runs up as a window's content view's does; and back, as the flip undoes itself.
        static func flip(_ frame: NSRect, in height: CGFloat) -> NSRect {
            NSRect(x: frame.minX, y: height - frame.maxY, width: frame.width, height: frame.height)
        }

        /// Where the window puts `kind`'s button, in the coordinates of `content`, the window's content.
        @MainActor
        func frame(of kind: NSWindow.ButtonType, in content: NSView) -> NSRect? {
            buttons.first { $0.kind == kind }.map { Self.flip($0.frame, in: content.bounds.height) }
        }
    }

    /// The title bar as a window made with `TitleBar.style` and dressed by `TitleBar.dress` has it,
    /// measured once from one that's never shown, with its frame view laid out, which places the
    /// sidebar's button.
    @MainActor static let titleBar: Bar = {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1180, height: 780), styleMask: TitleBar.style, backing: .buffered,
                              defer: false)
        let sidebar = TitleBar.dress(window) {}
        let content = window.contentView!
        content.superview?.layoutSubtreeIfNeeded()
        @MainActor func measured(_ view: NSView) -> NSRect {
            Bar.flip(content.convert(view.frame, from: view.superview), in: content.bounds.height)
        }
        let buttons = [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton].compactMap { kind in
            window.standardWindowButton(kind).map { (kind: kind, frame: measured($0)) }
        }
        return Bar(height: window.frame.height - window.contentLayoutRect.height, buttons: buttons, sidebar: measured(sidebar))
    }()

    /// The open chats side by side on a grid of one row, each in a slot numbered from 1.
    var grid: PaneGrid? {
        guard !open.isEmpty else { return nil }
        let n = Double(open.count)
        return PaneGrid(cols: (0 ... open.count).map { Double($0) / n }, rows: [0, 1],
                        slots: open.enumerated().map { PaneGrid.Slot(id: $0.offset + 1, c0: $0.offset, c1: $0.offset + 1, r0: 0, r1: 1, chat: $0.element.id) })
    }

    /// The window's content made from this screen in `conditions`: the sidebar; the chat list holding
    /// `chats`, the first open one selected; a conversation for each open chat, `Samples`' messages in
    /// the group's, kept alive as the content's children; and the bar, with the layout's name. Its safe
    /// area leaves the title bar's height at the top, as the app's window leaves it, and each
    /// conversation's header knows where the window's buttons are. It's laid out at its size once it's
    /// folded, so each conversation has its pane's width before its rows come on the next turn, as in
    /// the app, whose window lays the content out before jawd's rows arrive.
    @MainActor
    func content(_ conditions: Conditions) -> WindowContent {
        let roles = conditions.roles
        let list = ChatListController(folder: "all", roles: roles)
        let content = WindowContent(sidebar: SidebarController(), chatList: list, detail: DetailController(), bar: BottomBar(commands: { [] }))
        content.view.frame = NSRect(x: 0, y: 0, width: width, height: height)
        content.view.additionalSafeAreaInsets.top = Self.titleBar.height
        content.view.wantsLayer = true
        content.view.layer?.backgroundColor = roles.ground.cgColor
        list.apply(Update(seq: 1, upsert: chats, total: UInt64(chats.count)))
        let spec = LayoutSpec(style: conditions.style, me: ["person:1"])
        let layouts = Layouts()
        var views: [Int: NSView] = [:]
        var box: NSView?
        for (i, chat) in open.enumerated() {
            let c = ConversationController(chat: chat, spec: spec, layouts: layouts, place: .newest(100))
            c.windowButtons = { [weak view = content.view] in
                guard let view, let zoom = Self.titleBar.frame(of: .zoomButton, in: view) else { return nil }
                return view.convert(zoom, to: nil)
            }
            content.addChild(c)
            _ = c.view
            let rows = chat.id == Samples.group.id ? Samples.update
                : Update(seq: 1, upsert: [], total: 0, older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0))
            if chat.id == Samples.group.id { c.members = Fixtures.members }
            Pictures.apply(rows, to: c)
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
