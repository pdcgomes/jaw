import AppKit
import JawKit
import JawThemes

@MainActor
public protocol RosterHost: AnyObject {
    /// The roster scrolled near its end: its window should hold more people.
    func roster(_ roster: RosterController, move place: Place)
    /// Someone double-clicked, or Return on them: their chat opened, or one started with them.
    func roster(_ roster: RosterController, open person: RosterRow)
    /// Someone's menu, made as it opens.
    func roster(_ roster: RosterController, menuItemsFor person: RosterRow) -> [NSMenuItem]
}

/// The picked folder's people, under the folder tree, as `RosterLines` lays them out: each with their
/// avatar, their name and how they are, faint where jawd isn't following them now; 200 at a time from
/// the top, more as it scrolls. Watching them is what tells jawd whose presence to follow.
@MainActor
public final class RosterController: NSViewController, NSTableViewDataSource, NSTableViewDelegate, NSMenuDelegate {
    public weak var host: RosterHost?
    public private(set) var people: People<RosterRow>
    /// The tree, which groups the people under All and names the folder.
    public var tree = FolderTree() {
        didSet { rebuild() }
    }

    /// Who's typing or recording in their one-to-one chat now, by person id, as the app reads it from
    /// who jawd says is typing in each chat.
    public var typing: Set<String> = [] {
        didSet { if typing != oldValue { rebuild() } }
    }

    public var roles: Roles? {
        didSet { if isViewLoaded { table.reloadData() } }
    }

    /// The root jawd's paths are relative to, for avatars.
    public var root: String?
    /// What time it is, for how long ago someone was seen; the harness and the tests set their own.
    public var now: @MainActor () -> Int64 = { Format.now() }
    var open: Set<String> = []
    var lines: [RosterLine] = []
    let table = RosterTable()
    let scroll = NSScrollView()
    var avatars = AvatarCache()
    /// Once a minute while it's on screen, how long ago people were seen is said again.
    var minute: Timer?

    /// How many people the window keeps from the top, grown as the roster scrolls.
    public static let rows: UInt32 = 200
    /// The most people jawd keeps in a roster's window after the top; it refuses more with -32602.
    public static let most: UInt32 = 1000

