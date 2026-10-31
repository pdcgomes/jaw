import CoreGraphics
import Foundation
import ImageIO
import JawKit
import JawThemes
import UniformTypeIdentifiers

/// Made-up rows for the harness and the snapshot tests: every span, every tick, replies, reactions,
/// edits, deletes, forwards, a failed message, system and call lines, a long message that folds,
/// emoji alone, and a card for each kind, in a group and a direct chat, across two days.
public enum Samples {
    /// A fixed now, so what's drawn doesn't depend on when it's drawn.
    public static let now: Int64 = 1_792_159_320_000

    public static let group = ChatRow(
        id: "120363012345678901@g.us", v: 40, sort: "0~1792144500000~4", kind: .group, name: "Lisbon Climbing",
        last: Last(from: "Rui", mine: false, summary: "Saturday at 9?", at: now - 60000), unread: 3, mentions: 1,
        folders: ["folder:3"],
    )

    public static let direct: ChatRow = {
        var direct = ChatRow(
            id: "351912345678@s.whatsapp.net", v: 42, sort: "1~1792145060000~17", name: "Ana Ribeiro",
            last: Last(from: nil, mine: true, summary: "yes, the one at the lake", status: .read, at: now - 120_000),
            pinned: true, draft: "see you", folders: ["folder:1"],
        )
        direct.favourite = 2
        return direct
    }()

    public static var chats: [ChatRow] {
        var muted = ChatRow(id: "351913000000@s.whatsapp.net", v: 30, sort: "0~1792100000000~9", name: "Mãe",
                            last: Last(from: nil, mine: false, summary: "have you eaten?", at: now - 3 * 86_400_000), unread: 1,
                            folders: ["folder:1"])
        muted.mutedUntil = -1
        let community = ChatRow(id: "120363000000000009@g.us", v: 20, sort: "0~1792000000000~3", kind: .community,
                                name: "Climbing Portugal",
                                last: Last(from: "Sofia", mine: false, summary: "new routes at Sintra", at: now - 9 * 86_400_000))
        return [direct, group, muted, community]
    }

    /// A folder tree as jawd's M6 sends one: the built-in folders, yours (Family with Cousins in it,
    /// Climbing, Friends, and Work with Clients in it) and the three smart folders jaw starts with.
    /// Its counts are drawn for the sidebar, not summed from `chats`.
    public static var folders: [FolderRow] {
        [
            FolderRow(id: "all", name: "All", position: 0, unread: 12, mentions: 1, count: 312),
            FolderRow(id: "unread", name: "Unread", position: 1, unread: 12, mentions: 1, count: 5),
            FolderRow(id: "groups", name: "Groups", position: 2, unread: 5, mentions: 1, count: 40),
            FolderRow(id: "communities", name: "Communities", position: 3, unread: 1, count: 4),
            FolderRow(id: "archived", name: "Archived", position: 4, unread: 2, count: 30),
            FolderRow(id: "folder:1", v: 50, name: "Family", position: 0, unread: 3, mentions: 1, count: 6, favourite: 1),
            FolderRow(id: "folder:2", v: 50, parent: "folder:1", name: "Cousins", position: 0, unread: 1, count: 2),
            FolderRow(id: "folder:3", v: 50, name: "Climbing", position: 1, count: 7, favourite: 3),
            FolderRow(id: "folder:4", v: 50, name: "Friends", position: 2, unread: 2, count: 9),
            FolderRow(id: "folder:5", v: 50, name: "Work", position: 3, count: 4),
            FolderRow(id: "folder:6", v: 50, parent: "folder:5", name: "Clients", position: 0, count: 2),
            FolderRow(id: "smart:1", name: "Waiting on me", position: 0, smart: .object(["waiting": .number(86400)]), unread: 2, count: 2),
            FolderRow(id: "smart:2", name: "Mentions me", position: 1, smart: .object(["mentions": .bool(true)]), unread: 1, mentions: 1,
                      count: 1),
            FolderRow(id: "smart:3", name: "Muted, unread", position: 2, smart: .object(["muted": .bool(true), "unread": .bool(true)])),
        ]
    }

    /// Family on ⌘1, Ana's chat on ⌘2, Climbing on ⌘3.
    public static let favourites = [
        Favourite(slot: 1, folder: "folder:1"), Favourite(slot: 2, chat: direct.id, name: direct.name), Favourite(slot: 3, folder: "folder:3"),
    ]

    public static var tree: FolderTree {
        FolderTree(folders, favourites: favourites)
    }

    /// The tree's window's first update.
    public static var folderUpdate: Update<FolderRow> {
        Update(seq: 300, upsert: folders, favourites: favourites)
    }

