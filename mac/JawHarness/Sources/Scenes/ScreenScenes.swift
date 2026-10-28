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

    static var screensChat: HarnessScene {
        HarnessScene(
            id: "screens-chat", title: "One chat", symbol: "text.bubble",
            synopsis: "The whole window with Lisbon Climbing open, in either look: its header, its messages, the box naming it, and the status line",
            section: .screens, covers: [WindowContent.self, DetailController.self, GridView.self, BoxPlace.self, ConversationController.self,
                                        ComposerController.self, ChatListController.self, SidebarController.self, BottomBar.self, StatusLine.self],
            axes: .look, width: 1180,
        ) { OneChatScene(conditions: $0) }
    }

    static var screensPanes: HarnessScene {
        HarnessScene(
            id: "screens-panes", title: "Three panes", symbol: "rectangle.split.3x1",
            synopsis: "Lisbon Climbing, Ana Ribeiro and Mãe side by side in the layout called Work, the first with the keys",
            section: .screens, covers: [WindowContent.self, DetailController.self, GridView.self, BoxPlace.self, ConversationController.self,
                                        EmptyChatView.self, BottomBar.self, StatusLine.self],
            width: 1180,
        ) { ThreePanesScene(conditions: $0) }
    }

    static var screensFocus: HarnessScene {
        HarnessScene(
            id: "screens-focus", title: "Focus mode", symbol: "rectangle.center.inset.filled",
            synopsis: "Lisbon Climbing open in focus mode, the sidebar and the chat list folded away",
            section: .screens, covers: [WindowContent.self, DetailController.self, GridView.self, BoxPlace.self, ConversationController.self,
                                        BottomBar.self, StatusLine.self],
            width: 1180,
        ) { FocusScene(conditions: $0) }
    }

    static var screensFolded: HarnessScene {
        HarnessScene(
            id: "screens-folded", title: "Folded", symbol: "rectangle.compress.vertical",
            synopsis: "Lisbon Climbing open as the window narrows: the sidebar folded at 940 points, the chat list too at 720, and at 520, the least the window takes",
            section: .screens, covers: [WindowContent.self, DetailController.self, GridView.self, BoxPlace.self, ConversationController.self,
                                        ChatListController.self, BottomBar.self, StatusLine.self],
            width: 940,
        ) { FoldedScene(conditions: $0) }
    }

    static var screensOverlays: HarnessScene {
        HarnessScene(
            id: "screens-overlays", title: "With overlays", symbol: "macwindow.on.rectangle",
            synopsis: "Lisbon Climbing open with the palette over it, with the glance over it, and with Settings over it",
            section: .screens, covers: [WindowContent.self, ConversationController.self, PaletteView.self, GlanceView.self, SettingsView.self],
            width: 1180,
        ) { OverlaysScene(conditions: $0) }
    }
}

/// One screen in a band of its own, with what it should show, and what's open over it, if anything.
private struct ScreenBand: View {
    let title: String
    let note: String
    let screen: Screen
    let conditions: Conditions
    var overlay: ScreenSpecimen.Overlay?

