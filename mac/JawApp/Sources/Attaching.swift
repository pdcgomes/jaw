import AppKit
import JawKit
import JawUI
import UniformTypeIdentifiers

/// Files attached and sent from a box: Photos & Videos… and Document… pick them in the open panel over
/// the window, the box's sends go to jawd with their key, and what jawd answers goes back to the box,
/// files turned down to their strip.
extension AppModel {
    /// What the open panel picks.
    enum Picking {
        /// Photos and videos, sent as what they are.
        case photos
        /// Any file, sent as a document whatever it is.
        case documents
    }

    /// The open panel over the window, several at once, for the focused pane's box.
    func attach(_ picking: Picking) {
        guard let c = conversation, let window = window?.window else { return say("open a chat to attach files to it") }
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.resolvesAliases = true
        panel.prompt = "Attach"
        switch picking {
        case .photos:
            panel.allowedContentTypes = [.image, .movie]
            panel.message = "Photos and videos to send to \(c.chat.name)"
        case .documents:
            panel.message = "Files to send to \(c.chat.name), each as a document"
        }
        let chat = c.chat.id
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK else { return }
            self?.panes[chat]?.attach(panel.urls.map { AttachedFile(url: $0, kind: picking == .documents ? .document : nil) })
        }
    }

    func conversation(_ c: ConversationController, send files: [AttachedFile], caption: String, hd: Bool, key: String, reply: String?) {
        filesSent[key] = (c.chat.id, files, caption)
        jawd.sendMedia(chat: c.chat.id, files: Attachments.params(files, caption: caption), quality: hd ? .hd : .sd, reply: reply, key: key)
    }

    /// jawd has the files sent with `key`, its own copies kept: those the app wrote go.
    func filesTaken(_ key: String) {
        guard let sent = filesSent.removeValue(forKey: key) else { return }
        AttachedFile.discard(sent.files)
    }

    /// jawd turned down the files sent with `key`: back to their box's strip with what was typed, or
    /// said when the chat's pane has gone.
    func filesUnsent(chat: String, key: String, files: [SendMedia.File], why: String) {
        let sent = filesSent.removeValue(forKey: key)
        if let c = panes[chat], let sent {
            c.unsent(files: sent.files, caption: sent.caption, why: why)
        } else {
            AttachedFile.discard(sent?.files ?? [])
            say("\(files.count == 1 ? "the file wasn't" : "the files weren't") sent: \(why)", warn: true)
        }
    }

    /// Files dropped on a pane with ⌥ held: staged in its chat's box, which takes the keys.
    func dropFiles(_ board: NSPasteboard, on slot: Int) {
        guard let chat = grid?.slot(slot)?.chat, let c = panes[chat] else { return say("drop files on a pane with a chat") }
        focus(slot: slot)
        c.attachDropped(from: board)
    }

    /// What jawd's status says it's missing, for the boxes to say when it's in the way.
    func read(_ status: Status) {
        ffmpegMissing = status.missing.first { $0.what == "ffmpeg" }?.neededFor
        for c in panes.values {
            c.composer.ffmpegMissing = ffmpegMissing
        }
    }
}