    /// People as a roster sends them, from the smallest sort up: Ana, Marta and Rui online, Mãe last
    /// seen 12 minutes ago where jawd isn't following her now, Inês and Tiago last seen longer ago,
    /// then four who hide it; filed in `folders`' Family, Cousins, Climbing and Work, and Leonor in
    /// none. Who's typing comes from their chats, so a test or a scene says it with the roster's
    /// `typing`.
    public static var people: [RosterRow] {
        func seen(_ ago: Int64) -> String {
            "1~\(Int64.max - (now - ago))~"
        }
        return [
            RosterRow(id: "person:1", v: 1, sort: "0~ana ribeiro\u{1}000000000001", name: "Ana Ribeiro", presence: Presence(state: .online),
                      chat: direct.id, folders: ["folder:1"]),
            RosterRow(id: "person:20", v: 1, sort: "0~marta\u{1}000000000020", name: "Marta", presence: Presence(state: .online),
                      folders: ["folder:3"]),
            RosterRow(id: "person:3", v: 1, sort: "0~rui costa\u{1}000000000003", name: "Rui Costa", presence: Presence(state: .online),
                      chat: "351913410003@s.whatsapp.net", folders: ["folder:3"]),
            RosterRow(id: "person:2", v: 1, sort: seen(12 * 60000) + "mãe\u{1}000000000002", name: "Mãe",
                      presence: Presence(state: .seen, at: now - 12 * 60000, known: false), chat: "351913000000@s.whatsapp.net",
                      folders: ["folder:1"]),
            RosterRow(id: "person:4", v: 1, sort: seen(2 * 3_600_000) + "inês gomes\u{1}000000000004", name: "Inês Gomes",
                      presence: Presence(state: .seen, at: now - 2 * 3_600_000), folders: ["folder:2"]),
            RosterRow(id: "person:6", v: 1, sort: seen(3 * 86_400_000) + "tiago fonseca\u{1}000000000006", name: "Tiago Fonseca",
                      presence: Presence(state: .seen, at: now - 3 * 86_400_000), chat: "351913660006@s.whatsapp.net", folders: ["folder:5"]),
            RosterRow(id: "person:27", v: 1, sort: "2~avó lurdes\u{1}000000000027", name: "Avó Lurdes", presence: Presence(state: .hidden),
                      chat: "351213770027@s.whatsapp.net", folders: ["folder:1"]),
            RosterRow(id: "person:21", v: 1, sort: "2~joão\u{1}000000000021", name: "João", presence: Presence(state: .hidden),
                      folders: ["folder:3"]),
            RosterRow(id: "person:22", v: 1, sort: "2~leonor\u{1}000000000022", name: "Leonor", presence: Presence(state: .hidden)),
            RosterRow(id: "person:5", v: 1, sort: "2~sofia martins\u{1}000000000005", name: "Sofia Martins", presence: Presence(state: .hidden),
                      chat: "351913550005@s.whatsapp.net", folders: ["folder:2"]),
        ]
    }

    static func m(_ n: Int, _ spans: [Span], at: Int64, from: Sender? = nil, mine: Bool = false, kind: MessageKind = .text,
                  status: MessageStatus? = nil, reply: Quote? = nil, reactions: [Reaction] = [], edited: Bool = false,
                  deleted: Bool = false, system: String? = nil, chat: String = group.id) -> MessageRow
    {
        MessageRow(id: String(format: "3EB0%012d", n), v: UInt64(100 + n), sort: "\(at)~\(String(format: "%06d", n))",
                   chat: chat, at: at, from: mine ? nil : from, mine: mine, kind: kind, spans: spans, reply: reply,
                   reactions: reactions, status: status, edited: edited, deleted: deleted, system: system)
    }

    static let ana = Sender(id: "person:4", name: "Ana", colour: 4)
    static let rui = Sender(id: "person:7", name: "Rui", colour: 2)
    static let ines = Sender(id: "person:9", name: "Inês", colour: 0)

    /// Who's typing in the samples' chats, as jawd says it: Rui, in Lisbon Climbing.
    public static let typing: Typists = [group.id: [Typist(id: rui.id, name: rui.name)]]

    /// A busy group, where `busy` are typing.
    public static let busyGroup = ChatRow(
        id: "120363012345678902@g.us", v: 38, sort: "0~1792144400000~5", kind: .group, name: "Família Gomes",
        last: Last(from: "Inês", mine: false, summary: "who's bringing the cake?", at: now - 300_000),
    )

    /// Five people typing at once, oldest first, their names more than a chat list's row holds.
    public static let busy = ["Ana Margarida Ribeiro", "Rui Pedro Costa", "Inês Gomes Sousa", "Francisco Xavier Mendonça",
                              "Maria do Carmo Vasconcelos"].enumerated().map { Typist(id: "person:\($0.offset + 20)", name: $0.element) }

