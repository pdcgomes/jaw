import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var states: HarnessScene {
        HarnessScene(
            id: "states", title: "The panes' place", symbol: "moon.zzz",
            synopsis: "What fills the panes' place when there's no chat to show, an empty chat, and the status line",
            section: .states, covers: [StateView.self, EmptyChatView.self, StatusLine.self],
        ) { StatesScene(roles: $0.roles) }
    }
}

/// Each state the panes' place can show, an empty chat, and the status line saying jawd's state.
private struct StatesScene: View {
    let roles: Roles

    var kinds: [(String, StateView.Kind)] {
        [
            ("no chat open", .noChat), ("no chats yet", .noChats), ("starting jawd", .starting(failed: nil)),
            ("jawd didn't start", .starting(failed: "jawd stopped as it started (status 1)")),
            ("jawd out of reach", .asleep(why: "jawd isn't running: nothing answers on ~/Library/Application Support/jaw/jaw.sock", retryIn: 2)),
            ("logged out", .loggedOut(reason: "logged out from the phone")), ("not linked", .unlinked),
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "States",
                note: "The Mimic awake, or asleep (grey, eyes shut) when jawd is out of reach; the heading says what's wrong and the line under it what to do, in jawd's words where they're jawd's. Logged out and not linked say Jaw isn't affiliated with WhatsApp or Meta.",
                roles: roles,
            ) {
                LazyVGrid(columns: [GridItem(.flexible(), spacing: Space.l), GridItem(.flexible())], spacing: Space.l) {
                    ForEach(kinds, id: \.0) { caption, kind in
                        Specimen(caption: caption, roles: roles) {
                            StateView(kind: kind, roles: roles)
                                .frame(height: 360)
                                .clipShape(RoundedRectangle(cornerRadius: Radius.field))
                        }
                    }
                }
            }
            SpecimenGroup(
                title: "An empty chat",
                note: "A group with no messages yet: the Mimic sits in it, and the line under it says who what you type goes to.",
                roles: roles,
            ) {
                EmptyChatView(chat: Samples.group)
                    .frame(maxWidth: .infinity)
                    .frame(height: 300)
            }
            SpecimenGroup(
                title: "The status line",
                note: "jawd's state on the left, in the accent, or in the mention colour when it warns, and how the focused chat is beside it: who's typing in the online colour, \"Rui typing…\", then \"muted\", quieter.",
                roles: roles,
            ) {
                VStack(spacing: Space.m) {
                    StatusLine(state: "catching up 1,204 of 5,000", warn: false, typing: "Rui typing…", chat: "muted", roles: roles)
                        .frame(height: 30)
                    StatusLine(state: "phone offline: photos and older history wait for it", warn: true, chat: nil, roles: roles)
                        .frame(height: 30)
                }
            }
        }
    }
}
