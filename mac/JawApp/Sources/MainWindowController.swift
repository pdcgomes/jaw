import AppKit
import JawKit
import JawThemes
import JawUI
import QuartzCore
import SwiftUI

/// The window: the sidebar, the chat list and the panes on macOS 26's split view, the status line
/// along the bottom of the whole window. Below 960 points the sidebar folds away, and below 740 the
/// chat list does too, as the terminal folds.
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
    /// The sidebar's and the chat list's folding before focus mode, to put back.
    var beforeFocus: (Bool, Bool)?

    init(model: AppModel) {
        self.model = model
        chatList = ChatListController(folder: model.folder, roles: model.roles)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1180, height: 780),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.title = "Jaw"
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
        palette.model.find = { [weak model] id, query, kinds, chat, limit in
            model?.jawd.find(id: id, query: query, kinds: kinds, chat: chat, limit: limit)
        }
        palette.model.chose = { [weak self] pick in self?.picked(pick) }
        palette.model.actions = { [weak model] in model?.actions() ?? [] }
        palette.goTo = { [weak self] m in
            self?.palette.close()
            self?.goTo(m)
        }
        detail.panes.moved = { [weak model] line, position, done in model?.moved(line, to: position, done: done) }
        detail.panes.close = { [weak model] slot in model?.close(slot: slot) }
        detail.panes.pick = { [weak model] slot in model?.focus(slot: slot) }
        detail.panes.paneMenu = { [weak model] slot in
            guard let model else { return nil }
            model.focus(slot: slot)
            return Menus.menu(["split.across", "split.down", "pane.close"].compactMap(model.command))
        }

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
        status.rootView = StatusLine(state: state?.0, warn: state?.1 ?? false, chat: model.chatLine, needs: model.needs, roles: model.roles,
                                     open: { [weak model] c in model?.open(c, opened: true) },
                                     glance: { [weak self] in self?.showGlance() })
        if case let .linking(step, loggedOut) = model.stateView {
            detail.showLinking(step: step, loggedOut: loggedOut, roles: model.roles,
                               start: { [weak model] phone in model?.jawd.linkStart(phone: phone) },
                               cancel: { [weak model] in model?.jawd.linkCancel() })
        } else if let kind = model.stateView {
            detail.show(state: kind, roles: model.roles)
        } else {
            showPanes()
        }
    }

    /// Lays the panes out as the model's layout has them, the focused one outlined.
    func showPanes() {
        guard let grid = model.grid, model.stateView == nil else { return }
        var views: [Int: NSView] = [:]
        for slot in grid.slots {
            if let chat = slot.chat, let c = model.panes[chat] { views[slot.id] = c.view }
        }
        detail.show(panes: grid, views: views, focused: model.focusedSlot, box: model.conversation?.composer.view, roles: model.roles)
    }

    func restyle() {
        window?.appearance = model.preferences.nsAppearance
        chatList.roles = model.roles
        window?.backgroundColor = model.roles.ground.nsColor
        refresh()
    }

    func toggleChatList() {
        listItem.animator().isCollapsed.toggle()
    }

    func toggleSidebar() {
        sidebarItem.animator().isCollapsed.toggle()
    }

    /// Focus mode: the sidebar and the chat list fold away for the focused pane, and come back.
    func focusMode(_ on: Bool) {
        if on {
            beforeFocus = (sidebarItem.isCollapsed, listItem.isCollapsed)
            sidebarItem.animator().isCollapsed = true
            listItem.animator().isCollapsed = true
        } else {
            sidebarItem.animator().isCollapsed = beforeFocus?.0 ?? false
            listItem.animator().isCollapsed = beforeFocus?.1 ?? false
        }
    }

    func windowDidResize(_: Notification) {
        guard let width = window?.frame.width, !model.focusMode else { return }
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

    // MARK: - The palette

    @objc func showPalette(_: Any?) {
        showPalette(mode: .find)
    }

    func showPalette(mode: PaletteModel.Mode = .find, kind: FindKind? = nil, scope: (String, String)? = nil) {
        guard let window else { return }
        palette.show(over: window, roles: model.roles, mode: mode, kind: kind, scope: scope)
    }

    func goTo(_ m: MessageRow) {
        if let chat = chatList.row(m.chat) ?? model.panes[m.chat]?.chat {
            model.open(chat, opened: true, around: m.id)
        } else {
            model.say("that chat isn't in the list yet")
        }
    }

    func picked(_ pick: PaletteModel.Pick) {
        palette.close()
        switch pick {
        case let .chat(row): model.open(row, opened: true)
        case let .person(p): model.jawd.startChat(person: p.id)
        case let .number(n): model.jawd.startChat(phone: n)
        case let .message(m): goTo(m)
        case let .folder(f):
            if SidebarController.folders.contains(where: { $0.id == f.id }) { model.pick(folder: f.id) } else {
                model.say("folders come with jawd's next step")
            }
        case let .action(action): action.run()
        case let .emoji(message, emoji):
            if let c = model.panes.values.first(where: { $0.window.get(message) != nil }) {
                model.jawd.react(chat: c.chat.id, message: message, emoji: emoji)
            }
        case let .forward(messages, from, to):
            model.jawd.forward(chat: from, messages: messages, to: to)
        }
    }

    // MARK: - The glance

    func showGlance() {
        model.glance.show(relativeTo: status, roles: model.roles)
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

    var keysSheet: NSWindow?

    func showKeys() {
        guard keysSheet == nil else { return }
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: KeysView(commands: Menus.keys(model.allCommands)) { [weak self] in
            guard let self, let sheet = keysSheet else { return }
            window?.endSheet(sheet)
            keysSheet = nil
        }))
        keysSheet = sheet
        window?.beginSheet(sheet)
    }

    var statusWindow: NSWindow?

    func showStatus(_ status: Status) {
        let text = (try? String(data: JSONEncoder.pretty.encode(status.raw), encoding: .utf8)) ?? ""
        let view = ScrollView { Text(text).textStyle(.mono).textSelection(.enabled).padding() }
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
    static let splitAcross = NSToolbarItem.Identifier("split")
    static let layout = NSToolbarItem.Identifier("layout")
    var layoutItem: NSMenuToolbarItem?
    var editorSheet: NSWindow?

    func toolbarDefaultItemIdentifiers(_: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.toggleSidebar, .sidebarTrackingSeparator, .flexibleSpace, Self.layout, Self.find, Self.splitAcross, Self.newChat]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(_: NSToolbar, itemForItemIdentifier id: NSToolbarItem.Identifier, willBeInsertedIntoToolbar _: Bool) -> NSToolbarItem? {
        switch id {
        case Self.find:
            let item = NSToolbarItem(itemIdentifier: id)
            let button = NSButton(title: "Find  \(Shortcut("k").display)", image: NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: "Find")!,
                                  target: self, action: #selector(showPalette(_:)))
            button.imagePosition = .imageLeading
            button.bezelStyle = .toolbar
            item.view = button
            item.label = "Find"
            return item
        case Self.splitAcross:
            let item = NSMenuToolbarItem(itemIdentifier: id)
            item.image = NSImage(systemSymbolName: "rectangle.split.2x1", accessibilityDescription: "Split")
            item.label = "Split"
            item.toolTip = "Another chat beside this one (⌘D), or below it from the menu"
            item.target = self
            item.action = #selector(splitAcross)
            item.showsIndicator = true
            item.menu = Menus.menu(["split.across", "split.down", "pane.close"].compactMap(model.command))
            return item
        case Self.layout:
            let item = NSMenuToolbarItem(itemIdentifier: id)
            item.label = "Layout"
            item.showsIndicator = true
            layoutItem = item
            layoutsChanged()
            return item
        case Self.newChat:
            let item = NSToolbarItem(itemIdentifier: id)
            item.image = NSImage(systemSymbolName: "square.and.pencil", accessibilityDescription: "New Chat")
            item.label = "New Chat"
            item.target = self
            item.action = #selector(showPalette(_:))
            item.toolTip = IconButton.tip("Start a chat: a name or a number", Shortcut("n"))
            return item
        default:
            return nil
        }
    }

    @objc func splitAcross() {
        model.split(.across)
    }

}

