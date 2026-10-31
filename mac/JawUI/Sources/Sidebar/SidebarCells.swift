import AppKit
import JawKit
import JawThemes

/// A folder's row: its symbol and its name, then, on the right, its ⌘ digit, an @ when it has
/// mentions and its unread total. VoiceOver reads it as one element.
final class FolderCell: NSTableCellView, RowSelectable {
    static let id = NSUserInterfaceItemIdentifier("folder")
    static let builtInSymbols = ["all": "bubble.left.and.bubble.right", "unread": "message.badge", "groups": "person.3",
                                 "communities": "person.2.circle", "archived": "archivebox"]
    static let smartSymbol = "folder.badge.gearshape"
    static let yoursSymbol = "folder"
    static let symbolSize = NSImage.SymbolConfiguration(pointSize: TextStyle.body.size, weight: TextStyle.body.weight)
    /// The symbols' column, as wide as the widest of them, so every name on a level starts at the same place.
    static let symbolWidth = (builtInSymbols.values + [smartSymbol, yoursSymbol]).compactMap { image($0)?.size.width }.max() ?? 0

    let symbol = NSImageView()
    let name = Labels.make("", .body)
    let digit = Labels.make("", .key)
    let mention = Labels.make("@", .caption)
    let count = Labels.make("", .caption)
    var roles: Roles?

    /// A selected row in a tree with the keys is drawn on the system's accent, in the selected text's
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
        imageView = symbol
        textField = name
        symbol.imageAlignment = .alignCenter
        symbol.imageScaling = .scaleProportionallyDown
        mention.font = TextStyle.caption.emphasised
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let stack = NSStackView()
        stack.setViews([symbol, name], in: .leading)
        stack.setViews([digit, mention, count], in: .trailing)
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            symbol.widthAnchor.constraint(equalToConstant: Self.symbolWidth),
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

    /// The menu of the folder on this row, from the sidebar.
    var menuFor: ((FolderCell) -> NSMenu?)?
    var popUp = MenuPopUp()

    /// VoiceOver's Show Menu: the folder's menu under its row.
    override func accessibilityPerformShowMenu() -> Bool {
        popUp.show(menuFor?(self), at: NSPoint(x: bounds.minX, y: bounds.minY), in: self)
    }

    static func image(_ symbol: String) -> NSImage? {
        NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?.withSymbolConfiguration(symbolSize)
    }

    func show(_ row: FolderRow?, badges: FolderTree.Badges, roles: Roles?) {
        guard let row else { return }
        symbol.image = Self.image(Self.symbol(for: row))
        name.stringValue = row.name
        digit.stringValue = badges.digit.map { "⌘\($0)" } ?? ""
        digit.isHidden = badges.digit == nil
        mention.isHidden = !badges.mention
        count.stringValue = badges.unread > 0 ? Format.num(badges.unread) : ""
        count.isHidden = badges.unread == 0
        self.roles = roles
        colour()
        setAccessibilityLabel([row.name, badges.unread > 0 ? "\(badges.unread) unread" : nil, badges.mention ? "mentions you" : nil,
                               badges.digit.map { "favourite \($0)" }].compactMap(\.self).joined(separator: ", "))
    }

    func colour() {
        guard let roles else { return }
        if backgroundStyle == .emphasized {
            for label in [name, digit, mention, count] {
                label.textColor = .alternateSelectedControlTextColor
            }
            symbol.contentTintColor = .alternateSelectedControlTextColor
            return
        }
        // The selection's grey without the keys takes the quieter tones under 3:1 in the dark look.
        Labels.style(name, .primary, roles: roles)
        Labels.style(digit, selected ? .primary : .tertiary, roles: roles)
        mention.textColor = roles.mention.nsColor
        Labels.style(count, selected ? .primary : .secondary, roles: roles)
        symbol.contentTintColor = (selected ? Tone.primary : .secondary).color(roles).nsColor
    }

    /// A built-in folder's symbol, a smart folder's, or a folder of yours'.
    static func symbol(for row: FolderRow) -> String {
        builtInSymbols[row.id] ?? (row.isSmart ? smartSymbol : yoursSymbol)
    }
}

/// A folder's name being typed in place, and under it jawd's refusal of the name last asked for.
final class NamingCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("naming")
    let symbol = NSImageView()
    let field = NSTextField()
    let refused = Labels.make("", .caption)

    /// The row's height with jawd's refusal under the field, room for two lines of it.
    static let refusedHeight: CGFloat = {
        let field = NSTextField()
        field.font = TextStyle.body.nsFont
        let caption = TextStyle.caption.nsFont
        return ceil(field.intrinsicContentSize.height) + 2 * ceil(caption.ascender - caption.descender + caption.leading) + 3 * Space.xxs
    }()

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        symbol.image = FolderCell.image(FolderCell.yoursSymbol)
        symbol.imageAlignment = .alignCenter
        symbol.imageScaling = .scaleProportionallyDown
        field.font = TextStyle.body.nsFont
        field.placeholderString = "Folder name"
        field.lineBreakMode = .byTruncatingTail
        refused.maximumNumberOfLines = 2
        refused.lineBreakMode = .byWordWrapping
        let column = NSStackView(views: [field, refused])
        column.orientation = .vertical
        column.alignment = .leading
        column.spacing = Space.xxs
        let row = NSStackView(views: [symbol, column])
        row.spacing = Space.s
        row.alignment = .top
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            symbol.widthAnchor.constraint(equalToConstant: FolderCell.symbolWidth),
            row.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            row.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            row.topAnchor.constraint(equalTo: topAnchor, constant: Space.xxs),
            field.trailingAnchor.constraint(equalTo: row.trailingAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ naming: SidebarController.Naming, delegate: NSTextFieldDelegate, roles: Roles?) {
        // Set only when it differs, so a refusal drawn under a field being typed in leaves its caret be.
        if field.stringValue != naming.text { field.stringValue = naming.text }
        field.delegate = delegate
        field.setAccessibilityLabel(naming.renaming == nil ? "The new folder's name" : "The folder's new name")
        field.setAccessibilityHelp(naming.refused)
        refused.stringValue = naming.refused ?? ""
        refused.isHidden = naming.refused == nil
        guard let roles else { return }
        Labels.style(refused, .secondary, roles: roles)
        symbol.contentTintColor = Tone.secondary.color(roles).nsColor
    }
}

/// A cell whose row tells it whether it's selected, since AppKit tells a cell only of a selection
/// with the keys.
@MainActor
protocol RowSelectable: NSView {
    var selected: Bool { get set }
}

/// A row of the sidebar's, a folder's or someone's in the roster, which tells its cell whether it's
/// selected.
final class SidebarRowView: NSTableRowView {
    static let id = NSUserInterfaceItemIdentifier("sidebar.row")

    override init(frame: NSRect) {
        super.init(frame: frame)
        identifier = Self.id
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var isSelected: Bool {
        didSet {
            for case let cell as RowSelectable in subviews {
                cell.selected = isSelected
            }
        }
    }

    override func didAddSubview(_ subview: NSView) {
        super.didAddSubview(subview)
        (subview as? RowSelectable)?.selected = isSelected
    }
}

/// Folders or Smart Folders, over their rows; the roster's title too.
final class HeadingCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("heading")
    let title = Labels.make("", .caption)

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        title.font = TextStyle.caption.emphasised
        title.translatesAutoresizingMaskIntoConstraints = false
        addSubview(title)
        NSLayoutConstraint.activate([
            title.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            title.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Space.s),
            title.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ text: String, roles: Roles?) {
        title.stringValue = text
        if let roles { Labels.style(title, .tertiary, roles: roles) }
    }
}