    /// A group's messages, oldest first.
    public static var messages: [MessageRow] {
        let yesterday = now - 86_400_000
        var long = ""
        for i in 1 ... 46 {
            long += "line \(i) of the route description\n"
        }
        var forwarded = m(5, [Span(.text, "Climbing gym opens at 7 on weekdays")], at: yesterday + 120_000, from: rui)
        forwarded.forwarded = 2
        var starred = m(12, [Span(.text, "Sintra on Sunday, then?")], at: now - 1_500_000, from: rui)
        starred.starred = true
        return [
            m(1, [], at: yesterday, kind: .system, system: "Rui added Sofia"),
            m(2, [Span(.text, "did you get the "), Span(.bold, "photos"), Span(.text, " from Sunday?")], at: yesterday + 1000, from: ana,
              reactions: [Reaction(emoji: "❤", n: 2, mine: true)]),
            m(3, [Span(.text, "yes, the one at the lake")], at: yesterday + 60000, mine: true, status: .read,
              reply: Quote(id: String(format: "3EB0%012d", 2), from: "Ana", summary: "did you get the photos from Sunday?")),
            m(4, [Span(.italic, "so"), Span(.text, " good. "), Span(.strike, "next week"), Span(.text, " this week? "),
                  Span(.mono, "rope: 60m"), Span(.text, " "), Span(.link, "climb.pt/sintra", url: "https://climb.pt/sintra")],
              at: yesterday + 90000, from: ana),
            forwarded,
            m(6, [], at: yesterday + 200_000, kind: .call, system: "Missed voice call"),
            m(7, [Span(.text, "Voice note from the wall")], at: now - 3_600_000, from: ines, kind: .voice),
            m(8, [Span(.mention, "@Pedro", person: "person:1"), Span(.text, " are you bringing the "), Span(.mention, "@Rui", person: "person:7"),
                  Span(.text, " quickdraws?")], at: now - 3_500_000, from: ines),
            m(9, [Span(.text, "👍🧗")], at: now - 3_400_000, mine: true, status: .delivered),
            m(10, [Span(.text, long)], at: now - 3_000_000, from: rui),
            m(11, [], at: now - 2_000_000, from: ana, deleted: true),
            starred,
            m(13, [Span(.text, "Saturday at 9? I can drive two of you")], at: now - 1_400_000, mine: true, status: .sent, edited: true),
            m(14, [Span(.text, "on my way")], at: now - 120_000, mine: true, status: .pending),
            m(15, [Span(.text, "this one didn't go")], at: now - 60000, mine: true, status: .failed),
            photo(m(16, [Span(.text, "sunset from the top")], at: now - 40000, from: rui, kind: .image,
                    reactions: [Reaction(emoji: "🔥", n: 1, mine: false), Reaction(emoji: "😍", n: 3, mine: false)]), "sunset", hd: true),
        ] + kinds
    }

