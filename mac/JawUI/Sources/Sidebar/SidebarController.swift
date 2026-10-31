import AppKit
import JawKit
import JawThemes

@MainActor
public protocol SidebarHost: AnyObject {
    /// A folder picked in the sidebar: the chat list becomes its window.
    func sidebar(_ sidebar: SidebarController, pick folder: String)
    /// Return in the name field: make the folder, or rename it, with `name`.
    func sidebar(_ sidebar: SidebarController, name: String, as naming: SidebarController.Naming)
    /// The menu of a folder right-clicked, made as it opens; for nil, on a heading or the space under
    /// the rows, the sidebar's own.
    func sidebar(_ sidebar: SidebarController, menuFor folder: String?) -> [NSMenuItem]
    /// Something dropped on the tree: what it does, and the name of what was dropped, for what's said.
    func sidebar(_ sidebar: SidebarController, do act: FolderAct, name: String)
}

/// The sidebar: the folder tree, as jawd's M6 serves it, on the sidebar's glass. The built-in folders
/// come first, without a heading; then Folders, yours, nested, each opened and closed with its
/// triangle; then Smart Folders. Each row has its unread total (none for Archived), an @ when it has
/// mentions, and its ⌘ digit when it's a favourite. Picking a row makes the chat list its window.
/// Until jawd serves folders it has All and Archived, as jawd serves them now.
@MainActor
public final class SidebarController: NSViewController, NSOutlineViewDataSource, NSOutlineViewDelegate, NSSplitViewDelegate {
    public weak var host: SidebarHost?
    public internal(set) var tree = FolderTree()
    /// How many times the tree's shape or slots have changed, which what the menus are made from
    /// follows, so the app makes its folder items again only then and not with every push of counts.
    public internal(set) var shapeChanges = 0
    /// The folder the chat list shows, selected whenever its row is in sight.
    public internal(set) var picked = "all"
    public var roles: Roles? {
        didSet {
            roster.roles = roles
            guard isViewLoaded else { return }
            keepingNaming {
                outline.reloadData()
                restoreOpen()
            }
        }
    }

    /// The defaults key what's open, the headings and your folders, is kept under from launch to launch:
    /// the app gives one, the harness and the tests none, so what they draw doesn't depend on what was
    /// opened before.
    public var keepsOpen: String? {
        didSet {
            guard let keepsOpen, let kept = UserDefaults.standard.stringArray(forKey: keepsOpen) else { return }
            open = Set(kept)
            if isViewLoaded { restoreOpen() }
        }
    }

    let outline = FolderOutline()
    let scroll = NSScrollView()
    /// One object for each row's id for as long as the sidebar lives: the outline knows its rows by
    /// identity, so what's open stays open as the tree is laid out again.
    var items: [String: Item] = [:]
    /// What's open: the headings until they're closed by hand, and your folders left open.
    var open: Set<String> = [SidebarController.foldersHeading, SidebarController.smartHeading]
    /// The picked folder's people, under the tree.
    public let roster = RosterController()
    let split = NSSplitView()
    /// The defaults key the divider is kept under from launch to launch, as the share of the height the
    /// tree has: the app gives one, the harness and the tests none.
    public var keepsDivider: String?
    var placed = false
    /// A folder being named in place, while its field is open.
    public internal(set) var naming: Naming?
    /// While the field is drawn again, its editing ending isn't the field given up.
    var keepingField = false
    static let namingRow = "naming"
    /// Says words to VoiceOver as they come, as jawd's refusal of a name; the tests listen in.
    var announce: @MainActor (String) -> Void = { words in
        NSAccessibility.post(element: NSApplication.shared, notification: .announcementRequested,
                             userInfo: [.announcement: words, .priority: NSAccessibilityPriorityLevel.high.rawValue])
    }

    final class Item: NSObject {
        let id: String

        init(_ id: String) {
            self.id = id
        }
    }

    static let foldersHeading = "heading:folders"
    static let smartHeading = "heading:smart"

