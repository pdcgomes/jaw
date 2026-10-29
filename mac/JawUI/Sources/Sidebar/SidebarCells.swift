import AppKit
import JawKit
import JawThemes

/// A folder's row: its symbol and its name, then, on the right, its ⌘ digit, an @ when it has
/// mentions and its unread total. VoiceOver reads it as one element.
final class FolderCell: NSTableCellView {
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
        Labels.style(name, .primary, roles: roles)
        Labels.style(digit, .tertiary, roles: roles)
        mention.textColor = roles.mention.nsColor
        Labels.style(count, .secondary, roles: roles)
        symbol.contentTintColor = Tone.secondary.color(roles).nsColor
    }

    /// A built-in folder's symbol, a smart folder's, or a folder of yours'.
    static func symbol(for row: FolderRow) -> String {
        builtInSymbols[row.id] ?? (row.isSmart ? smartSymbol : yoursSymbol)
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
