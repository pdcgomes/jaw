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
final class Glance: NSObject, NSPopoverDelegate {
    weak var model: AppModel?
    var popover: NSPopover?
    var windows: [WindowID: String] = [:]
    let state = GlanceState()

    static let chats = 4
    static let lines: UInt32 = 3

    var isOpen: Bool {
        popover?.isShown ?? false
    }

    func show(relativeTo anchor: NSView, roles: Roles) {
        guard let model else { return }
        if isOpen {
            popover?.close()
            return
        }
        state.chats = Array(model.needs.prefix(Self.chats))
        state.rows = [:]
        state.index = 0
        for chat in state.chats {
            windows[model.jawd.watchMessages(chat.id, .newest(Self.lines))] = chat.id
        }
        let view = GlanceView(state: state, roles: roles, go: { [weak self] chat, pane in self?.go(chat, newPane: pane) })
        let host = NSHostingController(rootView: view)
        let p = NSPopover()
        p.contentViewController = host
        p.behavior = .transient
        p.delegate = self
        p.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
        popover = p
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
        popover?.close()
        guard let model else { return }
        if newPane, model.grid != nil, model.panes[chat.id] == nil {
            model.split(.across, with: chat)
        } else {
            model.open(chat, opened: true)
        }
    }

    func popoverDidClose(_: Notification) {
        for w in windows.keys {
            model?.jawd.unwatch(w)
        }
        windows.removeAll()
        popover = nil
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
    @FocusState var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if state.chats.isEmpty {
                Text("Nothing needs you.").foregroundStyle(roles.secondary.color)
            }
            ForEach(Array(state.chats.enumerated()), id: \.element.id) { i, chat in
                card(chat, selected: i == state.index)
            }
            Text("↩ goes to it · ⌘↩ in a new pane · nothing is marked read").textStyle(.micro).foregroundStyle(roles.tertiary.color)
        }
        .padding(14)
        .frame(width: 460)
        .background(roles.ground.color)
        .focusable()
        .focused($focused)
        .onAppear { focused = true }
        .onKeyPress(.downArrow) { state.index = min(state.index + 1, max(state.chats.count - 1, 0)); return .handled }
        .onKeyPress(.upArrow) { state.index = max(state.index - 1, 0); return .handled }
        .onKeyPress(.return, phases: .down) { press in
            guard state.index < state.chats.count else { return .ignored }
            go(state.chats[state.index], press.modifiers.contains(.command))
            return .handled
        }
        .focusEffectDisabled()
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
