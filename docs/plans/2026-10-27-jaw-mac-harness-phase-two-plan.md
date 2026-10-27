# Jaw Harness, phase two: screens from values

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every screen of jaw's Mac app drawn from values, in the harness as in the app, as the second phase of `docs/plans/2026-10-27-jaw-mac-harness-design.md` has it: what the screen says becomes pure functions of jawd's situation, the window's content and its parts move into `JawUI`, and the harness gains a Screens section drawing the whole window in each state.

**Architecture:** `Situation`, a value in `JawUI`, holds how jawd is reached, whether the account is linked, the connection, the link step and what's been said, and answers the status line's words, how the focused chat is, what fills the panes' place, and the chats that need you. `WindowContent`, an `NSViewController` in `JawUI`, holds the split view of the sidebar, the chat list and the panes' place (`DetailController`, moved), with the bottom bar (`BottomBar`, moved, given commands instead of `AppModel`), folds by its own width, and shows a `Situation` with the panes; `MainWindowController` keeps only the window around it. The glance's view, the status window's view and the layout prompts move too, so the app target defines no view of its own. In the harness, a `Screen` value describes a window, and `ScreenSpecimen` draws the `WindowContent` made from it, with an overlay where the window puts one.

**Tech Stack:** Swift 6 (AppKit and SwiftUI, macOS 26, Swift Testing), Tuist 4.197, xcodebuild on the build Mac through `mac/scripts/build-mac`, POSIX `sh`.

## Global Constraints

- The spec is `docs/plans/2026-10-27-jaw-mac-harness-design.md`, and this plan is its second phase, "Screens from values". Where this plan and the spec differ, the spec wins: stop and say so.
- Work only in the worktree `/Users/pedrogomes/src/jaw-harness`, on the branch `mac-harness`. Never push, never pass `--no-verify`, never rewrite commits that aren't yours. Never edit, build in, or run a git command that changes anything in `/Users/pedrogomes/src/jaw-mac-m1`: the Mac app's chat works there, on the files this phase moves.
- Build and test on the build Mac with `mac/scripts/build-mac` (`build` for the `jaw` app, `build JawHarness`, `test JawUITests`, `test JawHarnessTests`, `test JawKit`, `test JawThemes`, `snapshot`, `sweep`, `shot`). `mac/scripts/check` exits 1 when a build or the tests fail, so a run's exit status and its printed `** BUILD SUCCEEDED **` / `** TEST SUCCEEDED **` and `Test run with …` lines both count. For a full log after a run: `ssh -o BatchMode=yes build-mac 'tail -150 ~/src/build-mac/jaw/jaw-harness/mac/build/check.log'`.
- `build-mac snapshot` runs the app against `jawd demo` on the build Mac and needs `target/debug/jawd` in this worktree, which `build-mac` copies there. Get it by copying the Mac app's chat's binary, never by building Rust here: `mkdir -p target/debug && cp /Users/pedrogomes/src/jaw-mac-m1/target/debug/jawd target/debug/jawd` (40 MB, ignored by git). Never run the Go engine; `--demo` only. Never stop a process by name.
- The move changes nothing the app draws: where a task moves code out of the app target, the app's snapshots taken before the change and after it, with the same options, differ only where the clock draws (times, "x min ago").
- Swift 6 language mode, macOS 26, warnings are errors. `mac/scripts/check-components` refuses `systemFont(ofSize`, `.font(.system(`, `cornerRadius: <digit>` and `borderWidth = <digit>` outside `JawUI/Sources/Components` and the layout's files, and in the harness: take fonts from `TextStyle`, radii from `Radius`, lines from `Stroke`, spacing from `Space`.
- The harness asks jawd for nothing: it never names `Jawd` or JawKit's `Client` (`check-harness` refuses both). Code moved into `JawUI` asks jawd for nothing either: it takes values and reports through closures and hosts.
- Scratch stays in the worktree's ignored `mac/build/` (snapshots compared in `mac/build/compare/`) and the build Mac's copy of the checkout. A test that writes files uses its own `jaw-harness-tests-<name>-<UUID>` folder and deletes it.
- Commit messages are one long sentence in the project's voice (read `git log -5`), with no prefixes. Code matches its surroundings: doc comments in plain sentences, British spelling, no comments that narrate a change, 4-space indentation, trailing commas in multi-line argument lists.
- Phase two builds only what the spec's second phase lists. Live and Performance scenes, `fillsStage` (screens are drawn at their sizes in bands, as specimens are), `--check`, the pinned references and the account written longhand come with later phases.
- Between tasks, the controller fixes what the owner has marked Needs work on the review canvas (`mac/scripts/review-marks`); a task's implementer leaves the marks alone.

## File structure

New, in `mac/JawUI/Sources/Window/`:

- `Situation.swift`: jawd's situation as a value, and what the screen says from it.
- `DetailController.swift`: the panes' place, moved from the app.
- `BottomBar.swift`: the bar along the bottom of the window, moved from the app.
- `GlanceView.swift`: the glance's state and view, moved from the app.
- `StatusView.swift`: jawd's status, as the Status window shows it.
- `Prompts.swift`: the layout prompts' alerts.
- `WindowContent.swift`: the window's content.

New in the harness: `mac/JawHarness/Sources/Fixtures/Screens.swift` (`Screen`, a window as values), `mac/JawHarness/Sources/Scenes/ScreenScenes.swift` (the Screens section). New tests: `mac/Tests/JawUITests/SituationTests.swift`, `mac/Tests/JawUITests/WindowContentTests.swift`.

Changed: `mac/JawApp/Sources/AppModel.swift`, `MainWindowController.swift`, `Glance.swift`; deleted: `mac/JawApp/Sources/BottomBar.swift`. `mac/JawUI/Sources/Views/Linking.swift` (its clock), `mac/JawHarness/Sources/Specimen.swift`, `BuiltInScenes.swift`, `mac/scripts/check-harness`, `mac/README.md`.

---

### Task 1: The situation as a value

**Files:**
- Create: `mac/JawUI/Sources/Window/Situation.swift`, `mac/Tests/JawUITests/SituationTests.swift`
- Modify: `mac/JawApp/Sources/AppModel.swift`

**Interfaces:**
- Produces, in `JawUI`:
  - `public struct Situation: Equatable, Sendable` with `public enum Reach: Equatable, Sendable { case waiting, starting(failed: String?), up, unreachable(why: String, retry: Int) }`, `public struct Said: Equatable, Sendable { var text: String; var until: Date; var warn: Bool; init(_ text: String, until: Date, warn: Bool = false) }`, the fields `reach`, `linked`, `connection`, `link`, `said`, `init(reach: = .waiting, linked: = true, connection: = Connection(state: .connecting), link: = nil, said: = nil)`, and:
    - `public var stateLine: (text: String, warn: Bool)? { get }`
    - `public func chatLine(focused chat: ChatRow?, now: Date) -> String?`
    - `public func stateView(hasChats: Bool, hasPanes: Bool) -> StateView.Kind?`
    - `public static func needs(_ chats: [ChatRow], shown: Set<String>) -> [ChatRow]`