    override public func loadView() {
        let column = NSTableColumn(identifier: .init("folder"))
        outline.addTableColumn(column)
        outline.outlineTableColumn = column
        outline.headerView = nil
        outline.style = .sourceList
        outline.rowHeight = 28
        outline.floatsGroupRows = false
        outline.dataSource = self
        outline.delegate = self
        outline.sidebar = self
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        outline.menu = menu
        outline.registerForDraggedTypes([.jawChat, .jawCommunity, .jawPerson, .jawFolder])
        outline.setDraggingSourceOperationMask(.move, forLocal: true)
        outline.setAccessibilityLabel("Folders")
        Scrolling.style(scroll)
        scroll.documentView = outline
        scroll.drawsBackground = false
        split.isVertical = false
        split.dividerStyle = .thin
        split.delegate = self
        split.addArrangedSubview(scroll)
        addChild(roster)
        split.addArrangedSubview(roster.view)
        roster.view.isHidden = !tree.served
        view = split
        outline.reloadData()
        restoreOpen()
    }

    // MARK: The tree

    /// jawd's tree, changed. A row whose counts alone changed is drawn again where it is; a change to
    /// the tree's shape lays it out again, keeping what's open, so a flood of messages costs the
    /// sidebar a few rows' drawing. A name being typed keeps its field, its text and its caret, until
    /// the folder it names, or the one a new folder is being made in, is gone.
    public func apply(_ update: Update<FolderRow>) {
        let before = tree
        tree.apply(update)
        if let n = naming, let gone = n.renaming ?? n.parent, tree.row(gone) == nil { naming = nil }
        let reshaped = before.shape != tree.shape || before.served != tree.served
        if reshaped || before.favourites != tree.favourites { shapeChanges += 1 }
        guard isViewLoaded else { return }
        if reshaped {
            keepingNaming {
                outline.reloadData()
                restoreOpen()
            }
            // The roster groups and names by the tree's shape, so a flood's counts don't draw it again.
            roster.tree = tree
            roster.view.isHidden = !tree.served
            view.needsLayout = true
            return
        }
        // Drawing a row again replaces its cell, which would end the editing in it, so the folder being
        // renamed keeps its counts until its field closes.
        for id in tree.rows.keys where id != naming?.renaming && (before.row(id) != tree.row(id) || before.badges(id) != tree.badges(id)) {
            if let item = items[id] { outline.reloadItem(item) }
        }
    }

    /// The app picked `folder`, from a key, a favourite or the palette: its row is selected, the
    /// heading and the folders it's under opened, and the host isn't asked again.
    public func select(_ folder: String) {
        picked = folder
        guard isViewLoaded else { return }
        for id in above(folder) {
            outline.expandItem(item(id))
        }
        reselect()
    }

    /// The folder whose row is selected: the picked one whenever its row is in sight.
    public var selected: String? {
        (outline.item(atRow: outline.selectedRow) as? Item)?.id
    }

    /// Picks the folder on `row` as a click does, the tree taking the keys; false when the row isn't a
    /// folder.
    @discardableResult
    public func pick(row: Int) -> Bool {
        guard row >= 0, let id = (outline.item(atRow: row) as? Item)?.id, tree.row(id) != nil else { return false }
        outline.window?.makeFirstResponder(outline)
        outline.selectRowIndexes([row], byExtendingSelection: false)
        return true
    }

    /// Opens a folder of yours to show the folders in it, its heading and the ones it's in first.
    public func expand(_ id: String) {
        for a in above(id) + [id] {
            outline.expandItem(item(a))
        }
    }

    /// What a folder is under, outermost first: its heading, when it has one, then the folders it's in.
    func above(_ id: String) -> [String] {
        guard let row = tree.row(id) else { return [] }
        let heading = row.isYours ? [Self.foldersHeading] : row.isSmart ? [Self.smartHeading] : []
        return heading + tree.ancestors(id).reversed()
    }

