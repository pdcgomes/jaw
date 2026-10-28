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
final class MainWindowController: NSWindowController, NSWindowDelegate {
    let model: AppModel
    let content: WindowContent
    var sidebar: SidebarController {
        content.sidebar
    }

    var chatList: ChatListController {
        content.chatList
    }

    var detail: DetailController {
        content.detail
    }

    var bar: BottomBar {
        content.bar
    }

    let palette = PalettePanel()
    var editorSheet: NSWindow?
    var sidebarToggle: IconButton?
    var status: NSHostingView<StatusLine> {
        bar.status
    }
    var displayLink: CADisplayLink?
    var settings: NSWindow?

    init(model: AppModel) {
        self.model = model
        content = WindowContent(sidebar: SidebarController(), chatList: ChatListController(folder: model.folder, roles: model.roles),
                                detail: DetailController(), bar: BottomBar(commands: { [weak model] in model?.allCommands ?? [] }))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1180, height: 780),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.title = "Jaw"
        window.minSize = NSSize(width: 520, height: 420)
        window.setFrameAutosaveName("jaw")
        window.titlebarAppearsTransparent = true
        // No toolbar: its controls are in the bottom bar, so every pane has the window's height.
        window.titleVisibility = .hidden
        super.init(window: window)
        window.delegate = self

        window.contentViewController = content
        if window.frame.origin == .zero { window.center() }

        // The sidebar's button beside the window's, in the title bar as an accessory: a toolbar
        // would make the title bar taller and take the clicks of the panes' headers under it.
        let toggle = IconButton(symbol: "sidebar.left", label: "Show or fold the sidebar", shortcut: Shortcut("b"), tone: .secondary)
        { [weak self] in self?.toggleSidebar() }
        sidebarToggle = toggle
        let holder = NSView(frame: NSRect(x: 0, y: 0, width: 34, height: 28))
        holder.addSubview(toggle)
        NSLayoutConstraint.activate([
            toggle.leadingAnchor.constraint(equalTo: holder.leadingAnchor, constant: Space.xs),
            toggle.centerYAnchor.constraint(equalTo: holder.centerYAnchor),
        ])
        let accessory = NSTitlebarAccessoryViewController()
        accessory.view = holder
        accessory.layoutAttribute = .leading
        window.addTitlebarAccessoryViewController(accessory)


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
        detail.panes.dropped = { [weak model] chat, slot, zone in model?.drop(chat, on: slot, zone) }
        detail.panes.paneMenu = { [weak model] slot in
            guard let model else { return nil }
            model.focus(slot: slot)
            if let chat = model.grid?.slot(slot)?.chat, let c = model.panes[chat] { return model.paneMenu(for: c) }
            return Menus.menu(ChatMenu.splits) { id in model.command(id) }
        }

        let link = content.view.displayLink(target: self, selector: #selector(frame(_:)))
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
        content.show(model.situation, chats: chatList.all, focused: model.conversation?.chat, panes: panesShown, roles: model.roles,
                     now: Date(), actions: WindowContent.Actions(
                         open: { [weak model] c in model?.open(c, opened: true) },
                         glance: { [weak self] in self?.showGlance() },
                         linkStart: { [weak model] phone in model?.jawd.linkStart(phone: phone) },
                         linkCancel: { [weak model] in model?.jawd.linkCancel() },
                     ))
    }

    /// The panes as the model's layout has them, the focused one outlined, or nil with no layout.
    var panesShown: WindowContent.Panes? {
        guard let grid = model.grid else { return nil }
        var views: [Int: NSView] = [:]
        for slot in grid.slots {
            if let chat = slot.chat, let c = model.panes[chat] { views[slot.id] = c.view }
        }
        return WindowContent.Panes(grid: grid, views: views, focused: model.focusedSlot, box: model.conversation?.composer.view,
                                   look: model.preferences.paneLook(model.roles))
    }

    /// Lays the panes out as the model's layout has them, the focused one outlined.
    func showPanes() {
        guard let panes = panesShown, model.stateView == nil else { return }
        detail.show(panes: panes.grid, views: panes.views, focused: panes.focused, box: panes.box, roles: model.roles, look: panes.look)
    }

    func restyle() {
        window?.appearance = model.preferences.nsAppearance
        chatList.roles = model.roles
        window?.backgroundColor = model.roles.ground.nsColor
        bar.restyle(model.roles)
        sidebarToggle?.roles = model.roles
        refresh()
    }

    func toggleChatList() {
        content.toggleChatList()
    }

    func toggleSidebar() {
        content.toggleSidebar()
    }

    /// Focus mode: the sidebar and the chat list fold away for the focused pane, and come back.
    func focusMode(_ on: Bool) {
        content.focusMode(on)
    }

    func windowDidResize(_: Notification) {
        content.fold(chatOpen: model.conversation != nil)
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
        case let .newPane(row): model.openInNewPane(row)
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
        guard let window else { return }
        model.glance.show(over: window, roles: model.roles)
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
        let view = StatusView(status: status)
        let w = statusWindow ?? NSWindow(contentViewController: NSHostingController(rootView: view))
        (w.contentViewController as? NSHostingController<StatusView>)?.rootView = view
        w.title = "jawd's status"
        statusWindow = w
        w.makeKeyAndOrderFront(nil)
    }

    /// Settings, in a sheet over the window: modal to it, centred, Done or Esc to close.
    func showSettings() {
        guard let window, settings == nil else { return }
        let view = SettingsView(preferences: model.preferences, accent: model.roles.accent) { [weak self] in
            guard let self, let sheet = settings else { return }
            window.endSheet(sheet)
            settings = nil
        }
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: view))
        settings = sheet
        window.beginSheet(sheet)
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
        bar.show(layout: model.book.current ?? "Unsaved Panes", edited: model.book.isEdited)
    }

    /// Asks for a name, in a sheet that takes the keys at once: Return saves, Esc cancels.
    func askName(_ title: String, _ detail: String, value: String = "", button: String, done: @escaping (String) -> Void) {
        guard let window else { return }
        let (alert, field) = Prompts.name(title, detail, value: value, button: button)
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
        Prompts.deleteLayout(name).beginSheetModal(for: window) { [weak model] response in
            if response == .alertFirstButtonReturn { model?.deleteLayout(name) }
        }
    }
}
