import JawKit
import JawThemes
import Observation
import SwiftUI

/// What the glance shows: the chats that need you, their latest lines, and which one is picked.
@MainActor @Observable
public final class GlanceState {
    public var chats: [ChatRow] = []
    public var rows: [String: Messages<MessageRow>] = [:]
    public var index = 0

    public init() {}
}

/// The glance: each chat that needs you as a card of its latest lines, the picked one selected.
public struct GlanceView: View {
    @Bindable var state: GlanceState
    let roles: Roles
    let go: (ChatRow, Bool) -> Void

    public init(state: GlanceState, roles: Roles, go: @escaping (ChatRow, Bool) -> Void) {
        self.state = state
        self.roles = roles
        self.go = go
    }

    public var body: some View {
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

    /// The latest lines each chat's card shows.
    public static let lines: UInt32 = 3
    /// The least its panel takes, however few chats need you.
    public static let least = CGSize(width: 380, height: 260)

    /// Room for each chat's card and its three latest lines, and the hint.
    public static func height(_ chats: Int) -> CGFloat {
        CGFloat(max(chats, 1)) * 86 + 56
    }

    /// Its size for `chats` chats: 520 points wide, with room for five chats' cards at most; its panel
    /// holds it to `least`.
    public static func size(_ chats: Int) -> CGSize {
        CGSize(width: 520, height: height(min(chats, 5)))
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
