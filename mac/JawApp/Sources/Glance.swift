import AppKit
import JawKit
import JawThemes
import JawUI
import Observation
import SwiftUI

/// The glance (⌘J), what the spike calls the quick look: the latest unread messages of the chats that
/// need you, three of each of at most four, from windows watched only while it's open, as the
/// spike's are. Nothing is marked read; Enter goes to a chat, ⌘↩ opens it in a new pane.
@MainActor
final class Glance: NSObject {
    weak var model: AppModel?
    let panel = FloatingPanel(resizable: true, least: NSSize(width: 380, height: 260), keeps: "glance.size")
    var windows: [WindowID: String] = [:]
    let state = GlanceState()

    /// The most chats it shows, each a messages window of jawd's 32 a connection.
    static let chats = 12
    static let lines: UInt32 = 3

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
            windows[model.jawd.watchMessages(chat.id, .newest(Self.lines))] = chat.id
        }
        let size = NSSize(width: 520, height: GlanceView.height(min(state.chats.count, 5)))
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
        var rows = state.rows[chat] ?? Messages(chat: chat, place: .newest(Self.lines))
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

@MainActor @Observable
final class GlanceState {
    var chats: [ChatRow] = []
    var rows: [String: Messages<MessageRow>] = [:]
    var index = 0
}

struct GlanceView: View {
    @Bindable var state: GlanceState
    let roles: Roles
    let go: (ChatRow, Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: Space.m) {
            if state.chats.isEmpty {
                Text("Nothing needs you.").foregroundStyle(roles.secondary.color)
                Spacer()
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(alignment: .leading, spacing: Space.m + 2) {
                            ForEach(Array(state.chats.enumerated()), id: \.element.id) { i, chat in
                                card(chat, selected: i == state.index).id(i)
                            }
                        }
                    }
                    .scrollIndicators(.automatic)
                    .onChange(of: state.index) { proxy.scrollTo(state.index) }
                }
            }
            HStack {
                Text("↩ goes to it · ⌘↩ in a new pane · nothing is marked read")
                Spacer()
                Text("\(state.chats.count) \(state.chats.count == 1 ? "chat" : "chats") · drag an edge to resize")
            }
            .textStyle(.micro).foregroundStyle(roles.tertiary.color)
        }
        .padding(Space.l + 2)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .panel(roles)
    }

    /// Room for each chat's card and its three latest lines, and the hint.
    static func height(_ chats: Int) -> CGFloat {
        CGFloat(max(chats, 1)) * 86 + 56
    }

    func card(_ chat: ChatRow, selected: Bool) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(chat.name).textStyle(.body).fontWeight(.semibold)
                if chat.mentions > 0 { Text("@").foregroundStyle(roles.mention.color).bold() }
                Spacer()
                Text("\(chat.unread) unread").textStyle(.caption).foregroundStyle(roles.secondary.color)
            }
            ForEach(state.rows[chat.id]?.list ?? [], id: \.id) { m in
                line(m, in: chat)
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill((selected ? roles.selection : roles.surface).color))
        .contentShape(Rectangle())
        .onTapGesture { go(chat, false) }
    }

    func line(_ m: MessageRow, in chat: ChatRow) -> some View {
        let who = m.mine ? "You" : (m.from?.name ?? chat.name)
        let tint: RGBA = m.mine ? roles.accent : roles.sender(m.from?.colour ?? 0)
        let what = m.deleted ? "This message was deleted" : (m.plain.isEmpty ? (m.system ?? m.kind.rawValue) : m.plain)
        return HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(who).foregroundStyle(tint.color).frame(width: 90, alignment: .leading).lineLimit(1)
            Text(what).lineLimit(1).foregroundStyle(roles.text.color)
        }
        .textStyle(.callout)
    }

}
