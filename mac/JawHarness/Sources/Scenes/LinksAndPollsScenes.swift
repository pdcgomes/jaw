import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var pollForm: HarnessScene {
        HarnessScene(
            id: "poll-form", title: "The poll form", symbol: "chart.bar.doc.horizontal",
            synopsis: "The sheet the box's Poll… opens: empty, written, with something in the way, and turned down by jawd",
            section: .overlays, covers: [PollFormView.self],
        ) { PollFormScene(roles: $0.roles) }
    }

    static var polls: HarnessScene {
        HarnessScene(
            id: "polls", title: "Polls", symbol: "chart.bar.xaxis",
            synopsis: "Polls with their options, tallies and how many voted, yours ticked, in either look",
            section: .conversation, covers: [ConversationController.self], axes: [.look], width: 680,
        ) { PollsScene(conditions: $0) }
    }

    static var linkPreviews: HarnessScene {
        HarnessScene(
            id: "link-previews", title: "Link previews", symbol: "link",
            synopsis: "Links' previews under their text, received and sent, with their thumbnails and without, in either look",
            section: .conversation, covers: [ConversationController.self], axes: [.look], width: 680,
        ) { LinkPreviewsScene(conditions: $0) }
    }
}

/// The poll form for Lisbon Climbing as Poll… opens it, as it's written, when something's in the
/// way of sending it, and when jawd turned it down.
private struct PollFormScene: View {
    let roles: Roles
    @State private var empty = PollForm(chat: Samples.group.name)
    @State private var written = PollForm(chat: Samples.group.name, question: "Dinner after?",
                                          options: ["Ramen", "Tasca do Chico", "Pizza", ""], several: true)
    @State private var twice = PollForm(chat: Samples.group.name, question: "Which day?", options: ["Saturday", "Saturday ", "Sunday"])
    @State private var refused = PollForm(
        chat: Samples.group.name,
        returned: SendPoll(chat: Samples.group.id, question: "Leave at 8 or at 9?", options: ["At 8", "At 9"], key: nil),
        said: "1000 wait to send, 2 failed; it can go once some have",
    )

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "As it opens, and written",
                note: "New Poll and the chat it's to, the question, two options to start with and Add Option, Allow several answers, and Cancel and Send; Send is greyed until there's a question and two options, and the foot says what's still to do, or how Return goes. Written, each option past the second has its × to take it out, and an empty one waits at the end for Return to send.",
                roles: roles,
            ) {
                HStack(alignment: .top, spacing: Space.xxl) {
                    Specimen(caption: "as Poll… opens it", roles: roles) { PollFormView(model: empty, roles: roles, send: {}, cancel: {}) }
                    Specimen(caption: "written, several answers allowed", roles: roles) {
                        PollFormView(model: written, roles: roles, send: {}, cancel: {})
                    }
                }
            }
            SpecimenGroup(
                title: "What stops it",
                note: "An option named twice, as jawd counts them once trimmed, says so at the foot and greys Send; a poll jawd turned down comes back as it was, its words in the warning colour at the foot, until it's changed.",
                roles: roles,
            ) {
                HStack(alignment: .top, spacing: Space.xxl) {
                    Specimen(caption: "an option twice", roles: roles) { PollFormView(model: twice, roles: roles, send: {}, cancel: {}) }
                    Specimen(caption: "turned down by jawd", roles: roles) { PollFormView(model: refused, roles: roles, send: {}, cancel: {}) }
                }
            }
        }
    }
}

/// `Samples.polls` in Lisbon Climbing, as a pane 640 points wide shows them.
private struct PollsScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Polls in Lisbon Climbing",
            note: "Each poll's question, on three lines at most, then what a vote may choose (Choose one, Choose any, Choose up to 2), its options each with a tick, a circle for one choice and a square for several, solid in the accent with its tick in white, or in the ground's colour when dark, where you chose, the count on the right and a bar for the option's share of those who voted, and how many voted at the foot: Inês's with your Saturday, Rui's of any with Quickdraws' bar full, Ana's with no votes yet and no bars, yours still on its way with its clock, and Rui's of up to two with its long question wrapped and its long options cut with an ellipsis.",
            roles: conditions.roles,
        ) {
            ControllerSpecimen {
                let c = ConversationController(chat: Samples.group,
                                               spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                               layouts: Layouts(), place: .newest(100))
                c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
                _ = c.view
                Pictures.apply(Samples.update(of: Samples.polls), to: c)
                return c
            }
            .frame(width: 640, height: 1220)
        }
    }
}

/// `Samples.links` in Lisbon Climbing, as a pane 640 points wide shows them, their thumbnails decoded
/// from the made-up pictures as the app decodes jawd's from its folder.
private struct LinkPreviewsScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Links in Lisbon Climbing",
            note: "Each preview sits under its message's text, inside the bubble in bubbles and at the message's indent in lines: Ana's with its thumbnail on the left, Rui's from history without one, so the accent's bar is there instead, yours with its thumbnail and a description cut after two lines with an ellipsis, Inês's with a title and its host alone, and your last with no preview, its link only in the text. Each card's host has no www.; in bubbles the time sits under the card, never on it, and in lines at the right edge, as for any message.",
            roles: conditions.roles,
        ) {
            ControllerSpecimen {
                let c = ConversationController(chat: Samples.group,
                                               spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                               layouts: Layouts(), place: .newest(100))
                c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
                _ = c.view
                Pictures.apply(Samples.update(of: Samples.links), to: c)
                return c
            }
            .frame(width: 640, height: 680)
        }
    }
}
