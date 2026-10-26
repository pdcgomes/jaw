import AppKit
import JawKit
import JawThemes
import JawUI
import QuartzCore
import SwiftUI

/// The window: the sidebar, the chat list and the conversation on macOS 26's split view, the
/// status line along the bottom of the whole window. Below 960 points the sidebar folds away, and
/// below 740 the chat list does too, as the terminal folds.
@MainActor
final class MainWindowController: NSWindowController, NSWindowDelegate, NSToolbarDelegate {
    let model: AppModel
    let split = NSSplitViewController()
    let sidebar = SidebarController()
    let chatList: ChatListController
    let detail = DetailController()
    let palette = PalettePanel()
    var status: NSHostingView<StatusLine>!
    var sidebarItem: NSSplitViewItem!
    var listItem: NSSplitViewItem!
    var displayLink: CADisplayLink?
    var settings: NSWindow?

    init(model: AppModel) {
        self.model = model
        chatList = ChatListController(folder: model.folder, roles: model.roles)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1180, height: 780),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.title = "jaw"
        window.minSize = NSSize(width: 520, height: 420)
        window.setFrameAutosaveName("jaw")
        window.titlebarAppearsTransparent = true
        window.toolbarStyle = .unified
        super.init(window: window)
        window.delegate = self

        sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebar)
        sidebarItem.minimumThickness = 180
        sidebarItem.maximumThickness = 280
        sidebarItem.canCollapseFromWindowResize = true
        listItem = NSSplitViewItem(contentListWithViewController: chatList)
        listItem.minimumThickness = 260
        listItem.maximumThickness = 420
        let detailItem = NSSplitViewItem(viewController: detail)
        detailItem.minimumThickness = 360
        split.addSplitViewItem(sidebarItem)
        split.addSplitViewItem(listItem)
        split.addSplitViewItem(detailItem)
        split.splitView.autosaveName = "jaw.split"

        let root = NSViewController()
        let content = NSView()
        root.view = content
        status = NSHostingView(rootView: StatusLine(state: nil, warn: false, chat: nil, roles: model.roles))
        root.addChild(split)
        for v in [split.view, status!] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview(v)
        }
        NSLayoutConstraint.activate([
            split.view.topAnchor.constraint(equalTo: content.topAnchor),
            split.view.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            split.view.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            split.view.bottomAnchor.constraint(equalTo: status.topAnchor),
            status.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            status.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            status.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            status.heightAnchor.constraint(equalToConstant: 24),
        ])
        window.contentViewController = root
        if window.frame.origin == .zero { window.center() }

        let toolbar = NSToolbar(identifier: "jaw")
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        window.toolbar = toolbar

        sidebar.picked = { [weak model] folder in model?.pick(folder: folder.id) }
        chatList.host = model
        palette.model.find = { [weak model] id, query in model?.jawd.find(id: id, query: query, limit: 10) }
        palette.model.picked = { [weak self] pick in self?.picked(pick) }

        let link = content.displayLink(target: self, selector: #selector(frame(_:)))
        link.isPaused = true
        link.add(to: .main, forMode: .common)
        displayLink = link
        restyle()
        refresh()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Frames

    /// jawd has pushes waiting: drain them at the next display frame.
    func wantFrame() {
        displayLink?.isPaused = false
    }

    @objc func frame(_: CADisplayLink) {
        displayLink?.isPaused = true
        model.drain()
    }

    // MARK: - What it shows

    func refresh() {
        let state = model.stateLine
        status.rootView = StatusLine(state: state?.0, warn: state?.1 ?? false, chat: model.chatLine, roles: model.roles)
        if let kind = model.stateView {
            detail.show(state: kind, roles: model.roles)
        } else if let c = model.conversation {
            detail.show(conversation: c)
        }
    }

    func show(conversation c: ConversationController) {
        detail.show(conversation: c)
        refresh()
        c.focusBox()
    }

    func restyle() {
        window?.appearance = model.preferences.nsAppearance
        chatList.roles = model.roles
        window?.backgroundColor = model.roles.ground.nsColor
        refresh()
    }

    func windowDidResize(_: Notification) {
        guard let width = window?.frame.width else { return }
        sidebarItem.animator().isCollapsed = width < 960
        listItem.animator().isCollapsed = width < 740 && model.conversation != nil
    }

    func windowDidBecomeKey(_: Notification) {
        model.updateFocus()
    }

    func windowDidResignKey(_: Notification) {
        model.updateFocus()
    }

    func windowDidChangeOcclusionState(_: Notification) {
        model.updateFocus()
    }

    func windowDidMiniaturize(_: Notification) {
        model.updateFocus()
    }

    func windowDidDeminiaturize(_: Notification) {
        model.updateFocus()
    }

    func windowDidChangeEffectiveAppearance() {
        model.restyle()
    }

    // MARK: - The palette

    @objc func showPalette(_: Any?) {
        guard let window else { return }
        palette.show(over: window, roles: model.roles)
    }

    func picked(_ pick: PaletteModel.Pick) {
        palette.close()
        switch pick {
        case let .chat(row): model.open(row, opened: true)
        case let .person(p): model.jawd.startChat(person: p.id)
        case let .number(n): model.jawd.startChat(phone: n)
        case let .message(m):
            if let chat = chatList.row(m.chat) {
                model.open(chat, opened: true, around: m.id)
            } else {
                model.say("that chat isn't in the list yet")
            }
        case let .folder(f):
            if SidebarController.folders.contains(where: { $0.id == f.id }) { model.pick(folder: f.id) } else {
                model.say("folders come with jawd's next step")
            }
        }
    }

    // MARK: - Sheets

    var infoSheet: NSWindow?

    func showInfo(info: MessageInfo?, why: String?, message: MessageRow?) {
        let view = InfoView(info: info, why: why, mine: message?.mine ?? true, voice: message?.kind == .voice) { [weak self] in
            guard let self, let sheet = infoSheet else { return }
            window?.endSheet(sheet)
            infoSheet = nil
        }
        if let sheet = infoSheet {
            (sheet.contentViewController as? NSHostingController<InfoView>)?.rootView = view
            return
        }
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: view))
        infoSheet = sheet
        window?.beginSheet(sheet)
    }

    var statusWindow: NSWindow?

    func showStatus(_ status: Status) {
        let text = (try? String(data: JSONEncoder.pretty.encode(status.raw), encoding: .utf8)) ?? ""
        let view = ScrollView { Text(text).font(.system(size: 11, design: .monospaced)).textSelection(.enabled).padding() }
            .frame(minWidth: 520, minHeight: 420)
        let w = statusWindow ?? NSWindow(contentViewController: NSHostingController(rootView: AnyView(view)))
        (w.contentViewController as? NSHostingController<AnyView>)?.rootView = AnyView(view)
        w.title = "jawd's status"
        statusWindow = w
        w.makeKeyAndOrderFront(nil)
    }

    func showSettings() {
        let w = settings ?? NSWindow(contentViewController: NSHostingController(rootView: SettingsView(preferences: model.preferences)))
        w.title = "Settings"
        w.styleMask.remove(.resizable)
        settings = w
        w.center()
        w.makeKeyAndOrderFront(nil)
    }

    // MARK: - The toolbar

    static let find = NSToolbarItem.Identifier("find")
    static let newChat = NSToolbarItem.Identifier("newChat")

    func toolbarDefaultItemIdentifiers(_: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.toggleSidebar, .sidebarTrackingSeparator, .flexibleSpace, Self.find, Self.newChat]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(_: NSToolbar, itemForItemIdentifier id: NSToolbarItem.Identifier, willBeInsertedIntoToolbar _: Bool) -> NSToolbarItem? {
        switch id {
        case Self.find:
            let item = NSToolbarItem(itemIdentifier: id)
            let button = NSButton(title: "Find  ⌘K", image: NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: "Find")!,
                                  target: self, action: #selector(showPalette(_:)))
            button.imagePosition = .imageLeading
            button.bezelStyle = .toolbar
            item.view = button
            item.label = "Find"
            return item
        case Self.newChat:
            let item = NSToolbarItem(itemIdentifier: id)
            item.image = NSImage(systemSymbolName: "square.and.pencil", accessibilityDescription: "New Chat")
            item.label = "New Chat"
            item.target = self
            item.action = #selector(showPalette(_:))
            item.toolTip = "Start a chat: a name or a number (⌘N)"
            return item
        default:
            return nil
        }
    }
}

extension JSONEncoder {
    static var pretty: JSONEncoder {
        let e = JSONEncoder()
        e.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return e
    }
}

/// The conversation's place: a chat, or what's there instead.
@MainActor
final class DetailController: NSViewController {
    var current: NSViewController?
    var stateHost: NSHostingController<StateView>?

    override func loadView() {
        view = NSView()
    }

    func show(conversation c: ConversationController) {
        guard current !== c else { return }
        place(c)
    }

    func show(state kind: StateView.Kind, roles: Roles) {
        let view = StateView(kind: kind, roles: roles)
        if let host = stateHost, current === host {
            host.rootView = view
            return
        }
        let host = NSHostingController(rootView: view)
        host.sizingOptions = []
        stateHost = host
        place(host)
    }

    func place(_ vc: NSViewController) {
        current?.view.removeFromSuperview()
        current?.removeFromParent()
        addChild(vc)
        vc.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(vc.view)
        NSLayoutConstraint.activate([
            vc.view.topAnchor.constraint(equalTo: view.topAnchor),
            vc.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            vc.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            vc.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ])
        current = vc
    }
}
