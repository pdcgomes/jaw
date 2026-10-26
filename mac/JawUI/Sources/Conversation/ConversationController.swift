import AppKit
import JawKit
import JawThemes
import SwiftUI

/// What a conversation asks of the app: jawd, mostly.
@MainActor
public protocol ConversationHost: AnyObject {
    func conversation(_ c: ConversationController, move place: Place)
    func conversation(_ c: ConversationController, send text: String, key: String, reply: String?, mentions: [String])
    func conversation(_ c: ConversationController, react message: String, emoji: String)
    func conversation(_ c: ConversationController, edit message: String, text: String)
    func conversation(_ c: ConversationController, delete message: String, everyone: Bool)
    func conversation(_ c: ConversationController, info message: String)
    func conversation(_ c: ConversationController, retry message: String)
    func conversation(_ c: ConversationController, read upTo: String)
    func conversation(_ c: ConversationController, typing state: TypingState)
    func conversation(_ c: ConversationController, draft text: String)
    func conversationWantsHistory(_ c: ConversationController)
    func conversation(_ c: ConversationController, open url: String)
    /// Whether what's on screen counts as read now: the app active and the window key.
    func conversationIsRead(_ c: ConversationController) -> Bool
}

/// One chat's messages and its box. The window's rows are jawd's; this keeps where you're reading,
/// so history arriving above moves nothing and new messages below scroll only when you're at the
/// bottom, and moves jawd's window as you scroll, as the terminal's `settle` does.
@MainActor
public final class ConversationController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    public private(set) var chat: ChatRow
    public private(set) var window: Messages<MessageRow>
    public weak var host: ConversationHost?
    public var spec: LayoutSpec { didSet { if spec != oldValue { relayout() } } }
    let layouts: Layouts

    var rows: [MessageRow] = []
    var contexts: [MessageContext] = []
    var unfolded: Set<String> = []
    /// The message selected with the keys or a click, by id.
    public private(set) var selected: String?
    /// The newest message seen at the bottom, from which what arrives below is counted.
    var seen: String?
    var askedPlace: Place?
    var askedHistory = false
    var anchoredFirst = false
    /// How many were unread when the chat opened, for the "2 new" rule.
    var unreadAtOpen: Int
    var lastRead: (id: String, at: Date)?
    public var members: [ChatInfo.Member] = [] {
        didSet { composer.members = members }
    }

    let scroll = NSScrollView()
    let table = NSTableView()
    let header = HeaderView()
    let newPill = PillButton()
    let hover = HoverBar()
    public let composer: ComposerController
    var empty: NSHostingView<EmptyChatView>?
    var hoveredRow: Int?

    /// The rows each window watch keeps on each side, as the terminal's `WINDOW`.
    public static let rowsAround: UInt32 = 100
    /// How close to an end of the window scrolling comes before the window moves, as `EDGE`.
    static let edge = 20

    public init(chat: ChatRow, spec: LayoutSpec, layouts: Layouts, place: Place) {
        self.chat = chat
        self.spec = spec
        self.layouts = layouts
        unreadAtOpen = chat.unread
        window = Messages(chat: chat.id, place: place)
        composer = ComposerController(chat: chat)
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Where a chat opens: around its first unread message, or at its newest.
    public static func place(for chat: ChatRow) -> Place {
        chat.unread > 0 ? .unread(before: rowsAround / 2, after: rowsAround) : .newest(rowsAround)
    }

    override public func loadView() {
        let root = NSView()
        root.wantsLayer = true
        view = root

        let column = NSTableColumn(identifier: .init("message"))
        column.resizingMask = .autoresizingMask
        table.addTableColumn(column)
        table.headerView = nil
        table.intercellSpacing = .zero
        table.selectionHighlightStyle = .none
        table.backgroundColor = .clear
        table.usesAutomaticRowHeights = false
        table.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        table.dataSource = self
        table.delegate = self
        table.refusesFirstResponder = true
        table.style = .plain
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.automaticallyAdjustsContentInsets = false
        scroll.contentInsets = NSEdgeInsets(top: 8, left: 0, bottom: 8, right: 0)
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification,
                                               object: scroll.contentView)

        newPill.isHidden = true
        newPill.target = self
        newPill.action = #selector(toNewest)

        composer.delegate = self
        addChild(composer)

        for v in [header, scroll, newPill, composer.view] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(v)
        }
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: root.safeAreaLayoutGuide.topAnchor),
            header.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            header.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            header.heightAnchor.constraint(equalToConstant: 48),
            scroll.topAnchor.constraint(equalTo: header.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: composer.view.topAnchor),
            composer.view.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            composer.view.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            composer.view.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            newPill.centerXAnchor.constraint(equalTo: scroll.centerXAnchor),
            newPill.bottomAnchor.constraint(equalTo: scroll.bottomAnchor, constant: -12),
        ])
        hover.react = { [weak self] in self?.hoverAction(.react) }
        hover.reply = { [weak self] in self?.hoverAction(.reply) }
        hover.more = { [weak self] sender in self?.hoverMore(sender) }
        applyTheme()
        header.show(chat)
        updateEmpty()
    }

    var lastWidth: CGFloat = 0

    override public func viewDidLayout() {
        super.viewDidLayout()
        let width = layoutWidth
        if abs(width - lastWidth) >= 1 {
            lastWidth = width
            let anchor = currentAnchor()
            table.noteHeightOfRows(withIndexesChanged: IndexSet(integersIn: 0 ..< rows.count))
            table.reloadData()
            restore(anchor)
        }
    }

    var layoutWidth: CGFloat {
        floor(scroll.contentView.bounds.width)
    }

    public func applyTheme() {
        let roles = spec.style.roles
        view.layer?.backgroundColor = roles.ground.cgColor
        header.roles = roles
        newPill.roles = roles
        hover.layer?.backgroundColor = roles.surface.cgColor
        hover.layer?.borderColor = roles.separator.cgColor
        hover.layer?.borderWidth = 1
        composer.roles = roles
    }

    // MARK: - jawd's rows

    /// The chat's row changed: its name, who's typing, its draft.
    public func update(chat row: ChatRow) {
        chat = row
        header.show(row)
        composer.chatChanged(row)
    }

    /// Applies a window update from jawd, keeping where you're reading.
    public func apply(_ update: Update<MessageRow>) {
        let wasEmpty = window.isEmpty
        let atBottom = isAtBottom || !anchoredFirst
        let anchor = currentAnchor()
        window.apply(update)
        rebuild()
        let items = Array(zip(rows, contexts))
        // The newest first, so what's on screen when following is laid out before the rest.
        layouts.prepare(Array(items.reversed()), width: layoutWidth, spec: spec) { [weak self] in
            guard let self else { return }
            table.reloadData()
            if !anchoredFirst, !rows.isEmpty {
                anchoredFirst = true
                anchorAtOpen()
            } else if atBottom, window.holdsNewest {
                scrollToBottom()
            } else {
                restore(anchor)
            }
            if wasEmpty, !rows.isEmpty { empty?.rootView.chomped += 1 }
            if let id = pendingGo, rows.contains(where: { $0.id == id }) {
                pendingGo = nil
                select(id)
            }
            updateEmpty()
            settle()
            layouts.keep(Set(rows.map(\.id)), width: layoutWidth, spec: spec)
            if !rows.isEmpty, let drawn {
                self.drawn = nil
                drawn()
            }
        }
    }

    func rebuild() {
        rows = window.list
        var cs = MessageContext.of(rows, group: chat.kind.isGroup, unfolded: unfolded, now: Format.now())
        if unreadAtOpen > 0 {
            var others = 0
            for i in stride(from: rows.count - 1, through: 0, by: -1) where !rows[i].mine {
                others += 1
                if others == unreadAtOpen {
                    cs[i].newAbove = unreadAtOpen
                    break
                }
            }
        }
        contexts = cs
    }

    func relayout() {
        let anchor = currentAnchor()
        rebuild()
        table.reloadData()
        restore(anchor)
        applyTheme()
    }

    // MARK: - Where you're reading

    struct Anchor {
        let id: String
        let sort: String
        let offset: CGFloat
    }

    var isAtBottom: Bool {
        let clip = scroll.contentView
        return clip.bounds.maxY >= table.bounds.height + scroll.contentInsets.bottom - 4
    }

    func currentAnchor() -> Anchor? {
        let visible = scroll.contentView.bounds
        let range = table.rows(in: visible)
        guard range.length > 0, range.location < rows.count else { return nil }
        let i = range.location
        let row = rows[i]
        return Anchor(id: row.id, sort: row.sort, offset: table.rect(ofRow: i).minY - visible.minY)
    }

    func restore(_ anchor: Anchor?) {
        guard let anchor else { return }
        let target = rows.firstIndex { $0.id == anchor.id } ?? window.nearest(anchor.sort).flatMap { n in rows.firstIndex { $0.id == n.id } }
        guard let i = target else { return }
        scroll(to: table.rect(ofRow: i).minY - anchor.offset)
    }

    func scroll(to y: CGFloat) {
        let clip = scroll.contentView
        let most = max(table.bounds.height - clip.bounds.height + scroll.contentInsets.bottom, -scroll.contentInsets.top)
        clip.scroll(to: NSPoint(x: 0, y: min(max(y, -scroll.contentInsets.top), most)))
        scroll.reflectScrolledClipView(clip)
    }

    func scrollToBottom() {
        scroll(to: .greatestFiniteMagnitude)
    }

    /// A chat opens on its first unread message, a little way down, with the rest below it; or at
    /// its newest.
    func anchorAtOpen() {
        if let i = contexts.firstIndex(where: { $0.newAbove > 0 }) {
            scroll(to: table.rect(ofRow: i).minY - 40)
        } else {
            scrollToBottom()
        }
    }

    @objc func scrolled() {
        settle()
    }

    /// Records where the conversation is, moves jawd's window when scrolling comes near its ends,
    /// marks what's seen read, and says how many arrived below.
    func settle() {
        let visible = scroll.contentView.bounds
        let range = table.rows(in: visible)
        guard !rows.isEmpty, range.length > 0 else {
            updatePill()
            return
        }
        let first = range.location, last = min(range.location + range.length - 1, rows.count - 1)
        let atBottom = isAtBottom
        let window = self.window
        if first < Self.edge, window.older {
            ask(.around(rows[first].id, before: Self.rowsAround, after: Self.rowsAround))
        } else if atBottom, !window.following {
            ask(.newest(Self.rowsAround))
        } else if !window.following, !window.holdsNewest, rows.count - 1 - last < Self.edge {
            ask(.around(rows[last].id, before: Self.rowsAround, after: Self.rowsAround))
        }
        if first == 0, !window.older, !askedHistory, window.loaded, visible.minY <= 0 {
            askedHistory = true
            host?.conversationWantsHistory(self)
        }
        let newestSeen = rows[last]
        if seen.flatMap({ window.get($0) }).map({ Self.precedes($0, newestSeen) }) ?? true {
            seen = newestSeen.id
        }
        if atBottom { readIfLooking() }
        updatePill()
    }

    /// Whether `a` comes before `b` in the chat, by their sorts compared bytewise, as jawd's are.
    static func precedes(_ a: MessageRow, _ b: MessageRow) -> Bool {
        a.sort == b.sort ? a.id.utf8.lexicographicallyPrecedes(b.id.utf8) : a.sort.utf8.lexicographicallyPrecedes(b.sort.utf8)
    }

    func ask(_ place: Place) {
        guard place != askedPlace else { return }
        askedPlace = place
        host?.conversation(self, move: place)
    }

    /// Reads the chat up to its newest message on screen, at most once a second while they come.
    public func readIfLooking() {
        guard isAtBottom, let host, host.conversationIsRead(self), let newest = rows.last else { return }
        if let last = lastRead, last.id == newest.id { return }
        if let last = lastRead, Date().timeIntervalSince(last.at) < 1 {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in self?.readIfLooking() }
            return
        }
        lastRead = (newest.id, Date())
        unreadAtOpen = 0
        host.conversation(self, read: newest.id)
    }

    func updatePill() {
        let after = seen.map { id in window.get(id).map { window.countAfter($0.sort) } ?? 0 } ?? 0
        let n = isAtBottom ? 0 : after + Int(window.arrived)
        newPill.isHidden = n == 0
        newPill.count = n
    }

    @objc func toNewest() {
        if window.holdsNewest { scrollToBottom() } else { ask(.newest(Self.rowsAround)) }
    }

    func updateEmpty() {
        let isEmpty = window.loaded && rows.isEmpty
        if isEmpty, empty == nil {
            let hosting = NSHostingView(rootView: EmptyChatView(chat: chat))
            hosting.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(hosting, positioned: .above, relativeTo: scroll)
            NSLayoutConstraint.activate([
                hosting.centerXAnchor.constraint(equalTo: scroll.centerXAnchor),
                hosting.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            ])
            empty = hosting
        }
        empty?.isHidden = !isEmpty
    }

    // MARK: - The table

    public func numberOfRows(in _: NSTableView) -> Int {
        rows.count
    }

    public func tableView(_: NSTableView, heightOfRow row: Int) -> CGFloat {
        guard row < rows.count else { return 1 }
        return layouts.laid(rows[row], width: layoutWidth, spec: spec, context: contexts[row]).height
    }

    public func tableView(_ tableView: NSTableView, viewFor _: NSTableColumn?, row: Int) -> NSView? {
        let id = NSUserInterfaceItemIdentifier("message")
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? MessageView) ?? {
            let v = MessageView(frame: .zero)
            v.identifier = id
            v.clicked = { [weak self] view, point, clicks in self?.clicked(view, point, clicks) }
            v.menuFor = { [weak self] view in self?.menu(for: view) }
            v.hovered = { [weak self] view, inside in self?.hovered(view, inside) }
            return v
        }()
        let m = rows[row]
        cell.message = m
        cell.roles = spec.style.roles
        cell.laid = layouts.laid(m, width: layoutWidth, spec: spec, context: contexts[row])
        cell.selected = selected == m.id
        return cell
    }

    func message(of view: MessageView) -> MessageRow? {
        view.message.flatMap { window.get($0.id) }
    }

    func clicked(_ view: MessageView, _ point: CGPoint, _ clicks: Int) {
        guard let m = message(of: view), let laid = view.laid else { return }
        if let region = laid.region(at: point) {
            switch region {
            case let .reaction(emoji): toggle(emoji, on: m)
            case .retry: host?.conversation(self, retry: m.id)
            case .showMore: unfold(m.id)
            case let .quote(id): go(to: id)
            }
            return
        }
        if let link = laid.link(at: point) {
            host?.conversation(self, open: link)
            return
        }
        if clicks >= 2 {
            select(m.id)
            open(m)
        } else if laid.body.contains(point) {
            select(selected == m.id ? nil : m.id)
        } else {
            select(nil)
        }
    }

    func hovered(_ view: MessageView, _ inside: Bool) {
        guard inside, let m = message(of: view), let laid = view.laid, m.kind != .system, m.kind != .call, !m.deleted else {
            if hover.superview === view { hover.removeFromSuperview() }
            return
        }
        hover.removeFromSuperview()
        view.addSubview(hover)
        let size = hover.fittingSize
        let x = m.mine ? laid.body.minX - size.width - 6 : min(laid.body.maxX + 6, view.bounds.width - size.width - 8)
        hover.frame = NSRect(x: max(x, 4), y: laid.body.minY + 2, width: size.width, height: size.height)
        hoveredRow = rows.firstIndex { $0.id == m.id }
    }

    enum HoverAction { case react, reply }

    func hoverAction(_ action: HoverAction) {
        guard let row = hoveredRow, row < rows.count else { return }
        let m = rows[row]
        switch action {
        case .react: showReactions(for: m, from: hover)
        case .reply: reply(to: m)
        }
    }

    func hoverMore(_ sender: NSView) {
        guard let row = hoveredRow, row < rows.count, let cell = table.view(atColumn: 0, row: row, makeIfNecessary: false) as? MessageView,
              let menu = menu(for: cell)
        else { return }
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: sender.bounds.height + 4), in: sender)
    }

    // MARK: - Selecting, as the terminal's keys do

    public func select(_ id: String?) {
        let previous = selected
        selected = id
        for changed in [previous, id].compactMap({ $0 }) {
            if let i = rows.firstIndex(where: { $0.id == changed }),
               let cell = table.view(atColumn: 0, row: i, makeIfNecessary: false) as? MessageView
            {
                cell.selected = selected == changed
            }
        }
        if let id, let i = rows.firstIndex(where: { $0.id == id }) {
            table.scrollRowToVisible(i)
        }
        composer.selecting = id != nil
    }

    /// ↑ from an empty box, or a step through the messages.
    func step(_ by: Int) {
        let selectable = rows.indices.filter { rows[$0].kind != .system && rows[$0].kind != .call }
        guard !selectable.isEmpty else { return }
        guard let id = selected, let i = rows.firstIndex(where: { $0.id == id }),
              let at = selectable.firstIndex(of: i)
        else {
            if by < 0 { select(rows[selectable.last!].id) }
            return
        }
        let next = at + by
        if next >= selectable.count {
            select(nil)
        } else {
            select(rows[selectable[max(next, 0)]].id)
        }
    }

    func go(to id: String) {
        if rows.contains(where: { $0.id == id }) {
            select(id)
        } else {
            ask(.around(id, before: Self.rowsAround, after: Self.rowsAround))
            pendingGo = id
        }
    }

    var pendingGo: String?

    func unfold(_ id: String) {
        unfolded.insert(id)
        relayout()
    }

    /// Enter on a message: try a failed one again, unfold a long one, open its first link.
    func open(_ m: MessageRow) {
        if m.mine, m.status == .failed {
            host?.conversation(self, retry: m.id)
        } else if contexts.indices.contains(rows.firstIndex(of: m) ?? -1), !unfolded.contains(m.id),
                  m.plain.split(separator: "\n").count > MessageLayout.fold
        {
            unfold(m.id)
        } else if let link = m.spans.first(where: { $0.t == .link }) {
            host?.conversation(self, open: link.url ?? link.s)
        }
    }

    var selectedMessage: MessageRow? {
        selected.flatMap { window.get($0) }
    }

    /// A key while a message is selected; says whether it took it.
    func selectionKey(_ event: NSEvent) -> Bool {
        guard let m = selectedMessage else { return false }
        let chars = event.charactersIgnoringModifiers ?? ""
        let plain = event.modifierFlags.intersection([.command, .control, .option]).isEmpty
        switch event.keyCode {
        case 126: step(-1); return true
        case 125: step(1); return true
        case 53: select(nil); return true
        case 36, 76: open(m); return true
        default: break
        }
        guard plain else { return false }
        switch chars {
        case "r": reply(to: m)
        case "e": edit(m)
        case "d": deleteAsked(m)
        case "+", "=": showReactions(for: m, from: nil)
        case "y": copy(m)
        case "i": host?.conversation(self, info: m.id)
        case "1", "2", "3", "4", "5", "6":
            toggle(Self.quick[Int(chars)! - 1], on: m)
        case "f": NSSound.beep()
        default:
            select(nil)
            return false
        }
        return true
    }

    // MARK: - Actions

    /// The six quick reactions, as the terminal's.
    static let quick = ["👍", "❤️", "😂", "😮", "😢", "🙏"]

    func toggle(_ emoji: String, on m: MessageRow) {
        let strip = { (s: String) in s.unicodeScalars.filter { $0.value != 0xFE0F }.map(String.init).joined() }
        let mine = m.reactions.first { $0.mine }.map { strip($0.emoji) == strip(emoji) } ?? false
        host?.conversation(self, react: m.id, emoji: mine ? "" : emoji)
    }

    func reply(to m: MessageRow) {
        guard !m.deleted, m.kind != .system, m.kind != .call else {
            say("there's nothing to reply to in this one")
            return
        }
        select(nil)
        composer.reply(to: m)
    }

    /// Why a message can't be edited, if it can't: yours, text, not deleted, within 15 minutes.
    static func notEditable(_ m: MessageRow, now: Int64 = Format.now()) -> String? {
        if !m.mine { return "only your own messages can be edited" }
        if m.deleted { return "it's been deleted" }
        if m.kind != .text { return "only text can be edited" }
        if m.status == .failed || m.status == .pending { return "it hasn't gone yet" }
        if now - m.at > 15 * 60000 { return "WhatsApp lets you edit a message for 15 minutes" }
        return nil
    }

    static func notForEveryone(_ m: MessageRow, now: Int64 = Format.now()) -> String? {
        if !m.mine { return "only your own messages can be deleted for everyone" }
        if m.deleted { return "it's deleted already" }
        if m.status == .failed { return "it didn't go, so delete it for you" }
        if now - m.at > 60 * 3_600_000 { return "WhatsApp lets you delete a message for everyone for 60 hours" }
        return nil
    }

    func edit(_ m: MessageRow) {
        if let why = Self.notEditable(m) {
            say("can't edit this: \(why)")
            return
        }
        select(nil)
        composer.edit(m)
    }

    func deleteAsked(_ m: MessageRow) {
        guard !m.deleted else { return }
        let alert = NSAlert()
        alert.messageText = "Delete this message?"
        alert.informativeText = m.plain.isEmpty ? (MessageLayout.Builder.kindLine(m) ?? "") : String(m.plain.prefix(200))
        alert.addButton(withTitle: "Delete for Me")
        let why = Self.notForEveryone(m)
        if why == nil { alert.addButton(withTitle: "Delete for Everyone") }
        alert.addButton(withTitle: "Cancel")
        if let why { alert.informativeText += "\n\nNot for everyone: \(why)." }
        guard let w = view.window else { return }
        alert.beginSheetModal(for: w) { [weak self] response in
            guard let self else { return }
            switch response {
            case .alertFirstButtonReturn: host?.conversation(self, delete: m.id, everyone: false)
            case .alertSecondButtonReturn where why == nil: host?.conversation(self, delete: m.id, everyone: true)
            default: break
            }
        }
    }

    func copy(_ m: MessageRow) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(m.plain, forType: .string)
        say("copied")
    }

    func showReactions(for m: MessageRow, from anchor: NSView?) {
        let menu = NSMenu()
        for (i, emoji) in Self.quick.enumerated() {
            let item = NSMenuItem(title: emoji, action: #selector(reactFromMenu(_:)), keyEquivalent: "\(i + 1)")
            item.keyEquivalentModifierMask = []
            item.target = self
            item.representedObject = [m.id, emoji]
            menu.addItem(item)
        }
        if let mine = m.reactions.first(where: \.mine) {
            menu.addItem(.separator())
            let item = NSMenuItem(title: "Take back \(mine.emoji)", action: #selector(reactFromMenu(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = [m.id, ""]
            menu.addItem(item)
        }
        if let anchor {
            menu.popUp(positioning: nil, at: NSPoint(x: 0, y: anchor.bounds.height + 4), in: anchor)
        } else if let i = rows.firstIndex(of: m) {
            let rect = table.rect(ofRow: i)
            menu.popUp(positioning: nil, at: NSPoint(x: 40, y: rect.midY), in: table)
        }
    }

    @objc func reactFromMenu(_ item: NSMenuItem) {
        guard let pair = item.representedObject as? [String], pair.count == 2 else { return }
        host?.conversation(self, react: pair[0], emoji: pair[1])
    }

    func menu(for view: MessageView) -> NSMenu? {
        guard let m = message(of: view), m.kind != .system, m.kind != .call else { return nil }
        let menu = NSMenu()
        func item(_ title: String, _ key: String, _ run: @escaping () -> Void) {
            let i = ClosureItem(title: title, run: run)
            i.keyEquivalent = key
            i.keyEquivalentModifierMask = []
            menu.addItem(i)
        }
        if !m.deleted {
            item("Reply", "r") { [weak self] in self?.reply(to: m) }
            let react = NSMenuItem(title: "React", action: nil, keyEquivalent: "")
            let sub = NSMenu()
            for emoji in Self.quick {
                sub.addItem(ClosureItem(title: emoji) { [weak self] in self?.toggle(emoji, on: m) })
            }
            react.submenu = sub
            menu.addItem(react)
            item("Copy", "y") { [weak self] in self?.copy(m) }
        }
        if Self.notEditable(m) == nil {
            item("Edit", "e") { [weak self] in self?.edit(m) }
        }
        if m.mine, m.status == .failed {
            item("Send Again", "") { [weak self] in
                guard let self else { return }
                host?.conversation(self, retry: m.id)
            }
        }
        menu.addItem(.separator())
        if !m.deleted {
            item("Delete…", "d") { [weak self] in self?.deleteAsked(m) }
        }
        item("Info", "i") { [weak self] in
            guard let self else { return }
            host?.conversation(self, info: m.id)
        }
        return menu
    }

    public var notice: ((String) -> Void)?
    /// Called once each time rows are drawn after an update, for the bench.
    public var drawn: (() -> Void)?

    func say(_ text: String) {
        notice?(text)
    }

    /// A message jawd refused goes back to the box, with jawd's reason.
    public func unsent(text: String, why: String) {
        composer.unsent(text: text, why: why)
    }

    public func focusBox() {
        view.window?.makeFirstResponder(composer.textView)
    }

    /// Leaving the chat: its draft and typing settle now.
    public func leave() {
        composer.leave()
    }
}

extension ConversationController: ComposerDelegate {
    func composer(_: ComposerController, send text: String, key: String, reply: String?, mentions: [String]) {
        host?.conversation(self, send: text, key: key, reply: reply, mentions: mentions)
    }

    func composer(_: ComposerController, edit message: String, text: String) {
        host?.conversation(self, edit: message, text: text)
    }

    func composer(_: ComposerController, draft text: String) {
        host?.conversation(self, draft: text)
    }

    func composer(_: ComposerController, typing state: TypingState) {
        host?.conversation(self, typing: state)
    }

    func composerStepIntoMessages(_: ComposerController) {
        step(-1)
    }

    func composer(_: ComposerController, key event: NSEvent) -> Bool {
        selectionKey(event)
    }

    func composerScroll(_: ComposerController, pages: Int) {
        let clip = scroll.contentView
        scroll(to: clip.bounds.minY + CGFloat(pages) * (clip.bounds.height - 40))
    }
}

/// A menu item that runs a closure.
final class ClosureItem: NSMenuItem {
    let run: () -> Void

    init(title: String, run: @escaping () -> Void) {
        self.run = run
        super.init(title: title, action: #selector(go), keyEquivalent: "")
        target = self
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc func go() {
        run()
    }
}

/// "2 new ↓", over the bottom of the conversation while new messages wait below.
final class PillButton: NSButton {
    var count = 0 {
        didSet { title = "\(Format.num(count)) new ↓" }
    }

    var roles: Roles? {
        didSet {
            layer?.backgroundColor = roles?.accent.cgColor
            contentTintColor = roles.map { $0.isDark ? $0.ground.nsColor : .white }
            attributedTitle = NSAttributedString(string: title, attributes: [
                .foregroundColor: roles.map { $0.isDark ? $0.ground.nsColor : NSColor.white } ?? .white,
                .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
            ])
        }
    }

    override var title: String {
        didSet {
            attributedTitle = NSAttributedString(string: title, attributes: [
                .foregroundColor: roles.map { $0.isDark ? $0.ground.nsColor : NSColor.white } ?? .white,
                .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
            ])
        }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = 13
        heightAnchor.constraint(equalToConstant: 26).isActive = true
        title = "new ↓"
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var intrinsicContentSize: NSSize {
        let size = super.intrinsicContentSize
        return NSSize(width: size.width + 24, height: 26)
    }
}

/// The conversation's header: the chat's name, and how it is.
final class HeaderView: NSView {
    let name = NSTextField(labelWithString: "")
    let detail = NSTextField(labelWithString: "")
    var roles: Roles? {
        didSet { colour() }
    }

    init() {
        super.init(frame: .zero)
        name.font = .systemFont(ofSize: 14, weight: .semibold)
        detail.font = .systemFont(ofSize: 11)
        let stack = NSStackView(views: [name, detail])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 1
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -16),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        name.lineBreakMode = .byTruncatingTail
        detail.lineBreakMode = .byTruncatingTail
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    var typing = false

    func show(_ chat: ChatRow) {
        name.stringValue = chat.name
        typing = !chat.typing.isEmpty
        var parts: [String] = []
        if typing { parts.append(chat.kind.isGroup ? "someone's typing…" : "typing…") }
        if chat.muted { parts.append("muted") }
        if parts.isEmpty, chat.kind.isGroup { parts.append("group") }
        detail.stringValue = parts.joined(separator: " · ")
        detail.isHidden = parts.isEmpty
        setAccessibilityLabel("\(chat.name), \(detail.stringValue)")
        colour()
    }

    func colour() {
        guard let roles else { return }
        name.textColor = roles.text.nsColor
        detail.textColor = typing ? roles.online.nsColor : roles.secondary.nsColor
    }
}
