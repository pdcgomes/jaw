import AppKit
import JawKit
import JawUI

/// The chat's actions, written once, and the right-click menus made from them as they open: a
/// chat's in the list, a pane's on its header and around its messages, and the end of a message's.
/// The menu bar and the palette bind the actions to the focused pane's chat, a right-click menu to
/// the chat it was opened on.
extension AppModel {
    func chatCommands(for chat: @escaping @MainActor @Sendable () -> ChatRow?) -> [Command] {
        let isChat: @MainActor @Sendable () -> Bool = { chat().map { $0.kind != .community } ?? false }
        var out: [Command] = []
        for length in MuteLength.allCases {
            out.append(Command(length.id, length.title, phrase: "Mute this chat \(length.title.lowercased())",
                               detail: "no notifications from it", place: .chat, submenu: "Mute",
                               enabled: { chat().map { !$0.muted && $0.kind != .community } ?? false })
            { [weak self] in
                guard let c = chat() else { return }
                let until = Double(length.until(now: Format.now()))
                self?.jawd.chatAction("chat.mute", chat: c.id, ["until": .number(until)], what: "muting")
            })
        }
        out.append(Command("unmute", "Unmute", phrase: "Unmute this chat", detail: "its notifications back", place: .chat,
                           submenu: "Mute", enabled: { chat()?.muted ?? false })
        { [weak self] in
            guard let c = chat() else { return }
            self?.jawd.chatAction("chat.mute", chat: c.id, ["until": .null], what: "unmuting")
        })
        out.append(Command("archive", "Archived",
                           says: { (chat()?.archived ?? false ? "Unarchive this chat" : "Archive this chat", "as on the phone") },
                           place: .chat, group: 1, enabled: isChat, on: { chat()?.archived ?? false })
        { [weak self] in
            guard let c = chat() else { return }
            self?.jawd.chatAction("chat.archive", chat: c.id, ["archived": .bool(!c.archived)], what: "archiving")
        })
        out.append(Command("pin", "Pinned",
                           says: { (chat()?.pinned ?? false ? "Unpin this chat" : "Pin this chat", "at the top of the list") },
                           place: .chat, group: 1, enabled: isChat, on: { chat()?.pinned ?? false })
        { [weak self] in
            guard let c = chat() else { return }
            self?.jawd.chatAction("chat.pin", chat: c.id, ["pinned": .bool(!c.pinned)], what: "pinning")
        })
        let isUnread: @MainActor @Sendable () -> Bool = { chat().map { $0.unread > 0 || $0.markedUnread } ?? false }
        out.append(Command("mark", "Unread", says: { (isUnread() ? "Mark this chat read" : "Mark this chat unread", "as on the phone") },
                           place: .chat, group: 1, enabled: isChat, on: isUnread)
        { [weak self] in
            guard let c = chat() else { return }
            let unread = isUnread()
            self?.jawd.chatAction("chat.mark_unread", chat: c.id, ["unread": .bool(!unread)],
                                  what: unread ? "marking a chat read" : "marking a chat unread")
        })
        return out
    }

    /// A chat in a pane of its own: beside the focused pane, or in it when it's empty; a chat a pane
    /// already shows is gone to there, as ⌘↩ does in the glance.
    func openInNewPane(_ chat: ChatRow) {
        guard let grid, panes[chat.id] == nil, let at = focusedSlot, grid.slot(at)?.chat != nil else {
            return open(chat, opened: true)
        }
        split(.across, with: chat)
        if panes[chat.id] != nil { jawd.opened(chat.id) }
    }

    /// The chat list's menu for `chat`, its actions acting on it.
    func chatMenuItems(for chat: ChatRow) -> [NSMenuItem] {
        let open = Command(ChatMenu.openInNewPane, "Open in New Pane", phrase: "Open \(chat.name) in a new pane", place: nil,
                           palette: false) { [weak self] in self?.openInNewPane(chat) }
        let bound = chatCommands(for: { chat }) + [open]
        return Menus.items(ChatMenu.list(chat)) { id in bound.first { $0.id == id } }
    }

    /// A pane's menu, on its header, around its messages and on its system lines. The pane has the
    /// keys by the time it's asked for, so the splits and Swap act on it; its chat's actions act on
    /// its chat.
    func paneMenu(for c: ConversationController) -> NSMenu {
        let chat = c.chat
        let bound = chatCommands(for: { chat })
        return Menus.menu(ChatMenu.pane(chat)) { id in
            bound.first { $0.id == id } ?? command(id)
        }
    }
}
