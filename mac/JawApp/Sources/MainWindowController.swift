import AppKit
import JawKit
import JawThemes
import JawUI
import Quartz
import QuartzCore
import SwiftUI

/// The window around its content, `WindowContent`: its frame and where it's kept, the title bar's
/// sidebar button, the sheets, the palette's panel and the display link that drains jawd's pushes.
/// It fills the content from the model, and asks it to fold as the window is resized.
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
    var displayLink: CADisplayLink?

    init(model: AppModel) {
        self.model = model
        // A snapshot takes nothing from the last run and leaves nothing for the next: every checkout's
        // snapshots on the build Mac share the app's defaults.
        let keeps = model.launch.snapshot == nil
        content = WindowContent(sidebar: SidebarController(), chatList: ChatListController(folder: model.folder, roles: model.roles),
                                detail: DetailController(), bar: BottomBar(commands: { [weak model] in model?.allCommands ?? [] }),
                                autosaveName: keeps ? "jaw.split" : nil)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1180, height: 780), styleMask: TitleBar.style,
                              backing: .buffered, defer: false)
        window.title = "Jaw"
        window.minSize = NSSize(width: 520, height: 420)
        if keeps { window.setFrameAutosaveName("jaw") }
        super.init(window: window)
        window.delegate = self

        window.contentViewController = content
        if window.frame.origin == .zero { window.center() }

        sidebarToggle = TitleBar.dress(window) { [weak self] in self?.toggleSidebar() }


        sidebar.host = model
        if keeps {
            sidebar.keepsOpen = "jaw.sidebar.open"
            sidebar.keepsDivider = "jaw.sidebar.divider"
        }
        sidebar.roster.host = model
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
        detail.panes.droppedFiles = { [weak model] board, slot in model?.dropFiles(board, on: slot) }
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
        content.show(model.situation, chats: chatList.all, hasChats: !chatList.all.isEmpty || model.folder != "all", typing: model.typing,
                     focused: model.conversation?.chat, panes: panesShown, roles: model.roles, now: Date(), actions: WindowContent.Actions(
                         open: { [weak model] c in model?.open(c, opened: true) },
                         glance: { [weak self] in self?.showGlance() },
                         linkStart: { [weak model] phone in model?.jawd.linkStart(phone: phone) },
                         // The linking screen goes once linking is done too, when there's nothing to cancel.
                         linkCancel: { [weak model] in if model?.linked == false { model?.jawd.linkCancel() } },
                     ))
        model.chatMenuFoldersChanged()
    }

    /// The panes as the model's layout has them, the focused one outlined, or nil with no layout.
    var panesShown: WindowContent.Panes? {
        guard let grid = model.grid else { return nil }
        var views: [Int: NSView] = [:]
        for slot in grid.slots {
            if let chat = slot.chat, let c = model.panes[chat] { views[slot.id] = c.view }
            if let id = slot.chat, let l = model.libraries[id] { views[slot.id] = l.view }
        }
        let library = model.focusedChat.map(LibraryPane.isLibrary) ?? false
        return WindowContent.Panes(grid: grid, views: views, unopened: model.unopened, focused: model.focusedSlot,
                                   box: model.conversation?.composer.view, boxSays: library ? LibraryController.keys : nil,
                                   look: model.preferences.paneLook(model.roles))
    }

    /// Lays the panes out as the model's layout has them, the focused one outlined.
    func showPanes() {
        guard let panes = panesShown, model.stateView == nil else { return }
        content.show(panes, roles: model.roles)
    }

    func restyle() {
        window?.appearance = model.preferences.nsAppearance
        chatList.roles = model.roles
        sidebar.roles = model.roles
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

    /// The sidebar unfolded, as naming in place needs it.
    func showSidebar() {
        content.showSidebar()
    }

    /// Focus mode: the sidebar and the chat list fold away for the focused pane, and come back.
    func focusMode(_ on: Bool) {
        content.focusMode(on)
    }

    func windowDidResize(_: Notification) {
        content.fold(chatOpen: model.focusedChat != nil)
    }

    func windowDidBecomeKey(_: Notification) {
        model.updateFocus()
        model.windowShown()
        // A panel that took the keys, as Quick Look's does, leaves the window with none of its own: a
        // library pane with the focus takes them back, as a chat's box keeps them.
        if let id = model.focusedChat, let library = model.libraries[id], !library.hasKeys { library.focus() }
    }

    func windowWillClose(_: Notification) {
        model.windowClosed()
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
        goTo(message: m.id, in: m.chat)
    }

    func goTo(message: String, in id: String) {
        if let chat = chatList.row(id) ?? model.panes[id]?.chat {
            model.open(chat, opened: true, around: message)
        } else {
            model.say("that chat isn't in the list yet")
        }
    }

    func picked(_ pick: PaletteModel.Pick) {
        palette.close()
        switch pick {
        case let .chat(row): model.open(row, opened: true)
        case let .newPane(row): model.openInNewPane(row)
        case let .person(p): model.startChat(person: p.id)
        case let .number(n): model.startChat(phone: n)
        case let .message(m): goTo(m)
        case let .link(l): model.openLink(l.link.url)
        case let .linkMessage(l): goTo(message: l.message, in: l.chat)
        case let .folder(f):
            if sidebar.tree.row(f.id) != nil { model.pick(folder: f.id) } else { model.say(sidebar.tree.notHeld(f.name)) }
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

    var cardsSheet: NSWindow?

    /// A message's contact cards, in a sheet over the window: Message on one starts a chat with it.
    func showCards(_ m: MessageRow) {
        guard let window, cardsSheet == nil, let contact = m.contact else { return }
        let end: @MainActor () -> Void = { [weak self] in
            guard let self, let sheet = cardsSheet else { return }
            window.endSheet(sheet)
            cardsSheet = nil
        }
        let from = m.mine ? "Yours" : "From \(m.from?.name ?? "someone")"
        let view = ContactCardsView(contact: contact, from: from, roles: model.roles, message: { [weak model] phone, name in
            end()
            model?.startChat(phone: phone, name: name)
        }, done: end)
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: view))
        cardsSheet = sheet
        window.beginSheet(sheet)
    }

    var contactSheet: NSWindow?

    /// The contact form, in a sheet over the window: Add a Contact… shows the system's picker from
    /// under itself, Send sends the cards, and Cancel lets them go.
    func showContactForm(_ form: ContactForm) {
        guard let window, contactSheet == nil else { return }
        let end: @MainActor () -> Void = { [weak self] in
            guard let self, let sheet = contactSheet else { return }
            form.closed()
            model.contactPicking.close()
            window.endSheet(sheet)
            contactSheet = nil
        }
        let view = ContactFormView(form: form, roles: model.roles, pick: { [weak model] anchor in
            model?.contactPicking.pick(for: form, under: anchor)
        }, send: { [weak model] in
            end()
            model?.send(form)
        }, cancel: end)
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: view))
        contactSheet = sheet
        window.beginSheet(sheet)
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

    var pollSheet: NSWindow?
    /// The poll form in that sheet, for the snapshot's log.
    var pollForm: PollForm?

    /// The poll form, in a sheet over the window: Send closes it and sends the poll to `chat`.
    func showPollForm(_ form: PollForm, chat: String) {
        guard let window, pollSheet == nil else { return }
        let end: @MainActor () -> Void = { [weak self] in
            guard let self, let sheet = pollSheet else { return }
            window.endSheet(sheet)
            pollSheet = nil
            pollForm = nil
        }
        let view = PollFormView(model: form, roles: model.roles, send: { [weak self, weak model] in
            guard self?.pollSheet != nil, let poll = form.send() else { return }
            end()
            model?.sendPoll(chat: chat, poll, key: form.key)
        }, cancel: end)
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: view))
        pollSheet = sheet
        pollForm = form
        window.beginSheet(sheet)
    }

}

