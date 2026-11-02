import AppKit
import JawKit
import JawThemes

/// Whose library a pane is: every chat's, or one chat's.
public enum LibraryScope: Sendable, Hashable {
    case every
    case chat(id: String, name: String)

    public var chat: String? {
        if case let .chat(id, _) = self { id } else { nil }
    }

    /// Where it's from, as its header says it.
    public var said: String {
        switch self {
        case .every: "every chat"
        case let .chat(_, name): name
        }
    }
}

/// What a library pane asks of the app: jawd, mostly.
@MainActor
public protocol LibraryHost: AnyObject {
    /// Watch the library for `filter` from `place`, in place of the window the pane watched.
    func library(_ l: LibraryController, watch filter: LibraryFilter, _ place: Place)
    /// Move the pane's window, keeping its filter.
    func library(_ l: LibraryController, move place: Place)
    /// Enter, or a double-click, on an item: `among` the items it pages through with, in order.
    func library(_ l: LibraryController, open item: LibraryRow, among: [LibraryRow])
    /// ⌘↩ on an item: its message, in its chat.
    func library(_ l: LibraryController, goTo item: LibraryRow)
    /// The items on screen, by id, in order, as they change.
    func library(_ l: LibraryController, shown items: [String])
    /// What an item's menu does with its file: saved to Downloads, or shown in the Finder.
    func library(_ l: LibraryController, media act: MediaOffer.Act, item: LibraryRow)
    /// Esc: the pane closes.
    func libraryClose(_ l: LibraryController)
    /// The pane was clicked or typed in: it takes the keys.
    func libraryFocused(_ l: LibraryController)
}

public extension LibraryHost {
    func library(_: LibraryController, open _: LibraryRow, among _: [LibraryRow]) {}
    func library(_: LibraryController, goTo _: LibraryRow) {}
    func library(_: LibraryController, shown _: [String]) {}
    func library(_: LibraryController, media _: MediaOffer.Act, item _: LibraryRow) {}
    func libraryClose(_: LibraryController) {}
    func libraryFocused(_: LibraryController) {}
}

