import Foundation
import JawKit

/// Made-up rows for the harness and the snapshot tests: every span, every tick, replies, reactions,
/// edits, deletes, forwards, a failed message, system and call lines, a long message that folds,
/// emoji alone, and each media kind's line, in a group and a direct chat, across two days.
public enum Samples {
    /// A fixed now, so what's drawn doesn't depend on when it's drawn.
    public static let now: Int64 = 1_792_159_320_000

    public static let group = ChatRow(
        id: "120363012345678901@g.us", v: 40, sort: "0~1792144500000~4", kind: .group, name: "Lisbon Climbing",
        last: Last(from: "Rui", mine: false, summary: "Saturday at 9?", at: now - 60000), unread: 3, mentions: 1,
        typing: ["person:7"],
    )

    public static let direct = ChatRow(
        id: "351912345678@s.whatsapp.net", v: 42, sort: "1~1792145060000~17", name: "Ana Ribeiro",
        last: Last(from: nil, mine: true, summary: "yes, the one at the lake", status: .read, at: now - 120_000),
        pinned: true, draft: "see you",
    )

    public static var chats: [ChatRow] {
        var muted = ChatRow(id: "351913000000@s.whatsapp.net", v: 30, sort: "0~1792100000000~9", name: "Mãe",
                            last: Last(from: nil, mine: false, summary: "have you eaten?", at: now - 3 * 86_400_000), unread: 1)
        muted.mutedUntil = -1
        let community = ChatRow(id: "120363000000000009@g.us", v: 20, sort: "0~1792000000000~3", kind: .community,
                                name: "Climbing Portugal",
                                last: Last(from: "Sofia", mine: false, summary: "new routes at Sintra", at: now - 9 * 86_400_000))
        return [direct, group, muted, community]
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
            m(16, [Span(.text, "sunset from the top")], at: now - 30000, from: rui, kind: .image,
              reactions: [Reaction(emoji: "🔥", n: 1, mine: false), Reaction(emoji: "😍", n: 3, mine: false)]),
        ]
    }

    /// The window update a watch of those messages returns.
    public static var update: Update<MessageRow> {
        Update(seq: 200, upsert: messages, total: UInt64(messages.count), older: false, newer: 0,
               moved: Moved(place: .newest(100), newer: 0))
    }
}
