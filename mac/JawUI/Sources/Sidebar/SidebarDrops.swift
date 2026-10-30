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

extension SidebarController {
    /// Where a drop the outline proposes lands: onto a folder (`at` nil), or between the rows in a folder
    /// of yours, or at the top under Folders (`at` their place); nil anywhere else.
    func place(_ item: Any?, _ index: Int) -> (target: String?, at: Int?)? {
        guard let id = (item as? Item)?.id else { return nil }
        if id == Self.foldersHeading {
            return (nil, index == NSOutlineViewDropOnItemIndex ? tree.children(of: nil).count : index)
        }
        guard tree.row(id) != nil else { return nil }
        return (id, index == NSOutlineViewDropOnItemIndex ? nil : index)
    }

    /// What a drag carries: a folder of yours, or a chat, a community or someone with their name.
    static func dragged(_ board: NSPasteboard) -> FolderTree.Dragged? {
        if let id = board.string(forType: .jawFolder) { return .folder(id) }
        guard let id = board.string(forType: .jawChat) ?? board.string(forType: .jawCommunity) ?? board.string(forType: .jawPerson)
        else { return nil }
        return .item(id, name: board.string(forType: .string) ?? id)
    }

    /// Where a chat, a community or someone dropped between the rows goes: onto the folder of yours on
    /// `row`, the one under the pointer, as AppKit proposes between the rows near a row's edge and only a
    /// folder takes them; nil when that row isn't one of yours, and for a folder, which goes between.
    func dropTarget(_ dragged: FolderTree.Dragged, row: Int) -> Item? {
        guard case .item = dragged, row >= 0, let item = outline.item(atRow: row) as? Item, tree.drop(dragged, onto: item.id, at: nil) != nil
        else { return nil }
        return item
    }

    public func outlineView(_: NSOutlineView, pasteboardWriterForItem item: Any) -> (any NSPasteboardWriting)? {
        guard naming == nil, let id = (item as? Item)?.id, let row = tree.row(id), row.isYours else { return nil }
        let board = NSPasteboardItem()
        board.setString(id, forType: .jawFolder)
        board.setString(row.name, forType: .string)
        return board
    }

    public func outlineView(_ outlineView: NSOutlineView, validateDrop info: any NSDraggingInfo, proposedItem item: Any?,
                            proposedChildIndex index: Int) -> NSDragOperation
    {
        guard let dragged = Self.dragged(info.draggingPasteboard) else { return [] }
        if index != NSOutlineViewDropOnItemIndex,
           let onto = dropTarget(dragged, row: outline.row(at: outline.convert(info.draggingLocation, from: nil)))
        {
            outlineView.setDropItem(onto, dropChildIndex: NSOutlineViewDropOnItemIndex)
            return .move
        }
        guard let spot = place(item, index), tree.drop(dragged, onto: spot.target, at: spot.at) != nil else { return [] }
        return .move
    }

    public func outlineView(_: NSOutlineView, acceptDrop info: any NSDraggingInfo, item: Any?, childIndex index: Int) -> Bool {
        guard let dragged = Self.dragged(info.draggingPasteboard), let spot = place(item, index),
              let act = tree.drop(dragged, onto: spot.target, at: spot.at)
        else { return false }
        let name = switch dragged {
        case let .item(_, name): name
        case let .folder(id): tree.label(id)
        }
        host?.sidebar(self, do: act, name: name)
        return true
    }
}