- In the app: `AppModel.Reach` becomes `typealias Reach = Situation.Reach`; `AppModel.situation: Situation { get }`; `stateLine`, `chatLine`, `needs` and `stateView` keep their names and types and ask the situation.

- [ ] **Step 1: Write the failing tests**

`mac/Tests/JawUITests/SituationTests.swift`:

```swift
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
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, the build failing with `cannot find 'Situation' in scope`.

- [ ] **Step 3: Write the situation**

`mac/JawUI/Sources/Window/Situation.swift`:

```swift
import Foundation
import JawKit

/// What the app knows of jawd and the account, as a value: how jawd is reached, whether the account
/// is linked, the connection, the link step and what's been said. The app fills one from jawd's
/// pushes and the harness makes one for each state, and what the screen says comes from it alone.
public struct Situation: Equatable, Sendable {
    /// How jawd is reached.
    public enum Reach: Equatable, Sendable {
        case waiting
        case starting(failed: String?)
        case up
        case unreachable(why: String, retry: Int)
    }

    /// Something said in the status line for a few seconds: a refusal, a hint.
    public struct Said: Equatable, Sendable {
        public var text: String
        public var until: Date
        public var warn: Bool

        public init(_ text: String, until: Date, warn: Bool = false) {
            self.text = text
            self.until = until
            self.warn = warn
        }
    }

    public var reach: Reach
    public var linked: Bool
    public var connection: Connection
    public var link: LinkStep?
    public var said: Said?

    public init(reach: Reach = .waiting, linked: Bool = true, connection: Connection = Connection(state: .connecting),
                link: LinkStep? = nil, said: Said? = nil)
    {
        self.reach = reach
        self.linked = linked
        self.connection = connection
        self.link = link
        self.said = said
    }

