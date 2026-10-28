import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var screensJawd: HarnessScene {
        HarnessScene(
            id: "screens-jawd", title: "Reaching jawd", symbol: "power",
            synopsis: "The whole window while jawd starts, when it didn't start, when it's out of reach, and while the app connects to it",
            section: .screens, covers: [WindowContent.self, DetailController.self, BottomBar.self, StateView.self, StatusLine.self],
            width: 1180,
        ) { ReachingJawdScene(conditions: $0) }
    }

    static var screensConnection: HarnessScene {
        HarnessScene(
            id: "screens-connection", title: "The connection", symbol: "antenna.radiowaves.left.and.right",
            synopsis: "Lisbon Climbing open while the connection catches up, the phone or the Mac is offline, an update is needed, the account is banned and the engine restarts",
            section: .screens, covers: [WindowContent.self, DetailController.self, BottomBar.self, StatusLine.self],
            width: 1180,
        ) { ConnectionScene(conditions: $0) }
    }

    static var screensLinking: HarnessScene {
        HarnessScene(
            id: "screens-linking", title: "Linking", symbol: "qrcode",
            synopsis: "The whole window with the account not linked, showing jawd's QR code, and logged out from the phone",
            section: .screens, covers: [WindowContent.self, DetailController.self, BottomBar.self, StatusLine.self, LinkingView.self],
            width: 1180,
        ) { LinkingScene(conditions: $0) }
    }

    static var screensEmpty: HarnessScene {
        HarnessScene(
            id: "screens-empty", title: "Nothing open", symbol: "rectangle.dashed",
            synopsis: "The whole window, linked and online, with no chats yet, and with chats but none open",
            section: .screens, covers: [WindowContent.self, DetailController.self, BottomBar.self, StateView.self, StatusLine.self],
            width: 1180,
        ) { NothingOpenScene(conditions: $0) }
    }
}

/// One screen in a band of its own, with what it should show.
private struct ScreenBand: View {
    let title: String
    let note: String
    let screen: Screen
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(title: title, note: note, roles: conditions.roles) {
            ScreenSpecimen(screen: screen, conditions: conditions)
        }
    }
}

/// The window while jawd starts, once it didn't start, out of reach before any chats came, and
/// while the app connects to it, before any chats come.
private struct ReachingJawdScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "Starting jawd",
                note: "The Mimic awake in the panes' place, \"Starting jawd\" and what jawd is for; the status line says \"starting jawd…\" in the accent, with @ Lisbon Climbing in the mention colour and ⌘J glance on its right. The rest is the window as it always is: its three buttons at the top left, the sidebar, the four chats in the list, none selected, and the bar with Unsaved Panes, the split button, Find and New Chat. A picture can't hold the sidebar's selection material: the selected folder is a black pill (in light it hides All), and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
                screen: Screen(situation: Situation(reach: .starting(failed: nil))), conditions: conditions,
            )
            ScreenBand(
                title: "jawd didn't start",
                note: "The Mimic over \"jawd didn't start\" in the panes' place, with jawd's own words under it, \"It didn't start: jawd stopped as it started (status 1)\"; the status line says \"jawd didn't start\" in the mention colour, as it warns. The rest of the window is as while jawd starts.",
                screen: Screen(situation: Situation(reach: .starting(failed: "jawd stopped as it started (status 1)"))), conditions: conditions,
            )
            ScreenBand(
                title: "Out of reach, with nothing drawn yet",
                note: "No chats came before jawd went: the Mimic asleep (grey, eyes shut), \"jawd isn't answering\", why, in jawd's words, and that Jaw tries again by itself in 2 s; the status line says \"can't reach jawd · trying again in 2 s\" in the mention colour. The chat list is empty, and nothing is on the status line's right.",
                screen: Screen(situation: Situation(reach: .unreachable(why: "jawd isn't running: nothing answers on ~/Library/Application Support/jaw/jaw.sock", retry: 2)),
                               chats: []),
                conditions: conditions,
            )
            ScreenBand(
                title: "Connecting to jawd",
                note: "The chat list is empty, as it is until jawd first answers: the Mimic over \"No chat open\" in the panes' place, with ⌘K and ⌘N; the status line says \"connecting to jawd…\" in the accent, with nothing on its right.",
                screen: Screen(situation: Situation(reach: .waiting), chats: []), conditions: conditions,
            )
        }
    }
}

/// Lisbon Climbing open while the connection catches up, and in each state of it that warns.
private struct ConnectionScene: View {
    let conditions: Conditions

