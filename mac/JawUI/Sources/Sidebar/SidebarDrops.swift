import AppKit
import JawKit

/// A folder's menu, and what's dropped on the tree.
extension SidebarController: NSMenuDelegate {
    /// Fills the outline's menu for the row right-clicked, which AppKit outlines while it's open.
    public func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        for item in menuItems(forRow: outline.clickedRow) {
            menu.addItem(item)
        }
    }

    /// The menu of the folder on row `i`, or the sidebar's own on a heading, the field or no row.
    public func menuItems(forRow i: Int) -> [NSMenuItem] {
        let id = i >= 0 ? (outline.item(atRow: i) as? Item)?.id : nil
        return host?.sidebar(self, menuFor: id.flatMap { tree.row($0) != nil ? $0 : nil }) ?? []
    }

    /// The menu of the folder on `cell`'s row, for VoiceOver's Show Menu, which makes no click for the
    /// outline's menu to be filled for.
    func menu(for cell: NSView) -> NSMenu? {
        let items = menuItems(forRow: outline.row(for: cell))
        guard !items.isEmpty else { return nil }
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in items {
            menu.addItem(item)
        }
        return menu
    }
}