    /// The status line's left half: jawd's state when it isn't online, as the terminal's header says
    /// it, and whether it warns.
    public var stateLine: (text: String, warn: Bool)? {
        switch reach {
        case .waiting: return ("connecting to jawd…", false)
        case .starting(nil): return ("starting jawd…", false)
        case .starting: return ("jawd didn't start", true)
        case let .unreachable(_, retry): return ("can't reach jawd · trying again in \(retry) s", true)
        case .up: break
        }
        if !linked { return ("not linked", true) }
        switch connection.state {
        case .online: return nil
        case .connecting: return ("connecting", false)
        case .catchingUp:
            guard let done = connection.done, let total = connection.total else { return ("catching up", false) }
            return ("catching up \(Format.num(done)) of \(Format.num(total))", false)
        case .phoneOffline: return ("phone offline: photos and older history wait for it", true)
        case .offline: return ("offline: what you send waits until it's back", true)
        case .updateNeeded: return ("update needed: WhatsApp wants a newer Jaw · reading still works", true)
        case .banned: return ("temporarily banned until \(connection.until.map { Format.until($0) } ?? "later")", true)
        case .engineRestarting: return ("reconnecting: the engine restarted", true)
        case .loggedOut: return ("logged out", true)
        case .other: return ("connection: \(connection.reason ?? "a state this Jaw doesn't know")", true)
        }
    }

    /// How the focused chat is, without its name, which its box and header already say; or what was
    /// said, while it lasts.
    public func chatLine(focused chat: ChatRow?, now: Date) -> String? {
        if let said, said.until > now { return said.text }
        guard let chat else { return nil }
        var parts: [String] = []
        if !chat.typing.isEmpty { parts.append(chat.kind.isGroup ? "someone's typing…" : "typing…") }
        if chat.muted { parts.append("muted") }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// What fills the panes' place when it isn't chats: jawd starting or out of reach, linking, or
    /// nothing open.
    public func stateView(hasChats: Bool, hasPanes: Bool) -> StateView.Kind? {
        switch reach {
        case let .starting(failed): return .starting(failed: failed)
        case let .unreachable(why, retry) where !hasChats: return .asleep(why: why, retryIn: retry)
        default: break
        }
        if reach == .up, !linked {
            return .linking(step: link, loggedOut: connection.state == .loggedOut ? (connection.reason ?? "logged out from the phone") : nil)
        }
        if !hasPanes {
            return !hasChats && reach == .up ? .noChats : .noChat
        }
        return nil
    }

    /// The chats that need you that no pane shows: mentions first, then people, then groups, each by
    /// its unread, as the spike's status line reads them from the chat list.
    public static func needs(_ chats: [ChatRow], shown: Set<String>) -> [ChatRow] {
        let waiting = chats.filter {
            !shown.contains($0.id) && !$0.muted && !$0.archived && $0.kind != .community && ($0.unread > 0 || $0.markedUnread)
        }
        func rank(_ c: ChatRow) -> Int {
            c.mentions > 0 ? 0 : c.kind.isGroup ? 2 : 1
        }
        return waiting.sorted { (rank($0), -$0.unread) < (rank($1), -$1.unread) }
    }
}
```

- [ ] **Step 4: The app asks the situation**

In `mac/JawApp/Sources/AppModel.swift`:

1. Replace the `enum Reach: Equatable { … }` block with:

```swift
    typealias Reach = Situation.Reach
```

2. Replace the bodies of `stateLine`, `chatLine`, `needs` and `stateView` (under "What the screen says"), keeping their names, types and doc comments, and add `situation` above them:

```swift
    /// jawd's situation as this model has it, which says what the screen says.
    var situation: Situation {
        Situation(reach: reach, linked: linked, connection: connection, link: link,
                  said: notice.map { Situation.Said($0.text, until: $0.until, warn: $0.warn) })
    }

    var stateLine: (String, Bool)? {
        situation.stateLine.map { ($0.text, $0.warn) }
    }

    var chatLine: String? {
        situation.chatLine(focused: conversation?.chat, now: Date())
    }

    var needs: [ChatRow] {
        Situation.needs(window?.chatList.all ?? [], shown: Set(panes.keys))
    }

    var stateView: StateView.Kind? {
        situation.stateView(hasChats: !(window?.chatList.all.isEmpty ?? true), hasPanes: grid != nil)
    }
```

- [ ] **Step 5: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac build`, then `mac/scripts/build-mac test JawHarnessTests`.
Expected: each exits 0; JawUITests passes with four more tests than before; the app builds.

- [ ] **Step 6: Commit**

```bash
git add mac/JawUI/Sources/Window/Situation.swift mac/Tests/JawUITests/SituationTests.swift mac/JawApp/Sources/AppModel.swift
git commit -F- <<'EOF'
What the screen says comes from jawd's situation as a value, Situation in JawUI: how jawd is reached, whether the account is linked, the connection, the link step and what's been said answer the status line's words and whether they warn, how the focused chat is, what fills the panes' place and the chats that need you, each tested over every state in the app's own words; the app's model makes one from what jawd has pushed and asks it, as the harness will
EOF
```

---

### Task 2: The window's parts into JawUI

**Files:**
- Create: `mac/JawUI/Sources/Window/DetailController.swift`, `BottomBar.swift`, `GlanceView.swift`, `StatusView.swift`, `Prompts.swift` (all in `mac/JawUI/Sources/Window/`)
- Delete: `mac/JawApp/Sources/BottomBar.swift`
- Modify: `mac/JawApp/Sources/MainWindowController.swift`, `mac/JawApp/Sources/Glance.swift`, `mac/scripts/check-harness`

**Interfaces:**
- Consumes: `Situation` (Task 1).
- Produces, in `JawUI`, each `public` and `@MainActor`:
  - `final class DetailController: NSViewController` with `let panes: GridView`, `let box: BoxPlace`, `func show(panes:views:focused:box:roles:look:force:)`, `func show(state:roles:)`, `func showLinking(step:loggedOut:roles:start:cancel:)`, as today.
  - `final class BottomBar: NSView` with `init(commands: @escaping @MainActor () -> [Command])`, `let status: NSHostingView<StatusLine>`, `static let height: CGFloat`, `func restyle(_: Roles)`, `func show(layout: String, edited: Bool)`.
  - `final class GlanceState` (`@Observable`, `init()`, `var chats: [ChatRow]`, `var rows: [String: Messages<MessageRow>]`, `var index: Int`) and `struct GlanceView: View` with `init(state:roles:go:)` and `static func height(_ chats: Int) -> CGFloat`.
  - `struct StatusView: View` with `init(status: Status)`.
  - `enum Prompts` with `static func name(_ title: String, _ detail: String, value: String = "", button: String) -> (NSAlert, NSTextField)` and `static func deleteLayout(_ name: String) -> NSAlert`.
- `check-harness`'s second rule: the app target defines no view or view controller of its own.

- [ ] **Step 1: Take the app's pictures before the move**

Copy jawd once (see the Global Constraints), then:

```sh
mkdir -p mac/build/compare
mac/scripts/build-mac snapshot mac/build/compare/before-window.png
mac/scripts/build-mac snapshot mac/build/compare/before-split.png --keys cmd+d
mac/scripts/build-mac snapshot mac/build/compare/before-glance.png --keys cmd+j
mac/scripts/build-mac snapshot mac/build/compare/before-settings.png --keys settings
```

Expected: `==> mac/build/compare/before-….png` each time. Look at each (the Read tool shows them): the window with a chat open; two panes; the glance; the Settings sheet.

- [ ] **Step 2: Move `DetailController`**

Move the `DetailController` class from the end of `mac/JawApp/Sources/MainWindowController.swift` into `mac/JawUI/Sources/Window/DetailController.swift`, unchanged but for `public` on the class, `panes`, `box`, and the three `show` methods, and its doc comment, which reads: `/// The panes' place: the panes with the focused chat's box under them, or what's there instead, a state or linking.` The file imports `AppKit`, `JawKit`, `JawThemes` and `SwiftUI`.

- [ ] **Step 3: Move `BottomBar`, given commands rather than the model**

Move `mac/JawApp/Sources/BottomBar.swift` to `mac/JawUI/Sources/Window/BottomBar.swift` (`git mv`), and change it so it asks nothing of `AppModel`:

```swift
import AppKit
import JawThemes
import SwiftUI

/// The bar along the bottom of the window, where the controls live so the panes have the window's
/// whole height: the layout and the splits on the left, the status line between, and Find and New
/// Chat on the right. Each runs the same command as its menu item and key.
@MainActor
public final class BottomBar: NSView {
    public let status: NSHostingView<StatusLine>
    let layout: IconButton
    let split: IconButton
    let find: IconButton
    let newChat: IconButton
    let rule = NSView()
    public static let height: CGFloat = 30

    /// `commands` are the app's, read each time a button runs one or opens its menu, so the bar runs
    /// what the menus and the keys run.
    public init(commands: @escaping @MainActor () -> [Command]) {
        status = NSHostingView(rootView: StatusLine(state: nil, warn: false, chat: nil, roles: Catalog.jaw.dark))
        let command = { (id: String) in commands().first { $0.id == id } }
        layout = IconButton(symbol: "square.grid.3x2", label: "Layouts on a grid", shortcut: Shortcut("e", [.command, .option]),
                            title: "Unsaved Panes") {}
        split = IconButton(symbol: "rectangle.split.2x1", label: "Split this pane", shortcut: Shortcut("d")) {}
        find = IconButton(symbol: "magnifyingglass", label: "Find: chats, people, messages and every action", shortcut: Shortcut("k"),
                          title: "Find") { command("palette")?.run() }
        newChat = IconButton(symbol: "square.and.pencil", label: "Start a chat", shortcut: Shortcut("n")) { command("chat.new")?.run() }
        super.init(frame: .zero)
        wantsLayer = true
        layout.menuToOpen = { Menus.menu(commands().filter { $0.place == .layouts }) }
        split.menuToOpen = { Menus.menu(["split.across", "split.down", "pane.close"].compactMap(command)) }
        // … the rest of `init` as it was, from `let left = NSStackView(views: [layout, split])` on.
    }

    // … `init?(coder:)`, `restyle(_:)` and `show(layout:edited:)` as they were, the last two `public`.
}
```

(The comment lines above stand for code that stays exactly as it is in today's file; write it out in full.)

In `MainWindowController.init`, make the bar with `bar = BottomBar(commands: { [weak model] in model?.allCommands ?? [] })`.

- [ ] **Step 4: Move the glance's state and view**

Move `GlanceState` and `GlanceView` from `mac/JawApp/Sources/Glance.swift` into `mac/JawUI/Sources/Window/GlanceView.swift`, unchanged but for `public` on both types, `GlanceState`'s three properties and a `public init() {}`, `GlanceView`'s `init(state:roles:go:)` (written out: `public init(state: GlanceState, roles: Roles, go: @escaping (ChatRow, Bool) -> Void)`), its `body` and `height(_:)`, with doc comments: `/// What the glance shows: the chats that need you, their latest lines, and which one is picked.` and `/// The glance: each chat that needs you as a card of its latest lines, the picked one selected.` `Glance` stays in the app and uses them.

- [ ] **Step 5: The status window's view and the layout prompts**

`mac/JawUI/Sources/Window/StatusView.swift`:

```swift
import JawKit
import SwiftUI

/// jawd's status as it answered `status`, in full: what the Status window shows.
public struct StatusView: View {
    let status: Status

    public init(status: Status) {
        self.status = status
    }

    public var body: some View {
        ScrollView { Text(Self.text(status)).textStyle(.mono).textSelection(.enabled).padding() }
            .frame(minWidth: 520, minHeight: 420)
    }

    /// The answer as jawd sent it, pretty-printed with its keys in order.
    static func text(_ status: Status) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return (try? String(data: encoder.encode(status.raw), encoding: .utf8)) ?? ""
    }
}
```

`mac/JawUI/Sources/Window/Prompts.swift`:

```swift
import AppKit

/// The window's questions about its layouts, as alerts it shows as sheets: a name to save one as or
/// rename it to, and whether to delete one.
@MainActor
public enum Prompts {
    /// An alert asking for a name, and its field, which takes the keys at once: Return answers with
    /// `button`, Esc cancels.
    public static func name(_ title: String, _ detail: String, value: String = "", button: String) -> (NSAlert, NSTextField) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        let field = NSTextField(string: value)
        field.placeholderString = "Work"
        field.frame = NSRect(x: 0, y: 0, width: 260, height: 24)
        alert.accessoryView = field
        alert.addButton(withTitle: button)
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        return (alert, field)
    }

    /// An alert asking whether to delete the layout `name`, its Delete marked destructive.
    public static func deleteLayout(_ name: String) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = "Delete \(name)?"
        alert.informativeText = "Its panes stay on screen, unsaved."
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        return alert
    }
}
```

In `MainWindowController.swift`:

- `showStatus(_:)` shows `StatusView(status: status)`:

```swift
    func showStatus(_ status: Status) {
        let view = StatusView(status: status)
        let w = statusWindow ?? NSWindow(contentViewController: NSHostingController(rootView: view))
        (w.contentViewController as? NSHostingController<StatusView>)?.rootView = view
        w.title = "jawd's status"
        statusWindow = w
        w.makeKeyAndOrderFront(nil)
    }
```

- `askName` makes its alert with `Prompts.name`, and `askLayoutDelete` with `Prompts.deleteLayout`:

```swift
    func askName(_ title: String, _ detail: String, value: String = "", button: String, done: @escaping (String) -> Void) {
        guard let window else { return }
        let (alert, field) = Prompts.name(title, detail, value: value, button: button)
        alert.beginSheetModal(for: window) { response in
            if response == .alertFirstButtonReturn { done(field.stringValue) }
        }
    }
```

```swift
    func askLayoutDelete() {
        guard let window, let name = model.book.current else { return }
        Prompts.deleteLayout(name).beginSheetModal(for: window) { [weak model] response in
            if response == .alertFirstButtonReturn { model?.deleteLayout(name) }
        }
    }
```

- The `extension JSONEncoder { static var pretty … }` goes: `StatusView` was its only user.

- [ ] **Step 6: `check-harness`'s second rule**

In `mac/scripts/check-harness`, after the `Client` rule, add a rule that fails when a file in `JawApp/Sources` declares a class or struct whose inherited types include a view or a view controller (`NSView` or any `NS…View`, `NSViewController` or any `NS…ViewController`, `NSControl`, `NSButton`, `NSTextField`, `NSHostingController`, `NSHostingView`, `NSViewRepresentable`, `NSViewControllerRepresentable`, or SwiftUI's `View`), printing the declarations and `check-harness: the app target draws nothing of its own: a view or a view controller belongs in JawUI, where the harness can draw it from values`. A window controller (`MainWindowController: NSWindowController, NSWindowDelegate`), `Glance: NSObject` and the app's other types must pass. Check it with probe files in `JawApp/Sources` (each removed after): `final class Probe: NSView {}`, `final class Probe: NSStackView {}`, `final class Probe: NSViewController {}`, `struct Probe: View { var body: some View { EmptyView() } }`, `final class Probe: NSObject, NSWindowDelegate {}` (passes), and say what each did. Update the script's header comment, which lists its rules.

- [ ] **Step 7: Build, test, and take the pictures after**

Run: `mac/scripts/build-mac build`, `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests`, then the four `snapshot` commands of Step 1 with `after-` in place of `before-`.
Expected: each exits 0. Compare each pair, by looking and by counting the pixels that differ with a throwaway script in `mac/build/compare/` (for example a short Swift script that reads both PNGs with `NSBitmapImageRep` and prints the count and the bounding boxes of pixels more than 24 levels apart): they differ only where the clock draws. Say what differed, and where, in the report.

- [ ] **Step 8: Commit**

```bash
git add -A mac/JawUI/Sources/Window mac/JawApp/Sources mac/scripts/check-harness
git commit -F- <<'EOF'
The window's parts move into JawUI, so the harness can draw each from values: the panes' place, DetailController, as it was; the bottom bar, BottomBar, given the app's commands rather than its model; the glance's state and view; jawd's status as StatusView; and the layout prompts' alerts as Prompts; the app's pictures before the move and after it differ only where the clock draws, and check-harness fails when the app target defines a view or a view controller of its own
EOF
```

---

### Task 3: The window's content

**Files:**
- Create: `mac/JawUI/Sources/Window/WindowContent.swift`, `mac/Tests/JawUITests/WindowContentTests.swift`
- Modify: `mac/JawApp/Sources/MainWindowController.swift`

**Interfaces:**
- Consumes: `Situation` (Task 1); `DetailController`, `BottomBar` (Task 2).
- Produces, in `JawUI`:
  - `@MainActor public final class WindowContent: NSViewController` with `init(sidebar: SidebarController, chatList: ChatListController, detail: DetailController, bar: BottomBar, autosaveName: String? = nil)` (the app passes `"jaw.split"`; a screen the harness draws keeps nothing), `let sidebar`, `let chatList`, `let detail`, `let bar`, `static let sidebarFold: CGFloat = 960`, `static let listFold: CGFloat = 740`, `var inFocusMode: Bool { get }`, `func fold(chatOpen: Bool, animated: Bool = true)`, `func focusMode(_ on: Bool, animated: Bool = true)` (a still screen passes `animated: false`, so it's drawn folded), `func toggleSidebar()`, `func toggleChatList()`, `func show(_ situation: Situation, chats: [ChatRow], focused: ChatRow?, panes: Panes?, roles: Roles, now: Date, actions: Actions)` and `func show(_ panes: Panes, roles: Roles)`.
  - `WindowContent.Panes` (`grid: PaneGrid`, `views: [Int: NSView]`, `focused: Int?`, `box: NSView?`, `look: PaneLook`, with a memberwise `public init`) and `WindowContent.Actions` (`open: (ChatRow) -> Void`, `glance: () -> Void`, `linkStart: (String?) -> Void`, `linkCancel: () -> Void`, each defaulting to doing nothing in `public init`).
- In the app: `MainWindowController` keeps `chatList`, `detail`, `bar`, `sidebar` as computed properties onto its `content`, so `AppModel`, the commands and the bench go on reaching them as they do.

- [ ] **Step 1: Take the app's pictures before**

As Task 2's Step 1, into `mac/build/compare/` with `before3-` names, and add the folds and focus mode:

```sh
mac/scripts/build-mac snapshot mac/build/compare/before3-window.png
mac/scripts/build-mac snapshot mac/build/compare/before3-940.png --keys size:940x700
mac/scripts/build-mac snapshot mac/build/compare/before3-720.png --keys size:720x600
mac/scripts/build-mac snapshot mac/build/compare/before3-focus.png --keys cmd+shift+return
mac/scripts/build-mac snapshot mac/build/compare/before3-split.png --keys cmd+d
```

Expected: the window; the sidebar folded at 940; the chat list folded too at 720; focus mode with both folded; two panes.

- [ ] **Step 2: Write the failing test**

`mac/Tests/JawUITests/WindowContentTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The window's content folds by its own width, as the window's resize asks: the sidebar below 960
/// points, the chat list too below 740 while a chat is open; focus mode folds both and puts them back.

@MainActor
func content(width: CGFloat) -> WindowContent {
    let c = WindowContent(sidebar: SidebarController(), chatList: ChatListController(folder: "all", roles: Catalog.jaw.dark),
                          detail: DetailController(), bar: BottomBar(commands: { [] }))
    c.view.frame = NSRect(x: 0, y: 0, width: width, height: 700)
    c.view.layoutSubtreeIfNeeded()
    return c
}

@MainActor @Test func `the window's content folds by its own width, and focus mode folds both and puts them back`() {
    let before = Motion.reduced
    defer { Motion.reduced = before }
    Motion.reduced = { true }
    let wide = content(width: 1180)
    wide.fold(chatOpen: true)
    #expect(!wide.sidebarItem.isCollapsed && !wide.listItem.isCollapsed)
    let mid = content(width: 940)
    mid.fold(chatOpen: true)
    #expect(mid.sidebarItem.isCollapsed && !mid.listItem.isCollapsed)
    let narrow = content(width: 720)
    narrow.fold(chatOpen: false)
    #expect(narrow.sidebarItem.isCollapsed && !narrow.listItem.isCollapsed, "the list stays while no chat is open")
    narrow.fold(chatOpen: true)
    #expect(narrow.sidebarItem.isCollapsed && narrow.listItem.isCollapsed)
    wide.focusMode(true)
    #expect(wide.inFocusMode && wide.sidebarItem.isCollapsed && wide.listItem.isCollapsed)
    wide.fold(chatOpen: true)
    #expect(wide.sidebarItem.isCollapsed, "focus mode keeps them folded whatever the width")
    wide.focusMode(false)
    #expect(!wide.inFocusMode && !wide.sidebarItem.isCollapsed && !wide.listItem.isCollapsed, "and puts them back as they were")
}
```

Run: `mac/scripts/build-mac test JawUITests`. Expected: exit 1, `cannot find 'WindowContent' in scope`.

- [ ] **Step 3: Write the window's content**

`mac/JawUI/Sources/Window/WindowContent.swift`, taking the split view and the bar from `MainWindowController.init`, its folding from `windowDidResize`, focus mode and the toggles from the window controller, and the body of its `refresh()`:

```swift
import AppKit
import JawKit
import JawThemes
import SwiftUI

/// The window's content: the sidebar, the chat list and the panes' place on macOS 26's split view,
/// with the bottom bar along the bottom of the whole window. It folds by its own width, as the
/// terminal folds: below 960 points the sidebar, and below 740, while a chat is open, the chat list
/// too; focus mode folds both for the focused pane and puts them back as they were.
@MainActor
public final class WindowContent: NSViewController {
    public let sidebar: SidebarController
    public let chatList: ChatListController
    public let detail: DetailController
    public let bar: BottomBar
    let split = NSSplitViewController()
    let sidebarItem: NSSplitViewItem
    let listItem: NSSplitViewItem
    /// The sidebar's and the chat list's folding before focus mode, to put back; nil out of it.
    var beforeFocus: (Bool, Bool)?

    /// Below this width the sidebar folds away.
    public static let sidebarFold: CGFloat = 960
    /// Below this width, while a chat is open, the chat list folds too.
    public static let listFold: CGFloat = 740

    /// The panes as the window shows them: the layout's grid, each slot's view (none yet for a chat the
    /// list doesn't hold), the slot with the keys, the focused chat's box under them, and how the panes
    /// say which has the keys.
    public struct Panes {
        public var grid: PaneGrid
        public var views: [Int: NSView]
        public var focused: Int?
        public var box: NSView?
        public var look: PaneLook

        public init(grid: PaneGrid, views: [Int: NSView], focused: Int?, box: NSView?, look: PaneLook) {
            self.grid = grid
            self.views = views
            self.focused = focused
            self.box = box
            self.look = look
        }
    }

    /// What the content's parts do when they're used: open a chat, open the glance, start or cancel linking.
    public struct Actions {
        public var open: (ChatRow) -> Void
        public var glance: () -> Void
        public var linkStart: (String?) -> Void
        public var linkCancel: () -> Void

        public init(open: @escaping (ChatRow) -> Void = { _ in }, glance: @escaping () -> Void = {},
                    linkStart: @escaping (String?) -> Void = { _ in }, linkCancel: @escaping () -> Void = {})
        {
            self.open = open
            self.glance = glance
            self.linkStart = linkStart
            self.linkCancel = linkCancel
        }
    }

    public init(sidebar: SidebarController, chatList: ChatListController, detail: DetailController, bar: BottomBar) {
        self.sidebar = sidebar
        self.chatList = chatList
        self.detail = detail
        self.bar = bar
        sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebar)
        sidebarItem.minimumThickness = 180
        sidebarItem.maximumThickness = 280
        sidebarItem.canCollapseFromWindowResize = true
        listItem = NSSplitViewItem(contentListWithViewController: chatList)
        listItem.minimumThickness = 260
        listItem.maximumThickness = 420
        super.init(nibName: nil, bundle: nil)
        let detailItem = NSSplitViewItem(viewController: detail)
        detailItem.minimumThickness = 360
        split.addSplitViewItem(sidebarItem)
        split.addSplitViewItem(listItem)
        split.addSplitViewItem(detailItem)
        split.splitView.autosaveName = "jaw.split"
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        let content = NSView()
        view = content
        addChild(split)
        for v in [split.view, bar] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview(v)
        }
        NSLayoutConstraint.activate([
            split.view.topAnchor.constraint(equalTo: content.topAnchor),
            split.view.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            split.view.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            split.view.bottomAnchor.constraint(equalTo: bar.topAnchor),
            bar.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            bar.bottomAnchor.constraint(equalTo: content.bottomAnchor),
        ])
    }

    /// Whether focus mode has the sidebar and the chat list folded.
    public var inFocusMode: Bool {
        beforeFocus != nil
    }

    /// Folds the sidebar and the chat list for the content's own width, as the window asks when it's
    /// resized, unless focus mode has them folded; at once under Reduce Motion.
    public func fold(chatOpen: Bool) {
        guard !inFocusMode else { return }
        let width = view.bounds.width
        collapse(sidebarItem, width < Self.sidebarFold)
        collapse(listItem, width < Self.listFold && chatOpen)
    }

    /// Focus mode: the sidebar and the chat list fold away for the focused pane, and come back as they were.
    public func focusMode(_ on: Bool) {
        if on {
            guard !inFocusMode else { return }
            beforeFocus = (sidebarItem.isCollapsed, listItem.isCollapsed)
            collapse(sidebarItem, true)
            collapse(listItem, true)
        } else {
            collapse(sidebarItem, beforeFocus?.0 ?? false)
            collapse(listItem, beforeFocus?.1 ?? false)
            beforeFocus = nil
        }
    }

    public func toggleSidebar() {
        collapse(sidebarItem, !sidebarItem.isCollapsed)
    }

    public func toggleChatList() {
        collapse(listItem, !listItem.isCollapsed)
    }

    func collapse(_ item: NSSplitViewItem, _ collapsed: Bool) {
        (Motion.reduced() ? item : item.animator()).isCollapsed = collapsed
    }

    /// Shows what jawd's situation says: the status line, from the situation, the chats and the focused
    /// chat, the chats that need you leaving out those a pane shows; and in the panes' place the state
    /// it calls for, linking, or `panes`.
    public func show(_ situation: Situation, chats: [ChatRow], focused: ChatRow?, panes: Panes?, roles: Roles, now: Date,
                     actions: Actions)
    {
        let state = situation.stateLine
        let shown = Set(panes.map { p in p.grid.slots.filter { p.views[$0.id] != nil }.compactMap(\.chat) } ?? [])
        bar.status.rootView = StatusLine(state: state?.text, warn: state?.warn ?? false,
                                         chat: situation.chatLine(focused: focused, now: now),
                                         needs: Situation.needs(chats, shown: shown), roles: roles, open: actions.open,
                                         glance: actions.glance)
        switch situation.stateView(hasChats: !chats.isEmpty, hasPanes: panes != nil) {
        case let .linking(step, loggedOut)?:
            detail.showLinking(step: step, loggedOut: loggedOut, roles: roles, start: actions.linkStart, cancel: actions.linkCancel)
        case let kind?:
            detail.show(state: kind, roles: roles)
        case nil:
            if let panes {
                detail.show(panes: panes.grid, views: panes.views, focused: panes.focused, box: panes.box, roles: roles, look: panes.look)
            }
        }
    }
}
```

The test reaches `sidebarItem` and `listItem` through `@testable import JawUI`.

- [ ] **Step 4: The window controller keeps the window around its content**

In `mac/JawApp/Sources/MainWindowController.swift`:

1. Replace the stored `split`, `sidebar`, `chatList`, `detail`, `bar`, `sidebarItem`, `listItem` and `beforeFocus` with `let content: WindowContent` and computed properties onto it, so every caller keeps its names:

```swift
    let content: WindowContent
    var sidebar: SidebarController {
        content.sidebar
    }

    var chatList: ChatListController {
        content.chatList
    }

    var detail: DetailController {
        content.detail
    }

    var bar: BottomBar {
        content.bar
    }
```

2. In `init`, make the content before `super.init` and make it the window's content view controller, in place of the split view, the root view controller and their constraints:

```swift
        content = WindowContent(sidebar: SidebarController(), chatList: ChatListController(folder: model.folder, roles: model.roles),
                                detail: DetailController(), bar: BottomBar(commands: { [weak model] in model?.allCommands ?? [] }))
```

and, once the window is made and `super.init` has run, `window.contentViewController = content`. The title bar's sidebar button, the hosts' closures, the display link (on `content.view`), `restyle()` and `refresh()` stay as they are.

3. `refresh()` and `showPanes()` build the panes once and hand the rest to the content:

```swift
    func refresh() {
        content.show(model.situation, chats: chatList.all, focused: model.conversation?.chat, panes: panesShown, roles: model.roles,
                     now: Date(), actions: WindowContent.Actions(
                         open: { [weak model] c in model?.open(c, opened: true) },
                         glance: { [weak self] in self?.showGlance() },
                         linkStart: { [weak model] phone in model?.jawd.linkStart(phone: phone) },
                         linkCancel: { [weak model] in model?.jawd.linkCancel() },
                     ))
    }

    /// The panes as the model's layout has them, the focused one outlined, or nil with no layout.
    var panesShown: WindowContent.Panes? {
        guard let grid = model.grid else { return nil }
        var views: [Int: NSView] = [:]
        for slot in grid.slots {
            if let chat = slot.chat, let c = model.panes[chat] { views[slot.id] = c.view }
        }
        return WindowContent.Panes(grid: grid, views: views, focused: model.focusedSlot, box: model.conversation?.composer.view,
                                   look: model.preferences.paneLook(model.roles))
    }

    /// Lays the panes out as the model's layout has them, the focused one outlined.
    func showPanes() {
        guard let panes = panesShown, model.stateView == nil else { return }
        detail.show(panes: panes.grid, views: panes.views, focused: panes.focused, box: panes.box, roles: model.roles, look: panes.look)
    }
```

4. Folding, focus mode and the toggles go to the content:

```swift
    func toggleChatList() {
        content.toggleChatList()
    }

    func toggleSidebar() {
        content.toggleSidebar()
    }

    /// Focus mode: the sidebar and the chat list fold away for the focused pane, and come back.
    func focusMode(_ on: Bool) {
        content.focusMode(on)
    }

    func windowDidResize(_: Notification) {
        content.fold(chatOpen: model.conversation != nil)
    }
```

- [ ] **Step 5: Build, test, and take the pictures after**

Run: `mac/scripts/build-mac build`, `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests`, `mac/scripts/build-mac test JawKit`, then Step 1's five `snapshot` commands with `after3-` in place of `before3-`.
Expected: each exits 0; each pair of pictures differs only where the clock draws, the folds and focus mode included. Say what differed, and where.

- [ ] **Step 6: Commit**

```bash
git add mac/JawUI/Sources/Window/WindowContent.swift mac/Tests/JawUITests/WindowContentTests.swift mac/JawApp/Sources/MainWindowController.swift
git commit -F- <<'EOF'
The window's content moves into JawUI as WindowContent: the sidebar, the chat list and the panes' place on the split view with the bottom bar under them, folding by its own width (the sidebar below 960 points, the chat list too below 740 while a chat is open, at once under Reduce Motion), focus mode folding both and putting them back, and showing a Situation with the panes, the status line and the panes' place from one value; the window controller keeps the window around it, and the app's pictures, folded and in focus mode too, are as they were
EOF
```

---

### Task 4: Screens, the window in each of jawd's states

**Files:**
- Create: `mac/JawHarness/Sources/Fixtures/Screens.swift`, `mac/JawHarness/Sources/Scenes/ScreenScenes.swift`
- Modify: `mac/JawHarness/Sources/Specimen.swift` (`ScreenSpecimen`), `mac/JawHarness/Sources/BuiltInScenes.swift`, `mac/JawUI/Sources/Views/Linking.swift` (its clock)

**Interfaces:**
- Consumes: `WindowContent`, `Situation`, `BottomBar`, `DetailController` (Tasks 1 to 3); `Samples`, `Fixtures`, `ControllerSpecimen`, `SpecimenGroup`, `Conditions` (phase one).
- Produces, in the harness:
  - `struct Screen` with `var situation: Situation` (up, linked, online), `var chats: [ChatRow]` (`Samples.chats`), `var open: [ChatRow]` (none), `var layout = "Unsaved Panes"`, `var focusMode = false`, `var width: CGFloat = 1180`, `var height: CGFloat = 780`, and `@MainActor func content(_ conditions: Conditions) -> WindowContent`.
  - `struct ScreenSpecimen: View` with `init(screen: Screen, conditions: Conditions)`.
  - Scenes `screens-jawd`, `screens-connection`, `screens-linking`, `screens-empty` in the Screens section.

- [ ] **Step 1: The linking screen's clock**

`LinkingView` counts a QR code's seconds left from the system's clock (`@State var now = Date()` and the timer's dates), so its picture changes every second. In `mac/JawUI/Sources/Views/Linking.swift`, take `now` from `Format.now()` instead, at first and on each tick (`Date(timeIntervalSince1970: Double(Format.now()) / 1000)`), so the harness's fixed clock holds it still and the app's follows the system's as before.

- [ ] **Step 2: A window as values**

`mac/JawHarness/Sources/Fixtures/Screens.swift`:

```swift
import AppKit
import JawKit
import JawThemes
import JawUI

/// A window as values: jawd's situation, the chats in the list, the chats open in panes (the first
/// with the keys), the layout's name, focus mode and the window's size. The harness makes a
/// `WindowContent` from one as the app makes one from jawd's windows, and shows it as of `Samples.now`.
struct Screen {
    var situation = Situation(reach: .up, linked: true, connection: Connection(state: .online))
    var chats: [ChatRow] = Samples.chats
    var open: [ChatRow] = []
    var layout = "Unsaved Panes"
    var focusMode = false
    var width: CGFloat = 1180
    var height: CGFloat = 780

    /// The open chats side by side on a grid of one row, each in a slot numbered from 1.
    var grid: PaneGrid? {
        guard !open.isEmpty else { return nil }
        let n = Double(open.count)
        return PaneGrid(cols: (0 ... open.count).map { Double($0) / n }, rows: [0, 1],
                        slots: open.enumerated().map { PaneGrid.Slot(id: $0.offset + 1, c0: $0.offset, c1: $0.offset + 1, r0: 0, r1: 1, chat: $0.element.id) })
    }

    /// The window's content made from this screen in `conditions`: the sidebar; the chat list holding
    /// `chats`, the first open one selected; a conversation for each open chat, `Samples`' messages in
    /// the group's, kept alive as the content's children; and the bar, with the layout's name.
    @MainActor
    func content(_ conditions: Conditions) -> WindowContent {
        let roles = conditions.roles
        let list = ChatListController(folder: "all", roles: roles)
        let content = WindowContent(sidebar: SidebarController(), chatList: list, detail: DetailController(), bar: BottomBar(commands: { [] }))
        content.view.frame = NSRect(x: 0, y: 0, width: width, height: height)
        content.view.wantsLayer = true
        content.view.layer?.backgroundColor = roles.ground.cgColor
        list.apply(Update(seq: 1, upsert: chats, total: UInt64(chats.count)))
        let spec = LayoutSpec(style: conditions.style, me: ["person:1"])
        let layouts = Layouts()
        var views: [Int: NSView] = [:]
        var box: NSView?
        for (i, chat) in open.enumerated() {
            let c = ConversationController(chat: chat, spec: spec, layouts: layouts, place: .newest(100))
            content.addChild(c)
            _ = c.view
            let rows = chat.id == Samples.group.id ? Samples.update
                : Update(seq: 1, upsert: [], total: 0, older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0))
            if chat.id == Samples.group.id { c.members = Fixtures.members }
            DispatchQueue.main.async { c.apply(rows) }
            views[i + 1] = c.view
            if i == 0 {
                box = c.composer.view
                list.select(chat.id)
            }
        }
        content.bar.restyle(roles)
        content.bar.show(layout: layout, edited: false)
        content.show(situation, chats: chats, focused: open.first,
                     panes: grid.map { WindowContent.Panes(grid: $0, views: views, focused: 1, box: box, look: PaneLook(outline: roles.accent)) },
                     roles: roles, now: Date(timeIntervalSince1970: Double(Samples.now) / 1000), actions: WindowContent.Actions())
        content.fold(chatOpen: !open.isEmpty, animated: false)
        if focusMode { content.focusMode(true, animated: false) }
        return content
    }
}
```

Read `PaneLook`'s initialiser and the app's `Preferences.paneLook` before relying on `PaneLook(outline:)`: use what the app draws by default (its dim and its outline in the accent).

- [ ] **Step 3: The screen specimen**

In `mac/JawHarness/Sources/Specimen.swift`, after `ControllerSpecimen`:

```swift
/// A whole window's content at the screen's size, as the window holds it, made from a `Screen`.
struct ScreenSpecimen: View {
    let screen: Screen
    let conditions: Conditions

