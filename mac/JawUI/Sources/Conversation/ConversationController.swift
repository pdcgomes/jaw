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
    /// `f` and `+`: the palette, to pick the chats to forward to or the reaction.
    func conversation(_ c: ConversationController, forward message: MessageRow)
    func conversation(_ c: ConversationController, reactTo message: MessageRow)
    /// The pane was clicked or typed in: it takes the keys.
    func conversationFocused(_ c: ConversationController)
    func conversation(_ c: ConversationController, open url: String)
    /// Your vote in a poll: every option chosen now, by name, none to take it back.
    func conversation(_ c: ConversationController, vote message: String, options: [String])
    /// Whether what's on screen counts as read now: the app active and the window key.
    func conversationIsRead(_ c: ConversationController) -> Bool
    /// A message's files, of `row`: looked at, an album's from the one at `at`; opened, saved, asked
    /// for or stopped, the one at `at`.
    func conversation(_ c: ConversationController, media act: MediaOffer.Act, _ offers: [MediaOffer], at: Int, of row: MessageRow)
    /// What a file drags out as: its clone once it's here, a promise kept by saving it before.
    func conversation(_ c: ConversationController, dragging offer: MediaOffer) -> NSPasteboardWriting?
    /// The header's Library button: the chat's library, beside its pane.
    func conversationLibrary(_ c: ConversationController)
    /// Message on a contact card: a chat with `phone`, the number to dial, for the card named `name`.
    func conversation(_ c: ConversationController, startChat phone: String, name: String)
    /// A message of several contact cards, clicked or opened: the sheet of them.
    func conversation(_ c: ConversationController, cardsOf message: MessageRow)
    /// The box's files, with the key it made for them, what was typed being the first one's caption.
    func conversation(_ c: ConversationController, send files: [AttachedFile], caption: String, hd: Bool, key: String, reply: String?)
}

public extension ConversationHost {
    func conversationLibrary(_: ConversationController) {}

    func conversation(_: ConversationController, send _: [AttachedFile], caption _: String, hd _: Bool, key _: String, reply _: String?) {}

    func conversation(_: ConversationController, media _: MediaOffer.Act, _: [MediaOffer], at _: Int, of _: MessageRow) {}

    func conversation(_: ConversationController, dragging _: MediaOffer) -> NSPasteboardWriting? {
        nil
    }
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
    /// The pictures its messages show, decoded and cached for every pane alike.
    public var pictures = MessagePictures()

    var rows: [MessageRow] = []
    var contexts: [MessageContext] = []
    var unfolded: Set<String> = []
    /// The message selected with the keys or a click, by id.
    public private(set) var selected: String?
    /// The choices of the vote last asked in each poll, by its message, until its row shows them or
    /// jawd says it wasn't done.
    var asked: [String: [String]] = [:]
    /// How far each file of yours on its way in the window has got, by its message, as jawd's last
    /// job for it said, until it's uploaded or sent.
    var sending: [String: Sending] = [:]
    /// The newest message seen at the bottom, from which what arrives below is counted.
    var seen: String?
    var askedPlace: Place?
    var askedHistory = false
    var anchoredFirst = false
    /// How many were unread when the chat opened, for the "2 new" rule.
    var unreadAtOpen: Int
    var lastRead: (id: String, at: Date)?
    public var members: [ChatInfo.Member] = [] {
        didSet {
            composer.members = members
            header.members = members.count
            header.show(chat)
        }
    }

    /// The chat's picture, from the chat list's cache.
    public var avatar: NSImage? {
        didSet {
            header.avatar.show(chat.name, id: chat.id, image: avatar, roles: spec.style.roles)
            composer.avatar = avatar
        }
    }

    let scroll = NSScrollView()
    let table = TranscriptTable()
    let header = HeaderView()
    /// The pane's menu, made as it opens: on the header, on the transcript's blank space under,
    /// between and beside the messages, and on system and call lines.
    public var paneMenu: (() -> NSMenu?)?
    /// What a message's menu ends with: the pane's splits.
    public var messageMenuTail: (() -> [NSMenuItem])?
    /// The chat's Add to Folder, for m, made as it opens.
    public var folderMenu: (() -> NSMenu?)?

    /// Where the window's buttons are, in the window's coordinates as `convert(_:to: nil)` gives them,
    /// or nil when there are none: a header under them starts after them. By default it's the zoom
    /// button's frame, the last of the three, from the pane's window; a host drawing the pane outside
    /// a window says where they'd be.
    public var windowButtons: () -> NSRect? {
        get { header.windowButtons }
        set { header.windowButtons = newValue }
    }

    let newPill = PillButton()
    let hover = HoverBar()
    /// Pops up a contact card's numbers to choose from, as a click on the card or Enter asks.
    var popUp = MenuPopUp()
    public let composer: ComposerController
    var empty: NSHostingView<EmptyChatView>?
    var hoveredRow: Int?

    /// Who's typing in the chat now, oldest first, as jawd says it. While anyone is, the header says
    /// who and the typing bubble waits below the newest message. Only jawd ends it, as it does when
    /// their message comes, so a pane never says otherwise than the chat list and the status line.
    public var typing: [Typist] = [] {
        didSet {
            guard typing != oldValue else { return }
            header.typing = typing
            header.show(chat)
            typingRow.words = TypingWords.line(typing, group: chat.kind.isGroup) ?? ""
            showTyping()
        }
    }

