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

/// `Samples`' four chats in the chat list at its usual width, Ana's open.
private struct ChatListScene: View {
    let roles: Roles

    var body: some View {
        SpecimenGroup(
            title: "Four chats at 300 points",
            note: "Ana Ribeiro pinned and open, with \"Draft:\" in the accent before your draft, \"see you\"; Lisbon Climbing with 3 unread, a mention and someone typing; Mãe muted, with 1 unread; Climbing Portugal, a community. A name is bold only when it has unread messages, the times line up on the right, and nothing is cut off.",
            roles: roles,
        ) {
            ControllerSpecimen {
                let list = ChatListController(folder: "all", roles: roles)
                _ = list.view
                let rows = Samples.chats
                list.apply(Update(seq: 1, upsert: rows, total: UInt64(rows.count)))
                list.select(Samples.direct.id)
                return list
            }
            .frame(width: 300, height: 260)
        }
    }
}
