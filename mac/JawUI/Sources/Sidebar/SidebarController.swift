import AppKit
import JawThemes

/// The sidebar's folders. Until jawd serves folders (its M6), it has All and Archived, as jawd
/// serves them now.
@MainActor
public final class SidebarController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    public struct Folder: Hashable {
        public let id: String
        public let name: String
        public let symbol: String
    }

    public static let folders = [
        Folder(id: "all", name: "All", symbol: "bubble.left.and.bubble.right"),
        Folder(id: "archived", name: "Archived", symbol: "archivebox"),
    ]

    public var picked: ((Folder) -> Void)?
    let table = NSTableView()
    public private(set) var current = "all"

    override public func loadView() {
        let column = NSTableColumn(identifier: .init("folder"))
        table.addTableColumn(column)
        table.headerView = nil
        table.style = .sourceList
        table.rowHeight = 28
        table.dataSource = self
        table.delegate = self
        table.setAccessibilityLabel("Folders")
        let scroll = NSScrollView()
        Scrolling.style(scroll)
        scroll.documentView = table
        scroll.drawsBackground = false
        view = scroll
        table.selectRowIndexes([0], byExtendingSelection: false)
    }

    public func numberOfRows(in _: NSTableView) -> Int {
        Self.folders.count
    }

    public func tableView(_ tableView: NSTableView, viewFor _: NSTableColumn?, row: Int) -> NSView? {
        let folder = Self.folders[row]
        let cell = NSTableCellView()
        let image = NSImageView(image: NSImage(systemSymbolName: folder.symbol, accessibilityDescription: nil)!)
        let text = Labels.make(folder.name, .body)
        cell.imageView = image
        cell.textField = text
        let stack = NSStackView(views: [image, text])
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: Space.xs),
            stack.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        _ = tableView
        return cell
    }

    public func tableViewSelectionDidChange(_: Notification) {
        let row = table.selectedRow
        guard row >= 0, row < Self.folders.count, Self.folders[row].id != current else { return }
        current = Self.folders[row].id
        picked?(Self.folders[row])
    }
}