    /// Gives each of `panes` its chat's typists from `typing`, who's typing in every chat jawd's
    /// windows show, as the app does when it makes a pane and each time jawd says who's typing; a pane
    /// whose chat's typists are as they were is left as it is.
    public static func give(_ typing: Typists, to panes: some Sequence<ConversationController>) {
        for c in panes {
            c.typing = typing[c.chat.id] ?? []
        }
    }

    let typingRow = TypingRowView()
    /// Whether the table has the typing row, after the messages.
    var showsTyping = false
    /// Whether `rows` has moved on from what the table holds, while their layouts are made.
    var reloading = false

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
        Scrolling.style(scroll)
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
        composer.textView.focused = { [weak self] in
            guard let self else { return }
            host?.conversationFocused(self)
        }
        for v in [scroll, header, newPill] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(v)
        }
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: root.topAnchor),
            header.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            header.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            header.heightAnchor.constraint(equalToConstant: ConversationHeader.height),
            scroll.topAnchor.constraint(equalTo: root.topAnchor),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            newPill.centerXAnchor.constraint(equalTo: scroll.centerXAnchor),
            newPill.bottomAnchor.constraint(equalTo: scroll.bottomAnchor, constant: -12),
        ])
        hover.react = { [weak self] in self?.hoverAction(.react) }
        hover.reply = { [weak self] in self?.hoverAction(.reply) }
        hover.more = { [weak self] sender in self?.hoverMore(sender) }
        typingRow.clicked = { [weak self] in
            guard let self else { return }
            host?.conversationFocused(self)
        }
        header.menuFor = { [weak self] in self?.paneMenuShown() }
        header.libraryAsked = { [weak self] in
            guard let self else { return }
            host?.conversationLibrary(self)
        }
        table.menuFor = { [weak self] in self?.paneMenuShown() }
        applyTheme()
        header.show(chat)
        updateEmpty()
    }

    var lastWidth: CGFloat = 0

    override public func viewDidLayout() {
        super.viewDidLayout()
        // The transcript runs under the header, which floats over it at the pane's top, the
        // window's title bar's too: there's no toolbar, so every pane has the same height.
        let top = ConversationHeader.height + Space.m
        if abs(scroll.contentInsets.top - top) >= 0.5 {
            scroll.contentInsets = NSEdgeInsets(top: top, left: 0, bottom: Space.m, right: 0)
            scroll.scrollerInsets = NSEdgeInsets(top: top - Space.m, left: 0, bottom: 0, right: 0)
        }
        header.clearWindowButtons()
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
        hover.roles = roles
        composer.roles = roles
        typingRow.style = spec.style
    }

    // MARK: - jawd's rows

    /// The chat's row changed: its name, its draft, whether it's muted.
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
        shown(votes: update.upsert.map(\.id))
        rebuild()
        reloading = true
        let items = Array(zip(rows, contexts))
        // The newest first, so what's on screen when following is laid out before the rest.
        layouts.prepare(Array(items.reversed()), width: layoutWidth, spec: spec) { [weak self] in
            guard let self else { return }
            reloading = false
            let wasTyping = showsTyping
            showsTyping = wantsTyping
            table.reloadData()
            if showsTyping, wasTyping {
                typingRow.stay()
            } else if showsTyping {
                typingRow.dots.since = CACurrentMediaTime()
                typingRow.arrive()
            }
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
        if !sending.isEmpty {
            let going = rows.filter { $0.mine && $0.status == .pending }
            let held = Set(going.flatMap { [$0.id] + ($0.album?.items.map(\.id) ?? []) })
            sending = sending.filter { held.contains($0.key) }
            for i in rows.indices where rows[i].mine && rows[i].status == .pending {
                cs[i].sending = sending(in: rows[i])
            }
        }
        contexts = cs
    }

    /// The files of yours on their way in `m`, by message: its own, or its album's items'.
    func sending(in m: MessageRow) -> [String: Sending] {
        guard m.mine, m.status == .pending else { return [:] }
        let ids = [m.id] + (m.album?.items.map(\.id) ?? [])
        return sending.filter { ids.contains($0.key) }
    }

    /// How far a copy of a file of yours has got, as jawd's job says: the row it's in laid out again
    /// and drawn when that changes what it shows. A copy uploaded is done, its tick saying the rest;
    /// a download's job, or one for a message the window doesn't hold, changes nothing.
    public func took(_ job: Job) {
        guard job.kind == .transcode || job.kind == .upload,
              let i = rows.firstIndex(where: { $0.id == job.message || $0.album?.items.contains { $0.id == job.message } == true })
        else { return }
        let now = job.kind == .upload && job.progress >= 1 ? nil : Sending(job)
        guard sending[job.message] != now else { return }
        sending[job.message] = now
        contexts[i].sending = sending(in: rows[i])
        let m = rows[i]
        layouts.prepare([(m, contexts[i])], width: layoutWidth, spec: spec) { [weak self] in
            guard let self, !reloading, let at = rows.firstIndex(where: { $0.id == m.id }) else { return }
            table.reloadData(forRowIndexes: [at], columnIndexes: [0])
        }
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

    /// The transcript's height and the height of it shown, and a scroll to a place in it, for the
    /// bench's fling.
    public var transcript: (height: CGFloat, shown: CGFloat) {
        (table.bounds.height, scroll.contentView.bounds.height)
    }

    public func scrollTranscript(to y: CGFloat) {
        scroll(to: y)
    }

    /// Draws every row again, as the bench does when it changes the pictures the pane draws from.
    public func relayoutForBench() {
        relayout()
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
        lookingChanged()
        guard !rows.isEmpty, range.length > 0 else {
            updatePill()
            return
        }
        let first = min(range.location, rows.count - 1), last = min(range.location + range.length - 1, rows.count - 1)
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

    // MARK: - Who's typing

    /// Whether the typing bubble belongs below the messages: someone's typing, the window holds the
    /// chat's newest messages, and there are messages, since an empty chat has the Mimic in it.
    var wantsTyping: Bool {
        !typing.isEmpty && window.holdsNewest && !rows.isEmpty
    }

    /// The typing row comes or goes as `typing` changes, keeping the bottom in view if you were at
    /// the bottom and what you're reading where it was if not.
    func showTyping() {
        guard isViewLoaded, !reloading else { return }
        if wantsTyping {
            guard !showsTyping else { return typingRow.stay() }
            keepingPlace {
                showsTyping = true
                typingRow.dots.since = CACurrentMediaTime()
                table.insertRows(at: IndexSet(integer: rows.count), withAnimation: [])
            }
            typingRow.arrive()
        } else if showsTyping {
            typingRow.leave { [weak self] in self?.dropTyping() }
        }
        lookingChanged()
    }

    func dropTyping() {
        guard showsTyping, !wantsTyping, !reloading else { return }
        keepingPlace {
            showsTyping = false
            table.removeRows(at: IndexSet(integer: rows.count), withAnimation: [])
        }
        lookingChanged()
    }

    /// Makes a change below the messages without moving what you're reading.
    func keepingPlace(_ change: () -> Void) {
        let atBottom = isAtBottom
        let anchor = currentAnchor()
        change()
        if atBottom { scrollToBottom() } else { restore(anchor) }
    }

    /// Lets the header's typing and the typing bubble play while this is the pane you're reading,
    /// the bubble while its row is on screen. Without a host, as in the harness, the pane is the one
    /// you're reading while its window is key.
    public func lookingChanged() {
        guard isViewLoaded else { return }
        let reading = host.map { $0.conversationIsRead(self) } ?? (view.window?.isKeyWindow ?? false)
        let shown = showsTyping && NSLocationInRange(rows.count, table.rows(in: scroll.contentView.bounds))
        typingRow.dots.allowed = reading && shown
        header.typingLabel.allowed = reading && !typing.isEmpty
    }

    // MARK: - The table

    public func numberOfRows(in _: NSTableView) -> Int {
        rows.count + (showsTyping ? 1 : 0)
    }

    public func tableView(_: NSTableView, heightOfRow row: Int) -> CGFloat {
        if showsTyping, row >= rows.count { return TypingRowView.height(spec.style) }
        guard row < rows.count else { return 1 }
        return layouts.laid(rows[row], width: layoutWidth, spec: spec, context: contexts[row]).height
    }

    public func tableView(_ tableView: NSTableView, viewFor _: NSTableColumn?, row: Int) -> NSView? {
        if showsTyping, row >= rows.count { return typingRow }
        guard row < rows.count else { return nil }
        let id = NSUserInterfaceItemIdentifier("message")
        let cell = (tableView.makeView(withIdentifier: id, owner: self) as? MessageView) ?? {
            let v = MessageView(frame: .zero)
            v.identifier = id
            v.clicked = { [weak self] view, point, clicks in self?.clicked(view, point, clicks) }
            v.pressed = { [weak self] view in self?.pressed(view) ?? false }
            v.menuFor = { [weak self] view in self?.menu(for: view) }
            v.hovered = { [weak self] view, inside in self?.hovered(view, inside) }
            v.dragFor = { [weak self] view, item in self?.dragging(view, item) }
            return v
        }()
        let m = rows[row]
        cell.message = m
        cell.sending = contexts[row].sending
        cell.roles = spec.style.roles
        cell.pictures = pictures
        cell.laid = layouts.laid(m, width: layoutWidth, spec: spec, context: contexts[row])
        cell.selected = selected == m.id
        return cell
    }

    /// A row scrolled out of the transcript lets go of the pictures it asked for, so what a fling
    /// passes isn't decoded.
    public func tableView(_: NSTableView, didRemove rowView: NSTableRowView, forRow _: Int) {
        for case let cell as MessageView in rowView.subviews {
            cell.letGo()
        }
    }

    /// The cells on screen ask again for their pictures, let go of while the window was closed, so
    /// they draw from them again rather than from their wells.
    public func askPicturesAgain() {
        table.enumerateAvailableRowViews { rowView, _ in
            for case let cell as MessageView in rowView.subviews {
                cell.askPictures()
            }
        }
    }

    /// The cells on screen let go of what they asked for, so what's still waiting isn't decoded.
    func letGoOfPictures() {
        table.enumerateAvailableRowViews { rowView, _ in
            for case let cell as MessageView in rowView.subviews {
                cell.letGo()
            }
        }
    }

    /// As the window closes, the panes' cells let go of what they asked for and the cache is closed,
    /// keeping nothing and asked for nothing until the window's back, so neither what was being
    /// decoded nor a row that comes meanwhile fills it again, as the design's "under 25 MB with no
    /// window open" asks.
    public static func windowClosed(_ panes: some Sequence<ConversationController>, pictures: MessagePictures) {
        for c in panes {
            c.letGoOfPictures()
        }
        pictures.cache.close()
    }

    /// Once the window's back, after it was closed, the cache is opened and the cells on screen ask
    /// for their pictures again.
    public static func windowShown(_ panes: some Sequence<ConversationController>, pictures: MessagePictures) {
        guard pictures.cache.isClosed else { return }
        pictures.cache.open()
        for c in panes {
            c.askPicturesAgain()
        }
    }

    func message(of view: MessageView) -> MessageRow? {
        view.message.flatMap { window.get($0.id) }
    }

    func clicked(_ view: MessageView, _ point: CGPoint, _ clicks: Int) {
        host?.conversationFocused(self)
        guard let m = message(of: view), let laid = view.laid else { return }
        if let region = laid.region(at: point) {
            switch region {
            case let .reaction(emoji): toggle(emoji, on: m)
            case .retry: host?.conversation(self, retry: m.id)
            case .showMore: unfold(m.id)
            case let .quote(id): go(to: id)
            case let .open(url): host?.conversation(self, open: url)
            // A double click's second click would take back the vote its first made.
            case let .vote(name) where clicks == 1: vote(name, in: m.id)
            case .vote: break
            case let .media(item):
                select(m.id)
                media(.look, m, item: item)
            case let .stop(item): media(.stop, m, item: item)
            case .card: card(m, at: point, in: view)
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
        } else if laid.contains(point) {
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
        case .react: host?.conversation(self, reactTo: m)
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

    /// Goes to a message, moving the window around it when it isn't held, and selects it.
    public func go(to id: String) {
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

    /// Enter on a message: try a failed one again, message whoever a contact card shares, unfold a
    /// long one, look at its file, open a poll's Vote menu under it, open its first link. Says
    /// whether it did any of them.
    @discardableResult
    func open(_ m: MessageRow) -> Bool {
        if m.mine, m.status == .failed {
            host?.conversation(self, retry: m.id)
        } else if m.kind == .contact, !m.deleted {
            card(m, at: nil, in: nil)
        } else if contexts.indices.contains(rows.firstIndex(of: m) ?? -1), !unfolded.contains(m.id),
                  m.plain.split(separator: "\n").count > MessageLayout.fold
        {
            unfold(m.id)
        } else if MediaOffer.of(m) != nil {
            media(.look, m, item: nil)
        } else if let votes = voteMenu(for: m) {
            showVotes(votes, for: m)
        } else if let link = m.spans.first(where: { $0.t == .link }) {
            host?.conversation(self, open: link.url ?? link.s)
        } else {
            return false
        }
        return true
    }

    /// VoiceOver's press on a message, which has no Enter of its own: the message selected, and what
    /// Enter does on it done. Says whether that did anything.
    func pressed(_ view: MessageView) -> Bool {
        host?.conversationFocused(self)
        guard let m = message(of: view) else { return false }
        select(m.id)
        return open(m)
    }

    /// Does `act` with a message's file, or its album's item's; an album looked at as a whole pages
    /// through every item from the first. A message with none says so.
    func media(_ act: MediaOffer.Act, _ m: MessageRow, item: Int?) {
        let offers = MediaOffer.all(m)
        guard !offers.isEmpty else {
            return say(m.viewOnce ? "it's view-once: open it on your phone" : "there's no file in this one")
        }
        let at = item.flatMap { i in offers.firstIndex { $0.item == i } } ?? 0
        host?.conversation(self, media: act, offers, at: at, of: m)
    }

    /// Does `act` with the selected message's file, for the menus and the palette.
    public func media(_ act: MediaOffer.Act) {
        guard let m = selectedMessage else { return }
        media(act, m, item: nil)
    }

    func dragging(_ view: MessageView, _ item: Int?) -> NSPasteboardWriting? {
        guard let m = message(of: view), let offer = MediaOffer.of(m, item: item) else { return nil }
        return host?.conversation(self, dragging: offer)
    }

    var selectedMessage: MessageRow? {
        selected.flatMap { window.get($0) }
    }

    /// Message on a contact card, from a click on it, Enter or its menu: a chat with its one number,
    /// its numbers to choose from where it was clicked, or under it, when it has several, or for
    /// several cards the sheet of them.
    public func card(_ m: MessageRow, at point: CGPoint? = nil, in view: NSView? = nil) {
        switch CardAction.of(m) {
        case let .message(phone, name):
            host?.conversation(self, startChat: phone.dialled, name: name)
        case let .choose(phones, name):
            let menu = numbersMenu(phones, name: name)
            if let point, let view {
                _ = popUp.show(menu, at: point, in: view)
            } else if let i = rows.firstIndex(where: { $0.id == m.id }) {
                _ = popUp.show(menu, at: NSPoint(x: 40, y: table.rect(ofRow: i).midY), in: table)
            }
        case .nothing:
            say(CardAction.noNumber)
        case .cards:
            host?.conversation(self, cardsOf: m)
        }
    }

    /// A card's numbers to start a chat with one, each as the card writes it.
    public func numbersMenu(_ phones: [ContactCard.Phone], name: String) -> NSMenu {
        let menu = NSMenu(title: "Message \(name)")
        for phone in phones {
            menu.addItem(ClosureItem(title: CardAction.label(phone)) { [weak self] in
                guard let self else { return }
                host?.conversation(self, startChat: phone.dialled, name: name)
            })
        }
        return menu
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
        case 36, 76:
            if event.modifierFlags.intersection([.command, .control, .option, .shift]) == .command, MediaOffer.of(m) != nil {
                media(.open, m, item: nil)
            } else {
                open(m)
            }
            return true
        case 49 where plain && MediaOffer.of(m) != nil:
            media(.look, m, item: nil)
            return true
        default: break
        }
        guard plain else { return false }
        switch chars {
        case "s" where MediaOffer.of(m) != nil: media(.save, m, item: nil)
        case "r": reply(to: m)
        case "e": edit(m)
        case "d": deleteAsked(m)
        case "+", "=": host?.conversation(self, reactTo: m)
        case "y": copy(m)
        case "i": host?.conversation(self, info: m.id)
        case "m": fileChat()
        case "1", "2", "3", "4", "5", "6":
            toggle(Self.quick[Int(chars)! - 1], on: m)
        case "f": forward(m)
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

    /// Votes for `name` in poll `id`, as a click on the option, the Vote menu and the snapshot's step
    /// ask: from the vote last asked in it while no row shows that yet, so a second click before
    /// jawd's row for the first builds on it, or else from its row; a choice past what the poll takes
    /// is said, and asks jawd nothing.
    public func vote(_ name: String, in id: String) {
        guard let m = window.get(id), let poll = m.poll, !m.deleted else { return }
        switch Voting.choosing(name, in: poll, mine: asked[id]) {
        case let .vote(options): ask(options, in: m.id)
        case let .tooMany(most): say(Voting.tooMany(most))
        }
    }

    /// Asks jawd for a vote of `options` in poll `id`, kept as the last asked until a row shows it.
    func ask(_ options: [String], in id: String) {
        asked[id] = options
        host?.conversation(self, vote: id, options: options)
    }

    /// Lets go of the votes asked in polls `ids` that their rows, as the window now has them, show, or
    /// whose poll was deleted.
    func shown(votes ids: [String]) {
        for id in ids {
            guard let options = asked[id], let m = window.get(id) else { continue }
            if m.deleted || Set(m.poll?.mine ?? []) == Set(options) { asked[id] = nil }
        }
    }

    /// jawd said the vote last asked in poll `message` wasn't done, or can't tell whether it was, so
    /// the next click goes by the poll's row.
    public func forgetVote(in message: String) {
        asked[message] = nil
    }

    /// The newest poll the pane holds that can be voted in.
    public var newestPoll: MessageRow? {
        rows.last { $0.kind == .poll && $0.poll != nil && !$0.deleted }
    }

    /// A poll's options to vote with, yours ticked, and Take Back My Vote once you have one; nil for
    /// anything that isn't a poll that can be voted in. The options have no keys: an open menu takes a
    /// bare digit as the start of an item's name, so the arrows or an option's first letters and
    /// Return choose, and a selected message's digits stay its quick reactions.
    func voteMenu(for m: MessageRow) -> NSMenu? {
        guard m.kind == .poll, let poll = m.poll, !m.deleted else { return nil }
        let menu = NSMenu()
        for option in poll.options {
            let item = ClosureItem(title: option.name) { [weak self] in self?.vote(option.name, in: m.id) }
            item.state = option.mine ? .on : .off
            menu.addItem(item)
        }
        if !poll.mine.isEmpty {
            menu.addItem(.separator())
            menu.addItem(ClosureItem(title: "Take Back My Vote") { [weak self] in self?.ask([], in: m.id) })
        }
        return menu
    }

    /// The Vote menu under a poll's bubble, as Enter opens it.
    func showVotes(_ menu: NSMenu, for m: MessageRow) {
        guard let i = rows.firstIndex(where: { $0.id == m.id }),
              let cell = table.view(atColumn: 0, row: i, makeIfNecessary: false) as? MessageView, let laid = cell.laid
        else { return }
        _ = cell.popUp.show(menu, at: NSPoint(x: laid.body.minX, y: laid.body.maxY), in: cell)
    }

    func forward(_ m: MessageRow) {
        if let why = Self.notForwardable(m) {
            say("can't forward this: \(why)")
            return
        }
        host?.conversation(self, forward: m)
    }

    /// Why a message can't be forwarded, as jawd refuses: deleted, view-once, a line about the chat, or
    /// a kind WhatsApp doesn't forward.
    public static func notForwardable(_ m: MessageRow) -> String? {
        if m.deleted { return "it's been deleted" }
        if m.viewOnce { return "it's view-once" }
        switch m.kind {
        case .system, .call: return "it's a line about the chat"
        case .poll, .event, .invite, .liveLocation, .album: return "WhatsApp doesn't forward these"
        default: return nil
        }
    }

    /// The selected message, for the palette's actions on it.
    public var selectedRow: MessageRow? {
        selectedMessage
    }

    /// Does what a key on the selected message does, for the palette and the menus.
    public func act(_ key: Character) {
        guard let m = selectedMessage else { return }
        switch key {
        case "r": reply(to: m)
        case "e": edit(m)
        case "d": deleteAsked(m)
        case "+": host?.conversation(self, reactTo: m)
        case "f": forward(m)
        case "y": copy(m)
        case "i": host?.conversation(self, info: m.id)
        case "m": fileChat()
        case "\n": open(m)
        case " ": media(.look, m, item: nil)
        case "s": media(.save, m, item: nil)
        default: break
        }
    }

    /// m: the chat's Add to Folder under the pane's header, as the terminal's m files the open chat.
    func fileChat() {
        _ = header.popUp.show(folderMenu?(), at: NSPoint(x: header.avatar.frame.minX, y: header.bounds.minY), in: header)
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
    public static func notEditable(_ m: MessageRow, now: Int64 = Format.now()) -> String? {
        if !m.mine { return "only your own messages can be edited" }
        if m.deleted { return "it's been deleted" }
        if m.kind != .text { return "only text can be edited" }
        if m.status == .failed || m.status == .pending { return "it hasn't gone yet" }
        if now - m.at > 15 * 60000 { return "WhatsApp lets you edit a message for 15 minutes" }
        return nil
    }

    public static func notForEveryone(_ m: MessageRow, now: Int64 = Format.now()) -> String? {
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
        NSPasteboard.general.setString(Self.copied(m), forType: .string)
        say("copied")
    }

    /// A message's words as they're copied: a contact card's lines as they're drawn, as the terminal
    /// copies them, and the rest's text.
    public static func copied(_ m: MessageRow) -> String {
        guard m.kind == .contact, !m.deleted, let contact = m.contact else { return m.plain }
        return ContactWords.lines(contact)
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

    /// A right click on a message, or its More button: the pane takes the keys first, so the menu
    /// acts on it.
    func menu(for view: MessageView) -> NSMenu? {
        host?.conversationFocused(self)
        return menu(for: message(of: view))
    }

    /// The menu a right click on the message at row `i` opens, for the snapshot's log, made without
    /// moving the focus.
    public func menu(forRow i: Int) -> NSMenu? {
        guard i >= 0, i < rows.count else { return nil }
        return menu(for: rows[i])
    }

    /// The pane's menu for a right click, the pane taking the keys first so the menu acts on it.
    func paneMenuShown() -> NSMenu? {
        host?.conversationFocused(self)
        return paneMenu?()
    }

    /// A message's menu: its actions, then the pane's splits. A system or call line, which has no
    /// actions of its own, has the pane's menu. Making it leaves the focus alone; a right click moves
    /// it first.
    func menu(for m: MessageRow?) -> NSMenu? {
        guard let m, m.kind != .system, m.kind != .call else { return paneMenu?() }
        let menu = NSMenu()
        func item(_ title: String, _ key: String, _ run: @escaping () -> Void) {
            let i = ClosureItem(title: title, run: run)
            i.keyEquivalent = key
            i.keyEquivalentModifierMask = []
            menu.addItem(i)
        }
        switch CardAction.of(m) {
        case let .message(phone, name):
            item("Message \(name)", "\r") { [weak self] in
                guard let self else { return }
                host?.conversation(self, startChat: phone.dialled, name: name)
            }
        case let .choose(phones, name):
            let message = NSMenuItem(title: "Message \(name)", action: nil, keyEquivalent: "")
            message.submenu = numbersMenu(phones, name: name)
            menu.addItem(message)
        case .cards:
            item("View Contacts…", "\r") { [weak self] in
                guard let self else { return }
                host?.conversation(self, cardsOf: m)
            }
        case .nothing:
            break
        }
        if !menu.items.isEmpty { menu.addItem(.separator()) }
        if !m.deleted {
            item("Reply", "r") { [weak self] in self?.reply(to: m) }
            let react = NSMenuItem(title: "React", action: nil, keyEquivalent: "")
            let sub = NSMenu()
            for emoji in Self.quick {
                sub.addItem(ClosureItem(title: emoji) { [weak self] in self?.toggle(emoji, on: m) })
            }
            react.submenu = sub
            menu.addItem(react)
            if let votes = voteMenu(for: m) {
                let vote = NSMenuItem(title: "Vote", action: nil, keyEquivalent: "")
                vote.submenu = votes
                menu.addItem(vote)
            }
            item("Copy", "y") { [weak self] in self?.copy(m) }
            if Self.notForwardable(m) == nil {
                item("Forward…", "f") { [weak self] in self?.forward(m) }
            }
        }
        if let offer = MediaOffer.of(m) {
            menu.addItem(.separator())
            for (act, title, key) in offer.menu {
                let i = ClosureItem(title: title) { [weak self] in self?.media(act, m, item: nil) }
                switch key {
                case "Space": i.keyEquivalent = " "
                case "⌘↩":
                    i.keyEquivalent = "\r"
                    i.keyEquivalentModifierMask = .command
                case let key?: i.keyEquivalent = key
                case nil: break
                }
                if key != "⌘↩" { i.keyEquivalentModifierMask = [] }
                menu.addItem(i)
                let apps = act == .open ? openWith(offer, m) : []
                if !apps.isEmpty {
                    let holder = NSMenuItem(title: "Open With", action: nil, keyEquivalent: "")
                    holder.submenu = NSMenu(title: "Open With")
                    for app in apps {
                        holder.submenu?.addItem(app)
                    }
                    menu.addItem(holder)
                }
            }
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
        let tail = messageMenuTail?() ?? []
        if !tail.isEmpty {
            menu.addItem(.separator())
            for item in tail {
                menu.addItem(item)
            }
        }
        return menu
    }

    /// Open With's items, the apps that open a file that's here, its own first; none before it's here.
    func openWith(_ offer: MediaOffer, _ m: MessageRow) -> [NSMenuItem] {
        guard let here = offer.here, let url = pictures.url(here) else { return [] }
        let apps = NSWorkspace.shared.urlsForApplications(toOpen: url)
        let first = NSWorkspace.shared.urlForApplication(toOpen: url)
        return ([first].compactMap(\.self) + apps.filter { $0 != first }).prefix(12).map { app in
            let name = FileManager.default.displayName(atPath: app.path).replacingOccurrences(of: ".app", with: "")
            let i = ClosureItem(title: app == first ? "\(name) (default)" : name) { [weak self] in self?.media(.openWith(app), m, item: nil) }
            i.image = NSWorkspace.shared.icon(forFile: app.path)
            i.image?.size = NSSize(width: 16, height: 16)
            return i
        }
    }

    /// Open With's items for the selected message's file, for the Message menu.
    public func openWithItems() -> [NSMenuItem] {
        guard let m = selectedMessage, let offer = MediaOffer.of(m) else { return [] }
        return openWith(offer, m)
    }

    public var notice: ((String) -> Void)?
    /// Called once, then cleared, when the next update with rows has been laid out and its rows are in
    /// the table: set by whatever waits for them, as the bench does to time an open and the harness
    /// before a picture.
    public var drawn: (() -> Void)?

    func say(_ text: String) {
        notice?(text)
    }

    /// A message jawd refused goes back to the box, with jawd's reason.
    public func unsent(text: String, why: String) {
        composer.unsent(text: text, why: why)
    }

    /// Files jawd refused go back to the strip, with what was typed and jawd's reason.
    public func unsent(files: [AttachedFile], caption: String, why: String) {
        composer.unsent(files: files, caption: caption, why: why)
    }

    /// Files picked or dropped for this chat, into its box's strip.
    public func attach(_ files: [AttachedFile]) {
        composer.attach(files)
    }

    public func focusBox() {
        view.window?.makeFirstResponder(composer.textView)
    }

    /// Leaving the chat, or the focus leaving its pane: its draft and typing settle now. The box
    /// is the window's, under the panes, showing the focused chat's composer.
    public func leave() {
        composer.leave()
    }
}

extension ConversationController: ComposerDelegate {
    func composer(_: ComposerController, send text: String, key: String, reply: String?, mentions: [String]) {
        host?.conversation(self, send: text, key: key, reply: reply, mentions: mentions)
    }

    func composer(_: ComposerController, send files: [AttachedFile], caption: String, hd: Bool, key: String, reply: String?) {
        host?.conversation(self, send: files, caption: caption, hd: hd, key: key, reply: reply)
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
                .font: TextStyle.callout.emphasised,
            ])
        }
    }

    override var title: String {
        didSet {
            attributedTitle = NSAttributedString(string: title, attributes: [
                .foregroundColor: roles.map { $0.isDark ? $0.ground.nsColor : NSColor.white } ?? .white,
                .font: TextStyle.callout.emphasised,
            ])
        }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = Self.height / 2
        heightAnchor.constraint(equalToConstant: Self.height).isActive = true
        title = "new ↓"
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var intrinsicContentSize: NSSize {
        let size = super.intrinsicContentSize
        return NSSize(width: size.width + Space.xxl, height: Self.height)
    }

    static let height: CGFloat = 26
}

/// The header's height, over the transcript.
enum ConversationHeader {
    static let height: CGFloat = 44
}

/// The conversation's header, floating over the transcript: the chat's picture, its name, and how
/// it is (typing, online or last seen once jawd says presence, muted, a group's members), on a
/// translucent bar the messages scroll under.
final class HeaderView: NSView {
    let blur = NSVisualEffectView()
    let tint = NSView()
    let rule = NSView()
    let avatar = AvatarView()
    let name = Labels.make("", .body)
    let detail = Labels.make("", .caption)
    /// In the detail's place while someone types: "typing", its dots counting, under a shimmer.
    let typingLabel = TypingLabel()
    lazy var lead = avatar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.inset)
    static let inset = Space.l + 2
    var members = 0
    /// What jawd says of the person's presence, "online" or "last seen 14:02", for a chat with one.
    var presence: String?
    var roles: Roles? {
        didSet { colour() }
    }

    /// The pane's menu, asked for as a right click opens it.
    var menuFor: (() -> NSMenu?)?
    var popUp = MenuPopUp()
    /// The chat's library, beside its pane: the button shown while the pointer is over the header,
    /// as the pane's close button is, left of where that sits.
    var libraryAsked: (() -> Void)?
    lazy var library = IconButton(symbol: "photo.on.rectangle.angled", label: "This Chat's Library", shortcut: Shortcut("l", [.command, .option]),
                                  size: .small) { [weak self] in self?.libraryAsked?() }

    override func menu(for _: NSEvent) -> NSMenu? {
        menuFor?()
    }

    /// VoiceOver's Show Menu, which doesn't ask `menu(for:)`: the pane's menu under the header, the
    /// pane taking the keys first, as for a right click.
    override func accessibilityPerformShowMenu() -> Bool {
        popUp.show(menuFor?(), at: NSPoint(x: avatar.frame.minX, y: bounds.minY), in: self)
    }

    init() {
        super.init(frame: .zero)
        name.font = TextStyle.body.emphasised
        blur.blendingMode = .withinWindow
        blur.material = .headerView
        blur.state = .active
        tint.wantsLayer = true
        rule.wantsLayer = true
        typingLabel.isHidden = true
        let stack = NSStackView(views: [name, detail, typingLabel])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        for v in [blur, tint, rule, avatar, stack] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        library.isHidden = true
        addSubview(library)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
        let side: CGFloat = 28
        NSLayoutConstraint.activate([
            blur.topAnchor.constraint(equalTo: topAnchor), blur.bottomAnchor.constraint(equalTo: bottomAnchor),
            blur.leadingAnchor.constraint(equalTo: leadingAnchor), blur.trailingAnchor.constraint(equalTo: trailingAnchor),
            tint.topAnchor.constraint(equalTo: topAnchor), tint.bottomAnchor.constraint(equalTo: bottomAnchor),
            tint.leadingAnchor.constraint(equalTo: leadingAnchor), tint.trailingAnchor.constraint(equalTo: trailingAnchor),
            // The rule ends where the focused pane's outline runs, inside the pane's edge.
            rule.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Shade.inset),
            rule.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Shade.inset),
            rule.bottomAnchor.constraint(equalTo: bottomAnchor), rule.heightAnchor.constraint(equalToConstant: Stroke.hairline),
            lead,
            avatar.centerYAnchor.constraint(equalTo: centerYAnchor),
            avatar.widthAnchor.constraint(equalToConstant: side), avatar.heightAnchor.constraint(equalToConstant: side),
            stack.leadingAnchor.constraint(equalTo: avatar.trailingAnchor, constant: Self.stackGap),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Self.trailing),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            library.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.closing),
            library.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    override func mouseEntered(with _: NSEvent) {
        library.isHidden = false
    }

    override func mouseExited(with _: NSEvent) {
        library.isHidden = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Who's typing, as the conversation's `typing` says; its dots start counting when someone starts.
    var typing: [Typist] = [] {
        didSet { if !typing.isEmpty, oldValue.isEmpty { typingLabel.since = CACurrentMediaTime() } }
    }

    var online = false

    /// Where the name and how the chat is start, after the picture.
    static let stackGap = Space.m + 2
    /// What's kept clear at the header's right, for the pane's buttons: the library's, left of where
    /// the pane's close button sits.
    static let closing = Space.xxl + Space.xl
    static let trailing = closing + 22 + Space.xs

    /// Who's typing gets the room the header has after its picture, its names cut short to fit.
    override func layout() {
        super.layout()
        typingLabel.room = max(bounds.width - avatar.frame.maxX - Self.stackGap - Self.trailing, 0)
    }

    /// Where the window's buttons are, as the conversation's `windowButtons` says: by default its
    /// window's zoom button, the last of the three.
    lazy var windowButtons: () -> NSRect? = { [weak self] in
        guard let zoom = self?.window?.standardWindowButton(.zoomButton), let holder = zoom.superview else { return nil }
        return holder.convert(zoom.frame, to: nil)
    }

    /// A pane at the window's top left, with the sidebar and the list folded away, would have its
    /// picture under the window's buttons: the header starts after them there.
    func clearWindowButtons() {
        guard let buttons = windowButtons()?.insetBy(dx: -Space.xs, dy: -Space.xs) else { return }
        let mine = convert(bounds, to: nil)
        let inset = mine.intersects(buttons) ? max(buttons.maxX + Space.m - mine.minX, Self.inset) : Self.inset
        if abs(lead.constant - inset) >= 0.5 { lead.constant = inset }
    }

    /// The header's empty parts move the window, as a title bar does.
    override var mouseDownCanMoveWindow: Bool {
        true
    }

    func show(_ chat: ChatRow) {
        name.stringValue = chat.name
        let said = TypingWords.line(typing, group: chat.kind.isGroup)
        online = said == nil && presence == "online"
        var parts: [String] = []
        if let said { parts.append(said) }
        if said == nil, let presence { parts.append(presence) }
        if chat.kind.isGroup, said == nil { parts.append(members > 0 ? "\(members) members" : "group") }
        if chat.muted { parts.append("muted") }
        detail.stringValue = parts.joined(separator: " · ")
        detail.isHidden = parts.isEmpty || said != nil
        typingLabel.isHidden = said == nil
        typingLabel.who = TypingWords.who(typing, group: chat.kind.isGroup)
        typingLabel.words = TypingWords.doing(typing) ?? "typing"
        typingLabel.rest = chat.muted ? " · muted" : ""
        setAccessibilityLabel("\(chat.name), \(detail.stringValue)")
        if let roles { avatar.show(chat.name, id: chat.id, image: avatar.image, roles: roles) }
        colour()
    }

    func colour() {
        guard let roles else { return }
        tint.layer?.backgroundColor = roles.ground.opacity(0.72).cgColor
        rule.layer?.backgroundColor = roles.separator.opacity(0.7).cgColor
        name.textColor = roles.text.nsColor
        detail.textColor = !typing.isEmpty || online ? roles.online.nsColor : roles.secondary.nsColor
        typingLabel.colour = roles.online
        library.roles = roles
    }
}

/// The transcript's table: a right click on its blank space, under the messages or beside and between
/// them, which a message's view hands on, opens the pane's menu, as one on a message opens the
/// message's.
final class TranscriptTable: NSTableView {
    var menuFor: (() -> NSMenu?)?

    override func menu(for _: NSEvent) -> NSMenu? {
        menuFor?()
    }
}