    public init(folder: String = "all") {
        people = People(folder: folder, place: .top(Self.rows))
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        let column = NSTableColumn(identifier: .init("person"))
        table.addTableColumn(column)
        table.headerView = nil
        table.style = .sourceList
        table.rowHeight = 26
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(clicked)
        table.roster = self
        table.doubleAction = #selector(doubled)
        table.setDraggingSourceOperationMask(.move, forLocal: true)
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        table.menu = menu
        table.setAccessibilityLabel("People")
        Scrolling.style(scroll)
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification,
                                               object: scroll.contentView)
        view = scroll
        rebuild()
    }

    override public func viewDidAppear() {
        super.viewDidAppear()
        minute?.invalidate()
        minute = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.table.reloadData() }
        }
    }

    override public func viewDidDisappear() {
        super.viewDidDisappear()
        minute?.invalidate()
        minute = nil
    }

    /// A new folder's people: its window starts again from the top, its folds closed.
    public func show(folder: String) {
        people = People(folder: folder, place: .top(Self.rows))
        open = []
        rebuild()
    }

    public func apply(_ update: Update<RosterRow>) {
        people.apply(update)
        rebuild()
    }

    /// Opens a fold to list the people in it, or closes it, as a click on it does.
    public func toggle(_ group: String) {
        if !open.insert(group).inserted { open.remove(group) }
        rebuild()
    }

    func rebuild() {
        let kept = selectedLine
        lines = RosterLines.lines(people.list, folder: people.folder, tree: tree, open: open, typing: typing)
        guard isViewLoaded else { return }
        table.reloadData()
        // Reloading keeps the selected row, not its line, so the selection follows someone as people
        // come, go and move, and goes when their line does.
        if let kept, let i = lines.firstIndex(where: { Self.key($0) == kept }) {
            table.selectRowIndexes([i], byExtendingSelection: false)
        } else if table.selectedRow >= 0 {
            table.deselectAll(nil)
        }
        // A fold opened, or people gone, can bring the last person held into sight with no scroll.
        grow()
    }

    /// The selected line, as `key` names it.
    var selectedLine: String? {
        let i = table.selectedRow
        return i >= 0 && i < lines.count ? Self.key(lines[i]) : nil
    }

    /// Someone's line by their group and id, as someone filed in two of your top folders has a line in
    /// each under All, or a fold's by its group; none for the title or a heading, which can't be
    /// selected.
    static func key(_ line: RosterLine) -> String? {
        switch line {
        case let .person(p, group): "\(group)/\(p.id)"
        case let .offline(group, _, _): "\(group)/offline"
        case .title, .heading: nil
        }
    }

    /// How someone is, typing… from their chat while it says so.
    func how(_ p: RosterRow) -> RosterLines.Said {
        RosterLines.said(p.presence, typing: typing.contains(p.id), now: now())
    }

    /// The roster as it's drawn, a line each, for the tests and the snapshot: its title with a colon, a
    /// group's heading with who's around of how many, someone with how they are and "(faint)" where
    /// jawd isn't following them and they aren't typing, and a fold with ▸ or ▾.
    public var said: [String] {
        lines.map { line in
            switch line {
            case let .title(text): "\(text):"
            case let .heading(name, around, of): "\(name) \(around) of \(of)"
            case let .person(p, _):
                [p.name, how(p).text, p.presence.known || typing.contains(p.id) ? "" : "(faint)"].filter { !$0.isEmpty }
                    .joined(separator: " ")
            case let .offline(_, count, open): "\(open ? "▾" : "▸") \(count) offline"
            }
        }
    }

    @objc func clicked() {
        let i = table.clickedRow
        guard i >= 0, i < lines.count, case let .offline(group, _, _) = lines[i] else { return }
        toggle(group)
    }

    @objc func scrolled() {
        grow()
    }

    /// Asks for more, as `grow(visibleTo:)` does, from the lines in sight.
    func grow() {
        let visible = table.rows(in: scroll.contentView.bounds)
        guard visible.length > 0 else { return }
        grow(visibleTo: visible.location + visible.length)
    }

    /// Asks for 200 more people, up to the 1,000 jawd keeps, once the last person held is on a line in
    /// sight, while the folder has more: with its folds closed the roster's lines are few, so the lines
    /// in sight say nothing of how near the end of its people it is.
    func grow(visibleTo end: Int) {
        guard UInt64(people.count) < people.total, people.place.after < Self.most, let last = people.last?.id,
              let at = lines.firstIndex(where: { if case let .person(p, _) = $0 { p.id == last } else { false } }), at < end
        else { return }
        let after = min(UInt32(people.count) + Self.rows, Self.most)
        if after > people.place.after { host?.roster(self, move: .top(after)) }
    }

    /// Someone on row `i`, when it's someone's row.
    func person(at i: Int) -> RosterRow? {
        guard i >= 0, i < lines.count, case let .person(p, _) = lines[i] else { return nil }
        return p
    }

    @objc func doubled() {
        if let p = person(at: table.clickedRow) { host?.roster(self, open: p) }
    }

    /// Return on someone selected opens their chat, as a double click does.
    func openSelected() {
        if let p = person(at: table.selectedRow) { host?.roster(self, open: p) }
    }

    /// A key on the selected line, by its code: Return on someone opens their chat; on a fold, Return or
    /// Space opens or closes it, → opens it and ← closes it, as an outline's triangles take them. Says
    /// whether it did something.
    func pressed(_ key: UInt16) -> Bool {
        let i = table.selectedRow
        guard i >= 0, i < lines.count else { return false }
        switch (lines[i], key) {
        case (.person, 36), (.person, 76):
            openSelected()
        case let (.offline(group, _, _), 36), let (.offline(group, _, _), 76), let (.offline(group, _, _), 49),
             let (.offline(group, _, false), 124), let (.offline(group, _, true), 123):
            toggle(group)
        default:
            return false
        }
        return true
    }

    public func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        for item in menuItems(forRow: table.clickedRow) {
            menu.addItem(item)
        }
    }

    /// The menu of the person on row `i`; none on a title, a heading or a fold.
    public func menuItems(forRow i: Int) -> [NSMenuItem] {
        person(at: i).map { host?.roster(self, menuItemsFor: $0) ?? [] } ?? []
    }

    /// Someone's menu for VoiceOver's Show Menu, which makes no click for the table's menu to be filled for.
    func menu(for cell: NSView) -> NSMenu? {
        let items = menuItems(forRow: table.row(for: cell))
        guard !items.isEmpty else { return nil }
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in items {
            menu.addItem(item)
        }
        return menu
    }

    /// Someone dragged onto a folder of yours, which files them there.
    public func tableView(_: NSTableView, pasteboardWriterForRow row: Int) -> (any NSPasteboardWriting)? {
        guard let p = person(at: row) else { return nil }
        let item = NSPasteboardItem()
        item.setString(p.id, forType: .jawPerson)
        item.setString(p.name, forType: .string)
        return item
    }

    // MARK: The table

    public func numberOfRows(in _: NSTableView) -> Int {
        lines.count
    }

    public func tableView(_: NSTableView, isGroupRow row: Int) -> Bool {
        if case .title = lines[row] { true } else { false }
    }

    /// Someone, or a fold, so the keys reach it.
    public func tableView(_: NSTableView, shouldSelectRow row: Int) -> Bool {
        switch lines[row] {
        case .person, .offline: true
        case .title, .heading: false
        }
    }

    public func tableView(_ tableView: NSTableView, viewFor _: NSTableColumn?, row: Int) -> NSView? {
        switch lines[row] {
        case let .title(text):
            let cell = (tableView.makeView(withIdentifier: HeadingCell.id, owner: self) as? HeadingCell) ?? HeadingCell()
            cell.show(text, roles: roles)
            return cell
        case let .heading(name, around, of):
            let cell = (tableView.makeView(withIdentifier: GroupCell.id, owner: self) as? GroupCell) ?? GroupCell()
            cell.show(name, "\(around) of \(of)", roles: roles)
            return cell
        case let .person(p, _):
            let cell = (tableView.makeView(withIdentifier: PersonCell.id, owner: self) as? PersonCell) ?? PersonCell()
            cell.show(p, said: how(p), avatar: avatars.image(p.avatar, root: root), roles: roles)
            cell.menuFor = { [weak self] cell in self?.menu(for: cell) }
            return cell
        case let .offline(group, count, open):
            let cell = (tableView.makeView(withIdentifier: GroupCell.id, owner: self) as? GroupCell) ?? GroupCell()
            // Under All, whose fold it is: the group's heading is a line of its own.
            let whose = people.folder != "all" ? "" : " in " + (group == "others" ? "Others" : tree.label(group))
            cell.show("\(open ? "▾" : "▸") \(count) offline", "", roles: roles,
                      fold: GroupCell.Fold(open: open, said: "\(count) offline\(whose)") { [weak self] in self?.toggle(group) })
            return cell
        }
    }

    public func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        if case .title = lines[row] { return nil }
        return (tableView.makeView(withIdentifier: SidebarRowView.id, owner: self) as? SidebarRowView) ?? SidebarRowView()
    }
}

