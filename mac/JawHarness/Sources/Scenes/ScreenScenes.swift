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
            synopsis: "Lisbon Climbing, Ana Ribeiro and Mãe side by side in the layout called Work, the first with the keys, and in a window too short for its messages, one of them selected",
            section: .screens, covers: [WindowContent.self, DetailController.self, GridView.self, BoxPlace.self, ConversationController.self,
                                        EmptyChatView.self, BottomBar.self, StatusLine.self],
            width: 1180,
        ) { ThreePanesScene(conditions: $0) }
    }

    static var screensLibrary: HarnessScene {
        HarnessScene(
            id: "screens-library", title: "A chat beside its library", symbol: "photo.on.rectangle.angled",
            synopsis: "Lisbon Climbing with its library beside it in a pane of its own, as its header's button or ⌥⌘L opens it, the library with the keys",
            section: .screens, covers: [WindowContent.self, DetailController.self, GridView.self, BoxPlace.self, ConversationController.self,
                                        LibraryController.self, BottomBar.self, StatusLine.self],
            width: 1180,
        ) { LibraryScreenScene(conditions: $0) }
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
            synopsis: "Lisbon Climbing open with the palette, the glance or Settings over it, and Ana Ribeiro open with the glance listing Lisbon Climbing",
            section: .screens, covers: [WindowContent.self, DetailController.self, GridView.self, BoxPlace.self, ConversationController.self,
                                        EmptyChatView.self, BottomBar.self, StatusLine.self, PaletteView.self, GlanceView.self, SettingsView.self],
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
                note: "Lisbon Climbing, selected in the chat list, opens at its first unread, as a chat with unread messages does: the \"3 new\" line under its header, then Rui's contact card, Ana's link, your voice message and Inês's album, the top of Rui's typing bubble at the pane's foot, and the box under the pane. The status line says \"catching up 1,204 of 5,000\" in the accent, then \"Rui typing…\" in the online colour; nothing needs you elsewhere, so its right is empty. A picture can't hold the sidebar's selection material or the header's blur: the selected folder is a black pill (in light it hides All), what's under the header shows through it, and the window's buttons are grey, as in a window that isn't key; judge those in the app.",
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
            note: "Lisbon Climbing, selected and open: its header says \"Rui typing...\" in the online colour, the three dots still; then come \"3 new\", Rui's card, Ana's link, your voice message, Inês's album and Rui's typing at the foot. In lines, the poll's end sits above \"3 new\", names head their messages, times are on the right and yours start a third of the way across. The box names the chat; the chat list and the status line say \"Rui typing…\" in the online colour, the status line's right empty, as the one chat that needs you is open: the glance band with Ana's chat open shows it filled. A picture can't hold the sidebar's selection (a black pill), the header's blur (what's under it shows through) or a key window's buttons.",
            screen: Screen(open: [Samples.group]), conditions: conditions,
        )
    }
}

/// Lisbon Climbing, Ana Ribeiro and Mãe open side by side in the layout called Work, Lisbon Climbing
/// with the keys; Ana's and Mãe's chats have no messages among `Samples`. Then the same in a window
/// too short for Lisbon Climbing's messages, Ana's link selected among them.
private struct ThreePanesScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "Three panes, the first with the keys",
                note: "The app's fault, to fix in JawUI: an empty chat's words keep to one line, cut short, whatever its pane, so Ana's and Mãe's \"No messages with … yet. What you type goes to…\" run over the panes beside them and off the window's edge. Lisbon Climbing's pane has the keys, outlined in the accent, its cards' second lines cut short; the others are dimmed, faintly, by the app's default 18%. Mãe's header says \"muted\"; the box is Lisbon Climbing's, the bar says Work, and the status line's right is empty, as the one chat that needs you is open. A picture can't hold the sidebar's selection, the header's blur or a key window's buttons.",
                screen: Screen(open: [Samples.group, Samples.direct, Self.mae], layout: "Work"),
                conditions: conditions,
            )
            ScreenBand(
                title: "At 1180 by 560 points, Ana's link selected",
                note: "Lisbon Climbing's pane, shorter, opens at its first unread with its messages running on past its foot, and Ana's link is selected, its band across the pane: Inês's album is cut where the outline runs, and the band stops at the outline on either side, nothing of either drawn between the outline and the pane's edge, where there's only the ground. The other panes, the box and the bar are as above.",
                screen: Screen(open: [Samples.group, Samples.direct, Self.mae], layout: "Work", height: 560, selected: "K6"),
                conditions: conditions,
            )
        }
    }

    /// Mãe's chat, which `Samples` has only among its chats.
    static var mae: ChatRow {
        guard let mae = Samples.chats.first(where: { $0.name == "Mãe" }) else {
            preconditionFailure("Samples.chats has no chat called Mãe for the three panes")
        }
        return mae
    }
}

/// Lisbon Climbing beside its library.
private struct LibraryScreenScene: View {
    let conditions: Conditions

