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
                note: "All with an @ and 12, Unread with an @ and 12, Groups with an @ and 5, Communities with 1, Archived with no count; under Folders, Family with ⌘1, an @ and 3, open to show Cousins with 1, then Climbing with ⌘3, Friends with 2, and Work, closed, with a triangle for Clients in it; under Smart Folders, Waiting on me with 2, Mentions me with an @ and 1, and Muted, unread. The names start together on each level, the symbols in a column of their own, the digits, @s and counts line up on the right, and nothing is cut off. On the selection, Family's symbol, ⌘1 and 3 are in its name's colour, where the other rows' are quieter. The selection's colour is the window's, which a picture may not show. The roster, drawn in a scene of its own, is hidden here, so the tree has the whole height.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    s.apply(Samples.folderUpdate)
                    s.roster.view.isHidden = true
                    s.expand("folder:1")
                    s.select("folder:1")
                    return s
                }
                .frame(width: 220, height: 440)
            }
            SpecimenGroup(
                title: "Naming in place",
                note: "A new folder's field open under Family, after Cousins, holding \"Cousins\", with jawd's refusal under it in the secondary colour: \"a folder there is called Cousins already\". The row is taller to hold it, and nothing under it is covered. The whole tree shows, from All, nothing scrolled away. The roster, drawn in a scene of its own, is hidden here, so the tree has the whole height.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    // Its size before the field takes the keys, which scrolls the field into sight.
                    s.view.frame = NSRect(x: 0, y: 0, width: 220, height: 520)
                    s.apply(Samples.folderUpdate)
                    s.roster.view.isHidden = true
                    s.view.layoutSubtreeIfNeeded()
                    s.beginNaming(parent: "folder:1", text: "Cousins")
                    s.notNamed("a folder there is called Cousins already")
                    return s
                }
                .frame(width: 220, height: 520)
            }
            SpecimenGroup(
                title: "From jawd's fixture",
                note: "protocol/fixtures/folders.watch.result.json as jawd sends it: All with an @ and 12, Unread with ⌘2, an @ and 12, Family with ⌘1, an @ and 3, open to Cousins with 1, and Mentions me with an @ and 1. The roster, drawn in a scene of its own, is hidden here, so the tree has the whole height.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    s.apply(Update(seq: Fixtures.tree.seq, upsert: Fixtures.tree.rows, favourites: Fixtures.tree.favourites))
                    s.roster.view.isHidden = true
                    s.expand("folder:9")
                    return s
                }
                .frame(width: 220, height: 260)
            }
        }
    }
}

extension HarnessScene {
    static var emptyFolder: HarnessScene {
        HarnessScene(
            id: "empty-folder", title: "An empty folder", symbol: "folder",
            synopsis: "A folder's chat list with nothing in it, saying so in the terminal's words",
            section: .chatList, covers: [ChatListController.self], width: 640,
        ) { EmptyFolderScene(roles: $0.roles) }
    }
}

/// Family and Unread, each with nothing in it.
private struct EmptyFolderScene: View {
    let roles: Roles

    var body: some View {
        SpecimenGroup(
            title: "Family and Unread, empty",
            note: "\"No chats in this folder.\" in each, as the terminal says it, centred near the top of the list in the secondary colour, not cut off.",
            roles: roles,
        ) {
            HStack(spacing: Space.l) {
                ForEach(["folder:1", "unread"], id: \.self) { id in
                    ControllerSpecimen {
                        let list = ChatListController(folder: id, roles: roles)
                        _ = list.view
                        list.emptyLine = Samples.tree.emptyLine(id)
                        list.apply(Update(seq: 1, upsert: [], total: 0))
                        return list
                    }
                    .frame(width: 300, height: 180)
                }
            }
        }
    }
}

extension HarnessScene {
    static var roster: HarnessScene {
        HarnessScene(
            id: "roster", title: "The roster", symbol: "person.2",
            synopsis: "The picked folder's people under the tree: grouped under All, the rest folded under N offline, faint where jawd isn't following someone",
            section: .chatList, covers: [RosterController.self], width: 760,
        ) { RosterScene(roles: $0.roles) }
    }
}

/// The roster under All, with Family's fold open, and under Family.
private struct RosterScene: View {
    let roles: Roles

    func roster(_ folder: String, _ people: [RosterRow], open: String? = nil) -> some View {
        ControllerSpecimen {
            let r = RosterController(folder: folder)
            r.tree = Samples.tree
            r.now = { Samples.now }
            r.roles = roles
            _ = r.view
            r.apply(Update(seq: 1, upsert: people, total: UInt64(people.count)))
            r.typing = ["person:1": .typing]
            if let open { r.toggle(open) }
            return r
        }
        .frame(width: 220, height: 340)
    }

    var body: some View {
        SpecimenGroup(
            title: "Under All, with Family's fold open, and under Family",
            note: "Ana's chat says she's typing in each. Under All: Family 1 of 5 with Ana typing…, its 4 folded; Climbing 2 of 3 with Marta and Rui online; Work and Others with theirs folded. Opened, Family lists Mãe, Inês, Avó Lurdes and Sofia. Under Family: Ana typing…, Mãe 12 min ago, faint, as jawd isn't following her, and Inês 2 h ago, then 2 offline. typing… is in the online colour, a green dot marks who's around, and no name is cut off.",
            roles: roles,
        ) {
            HStack(alignment: .top, spacing: Space.l) {
                roster("all", Samples.people)
                roster("all", Samples.people, open: "folder:1")
                roster("folder:1", Samples.people.filter { !Samples.tree.subtree("folder:1").isDisjoint(with: $0.folders) })
            }
        }
    }
}
