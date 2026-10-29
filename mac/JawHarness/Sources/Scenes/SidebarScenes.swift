import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var sidebar: HarnessScene {
        HarnessScene(
            id: "sidebar", title: "Sidebar", symbol: "sidebar.left",
            synopsis: "The folder tree: the built-in folders, yours nested under Folders, the smart ones, each with its counts, an @ and its ⌘ digit",
            section: .chatList, covers: [SidebarController.self], width: 520,
        ) { SidebarScene(roles: $0.roles) }
    }
}

/// The sidebar before jawd serves folders, and with `Samples`' tree, Family picked and open.
private struct SidebarScene: View {
    let roles: Roles

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "Before jawd serves folders",
                note: "All and Archived, as a jawd from main serves them, with no counts and nothing else.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    return s
                }
                .frame(width: 220, height: 100)
            }
            SpecimenGroup(
                title: "The tree, Family picked",
                note: "All with an @ and 12, Unread with an @ and 12, Groups with an @ and 5, Communities with 1, Archived with no count; under Folders, Family with ⌘1, an @ and 3, open to show Cousins with 1, then Climbing with ⌘3, Friends with 2, and Work, closed, with a triangle for Clients in it; under Smart Folders, Waiting on me with 2, Mentions me with an @ and 1, and Muted, unread. The names start together on each level, the symbols in a column of their own, the digits, @s and counts line up on the right, and nothing is cut off. The selection's colour is the window's, which a picture may not show.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    s.apply(Samples.folderUpdate)
                    s.expand("folder:1")
                    s.select("folder:1")
                    return s
                }
                .frame(width: 220, height: 440)
            }
        }
    }
}