    var body: some View {
        ScreenBand(
            title: "Lisbon Climbing beside its library",
            note: "The library's pane has the keys, outlined in the accent, the chat's dimmed beside it; its header says Lisbon Climbing and how many it shared, the strip every kind with its count, and every kind's shelves hold only the group's items: its photos and videos, files, links and Inês's voice message, each saying who without saying where; the links run on past the pane's foot, cut where the outline runs, nothing of them between it and the pane's edge. The box under the panes says the library's keys in place of a composer.",
            screen: Screen(open: [Samples.group], library: .chat(id: Samples.group.id, name: Samples.group.name)),
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
            note: "The app's fault, to fix in JawUI: the sidebar's button, in the title bar beside the window's three, sits on the pane's avatar, its glyph, at x 87.5–102.5 and y 10–22, over the avatar's upper half and the top of LC, as the header clears only the three buttons. Otherwise the sidebar and the chat list are folded away, and Lisbon Climbing's pane, its box and the bar take the window's width, the messages as in one chat, laid across it. A picture can't hold the header's blur (what's under it shows through) or a key window's buttons; the app's own snapshots can't show focus mode, so this is its only picture.",
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
                note: "Below 960 points the sidebar folds away and the chat list stays, at its least: the window's buttons and the sidebar's sit over its strip under the title bar, clear of its first row, Ana's. Lisbon Climbing's pane keeps its header's usual inset and shows the \"3 new\" line, Rui's card, Ana's link, your voice message and Inês's album down to the box. Whether macOS moves the title bar's buttons when the sidebar folds is for the app, whose own snapshots can't show a fold.",
                screen: Screen(open: [Samples.group], width: 940, height: 700), conditions: conditions,
            )
            ScreenBand(
                title: "At 720 by 600 points",
                note: "At the top left, the sidebar's glyph sits on the pane's avatar: the app's fault that focus mode's band describes. Below 740 points, with a chat open, the chat list folds too, and Lisbon Climbing's pane takes the window's width; Rui's card, Ana's link, your voice message and the top of Inês's album come down to the box.",
                screen: Screen(open: [Samples.group], width: 720, height: 600), conditions: conditions,
            )
            ScreenBand(
                title: "At 520 by 420 points, the least the window takes",
                note: "At the top left, the sidebar's glyph sits on the pane's avatar, the app's fault, as at 720. Both are folded, and the chat opens at its first unread: Rui's card and Ana's link under the \"3 new\" line, then your voice message under \"1 new ↓\", the pill counting Inês's album below. The box and the bar fit the width, with Unsaved Panes, the split button, \"Rui typing…\", Find and New Chat.",
                screen: Screen(open: [Samples.group], width: 520, height: 420), conditions: conditions,
            )
        }
    }
}

/// Lisbon Climbing open with the palette, the glance and Settings each open over it, and Ana Ribeiro
/// open with the glance over her chat, listing Lisbon Climbing, each where the window puts it.
private struct OverlaysScene: View {
    let conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenBand(
                title: "The palette",
                note: "The palette as its own scene draws it, over the window: centred, its top 60 points down, below the title bar, and as tall as its view, 325 points with these four results. \"ar\" shows selected, as the field selects what's in it when it opens with a query; typing would leave the caret after it. The window under it is as in one chat. A picture can't hold the panel's shadow.",
                screen: Screen(open: [Samples.group]), conditions: conditions, overlay: .palette,
            )
            ScreenBand(
                title: "The glance, with Lisbon Climbing open",
                note: "The glance centred in the window, 520 by 260 points, the least its panel takes. It leaves out the chats a pane shows, as the app's does, and with Lisbon Climbing open nothing else needs you (Ana's chat has nothing unread, Mãe's is muted), so it says \"Nothing needs you.\" and \"0 chats\", as the app's glance does then. A picture can't hold the panel's shadow.",
                screen: Screen(open: [Samples.group]), conditions: conditions, overlay: .glance,
            )
            ScreenBand(
                title: "The glance, with Ana Ribeiro open",
                note: "The app's fault, as in Three panes: under the glance, Ana's empty chat cuts its line at \"goes to…\". The glance lists the chat that needs you, Lisbon Climbing: \"@\" in the mention colour, \"3 unread\" and its newest three, \"Ana climb.pt/sintra\", \"You voice\" and \"Inês album\", as the app's glance names media by its kind's bare word. One card asks for less than its panel's least, so it's 520 by 260, centred. The status line's right shows what needs you: \"@ Lisbon Climbing\" in the mention colour, then \"⌘J glance\". A picture can't hold the panel's shadow.",
                screen: Screen(open: [Samples.direct]), conditions: conditions, overlay: .glance,
            )
            ScreenBand(
                title: "Settings",
                note: "The app's fault, most likely: its sheet is 432 points tall, 16 short of the 448 its view takes here, which would cut 8 from its top and its foot; look in the app. It's centred in the window, as the app's sheet is on the build Mac. Appearance is chosen, the system's appearance and Jaw (Tokyo Night) under it; the segment and Done are grey, as in a window that isn't key. A picture can't hold a sheet's material or shadow, so it's on the window's colour with the separator's shadow.",
                screen: Screen(open: [Samples.group]), conditions: conditions, overlay: .settings,
            )
        }
    }
}
