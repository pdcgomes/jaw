import AppKit
import JawKit
import JawThemes

@MainActor
public protocol ChatListHost: AnyObject {
    /// You picked a chat yourself, from the list.
    func chatList(_ list: ChatListController, open chat: ChatRow)
    func chatList(_ list: ChatListController, move place: Place)
    /// The menu for a chat right-clicked in the list, made as it opens.
    func chatList(_ list: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem]
}

/// A folder's chats, pinned first, then newest, in 56-point rows. The list holds its order while
/// the pointer is over it, or for 1.5 s after a key moves through it, so nothing moves under the
/// pointer before a click, as the terminal holds it. A community is one row that opens to its
/// groups, the announcement group first.
@MainActor
public final class ChatListController: NSViewController, NSTableViewDataSource, NSTableViewDelegate, NSMenuDelegate {
    public weak var host: ChatListHost?
    public private(set) var chats: Chats<ChatRow>
    public var roles: Roles {
        didSet { applyTheme() }
    }

    /// The root jawd's paths are relative to, for avatars.
    public var root: String?
    let table = ChatTable()
    let scroll = NSScrollView()
    var shown: [ChatRow] = []
    var open: Set<String> = []
    public private(set) var selected: String?
    var holdTimer: Timer?
    var pointerInside = false
    var avatars = AvatarCache()