// MARK: - Layouts

extension MainWindowController {
    /// The layouts changed, or the one on screen did: the Layouts menu and the toolbar's say so.
    func layoutsChanged() {
        let commands = model.allCommands.filter { $0.place == .layouts }
        if let item = NSApp.mainMenu?.item(withTitle: MenuPlace.layouts.rawValue) {
            item.submenu = Menus.menu(commands, title: MenuPlace.layouts.rawValue)
        }
        guard let item = layoutItem else { return }
        let name = model.book.current ?? "Unsaved Panes"
        item.title = model.book.isEdited ? "\(name) •" : name
        item.toolTip = model.book.isEdited ? "\(name), changed since it was saved: Save Layout (⌥⌘S) keeps it" : "The layout: ⌃⌘1 to ⌃⌘9 go to one"
        item.menu = Menus.menu(commands)
    }

    /// Asks for a name, in a sheet that takes the keys at once: Return saves, Esc cancels.
    func askName(_ title: String, _ detail: String, value: String = "", button: String, done: @escaping (String) -> Void) {
        guard let window else { return }
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        let field = NSTextField(string: value)
        field.placeholderString = "Work"
        field.frame = NSRect(x: 0, y: 0, width: 260, height: 24)
        alert.accessoryView = field
        alert.addButton(withTitle: button)
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        alert.beginSheetModal(for: window) { response in
            if response == .alertFirstButtonReturn { done(field.stringValue) }
        }
    }

