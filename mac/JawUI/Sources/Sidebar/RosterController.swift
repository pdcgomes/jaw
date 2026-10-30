import AppKit
import JawKit
import JawThemes

@MainActor
public protocol RosterHost: AnyObject {
    /// The roster scrolled near its end: its window should hold more people.
    func roster(_ roster: RosterController, move place: Place)
}

/// The picked folder's people, under the folder tree, as `RosterLines` lays them out: each with their
/// avatar, their name and how they are, faint where jawd isn't following them now; 200 at a time from
/// the top, more as it scrolls. Watching them is what tells jawd whose presence to follow.
@MainActor
public final class RosterController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
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
    let table = NSTableView()
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
        lines = RosterLines.lines(people.list, folder: people.folder, tree: tree, open: open, typing: typing)
        if isViewLoaded { table.reloadData() }
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
        let visible = table.rows(in: scroll.contentView.bounds)
        grow(visibleTo: visible.location + visible.length)
    }

    /// Asks for 200 more people, up to the 1,000 jawd keeps, once the last person held is on a line in
    /// sight, while the folder has more: with its folds closed the roster's lines are few, so the lines
    /// in sight say nothing of how near the end of its people it is.
    func grow(visibleTo end: Int) {
        guard UInt64(people.count) < people.total, people.place.after < Self.most, let last = people.list.last?.id,
              let at = lines.firstIndex(where: { if case let .person(p, _) = $0 { p.id == last } else { false } }), at < end
        else { return }
        let after = min(UInt32(people.count) + Self.rows, Self.most)
        if after > people.place.after { host?.roster(self, move: .top(after)) }
    }

    // MARK: The table

    public func numberOfRows(in _: NSTableView) -> Int {
        lines.count
    }

    public func tableView(_: NSTableView, isGroupRow row: Int) -> Bool {
        if case .title = lines[row] { true } else { false }
    }

    public func tableView(_: NSTableView, shouldSelectRow row: Int) -> Bool {
        if case .person = lines[row] { true } else { false }
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
            return cell
        case let .offline(_, count, open):
            let cell = (tableView.makeView(withIdentifier: GroupCell.id, owner: self) as? GroupCell) ?? GroupCell()
            cell.show("\(open ? "▾" : "▸") \(count) offline", "", roles: roles)
            return cell
        }
    }
}

/// Someone's row: a dot in the online colour while they're around, their avatar, their name and how
/// they are, all faint where jawd isn't following them now, unless they're typing, which shows only
/// while jawd hears it; their about text, when they have one, as its tooltip.
final class PersonCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("person")
    let dot = NSView()
    let avatar = AvatarView()
    let name = Labels.make("", .callout)
    let state = Labels.make("", .caption)

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
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ p: RosterRow, said: RosterLines.Said, avatar image: NSImage?, roles: Roles?) {
        name.stringValue = p.name
        state.stringValue = said.text
        toolTip = p.about
        setAccessibilityLabel([p.name, said.text].filter { !$0.isEmpty }.joined(separator: ", "))
        guard let roles else { return }
        // `said.online` is typing, from their chat.
        let faint = !p.presence.known && !said.online
        avatar.show(p.name, id: p.id, image: image, roles: roles)
        dot.layer?.backgroundColor = ((p.presence.around || said.online) && !faint ? roles.online : roles.online.opacity(0)).cgColor
        Labels.style(name, faint ? .tertiary : .primary, roles: roles)
        state.textColor = (faint ? roles.tertiary : said.online ? roles.online : roles.secondary).nsColor
    }
}

/// A group's heading under All, with who's around of how many, or a fold of people.
final class GroupCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("group")
    let title = Labels.make("", .caption)
    let count = Labels.make("", .caption)

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

    func show(_ text: String, _ counted: String, roles: Roles?) {
        title.stringValue = text
        count.stringValue = counted
        count.isHidden = counted.isEmpty
        guard let roles else { return }
        Labels.style(title, .secondary, roles: roles)
        Labels.style(count, .tertiary, roles: roles)
    }
}