    public init(folder: String, roles: Roles) {
        chats = Chats(folder: folder, place: .top(Self.rows))
        self.roles = roles
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// How many chats the window keeps from the top, grown as the list scrolls.
    public static let rows: UInt32 = 120

    override public func loadView() {
        let column = NSTableColumn(identifier: .init("chat"))
        table.addTableColumn(column)
        table.headerView = nil
        table.rowHeight = 56
        table.intercellSpacing = .zero
        table.style = .plain
        table.backgroundColor = .clear
        table.selectionHighlightStyle = .none
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(clicked)
        table.list = self
        table.setAccessibilityLabel("Chats")
        table.setDraggingSourceOperationMask(.move, forLocal: true)
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        table.menu = menu
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        Scrolling.style(scroll)
        scroll.drawsBackground = false
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification,
                                               object: scroll.contentView)
        let root = PointerView()
        root.entered = { [weak self] inside in self?.pointer(inside) }
        root.wantsLayer = true
        scroll.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(scroll)
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: root.safeAreaLayoutGuide.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor),
        ])
        view = root
        applyTheme()
    }

    func applyTheme() {
        guard isViewLoaded else { return }
        view.layer?.backgroundColor = roles.surface.cgColor
        table.reloadData()
    }

    /// A new folder's list.
    public func show(folder: String) {
        chats = Chats(folder: folder, place: .top(Self.rows))
        rebuild()
    }

    public func apply(_ update: Update<ChatRow>) {
        chats.apply(update)
        rebuild()
    }

    func rebuild() {
        var out: [ChatRow] = []
        let list = chats.list
        let groups = Dictionary(grouping: list.filter { $0.community != nil }, by: { $0.community! })
        for row in list {
            if row.community != nil, chats.get(row.community!) != nil { continue }
            out.append(row)
            if row.kind == .community, open.contains(row.id) {
                let mine = (groups[row.id] ?? []).sorted { ($0.kind == .announcement ? 0 : 1) < ($1.kind == .announcement ? 0 : 1) }
                out.append(contentsOf: mine)
            }
        }
        shown = out
        table.reloadData()
        if let selected, let i = shown.firstIndex(where: { $0.id == selected }) {
            table.selectRowIndexes([i], byExtendingSelection: false)
        }
    }

    public func row(_ id: String) -> ChatRow? {
        chats.get(id)
    }

    public var all: [ChatRow] {
        shown
    }

    // MARK: Holding the order

    func pointer(_ inside: Bool) {
        pointerInside = inside
        if inside { chats.hold() } else { releaseSoon(0) }
    }

    func releaseSoon(_ after: TimeInterval = 1.5) {
        holdTimer?.invalidate()
        holdTimer = Timer.scheduledTimer(withTimeInterval: after, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, !self.pointerInside else { return }
                if self.chats.release() { self.rebuild() }
            }
        }
    }

    // MARK: Choosing

    public func select(_ id: String?) {
        selected = id
        for (i, row) in shown.enumerated() {
            (table.view(atColumn: 0, row: i, makeIfNecessary: false) as? ChatCell)?.isSelected = row.id == id
        }
    }

    @objc func clicked() {
        let i = table.clickedRow
        guard i >= 0, i < shown.count else { return }
        choose(shown[i])
    }

    func choose(_ row: ChatRow) {
        if row.kind == .community {
            if open.contains(row.id) { open.remove(row.id) } else { open.insert(row.id) }
            rebuild()
            return
        }
        select(row.id)
        host?.chatList(self, open: row)
    }

    /// ⌥↑ and ⌥↓: the chat before or after the open one, holding the order meanwhile.
    public func step(_ by: Int, skipping others: Set<String> = []) {
        let pickable = shown.filter { $0.kind != .community && !others.contains($0.id) }
        guard !pickable.isEmpty else { return }
        chats.hold()
        releaseSoon()
        let at = selected.flatMap { id in pickable.firstIndex { $0.id == id } } ?? (by > 0 ? -1 : pickable.count)
        let next = min(max(at + by, 0), pickable.count - 1)
        choose(pickable[next])
        if let i = shown.firstIndex(where: { $0.id == pickable[next].id }) { table.scrollRowToVisible(i) }
    }

    @objc func scrolled() {
        let visible = table.rows(in: scroll.contentView.bounds)
        if visible.location + visible.length > shown.count - 20, UInt64(chats.count) < chats.total,
           chats.place.after < UInt32(chats.count) + Self.rows
        {
            host?.chatList(self, move: .top(UInt32(chats.count) + Self.rows))
        }
    }

    // MARK: Its menu

    /// Fills the table's menu for the row right-clicked, which AppKit outlines while it's open.
    public func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        for item in menuItems(forRow: table.clickedRow) {
            menu.addItem(item)
        }
    }

    /// The menu of the chat at row `i`, from the host; none off the rows.
    func menuItems(forRow i: Int) -> [NSMenuItem] {
        guard i >= 0, i < shown.count else { return [] }
        return host?.chatList(self, menuItemsFor: shown[i]) ?? []
    }

    // MARK: The table

    /// A chat's picture, from the list's cache of jawd's avatars.
    public func avatar(for chat: ChatRow) -> NSImage? {
        avatars.image(chat.avatar, root: root)
    }

    /// A chat dragged out of the list, onto a pane.
    public func tableView(_: NSTableView, pasteboardWriterForRow row: Int) -> (any NSPasteboardWriting)? {
        guard row < shown.count, shown[row].kind != .community else { return nil }
        let item = NSPasteboardItem()
        item.setString(shown[row].id, forType: .jawChat)
        item.setString(shown[row].name, forType: .string)
        return item
    }

    public func numberOfRows(in _: NSTableView) -> Int {
        shown.count
    }

    public func tableView(_ tableView: NSTableView, viewFor _: NSTableColumn?, row: Int) -> NSView? {
        let id = NSUserInterfaceItemIdentifier("chat")
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? ChatCell) ?? {
            let c = ChatCell()
            c.identifier = id
            return c
        }()
        let chat = shown[row]
        cell.show(chat, roles: roles, open: open.contains(chat.id), inCommunity: chat.community != nil && chats.get(chat.community!) != nil,
                  avatar: avatars.image(chat.avatar, root: root))
        cell.isSelected = chat.id == selected
        return cell
    }
}

/// The list's table: Return opens the highlighted chat.
final class ChatTable: NSTableView {
    weak var list: ChatListController?
}

/// Tells when the pointer comes and goes.
final class PointerView: NSView {
    var entered: ((Bool) -> Void)?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas {
            removeTrackingArea(area)
        }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self))
    }

    override func mouseEntered(with _: NSEvent) {
        entered?(true)
    }

    override func mouseExited(with _: NSEvent) {
        entered?(false)
    }
}

/// Avatars, read once from jawd's folder at the size they're drawn.
struct AvatarCache {
    var images: [String: NSImage] = [:]

    mutating func image(_ path: String?, root: String?) -> NSImage? {
        guard let path, let root else { return nil }
        if let image = images[path] { return image }
        guard let image = NSImage(contentsOfFile: (root as NSString).appendingPathComponent(path)) else { return nil }
        image.size = NSSize(width: 36, height: 36)
        if images.count > 300 { images.removeAll() }
        images[path] = image
        return image
    }
}