// MARK: - Layouts

extension MainWindowController {
    /// The layouts changed, or the one on screen did: the Layouts menu and the toolbar's say so.
    /// The slots changed, or a folder on one was renamed: the Find menu's Favourites say what's on each now.
    func favouritesChanged() {
        let find = model.allCommands.filter { $0.place == .find }
        NSApp.mainMenu?.item(withTitle: MenuPlace.find.rawValue)?.submenu = Menus.menu(find, title: MenuPlace.find.rawValue)
    }

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

    /// Log Out…: asks first, then has jawd log the account out; the linking screen follows once jawd
    /// says it has.
    func askLogOut() {
        guard let window, model.linked else { return }
        Prompts.logOut(model.welcome?.account.me?.phone).beginSheetModal(for: window) { [weak model] response in
            guard response == .alertFirstButtonReturn else { return }
            model?.jawd.linkLogout()
        }
    }

    /// Delete…: asks first, then deletes the folder of yours, saying so once jawd has.
    func askFolderDelete(_ folder: String) {
        guard let window, let row = model.tree.row(folder), row.isYours else { return }
        Prompts.deleteFolder(row.name).beginSheetModal(for: window) { [weak model] response in
            guard response == .alertFirstButtonReturn, let model else { return }
            model.jawd.folderDelete(folder, said: FolderMenu.said(.delete(folder), name: row.name, tree: model.tree))
        }
    }
}

/// Quick Look asks the key window's responder chain for whoever shows files in it: the window's
/// controller hands it the files the conversation asked to look at.
extension MainWindowController {
    override func acceptsPreviewPanelControl(_: QLPreviewPanel!) -> Bool {
        true
    }

    // The panel asks on the main thread, as AppKit's responders are asked.
    override func beginPreviewPanelControl(_ panel: QLPreviewPanel!) {
        let panel = panel!
        MainActor.assumeIsolated { QuickLook.shared.begin(panel) }
    }

    override func endPreviewPanelControl(_ panel: QLPreviewPanel!) {
        let panel = panel!
        MainActor.assumeIsolated { QuickLook.shared.end(panel) }
    }
}
