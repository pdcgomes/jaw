import AppKit
import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var attachmentStrip: HarnessScene {
        HarnessScene(
            id: "attachment-strip", title: "Attached files", symbol: "paperclip",
            synopsis: "The box's strip of what's attached: a photo, three with a document in HD, thirty, turned down by jawd, and with jawd missing ffmpeg",
            section: .conversation, covers: [ComposerController.self, AttachmentStrip.self], width: 680,
        ) { AttachmentStripScene(conditions: $0) }
    }
}

extension HarnessScene {
    static var uploadStates: HarnessScene {
        HarnessScene(
            id: "upload-states", title: "Files on their way", symbol: "arrow.up.circle",
            synopsis: "Your photos, a video, an album's items and a document as they're made ready and uploaded, and one you stopped",
            section: .conversation, covers: [ConversationController.self, MessagePictures.self], axes: [.look], width: 680,
        ) { UploadStatesScene(conditions: $0) }
    }
}

/// Ana's box with files in its strip, each band's as its note says.
private struct AttachmentStripScene: View {
    let conditions: Conditions

    var body: some View {
        let thirty = (0 ..< 30).map { ["lake", "wall", "ridge", "rope", "sunset", "note"][$0 % 6] + ".jpg" }
        VStack(alignment: .leading, spacing: 0) {
            band("A photo", "Its picture filling a square tile, rounded, with × on a disc at its corner; SD at the strip's end; Send showing, though nothing's typed.",
                 files: Samples.attached(["lake.jpg"]))
            band("Three photos and a document, in HD", "Three squares, then the document's icon and its name, cut in its middle where it's long, each with its ×; HD at the strip's end, Settings' default being HD here.",
                 files: Samples.attached(["lake.jpg", "wall.jpg", "ridge.jpg", "Sintra topo, the long way round.pdf"],
                                         documents: ["Sintra topo, the long way round.pdf"]), hd: true)
            band("Thirty", "As many as one send takes, in a row that scrolls across; the last ones out of sight, none squeezed.",
                 files: Samples.attached(thirty))
            band("Turned down", "jawd's words in red in the bar, after Not sent:, the files back in the strip and the caption back in the field.",
                 files: Samples.attached(["lake.jpg", "swim.mov"]), refused: "swim.mov can't go as image")
            band("No ffmpeg", "With a video in the strip and jawd's status saying ffmpeg is missing, the strip says so in red under the tiles, in jawd's words.",
                 files: Samples.attached(["lake.jpg", "poster.mov"]), ffmpeg: "sending videos, GIFs and audio files")
        }
    }

    func band(_ title: String, _ note: String, files: [AttachedFile], hd: Bool = false, refused: String? = nil, ffmpeg: String? = nil) -> some View {
        SpecimenGroup(title: title, note: note, roles: conditions.roles) {
            ControllerSpecimen {
                let pane = ConversationController(chat: Samples.direct, spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                                  layouts: Layouts(), place: .newest(100))
                _ = pane.view
                let box = pane.composer
                box.thumbnail = { url, _, done in done(SamplePictures.attached(url.lastPathComponent)) }
                box.hdByDefault = hd
                box.ffmpegMissing = ffmpeg
                if let refused { box.unsent(files: files, caption: "the lake on Sunday", why: refused) } else { box.attach(files) }
                return BoxSpecimen(box)
            }
            .frame(width: 640, height: 150)
        }
    }
}

/// A box as the window holds one under the panes: as tall as it is, at the foot of its place.
private final class BoxSpecimen: NSViewController {
    let box: ComposerController

    init(_ box: ComposerController) {
        self.box = box
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        let root = NSView()
        addChild(box)
        box.view.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(box.view)
        NSLayoutConstraint.activate([
            box.view.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            box.view.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            box.view.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            box.view.topAnchor.constraint(greaterThanOrEqualTo: root.topAnchor),
        ])
        view = root
    }
}

/// Your files on their way to Ana, each as far as `Samples.uploadSending` says.
private struct UploadStatesScene: View {
    let conditions: Conditions

    var body: some View {
        let rows = Samples.uploadStates
        SpecimenGroup(
            title: "Photos, a video and an album, on their way",
            note: "The lake uploading, its ring filled to 70% with × to stop it and uploading 70% across its foot; the wall with no previews yet, its well and what it is, making it ready 40%; the video's poster uploading at 25%, no ▶ until it's sent; the album's second tile at 60% and its third being made ready at 10%, each with its own ring, the first uploaded already and the last waiting, neither with one; your clock on each, and nothing anywhere offering to download.",
            roles: conditions.roles,
        ) {
            SendingConversation(conditions: conditions, rows: Array(rows.prefix(4)))
        }
        SpecimenGroup(
            title: "A document on its way, and a photo you stopped",
            note: "The document's circle filling at 55% with ×, uploading 55% in its second line; the photo you stopped failed, Not sent · Retry under it, and no ring.",
            roles: conditions.roles,
        ) {
            SendingConversation(conditions: conditions, rows: Array(rows.dropFirst(4)))
        }
    }
}

/// A conversation of `rows` from `Samples.uploadStates`, as far as each has got, its pictures
/// decoded as it draws them.
private struct SendingConversation: View {
    let conditions: Conditions
    let rows: [MessageRow]

    var body: some View {
        ControllerSpecimen {
            let c = ConversationController(chat: Samples.direct, spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                           layouts: Layouts(), place: .newest(100))
            c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
            _ = c.view
            c.show(sending: Samples.uploadSending)
            Pictures.apply(Samples.update(of: rows), to: c)
            return c
        }
        .frame(width: 640, height: PictureConversation.height(rows, conditions.style))
    }
}
