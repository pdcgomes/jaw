import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var pictures: HarnessScene {
        HarnessScene(
            id: "pictures", title: "Pictures", symbol: "photo.on.rectangle",
            synopsis: "Photos, albums, a sticker and the posters of a video, a GIF and a video message, each in its place from jawd's previews",
            section: .conversation, covers: [ConversationController.self, MessagePictures.self], axes: [.look], width: 680,
        ) { PicturesScene(conditions: $0) }
    }
}

extension HarnessScene {
    static var downloadStates: HarnessScene {
        HarnessScene(
            id: "download-states", title: "Download states", symbol: "arrow.down.circle",
            synopsis: "A photo, a video, a sticker, an album's tiles and a document in each state jawd gives a file, each saying what it is and what can be done",
            section: .conversation, covers: [ConversationController.self, MessagePictures.self], axes: [.look], width: 680,
        ) { DownloadStatesScene(conditions: $0) }
    }
}

/// A conversation of `rows` from `Samples.pictures`, its pictures decoded as it draws them, as tall as
/// its messages are at a pane 640 points wide, so the oldest of them is at its top.
struct PictureConversation: View {
    let conditions: Conditions
    let rows: [MessageRow]

    var body: some View {
        ControllerSpecimen {
            let c = ConversationController(chat: Samples.direct, spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                           layouts: Layouts(), place: .newest(100))
            c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
            _ = c.view
            Pictures.apply(Update(seq: 300, upsert: rows, total: UInt64(rows.count), older: false, newer: 0,
                                  moved: Moved(place: .newest(100), newer: 0)), to: c)
            return c
        }
        .frame(width: 640, height: Self.height(rows, conditions.style))
    }

    /// The transcript's height for `rows` at 640 points: their layouts, and the header's room above.
    static func height(_ rows: [MessageRow], _ style: Style) -> CGFloat {
        let spec = LayoutSpec(style: style, me: ["person:1"])
        let contexts = MessageContext.of(rows, group: false, unfolded: [], now: Samples.now)
        let laid = zip(rows, contexts).reduce(CGFloat(0)) { sum, row in
            sum + MessageLayout.lay(row.0, width: 640, context: row.1, style: style, fonts: spec.fonts, me: spec.me).height
        }
        return (laid + 44 + 2 * Space.m + Space.l).rounded(.up)
    }
}

private struct PicturesScene: View {
    let conditions: Conditions

    var body: some View {
        let rows = Samples.pictures
        SpecimenGroup(
            title: "Photos and albums",
            note: "Each photo in its own shape inside 320 by 400 points: a landscape with its caption, a portrait, a panorama held at 120 points tall and cropped, a square with HD; albums of two side by side, three with one wide below, four two by two and eight with +5 on the fourth, tiles 2 points apart and cropped from their centres. Nothing stretched, nothing overlapping its caption or its time.",
            roles: conditions.roles,
        ) {
            PictureConversation(conditions: conditions, rows: Array(rows.prefix(8)))
        }
        SpecimenGroup(
            title: "A sticker, posters, and pictures still coming",
            note: "The sticker on its own at 160 points with no bubble; the video's poster with ▶, its length and size; the GIF marked GIF; the video message a circle with its length; a photo whose previews aren't made yet drawn from its blurred thumbnail; one with nothing yet, its well and what it is; and an album as media in's jawd sends one, a line of counts.",
            roles: conditions.roles,
        ) {
            PictureConversation(conditions: conditions, rows: Array(rows.dropFirst(8)))
        }
    }
}

private struct DownloadStatesScene: View {
    let conditions: Conditions

    var body: some View {
        let rows = Samples.downloadStates
        SpecimenGroup(
            title: "Pictures, from their thumbnails",
            note: "Each from its blurred thumbnail: remote with ↓ and its size; coming, a ring filled to 40% with × to stop it; failed, with jawd's words and Try again; waiting for the phone; expired with Ask the phone again; gone with no thumbnail, its well and the same. The video remote with ↓ in ▶'s place and its length, then coming at 70%; the sticker coming; the album's tiles here, coming, remote and expired, each saying only what can be done. No words run past their picture.",
            roles: conditions.roles,
        ) {
            PictureConversation(conditions: conditions, rows: Array(rows.prefix(10)))
        }
        SpecimenGroup(
            title: "A document in each",
            note: "Remote with ↓ at its right; coming with its ring and ×; expired, jawd's words in its second line and Ask the phone again; waiting for the phone; and here, its type, size and pages with nothing at its right.",
            roles: conditions.roles,
        ) {
            PictureConversation(conditions: conditions, rows: Array(rows.dropFirst(10)))
        }
    }
}