    /// The layout editor, in a sheet over the window.
    func showLayoutEditor(_ mode: LayoutEditorModel.Mode) {
        guard let window, editorSheet == nil else { return }
        let editor = LayoutEditorModel(mode: mode, grid: model.grid, name: model.book.current, taken: model.book.names)
        { [weak model] id in model?.chatName(id) ?? id }
        let end: @MainActor () -> Void = { [weak self] in
            guard let self, let sheet = editorSheet else { return }
            window.endSheet(sheet)
            editorSheet = nil
        }
        let view = LayoutEditorView(model: editor, roles: model.roles, save: { [weak model] name, grid in
            end()
            model?.applyEditor(mode, name: name, grid: grid)
        }, cancel: end)
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: view))
        editorSheet = sheet
        window.beginSheet(sheet)
    }

    func askLayoutRename() {
        guard let old = model.book.current else { return }
        askName("Rename \(old)", "", value: old, button: "Rename") { [weak model] name in model?.renameLayout(old, to: name) }
    }

    func askLayoutDelete() {
        guard let window, let name = model.book.current else { return }
        let alert = NSAlert()
        alert.messageText = "Delete \(name)?"
        alert.informativeText = "Its panes stay on screen, unsaved."
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { [weak model] response in
            if response == .alertFirstButtonReturn { model?.deleteLayout(name) }
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

/// The panes' place: the panes, or what's there instead.
@MainActor
final class DetailController: NSViewController {
    let panes = GridView()
    let box = BoxPlace()
    /// The panes with the box under them.
    let panesArea = NSView()
    var stateHost: NSHostingController<StateView>?
    var linkingHost: NSHostingController<LinkingView>?
    var shown: NSView?

    override func loadView() {
        view = NSView()
        for v in [panes, box] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            panesArea.addSubview(v)
        }
        NSLayoutConstraint.activate([
            panes.topAnchor.constraint(equalTo: panesArea.topAnchor),
            panes.leadingAnchor.constraint(equalTo: panesArea.leadingAnchor),
            panes.trailingAnchor.constraint(equalTo: panesArea.trailingAnchor),
            panes.bottomAnchor.constraint(equalTo: box.topAnchor),
            box.leadingAnchor.constraint(equalTo: panesArea.leadingAnchor),
            box.trailingAnchor.constraint(equalTo: panesArea.trailingAnchor),
            box.bottomAnchor.constraint(equalTo: panesArea.bottomAnchor),
        ])
    }

    var drawn: (PaneGrid, Int?, [Int: ObjectIdentifier], ObjectIdentifier?, Roles)?

    /// Lays the panes out with the focused chat's box under them, unless they're already so.
    func show(panes grid: PaneGrid, views: [Int: NSView], focused: Int?, box composer: NSView?, roles: Roles, force: Bool = false) {
        let ids = views.mapValues(ObjectIdentifier.init)
        let boxID = composer.map(ObjectIdentifier.init)
        if !force, shown === panesArea, let drawn, drawn.0 == grid, drawn.1 == focused, drawn.2 == ids, drawn.3 == boxID, drawn.4 == roles {
            return
        }
        place(panesArea)
        panes.show(grid, views: views, focused: focused, roles: roles)
        box.show(composer, roles: roles)
        drawn = (grid, focused, ids, boxID, roles)
    }

    func show(state kind: StateView.Kind, roles: Roles) {
        let view = StateView(kind: kind, roles: roles)
        if let host = stateHost {
            host.rootView = view
        } else {
            let host = NSHostingController(rootView: view)
            host.sizingOptions = []
            stateHost = host
        }
        place(stateHost!.view)
    }

    func showLinking(step: LinkStep?, loggedOut: String?, roles: Roles, start: @escaping (String?) -> Void, cancel: @escaping () -> Void) {
        let view = LinkingView(step: step, loggedOut: loggedOut, roles: roles, start: start, cancel: cancel)
        if let host = linkingHost {
            host.rootView = view
        } else {
            let host = NSHostingController(rootView: view)
            host.sizingOptions = []
            linkingHost = host
        }
        place(linkingHost!.view)
    }

    func place(_ v: NSView) {
        guard shown !== v else { return }
        shown?.removeFromSuperview()
        v.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(v)
        NSLayoutConstraint.activate([
            v.topAnchor.constraint(equalTo: view.topAnchor),
            v.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            v.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            v.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ])
        shown = v
        if v !== linkingHost?.view { linkingHost = nil }
    }
}
