# Jaw for the Mac: the right-click menus, implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The chat's menus (a message's menu ending with the pane's splits, the pane's menu on its header and around its messages) and the chat list's menu (Open in New Pane, Pin, Mute, Archive, Mark as Read or Unread), as [the menus and folders design](2026-10-27-jaw-mac-menus-and-folders-design.md) has them, build-order steps 1 and 2, and Star once jawd serves it.

**Architecture:** A right-click menu is a list of `MenuEntry` values (a command by id with an optional title, a submenu, a separator) worked out from a row by `ChatMenu`, a pure type in JawUI that's tested without AppKit, and made into `NSMenuItem`s by `Menus.menu(_:command:)` as the menu opens. The app writes the chat's actions once, in `chatCommands(for:)`, and binds them to the focused pane's chat for the menu bar and the palette, and to the chat right-clicked for its menu. The chat list asks its host for a row's items through its table's menu delegate, so AppKit outlines the row; the conversation asks for the pane's menu from its header, its table and its system lines, and ends a message's menu with the pane's splits, the pane taking the keys first.

**Tech Stack:** Swift 6 (language mode 6, warnings as errors), AppKit, Swift Testing, Tuist through mise; builds and tests on the build Mac through `mac/scripts/build-mac`.

## Global Constraints

- macOS 26 and later, Swift 6 language mode, warnings as errors (`mac/Project.swift`).
- No font, radius or line width set by hand outside `JawUI/Sources/Components`: `mac/scripts/check-components` fails the build on one.
- Each command is written once, with its menu title, palette phrase, key and menu; menus, the palette and the keys sheet are made from commands (`mac/README.md`).
- The words are the design's: "Open in New Pane", "Pin", "Unpin", "Mute" with "For 8 Hours", "For a Week", "Always", "Unmute", "Archive", "Unarchive", "Mark as Read", "Mark as Unread", "Split Across", "Split Down", "Close Pane", "Swap" with "Left", "Right", "Up", "Down", "Search This Chat", "Star", "Unstar".
- A debug build never opens the owner's jaw folder; the app's snapshot runs `--demo`. Nothing here touches jawd or the owner's processes.
- Tests run on the build Mac: `mac/scripts/build-mac test JawUITests` (JawUI's tests) and `mac/scripts/build-mac build jaw` (the app). Each prints only jaw's diagnostics and `** TEST SUCCEEDED **` or `** BUILD SUCCEEDED **`.
- Commits are on `mac-menus` in `~/src/jaw-mac-menus`, in the repository's style: one or more plain sentences saying what changed and why, no prefix. `mac-menus` rebases onto `mac-m1`'s tip, again if `mac-m1` moves before the hand-over, and the Mac builder fast-forwards `mac-m1` to it. It isn't merged: `mac-m1` is rebased onto main from time to time, and a merge would bring its old commits back beside their twins.

## Files

- `mac/JawUI/Sources/Commands/Command.swift` (modify): `MenuEntry`, and `Menus.menu(_:command:)` and `Menus.items(_:command:)`, which make a right-click menu from entries.
- `mac/JawUI/Sources/ChatList/ChatMenu.swift` (create): `ChatMenu`, the entries of the chat list's menu and the pane's menu from a row; `MuteLength`, the three mute lengths and their ends counted when chosen.
- `mac/JawUI/Sources/ChatList/ChatListController.swift` (modify): the table's menu, filled for the row right-clicked from `ChatListHost.chatList(_:menuItemsFor:)`.
- `mac/JawUI/Sources/Conversation/ConversationController.swift` (modify): `paneMenu` and `messageMenuTail` in place of `headerMenu`; `TranscriptTable`; `HeaderView.menuFor`; a message's menu ending with the tail; `menu(forRow:)`; Star (task 7).
- `mac/JawApp/Sources/ChatMenus.swift` (create): `chatCommands(for:)`, `openInNewPane(_:)`, `chatMenuItems(for:)`, `paneMenu(for:)`.
- `mac/JawApp/Sources/Commands.swift` (modify): the Chat section takes `chatCommands(for:)`.
- `mac/JawApp/Sources/AppModel.swift` (modify): `makePane` sets `paneMenu` and `messageMenuTail`; `ChatListHost` gains the menu's items.
- `mac/JawApp/Sources/MainWindowController.swift` (modify): an empty pane's menu from `ChatMenu.splits`, and a pane showing a chat its whole menu on the strips and controls no view claims (task 5's review).
- `mac/JawApp/Sources/Bench.swift` (modify): the `menu:` steps of `--keys`.
- `mac/JawApp/Sources/Glance.swift` (modify): ⌘↩ through `openInNewPane(_:)` (the branch's review).
- `mac/JawUI/Sources/Conversation/MessageView.swift` and `mac/JawUI/Sources/Layout/MessageLayout.swift` (modify): a message answers a right click only on itself, `Laid.contains` (task 5's review), and VoiceOver's Show Menu (the branch's review).
- `mac/README.md` (modify): the `menu:` steps.
- Tests (create): `mac/Tests/JawUITests/MenuEntryTests.swift`, `ChatMenuTests.swift`, `ChatListMenuTests.swift`, `PaneMenuTests.swift`; (modify, task 7) `TypingTests.swift`, whose `Reader` is a `ConversationHost`.

---

### Task 1: Right-click menus made from entries

Amended after its review (3f630c9): an item under an entry's own title says what it will do, so it has no tick, never "✓ Unpin", neither as `Menus.items` makes it nor when a menu that enables its items checks it (`CommandBox.ticks`, read by `CommandTarget.validateMenuItem`); an item is enabled and ticked by that one rule; and the tests cover a separator where two met, a submenu that can run and a setting that's off.

**Files:**
- Modify: `mac/JawUI/Sources/Commands/Command.swift` (after `Menus.menu(_:title:)`, and a new enum at the end of the file)
- Test: `mac/Tests/JawUITests/MenuEntryTests.swift`

**Interfaces:**
- Produces: `public enum MenuEntry: Equatable, Sendable { case command(String, title: String? = nil); case submenu(String, [MenuEntry]); case separator }`; `Menus.menu(_ entries: [MenuEntry], command: (String) -> Command?) -> NSMenu`; `Menus.items(_ entries: [MenuEntry], command: (String) -> Command?) -> [NSMenuItem]`.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/MenuEntryTests.swift`:

```swift
import AppKit
@testable import JawUI
import Testing

/// Right-click menus are made from entries worked out from rows: a command under a title saying
/// what it will do, submenus, separators only between items, and what can't run greyed.

@MainActor
@Test func `a right-click menu takes each command's title or the entry's, enabled and ticked as it stands`() {
    let pinned = Ran()
    let commands = [
        Command("pin", "Pinned", place: .chat, on: { pinned.flag }) {},
        Command("pane.close", "Close Pane", key: Shortcut("w"), place: .file, enabled: { false }) {},
    ]
    let menu = Menus.menu([.command("pin", title: "Unpin"), .separator, .command("pane.close")]) { id in
        commands.first { $0.id == id }
    }
    #expect(menu.items.map { $0.isSeparatorItem ? "—" : $0.title } == ["Unpin", "—", "Close Pane"])
    #expect(menu.items[0].state == .on && menu.items[0].isEnabled)
    #expect(!menu.items[2].isEnabled, "greyed, as it can't run")
    #expect(menu.items[2].keyEquivalent == "w", "its key shown")
    #expect(!menu.autoenablesItems, "made as it opens, so as things stand then")
}

@MainActor
@Test func `an entry with no command, an empty submenu and a stray separator are left out`() {
    let commands = [Command("swap.left", "Swap Left", place: .panes, enabled: { false }) {}]
    let entries: [MenuEntry] = [
        .separator, .command("nowhere"), .submenu("Empty", [.command("nowhere")]), .separator,
        .submenu("Swap", [.command("swap.left", title: "Left")]), .separator,
    ]
    let menu = Menus.menu(entries) { id in commands.first { $0.id == id } }
    #expect(menu.items.map(\.title) == ["Swap"])
    #expect(menu.items[0].submenu?.items.map(\.title) == ["Left"])
    #expect(!menu.items[0].isEnabled, "a submenu whose items can't run is greyed")
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with errors in `MenuEntryTests.swift`: `cannot find type 'MenuEntry' in scope` and `extra trailing closure passed in call`.

- [ ] **Step 3: Write the code**

In `mac/JawUI/Sources/Commands/Command.swift`, inside `public enum Menus`, after `public static func menu(_ commands: [Command], title: String = "") -> NSMenu { … }`, add:

```swift
    /// A right-click menu from its entries, each command found by `command`: an entry with no command
    /// is left out, a submenu with nothing in it too, and a separator only stands between items.
    /// Items are enabled and ticked as things stand when it's made, which is as it opens.
    public static func menu(_ entries: [MenuEntry], command: (String) -> Command?) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in items(entries, command: command) {
            menu.addItem(item)
        }
        return menu
    }

    /// The items `menu(_:command:)` makes, to go on the end of another menu.
    public static func items(_ entries: [MenuEntry], command: (String) -> Command?) -> [NSMenuItem] {
        var out: [NSMenuItem] = []
        for entry in entries {
            switch entry {
            case let .command(id, title):
                guard let c = command(id) else { continue }
                let item = c.menuItem(title: title)
                item.isEnabled = c.enabled()
                if let on = c.on { item.state = on() ? .on : .off }
                out.append(item)
            case let .submenu(title, inner):
                let made = items(inner, command: command)
                guard !made.isEmpty else { continue }
                let sub = NSMenu(title: title)
                sub.autoenablesItems = false
                for item in made {
                    sub.addItem(item)
                }
                let holder = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                holder.submenu = sub
                holder.isEnabled = made.contains(where: \.isEnabled)
                out.append(holder)
            case .separator:
                if let last = out.last, !last.isSeparatorItem { out.append(.separator()) }
            }
        }
        while out.last?.isSeparatorItem == true {
            out.removeLast()
        }
        return out
    }
```

At the end of the file, add:

```swift
/// One entry of a right-click menu: a command by its id, under a title of its own when that says
/// what it will do (Unpin); a submenu; a separator. A menu's entries are worked out from rows, so
/// what it offers is tested without AppKit, and made into items as it opens.
public enum MenuEntry: Equatable, Sendable {
    case command(String, title: String? = nil)
    case submenu(String, [MenuEntry])
    case separator
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the two new tests among those passed.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources/Commands/Command.swift mac/Tests/JawUITests/MenuEntryTests.swift
git commit -m "Right-click menus are made from entries worked out from rows, a command by its id under a title saying what it will do, a submenu or a separator, so what a menu offers is tested without AppKit: Menus.menu(_:command:) makes them into items as the menu opens, enabled and ticked as things stand then, leaving out an entry with no command, an empty submenu and a separator with nothing on one side, and greying a submenu none of whose items can run"
```

---

### Task 2: What a chat's menus offer

Amended after the branch's review: a chat's two rules are written once, as `ChatRow.hasChatActions` (it isn't a community) and `ChatRow.readsAsUnread` (`unread > 0 || markedUnread`), in `ChatMenu.swift`. `ChatMenu.list` and `ChatMenu.actions` read them, as task 3's `chatCommands(for:)` does, where the plan's code wrote them out in each. The community test has a pinned, muted and unread community, which still offers nothing, and its announcements group, of kind `.announcement`, which offers everything.

**Files:**
- Create: `mac/JawUI/Sources/ChatList/ChatMenu.swift`
- Test: `mac/Tests/JawUITests/ChatMenuTests.swift`

**Interfaces:**
- Consumes: `MenuEntry` (task 1); `ChatRow` (`pinned`, `muted`, `archived`, `unread`, `markedUnread`, `kind`).
- Produces: `public enum MuteLength: CaseIterable, Sendable` with `id: String` (`"mute.8h"`, `"mute.week"`, `"mute.always"`), `title: String` and `until(now: Int64) -> Int64`; `public enum ChatMenu` with `static let openInNewPane = "chat.open.pane"`, `static let splits: [MenuEntry]`, `static func list(_ chat: ChatRow) -> [MenuEntry]`, `static func pane(_ chat: ChatRow) -> [MenuEntry]`, `static func actions(_ chat: ChatRow) -> [MenuEntry]`. Swap ▸ is always in the pane's menu; its items can't run with one pane, so task 1's rule greys it. Command ids used: `pin`, `mute.8h`, `mute.week`, `mute.always`, `unmute`, `archive`, `mark`, `split.across`, `split.down`, `pane.close`, `swap.left`, `swap.right`, `swap.up`, `swap.down`, `search.chat`, `chat.open.pane`.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/ChatMenuTests.swift`:

```swift
@testable import JawKit
@testable import JawUI
import Testing

/// A chat's menus say what each item will do as the chat stands, and leave out what doesn't apply.

func titles(_ entries: [MenuEntry]) -> [String] {
    entries.map { entry in
        switch entry {
        case let .command(id, title): title ?? id
        case let .submenu(title, inner): "\(title) ▸ \(titles(inner).joined(separator: ", "))"
        case .separator: "—"
        }
    }
}

@Test func `a chat's menu says what each item will do`() {
    var chat = ChatRow(id: "ana", v: 1, sort: "1", name: "Ana")
    #expect(titles(ChatMenu.list(chat)) == [
        "chat.open.pane", "—", "Pin", "Mute ▸ mute.8h, mute.week, mute.always", "Archive", "Mark as Unread",
    ])
    chat.pinned = true
    chat.mutedUntil = -1
    chat.archived = true
    chat.unread = 2
    #expect(titles(ChatMenu.list(chat)) == ["chat.open.pane", "—", "Unpin", "unmute", "Unarchive", "Mark as Read"])
    chat.unread = 0
    chat.markedUnread = true
    #expect(titles(ChatMenu.actions(chat)).last == "Mark as Read", "a chat marked unread is read by it")
}

@Test func `a community's row has none of a chat's actions, and its groups have them`() {
    let community = ChatRow(id: "c", v: 1, sort: "1", kind: .community, name: "Bairro Alto")
    #expect(ChatMenu.list(community).isEmpty)
    let group = ChatRow(id: "g", v: 1, sort: "1", kind: .group, name: "Bairro Alto: Announcements", community: "c")
    #expect(titles(ChatMenu.list(group)).contains("Pin"))
}

@Test func `the pane's menu has its splits and Swap, then its chat's actions`() {
    let chat = ChatRow(id: "ana", v: 1, sort: "1", name: "Ana")
    #expect(titles(ChatMenu.pane(chat)) == [
        "split.across", "split.down", "pane.close", "Swap ▸ Left, Right, Up, Down", "—",
        "Pin", "Mute ▸ mute.8h, mute.week, mute.always", "Archive", "Mark as Unread", "—", "search.chat",
    ])
    #expect(titles(ChatMenu.splits) == ["split.across", "split.down", "pane.close"], "a message's menu's end, an empty pane's menu")
}