    var body: some View {
        ControllerSpecimen { screen.content(conditions) }
            .frame(width: screen.width, height: screen.height)
    }
}
```

- [ ] **Step 4: The scenes**

`mac/JawHarness/Sources/Scenes/ScreenScenes.swift`: four scenes in the Screens section, each 1,180 points wide (`width: 1180`), each a `VStack` of bands, one band a screen, with notes saying what the screen should show (the state in the panes' place, the status line's words, in the accent or, when it warns, the mention colour; nothing else of the window changed), covering `WindowContent`, `DetailController`, `BottomBar`, `StateView`, `StatusLine` and, for linking, `LinkingView`:

- `screens-jawd`, "Reaching jawd": starting jawd (`reach: .starting(failed: nil)`); jawd didn't start (`.starting(failed: "jawd stopped as it started (status 1)")`); out of reach with nothing drawn yet (`.unreachable(why: "jawd isn't running: nothing answers on ~/Library/Application Support/jaw/jaw.sock", retry: 2)` with `chats: []`); connecting to jawd, the chats there and none open (`.waiting`).
- `screens-connection`, "The connection": Lisbon Climbing open (`open: [Samples.group]`) while catching up 1,204 of 5,000, with the phone offline, offline, with an update needed, banned until three days after `Samples.now`, and with the engine restarting.
- `screens-linking`, "Linking": not linked, with a QR code (`linked: false`, `link: .qr(code: "2@jaw-harness,QWERTYUIOPASDFGHJKL,ZXCVBNM=", expires: Samples.now + 45_000)`); logged out (`linked: false`, the connection `.loggedOut` with the reason "logged out from the phone").
- `screens-empty`, "Nothing open": no chats yet (`chats: []`); no chat open (the chats, none open).

Register the four in `BuiltInScenes.swift`, after `.states`.

- [ ] **Step 5: Run the tests, sweep and look**

Run: `mac/scripts/build-mac test JawHarnessTests` (every scene drawn, twice the same), then `mac/scripts/build-mac sweep`.
Expected: both exit 0; the sweep draws 10 scenes into 26 pictures. Look at each new scene's dark picture and one light one against their notes: the states fill the panes' place, the status line says the state, the chat list holds its four chats where it should, and the QR code is drawn crisp with its seconds left. Sweep again, a minute later, and check that the linking scene's hashes are the same.

- [ ] **Step 6: Commit**

```bash
git add mac/JawHarness mac/JawUI/Sources/Views/Linking.swift
git commit -F- <<'EOF'
Jaw Harness draws the whole window from values: a Screen gives jawd's situation, the chats, the chats open, the layout's name, focus mode and the size, and makes the window's content from them as the app does; the Screens section shows it while jawd starts, didn't start, is out of reach or is connecting, through each state of the connection, linking and logged out, and with nothing open; the linking screen counts a code's seconds by Format's clock, so its picture holds still in the harness
EOF
```

---

### Task 5: Screens, with chats and with overlays

**Files:**
- Modify: `mac/JawHarness/Sources/Scenes/ScreenScenes.swift`, `mac/JawHarness/Sources/Specimen.swift` (overlays), `mac/JawHarness/Sources/BuiltInScenes.swift`, `mac/README.md`

**Interfaces:**
- Consumes: `Screen`, `ScreenSpecimen` (Task 4); `PaletteView`, `PaletteModel`, `GlanceView`, `GlanceState`, `SettingsView`, `Preferences`, `Fixtures.found`.
- Produces: `ScreenSpecimen`'s `overlay: Overlay?` (`enum Overlay { case palette, glance, settings }`), and scenes `screens-chat`, `screens-panes`, `screens-focus`, `screens-folded`, `screens-overlays`.

- [ ] **Step 1: Overlays drawn where the window puts them**

Give `ScreenSpecimen` an `overlay: Overlay? = nil`, drawn over the window's content in a `ZStack`, as views rather than panels so a picture holds them:

- **The palette:** `PaletteView` over a `PaletteModel` with `"ar"` typed and `Fixtures.found` answered (as the palette scene makes it), 620 by 490 points, centred, its top 60 points below the window's top, as `PalettePanel.show(over:)` places it.
- **The glance:** `GlanceView` over a `GlanceState` holding `Situation.needs(screen.chats, shown: …)` of the screen's chats and, for Lisbon Climbing, the newest three of `Samples.messages` (`Messages(chat:place:)` with `.newest(3)`, applied as `Glance.apply` does), 520 points wide and `GlanceView.height(min(count, 5))` tall, centred in the window, as the app's `Glance` shows it.
- **Settings:** `SettingsView` over a `Preferences` (the harness's own defaults; set its look, density, text size and font from the conditions first), at its own size, centred along the window's top edge, as a sheet comes down from it, with a shadow from the theme's separator.

Read `PalettePanel.show(over:)`, `Glance.show(over:roles:)` and `MainWindowController.showSettings()` before placing them, and place each as they do.

- [ ] **Step 2: The scenes**

In `ScreenScenes.swift`, five more scenes in the Screens section, 1,180 points wide but where they say, each with notes on what to look for, covering what they draw (`WindowContent`, `ConversationController`, `GridView`, `BoxPlace`, `PaletteView`, `GlanceView`, `SettingsView` among them):

- `screens-chat`, "One chat": Lisbon Climbing open, in both looks (`axes: .look`): the header, the messages, the box naming the chat, the status line with what needs you elsewhere.
- `screens-panes`, "Three panes": Lisbon Climbing, Ana Ribeiro and Mãe open side by side, the first with the keys and outlined, the others dimmed, the layout's name "Work" in the bar; Ana's and Mãe's panes are empty chats, the Mimic in each, until phase three's account gives them messages.
- `screens-focus`, "Focus mode": Lisbon Climbing open in focus mode, the sidebar and the chat list folded away.
- `screens-folded`, "Folded": Lisbon Climbing open at 940 by 700 points, the sidebar folded; at 720 by 600, the chat list folded too; and at 520 by 420, the least the window takes. The scene is as wide as its widest screen.
- `screens-overlays`, "With overlays": Lisbon Climbing open with the palette over it, with the glance over it, and with Settings over it.

Register them in `BuiltInScenes.swift` after `screens-empty`.

- [ ] **Step 3: Say so in the README**

In `mac/README.md`'s "Component harness", add a sentence: the Screens section draws the whole window from a `Screen` (jawd's situation, the chats, the chats open, focus mode, the size), as the app draws it from jawd's windows, through the same `WindowContent`.

- [ ] **Step 4: Run the tests, sweep and look**

Run: `mac/scripts/build-mac test JawHarnessTests`, then `mac/scripts/build-mac sweep`, then `mac/scripts/review-marks`.
Expected: each exits 0; the sweep draws 15 scenes into 38 pictures; `review-marks` lists 15 scenes. Look at each new scene's dark picture and one light one against their notes, and say what each shows. Say the sweep's size (`du -sh mac/build/review`).

- [ ] **Step 5: Commit**

```bash
git add mac/JawHarness mac/README.md
git commit -F- <<'EOF'
Jaw Harness's Screens show the window with chats too: one chat in each look, three panes with the first outlined and the others dimmed, focus mode, folded at 940, 720 and 520 points, and with the palette, the glance or Settings over it, each drawn as a view where the window puts it, so a picture holds it; the Mac README says the Screens section draws the window from a Screen through the same WindowContent the app draws
EOF
```