    /// A media row's file as jawd sends it once it's here, drawn from one of `SamplePictures`.
    static func here(_ picture: String, kind: String = "image", seconds: Int? = nil, bytes: Int = 2_400_000, hd: Bool = false) -> String {
        let (w, h) = SamplePictures.size(picture)
        let length = seconds.map { #","seconds":\#($0)"# } ?? ""
        return #"{"w":\#(w),"h":\#(h),"bytes":\#(bytes),"mime":"\#(kind == "sticker" ? "image/webp" : kind == "image" ? "image/jpeg" : "video/mp4")","state":"local","path":"media/files/\#(picture).\#(kind == "image" ? "jpg" : kind == "sticker" ? "webp" : "mp4")","previews":{"256":"\#(SamplePictures.preview(picture, 256))","768":"\#(SamplePictures.preview(picture, 768))"},"thumbnail":"\#(SamplePictures.thumbnail(picture))","hd":\#(hd)\#(length)}"#
    }

    static func photo(_ row: MessageRow, _ picture: String, hd: Bool = false) -> MessageRow {
        var row = row
        row.media = try? JSONDecoder().decode(Media.self, from: Data(here(picture, hd: hd).utf8))
        return row
    }

    /// One message of each kind jawd draws a card for, as jawd sends them: their media's size,
    /// length and name, a place, a contact, a poll's options, a link's preview, an album.
    static var kinds: [MessageRow] {
        let at = now - 20000
        let rows = """
        [
          {"id":"K1","v":1,"sort":"\(at)~000101","chat":"g","at":\(at),"from":{"id":"person:7","name":"Rui","colour":2},"kind":"video",
           "spans":[{"t":"text","s":"the last move"}],"media":\(here("poster", kind: "video", seconds: 42, bytes: 12_500_000))},
          {"id":"K2","v":1,"sort":"\(at + 1000)~000102","chat":"g","at":\(at + 1000),"mine":true,"status":"delivered","kind":"document",
           "spans":[{"t":"text","s":"Sintra topo.pdf"}],"media":{"name":"Sintra topo.pdf","bytes":2400000,"mime":"application/pdf","pages":12}},
          {"id":"K3","v":1,"sort":"\(at + 2000)~000103","chat":"g","at":\(at + 2000),"from":{"id":"person:4","name":"Ana","colour":4},"kind":"location",
           "location":{"lat":38.7876,"lon":-9.3906,"name":"Cabo da Roca","address":"Colares, Sintra"}},
          {"id":"K4","v":1,"sort":"\(at + 3000)~000104","chat":"g","at":\(at + 3000),"from":{"id":"person:9","name":"Inês","colour":0},"kind":"poll",
           "poll":{"question":"Which day?","max":1,"voters":4,"options":[{"name":"Saturday","votes":3,"mine":true},{"name":"Sunday","votes":1}]}},
          {"id":"K5","v":1,"sort":"\(at + 4000)~000105","chat":"g","at":\(at + 4000),"from":{"id":"person:7","name":"Rui","colour":2},"kind":"contact",
           "contact":{"name":"Climbing shop","vcard":"BEGIN:VCARD\\r\\nVERSION:3.0\\r\\nFN:Climbing shop\\r\\nTEL;type=WORK:+351 21 987 6543\\r\\nEND:VCARD\\r\\n"}},
          {"id":"K6","v":1,"sort":"\(at + 5000)~000106","chat":"g","at":\(at + 5000),"from":{"id":"person:4","name":"Ana","colour":4},"kind":"text",
           "spans":[{"t":"link","s":"climb.pt/sintra","url":"https://climb.pt/sintra"}],
           "link":{"url":"https://climb.pt/sintra","title":"Sintra: the routes","description":"Granite slabs above the sea, from 4 to 7b",
                   "preview":"media/links/sintra.jpg"}},
          {"id":"K7","v":1,"sort":"\(at + 6000)~000107","chat":"g","at":\(at + 6000),"mine":true,"status":"read","kind":"voice","media":{"seconds":14}},
          {"id":"K8","v":1,"sort":"\(at + 7000)~000108","chat":"g","at":\(at + 7000),"from":{"id":"person:9","name":"Inês","colour":0},"kind":"album",
           "album":{"items":[{"id":"a","kind":"image","media":\(here("lake"))},{"id":"b","kind":"image","media":\(here("wall"))},
                             {"id":"c","kind":"video","media":\(here("poster", kind: "video", seconds: 9))},
                             {"id":"d","kind":"image","media":\(here("ridge"))},{"id":"e","kind":"image","media":\(here("rope"))}]}}
        ]
        """
        return (try? JSONDecoder().decode([MessageRow].self, from: Data(rows.utf8))) ?? []
    }

    /// Links' previews as jawd sends them, in the group: Ana's with its thumbnail, Rui's from history
    /// without one, yours whose description runs past two lines, Inês's with a title alone, and yours
    /// whose page gave no preview, its link in the text alone.
    public static var links: [MessageRow] {
        let at = now - 600_000
        let rows = """
        [
          {"id":"L1","v":1,"sort":"\(at)~000201","chat":"\(group.id)","at":\(at),"from":{"id":"person:4","name":"Ana","colour":4},"kind":"text",
           "spans":[{"t":"text","s":"the forecast for Sunday "},
                    {"t":"link","s":"https://www.ipma.pt/pt/otempo/prev.localidade.hora/","url":"https://www.ipma.pt/pt/otempo/prev.localidade.hora/"}],
           "link":{"url":"https://www.ipma.pt/pt/otempo/prev.localidade.hora/","title":"Previsão para Sintra · IPMA",
                   "description":"Céu pouco nublado, vento fraco. Máxima de 22 °C.","preview":"media/links/ipma.jpg"}},
          {"id":"L2","v":1,"sort":"\(at + 60000)~000202","chat":"\(group.id)","at":\(at + 60000),"from":{"id":"person:7","name":"Rui","colour":2},
           "kind":"text","spans":[{"t":"link","s":"climb.pt/sintra","url":"https://climb.pt/sintra"}],
           "link":{"url":"https://climb.pt/sintra","title":"Sintra: the routes","description":"Granite slabs above the sea, from 4 to 7b"}},
          {"id":"L3","v":1,"sort":"\(at + 120000)~000203","chat":"\(group.id)","at":\(at + 120000),"mine":true,"status":"read","kind":"text",
           "spans":[{"t":"text","s":"booked us this "},{"t":"link","s":"https://www.casadapena.pt/rooms","url":"https://www.casadapena.pt/rooms"}],
           "link":{"url":"https://www.casadapena.pt/rooms","title":"Casa da Pena, a guest house in Sintra",
                   "description":"Six rooms in the old town, ten minutes on foot from the walls of the Moorish Castle, with a garden, a terrace, breakfast from eight, and the hills down to the sea from the top floor on a clear day.",
                   "preview":"media/links/pena.jpg"}},
          {"id":"L4","v":1,"sort":"\(at + 180000)~000204","chat":"\(group.id)","at":\(at + 180000),"from":{"id":"person:9","name":"Inês","colour":0},
           "kind":"text","spans":[{"t":"text","s":"how it's done "},{"t":"link","s":"https://youtu.be/sintra-boulders","url":"https://youtu.be/sintra-boulders"}],
           "link":{"url":"https://youtu.be/sintra-boulders","title":"Sintra bouldering, part 2"}},
          {"id":"L5","v":1,"sort":"\(at + 240000)~000205","chat":"\(group.id)","at":\(at + 240000),"mine":true,"status":"delivered","kind":"text",
           "spans":[{"t":"text","s":"and the topo "},{"t":"link","s":"https://climb.pt/topo.pdf","url":"https://climb.pt/topo.pdf"}]}
        ]
        """
        return (try? JSONDecoder().decode([MessageRow].self, from: Data(rows.utf8))) ?? []
    }

    /// Polls as jawd sends them, in the group: Inês's of one choice with your vote, Rui's of any number
    /// with none of yours, Ana's that nobody has voted in, yours still on its way, and Rui's of up to
    /// two with a long question and long options, two of them yours.
    public static var polls: [MessageRow] {
        let at = now - 900_000
        func option(_ name: String, _ votes: Int, mine: Bool = false) -> String {
            #"{"name":"\#(name)","votes":\#(votes),"mine":\#(mine)}"#
        }
        let rows = """
        [
          {"id":"P1","v":1,"sort":"\(at)~000301","chat":"\(group.id)","at":\(at),"from":{"id":"person:9","name":"Inês","colour":0},"kind":"poll",
           "spans":[{"t":"text","s":"Which day?"}],
           "poll":{"question":"Which day?","max":1,"voters":4,"options":[\(option("Saturday", 3, mine: true)),\(option("Sunday", 1))]}},
          {"id":"P2","v":1,"sort":"\(at + 60000)~000302","chat":"\(group.id)","at":\(at + 60000),"from":{"id":"person:7","name":"Rui","colour":2},
           "kind":"poll","spans":[{"t":"text","s":"What do we bring?"}],
           "poll":{"question":"What do we bring?","max":3,"voters":3,"options":[\(option("Rope", 2)),\(option("Quickdraws", 3)),\(option("Crash pads", 1))]}},
          {"id":"P3","v":1,"sort":"\(at + 120000)~000303","chat":"\(group.id)","at":\(at + 120000),"from":{"id":"person:4","name":"Ana","colour":4},
           "kind":"poll","spans":[{"t":"text","s":"Dinner after?"}],
           "poll":{"question":"Dinner after?","max":1,"voters":0,"options":[\(option("Ramen", 0)),\(option("Tasca do Chico", 0)),\(option("Pizza", 0))]}},
          {"id":"P4","v":1,"sort":"\(at + 180000)~000304","chat":"\(group.id)","at":\(at + 180000),"mine":true,"status":"pending","kind":"poll",
           "spans":[{"t":"text","s":"Leave at 8 or at 9?"}],
           "poll":{"question":"Leave at 8 or at 9?","max":1,"voters":0,"options":[\(option("At 8", 0)),\(option("At 9", 0))]}},
          {"id":"P5","v":1,"sort":"\(at + 240000)~000305","chat":"\(group.id)","at":\(at + 240000),"from":{"id":"person:7","name":"Rui","colour":2},
           "kind":"poll","spans":[{"t":"text","s":"If the rock is dry by Saturday morning, where do we start before the sun gets onto the walls?"}],
           "poll":{"question":"If the rock is dry by Saturday morning, where do we start before the sun gets onto the walls?","max":2,"voters":5,
                   "options":[\(option("Cabo da Roca, on the slabs above the lighthouse", 2, mine: true)),\(option("Peninha, if the wind has dropped", 1)),
                              \(option("The boulders below the palace", 2, mine: true)),\(option("Anywhere with shade all day, please", 4))]}}
        ]
        """
        return (try? JSONDecoder().decode([MessageRow].self, from: Data(rows.utf8))) ?? []
    }


    /// A chat of pictures, as jawd sends each once it's here: photos of every shape, one with an HD
    /// copy, albums of two, three, four and eight, a sticker, a video's, a GIF's and a video message's
    /// posters, a photo whose previews aren't made yet and one with nothing yet, and an album as media
    /// in's jawd sends one, its items as messages of their own.
    public static var pictures: [MessageRow] {
        let at = now - 600_000
        func album(_ pictures: [String]) -> String {
            let items = pictures.enumerated().map { i, p in #"{"id":"\#(p)\#(i)","kind":"\#(p == "poster" ? "video" : "image")","media":\#(here(p, kind: p == "poster" ? "video" : "image", seconds: p == "poster" ? 9 : nil))}"# }
            return #"{"items":[\#(items.joined(separator: ","))]}"#
        }
        let rows: [(String, String, Bool, String, String)] = [
            ("P1", "image", false, "[{\"t\":\"text\",\"s\":\"the lake, from the top\"}]", #""media":\#(here("lake"))"#),
            ("P2", "image", true, "[]", #""media":\#(here("wall"))"#),
            ("P3", "image", false, "[{\"t\":\"text\",\"s\":\"the whole ridge\"}]", #""media":\#(here("ridge"))"#),
            ("P4", "image", true, "[]", #""media":\#(here("rope", hd: true))"#),
            ("P5", "album", false, "[]", #""album":\#(album(["lake", "wall"]))"#),
            ("P6", "album", true, "[{\"t\":\"text\",\"s\":\"three from Sunday\"}]", #""album":\#(album(["ridge", "rope", "sunset"]))"#),
            ("P7", "album", false, "[]", #""album":\#(album(["sunset", "lake", "poster", "wall"]))"#),
            ("P8", "album", false, "[{\"t\":\"text\",\"s\":\"the whole day\"}]",
             #""album":\#(album(["lake", "wall", "ridge", "rope", "sunset", "poster", "lake", "wall"]))"#),
            ("P9", "sticker", false, "[]", #""media":\#(here("smile", kind: "sticker", bytes: 24000))"#),
            ("P10", "video", true, "[{\"t\":\"text\",\"s\":\"the last move\"}]", #""media":\#(here("poster", kind: "video", seconds: 42, bytes: 12_500_000))"#),
            ("P11", "gif", false, "[]", #""media":\#(here("lake", kind: "gif", bytes: 900_000))"#),
            ("P12", "video_note", false, "[]", #""media":\#(here("note", kind: "video_note", seconds: 14, bytes: 1_200_000))"#),
            ("P13", "image", false, "[]", #""media":{"w":4032,"h":3024,"state":"local","path":"media/files/sunset.jpg","thumbnail":"\#(SamplePictures.thumbnail("sunset"))"}"#),
            ("P14", "image", true, "[]", #""media":{"w":3024,"h":4032,"state":"local","path":"media/files/x.jpg"}"#),
            ("P15", "album", false, "[]", #""album":{"photos":3,"videos":1}"#),
        ]
        let json = rows.enumerated().map { i, r in
            let who = r.2 ? #""mine":true,"status":"read""# : #""from":{"id":"person:4","name":"Ana","colour":4}"#
            return #"{"id":"\#(r.0)","v":1,"sort":"\#(at + Int64(i) * 1000)~\#(String(format: "%06d", 300 + i))","chat":"\#(direct.id)","at":\#(at + Int64(i) * 1000),\#(who),"kind":"\#(r.1)","spans":\#(r.3),\#(r.4)}"#
        }
        return (try? JSONDecoder().decode([MessageRow].self, from: Data("[\(json.joined(separator: ","))]".utf8))) ?? []
    }

    /// A file in each state jawd gives one, as each is drawn: a photo remote, coming, failed, waiting
    /// for the phone, expired and gone; a video remote and coming; a sticker coming; an album whose
    /// tiles are each in a state of their own; and a document in each.
    public static var downloadStates: [MessageRow] {
        let at = now - 300_000
        func coming(_ picture: String, _ fields: String, bytes: Int = 3_480_211) -> String {
            let (w, h) = SamplePictures.size(picture)
            return #"{"w":\#(w),"h":\#(h),"bytes":\#(bytes),"mime":"image/jpeg","thumbnail":"\#(SamplePictures.thumbnail(picture))",\#(fields)}"#
        }
        func document(_ fields: String) -> String {
            #"{"name":"Sintra topo.pdf","mime":"application/pdf","bytes":2400000,"pages":12,\#(fields)}"#
        }
        let failed = #""state":"remote","error":"the download failed: the network connection was lost""#
        let expired = #""state":"expired","error":"WhatsApp's server no longer has it; ask for it to have your phone send it again""#
        let gone = #""state":"gone","error":"your phone no longer has this file""#
        let album = #"{"items":[{"id":"t0","kind":"image","media":\#(here("lake"))},{"id":"t1","kind":"image","media":\#(coming("wall", #""state":"downloading","progress":0.5"#))},{"id":"t2","kind":"video","media":\#(coming("poster", #""state":"remote""#, bytes: 48_000_000))},{"id":"t3","kind":"image","media":\#(coming("rope", expired))}]}"#
        let rows: [(String, String, String)] = [
            ("D1", "image", #""media":\#(coming("sunset", #""state":"remote""#))"#),
            ("D2", "image", #""media":\#(coming("lake", #""state":"downloading","progress":0.4"#))"#),
            ("D3", "image", #""media":\#(coming("ridge", failed))"#),
            ("D4", "image", #""media":\#(coming("rope", #""state":"waiting_for_phone""#))"#),
            ("D5", "image", #""media":\#(coming("wall", expired))"#),
            ("D6", "image", #""media":{"w":4032,"h":3024,\#(gone)}"#),
            ("D7", "video", #""media":\#(coming("poster", #""state":"remote","seconds":184"#, bytes: 48_000_000))"#),
            ("D8", "video", #""media":\#(coming("poster", #""state":"downloading","progress":0.7,"seconds":42"#, bytes: 12_500_000))"#),
            ("D9", "sticker", #""media":\#(coming("smile", #""state":"downloading","progress":0.2"#, bytes: 24000))"#),
            ("D10", "album", #""album":\#(album)"#),
            ("D11", "document", #""media":\#(document(#""state":"remote""#))"#),
            ("D12", "document", #""media":\#(document(#""state":"downloading","progress":0.3"#))"#),
            ("D13", "document", #""media":\#(document(expired))"#),
            ("D14", "document", #""media":\#(document(#""state":"waiting_for_phone""#))"#),
            ("D15", "document", #""media":\#(document(#""state":"local","path":"media/files/topo.pdf""#))"#),
        ]
        let json = rows.enumerated().map { i, r in
            let spans = r.1 == "document" ? #"[{"t":"text","s":"Sintra topo.pdf"}]"# : "[]"
            return #"{"id":"\#(r.0)","v":1,"sort":"\#(at + Int64(i) * 1000)~\#(String(format: "%06d", 400 + i))","chat":"\#(direct.id)","at":\#(at + Int64(i) * 1000),"from":{"id":"person:4","name":"Ana","colour":4},"kind":"\#(r.1)","spans":\#(spans),\#(r.2)}"#
        }
        return (try? JSONDecoder().decode([MessageRow].self, from: Data("[\(json.joined(separator: ","))]".utf8))) ?? []
    }

    /// The window update a watch of those messages returns.
    public static var update: Update<MessageRow> {
        update(of: messages)
    }

    /// The window update a watch of `rows`, opened at the newest, returns.
    public static func update(of rows: [MessageRow]) -> Update<MessageRow> {
        Update(seq: 200, upsert: rows, total: UInt64(rows.count), older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0))
    }

    // MARK: - Contact cards

    /// Contact cards in the group, as jawd sends them, each read from its vCard as JawKit reads a
    /// row: one with its photo, one on WhatsApp, two, five with some photos, one without a number, a
    /// long name with three numbers, yours on its way, and one deleted.
    public static var contacts: [MessageRow] {
        let at = now - 600_000
        let martaCard = vcard("Marta Lopes", [("+351 21 345 6789", "351213456789"), ("21 000 00 00", nil)], photo: photo(0x7DCFFF))
        let inesCard = vcard("Inês Gomes", [("+351 913 002 210", "351913002210")])
        let several = [vcard("Tiago Martins", [("+351 916 666 666", "351916666666")]), vcard("Sofia Costa", [("+351 917 777 777", nil)])]
        let five = [
            vcard("Tiago Martins", [("+351 916 666 666", "351916666666")], photo: photo(0x9ECE6A)),
            vcard("Sofia Costa", [("+351 917 777 777", nil)]),
            vcard("Mãe", [("+351 913 000 000", "351913000000")], photo: photo(0xBB9AF7)),
            vcard("Ricardo Pinto", [("+351 918 888 888", "351918888888")]),
            vcard("Joana Reis", [("+351 919 999 999", "351919999999")]),
        ]
        return [
            contactRow(1, at: at, from: rui, name: "Marta Lopes", vcard: martaCard),
            contactRow(2, at: at + 60000, from: ana, name: "Inês Gomes", vcard: inesCard),
            contactRow(3, at: at + 120_000, from: ines, name: "2 contacts", vcard: several.joined()),
            contactRow(4, at: at + 180_000, from: rui, name: "5 contacts", vcard: five.joined()),
            contactRow(5, at: at + 240_000, from: ana, name: "Escola de Escalada", vcard: vcard("Escola de Escalada", [])),
            contactRow(6, at: at + 300_000, from: ines, name: "Maria da Conceição Albuquerque de Vasconcelos e Sá",
                       vcard: vcard("Maria da Conceição Albuquerque de Vasconcelos e Sá",
                                    [("+351 912 345 678", "351912345678"), ("+351 21 111 1111", nil), ("+44 20 7946 0000", nil)])),
            contactRow(7, at: at + 360_000, mine: true, status: "pending", name: "Dr.ª Marta Lopes", vcard: martaCard),
            contactRow(8, at: at + 420_000, from: rui, name: "Inês Gomes", vcard: inesCard, deleted: true),
        ].compactMap(\.self)
    }

    /// A card as the contact form makes one from a contact, as Contacts exports it, with no photo and
    /// none of WhatsApp's marks: its name and numbers; and, for a card past what jawd takes in one
    /// message, `note` bytes of a note, which Contacts itself leaves out.
    public static func formCard(_ name: String, _ numbers: [String], note: Int = 0) -> ContactForm.Card {
        let words = name.split(separator: " ")
        var card = "BEGIN:VCARD\r\nVERSION:3.0\r\nPRODID:-//Apple Inc.//macOS 26.0//EN\r\n"
            + "N:\(words.last ?? "");\(words.dropLast().joined(separator: " "));;;\r\nFN:\(name)\r\n"
        for (i, number) in numbers.enumerated() {
            card += (i == 0 ? "TEL;type=CELL;type=VOICE;type=pref:" : "TEL;type=WORK;type=VOICE:") + number + "\r\n"
        }
        if note > 0 { card += "NOTE:" + String(repeating: "x", count: note) + "\r\n" }
        return ContactForm.Card(name: name, vcard: card + "END:VCARD\r\n")
    }

    /// The contact form's cards: Marta Lopes with two numbers, and Inês Gomes with one.
    public static var formCards: [ContactForm.Card] {
        [formCard("Marta Lopes", ["+351 213 456 789", "21 000 00 00"]), formCard("Inês Gomes", ["+351 913 002 210"])]
    }

    /// The window update a watch of the contact cards returns.
    public static var contactsUpdate: Update<MessageRow> {
        let rows = contacts
        return Update(seq: 300, upsert: rows, total: UInt64(rows.count), older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0))
    }

    /// A card as Contacts exports one, of `name` and its numbers, each with the WhatsApp account the
    /// phones mark it with, if any, and a photo.
    static func vcard(_ name: String, _ numbers: [(String, String?)], photo: Data? = nil) -> String {
        var card = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:;\(name);;;\r\nFN:\(name)\r\n"
        for (number, account) in numbers {
            card += "TEL;type=CELL" + (account.map { ";waid=\($0)" } ?? "") + ":\(number)\r\n"
        }
        if let photo { card += "PHOTO;ENCODING=b;TYPE=PNG:\(photo.base64EncodedString())\r\n" }
        return card + "END:VCARD\r\n"
    }

    /// A contact row in the group, as jawd sends one, read as JawKit reads it.
    static func contactRow(_ n: Int, at: Int64, from: Sender? = nil, mine: Bool = false, status: String? = nil, name: String,
                           vcard: String, deleted: Bool = false) -> MessageRow?
    {
        var row: [String: JSONValue] = [
            "id": .string(String(format: "3EB0C0%010d", n)), "v": .number(1), "sort": .string("\(at)~\(String(format: "%06d", 300 + n))"),
            "chat": .string(group.id), "at": .number(Double(at)), "mine": .bool(mine), "kind": .string("contact"), "deleted": .bool(deleted),
            "spans": .array([.object(["t": .string("text"), "s": .string(name)])]),
            "contact": .object(["name": .string(name), "vcard": .string(vcard)]),
        ]
        if let from, !mine { row["from"] = .object(["id": .string(from.id), "name": .string(from.name), "colour": .number(Double(from.colour))]) }
        if let status { row["status"] = .string(status) }
        guard let data = try? JSONEncoder().encode(JSONValue.object(row)) else { return nil }
        return try? JSONDecoder().decode(MessageRow.self, from: data)
    }

    /// A made-up photo for a card: a head and shoulders, lighter, on `hex`, 120 pixels square, as a PNG.
    static func photo(_ hex: UInt32) -> Data? {
        let side = 120
        guard let ctx = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        let ground = RGBA(hex)
        ctx.setFillColor(ground.cgColor)
        ctx.fill(CGRect(x: 0, y: 0, width: side, height: side))
        ctx.setFillColor(RGBA(0xFFFFFF).mixed(with: ground, 0.35).cgColor)
        ctx.fillEllipse(in: CGRect(x: 38, y: 54, width: 44, height: 44))
        ctx.fillEllipse(in: CGRect(x: 18, y: -30, width: 84, height: 76))
        guard let image = ctx.makeImage() else { return nil }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }
}