    /// The tree as it's drawn, a row a line, for the tests and the snapshot: a heading with a colon, a
    /// folder indented under the one it's in, then its ⌘ digit, an @, its unread total, and ▸ or ▾ when
    /// it has folders in it.
    public var said: [String] {
        (0 ..< outline.numberOfRows).compactMap { row in
            guard let id = (outline.item(atRow: row) as? Item)?.id else { return nil }
            if id == Self.foldersHeading { return "Folders:" }
            if id == Self.smartHeading { return "Smart Folders:" }
            if id == Self.namingRow || id == naming?.renaming, let naming {
                let indent = String(repeating: "  ", count: max(outline.level(forRow: row) - 1, 0))
                return indent + "[\(naming.text)]" + (naming.refused.map { " · \($0)" } ?? "")
            }
            guard let folder = tree.row(id) else { return nil }
            let b = tree.badges(id)
            var line = String(repeating: "  ", count: max(outline.level(forRow: row) - 1, 0)) + folder.name
            if let digit = b.digit { line += " ⌘\(digit)" }
            if b.mention { line += " @" }
            if b.unread > 0 { line += " \(Format.num(b.unread))" }
            if outline.isExpandable(items[id]) { line += outline.isItemExpanded(items[id]) ? " ▾" : " ▸" }
            return line
        }
    }

    /// The divider under the tree, where it was left, or at 60% of the height, once the roster shows.
    override public func viewDidLayout() {
        super.viewDidLayout()
        guard !placed, split.bounds.height > 0, !roster.view.isHidden else { return }
        placed = true
        let kept = keepsDivider.map { UserDefaults.standard.double(forKey: $0) } ?? 0
        split.setPosition(split.bounds.height * (kept > 0 ? kept : 0.6), ofDividerAt: 0)
    }

    public func splitViewDidResizeSubviews(_: Notification) {
        guard placed, let keepsDivider, split.bounds.height > 0, !roster.view.isHidden else { return }
        UserDefaults.standard.set(scroll.frame.height / split.bounds.height, forKey: keepsDivider)
    }

    /// Whether the tree has the keys, as ⌘⌫ needs it to.
    public var hasKeys: Bool {
        isViewLoaded && view.window?.firstResponder === outline
    }

    /// Whether it's on screen: in a window, and not folded away.
    public var isShown: Bool {
        isViewLoaded && view.window != nil && !view.isHiddenOrHasHiddenAncestor && view.frame.width >= 1
    }

    func reselect() {
        let row = outline.row(forItem: items[picked])
        if row >= 0 {
            outline.selectRowIndexes([row], byExtendingSelection: false)
        } else if outline.selectedRow >= 0 {
            outline.deselectAll(nil)
        }
    }

    /// The headings and your folders open as they were left, and the picked folder selected.
    func restoreOpen() {
        for id in [Self.foldersHeading, Self.smartHeading] + tree.yours.map(\.row.id) where open.contains(id) {
            outline.expandItem(item(id))
        }
        reselect()
    }

    // MARK: The outline

    func item(_ id: String) -> Item {
        if let held = items[id] { return held }
        let made = Item(id)
        items[id] = made
        return made
    }

    static func isHeading(_ id: String?) -> Bool {
        id == foldersHeading || id == smartHeading
    }

    /// The ids under `parent`: the built-in folders and the headings at the top, your folders under
    /// Folders and under each other, the smart ones under Smart Folders; and a new folder's field last
    /// where it will go, Folders shown for one at the top before you have any.
    func children(_ parent: String?) -> [String] {
        let new = naming.map { $0.renaming == nil } ?? false
        var out: [String] = switch parent {
        case nil:
            tree.builtIn.map(\.id) + (tree.children(of: nil).isEmpty && !(new && naming?.parent == nil) ? [] : [Self.foldersHeading])
                + (tree.smart.isEmpty ? [] : [Self.smartHeading])
        case .some(Self.foldersHeading): tree.children(of: nil).map(\.id)
        case .some(Self.smartHeading): tree.smart.map(\.id)
        case let .some(id): tree.children(of: id).map(\.id)
        }
        if new, let parent, parent == (naming?.parent ?? Self.foldersHeading) { out.append(Self.namingRow) }
        return out
    }

    public func outlineView(_: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        children((item as? Item)?.id).count
    }

    public func outlineView(_: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        self.item(children((item as? Item)?.id)[index])
    }