/// One chat's row: its avatar, its name (bold when unread), the time, its newest message's line (who
/// wrote it in a group, the draft, who's typing), the unread count, an @ for a mention, muted and
/// pinned, and your last message's ticks.
final class ChatCell: NSView {
    var chat: ChatRow?
    var roles: Roles?
    var open = false
    var inCommunity = false
    var avatar: NSImage?
    var isSelected = false {
        didSet { if isSelected != oldValue { needsDisplay = true } }
    }

    override var isFlipped: Bool {
        true
    }

    func show(_ chat: ChatRow, roles: Roles, open: Bool, inCommunity: Bool, avatar: NSImage?) {
        self.chat = chat
        self.roles = roles
        self.open = open
        self.inCommunity = inCommunity
        self.avatar = avatar
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        var label = chat.name
        if chat.unread > 0 { label += ", \(chat.unread) unread" }
        if chat.mentions > 0 { label += ", mentions you" }
        if let last = chat.last { label += ", \(last.from.map { "\($0): " } ?? "")\(last.summary)" }
        setAccessibilityLabel(label)
        needsDisplay = true
    }

    override func draw(_: NSRect) {
        guard let chat, let roles else { return }
        let indent: CGFloat = inCommunity ? 18 : 0
        if isSelected {
            roles.selection.nsColor.setFill()
            NSBezierPath(roundedRect: bounds.insetBy(dx: 6, dy: 2), xRadius: 8, yRadius: 8).fill()
        }
        let avatarRect = NSRect(x: 12 + indent, y: 10, width: 36, height: 36)
        drawAvatar(chat, in: avatarRect, roles)
        let x = avatarRect.maxX + 10
        let right = bounds.width - 12
        let when = chat.last.map { Format.when($0.at, now: Format.now()) } ?? ""
        let timeAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular),
            .foregroundColor: (chat.unread > 0 && !chat.muted ? roles.accent : roles.secondary).nsColor,
        ]
        let time = NSAttributedString(string: when, attributes: timeAttrs)
        let tw = time.size().width
        time.draw(at: NSPoint(x: right - tw, y: 11))
        var nameRight = right - tw - 8
        if chat.pinned {
            nameRight -= draw(symbol: "pin.fill", at: NSPoint(x: nameRight - 10, y: 12), size: 10, colour: roles.tertiary) + 4
        }
        if chat.muted {
            nameRight -= draw(symbol: "bell.slash.fill", at: NSPoint(x: nameRight - 11, y: 12), size: 10, colour: roles.tertiary) + 4
        }
        let bold = chat.unread > 0 || chat.markedUnread
        var name = chat.name
        if chat.kind == .community { name = (open ? "▾ " : "▸ ") + name }
        NSAttributedString(string: name, attributes: [
            .font: bold ? TextStyle.body.emphasised : TextStyle.body.nsFont,
            .foregroundColor: (chat.muted ? roles.secondary : roles.text).nsColor,
            .paragraphStyle: truncating,
        ]).draw(in: NSRect(x: x, y: 9, width: max(nameRight - x, 10), height: 18))

        var badgeRight = right
        let unread = chat.kind == .community ? (chat.childrenUnread ?? 0) : chat.unread
        if unread > 0 || chat.markedUnread {
            badgeRight -= badge(unread > 0 ? Format.num(unread) : " ", right: badgeRight,
                                fill: chat.muted ? roles.tertiary : roles.accent, roles) + 4
        }
        if chat.mentions > 0 {
            badgeRight -= badge("@", right: badgeRight, fill: roles.mention, roles) + 4
        }
        line(chat, roles).draw(in: NSRect(x: x, y: 30, width: max(badgeRight - x - 4, 10), height: 17))
        if chat.typing.isEmpty, chat.draft?.isEmpty ?? true, let last = chat.last, last.mine, let (count, colour) = Ticks.of(last.status, roles),
           let ctx = NSGraphicsContext.current?.cgContext
        {
            Ticks.draw(count, x: x, baseline: 43, size: 12, colour: colour.cgColor, in: ctx)
        }
    }

    var truncating: NSParagraphStyle {
        let p = NSMutableParagraphStyle()
        p.lineBreakMode = .byTruncatingTail
        return p
    }

    /// The second line: who's typing, the draft, or the newest message with your ticks.
    func line(_ chat: ChatRow, _ roles: Roles) -> NSAttributedString {
        let quiet: [NSAttributedString.Key: Any] = [.font: TextStyle.callout.nsFont, .foregroundColor: roles.secondary.nsColor,
                                                    .paragraphStyle: truncating]
        let out = NSMutableAttributedString()
        if !chat.typing.isEmpty {
            out.append(NSAttributedString(string: "typing…", attributes: quiet.merging([.foregroundColor: roles.online.nsColor]) { $1 }))
            return out
        }
        if let draft = chat.draft, !draft.isEmpty {
            out.append(NSAttributedString(string: "Draft: ", attributes: quiet.merging([.foregroundColor: roles.accent.nsColor]) { $1 }))
            out.append(NSAttributedString(string: draft.split(whereSeparator: \.isNewline).joined(separator: " "), attributes: quiet))
            return out
        }
        guard let last = chat.last else {
            out.append(NSAttributedString(string: "No messages yet", attributes: quiet))
            return out
        }
        if last.mine, let status = last.status {
            if let (count, _) = Ticks.of(status, roles) {
                // Room for the ticks, drawn over it by `draw`.
                let room = Ticks.width(count, size: 12) + 4
                out.append(NSAttributedString(string: " ", attributes: quiet.merging([.kern: room - 3.4]) { $1 }))
            } else if status == .pending {
                out.append(NSAttributedString(string: "◷ ", attributes: quiet.merging([.foregroundColor: roles.tertiary.nsColor]) { $1 }))
            } else {
                out.append(NSAttributedString(string: "! ", attributes: quiet.merging([.foregroundColor: roles.failed.nsColor]) { $1 }))
            }
        }
        if chat.kind.isGroup, let from = last.from, !last.mine {
            out.append(NSAttributedString(string: "\(from): ", attributes: quiet.merging([.foregroundColor: roles.text.opacity(0.8).nsColor]) { $1 }))
        }
        out.append(NSAttributedString(string: last.summary.split(whereSeparator: \.isNewline).joined(separator: " "), attributes: quiet))
        return out
    }

    /// A count, a blank or an @ in a pill 18 high, round for one digit; an even height, so a digit, 8
    /// pixels high at 1x, can sit in its middle. Its text is centred by its ink, leaving out a count's
    /// commas, which hang below its digits.
    @discardableResult
    func badge(_ text: String, right: CGFloat, fill: RGBA, _ roles: Roles) -> CGFloat {
        let attributes: [NSAttributedString.Key: Any] = [
            .font: TextStyle.caption.emphasised,
            .foregroundColor: (roles.isDark ? roles.ground : RGBA(0xFFFFFF)).nsColor,
        ]
        let s = NSAttributedString(string: text, attributes: attributes)
        let w = max(s.size().width + 10, 18)
        let rect = NSRect(x: right - w, y: 29, width: w, height: 18)
        fill.nsColor.setFill()
        NSBezierPath(roundedRect: rect, xRadius: rect.height / 2, yRadius: rect.height / 2).fill()
        let ink = NSAttributedString(string: text.filter { $0 != "," }, attributes: attributes).boundingRect(with: .zero, options: .usesDeviceMetrics)
        // Without .usesLineFragmentOrigin, the point drawn at is the baseline's.
        s.draw(with: NSRect(x: rect.midX - s.size().width / 2, y: (rect.midY + ink.midY).rounded(), width: 0, height: 0), options: [], context: nil)
        return w
    }

    @discardableResult
    func draw(symbol: String, at point: NSPoint, size: CGFloat, colour: RGBA) -> CGFloat {
        guard let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: size, weight: .regular).applying(.init(paletteColors: [colour.nsColor])))
        else { return 0 }
        image.draw(in: NSRect(origin: point, size: image.size))
        return image.size.width
    }

    func drawAvatar(_ chat: ChatRow, in rect: NSRect, _ roles: Roles) {
        Avatar.draw(chat.name, id: chat.id, image: avatar, in: rect, roles: roles)
    }
}

public extension NSPasteboard.PasteboardType {
    /// A chat's id, dragged from the list.
    static let jawChat = NSPasteboard.PasteboardType("dev.badcoffee.jaw.chat")
}
