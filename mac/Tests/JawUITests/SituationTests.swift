import Foundation
@testable import JawKit
@testable import JawUI
import Testing

/// What the screen says, from jawd's situation alone: the status line's left half, how the focused
/// chat is, what fills the panes' place, and the chats that need you, in the app's own words.

func situation(_ state: ConnectionState = .online, reach: Situation.Reach = .up, linked: Bool = true, done: UInt64? = nil,
               total: UInt64? = nil, until: Int64? = nil, reason: String? = nil) -> Situation
{
    Situation(reach: reach, linked: linked,
              connection: Connection(state: state, done: done, total: total, until: until, reason: reason))
}

@Test func `the status line says how jawd is reached and how the connection is, in the app's words`() {
    let said: [(Situation, String?, Bool)] = [
        (situation(reach: .waiting), "connecting to jawd…", false),
        (situation(reach: .starting(failed: nil)), "starting jawd…", false),
        (situation(reach: .starting(failed: "status 1")), "jawd didn't start", true),
        (situation(reach: .unreachable(why: "nothing answers", retry: 2)), "can't reach jawd · trying again in 2 s", true),
        (situation(linked: false), "not linked", true),
        (situation(.online), nil, false),
        (situation(.connecting), "connecting", false),
        (situation(.catchingUp), "catching up", false),
        (situation(.catchingUp, done: 1204, total: 5000), "catching up 1,204 of 5,000", false),
        (situation(.phoneOffline), "phone offline: photos and older history wait for it", true),
        (situation(.offline), "offline: what you send waits until it's back", true),
        (situation(.updateNeeded), "update needed: WhatsApp wants a newer Jaw · reading still works", true),
        (situation(.banned), "temporarily banned until later", true),
        (situation(.engineRestarting), "reconnecting: the engine restarted", true),
        (situation(.loggedOut), "logged out", true),
        (situation(.other, reason: "on fire"), "connection: on fire", true),
        (situation(.other), "connection: a state this Jaw doesn't know", true),
    ]
    for (s, text, warn) in said {
        #expect(s.stateLine?.text == text, "\(s)")
        #expect((s.stateLine?.warn ?? false) == warn, "\(s)")
    }
    #expect(situation(.banned, until: Samples.now).stateLine?.text == "temporarily banned until \(Format.until(Samples.now))")
}

@Test func `the focused chat's line says it's typing or muted, and what was said while it lasts`() {
    let now = Date(timeIntervalSince1970: 1_000_000)
    var s = situation()
    #expect(s.chatLine(focused: nil, now: now) == nil)
    #expect(s.chatLine(focused: Samples.group, now: now) == "someone's typing…", "a group says someone")
    var direct = Samples.direct
    direct.typing = ["person:4"]
    #expect(s.chatLine(focused: direct, now: now) == "typing…")
    direct.mutedUntil = -1
    #expect(s.chatLine(focused: direct, now: now) == "typing… · muted")
    direct.typing = []
    #expect(s.chatLine(focused: direct, now: now) == "muted")
    s.said = Situation.Said("not sent: no connection", until: now.addingTimeInterval(4), warn: true)
    #expect(s.chatLine(focused: direct, now: now) == "not sent: no connection", "what was said comes first while it lasts")
    #expect(s.chatLine(focused: direct, now: now.addingTimeInterval(5)) == "muted", "and goes once it's over")
}

@Test func `the panes' place shows what jawd's situation calls for`() {
    #expect(situation(reach: .starting(failed: nil)).stateView(hasChats: true, hasPanes: true) == .starting(failed: nil))
    #expect(situation(reach: .starting(failed: "status 1")).stateView(hasChats: false, hasPanes: false) == .starting(failed: "status 1"))
    let away = situation(reach: .unreachable(why: "nothing answers", retry: 2))
    #expect(away.stateView(hasChats: false, hasPanes: false) == .asleep(why: "nothing answers", retryIn: 2))
    #expect(away.stateView(hasChats: true, hasPanes: true) == nil, "what's drawn stays while jawd is away")
    #expect(away.stateView(hasChats: true, hasPanes: false) == .noChat)
    #expect(situation(linked: false).stateView(hasChats: true, hasPanes: true) == .linking(step: nil, loggedOut: nil))
    #expect(situation(.loggedOut, linked: false).stateView(hasChats: true, hasPanes: true)
        == .linking(step: nil, loggedOut: "logged out from the phone"))
    #expect(situation(.loggedOut, linked: false, reason: "you logged out on the phone").stateView(hasChats: true, hasPanes: true)
        == .linking(step: nil, loggedOut: "you logged out on the phone"))
    #expect(situation().stateView(hasChats: false, hasPanes: false) == .noChats)
    #expect(situation().stateView(hasChats: true, hasPanes: false) == .noChat)
    #expect(situation(reach: .waiting).stateView(hasChats: false, hasPanes: false) == .noChat, "no chats yet only once jawd is up")
    #expect(situation().stateView(hasChats: true, hasPanes: true) == nil)
}

@Test func `the chats that need you are mentions, then people, then groups, by unread, and none a pane shows`() {
    func chat(_ id: String, _ kind: ChatKind = .direct, unread: Int = 0, mentions: Int = 0) -> ChatRow {
        ChatRow(id: id, v: 1, sort: id, kind: kind, name: id, unread: unread, mentions: mentions)
    }
    var muted = chat("muted", unread: 9)
    muted.mutedUntil = -1
    var archived = chat("archived", unread: 9)
    archived.archived = true
    var marked = chat("marked")
    marked.markedUnread = true
    let chats = [
        chat("group", .group, unread: 7), chat("ana", unread: 2), chat("mention", .group, unread: 1, mentions: 1),
        chat("rui", unread: 5), chat("read"), muted, archived, chat("community", .community, unread: 3), marked,
        chat("shown", unread: 4),
    ]
    #expect(Situation.needs(chats, shown: ["shown"]).map(\.id) == ["mention", "rui", "ana", "marked", "group"])
}