/// Someone's row: a dot in the online colour while they're around, their avatar, their name and how
/// they are, all faint where jawd isn't following them now, unless they're typing, which shows only
/// while jawd hears it; their about text, when they have one, as its tooltip. VoiceOver reads it as
/// one element, saying "last known" after how they are where it's faint.
final class PersonCell: NSTableCellView, RowSelectable {
    static let id = NSUserInterfaceItemIdentifier("person")
    let dot = NSView()
    let avatar = AvatarView()
    let name = Labels.make("", .callout)
    let state = Labels.make("", .caption)
    /// Their menu, from the roster.
    var menuFor: ((PersonCell) -> NSMenu?)?
    var popUp = MenuPopUp()
    var roles: Roles?
    /// How the row is drawn: faint, the dot shown, and how they are in the online colour, for typing.
    var faint = false, around = false, typing = false

    /// A selected row in a roster with the keys is drawn on the system's accent, in the selected text's
    /// colour; any other row in the theme's.
    override var backgroundStyle: NSView.BackgroundStyle {
        didSet { colour() }
    }

    /// Whether its row is selected, which its row says, since AppKit tells a cell only of a selection
    /// with the keys.
    var selected = false {
        didSet { if selected != oldValue { colour() } }
    }

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        dot.wantsLayer = true
        dot.layer?.cornerRadius = Radius.small
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let stack = NSStackView(views: [dot, avatar, name, state])
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            dot.widthAnchor.constraint(equalToConstant: 2 * Radius.small),
            dot.heightAnchor.constraint(equalToConstant: 2 * Radius.small),
            avatar.widthAnchor.constraint(equalToConstant: 18),
            avatar.heightAnchor.constraint(equalToConstant: 18),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Its label says all of it, so nothing in it is read again.
    override func accessibilityChildren() -> [Any]? {
        []
    }

