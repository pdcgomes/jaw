import Foundation
import JawKit

/// What can be done with a file a message brings, or with one of an album's: worked out from its row,
/// so a click, the keys, the message's menu and the Message menu offer the same, in the same words.
public struct MediaOffer: Sendable, Hashable {
    /// The message whose file it is: an album item's own ID for one of its items.
    public let message: String
    public let chat: String
    /// The album's item, when it's one of an album's.
    public let item: Int?
    /// `image`, `video` and the rest, as an album's items say theirs.
    public let kind: MessageKind
    public let media: Media
    /// Its name as `media.save` names a copy: the message's own name for it, or its ID and the
    /// file's extension.
    public let name: String
    /// A file of yours on its way, waiting to send, which Stop Sending stops.
    public let sending: Bool
    /// Yours from the outbox, waiting to send or failed: its state is that of jawd's own copy, made
    /// or being made, never one to download.
    public let outbox: Bool

    /// What's done with a file.
    public enum Act: Sendable, Hashable {
        /// Quick Look, as Space, Enter or a click does.
        case look
        /// In its app, as ⌘↩ does, or in another.
        case open
        case openWith(URL)
        /// Into Downloads, its standard copy or its HD one.
        case save
        case saveHD
        /// The copy saved, in the Finder.
        case reveal
        /// Asked of jawd now: downloaded, tried again, or asked of the phone again.
        case fetch
        case stop
    }

    /// The file of `m`, or of its album's item `item`; nil for a message with none to look at or
    /// keep: text, a voice message jawd plays, one deleted, or one to view once, whose file jawd
    /// never keeps.
    public static func of(_ m: MessageRow, item: Int? = nil) -> MediaOffer? {
        guard !m.deleted, !m.viewOnce else { return nil }
        let sending = m.onItsWay, outbox = m.fromOutbox
        if m.kind == .album {
            guard let items = m.album?.items, !items.isEmpty else { return nil }
            let i = min(max(item ?? 0, 0), items.count - 1)
            guard let media = items[i].media else { return nil }
            return MediaOffer(message: items[i].id, chat: m.chat, item: i, kind: items[i].kind == "video" ? .video : .image, media: media,
                              sending: sending, outbox: outbox)
        }
        guard [.image, .video, .gif, .videoNote, .sticker, .audio, .document].contains(m.kind), let media = m.media else { return nil }
        return MediaOffer(message: m.id, chat: m.chat, item: nil, kind: m.kind, media: media, sending: sending, outbox: outbox)
    }

    /// The file of a library item: a photo's or a video's, as its message's row has it, or a file's
    /// from what the library says of it, named with its type's extension when its name has none, as
    /// an audio file's line, "Audio, 3:02", hasn't; nil for a voice message or a link.
    public static func of(_ item: LibraryRow) -> MediaOffer? {
        switch item.kind {
        case .photo, .video:
            guard let media = item.media else { return nil }
            return MediaOffer(message: item.message, chat: item.chat, item: nil, kind: item.kind == .video ? .video : .image, media: media)
        case .file:
            guard let f = item.file else { return nil }
            let ext = f.path.map { ($0 as NSString).pathExtension }.flatMap { $0.isEmpty ? nil : $0 } ?? f.type?.lowercased()
            let name = (f.name as NSString).pathExtension.isEmpty ? ext.map { "\(f.name).\($0)" } ?? f.name : f.name
            let media = Media(bytes: f.bytes, name: name, pages: f.pages, path: f.path, state: f.state ?? (f.path == nil ? .remote : .local),
                              progress: f.progress, error: f.error)
            return MediaOffer(message: item.message, chat: item.chat, item: nil, kind: .document, media: media)
        case .voice, .link, .other:
            return nil
        }
    }

    /// Every file a message brings: an album's each, or its one.
    public static func all(_ m: MessageRow) -> [MediaOffer] {
        if m.kind == .album {
            return (m.album?.items.indices).map { $0.compactMap { of(m, item: $0) } } ?? []
        }
        return of(m).map { [$0] } ?? []
    }

    init(message: String, chat: String, item: Int?, kind: MessageKind, media: Media, sending: Bool = false, outbox: Bool = false) {
        self.message = message
        self.chat = chat
        self.item = item
        self.kind = kind
        self.media = media
        self.sending = sending
        self.outbox = outbox
        name = Self.named(media, id: message)
    }

    /// The name jawd gives a saved copy: the file's own name, its slashes and nulls made `_`, unless
    /// it's empty or hidden; otherwise the message's ID with the cached file's extension.
    static func named(_ media: Media, id: String) -> String {
        if let name = media.name?.replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "\\", with: "_")
            .replacingOccurrences(of: "\0", with: "_"), !name.isEmpty, !name.hasPrefix(".")
        {
            return name
        }
        let ext = media.path.map { ($0 as NSString).pathExtension }.flatMap { $0.isEmpty ? nil : $0 } ?? Self.ext(media.mime)
        return "\(id).\(ext)"
    }

    static func ext(_ mime: String?) -> String {
        switch mime ?? "" {
        case "image/jpeg": "jpg"
        case "image/png": "png"
        case "image/webp": "webp"
        case "image/gif": "gif"
        case "video/mp4": "mp4"
        case "application/pdf": "pdf"
        case let m where m.hasPrefix("audio/"): m == "audio/mpeg" ? "mp3" : "m4a"
        default: "bin"
        }
    }

    public var isHere: Bool {
        media.isHere
    }

    /// The file jawd has here, relative to its folder: the HD copy's when that's here, for a look.
    public var here: String? {
        guard isHere else { return nil }
        return media.hdPath ?? media.path
    }

    /// The copy `act` saves: the HD one for Save HD Copy, and for Save the one a look shows, which is
    /// the HD one once that's here; nil for jawd's standard copy.
    public func variant(_ act: Act) -> Variant? {
        act == .saveHD || act == .save && isHere && media.hdPath != nil ? .hd : nil
    }

    /// What its state lets you do about it, as the menu says it: nothing once it's here; for a file of
    /// yours on its way, stopping it, and nothing once it's failed, Send Again being the message's.
    public var stateAct: (Act, String)? {
        if sending { return (.stop, "Stop Sending") }
        if outbox { return nil }
        return switch media.state {
        case .local: nil
        case .remote: (.fetch, media.error == nil ? "Download" : "Try Again")
        case .downloading: (.stop, "Stop Downloading")
        case .waitingForPhone: nil
        case .expired, .gone: (.fetch, "Ask the Phone Again")
        }
    }

    /// The same, as the palette says it.
    public var statePhrase: String? {
        if sending { return "Stop sending this file" }
        if outbox { return nil }
        return switch media.state {
        case .local, .waitingForPhone: nil
        case .remote: media.error == nil ? "Download this file" : "Try downloading this file again"
        case .downloading: "Stop downloading this file"
        case .expired, .gone: "Ask the phone for this file again"
        }
    }

    /// The message's menu's file items, in order: each act, its title, and the key that does the
    /// same once the message is selected.
    public var menu: [(act: Act, title: String, key: String?)] {
        var out: [(Act, String, String?)] = [(.look, "Quick Look", "Space"), (.open, "Open", "⌘↩"), (.save, "Save to Downloads", "s")]
        if media.hd { out.append((.saveHD, "Save HD Copy to Downloads", nil)) }
        out.append((.reveal, "Show in Finder", nil))
        if let (act, title) = stateAct { out.append((act, title, nil)) }
        return out
    }

    /// What's said while it's asked for to be looked at or opened once it's here.
    public var waitingWords: String {
        "\(name) is coming; it opens once it's here"
    }
}