    func screen(_ connection: Connection) -> Screen {
        Screen(situation: Situation(reach: .up, connection: connection), open: [Samples.group])
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "Catching up",
                note: "Lisbon Climbing, selected in the chat list, opens at its first unread, as a chat with unread messages does: the \"3 new\" line under its header, then Rui's contact card, Ana's link, your voice message and Inês's album, the top of Rui's typing bubble at the pane's foot, and the box under the pane. The status line says \"catching up 1,204 of 5,000\" in the accent, then \"someone's typing…\", quieter; nothing needs you elsewhere, so its right is empty. A picture can't hold the sidebar's selection material or the header's blur: the selected folder is a black pill (in light it hides All), what's under the header shows through it, and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
                screen: screen(Connection(state: .catchingUp, done: 1204, total: 5000)), conditions: conditions,
            )
            ScreenBand(
                title: "The phone offline",
                note: "The status line says \"phone offline: photos and older history wait for it\" in the mention colour; the chat and the rest of the window are as while catching up.",
                screen: screen(Connection(state: .phoneOffline)), conditions: conditions,
            )
            ScreenBand(
                title: "Offline",
                note: "\"offline: what you send waits until it's back\" in the mention colour; nothing else changes.",
                screen: screen(Connection(state: .offline)), conditions: conditions,
            )
            ScreenBand(
                title: "An update needed",
                note: "\"update needed: WhatsApp wants a newer Jaw · reading still works\" in the mention colour; nothing else changes.",
                screen: screen(Connection(state: .updateNeeded)), conditions: conditions,
            )
            ScreenBand(
                title: "Banned",
                note: "\"temporarily banned until 19 October, 15:02\" in the mention colour, three days after the samples' now in the Mac's time zone; nothing else changes.",
                screen: screen(Connection(state: .banned, until: Samples.now + 3 * 86_400_000)), conditions: conditions,
            )
            ScreenBand(
                title: "The engine restarting",
                note: "\"reconnecting: the engine restarted\" in the mention colour; nothing else changes.",
                screen: screen(Connection(state: .engineRestarting)), conditions: conditions,
            )
        }
    }
}

/// The account not linked, with jawd's QR code to scan, and logged out from the phone.
private struct LinkingScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "Not linked, with a QR code",
                note: "Linking fills the panes' place: jawd's QR code on white beside the Mimic, every module a hard-edged square (a soft edge means it was smoothed as it was scaled), \"Link Jaw to WhatsApp\", the steps on the phone, \"It changes in 45 s.\", held still by the harness's clock, and Use a Phone Number Instead, with \"Jaw isn't affiliated with WhatsApp or Meta.\" under them. The status line says \"not linked\" in the mention colour; the chats jawd kept are in the list, with @ Lisbon Climbing and ⌘J glance on the status line's right. A picture can't hold the sidebar's selection material: the selected folder is a black pill (in light it hides All), and Link Again and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
                screen: Screen(situation: Situation(reach: .up, linked: false,
                                                    link: .qr(code: "2@jaw-harness,QWERTYUIOPASDFGHJKL,ZXCVBNM=", expires: Samples.now + 45_000))),
                conditions: conditions,
            )
            ScreenBand(
                title: "Logged out",
                note: "The Mimic over \"Logged out\", with jawd's reason, from the phone, that jawd deleted the session and kept your messages, Link Again, and \"Jaw isn't affiliated with WhatsApp or Meta.\" under it; the four chats are in the list, and the status line says \"not linked\" in the mention colour, as not being linked comes before the connection's state, with @ Lisbon Climbing and ⌘J glance on its right.",
                screen: Screen(situation: Situation(reach: .up, linked: false,
                                                    connection: Connection(state: .loggedOut, reason: "logged out from the phone"))),
                conditions: conditions,
            )
        }
    }
}

/// The window linked and online with no chats yet, and with the chats there but none open.
private struct NothingOpenScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "No chats yet",
                note: "Linked and online before the phone has sent a chat: the Mimic over \"No chats yet\" and \"They come as your phone sends them.\" in the panes' place, the chat list empty, and nothing on the status line, as nothing is wrong and nothing needs you. A picture can't hold the sidebar's selection material: the selected folder is a black pill (in light it hides All), and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
                screen: Screen(chats: []), conditions: conditions,
            )
            ScreenBand(
                title: "No chat open",
                note: "The four chats, none open: the Mimic over \"No chat open\" in the panes' place, with ⌘K and ⌘N; nothing on the status line's left, as jawd is online, and @ Lisbon Climbing and ⌘J glance on its right.",
                screen: Screen(), conditions: conditions,
            )
        }
    }
}