    /// VoiceOver's Show Menu: their menu under their row.
    override func accessibilityPerformShowMenu() -> Bool {
        popUp.show(menuFor?(self), at: NSPoint(x: bounds.minX, y: bounds.minY), in: self)
    }

    func show(_ p: RosterRow, said: RosterLines.Said, avatar image: NSImage?, roles: Roles?) {
        name.stringValue = p.name
        state.stringValue = said.text
        toolTip = p.about
        // `said.online` is typing, from their chat.
        faint = !p.presence.known && !said.online
        around = (p.presence.around || said.online) && !faint
        typing = said.online
        setAccessibilityLabel([p.name, said.text, faint && !said.text.isEmpty ? "last known" : ""].filter { !$0.isEmpty }
            .joined(separator: ", "))
        self.roles = roles
        guard let roles else { return }
        avatar.show(p.name, id: p.id, image: image, roles: roles)
        colour()
    }

    func colour() {
        guard let roles else { return }
        if backgroundStyle == .emphasized {
            for label in [name, state] {
                label.textColor = .alternateSelectedControlTextColor
            }
            dot.layer?.backgroundColor = (around ? NSColor.alternateSelectedControlTextColor : .clear).cgColor
            return
        }
        // The selection's grey without the keys takes the faint and quieter tones under 3:1, and the
        // online colour under 4.5:1 in the light look, so there the words take the name's tone.
        Labels.style(name, faint && !selected ? .tertiary : .primary, roles: roles)
        state.textColor = (selected ? roles.text : faint ? roles.tertiary : typing ? roles.online : roles.secondary).nsColor
        dot.layer?.backgroundColor = (around ? roles.online : roles.online.opacity(0)).cgColor
    }
}

/// A group's heading under All, with who's around of how many, or a fold of people, which VoiceOver
/// reads as a disclosure triangle, open or not, that a press opens or closes.
final class GroupCell: NSTableCellView, RowSelectable {
    static let id = NSUserInterfaceItemIdentifier("group")
    let title = Labels.make("", .caption)
    let count = Labels.make("", .caption)
    var roles: Roles?
    /// A fold's: whether it's open, what VoiceOver says of it, and what opens or closes it.
    var fold: Fold?

    struct Fold {
        let open: Bool
        let said: String
        let toggle: @MainActor () -> Void
    }

    /// A selected fold in a roster with the keys is drawn on the system's accent, in the selected text's
    /// colour; any other in the theme's.
    override var backgroundStyle: NSView.BackgroundStyle {
        didSet { colour() }
    }

    /// Whether its row is selected, which its row says, since AppKit tells a cell only of a selection
    /// with the keys.
    var selected = false {
        didSet { if selected != oldValue { colour() } }
    }

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        title.font = TextStyle.caption.emphasised
        let stack = NSStackView(views: [title, count])
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Space.s),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ text: String, _ counted: String, roles: Roles?, fold: Fold? = nil) {
        title.stringValue = text
        count.stringValue = counted
        count.isHidden = counted.isEmpty
        self.fold = fold
        setAccessibilityElement(fold != nil)
        setAccessibilityRole(fold == nil ? nil : .disclosureTriangle)
        setAccessibilityLabel(fold?.said)
        setAccessibilityValue(fold.map { NSNumber(value: $0.open ? 1 : 0) })
        self.roles = roles
        colour()
    }

    func colour() {
        guard let roles else { return }
        if backgroundStyle == .emphasized {
            for label in [title, count] {
                label.textColor = .alternateSelectedControlTextColor
            }
            return
        }
        // The selection's grey without the keys takes the quieter tones under 3:1 in the dark look.
        Labels.style(title, selected ? .primary : .secondary, roles: roles)
        Labels.style(count, selected ? .primary : .tertiary, roles: roles)
    }

    /// A fold's label says all of it, so nothing in it is read again.
    override func accessibilityChildren() -> [Any]? {
        fold == nil ? super.accessibilityChildren() : []
    }

    /// VoiceOver's press on a fold: opened, or closed.
    override func accessibilityPerformPress() -> Bool {
        guard let fold else { return false }
        fold.toggle()
        return true
    }
}

/// The roster's table: Return opens the selected person's chat, and Return, Space, → and ← open and
/// close the selected fold.
final class RosterTable: NSTableView {
    weak var roster: RosterController?

    override func keyDown(with event: NSEvent) {
        if event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty, let roster, roster.pressed(event.keyCode) {
            return
        }
        super.keyDown(with: event)
    }
}
