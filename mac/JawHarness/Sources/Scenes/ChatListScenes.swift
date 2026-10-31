import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var chatList: HarnessScene {
        HarnessScene(
            id: "chat-list", title: "The list", symbol: "list.bullet.rectangle",
            synopsis: "A folder's chats as the list draws them, pinned first, the open one selected",
            section: .chatList, covers: [ChatListController.self], width: 640,
        ) { ChatListScene(roles: $0.roles) }
    }
}

/// `Samples`' four chats in the chat list at its usual width, Ana's open, and a busy group.
private struct ChatListScene: View {
    let roles: Roles

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "Four chats at 300 points",
                note: "Ana Ribeiro pinned and open, with \"Draft:\" in the accent before your draft, \"see you\"; Lisbon Climbing with 3 unread and a mention, \"Rui typing…\" in the online colour in place of its newest message; Mãe muted, with 1 unread; Climbing Portugal, a community. A name is bold only when it has unread messages, the times line up on the right, and nothing is cut off.",
                roles: roles,
            ) {
                ControllerSpecimen { list(Samples.chats, typing: Samples.typing, selected: Samples.direct.id) }
                    .frame(width: 300, height: 260)
            }
            SpecimenGroup(
                title: "A busy group at 300 points",
                note: "Família Gomes with five people typing: their names are cut short with \"…\" to leave room for \"typing…\", which always shows whole, in the online colour.",
                roles: roles,
            ) {
                ControllerSpecimen { list([Samples.busyGroup], typing: [Samples.busyGroup.id: Samples.busy], selected: nil) }
                    .frame(width: 300, height: 56)
            }
        }
    }

    func list(_ rows: [ChatRow], typing: Typists, selected: String?) -> ChatListController {
        let list = ChatListController(folder: "all", roles: roles)
        _ = list.view
        list.apply(Update(seq: 1, upsert: rows, total: UInt64(rows.count)))
        list.typing = typing
        list.select(selected)
        return list
    }
}