    public func outlineView(_: NSOutlineView, isItemExpandable item: Any) -> Bool {
        !children((item as? Item)?.id).isEmpty
    }

    public func outlineView(_: NSOutlineView, isGroupItem item: Any) -> Bool {
        Self.isHeading((item as? Item)?.id)
    }

    public func outlineView(_: NSOutlineView, shouldSelectItem item: Any) -> Bool {
        let id = (item as? Item)?.id
        return !Self.isHeading(id) && id != Self.namingRow
    }

    /// A heading's height: AppKit's own for a source list's group rows, which a delegate that gives
    /// rows their heights has to give too.
    static let headingHeight: CGFloat = 19

    public func outlineView(_: NSOutlineView, heightOfRowByItem item: Any) -> CGFloat {
        let id = (item as? Item)?.id
        if Self.isHeading(id) { return Self.headingHeight }
        return (id == Self.namingRow || id == naming?.renaming) && naming?.refused != nil ? NamingCell.refusedHeight : outline.rowHeight
    }

    public func outlineView(_ outlineView: NSOutlineView, viewFor _: NSTableColumn?, item: Any) -> NSView? {
        guard let id = (item as? Item)?.id else { return nil }
        if id == Self.namingRow || id == naming?.renaming, let naming {
            let cell = (outlineView.makeView(withIdentifier: NamingCell.id, owner: self) as? NamingCell) ?? NamingCell()
            cell.show(naming, delegate: self, roles: roles)
            return cell
        }
        if Self.isHeading(id) {
            let cell = (outlineView.makeView(withIdentifier: HeadingCell.id, owner: self) as? HeadingCell) ?? HeadingCell()
            cell.show(id == Self.foldersHeading ? "Folders" : "Smart Folders", roles: roles)
            return cell
        }
        let cell = (outlineView.makeView(withIdentifier: FolderCell.id, owner: self) as? FolderCell) ?? FolderCell()
        cell.show(tree.row(id), badges: tree.badges(id), roles: roles)
        cell.menuFor = { [weak self] cell in self?.menu(for: cell) }
        return cell
    }

    public func outlineView(_ outlineView: NSOutlineView, rowViewForItem item: Any) -> NSTableRowView? {
        guard !Self.isHeading((item as? Item)?.id) else { return nil }
        return (outlineView.makeView(withIdentifier: SidebarRowView.id, owner: self) as? SidebarRowView) ?? SidebarRowView()
    }

    public func outlineViewSelectionDidChange(_: Notification) {
        guard let id = selected else { return reselect() }
        guard id != picked, tree.row(id) != nil else { return }
        picked = id
        host?.sidebar(self, pick: id)
    }

    public func outlineViewItemDidCollapse(_ note: Notification) {
        keep(note, open: false)
        reselect()
    }

    public func outlineViewItemDidExpand(_ note: Notification) {
        keep(note, open: true)
        reselect()
    }

    func keep(_ note: Notification, open now: Bool) {
        guard let id = (note.userInfo?["NSObject"] as? Item)?.id, Self.isHeading(id) || tree.row(id)?.isYours == true else { return }
        let changed = now ? open.insert(id).inserted : open.remove(id) != nil
        if changed, let keepsOpen { UserDefaults.standard.set(open.sorted(), forKey: keepsOpen) }
    }
}

/// The tree's outline: Return on a folder of yours renames it, as Finder's does, and Esc drops a name
/// left in the tree without the keys, as one jawd turned down after a click away.
final class FolderOutline: NSOutlineView {
    weak var sidebar: SidebarController?

    override func keyDown(with event: NSEvent) {
        let plain = event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty
        if event.keyCode == 53, plain, let sidebar, sidebar.naming?.asked == false {
            sidebar.cancelNaming()
            return
        }
        if event.keyCode == 36 || event.keyCode == 76, plain, let sidebar, sidebar.naming == nil, sidebar.tree.row(sidebar.picked)?.isYours == true {
            sidebar.beginRenaming(sidebar.picked)
            return
        }
        super.keyDown(with: event)
    }
}
