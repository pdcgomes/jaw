import AppKit
import JawKit
import JawUI

/// The app's half of the folders: the tree and the roster watched, the sidebar's host, and what the
/// folder items do, asked of jawd.
extension AppModel: SidebarHost {
    /// The tree the sidebar holds, which the menus, the keys and the palette read.
    var tree: FolderTree {
        window?.sidebar.tree ?? FolderTree()
    }

    func sidebar(_: SidebarController, pick folder: String) {
        pick(folder: folder)
    }
}
