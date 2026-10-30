import JawKit

/// How long a chat is muted for, from its Mute menu.
public enum MuteLength: CaseIterable, Sendable {
    case eightHours, week, always

    public var id: String {
        switch self {
        case .eightHours: "mute.8h"
        case .week: "mute.week"
        case .always: "mute.always"
        }
    }

    public var title: String {
        switch self {
        case .eightHours: "For 8 Hours"
        case .week: "For a Week"
        case .always: "Always"
        }
    }

    /// Until when, in milliseconds since the epoch, counted from `now`, the moment it's chosen; -1 is
    /// for ever, as jawd's `chat.mute` takes it.
    public func until(now: Int64) -> Int64 {
        switch self {
        case .eightHours: now + 8 * 3_600_000
        case .week: now + 7 * 86_400_000
        case .always: -1
        }
    }
}

/// What a chat's right-click menus offer, from its row as they open: each item says what it will
/// do (Unpin on a pinned chat, Unmute on a muted one), and a community, whose groups carry these,
/// has none of them.
public enum ChatMenu {
    /// The command that opens a chat in a pane of its own, the app's.
    public static let openInNewPane = "chat.open.pane"

    /// The end of a message's menu, an empty pane's whole menu, and the start of a pane's.
    public static let splits: [MenuEntry] = [.command("split.across"), .command("split.down"), .command("pane.close")]

    /// The chat list's menu for `chat`.
    public static func list(_ chat: ChatRow) -> [MenuEntry] {
        guard chat.hasChatActions else { return [] }
        return [.command(openInNewPane), .separator] + actions(chat)
    }

    /// The pane's menu, on its header and around its messages: its splits and Swap, greyed with no
    /// other pane to swap with, then its chat's actions and searching it.
    public static func pane(_ chat: ChatRow) -> [MenuEntry] {
        let swap: MenuEntry = .submenu("Swap", [
            .command("swap.left", title: "Left"), .command("swap.right", title: "Right"),
            .command("swap.up", title: "Up"), .command("swap.down", title: "Down"),
        ])
        return splits + [swap, .separator] + actions(chat) + [.separator, .command("search.chat"), .command("chat.library")]
    }

    /// Pin, Mute, Archive and Mark as Read or Unread, as the chat stands.
    public static func actions(_ chat: ChatRow) -> [MenuEntry] {
        guard chat.hasChatActions else { return [] }
        return [
            .command("pin", title: chat.pinned ? "Unpin" : "Pin"),
            chat.muted ? .command("unmute") : .submenu("Mute", MuteLength.allCases.map { .command($0.id) }),
            .command("archive", title: chat.archived ? "Unarchive" : "Archive"),
            .command("mark", title: chat.readsAsUnread ? "Mark as Read" : "Mark as Unread"),
        ]
    }
}

/// The rules a chat's actions follow, written once for its right-click menus and for the app's
/// commands in the menu bar and the palette.
public extension ChatRow {
    /// Whether it has a chat's actions, Pin, Mute, Archive and Mark: a community's groups carry
    /// them, so a community has none.
    var hasChatActions: Bool {
        kind != .community
    }

    /// Whether it reads as unread, so Mark makes it read: it has unread messages, or was marked
    /// unread.
    var readsAsUnread: Bool {
        unread > 0 || markedUnread
    }
}

/// A chat's menus with their folder items, from the tree as they open.
public extension ChatMenu {
    /// A chat's folder items, one list for the chat list's menu, the pane's and the menu bar's Chat
    /// menu: Add to Folder, and Move to and Remove from while the list shows one of your folders that
    /// holds it.
    static func folderItems(_ chat: ChatRow, tree: FolderTree, in folder: String) -> [MenuEntry] {
        FolderMenu.entries(Filed(chat), tree: tree, list: folder)
    }

    /// The chat list's menu: a chat's actions, then its folder items; a community's row has the folder
    /// items alone, and its groups the whole menu.
    static func list(_ chat: ChatRow, tree: FolderTree, in folder: String) -> [MenuEntry] {
        with(list(chat), folderItems(chat, tree: tree, in: folder))
    }

    /// The pane's menu: its chat's folder items after its actions, before the menu's last part,
    /// Search This Chat and This Chat's Library.
    static func pane(_ chat: ChatRow, tree: FolderTree, in folder: String) -> [MenuEntry] {
        var out = pane(chat)
        let folders = folderItems(chat, tree: tree, in: folder)
        if !folders.isEmpty, let last = out.lastIndex(of: .separator) { out.insert(contentsOf: [.separator] + folders, at: last) }
        return out
    }

    /// `entries`, then `more` after a separator; either alone when the other is empty.
    static func with(_ entries: [MenuEntry], _ more: [MenuEntry]) -> [MenuEntry] {
        entries.isEmpty ? more : more.isEmpty ? entries : entries + [.separator] + more
    }
}
