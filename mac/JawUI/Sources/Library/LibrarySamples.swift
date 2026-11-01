import Foundation
import JawKit

/// The library's made-up items, as jawd's library rows carry them: photos and videos over three
/// months, one still coming and one with nothing to draw yet, files of several kinds, links with and
/// without a title, and voice messages, across Lisbon Climbing and Ana's chat.
public extension Samples {
    /// Every kind's items, newest first.
    static var library: [LibraryRow] {
        let day: Int64 = 86_400_000
        var out: [LibraryRow] = []
        func media(_ picture: String, kind: String = "image", seconds: Int? = nil) -> Media? {
            try? JSONDecoder().decode(Media.self, from: Data(here(picture, kind: kind, seconds: seconds).utf8))
        }
        func add(_ id: String, _ kind: LibraryKind, at: Int64, from: Sender?, group: Bool = true, media: Media? = nil, voice: LibraryVoice? = nil,
                 file: LibraryFile? = nil, link: LibraryLink? = nil, caption: [Span] = [])
        {
            let chat = group ? Samples.group : Samples.direct
            let sort = kind == .link ? "\(at)~0000000~99999" : "\(at)~0000000"
            out.append(LibraryRow(id: id, v: 1, sort: sort, kind: kind, message: kind == .link ? String(id.prefix { $0 != "~" }) : id,
                                  chat: chat.id, chatName: chat.name, from: from, mine: from == nil, at: at, caption: caption,
                                  media: media, voice: voice, file: file, link: link))
        }
        let pictures = ["sunset", "lake", "wall", "ridge", "rope", "lake", "wall", "sunset", "ridge", "rope", "lake", "wall"]
        for (i, picture) in pictures.enumerated() {
            add("LP\(i)", .photo, at: now - Int64(i) * 2 * day - 3_600_000, from: i % 3 == 0 ? nil : i % 3 == 1 ? ana : rui,
                group: i % 2 == 0, media: media(picture))
        }
        // A picture no other tile draws, as one still coming shares its thumbnail only with its own file's.
        var coming = media("note")
        coming?.previews = nil
        coming?.path = nil
        coming?.state = .downloading
        coming?.progress = 0.4
        add("LP12", .photo, at: now - 30 * day, from: ines, media: coming)
        add("LP13", .photo, at: now - 31 * day, from: ana, media: Media(w: 4032, h: 3024, state: .remote))
        for (i, picture) in ["sunset", "lake", "wall", "ridge"].enumerated() {
            add("LP\(14 + i)", .photo, at: now - Int64(48 + 3 * i) * day, from: rui, group: true, media: media(picture))
        }
        add("LV0", .video, at: now - day / 2, from: rui, media: media("poster", kind: "video", seconds: 42))
        add("LV1", .video, at: now - 5 * day, from: nil, media: media("poster", kind: "video", seconds: 184))
        add("LV2", .video, at: now - 40 * day, from: ines, group: false, media: media("note", kind: "video", seconds: 14))
        add("LF0", .file, at: now - day, from: ana, file: LibraryFile(name: "Sintra topo.pdf", type: "PDF", bytes: 2_400_000, pages: 12,
                                                                     path: "media/files/topo.pdf"))
        add("LF1", .file, at: now - 3 * day, from: rui, file: LibraryFile(name: "topo-sintra.gpx", type: "GPX", bytes: 48000))
        add("LF2", .file, at: now - 9 * day, from: nil, group: false,
            file: LibraryFile(name: "orcamento-obras-cozinha-segunda-versao-com-as-medidas-novas.pdf", type: "PDF", bytes: 380_000, pages: 3,
                              path: "media/files/orcamento.pdf"))
        add("LF3", .file, at: now - 35 * day, from: ines, file: LibraryFile(name: "Audio, 3:02", type: "M4A", bytes: 2_900_000))
        add("LL0~0", .link, at: now - day - 600_000, from: ana, group: false,
            link: LibraryLink(url: "https://www.ipma.pt/pt/otempo/prev.localidade.hora/", host: "www.ipma.pt", title: "Previsão para Sintra · IPMA",
                              description: "Céu pouco nublado, vento fraco. Máxima de 22 °C."),
            caption: [Span(.text, "a previsão para domingo https://www.ipma.pt/pt/otempo/prev.localidade.hora/")])
        add("LL1~0", .link, at: now - 4 * day, from: rui,
            link: LibraryLink(url: "https://vertigo.pt", host: "vertigo.pt", title: "Vertigo Climbing Center"))
        add("LL2~0", .link, at: now - 20 * day, from: nil, link: LibraryLink(url: "https://climb.pt/sintra/routes?sector=pena", host: "climb.pt"))
        add("LW0", .voice, at: now - 2 * 3_600_000, from: ines, voice: LibraryVoice(seconds: 14))
        add("LW1", .voice, at: now - 6 * day, from: nil, group: false,
            voice: LibraryVoice(seconds: 65, waveform: (0 ..< 64).map { Int(50 + 45 * sin(Double($0) / 3)) }, played: true))
        return out.sorted { $0.sort > $1.sort }
    }

    /// A library's counts for `items`, as jawd counts a kind across chats.
    static func counts(of items: [LibraryRow]) -> LibraryCounts {
        let n = { (kind: LibraryKind) in items.count { $0.kind == kind } }
        return LibraryCounts(photo: n(.photo), video: n(.video), voice: n(.voice), file: n(.file), link: n(.link))
    }

    /// The window update a library watch of `items` returns, from the top.
    static func libraryUpdate(of items: [LibraryRow], kind: LibraryKind?, counts: LibraryCounts? = nil) -> Update<LibraryRow> {
        let these = kind.map { k in items.filter { $0.kind == k } } ?? items
        let counts = counts ?? Self.counts(of: Samples.library)
        return Update(seq: 300, upsert: these, total: UInt64(kind.map { counts[$0] } ?? counts.shared), older: false,
                      moved: Moved(place: LibraryController.place(for: kind), newer: 0), counts: counts, note: .some(nil))
    }
}