/// A library in a pane: its header, and its window's items laid out as `LibraryLayout` lays them,
/// photos and videos as a grid of 256-pixel previews and the rest as rows. It keeps the item on
/// screen where it is as its window moves, and moves its window as the grid nears its ends, as a
/// conversation moves a chat's.
@MainActor
public final class LibraryController: NSViewController, NSCollectionViewDelegate,
    NSCollectionViewPrefetching, NSTextFieldDelegate
{
    public let scope: LibraryScope
    public weak var host: LibraryHost?
    public var style: Style {
        didSet { if style != oldValue { applyTheme(); relayout() } }
    }

    /// The pictures its tiles show, decoded and cached for every pane alike.
    public var pictures = MessagePictures() {
        didSet { collection.reloadData() }
    }

    /// The kind shown, nil for every kind's shelves.
    public private(set) var kind: LibraryKind?
    public private(set) var window: Library<LibraryRow>
    /// The item selected, by its id.
    public private(set) var selected: String?
    /// jawd's words for refusing the window, said in its place.
    public private(set) var refused: String?
    /// The search line's words and tokens.
    public private(set) var search = LibrarySearch()
    /// What a filter being typed could become, as the offers under the field show them.
    var offers: [LibraryOffer] = []
    /// The people `from:` offers besides you and the senders of the items held, and the chats `in:`
    /// offers, as the app knows them.
    public var people: () -> [(id: String, name: String)] = { [] }
    public var chats: () -> [(id: String, name: String)] = { [] }
    /// The filter last asked, so the words ask again only when they change.
    var asked: LibraryFilter?
    /// How many items the pane's scope shares, as jawd counted them while nothing narrowed what it
    /// was asked, so the header goes on saying it while a filter narrows the window.
    var shared: Int?
    /// The window shown while a new one's first answer comes, as words change, so the grid doesn't
    /// empty at each key.
    var stale: Library<LibraryRow>?
    /// The window's items of the kinds the library shows, newest first, and where each is.
    var items: [LibraryRow] = []
    var index: [String: Int] = [:]
    var laid = LibraryLaid.empty
    /// The place last asked of the window, so a scroll doesn't ask it again.
    var askedPlace: Place?
    /// What's prefetched, by item: the claim on its picture.
    var prefetched: [String: PictureToken] = [:]
    /// The items last named to jawd as on screen; whether naming waits for its pause to end; and
    /// whether the window's closed, when nothing's named.
    var named: [String] = []
    var naming = false
    var paused = false

    let root = LibraryRootView()
    let header = LibraryHeader()
    let strip = LibraryStrip()
    let bar = LibrarySearchBar()
    let offersView = CompletionView()
    let scroll = NSScrollView()
    let collection = NSCollectionView()
    let grid = LibraryGridLayout()

    /// What the box says while a library's pane has the keys, as the terminal's footer says them.
    public nonisolated static let keys = "↩ opens · ⌘↩ goes to its message · ⇥ another kind · ⎋ closes the library"

    /// Items a kind's window holds each side of the item it's around, and from the top.
    public nonisolated static let side: UInt32 = 300
    public nonisolated static let page: UInt32 = 240
    /// Every kind's window: the newest of each, for its shelves.
    public nonisolated static let shelf: UInt32 = 32

    public init(scope: LibraryScope, style: Style, kind: LibraryKind? = nil) {
        self.scope = scope
        self.style = style
        self.kind = kind
        window = Library(place: Self.place(for: kind))
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Where a window of `kind` starts: a kind's newest, or every kind's newest few.
    public nonisolated static func place(for kind: LibraryKind?) -> Place {
        .top(kind == nil ? shelf : page)
    }

    override public func loadView() {
        view = root
        root.controller = self
        grid.laid = laid
        collection.collectionViewLayout = grid
        grid.pane = self
        collection.dataSource = grid
        collection.delegate = self
        collection.prefetchDataSource = self
        collection.isSelectable = false
        collection.backgroundColors = [.clear]
        collection.register(LibraryGridItem.self, forItemWithIdentifier: LibraryGridItem.id)
        collection.register(LibraryGridItem.self, forItemWithIdentifier: LibraryGridItem.tile)
        scroll.documentView = collection
        scroll.hasVerticalScroller = true
        Scrolling.style(scroll)
        scroll.drawsBackground = false
        scroll.automaticallyAdjustsContentInsets = false
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification,
                                               object: scroll.contentView)
        for v in [scroll, header, strip, bar, offersView] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(v)
        }
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: root.topAnchor),
            header.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            header.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            header.heightAnchor.constraint(equalToConstant: ConversationHeader.height),
            strip.topAnchor.constraint(equalTo: header.bottomAnchor, constant: Space.xs),
            strip.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            strip.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            strip.heightAnchor.constraint(equalToConstant: Self.stripHeight),
            bar.topAnchor.constraint(equalTo: strip.bottomAnchor, constant: Space.xxs),
            bar.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            bar.heightAnchor.constraint(equalToConstant: Self.barHeight),
            offersView.topAnchor.constraint(equalTo: bar.bottomAnchor, constant: -Space.xxs),
            offersView.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: Space.l + Space.xxl),
            scroll.topAnchor.constraint(equalTo: bar.bottomAnchor, constant: Space.xs),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])
        header.clicked = { [weak self] in
            guard let self else { return }
            host?.libraryFocused(self)
        }
        strip.picked = { [weak self] kind in
            guard let self else { return }
            host?.libraryFocused(self)
            choose(kind)
        }
        bar.field.delegate = self
        bar.removed = { [weak self] i in self?.removeToken(i) }
        offersView.isHidden = true
        offersView.picked = { [weak self] item in self?.take(Int(item.value) ?? 0) }
        applyTheme()
        showHeader()
    }

    var lastWidth: CGFloat = 0

    override public func viewDidLayout() {
        super.viewDidLayout()
        header.clearWindowButtons()
        let width = floor(scroll.contentView.bounds.width)
        if abs(width - lastWidth) >= 1 {
            lastWidth = width
            relayout()
        }
    }

    public func applyTheme() {
        let roles = style.roles
        root.wantsLayer = true
        root.layer?.backgroundColor = roles.ground.cgColor
        header.roles = roles
        strip.roles = roles
        bar.roles = roles
        offersView.roles = roles
        collection.reloadData()
    }

    // MARK: - jawd's window

    /// What's asked of jawd: the kind, the scope's chat or `in:`'s, and what the search line narrows
    /// it to.
    public var filter: LibraryFilter {
        search.filter(kind: kind, chat: scope.chat, before: before)
    }

    /// The field's words up to the caret, where a filter being typed ends.
    var before: String {
        let text = bar.field.stringValue as NSString
        guard let editor = bar.field.currentEditor() as? NSTextView else { return text as String }
        return text.substring(to: min(editor.selectedRange().location, text.length))
    }

    /// Shows `kind`, nil for every kind's shelves, watched from the top.
    public func choose(_ kind: LibraryKind?) {
        guard kind != self.kind || refused != nil else { return }
        self.kind = kind
        ask(keeping: false)
    }

    /// Asks jawd for the window as the pane shows it now, from the top; `keeping` what's shown until
    /// its answer comes, as the words change, or not, as the kind does.
    public func ask(keeping: Bool = false) {
        // Each kind's count is of every kind, so it stands until jawd's answer.
        let counts = window.counts
        stale = keeping ? (window.loaded ? window : stale) : nil
        window = Library(place: Self.place(for: kind))
        window.counts = counts
        asked = filter
        askedPlace = nil
        refused = nil
        selected = nil
        rebuild()
        scroll.contentView.scroll(to: .zero)
        host?.library(self, watch: filter, window.place)
    }

    /// Applies an update of the pane's window, keeping the item on screen where it was.
    public func apply(_ update: Update<LibraryRow>) {
        let anchor = currentAnchor()
        window.apply(update)
        if let counts = update.counts, !(asked.map(narrows) ?? false) { shared = counts.shared }
        stale = nil
        if let moved = update.moved, moved.place == askedPlace { askedPlace = nil }
        rebuild()
        restore(anchor)
        settle()
        nameShown()
    }

    /// jawd refused the pane's window, in its words.
    public func refuse(_ why: String) {
        refused = why
        rebuild()
    }

    /// The items again from the window, laid out at the pane's width.
    func rebuild() {
        let shown = window.loaded ? window : stale ?? window
        items = shown.list.filter { LibraryKind.all.contains($0.kind) && (kind == nil || $0.kind == kind) }
        index = Dictionary(items.enumerated().map { ($0.element.id, $0.offset) }) { a, _ in a }
        if let selected, index[selected] == nil { self.selected = nil }
        showHeader()
        showStrip()
        relayout()
    }

    /// What's said where there are no items: nothing until the window's first answer.
    var said: String? {
        if let refused { return refused }
        guard window.loaded else { return nil }
        if let note = window.note, kind == .link || kind == nil { return note }
        return search.filtered ? "Nothing matches." : "Nothing shared here yet."
    }

    /// Whether jawd counts only so far: with words, a person or a time.
    var capped: Bool {
        search.capped(before: before)
    }

    func relayout() {
        guard isViewLoaded else { return }
        let width = floor(scroll.contentView.bounds.width)
        let shown = window.loaded ? window : stale ?? window
        laid = LibraryLayout.lay(items, kind: kind, counts: shown.counts, capped: capped, note: shown.note, said: said,
                                 width: max(width, 1), style: style)
        grid.laid = laid
        grid.invalidateLayout()
        collection.reloadData()
        collection.setFrameSize(NSSize(width: max(width, 1), height: max(laid.height, scroll.contentView.bounds.height)))
    }

    // MARK: - Where it is

    struct Anchor {
        let id: String
        let sort: String
        let offset: CGFloat
    }

    /// The first item on screen and how far below the top it is; none at the very top, which stays
    /// at the top so what comes is seen.
    func currentAnchor() -> Anchor? {
        let visible = scroll.contentView.bounds
        guard visible.minY > 0.5 else { return nil }
        for i in laid.indices(in: visible) {
            guard let id = laid.elements[i].item, let at = index[id] else { continue }
            return Anchor(id: id, sort: items[at].sort, offset: laid.elements[i].frame.minY - visible.minY)
        }
        return nil
    }

    /// Puts the anchor's item where it was on screen, or the newest after it when it's gone.
    func restore(_ anchor: Anchor?) {
        guard let anchor else { return }
        let id = index[anchor.id] != nil ? anchor.id : items.first { $0.sort <= anchor.sort }?.id
        guard let id, let i = laid.element(of: id) else { return }
        scroll(to: laid.elements[i].frame.minY - anchor.offset)
    }

    func scroll(to y: CGFloat) {
        let clip = scroll.contentView
        let most = max(laid.height - clip.bounds.height, 0)
        clip.scroll(to: NSPoint(x: 0, y: min(max(y, 0), most)))
        scroll.reflectScrolledClipView(clip)
    }

    @objc func scrolled() {
        settle()
        nameShown()
    }

    /// How far down the grid is shown, its height and the height of it shown, whether its window
    /// holds the kind's newest and oldest, and a scroll to a place in it, for the bench's fling.
    public var gridShown: (top: CGFloat, height: CGFloat, shown: CGFloat, newest: Bool, oldest: Bool) {
        let clip = scroll.contentView.bounds
        return (clip.minY, laid.height, clip.height, holdsNewest, !window.older)
    }

    public func scrollGrid(to y: CGFloat) {
        scroll(to: y)
    }

    /// The items on screen, by id, in their order, as many as `library.shown` names.
    public var onScreen: [String] {
        Array(laid.indices(in: scroll.contentView.bounds).compactMap { laid.elements[$0].item }.prefix(LibraryShown.most))
    }

    /// Names the items on screen to jawd as they change, as the terminal does, so it asks media for
    /// what they lack: at once, then at most once in 150 ms while they go on changing, as in a
    /// scroll, the last always named; and never again while they're the same, nor while the window's
    /// closed.
    func nameShown() {
        guard !paused, !naming, window.loaded else { return }
        let now = onScreen
        guard now != named else { return }
        named = now
        host?.library(self, shown: now)
        naming = true
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.namingPause) { [weak self] in
            self?.naming = false
            self?.nameShown()
        }
    }

    nonisolated static let namingPause = 0.15

    /// Whether the window holds the kind's newest: watched from the top, or around an item with
    /// fewer newer than it asked for.
    var holdsNewest: Bool {
        switch window.place.around {
        case let .row(id): (index[id] ?? Int.max) < Int(window.place.before)
        case .top, .newest, .unread: true
        }
    }

    /// Moves a kind's window around the item at the top of the screen as the grid comes within two
    /// screens of either end of what it holds; every kind's isn't paged.
    func settle() {
        guard kind != nil, window.loaded, !items.isEmpty else { return }
        let shown = laid.indices(in: scroll.contentView.bounds).compactMap { laid.elements[$0].item }
        guard let first = shown.first, let last = shown.last, let a = index[first], let b = index[last] else { return }
        let screen = max(shown.count, 1)
        let nearEnd = window.older && items.count - 1 - b < 2 * screen
        let nearTop = !holdsNewest && a < 2 * screen
        guard nearEnd || nearTop else { return }
        let place = Place.around(first, before: Self.side, after: Self.side)
        guard place != askedPlace, place != window.place else { return }
        askedPlace = place
        host?.library(self, move: place)
    }

    // MARK: - Selecting, as the terminal's keys move

    /// Says words to VoiceOver as they happen, as the selection moving says the item it's on.
    var announce: @MainActor (String) -> Void = { words in
        NSAccessibility.post(element: NSApplication.shared, notification: .announcementRequested,
                             userInfo: [.announcement: words, .priority: NSAccessibilityPriorityLevel.high.rawValue])
    }

    public func select(_ id: String?) {
        let moved = id != selected
        selected = id
        for case let item as LibraryGridItem in collection.visibleItems() {
            item.cell.selected = item.cell.item?.id == id && id != nil
        }
        guard let id, let i = laid.element(of: id) else { return }
        if moved, let item = selectedItem { announce(LibraryDrawing.spoken(item, context)) }
        let frame = laid.elements[i].frame
        let clip = scroll.contentView.bounds
        // A heading above the item shows with it.
        let top = i > 0 && laid.elements[i - 1].item == nil && laid.elements[i - 1].frame.maxY >= frame.minY - LibraryLayout.gap - 1
            ? laid.elements[i - 1].frame.minY : frame.minY
        if top < clip.minY {
            scroll(to: top - LibraryLayout.Metrics(style).margin)
        } else if frame.maxY > clip.maxY {
            scroll(to: frame.maxY - clip.height + LibraryLayout.Metrics(style).margin)
        }
    }

    public var selectedItem: LibraryRow? {
        selected.flatMap { id in index[id].map { items[$0] } }
    }

    /// Moves the selection that way, as the eye would: along the items, or a line up or down to the
    /// nearest column, as the terminal's library moves.
    public func move(_ dx: Int, _ dy: Int) {
        let slots = laid.elements.filter { $0.item != nil }
        guard !slots.isEmpty else { return }
        guard let at = slots.firstIndex(where: { $0.item == selected }) else { return select(slots[0].item) }
        let here = slots[at]
        let next: Int
        if dx != 0 {
            next = min(max(at + dx, 0), slots.count - 1)
        } else {
            var lines: [CGFloat] = []
            for s in slots where lines.last != s.frame.minY {
                lines.append(s.frame.minY)
            }
            let line = lines.firstIndex(of: here.frame.minY) ?? 0
            let target = lines[min(max(line + dy, 0), lines.count - 1)]
            next = slots.indices.filter { slots[$0].frame.minY == target }
                .min { abs(slots[$0].frame.minX - here.frame.minX) < abs(slots[$1].frame.minX - here.frame.minX) } ?? at
        }
        select(slots[next].item)
    }

    /// How many lines of the grid a screen holds, for Page Up and Page Down.
    var linesOnScreen: Int {
        let lines = laid.indices(in: scroll.contentView.bounds).map { laid.elements[$0].frame.minY }
        return max(Set(lines).count - 1, 1)
    }

    /// A key while the pane has the keys; says whether it took it.
    public func key(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .control, .option, .shift])
        guard flags.isEmpty || flags == .shift else { return false }
        switch event.keyCode {
        case 123: move(-1, 0)
        case 124: move(1, 0)
        case 126: move(0, -1)
        case 125: move(0, 1)
        case 116: move(0, -linesOnScreen)
        case 121: move(0, linesOnScreen)
        case 48: choose(LibraryKinds.step(kind, by: flags == .shift ? -1 : 1))
        default: return false
        }
        return true
    }

    func clicked(_ cell: LibraryCell, _ clicks: Int) {
        host?.libraryFocused(self)
        if case let .heading(_, _, kind?)? = cell.element { return choose(kind) }
        guard let id = cell.item?.id else { return }
        select(id)
        if clicks >= 2 { openSelected() }
    }

    /// ⌘↩ on the selected item: its message, in its chat.
    public func goToSelected() {
        guard let item = selectedItem else { return }
        host?.library(self, goTo: item)
    }

    /// Whether the pane has the keys: its search line, or its own view.
    public var hasKeys: Bool {
        guard let first = view.window?.firstResponder as? NSView else { return false }
        return first === root || first.isDescendant(of: root)
    }

    /// Something to do with an item, as its menu and VoiceOver offer it, with its key, and whether a
    /// separator comes before it in the menu.
    struct Action {
        let title: String
        var key = ""
        var mods: NSEvent.ModifierFlags = []
        var apart = false
        let run: () -> Void
    }

    /// What can be done with an item, selected first: Open, Go to Message, and for a file what's done
    /// with it, or a link's address.
    func actions(for item: LibraryRow) -> [Action] {
        var all = [
            Action(title: item.kind == .link ? "Open in Browser" : item.kind == .voice ? "Play" : "Quick Look", key: "\r") { [weak self] in
                self?.openSelected()
            },
            Action(title: "Go to Message", key: "\r", mods: .command) { [weak self] in self?.goToSelected() },
        ]
        if MediaOffer.of(item) != nil {
            for (title, act) in [("Save to Downloads", MediaOffer.Act.save), ("Show in Finder", .reveal)] {
                all.append(Action(title: title, apart: act == .save) { [weak self] in
                    guard let self else { return }
                    host?.library(self, media: act, item: item)
                })
            }
        }
        if let url = item.link?.url {
            all.append(Action(title: "Copy Link", apart: true) {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(url, forType: .string)
            })
        }
        return all
    }

    /// An item's menu: what can be done with it, the pane taking the keys and the item selected.
    func menu(for cell: LibraryCell) -> NSMenu? {
        guard let item = cell.item else { return nil }
        host?.libraryFocused(self)
        select(item.id)
        let menu = NSMenu()
        for action in actions(for: item) {
            if action.apart { menu.addItem(.separator()) }
            let i = ClosureItem(title: action.title, run: action.run)
            i.keyEquivalent = action.key
            i.keyEquivalentModifierMask = action.mods
            menu.addItem(i)
        }
        return menu
    }

    /// An item's menu as VoiceOver's actions on it, each taking the keys and selecting the item as
    /// it's done, as the menu does as it opens.
    func accessibilityActions(for cell: LibraryCell) -> [NSAccessibilityCustomAction] {
        guard let item = cell.item else { return [] }
        return actions(for: item).map { action in
            NSAccessibilityCustomAction(name: action.title) { [weak self] in
                guard let self else { return false }
                host?.libraryFocused(self)
                select(item.id)
                action.run()
                return true
            }
        }
    }

    /// The selected item's file, as the Message menu's Save to Downloads and Show in Finder take it
    /// while the pane is focused.
    public var selectedOffer: MediaOffer? {
        selectedItem.flatMap { MediaOffer.of($0) }
    }

    /// Saves the selected item's file to Downloads, or shows it in the Finder.
    public func media(_ act: MediaOffer.Act) {
        guard let item = selectedItem, MediaOffer.of(item) != nil else { return }
        host?.library(self, media: act, item: item)
    }

    // MARK: - The header and the strip

    nonisolated static let stripHeight: CGFloat = 32
    nonisolated static let barHeight: CGFloat = 32

    func showStrip() {
        strip.labels = LibraryKinds.labels(window.counts, capped: capped)
        strip.shown = kind
        strip.said = refused == nil ? LibraryKinds.said(kind, loaded: window.loaded, total: window.total, capped: capped, filtered: filtered) : ""
    }

    /// Whether anything narrows what's shown beyond its kind.
    var filtered: Bool {
        search.filtered
    }

    func showHeader() {
        header.show(title: "Library", detail: [scope.said, shared.map { "\(Format.num($0)) shared" }].compactMap(\.self).joined(separator: " · "))
    }

    /// Whether a window asked with `f` holds less than the pane's scope shares: from a person, in a
    /// time, with words, or in another chat.
    func narrows(_ f: LibraryFilter) -> Bool {
        f.person != nil || f.since != nil || f.until != nil || !f.text.isEmpty || f.chat != scope.chat
    }

    // MARK: - The grid

    var context: LibraryDrawing.Context {
        LibraryDrawing.Context(style: style, now: Format.now(), tz: .current, scoped: scope.chat != nil)
    }

    /// The grid's item at `indexPath`, for its layout, which counts the same elements.
    func item(_ collectionView: NSCollectionView, at indexPath: IndexPath) -> NSCollectionViewItem {
        guard laid.elements.indices.contains(indexPath.item) else { return LibraryGridLayout.empty(collectionView, indexPath) }
        let element = laid.elements[indexPath.item]
        let reuse = if case .tile = element.what { LibraryGridItem.tile } else { LibraryGridItem.id }
        let item = collectionView.makeItem(withIdentifier: reuse, for: indexPath) as! LibraryGridItem
        let cell = item.cell
        let row = element.item.flatMap { index[$0] }.map { items[$0] }
        cell.pictures = pictures
        cell.clicked = { [weak self] cell, clicks in self?.clicked(cell, clicks) }
        cell.menuFor = { [weak self] cell in self?.menu(for: cell) }
        cell.actionsFor = { [weak self] cell in self?.accessibilityActions(for: cell) ?? [] }
        cell.show(element.what, item: row, context: context, selected: row != nil && row?.id == selected)
        return item
    }

    /// Tiles about to be shown ask for their pictures ahead, each under a claim of its own that's let
    /// go if they're not shown after all, and once they are, when their cell asks for its own.
    public func collectionView(_: NSCollectionView, prefetchItemsAt indexPaths: [IndexPath]) {
        let scale = view.window?.backingScaleFactor ?? 2, space = LibraryCell.space(of: view.window)
        for path in indexPaths where laid.elements.indices.contains(path.item) {
            guard case let .tile(id) = laid.elements[path.item].what, prefetched[id] == nil, let i = index[id] else { continue }
            let frame = laid.elements[path.item].frame
            guard let slot = LibraryDrawing.drawn(.tile(id), item: items[i], size: frame.size, selected: false, context).slot else { continue }
            let token = pictures.token()
            prefetched[id] = token
            pictures.ask(slot, scale: scale, space: space, for: token) {}
        }
    }

    public func collectionView(_: NSCollectionView, cancelPrefetchingForItemsAt indexPaths: [IndexPath]) {
        for path in indexPaths where laid.elements.indices.contains(path.item) {
            guard let id = laid.elements[path.item].item, let token = prefetched.removeValue(forKey: id) else { continue }
            pictures.letGo(token)
        }
    }

    public func collectionView(_: NSCollectionView, willDisplay item: NSCollectionViewItem, forRepresentedObjectAt indexPath: IndexPath) {
        guard laid.elements.indices.contains(indexPath.item), let id = laid.elements[indexPath.item].item,
              let token = prefetched.removeValue(forKey: id) else { return }
        pictures.letGo(token)
        (item as? LibraryGridItem)?.cell.askPictures()
    }

    public func collectionView(_: NSCollectionView, didEndDisplaying item: NSCollectionViewItem, forRepresentedObjectAt _: IndexPath) {
        (item as? LibraryGridItem)?.cell.letGo()
    }

    /// As the window closes, the cells let go of what they asked for, and what was prefetched, and
    /// nothing's named on screen until it's back.
    public func letGoOfPictures() {
        paused = true
        for case let item as LibraryGridItem in collection.visibleItems() {
            item.cell.letGo()
        }
        for token in prefetched.values {
            pictures.letGo(token)
        }
        prefetched.removeAll()
    }

    /// Once the window's back, the cells on screen ask for their pictures again, and what's on
    /// screen is named.
    public func askPicturesAgain() {
        paused = false
        for case let item as LibraryGridItem in collection.visibleItems() {
            item.cell.askPictures()
        }
        named = []
        nameShown()
    }

    /// Gives the pane the keys: its search line takes them.
    public func focus() {
        view.window?.makeFirstResponder(bar.field)
    }

    // MARK: - The search line

    public func controlTextDidChange(_: Notification) {
        search.text = bar.field.stringValue
        showOffers()
        if filter != asked { ask(keeping: true) } else { showStrip(); showHeader() }
    }

    /// What the filter being typed could become, under the field: you, the people the app knows and
    /// the senders of the items held for `from:`, the chats for `in:`, and days for `since:` and
    /// `until:`.
    func showOffers() {
        let senders = items.compactMap { $0.from.map { (id: $0.id, name: $0.name) } }
        offers = search.offers(before: before, people: people() + senders, chats: chats(), now: Format.now())
        offersView.show(offers.enumerated().map { i, offer in
            CompletionView.Item(title: offer.said, detail: nil, value: String(i))
        })
    }

    var offering: Bool {
        !offersView.isHidden && !offers.isEmpty
    }

    /// Takes the offer at `i`: the filter typed becomes its token, and jawd's asked again.
    func take(_ i: Int) {
        guard offers.indices.contains(i) else { return }
        search.take(offers[i], before: before)
        bar.field.stringValue = search.text
        bar.field.currentEditor()?.selectedRange = NSRange(location: (search.text as NSString).length, length: 0)
        bar.tokens = search.tokens
        offersView.isHidden = true
        offers = []
        ask(keeping: true)
    }

    /// The search line as it is once `tokens` are taken and `text` typed after them, what the filter
    /// being typed could become under it, without asking jawd: as the harness and a test show it.
    public func typed(_ text: String, tokens: [LibraryToken]) {
        search.tokens = tokens
        search.text = text
        bar.tokens = tokens
        bar.field.stringValue = text
        showOffers()
        asked = filter
        showStrip()
        showHeader()
    }

    /// A token's × clicked: it goes, and jawd's asked again.
    func removeToken(_ i: Int) {
        guard search.tokens.indices.contains(i) else { return }
        search.tokens.remove(at: i)
        bar.tokens = search.tokens
        ask(keeping: true)
    }

    /// The keys the search line's field hands on, as the terminal's search line takes them; says
    /// whether it took one.
    public func control(_: NSControl, textView _: NSTextView, doCommandBy selector: Selector) -> Bool {
        let empty = bar.field.stringValue.isEmpty
        switch selector {
        case #selector(NSResponder.moveUp(_:)):
            if offering { offersView.move(-1) } else { move(0, -1) }
        case #selector(NSResponder.moveDown(_:)):
            if offering { offersView.move(1) } else { move(0, 1) }
        case #selector(NSResponder.moveLeft(_:)) where empty && !offering:
            move(-1, 0)
        case #selector(NSResponder.moveRight(_:)) where empty && !offering:
            move(1, 0)
        case #selector(NSResponder.scrollPageUp(_:)), #selector(NSResponder.pageUp(_:)):
            move(0, -linesOnScreen)
        case #selector(NSResponder.scrollPageDown(_:)), #selector(NSResponder.pageDown(_:)):
            move(0, linesOnScreen)
        case #selector(NSResponder.insertTab(_:)), #selector(NSResponder.insertBacktab(_:)):
            if offering { offersView.pick() } else { choose(LibraryKinds.step(kind, by: selector == #selector(NSResponder.insertTab(_:)) ? 1 : -1)) }
        case #selector(NSResponder.insertNewline(_:)):
            if offering { offersView.pick() } else { openSelected() }
        case #selector(NSResponder.cancelOperation(_:)):
            if offering {
                search.dismissed = search.text
                offersView.isHidden = true
            } else {
                host?.libraryClose(self)
            }
        case #selector(NSResponder.deleteBackward(_:)) where empty:
            if search.takeBack() {
                bar.tokens = search.tokens
                ask(keeping: true)
            }
        case Selector(("noop:")) where Self.commandReturn(NSApp.currentEvent):
            // ⌘↩, which no key binding names: the field hands it on as noop when the app isn't
            // active and the window's views weren't asked for it as a key equivalent first.
            goToSelected()
        default:
            return false
        }
        return true
    }

    /// Whether `event` is ⌘↩, and nothing else held.
    static func commandReturn(_ event: NSEvent?) -> Bool {
        guard let event, event.type == .keyDown, [36, 76].contains(event.keyCode) else { return false }
        return event.modifierFlags.intersection([.command, .control, .option, .shift]) == .command
    }

    /// Enter, or a double-click, on the selected item.
    public func openSelected() {
        guard let item = selectedItem else { return }
        host?.library(self, open: item, among: items)
    }
}

/// The library's view: it takes the keys while its pane has them.
final class LibraryRootView: NSView {
    weak var controller: LibraryController?

    override var isFlipped: Bool {
        true
    }

    override var acceptsFirstResponder: Bool {
        true
    }

    override func keyDown(with event: NSEvent) {
        if controller?.key(event) == true { return }
        super.keyDown(with: event)
    }

    /// ⌘↩ goes to the selected item's message. A key with ⌘ goes down the window's views before the
    /// menu bar and the search line's field see it, so it's taken here while the pane has the keys.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard LibraryController.commandReturn(event), let controller, controller.hasKeys else { return super.performKeyEquivalent(with: event) }
        controller.goToSelected()
        return true
    }

    override func mouseDown(with event: NSEvent) {
        if let controller { controller.host?.libraryFocused(controller) }
        super.mouseDown(with: event)
    }
}

/// The library's header, as a conversation's: its mark, "Library", and whose and how many are shared,
/// on a bar the window can be moved by.
final class LibraryHeader: NSView {
    let tint = NSView()
    let rule = NSView()
    let mark = NSImageView()
    let name = Labels.make("Library", .body)
    let detail = Labels.make("", .caption)
    lazy var lead = mark.leadingAnchor.constraint(equalTo: leadingAnchor, constant: HeaderView.inset)
    var clicked: (() -> Void)?
    var roles: Roles? {
        didSet { colour() }
    }

    init() {
        super.init(frame: .zero)
        name.font = TextStyle.body.emphasised
        tint.wantsLayer = true
        rule.wantsLayer = true
        mark.wantsLayer = true
        mark.imageScaling = .scaleNone
        mark.setAccessibilityElement(false)
        let stack = NSStackView(views: [name, detail])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        for v in [tint, rule, mark, stack] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        let side: CGFloat = 28
        NSLayoutConstraint.activate([
            tint.topAnchor.constraint(equalTo: topAnchor), tint.bottomAnchor.constraint(equalTo: bottomAnchor),
            tint.leadingAnchor.constraint(equalTo: leadingAnchor), tint.trailingAnchor.constraint(equalTo: trailingAnchor),
            rule.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Shade.inset),
            rule.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Shade.inset),
            rule.bottomAnchor.constraint(equalTo: bottomAnchor), rule.heightAnchor.constraint(equalToConstant: Stroke.hairline),
            lead,
            mark.centerYAnchor.constraint(equalTo: centerYAnchor),
            mark.widthAnchor.constraint(equalToConstant: side), mark.heightAnchor.constraint(equalToConstant: side),
            stack.leadingAnchor.constraint(equalTo: mark.trailingAnchor, constant: HeaderView.stackGap),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -HeaderView.trailing),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        setAccessibilityRole(.group)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(title: String, detail words: String) {
        name.stringValue = title
        detail.stringValue = words
        detail.isHidden = words.isEmpty
        setAccessibilityLabel([title, words].filter { !$0.isEmpty }.joined(separator: ", "))
    }

    func colour() {
        guard let roles else { return }
        tint.layer?.backgroundColor = roles.ground.cgColor
        rule.layer?.backgroundColor = roles.separator.opacity(0.7).cgColor
        name.textColor = roles.text.nsColor
        detail.textColor = roles.secondary.nsColor
        mark.layer?.cornerRadius = Radius.panel
        mark.layer?.backgroundColor = roles.accent.opacity(0.15).cgColor
        mark.image = NSImage(systemSymbolName: "photo.on.rectangle.angled", accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 12, weight: .medium).applying(.init(hierarchicalColor: roles.accent.nsColor)))
    }

    /// Where the window's buttons are; a header under them starts after them, as a conversation's does.
    lazy var windowButtons: () -> NSRect? = { [weak self] in
        guard let zoom = self?.window?.standardWindowButton(.zoomButton), let holder = zoom.superview else { return nil }
        return holder.convert(zoom.frame, to: nil)
    }

    func clearWindowButtons() {
        guard let buttons = windowButtons()?.insetBy(dx: -Space.xs, dy: -Space.xs) else { return }
        let mine = convert(bounds, to: nil)
        let inset = mine.intersects(buttons) ? max(buttons.maxX + Space.m - mine.minX, HeaderView.inset) : HeaderView.inset
        if abs(lead.constant - inset) >= 0.5 { lead.constant = inset }
    }

    override var mouseDownCanMoveWindow: Bool {
        true
    }

    override func mouseDown(with event: NSEvent) {
        clicked?()
        super.mouseDown(with: event)
    }
}