    var body: some View {
        SpecimenGroup(title: title, note: note, roles: conditions.roles) {
            ScreenSpecimen(screen: screen, conditions: conditions, overlay: overlay)
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
                note: "The Mimic awake in the panes' place, \"Starting jawd\" and what jawd is for; the status line says \"starting jawd…\" in the accent, with @ Lisbon Climbing in the mention colour and ⌘J glance on its right. The rest is the window as it always is: its three buttons at the top left and the sidebar's button beside them, the sidebar, the four chats in the list, none selected, and the bar with Unsaved Panes, the split button, Find and New Chat. A picture can't hold the sidebar's selection material: the selected folder is a black pill (in light it hides All), and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
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
                note: "Linking fills the panes' place: jawd's QR code on white beside the Mimic, every module a hard-edged square (a soft edge means it was smoothed as it was scaled), \"Link Jaw to WhatsApp\", the steps on the phone, \"It changes in 45 s.\", held still by the harness's clock, and Use a Phone Number Instead, with \"Jaw isn't affiliated with WhatsApp or Meta.\" under them. The status line says \"not linked\" in the mention colour; the chats jawd kept are in the list, with @ Lisbon Climbing and ⌘J glance on the status line's right. A picture can't hold the sidebar's selection material: the selected folder is a black pill (in light it hides All), and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
                screen: Screen(situation: Situation(reach: .up, linked: false,
                                                    link: .qr(code: "2@jaw-harness,QWERTYUIOPASDFGHJKL,ZXCVBNM=", expires: Samples.now + 45_000))),
                conditions: conditions,
            )
            ScreenBand(
                title: "Logged out",
                note: "The Mimic over \"Logged out\", with jawd's reason, from the phone, that jawd deleted the session and kept your messages, Link Again, grey as in a window that isn't key, and \"Jaw isn't affiliated with WhatsApp or Meta.\" under it; the four chats are in the list, and the status line says \"not linked\" in the mention colour, as not being linked comes before the connection's state, with @ Lisbon Climbing and ⌘J glance on its right.",
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

/// Lisbon Climbing open, linked and online, in the look the scene is drawn in.
private struct OneChatScene: View {
    let conditions: Conditions

    var body: some View {
        ScreenBand(
            title: "Lisbon Climbing open",
            note: "Lisbon Climbing, selected in the chat list, fills the panes' place, 739 points wide beside the sidebar and the list: its header with the LC avatar, its name and \"someone's typing…\" in the online colour, the \"3 new\" line, then Rui's contact card, Ana's link and its preview, your voice message and Inês's album of five; in bubbles, yours on the right and the top of Rui's typing bubble at the pane's foot; in lines, the poll's last option above the \"3 new\" line, each name over its messages with its time on the right, your voice message a third of the way across, and the typing dots alone. The box under the pane names the chat beside \"Message\". The status line says \"someone's typing…\", quieter, and nothing on its right: nothing needs you elsewhere, as Ana's chat has nothing unread and Mãe's is muted. A picture can't hold the sidebar's selection material or the header's blur: the selected folder is a black pill (in light it hides All), what's under the header shows through it (in lines, a time over its right end), and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
            screen: Screen(open: [Samples.group]), conditions: conditions,
        )
    }
}

/// Lisbon Climbing, Ana Ribeiro and Mãe open side by side in the layout called Work, Lisbon Climbing
/// with the keys; Ana's and Mãe's chats have no messages among `Samples`.
private struct ThreePanesScene: View {
    let conditions: Conditions

    var body: some View {
        ScreenBand(
            title: "Three panes, the first with the keys",
            note: "Three panes across the panes' place, 245, 246 and 246 points wide, between hairlines: Lisbon Climbing's, with the keys, outlined inside its edge in the accent, and Ana Ribeiro's and Mãe's dimmed, faintly, as the app's default of 18% does. Lisbon Climbing's has the poll's end above the \"3 new\" line, then its messages as in one chat, its cards' second lines cut short to fit; Ana's and Mãe's are empty chats, as the samples have no messages for them, each the Mimic awake over \"No messages with … yet. What you type goes to them.\" That line doesn't fit: it's one line about 316 points wide, centred on a pane 246 wide, so Ana's runs into Lisbon Climbing's pane and Mãe's, and Mãe's is cut at the window's edge. The app draws it so too, as an empty chat's words aren't held to its pane's width: a fault to fix in JawUI. Ana's header has her name alone and Mãe's says \"muted\". The box under the panes is the focused chat's, Lisbon Climbing's; the bar's layout is Work; the status line says \"someone's typing…\" for Lisbon Climbing and nothing on its right, as Ana's and Mãe's chats are open. A picture can't hold the sidebar's selection material or the header's blur, and the window's buttons are grey; judge those in the app.",
            screen: Screen(open: [Samples.group, Samples.direct] + Samples.chats.filter { $0.name == "Mãe" }, layout: "Work"),
            conditions: conditions,
        )
    }
}

/// Lisbon Climbing open in focus mode.
private struct FocusScene: View {
    let conditions: Conditions

    var body: some View {
        ScreenBand(
            title: "Focus mode",
            note: "The sidebar and the chat list folded away, so Lisbon Climbing's pane is the window's whole width, with its box under it and the bar along the foot. Its header starts at the window's top left, under the title bar: the window's three buttons, grey, at x 9 to 69 and y 9 to 23 points, over the header's left end, then the avatar at x 81 to 109 and y 8 to 36, as the header clears the three buttons, and the name and \"someone's typing…\" from x 120. But the sidebar's button is in the title bar too, and the app draws it over the pane's picture: its glyph, at x 87.5 to 102.5 and y 10 to 22, covers the avatar's upper half, down to its middle and across the top of the initials, LC, which are at y 18 to 26. The header clears only the window's three buttons: a fault to fix in JawUI. The messages are as in one chat, the window's width apart. A picture can't hold the header's blur, so what's under it shows through, behind the buttons and the avatar too, and the window's buttons are grey, as in a window that isn't key; judge those in the app. Focus mode is drawn here alone: the app's own snapshots can't show it, as the build Mac's locked screen never runs the fold's animation.",
            screen: Screen(open: [Samples.group], focusMode: true), conditions: conditions,
        )
    }
}

/// Lisbon Climbing open in a window too narrow for the sidebar, then for the chat list too, then at
/// the least the window takes.
private struct FoldedScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "At 940 by 700 points",
                note: "Below 960 points the sidebar folds away and the chat list stays, at its least, 260 points, with the four chats; the window's three buttons and the sidebar's button sit over its 32-point strip under the title bar, clear of its first row, Ana's, which starts at y 42. Lisbon Climbing's pane, 679 points wide, keeps its header's usual 14 points in from its edge, and shows the \"3 new\" line, Rui's card, Ana's link, your voice message and Inês's album down to the box. The harness draws the title bar's buttons where the window has them with the sidebar shown; if macOS moves them when the sidebar folds, judge that in the app. The app's own snapshots can't show a fold, as the build Mac's locked screen never runs its animation.",
                screen: Screen(open: [Samples.group], width: 940, height: 700), conditions: conditions,
            )
            ScreenBand(
                title: "At 720 by 600 points",
                note: "Below 740 points, with a chat open, the chat list folds too, and Lisbon Climbing's pane is the window's whole width, its header at the window's top left as in focus mode: the window's three buttons over its left end, the avatar at x 81 to 109, and the sidebar's glyph, at x 87.5 to 102.5 and y 10 to 22, over the avatar's upper half and the top of its initials, a fault to fix in JawUI, as focus mode's band says. Rui's card, Ana's link, your voice message and the top of Inês's album come down to the box.",
                screen: Screen(open: [Samples.group], width: 720, height: 600), conditions: conditions,
            )
            ScreenBand(
                title: "At 520 by 420 points, the least the window takes",
                note: "Both folded, as at 720, with the same header and the sidebar's glyph over its avatar. The chat opens at its first unread, so the pane shows Rui's card and Ana's link under the \"3 new\" line, then your voice message under \"1 new ↓\": Inês's album is below what the pane shows, and the pill counts it. The box and the bar fit the width, with Unsaved Panes, the split button, \"someone's typing…\", Find and New Chat.",
                screen: Screen(open: [Samples.group], width: 520, height: 420), conditions: conditions,
            )
        }
    }
}