@Test func `a mute's end is counted from when it's chosen, and always is -1`() {
    let now: Int64 = 1_792_159_320_000
    #expect(MuteLength.eightHours.until(now: now) == now + 28_800_000)
    #expect(MuteLength.week.until(now: now) == now + 604_800_000)
    #expect(MuteLength.always.until(now: now) == -1)
    #expect(MuteLength.allCases.map(\.title) == ["For 8 Hours", "For a Week", "Always"])
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `cannot find 'ChatMenu' in scope` and `cannot find 'MuteLength' in scope` in `ChatMenuTests.swift`.

- [ ] **Step 3: Write the code**

Create `mac/JawUI/Sources/ChatList/ChatMenu.swift`:

```swift
import JawKit

/// How long a chat is muted for, from its Mute menu.
public enum MuteLength: CaseIterable, Sendable {
    case eightHours, week, always

    public var id: String {
        switch self {
        case .eightHours: "mute.8h"
        case .week: "mute.week"
        case .always: "mute.always"
        }
    }

    public var title: String {
        switch self {
        case .eightHours: "For 8 Hours"
        case .week: "For a Week"
        case .always: "Always"
        }
    }

    /// Until when, in milliseconds since the epoch, counted from `now`, the moment it's chosen; -1 is
    /// for ever, as jawd's `chat.mute` takes it.
    public func until(now: Int64) -> Int64 {
        switch self {
        case .eightHours: now + 8 * 3_600_000
        case .week: now + 7 * 86_400_000
        case .always: -1
        }
    }
}

/// What a chat's right-click menus offer, from its row as they open: each item says what it will
/// do (Unpin on a pinned chat, Unmute on a muted one), and a community, whose groups carry these,
/// has none of them.
public enum ChatMenu {
    /// The command that opens a chat in a pane of its own, the app's.
    public static let openInNewPane = "chat.open.pane"

    /// The end of a message's menu, an empty pane's whole menu, and the start of a pane's.
    public static let splits: [MenuEntry] = [.command("split.across"), .command("split.down"), .command("pane.close")]

    /// The chat list's menu for `chat`.
    public static func list(_ chat: ChatRow) -> [MenuEntry] {
        let own = actions(chat)
        return own.isEmpty ? [] : [.command(openInNewPane), .separator] + own
    }

    /// The pane's menu, on its header and around its messages: its splits and Swap, greyed with no
    /// other pane to swap with, then its chat's actions and searching it.
    public static func pane(_ chat: ChatRow) -> [MenuEntry] {
        let swap: MenuEntry = .submenu("Swap", [
            .command("swap.left", title: "Left"), .command("swap.right", title: "Right"),
            .command("swap.up", title: "Up"), .command("swap.down", title: "Down"),
        ])
        return splits + [swap, .separator] + actions(chat) + [.separator, .command("search.chat")]
    }

    /// Pin, Mute, Archive and Mark as Read or Unread, as the chat stands.
    public static func actions(_ chat: ChatRow) -> [MenuEntry] {
        guard chat.kind != .community else { return [] }
        let unread = chat.unread > 0 || chat.markedUnread
        return [
            .command("pin", title: chat.pinned ? "Unpin" : "Pin"),
            chat.muted ? .command("unmute") : .submenu("Mute", MuteLength.allCases.map { .command($0.id) }),
            .command("archive", title: chat.archived ? "Unarchive" : "Archive"),
            .command("mark", title: unread ? "Mark as Read" : "Mark as Unread"),
        ]
    }
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the four new tests among those passed.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources/ChatList/ChatMenu.swift mac/Tests/JawUITests/ChatMenuTests.swift
git commit -m "What a chat's right-click menus offer, worked out from its row as they open: the chat list's has Open in New Pane, then Pin or Unpin, Mute with its three lengths or Unmute, Archive or Unarchive, and Mark as Read or Unread, each saying what it will do, and a community's row none of them; the pane's has Split Across, Split Down, Close Pane and Swap, then its chat's actions and Search This Chat; and a mute's end is counted from when it's chosen, always being -1, as jawd's chat.mute takes it"
```

---

### Task 3: The app's chat actions, written once and bound to a chat

Amended after the branch's review: every `enabled` in `chatCommands(for:)` reads `ChatRow.hasChatActions`, Unmute's too, so a muted community's Unmute is greyed with the rest of its actions, and Mark's `on` and `says` read `ChatRow.readsAsUnread`. The local `isChat` is `hasActions`, since it means a chat with the chat's actions, where `Commands.swift`'s `isChat` means any chat. Open in New Pane has no palette phrase, as it isn't in the palette. The glance's ⌘↩ calls `openInNewPane(_:)`, so the two agree: the chat goes into the focused pane when it's empty, opens when no pane has the keys, and counts as opened.

**Files:**
- Create: `mac/JawApp/Sources/ChatMenus.swift`
- Modify: `mac/JawApp/Sources/Commands.swift:71-105` (the Chat section's mute, unmute, archive, pin and mark commands)

**Interfaces:**
- Consumes: `ChatMenu`, `MuteLength`, `MenuEntry`, `Menus.items(_:command:)`, `Menus.menu(_:command:)` (tasks 1 and 2); `AppModel.split(_:with:)`, `open(_:opened:)`, `command(_:)`, `grid`, `panes`, `focusedSlot`, `jawd.chatAction(_:chat:_:what:)`, `jawd.opened(_:)`.
- Produces: `AppModel.chatCommands(for chat: @escaping @MainActor @Sendable () -> ChatRow?) -> [Command]` (ids `mute.8h`, `mute.week`, `mute.always`, `unmute`, `archive`, `pin`, `mark`); `AppModel.openInNewPane(_ chat: ChatRow)`; `AppModel.chatMenuItems(for chat: ChatRow) -> [NSMenuItem]`; `AppModel.paneMenu(for c: ConversationController) -> NSMenu`.

The app target has no test target; this task is checked by building the app here and by the snapshot run of task 6, and `ChatMenu`'s and `MuteLength`'s rules are tested in task 2.

- [ ] **Step 1: Write the chat's actions and the menus made from them**

Create `mac/JawApp/Sources/ChatMenus.swift`:

```swift
import AppKit
import JawKit
import JawUI

/// The chat's actions, written once, and the right-click menus made from them as they open: a
/// chat's in the list, a pane's on its header and around its messages, and the end of a message's.
/// The menu bar and the palette bind the actions to the focused pane's chat, a right-click menu to
/// the chat it was opened on.
extension AppModel {
    func chatCommands(for chat: @escaping @MainActor @Sendable () -> ChatRow?) -> [Command] {
        let isChat: @MainActor @Sendable () -> Bool = { chat().map { $0.kind != .community } ?? false }
        var out: [Command] = []
        for length in MuteLength.allCases {
            out.append(Command(length.id, length.title, phrase: "Mute this chat \(length.title.lowercased())",
                               detail: "no notifications from it", place: .chat, submenu: "Mute",
                               enabled: { chat().map { !$0.muted && $0.kind != .community } ?? false })
            { [weak self] in
                guard let c = chat() else { return }
                let until = Double(length.until(now: Format.now()))
                self?.jawd.chatAction("chat.mute", chat: c.id, ["until": .number(until)], what: "muting")
            })
        }
        out.append(Command("unmute", "Unmute", phrase: "Unmute this chat", detail: "its notifications back", place: .chat,
                           submenu: "Mute", enabled: { chat()?.muted ?? false })
        { [weak self] in
            guard let c = chat() else { return }
            self?.jawd.chatAction("chat.mute", chat: c.id, ["until": .null], what: "unmuting")
        })
        out.append(Command("archive", "Archived",
                           says: { (chat()?.archived ?? false ? "Unarchive this chat" : "Archive this chat", "as on the phone") },
                           place: .chat, group: 1, enabled: isChat, on: { chat()?.archived ?? false })
        { [weak self] in
            guard let c = chat() else { return }
            self?.jawd.chatAction("chat.archive", chat: c.id, ["archived": .bool(!c.archived)], what: "archiving")
        })
        out.append(Command("pin", "Pinned",
                           says: { (chat()?.pinned ?? false ? "Unpin this chat" : "Pin this chat", "at the top of the list") },
                           place: .chat, group: 1, enabled: isChat, on: { chat()?.pinned ?? false })
        { [weak self] in
            guard let c = chat() else { return }
            self?.jawd.chatAction("chat.pin", chat: c.id, ["pinned": .bool(!c.pinned)], what: "pinning")
        })
        let isUnread: @MainActor @Sendable () -> Bool = { chat().map { $0.unread > 0 || $0.markedUnread } ?? false }
        out.append(Command("mark", "Unread", says: { (isUnread() ? "Mark this chat read" : "Mark this chat unread", "as on the phone") },
                           place: .chat, group: 1, enabled: isChat, on: isUnread)
        { [weak self] in
            guard let c = chat() else { return }
            let unread = isUnread()
            self?.jawd.chatAction("chat.mark_unread", chat: c.id, ["unread": .bool(!unread)],
                                  what: unread ? "marking a chat read" : "marking a chat unread")
        })
        return out
    }

    /// A chat in a pane of its own: beside the focused pane, or in it when it's empty; a chat a pane
    /// already shows is gone to there, as ⌘↩ does in the glance.
    func openInNewPane(_ chat: ChatRow) {
        guard let grid, panes[chat.id] == nil, let at = focusedSlot, grid.slot(at)?.chat != nil else {
            return open(chat, opened: true)
        }
        split(.across, with: chat)
        if panes[chat.id] != nil { jawd.opened(chat.id) }
    }

    /// The chat list's menu for `chat`, its actions acting on it.
    func chatMenuItems(for chat: ChatRow) -> [NSMenuItem] {
        let open = Command(ChatMenu.openInNewPane, "Open in New Pane", phrase: "Open \(chat.name) in a new pane", place: nil,
                           palette: false) { [weak self] in self?.openInNewPane(chat) }
        let bound = chatCommands(for: { chat }) + [open]
        return Menus.items(ChatMenu.list(chat)) { id in bound.first { $0.id == id } }
    }

    /// A pane's menu, on its header, around its messages and on its system lines. The pane has the
    /// keys by the time it's asked for, so the splits and Swap act on it; its chat's actions act on
    /// its chat.
    func paneMenu(for c: ConversationController) -> NSMenu {
        let chat = c.chat
        let bound = chatCommands(for: { chat })
        return Menus.menu(ChatMenu.pane(chat)) { id in
            bound.first { $0.id == id } ?? command(id)
        }
    }
}
```

- [ ] **Step 2: Bind the menu bar's Chat commands to the focused chat through it**

In `mac/JawApp/Sources/Commands.swift`, replace the lines from `let now = Double(Format.now())` through the end of the `add(Command("mark", …))` call (the block that adds `mute.8h`, `mute.week`, `mute.always`, `unmute`, `archive`, `pin` and `mark`) with:

```swift
        for c in chatCommands(for: chat) {
            add(c)
        }
```

Keep `let isChat: @MainActor @Sendable () -> Bool = { chat() != nil }` above it: the attachments, the box's emoji and GIFs and focus mode still use it.

- [ ] **Step 3: Build the app**

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **` and no diagnostics.

- [ ] **Step 4: Commit**

```bash
git add mac/JawApp/Sources/ChatMenus.swift mac/JawApp/Sources/Commands.swift
git commit -m "The chat's actions are written once, in chatCommands(for:), and bound to a chat: the menu bar and the palette bind them to the focused pane's chat, as before, and a right-click menu to the chat it was opened on, with its items titled by ChatMenu; Open in New Pane splits the focused pane across for a chat, puts it in the focused pane when that's empty, or goes to the pane showing it; and Mute For 8 Hours and For a Week count from when they're chosen, where they counted from when the app made its commands, so a mute chosen a day after launch ended a day early"
```

---

### Task 4: The chat list's menu

**Files:**
- Modify: `mac/JawUI/Sources/ChatList/ChatListController.swift` (the `ChatListHost` protocol, `loadView`, and a new menu section)
- Modify: `mac/JawApp/Sources/AppModel.swift` (the `extension AppModel: ChatListHost` at the end)
- Test: `mac/Tests/JawUITests/ChatListMenuTests.swift`

**Interfaces:**
- Consumes: `AppModel.chatMenuItems(for:)` (task 3).
- Produces: `ChatListHost.chatList(_ list: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem]`; `ChatListController.menuItems(forRow:) -> [NSMenuItem]`; `ChatListController` conforms to `NSMenuDelegate`.

- [ ] **Step 1: Write the failing test**

Create `mac/Tests/JawUITests/ChatListMenuTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// A right click on a chat in the list asks the host for that chat's menu, made as it opens.

@MainActor
final class ListHost: ChatListHost {
    var asked: [String] = []

    func chatList(_: ChatListController, open _: ChatRow) {}

    func chatList(_: ChatListController, move _: Place) {}

    func chatList(_: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem] {
        asked.append(chat.id)
        return [NSMenuItem(title: chat.pinned ? "Unpin" : "Pin", action: nil, keyEquivalent: "")]
    }
}

@MainActor
@Test func `a right click on a chat asks for that chat's menu, and off the rows for none`() throws {
    let list = ChatListController(folder: "all", roles: Catalog.jaw.roles(dark: true))
    let host = ListHost()
    list.host = host
    _ = list.view
    list.shown = Samples.chats
    let menu = try #require(list.table.menu)
    #expect(menu.delegate === list, "filled for the row right-clicked as it opens")
    #expect(list.menuItems(forRow: 1).map(\.title) == [Samples.chats[1].pinned ? "Unpin" : "Pin"])
    #expect(host.asked == [Samples.chats[1].id])
    #expect(list.menuItems(forRow: -1).isEmpty && list.menuItems(forRow: Samples.chats.count).isEmpty)
    #expect(host.asked.count == 1, "nothing asked off the rows")
}
```

- [ ] **Step 2: Run the test to see it fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails in `ChatListMenuTests.swift` with `value of type 'ChatListController' has no member 'menuItems'`.

- [ ] **Step 3: Write the code**

In `mac/JawUI/Sources/ChatList/ChatListController.swift`, add to `public protocol ChatListHost` after `func chatList(_ list: ChatListController, move place: Place)`:

```swift
    /// The menu for a chat right-clicked in the list, made as it opens.
    func chatList(_ list: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem]
```

Change the class's declaration to add `NSMenuDelegate`:

```swift
public final class ChatListController: NSViewController, NSTableViewDataSource, NSTableViewDelegate, NSMenuDelegate {
```

In `loadView()`, after `table.setDraggingSourceOperationMask(.move, forLocal: true)`, add:

```swift
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        table.menu = menu
```

After the `// MARK: Choosing` section's `scrolled()` method, add:

```swift
    // MARK: Its menu

    /// Fills the table's menu for the row right-clicked, which AppKit outlines while it's open.
    public func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        for item in menuItems(forRow: table.clickedRow) {
            menu.addItem(item)
        }
    }

    /// The menu of the chat at row `i`, from the host; none off the rows.
    func menuItems(forRow i: Int) -> [NSMenuItem] {
        guard i >= 0, i < shown.count else { return [] }
        return host?.chatList(self, menuItemsFor: shown[i]) ?? []
    }
```

In `mac/JawApp/Sources/AppModel.swift`, in `extension AppModel: ChatListHost`, after `chatList(_:move:)`, add:

```swift
    func chatList(_: ChatListController, menuItemsFor chat: ChatRow) -> [NSMenuItem] {
        chatMenuItems(for: chat)
    }
```

- [ ] **Step 4: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the new test among those passed.

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources/ChatList/ChatListController.swift mac/JawApp/Sources/AppModel.swift mac/Tests/JawUITests/ChatListMenuTests.swift
git commit -m "The chat list has a right-click menu: its table's menu is filled as it opens for the row clicked, which AppKit outlines meanwhile without opening it, from the host's items for that chat, the app's chat actions bound to it with Open in New Pane; nothing is asked off the rows"
```

---

### Task 5: The chat's menus

Amended after its review: a message answers a right click only on itself, its body, its regions or the buttons it shows while hovered (`Laid.contains`, which left clicks use too). A right click on the blank space beside, between or under the messages goes on to the transcript's table and the pane's menu, as the design has it, where the plan's `MessageView` answered anywhere in its row. Building a menu doesn't move the focus: the right click's entry points, `menu(for view:)` and `paneMenuShown()`, give the pane the keys, and `menu(for m:)` and `menu(forRow:)` only build, so the snapshot's log changes nothing and the plan's `host.focused == 1` after `c.menu(for: text)` is 0. The slot frame gives a pane showing a chat its whole menu on the strips and controls no view claims, and an empty pane its splits, where the plan's frame offered only the splits. The tests send real right clicks through AppKit's hit test in an offscreen window. After the branch's review, VoiceOver's Show Menu, which has no pointer to hit-test, opens a message's menu under its bubble through `menu(for view:)`, so the pane takes the keys first (`MessageView.accessibilityPerformShowMenu()`, whose pop-up the test stands in for).

**Files:**
- Modify: `mac/JawUI/Sources/Conversation/ConversationController.swift` (`headerMenu`, `let table`, `loadView()`, `menu(for:)`, `HeaderView`, a new `TranscriptTable`)
- Modify: `mac/JawApp/Sources/AppModel.swift:348-349` (`makePane`'s `headerMenu`)
- Modify: `mac/JawApp/Sources/MainWindowController.swift:114-118` (`detail.panes.paneMenu`)
- Test: `mac/Tests/JawUITests/PaneMenuTests.swift`

**Interfaces:**
- Consumes: `ChatMenu.splits`, `Menus.items(_:command:)`, `Menus.menu(_:command:)` (tasks 1 and 2), `AppModel.paneMenu(for:)` (task 3), `ConversationHost.conversationFocused(_:)`.
- Produces: `ConversationController.paneMenu: (() -> NSMenu?)?` and `messageMenuTail: (() -> [NSMenuItem])?` (public, in place of `headerMenu`); `ConversationController.menu(for m: MessageRow?) -> NSMenu?`; `ConversationController.paneMenuShown() -> NSMenu?`; `public func menu(forRow i: Int) -> NSMenu?`; `final class TranscriptTable: NSTableView` with `menuFor`; `HeaderView.menuFor`.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/PaneMenuTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// A right click anywhere in a pane offers its splits: after a message's own actions, or as the
/// pane's whole menu on its header, around its messages and on its system lines. The pane takes
/// the keys first, so the menu acts on it.

@MainActor
final class PaneHost: ConversationHost {
    var focused = 0

    func conversation(_: ConversationController, move _: Place) {}
    func conversation(_: ConversationController, send _: String, key _: String, reply _: String?, mentions _: [String]) {}
    func conversation(_: ConversationController, react _: String, emoji _: String) {}
    func conversation(_: ConversationController, edit _: String, text _: String) {}
    func conversation(_: ConversationController, delete _: String, everyone _: Bool) {}
    func conversation(_: ConversationController, info _: String) {}
    func conversation(_: ConversationController, retry _: String) {}
    func conversation(_: ConversationController, read _: String) {}
    func conversation(_: ConversationController, typing _: TypingState) {}
    func conversation(_: ConversationController, draft _: String) {}
    func conversationWantsHistory(_: ConversationController) {}
    func conversation(_: ConversationController, forward _: MessageRow) {}
    func conversation(_: ConversationController, reactTo _: MessageRow) {}
    func conversationFocused(_: ConversationController) { focused += 1 }
    func conversation(_: ConversationController, open _: String) {}
    func conversationIsRead(_: ConversationController) -> Bool { false }
}

@MainActor
func pane(_ host: PaneHost) -> ConversationController {
    let c = ConversationController(chat: Samples.group, spec: spec(.bubbles, dark: true), layouts: Layouts(),
                                   place: ConversationController.place(for: Samples.group))
    c.host = host
    c.paneMenu = {
        let menu = NSMenu()
        menu.addItem(withTitle: "Pane", action: nil, keyEquivalent: "")
        return menu
    }
    c.messageMenuTail = { [NSMenuItem(title: "Split Across", action: nil, keyEquivalent: "d")] }
    return c
}

@MainActor
@Test func `a message's menu ends with the pane's splits, the pane taking the keys first`() throws {
    let host = PaneHost()
    let c = pane(host)
    let text = try #require(Samples.messages.first { $0.kind == .text && !$0.deleted })
    let menu = try #require(c.menu(for: text))
    #expect(menu.items.first?.title == "Reply")
    #expect(menu.items.last?.title == "Split Across")
    #expect(menu.items.dropLast().last?.isSeparatorItem == true, "after a separator")
    #expect(host.focused == 1)
}

@MainActor
@Test func `a system line, the header and the space around the messages open the pane's menu`() throws {
    let host = PaneHost()
    let c = pane(host)
    _ = c.view
    let system = try #require(Samples.messages.first { $0.kind == .system })
    #expect(c.menu(for: system)?.items.map(\.title) == ["Pane"])
    let click = try #require(NSEvent.mouseEvent(with: .rightMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
    #expect(c.header.menu(for: click)?.items.map(\.title) == ["Pane"])
    #expect(c.table.menu(for: click)?.items.map(\.title) == ["Pane"])
    #expect(host.focused == 3, "each takes the keys first")
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails in `PaneMenuTests.swift`: `value of type 'ConversationController' has no member 'paneMenu'` and `has no member 'messageMenuTail'`.

- [ ] **Step 3: Write the conversation's side**

In `mac/JawUI/Sources/Conversation/ConversationController.swift`, replace

```swift
    /// The pane's commands, on a right click of the header.
    public var headerMenu: NSMenu? {
        didSet { header.menu = headerMenu }
    }
```

with

```swift
    /// The pane's menu, made as it opens: on the header, around the messages and on system and call
    /// lines.
    public var paneMenu: (() -> NSMenu?)?
    /// What a message's menu ends with: the pane's splits.
    public var messageMenuTail: (() -> [NSMenuItem])?
```

Replace `let table = NSTableView()` with `let table = TranscriptTable()`.

In `loadView()`, after `hover.more = { [weak self] sender in self?.hoverMore(sender) }`, add:

```swift
        header.menuFor = { [weak self] in self?.paneMenuShown() }
        table.menuFor = { [weak self] in self?.paneMenuShown() }
```

Replace the start of `func menu(for view: MessageView) -> NSMenu? {` through its `let menu = NSMenu()` line:

```swift
    func menu(for view: MessageView) -> NSMenu? {
        guard let m = message(of: view), m.kind != .system, m.kind != .call else { return nil }
        let menu = NSMenu()
```

with

```swift
    func menu(for view: MessageView) -> NSMenu? {
        menu(for: message(of: view))
    }

    /// The menu a right click on the message at row `i` opens, for the snapshot's log.
    public func menu(forRow i: Int) -> NSMenu? {
        guard i >= 0, i < rows.count else { return nil }
        return menu(for: rows[i])
    }

    /// The pane's menu, the pane taking the keys first so the menu acts on it.
    func paneMenuShown() -> NSMenu? {
        host?.conversationFocused(self)
        return paneMenu?()
    }

    /// A message's menu, the pane taking the keys first: its actions, then the pane's splits. A system
    /// or call line, which has no actions of its own, has the pane's menu.
    func menu(for m: MessageRow?) -> NSMenu? {
        host?.conversationFocused(self)
        guard let m, m.kind != .system, m.kind != .call else { return paneMenu?() }
        let menu = NSMenu()
```

At the end of that same function, replace its last lines

```swift
        item("Info", "i") { [weak self] in
            guard let self else { return }
            host?.conversation(self, info: m.id)
        }
        return menu
    }
```

with

```swift
        item("Info", "i") { [weak self] in
            guard let self else { return }
            host?.conversation(self, info: m.id)
        }
        let tail = messageMenuTail?() ?? []
        if !tail.isEmpty {
            menu.addItem(.separator())
            for item in tail {
                menu.addItem(item)
            }
        }
        return menu
    }
```

In `final class HeaderView: NSView`, after `var roles: Roles? { didSet { colour() } }`, add:

```swift
    /// The pane's menu, asked for as a right click opens it.
    var menuFor: (() -> NSMenu?)?

    override func menu(for _: NSEvent) -> NSMenu? {
        menuFor?()
    }
```

At the end of the file, add:

```swift
/// The transcript's table: a right click between or under the messages, or on a line a message
/// view hands on, opens the pane's menu, as one on a message opens the message's.
final class TranscriptTable: NSTableView {
    var menuFor: (() -> NSMenu?)?

    override func menu(for _: NSEvent) -> NSMenu? {
        menuFor?()
    }
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the two new tests among those passed.

- [ ] **Step 5: Write the app's side**

In `mac/JawApp/Sources/AppModel.swift`, in `makePane(_:place:)`, replace

```swift
        c.headerMenu = Menus.menu(["split.across", "split.down", "pane.close", "swap.left", "swap.right", "swap.up", "swap.down"]
            .compactMap(command))
```

with

```swift
        c.paneMenu = { [weak self, weak c] in
            guard let self, let c else { return nil }
            return paneMenu(for: c)
        }
        c.messageMenuTail = { [weak self] in
            guard let self else { return [] }
            return Menus.items(ChatMenu.splits) { id in command(id) }
        }
```

In `mac/JawApp/Sources/MainWindowController.swift`, replace

```swift
            return Menus.menu(["split.across", "split.down", "pane.close"].compactMap(model.command))
```

with

```swift
            return Menus.menu(ChatMenu.splits) { id in model.command(id) }
```

- [ ] **Step 6: Build the app**

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 7: Commit**

```bash
git add mac/JawUI/Sources/Conversation/ConversationController.swift mac/JawApp/Sources/AppModel.swift mac/JawApp/Sources/MainWindowController.swift mac/Tests/JawUITests/PaneMenuTests.swift
git commit -m "A right click anywhere in a pane offers its splits: a message's menu ends, after a separator, with Split Across, Split Down and Close Pane; the header, the space between and under the messages, and system and call lines, which have no menu of their own, open the pane's menu, made as it opens, with the splits, Swap, greyed with no other pane, the chat's actions and Search This Chat; an empty pane's menu has the splits; and the pane right-clicked takes the keys first, so the menu acts on it and its chat"
```

---

### Task 6: The menus in the snapshot's log

Amended after its review: a row that isn't the chat list's, for `menu:list` or `drop:`, says `no row <i>` (`Bench.listRow(_:model:)`), where the plan's `i < list.count` guard let -1 through to trap on `list[i]`. A menu step prints one line, its menu, in place of the panes line it can't change. An unknown kind or extra parts say `unknown step`, hidden items aren't listed, and `said` is `words`, so `press`'s local `said` doesn't shadow it. The README says rows count from 0 and that `menu:message` reads the focused pane's transcript.

**Files:**
- Modify: `mac/JawApp/Sources/Bench.swift` (`press(_:model:then:)` and two new functions)
- Modify: `mac/README.md` (the paragraph on the app's options)

**Interfaces:**
- Consumes: `AppModel.chatMenuItems(for:)`, `AppModel.paneMenu(for:)` (task 3), `ConversationController.menu(forRow:)` (task 5), `model.window?.chatList.all`, `model.conversation`.
- Produces: the `--keys` steps `menu:list:<row>`, `menu:header` and `menu:message:<row>`, each printing a line `menu:…: Pin | Mute ▸ (For 8 Hours | For a Week | Always) | …` to standard error, greyed items in brackets.

- [ ] **Step 1: Write the steps**

In `mac/JawApp/Sources/Bench.swift`, in `press(_:model:then:)`, change

```swift
            let step = key.hasPrefix("save:") || key == "revert" || key == "settings" || key.hasPrefix("size:") || key.hasPrefix("drop:")
```

to

```swift
            let step = key.hasPrefix("save:") || key == "revert" || key == "settings" || key.hasPrefix("size:") || key.hasPrefix("drop:")
                || key.hasPrefix("menu:")
            if key.hasPrefix("menu:") {
                FileHandle.standardError.write(Data("\(key): \(menuSaid(String(key.dropFirst(5)), model: model))\n".utf8))
            }
```

After `press(_:model:then:)`, add:

```swift
    /// A right-click menu's items for the snapshot's log, as `menu:list:3` (the chat list's fourth
    /// row), `menu:header` (the focused pane's) or `menu:message:5` (its sixth message) makes it.
    static func menuSaid(_ what: String, model: AppModel) -> String {
        let parts = what.split(separator: ":").map(String.init)
        let row = parts.count > 1 ? Int(parts[1]) : nil
        switch (parts.first, row) {
        case let ("list", i?):
            guard let list = model.window?.chatList.all, i < list.count else { return "no row \(i)" }
            return said(model.chatMenuItems(for: list[i]))
        case ("header", _):
            guard let c = model.conversation else { return "no pane" }
            return said(model.paneMenu(for: c).items)
        case let ("message", i?):
            guard let menu = model.conversation?.menu(forRow: i) else { return "no message \(i)" }
            return said(menu.items)
        default:
            return "menu:list:<row>, menu:header or menu:message:<row>"
        }
    }

    /// Menu items as words: greyed ones in brackets, a submenu's items after it.
    static func said(_ items: [NSMenuItem]) -> String {
        items.map { item in
            if item.isSeparatorItem { return "—" }
            let title = item.isEnabled ? item.title : "[\(item.title)]"
            guard let sub = item.submenu else { return title }
            return "\(title) ▸ (\(said(sub.items)))"
        }.joined(separator: " | ")
    }
```

- [ ] **Step 2: Say them in the README**

In `mac/README.md`, in the paragraph on the app's options, replace

```
and the steps `size:1200x800`, `save:<layout>`, `revert` and `drop:<row>:<middle|left|right|top|bottom>`.
```

with

```
and the steps `size:1200x800`, `save:<layout>`, `revert`, `drop:<row>:<middle|left|right|top|bottom>`, and `menu:list:<row>`, `menu:header` and `menu:message:<row>`, which print the items of a chat's menu in the list, the focused pane's menu or a message's menu, greyed ones in brackets.
```

- [ ] **Step 3: Give the checkout a jawd for `--demo`**

The snapshot runs `jawd --engine demo` from `target/debug/jawd` in the checkout, which `build-mac` copies across. Copy the Mac builder's, built from the same main:

Run: `mkdir -p target/debug && cp ~/src/jaw-mac-m1/target/debug/jawd target/debug/jawd && ls -la target/debug/jawd`
Expected: the file, about 60 to 120 MB. If `~/src/jaw-mac-m1/target/debug/jawd` isn't there, build one: `CARGO_INCREMENTAL=0 cargo build -p jawd --bin jawd`, then delete `target` when this plan is done.

- [ ] **Step 4: Run the app on the demo account and read its menus**

Run: `mkdir -p /tmp/jaw-mac-menus && mac/scripts/build-mac snapshot /tmp/jaw-mac-menus/menus.png --keys "menu:list:0,menu:list:1,menu:header,menu:message:3,cmd+d,menu:header,menu:message:0"`
Expected, among the log's lines:
- `menu:list:0: Open in New Pane | — | Pin | Mute ▸ (For 8 Hours | For a Week | Always) | Archive | Mark as Unread` (titles as the demo's first chat stands: Unpin when it's pinned, Mark as Read when it has unread messages);
- `menu:header: Split Across | Split Down | [Close Pane] | [Swap] ▸ ([Left] | [Right] | [Up] | [Down]) | — | Pin | … | — | Search This Chat`, Close Pane and Swap in brackets with one pane;
- after `cmd+d`, `menu:header:` with neither `Close Pane` nor `Swap` bracketed, and among Swap's items the way to the other pane not bracketed;
- `menu:message:3:` starting `Reply | React ▸ (…)` and ending `— | Split Across | Split Down | …`; `menu:message:0`, when the demo's first row is a system line, giving the pane's menu.

- [ ] **Step 5: Commit**

```bash
git add mac/JawApp/Sources/Bench.swift mac/README.md
git commit -m "The snapshot's --keys can print a right-click menu, menu:list:<row> a chat's in the list, menu:header the focused pane's and menu:message:<row> a message's, its items as words with greyed ones in brackets and a submenu's after it, so the menus are checked on the demo account as the panes are"
```

---

### Task 7: Star in a message's menu (once jawd serves `star`)

Do this task only once jawd's step "Chat actions from jaw" is on main and `mac-m1`, and so `mac-menus`, has it: `protocol/fixtures/star.request.json` exists in the checkout. Until then a message's menu has no Star, as the design says.

**Files:**
- Modify: `mac/JawKit/Sources/Client/Jawd.swift` (a `star` request after `chatAction`)
- Modify: `mac/JawUI/Sources/Conversation/ConversationController.swift` (`ConversationHost`, `menu(for m:)`)
- Modify: `mac/JawApp/Sources/AppModel.swift` (the `ConversationHost` extension)
- Modify: `mac/Tests/JawUITests/PaneMenuTests.swift` (`PaneHost`, and a test)
- Modify: `mac/Tests/JawUITests/TypingTests.swift` (`Reader`, which conforms to `ConversationHost` on `mac-m1`, so it needs `conversation(_:star:)` too)
- Test: `mac/Tests/JawKitTests/FixtureTests.swift` (the request's fixture)

**Interfaces:**
- Consumes: `MessageRow.starred`; jawd's `star` (`message`, `starred`, `chat`).
- Produces: `Jawd.star(message: String, chat: String, starred: Bool)` and `Jawd.starParams(message:chat:starred:) -> JSONValue`; `ConversationHost.conversation(_ c: ConversationController, star message: MessageRow)`.

- [ ] **Step 1: Read the request's fixture**

Run: `cat protocol/fixtures/star.request.json`
Expected, as jawd's plan writes it: `{"id":20,"method":"star","params":{"message":"3EB0A1B2C3D4E5F6","starred":true,"chat":"351912345678@s.whatsapp.net"}}`.

- [ ] **Step 2: Write the failing tests**

In `mac/Tests/JawUITests/PaneMenuTests.swift`, add to `PaneHost`:

```swift
    var starred: [String] = []

    func conversation(_: ConversationController, star message: MessageRow) { starred.append(message.id) }
```

In `mac/Tests/JawUITests/TypingTests.swift`, add to `Reader`:

```swift
    func conversation(_: ConversationController, star _: MessageRow) {}
```

and in `PaneMenuTests.swift` add the test:

```swift
@MainActor
@Test func `a message's menu stars it, or takes its star off`() throws {
    let host = PaneHost()
    let c = pane(host)
    var text = try #require(Samples.messages.first { $0.kind == .text && !$0.deleted })
    let star = try #require(c.menu(for: text)?.items.first { $0.title == "Star" } as? ClosureItem)
    star.run()
    #expect(host.starred == [text.id])
    text.starred = true
    #expect(c.menu(for: text)?.items.contains { $0.title == "Unstar" } == true)
}
```

In `mac/Tests/JawKitTests/FixtureTests.swift`, in the test that compares each request JawKit writes with its fixture, after the line `try same("history.more.request.json", written(id: 12, "history.more", ChatRef(chat: "351912345678@s.whatsapp.net")))`, add:

```swift
    try same("star.request.json", written(id: 20, "star", Jawd.starParams(message: "3EB0A1B2C3D4E5F6",
                                                                          chat: "351912345678@s.whatsapp.net", starred: true)))
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails: `PaneHost` has `conversation(_:star:)` that the protocol doesn't require yet, and `ConversationController` makes no Star item.

- [ ] **Step 4: Write the code**

In `mac/JawKit/Sources/Client/Jawd.swift`, after `chatAction(_:chat:_:what:)`, add:

```swift
    /// Stars a message or takes its star off; the message's row comes back with it.
    public func star(message: String, chat: String, starred: Bool) {
        ask("star", Self.starParams(message: message, chat: chat, starred: starred), .did(starred ? "starring" : "taking a star off"))
    }

    /// `star`'s params, as jawd's fixture has them.
    public nonisolated static func starParams(message: String, chat: String, starred: Bool) -> JSONValue {
        .object(["message": .string(message), "starred": .bool(starred), "chat": .string(chat)])
    }
```

In `mac/JawUI/Sources/Conversation/ConversationController.swift`, add to `ConversationHost` after `func conversation(_ c: ConversationController, reactTo message: MessageRow)`:

```swift
    func conversation(_ c: ConversationController, star message: MessageRow)
```

In `menu(for m: MessageRow?)`, after the `if Self.notEditable(m) == nil { item("Edit", "e") { … } }` block, add:

```swift
        if !m.deleted {
            item(m.starred ? "Unstar" : "Star", "") { [weak self] in
                guard let self else { return }
                host?.conversation(self, star: m)
            }
        }
```

In `mac/JawApp/Sources/AppModel.swift`, in the `ConversationHost` extension, add:

```swift
    func conversation(_ c: ConversationController, star message: MessageRow) {
        jawd.star(message: message.id, chat: c.chat.id, starred: !message.starred)
    }
```

- [ ] **Step 5: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`.

Run: `mac/scripts/build-mac test JawKit`
Expected: `** TEST SUCCEEDED **`, the star request equal to its fixture.

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 6: Commit**

```bash
git add mac/JawKit/Sources/Client/Jawd.swift mac/JawUI/Sources/Conversation/ConversationController.swift mac/JawApp/Sources/AppModel.swift mac/Tests/JawUITests/PaneMenuTests.swift mac/Tests/JawUITests/TypingTests.swift mac/Tests/JawKitTests/FixtureTests.swift
git commit -m "A message's menu stars it, or takes its star off, now jawd serves star: Star or Unstar after Edit, sent with the message's chat, its request equal to jawd's fixture, the star shown when the message's row comes back"
```

---

## After the plan

- Delete `/tmp/jaw-mac-menus` (the snapshot), and `target` in `~/src/jaw-mac-menus` if task 6 built a jawd rather than copying one.
- The build Mac keeps `~/src/build-mac/jaw/jaw-mac-menus` with its `build/DerivedData`: delete it once `mac-m1` has `mac-menus` (`ssh build-mac rm -rf src/build-mac/jaw/jaw-mac-menus`).
- Tell the owner `mac-menus` is ready, rebased onto `mac-m1`'s tip, for the Mac builder to fast-forward `mac-m1` to, with what each commit does.
