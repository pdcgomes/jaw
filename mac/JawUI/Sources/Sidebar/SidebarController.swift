import AppKit
import JawKit
import JawThemes

@MainActor
public protocol SidebarHost: AnyObject {
    /// A folder picked in the sidebar: the chat list becomes its window.
    func sidebar(_ sidebar: SidebarController, pick folder: String)
}

/// The sidebar: the folder tree, as jawd's M6 serves it, on the sidebar's glass. The built-in folders
/// come first, without a heading; then Folders, yours, nested, each opened and closed with its
/// triangle; then Smart Folders. Each row has its unread total (none for Archived), an @ when it has
/// mentions, and its ⌘ digit when it's a favourite. Picking a row makes the chat list its window.
/// Until jawd serves folders it has All and Archived, as jawd serves them now.
@MainActor
public final class SidebarController: NSViewController, NSOutlineViewDataSource, NSOutlineViewDelegate {
    public weak var host: SidebarHost?
    public internal(set) var tree = FolderTree()
    /// The folder the chat list shows, selected whenever its row is in sight.
    public internal(set) var picked = "all"
    public var roles: Roles? {
        didSet {
            guard isViewLoaded else { return }
            outline.reloadData()
            restoreOpen()
        }
    }

    /// The defaults key your folders left open are kept under from launch to launch: the app gives one,
    /// the harness and the tests none, so what they draw doesn't depend on what was opened before.
    public var keepsOpen: String? {
        didSet {
            guard let keepsOpen, let kept = UserDefaults.standard.stringArray(forKey: keepsOpen) else { return }
            open = Set(kept)
            if isViewLoaded { restoreOpen() }
        }
    }

    let outline = NSOutlineView()
    let scroll = NSScrollView()
    /// One object for each row's id for as long as the sidebar lives: the outline knows its rows by
    /// identity, so what's open stays open as the tree is laid out again.
    var items: [String: Item] = [:]
    /// Your folders left open.
    var open: Set<String> = []
    /// While a folder closes, the selection AppKit moves isn't a pick.
    var collapsing = false

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
        outline.setAccessibilityLabel("Folders")
        Scrolling.style(scroll)
        scroll.documentView = outline
        scroll.drawsBackground = false
        view = scroll
        outline.reloadData()
        restoreOpen()
    }

    // MARK: The tree

    /// jawd's tree, changed. A row whose counts alone changed is drawn again where it is; a change to
    /// the tree's shape lays it out again, keeping what's open, so a flood of messages costs the
    /// sidebar a few rows' drawing.
    public func apply(_ update: Update<FolderRow>) {
        let before = tree
        tree.apply(update)
        guard isViewLoaded else { return }
        if before.shape != tree.shape || before.served != tree.served {
            outline.reloadData()
            restoreOpen()
            return
        }
        for id in tree.rows.keys where before.row(id) != tree.row(id) || before.badges(id) != tree.badges(id) {
            if let item = items[id] { outline.reloadItem(item) }
        }
    }

    /// The app picked `folder`, from a key, a favourite or the palette: its row is selected, the
    /// folders it's in opened, and the host isn't asked again.
    public func select(_ folder: String) {
        picked = folder
        guard isViewLoaded else { return }
        for id in tree.ancestors(folder).reversed() {
            outline.expandItem(item(id))
        }
        reselect()
    }

    /// Picks the folder on `row`, as a click does; false when the row isn't a folder.
    @discardableResult
    public func pick(row: Int) -> Bool {
        guard row >= 0, let id = (outline.item(atRow: row) as? Item)?.id, tree.row(id) != nil else { return false }
        outline.selectRowIndexes([row], byExtendingSelection: false)
        return true
    }

    /// Opens a folder of yours to show the folders in it, the ones it's in first.
    public func expand(_ id: String) {
        for a in tree.ancestors(id).reversed() + [id] {
            outline.expandItem(item(a))
        }
    }

    /// The tree as it's drawn, a row a line, for the tests and the snapshot: a heading with a colon, a
    /// folder indented under the one it's in, then its ⌘ digit, an @, its unread total, and ▸ or ▾ when
    /// it has folders in it.
    public var said: [String] {
        (0 ..< outline.numberOfRows).compactMap { row in
            guard let id = (outline.item(atRow: row) as? Item)?.id else { return nil }
            if id == Self.foldersHeading { return "Folders:" }
            if id == Self.smartHeading { return "Smart Folders:" }
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

    func reselect() {
        let row = outline.row(forItem: items[picked])
        if row >= 0 {
            outline.selectRowIndexes([row], byExtendingSelection: false)
        } else {
            outline.deselectAll(nil)
        }
    }

    /// The headings open, your folders as they were left, and the picked folder selected.
    func restoreOpen() {
        for heading in [Self.foldersHeading, Self.smartHeading] {
            outline.expandItem(item(heading))
        }
        for (row, _) in tree.yours where open.contains(row.id) {
            outline.expandItem(item(row.id))
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
    /// Folders and under each other, the smart ones under Smart Folders.
    func children(_ parent: String?) -> [String] {
        switch parent {
        case nil:
            tree.builtIn.map(\.id) + (tree.children(of: nil).isEmpty ? [] : [Self.foldersHeading])
                + (tree.smart.isEmpty ? [] : [Self.smartHeading])
        case .some(Self.foldersHeading): tree.children(of: nil).map(\.id)
        case .some(Self.smartHeading): tree.smart.map(\.id)
        case let .some(id): tree.children(of: id).map(\.id)
        }
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
        !Self.isHeading((item as? Item)?.id)
    }

    public func outlineView(_ outlineView: NSOutlineView, viewFor _: NSTableColumn?, item: Any) -> NSView? {
        guard let id = (item as? Item)?.id else { return nil }
        if Self.isHeading(id) {
            let cell = (outlineView.makeView(withIdentifier: HeadingCell.id, owner: self) as? HeadingCell) ?? HeadingCell()
            cell.show(id == Self.foldersHeading ? "Folders" : "Smart Folders", roles: roles)
            return cell
        }
        let cell = (outlineView.makeView(withIdentifier: FolderCell.id, owner: self) as? FolderCell) ?? FolderCell()
        cell.show(tree.row(id), badges: tree.badges(id), roles: roles)
        return cell
    }

    public func outlineViewSelectionDidChange(_: Notification) {
        guard !collapsing, let id = (outline.item(atRow: outline.selectedRow) as? Item)?.id, id != picked, tree.row(id) != nil else {
            return
        }
        picked = id
        host?.sidebar(self, pick: id)
    }

    public func outlineViewItemWillCollapse(_: Notification) {
        collapsing = true
    }

    public func outlineViewItemDidCollapse(_ note: Notification) {
        collapsing = false
        keep(note, open: false)
        reselect()
    }

    public func outlineViewItemDidExpand(_ note: Notification) {
        keep(note, open: true)
    }

    func keep(_ note: Notification, open now: Bool) {
        guard let id = (note.userInfo?["NSObject"] as? Item)?.id, tree.row(id)?.isYours == true else { return }
        if now { open.insert(id) } else { open.remove(id) }
        if let keepsOpen { UserDefaults.standard.set(open.sorted(), forKey: keepsOpen) }
    }
}
