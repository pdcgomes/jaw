import AppKit
import JawKit
import JawThemes
import SwiftUI

/// The window's content: the sidebar, the chat list and the panes' place on macOS 26's split view,
/// with the bottom bar along the bottom of the whole window. It folds by its own width, as the
/// terminal folds: below 960 points the sidebar, and below 740, while a chat is open, the chat list
/// too; focus mode folds both for the focused pane and puts them back as they were.
@MainActor
public final class WindowContent: NSViewController {
    public let sidebar: SidebarController
    public let chatList: ChatListController
    public let detail: DetailController
    public let bar: BottomBar
    private let split = NSSplitViewController()
    let sidebarItem: NSSplitViewItem
    let listItem: NSSplitViewItem
    /// The sidebar's and the chat list's folding before focus mode, to put back; nil out of it.
    private var beforeFocus: (Bool, Bool)?

    /// Below this width the sidebar folds away.
    public static let sidebarFold: CGFloat = 960
    /// Below this width, while a chat is open, the chat list folds too.
    public static let listFold: CGFloat = 740

    /// The panes as the window shows them: the layout's grid, each slot's view (none yet for a chat the
    /// list doesn't hold), the slot with the keys, the focused chat's box under them, and how the panes
    /// say which has the keys.
    public struct Panes {
        public var grid: PaneGrid
        public var views: [Int: NSView]
        public var focused: Int?
        public var box: NSView?
        public var look: PaneLook

        public init(grid: PaneGrid, views: [Int: NSView], focused: Int?, box: NSView?, look: PaneLook) {
            self.grid = grid
            self.views = views
            self.focused = focused
            self.box = box
            self.look = look
        }
    }

    /// What the content's parts do when they're used: open a chat, open the glance, start or cancel linking.
    public struct Actions {
        public var open: (ChatRow) -> Void
        public var glance: () -> Void
        public var linkStart: (String?) -> Void
        public var linkCancel: () -> Void

        public init(open: @escaping (ChatRow) -> Void = { _ in }, glance: @escaping () -> Void = {},
                    linkStart: @escaping (String?) -> Void = { _ in }, linkCancel: @escaping () -> Void = {})
        {
            self.open = open
            self.glance = glance
            self.linkStart = linkStart
            self.linkCancel = linkCancel
        }
    }

    /// The content from its parts, the split keeping its dividers under `autosaveName` when it has one.
    public init(sidebar: SidebarController, chatList: ChatListController, detail: DetailController, bar: BottomBar,
                autosaveName: String? = nil)
    {
        self.sidebar = sidebar
        self.chatList = chatList
        self.detail = detail
        self.bar = bar
        sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebar)
        sidebarItem.minimumThickness = 180
        sidebarItem.maximumThickness = 280
        sidebarItem.canCollapseFromWindowResize = true
        listItem = NSSplitViewItem(contentListWithViewController: chatList)
        listItem.minimumThickness = 260
        listItem.maximumThickness = 420
        super.init(nibName: nil, bundle: nil)
        let detailItem = NSSplitViewItem(viewController: detail)
        detailItem.minimumThickness = 360
        split.addSplitViewItem(sidebarItem)
        split.addSplitViewItem(listItem)
        split.addSplitViewItem(detailItem)
        split.splitView.autosaveName = autosaveName
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        let content = NSView()
        view = content
        addChild(split)
        for v in [split.view, bar] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview(v)
        }
        NSLayoutConstraint.activate([
            split.view.topAnchor.constraint(equalTo: content.topAnchor),
            split.view.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            split.view.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            split.view.bottomAnchor.constraint(equalTo: bar.topAnchor),
            bar.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            bar.bottomAnchor.constraint(equalTo: content.bottomAnchor),
        ])
    }

    /// Whether focus mode has the sidebar and the chat list folded.
    public var inFocusMode: Bool {
        beforeFocus != nil
    }

    /// Folds the sidebar and the chat list for the content's own width, as the window asks when it's
    /// resized, unless focus mode has them folded. It folds at once under Reduce Motion, and a still
    /// screen passes `animated: false`, so it's drawn folded.
    public func fold(chatOpen: Bool, animated: Bool = true) {
        guard !inFocusMode else { return }
        let width = view.bounds.width
        collapse(sidebarItem, width < Self.sidebarFold, animated: animated)
        collapse(listItem, width < Self.listFold && chatOpen, animated: animated)
    }

    /// Focus mode: the sidebar and the chat list fold away for the focused pane, and come back as they
    /// were. It folds at once under Reduce Motion, and a still screen passes `animated: false`, so it's
    /// drawn folded.
    public func focusMode(_ on: Bool, animated: Bool = true) {
        if on {
            guard !inFocusMode else { return }
            beforeFocus = (sidebarItem.isCollapsed, listItem.isCollapsed)
            collapse(sidebarItem, true, animated: animated)
            collapse(listItem, true, animated: animated)
        } else {
            let was = beforeFocus
            beforeFocus = nil
            collapse(sidebarItem, was?.0 ?? false, animated: animated)
            collapse(listItem, was?.1 ?? false, animated: animated)
        }
    }

    public func toggleSidebar() {
        collapse(sidebarItem, !sidebarItem.isCollapsed, animated: true)
    }

    public func toggleChatList() {
        collapse(listItem, !listItem.isCollapsed, animated: true)
    }

    private func collapse(_ item: NSSplitViewItem, _ collapsed: Bool, animated: Bool) {
        (animated && !Motion.reduced() ? item.animator() : item).isCollapsed = collapsed
    }

    /// Shows what jawd's situation says: the status line, from the situation, the chats and the focused
    /// chat, the chats that need you leaving out those a pane shows; and in the panes' place the state
    /// it calls for, linking, or `panes`.
    public func show(_ situation: Situation, chats: [ChatRow], focused: ChatRow?, panes: Panes?, roles: Roles, now: Date,
                     actions: Actions)
    {
        let state = situation.stateLine
        let shown = Set(panes.map { p in p.grid.slots.filter { p.views[$0.id] != nil }.compactMap(\.chat) } ?? [])
        bar.status.rootView = StatusLine(state: state?.text, warn: state?.warn ?? false,
                                         chat: situation.chatLine(focused: focused, now: now),
                                         needs: Situation.needs(chats, shown: shown), roles: roles, open: actions.open,
                                         glance: actions.glance)
        switch situation.stateView(hasChats: !chats.isEmpty, hasPanes: panes != nil) {
        case let .linking(step, loggedOut)?:
            detail.showLinking(step: step, loggedOut: loggedOut, roles: roles, start: actions.linkStart, cancel: actions.linkCancel)
        case let kind?:
            detail.show(state: kind, roles: roles)
        case nil:
            if let panes { show(panes, roles: roles) }
        }
    }

    /// Lays the panes out in the panes' place, with the focused chat's box under them.
    public func show(_ panes: Panes, roles: Roles) {
        detail.show(panes: panes.grid, views: panes.views, focused: panes.focused, box: panes.box, roles: roles, look: panes.look)
    }
}
