import AppKit
import JawKit
import JawThemes
import JawUI
import SwiftUI

/// The glance (⌘J), what the spike calls the quick look: the latest unread messages of the chats that
/// need you, three of each of at most four, from windows watched only while it's open, as the
/// spike's are. Nothing is marked read; Enter goes to a chat, ⌘↩ opens it in a new pane.
@MainActor
final class Glance: NSObject {
    weak var model: AppModel?
    let panel = FloatingPanel(resizable: true, least: GlanceView.least, keeps: "glance.size")
    var windows: [WindowID: String] = [:]
    let state = GlanceState()

    /// The most chats it shows, each a messages window of jawd's 32 a connection.
    static let chats = 12

    var isOpen: Bool {
        panel.isVisible
    }

    /// Shows the glance centred in `window`, or closes it when it's open.
    func show(over window: NSWindow, roles: Roles) {
        guard let model else { return }
        if isOpen {
            panel.close()
            return
        }
        state.chats = Array(model.needs.prefix(Self.chats))
        state.rows = [:]
        state.index = 0
        for chat in state.chats {
            windows[model.jawd.watchMessages(chat.id, .newest(GlanceView.lines))] = chat.id
        }
        let size = GlanceView.size(state.chats.count)
        let view = GlanceView(state: state, roles: roles, go: { [weak self] chat, pane in self?.go(chat, newPane: pane) })
        panel.keys = { [weak self] event in self?.key(event) ?? false }
        panel.closed = { [weak self] in self?.closed() }
        let host = NSHostingView(rootView: view)
        host.sizingOptions = []
        panel.show(host, size: size, over: window)
    }

    /// ↑ and ↓ move, Return goes to the chat and ⌘Return opens it in a new pane.
    func key(_ event: NSEvent) -> Bool {
        switch event.keyCode {
        case 125: state.index = min(state.index + 1, max(state.chats.count - 1, 0))
        case 126: state.index = max(state.index - 1, 0)
        case 36, 76:
            guard state.index < state.chats.count else { return false }
            go(state.chats[state.index], newPane: event.modifierFlags.contains(.command))
        default: return false
        }
        return true
    }

    func apply(_ window: WindowID, _ update: Update<MessageRow>) {
        guard let chat = windows[window] else { return }
        var rows = state.rows[chat] ?? Messages(chat: chat, place: .newest(GlanceView.lines))
        rows.apply(update)
        state.rows[chat] = rows
    }

    func chatsChanged() {
        guard isOpen, let model else { return }
        for chat in state.chats {
            if let row = model.window?.chatList.row(chat.id), let i = state.chats.firstIndex(where: { $0.id == row.id }) {
                state.chats[i] = row
            }
        }
    }

    func go(_ chat: ChatRow, newPane: Bool) {
        panel.close()
        guard let model else { return }
        if newPane {
            model.openInNewPane(chat)
        } else {
            model.open(chat, opened: true)
        }
    }

    func closed() {
        for w in windows.keys {
            model?.jawd.unwatch(w)
        }
        windows.removeAll()
    }
}
