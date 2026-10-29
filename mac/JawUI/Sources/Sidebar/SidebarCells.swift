import AppKit
import JawKit
import JawThemes

/// A folder's row: its symbol and its name, then, on the right, its ⌘ digit, an @ when it has
/// mentions and its unread total.
final class FolderCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("folder")
    let symbol = NSImageView()
    let name = Labels.make("", .body)
    let digit = Labels.make("", .caption)
    let mention = Labels.make("@", .caption)
    let count = Labels.make("", .caption)

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        imageView = symbol
        textField = name
        mention.font = TextStyle.caption.emphasised
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let stack = NSStackView()
        stack.setViews([symbol, name], in: .leading)
        stack.setViews([digit, mention, count], in: .trailing)
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ row: FolderRow?, badges: FolderTree.Badges, roles: Roles?) {
        guard let row else { return }
        symbol.image = NSImage(systemSymbolName: Self.symbol(for: row), accessibilityDescription: nil)
        name.stringValue = row.name
        digit.stringValue = badges.digit.map { "⌘\($0)" } ?? ""
        digit.isHidden = badges.digit == nil
        mention.isHidden = !badges.mention
        count.stringValue = badges.unread > 0 ? Format.num(badges.unread) : ""
        count.isHidden = badges.unread == 0
        if let roles {
            Labels.style(name, .primary, roles: roles)
            Labels.style(digit, .tertiary, roles: roles)
            mention.textColor = roles.mention.nsColor
            Labels.style(count, .secondary, roles: roles)
        }
        setAccessibilityLabel([row.name, badges.unread > 0 ? "\(badges.unread) unread" : nil, badges.mention ? "mentions you" : nil,
                               badges.digit.map { "favourite \($0)" }].compactMap(\.self).joined(separator: ", "))
    }

    /// A built-in folder's symbol, a smart folder's, or a folder of yours'.
    static func symbol(for row: FolderRow) -> String {
        switch row.id {
        case "all": "bubble.left.and.bubble.right"
        case "unread": "message.badge"
        case "groups": "person.3"
        case "communities": "person.2.circle"
        case "archived": "archivebox"
        default: row.isSmart ? "folder.badge.gearshape" : "folder"
        }
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