/// Lisbon Climbing open with the palette, the glance and Settings each open over it, where the window
/// puts them.
private struct OverlaysScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "The palette",
                note: "The palette over the window and its title bar, as its panel is a window above them: centred, at x 280 to 900, its top 60 points below the window's, and as tall as it is, 325 points, as its panel takes its view's size. \"ar\" is typed and selected, the strip counts all, chats 1, people 2, folders 1 and messages 214, each of the four results is under its kind's heading with the letters that matched marked, Ana Ribeiro's selected, and the hints run along its foot. The window under it is as in one chat. A picture can't hold its panel's shadow; judge that in the app.",
                screen: Screen(open: [Samples.group]), conditions: conditions, overlay: .palette,
            )
            ScreenBand(
                title: "The glance",
                note: "The glance centred in the window, at x 330 to 850 and y 260 to 520: 520 by 260 points, the least its panel takes. It leaves out the chats a pane shows, as the app's does, and with Lisbon Climbing open nothing else needs you, as Ana's chat has nothing unread and Mãe's is muted, so it says \"Nothing needs you.\", with \"↩ goes to it · ⌘↩ in a new pane · nothing is marked read\" and \"0 chats · drag an edge to resize\" along its foot. A picture can't hold its panel's shadow; judge that in the app.",
                screen: Screen(open: [Samples.group]), conditions: conditions, overlay: .glance,
            )
            ScreenBand(
                title: "Settings",
                note: "Settings in a sheet at its own size, 480 by 448 points, centred in the window, at x 350 to 830 and y 166 to 614, as macOS puts a sheet: the app's, on the build Mac, comes down to the middle of its window, not from its top. It's drawn on the window's colour with a shadow from the theme's separator, as a picture can't hold a sheet's material or its own shadow. Appearance is chosen in the segmented control, the system's appearance and Jaw (Tokyo Night) in the form under it, with \"Redlamp's themes come in a later version.\", and Done at the foot; the chosen segment and Done are grey, as in a window that isn't key. In the app the sheet is 432 points tall, 16 short of its view, which is worth a look there.",
                screen: Screen(open: [Samples.group]), conditions: conditions, overlay: .settings,
            )
        }
    }
}
