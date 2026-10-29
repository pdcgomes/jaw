# The panes kept by jawd (parity step 1.1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open the Terminal's Panes in jaw's Mac app, the first half of [the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 1.1: the panes jawd keeps for the terminal, asked for with `layout.get` and `interface` `jaw`, open as the unsaved panes with the terminal's focused pane taking the keys. Every pane gets its chat even when the chat list doesn't hold it, or its slot says why it can't: the chat is gone, or jawd refused to look it up.

**Architecture:** JawKit asks for an interface's panes and reads jawd's answer whole as `KeptLayout`, format 1's tree of splits. It also looks up the row of a chat no window of the app's holds, through a window of one row around it in `all` and then in `archived`, one chat at a time, let go as soon as jawd answers. JawUI reads the terminal's tree as a grid with `PaneGrid.from` (`TerminalPanes`), saying what can't open, and a slot whose chat won't have a pane says why, from words that travel with the grid in `WindowContent.Panes` (`unopened`). The app's model asks from a command in the Layouts menu and the palette, puts the grid where the unsaved panes were, and has `reconcile` look up the chats the chat list doesn't hold, the focused slot's first, which also fills the panes the app's own layouts used to leave waiting.

**Tech Stack:** Swift 6 (AppKit, Swift Testing), Tuist, xcodebuild on the build Mac through `mac/scripts/build-mac`, POSIX `sh`; Python 3 for `make check-parity` and for keeping a terminal layout on `jawd demo` in the live check.

## Global Constraints

- The spec is the first half ("Now") of the parity plan's step 1.1 (`docs/plans/2026-10-27-jaw-mac-parity-plan.md`), with the grid layouts design's "A tree reads as a grid", "Switching" and "When things go wrong" (`docs/plans/2026-10-26-jaw-mac-grid-layouts-design.md`) and the "Layouts" and "Windows" sections of `protocol/README.md`. Where this plan and they differ, they win: stop and say so.
- This is Part A. Part B, with P10, gets a plan of its own (the last section here). Nothing in the Mac's sources sends `layout.set` or reads the `layout` push, and that includes a `case "layout"` anywhere: `make check-parity` reads `mac/JawKit/Sources`, `mac/JawUI/Sources` and `mac/JawApp/Sources`, and those two rows stay in `mac/parity.toml` for Part B. Tests may use both, since `mac/Tests` isn't read.
- The branch is `mac-panes`, in its own worktree, `/Users/pedrogomes/src/jaw-panes`. The controller makes both from main at fa9265c when it commits this plan there; work only there. `mac-m1` has been on main since fa9265c. What has to land before Task 1 is `mac-harness`, the harness's second phase, into `mac-m1` and then main. Once it has, rebase: `git fetch origin && git rebase origin/main`. Then check that main has what this plan builds on: `grep -n "func reconcile(focus" mac/JawApp/Sources/AppModel.swift` and `ls mac/JawUI/Sources/Window/WindowContent.swift` must both find it; if either doesn't, stop and say so.
- Never push, never pass `--no-verify`, never rewrite commits that aren't yours. Never edit, build in, check out in, or run a git command that changes anything in another agent's worktree or in the owner's checkout, `/Users/pedrogomes/src/jaw`; only read them.
- Build and test on the build Mac with `mac/scripts/build-mac`: `test JawKit`, `test JawUITests`, `test JawHarnessTests`, `build` (the `jaw` app) and `snapshot`. `mac/scripts/check` exits 1 when a build or the tests fail, so both a run's exit status and its printed `** BUILD SUCCEEDED **` / `** TEST SUCCEEDED **` and `Test run with …` lines count. For the whole log: `ssh -o BatchMode=yes build-mac 'tail -150 ~/src/build-mac/jaw/jaw-panes/mac/build/check.log'`. Before each run, check that the build Mac answers: `ssh -o BatchMode=yes -o ConnectTimeout=5 build-mac true`. If it doesn't, or `build-mac` prints `doesn't answer; running here` and falls back to building on this Mac, stop (interrupt the run you started) and ask the controller. Never build or test on this Mac.
- JawKit's live tests, `--demo` and the live check need `target/debug/jawd` in this worktree, which `build-mac` copies to the build Mac. Copy the Mac builder's, as it is: `mkdir -p target/debug && cp /Users/pedrogomes/src/jaw-mac-m1/target/debug/jawd target/debug/jawd` (about 48 MB, ignored by git). Never build Rust, check anything out or change anything in that checkout. If its jawd is missing, older than that checkout's HEAD commit, or that HEAD isn't origin/main's, stop and ask the controller, who keeps it current. This prints `current` when none of those is so:

  ```sh
  m1=/Users/pedrogomes/src/jaw-mac-m1
  test -x "$m1/target/debug/jawd" && [ "$(git -C "$m1" rev-parse HEAD)" = "$(git rev-parse origin/main)" ] \
      && [ "$(stat -f %m "$m1/target/debug/jawd")" -ge "$(git -C "$m1" log -1 --format=%ct HEAD)" ] && echo current
  ```

  Never run the Go engine: use only `jawd --engine demo` and `jawd demo`, each on a folder of its own. Never stop a process by name; stop only one whose PID you started.
- Swift 6 language mode, macOS 26, warnings are errors. `mac/scripts/check-components` refuses fonts, radii and line widths set by hand outside `JawUI/Sources/Components`. `mac/scripts/check-harness` refuses a view or a view controller in the app target, and `Jawd` or `Client` named in the harness.
- The code goes where the harness's second phase leaves room for it: JawKit, `JawUI/Sources/Panes`, `AppModel`'s layout code, `Commands.swift` and `Bench.swift`'s steps. Beyond that, the words for a slot whose chat won't have a pane go into `WindowContent.Panes`, through `DetailController.show` to `GridView.show`, and `MainWindowController.panesShown` fills them from the model, so the harness can draw such a slot from values; nothing else changes in those three. Line numbers here are main's at fa9265c, where `mac-m1` landed, and mac-harness's for what it adds or rewrites (`DetailController`, `WindowContent` and `MainWindowController.panesShown`). Once mac-harness lands, `AppModel`'s `apply(_:)` moves up about 5 lines and its layout code about 60, so find each place by the symbol named.
- Opening the terminal's panes puts them where the unsaved panes (`book.current == nil`) were and discards what those held, without asking, even when a named layout is on screen and the unsaved panes aren't showing. That's unlike switching, which keeps the layout it leaves. It's the parity plan's choice, "the tree read as a grid into the unsaved panes", and the command's detail says so. A named layout that was on screen stays as it was, its saved shape and its edit alike. A tree of more than `PaneGrid.most` (12) panes is refused, with words that say so: the terminal makes six at most, and only some other client keeping panes under the terminal's name could keep more (jawd takes 16).
- A lookup holds one of a connection's 32 windows at most, and only until jawd answers it. With the chat list's one window, a layout's 12 and the glance's 12 while it's open, the app watches 26 at most.
- No audit: nothing here is on jaw's data paths (`scripts/check-data-audit.py`). The app reads jawd's layout and writes none, and its preferences are its own state.
- Scratch goes in three places: the worktree's ignored `mac/build/panes/`, the build Mac's copy of the checkout, and the live check's one `mktemp` folder under the build Mac's `TMPDIR`, which its script removes. The tests' folders (`/tmp/jawk-…`, `/tmp/jawl-…`) are their own, and the tests remove them. Before the final report, run `rm -rf mac/build/panes` and `make prune-builds`. Once the branch has landed and its worktree is removed, its copy on the build Mac goes too: `ssh -o BatchMode=yes build-mac 'rm -rf ~/src/build-mac/jaw/jaw-panes'`.
- Every commit passes `make check-parity`: Task 1 takes `layout.get`'s row out of `mac/parity.toml` in the commit where the Mac starts sending it.
- Commit messages are one long sentence in the project's voice (read `git log -5`), with no prefixes. Code matches its surroundings: doc comments in plain sentences, British spelling, no comments that narrate a change, 4-space indentation, and trailing commas in multi-line argument lists.

## File structure

New:

- `mac/JawKit/Sources/Protocol/Layouts.swift`: `layout.get`'s params and answer (`LayoutGet`, `KeptLayout`), and format 1's tree (`SplitLayout`, `SplitTree`).
- `mac/JawUI/Sources/Panes/TerminalPanes.swift`: the terminal's panes read as a grid (`TerminalPanes` and its `Refusal`), and `PaneLayout.init(_: SplitTree)`.
- `mac/Tests/JawKitTests/LookUpTests.swift`: lookups against a fake jawd.
- `mac/Tests/JawUITests/TerminalPanesTests.swift`: the tree read as a grid, and what a slot whose chat won't have a pane says, in the grid and through the window's panes.

Changed:

- `mac/JawKit/Sources/Client/Incoming.swift`: `Expect.layout` and `Reply.layout`, and their decoding.
- `mac/JawKit/Sources/Client/Jawd.swift`: `layout(of:what:)`, `lookUp(_:)` and its window, `Looked`, `JawUpdate.layout` and `JawUpdate.looked`.
- `mac/JawUI/Sources/Panes/GridView.swift`: `show`'s `unopened` words, `GridView.gone`, and the empty pane's words, kept within its well.
- `mac/JawUI/Sources/Window/DetailController.swift` and `mac/JawUI/Sources/Window/WindowContent.swift`, the harness's second phase's: `unopened` passed on, and carried in `WindowContent.Panes`.
- `mac/JawApp/Sources/AppModel.swift`: `openTerminalPanes()`, `terminalPanes(_:)`, `lookedUp(_:_:)`, `unopened`, and the lookups in `reconcile(focus:)`.
- `mac/JawApp/Sources/MainWindowController.swift`: `panesShown` passes `unopened`.
- `mac/JawApp/Sources/Commands.swift`: Open the Terminal's Panes.
- `mac/JawApp/Sources/Bench.swift`: the snapshot's `terminal-panes` step.
- `mac/Tests/JawKitTests/FixtureTests.swift`, `ClientTests.swift`, `LiveTests.swift`.
- `mac/parity.toml` (Task 1); `mac/README.md` and `docs/plans/2026-10-27-jaw-mac-parity-plan.md` (Task 6).

---

### Task 1: The panes jawd keeps, asked for and read in JawKit

**Files:**
- Create: `mac/JawKit/Sources/Protocol/Layouts.swift`
- Modify: `mac/JawKit/Sources/Client/Incoming.swift:4-16` (`Expect`, `Reply`) and `:72-82` (the reply's decoding); `mac/JawKit/Sources/Client/Jawd.swift` (`status()` at 154, `answered` at 239, `Asked` at 308, `refused` at 386, `JawUpdate` at 437); `mac/JawApp/Sources/AppModel.swift:202` (`apply`'s last case); `mac/parity.toml:8` (`layout.get`'s row)
- Test: `mac/Tests/JawKitTests/FixtureTests.swift`, `mac/Tests/JawKitTests/ClientTests.swift`

**Interfaces:**
- Produces, in JawKit:
  - `public struct LayoutGet: Encodable, Sendable, Hashable { public var interface: String? }`, its memberwise init internal.
  - `public struct KeptLayout: Decodable, Sendable, Hashable`, with `interface: String`, `v: UInt64`, `layout: SplitLayout?`, `dropped: [String]` and `public init(interface: String, v: UInt64, layout: SplitLayout? = nil, dropped: [String] = [])`. A layout in a format other than 1 decodes as none; a malformed tree in format 1 fails the answer with its decoding error.
  - `public struct SplitLayout: Decodable, Sendable, Hashable`, with `focused: Int`, `tree: SplitTree` and `public init(focused: Int, tree: SplitTree)`.
  - `public indirect enum SplitTree: Decodable, Sendable, Hashable { case chat(String); case split(How, first: Int, SplitTree, SplitTree) }`, with `public enum How: String, Decodable, Sendable, Hashable { case across, down }`.
  - `Expect.layout`, `Reply.layout(KeptLayout)`.
  - `Jawd.layout(of interface: String, what: String)`: `layout.get` with `interface`. The answer comes as `JawUpdate.layout(KeptLayout)`, and a refusal as `.notice(Jawd.whyNot(failure, what), .warning)`.
- The app answers nothing yet: `AppModel.apply` lets `.layout` go with `.push` and `.chats` until Task 5.
- `mac/parity.toml` loses `layout.get`'s row, so `make check-parity` passes from this commit on.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawKitTests/FixtureTests.swift`, make three changes.

In `every result and push fixture decodes`, add this line to the `want` switch, after `case "status": .reply(.status)`. The `layout` push needs no line: a push carries no id, `IncomingLine` reads it by its method, and it stays an untyped push until Part B.

```swift
        case "layout.get": .reply(.layout)
```

After `rows decode as the protocol draws them`, add a test:

```swift
@Test func `the panes jawd keeps decode as a tree, with the chats of the panes it left out`() throws {
    guard case let .reply(_, .layout(kept)) = try incoming("layout.get.result.json", .reply(.layout)) else {
        Issue.record("not layout.get's answer")
        return
    }
    #expect(kept == KeptLayout(interface: "jaw", v: 1_792_000_000_123, layout: SplitLayout(focused: 1, tree: .split(
        .across, first: 500, .chat("351912345678@s.whatsapp.net"), .chat("120363012345678901@g.us"),
    )), dropped: ["351919999999@s.whatsapp.net"]))
    let none = try JSONDecoder().decode(KeptLayout.self, from: Data(#"{"interface":"jaw","v":0}"#.utf8))
    #expect(none == KeptLayout(interface: "jaw", v: 0), "nothing kept")
    let book = try JSONDecoder().decode(KeptLayout.self, from: Data(#"{"interface":"jaw-mac","v":5,"layout":{"format":2,"layouts":[]}}"#.utf8))
    #expect(book.v == 5 && book.layout == nil, "panes in a format other than 1's tree are none this Jaw reads")
    let malformed = #"{"interface":"jaw","v":5,"layout":{"format":1,"focused":0,"tree":{"split":{"how":"sideways"}}},"dropped":["351911111111@s.whatsapp.net"]}"#
    #expect(throws: DecodingError.self, "a malformed tree fails the answer rather than reading as panes left out") {
        try JSONDecoder().decode(KeptLayout.self, from: Data(malformed.utf8))
    }
}
```

In `each request JawKit writes equals its fixture`, add this after the `chat.info.request.json` line:

```swift
    try same("layout.get.request.json", written(id: 30, "layout.get", LayoutGet(interface: "jaw")))
```

In `mac/Tests/JawKitTests/ClientTests.swift`, add this to the `ClientTests` suite, after `only the latest find is answered`:

```swift
    @Test func `the terminal's panes are asked for by its name and come whole, and a jawd without them says so`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "layout.get" else { return ordinary(method, params, id) }
            return ["{\"id\":\(id),\"result\":{\"interface\":\"jaw\",\"v\":7,\"layout\":{\"format\":1,\"focused\":1,\"tree\":{\"split\":{\"how\":\"down\",\"first\":400,\"a\":{\"chat\":\"ana\"},\"b\":{\"chat\":\"rui\"}}}},\"dropped\":[\"mum\"]}}"]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.layout(of: "jaw", what: "opening the terminal's panes")
        let all = await drain(jawd) { all in all.contains { if case .layout = $0 { true } else { false } } }
        let kept = all.compactMap { if case let .layout(k) = $0 { k } else { nil } }.first
        #expect(kept == KeptLayout(interface: "jaw", v: 7, layout: SplitLayout(focused: 1, tree: .split(.down, first: 400, .chat("ana"), .chat("rui"))),
                                   dropped: ["mum"]))
        #expect(requests(fake, "layout.get") == [.object(["interface": .string("jaw")])])
        let refused = jawd.refused(.layout("opening the terminal's panes"), .jawd(JawError(code: ErrorCode.unknownMethod, message: "?")))
        guard case let .notice(text, tone)? = refused.first else {
            Issue.record("nothing said: \(refused)")
            return
        }
        #expect(text == "opening the terminal's panes comes with the next step" && tone == .warning)
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 1, with the build failing on `cannot find 'KeptLayout' in scope` and `type 'Expect' has no member 'layout'`.

- [ ] **Step 3: Write the protocol's types**

`mac/JawKit/Sources/Protocol/Layouts.swift`:

```swift
import Foundation

/// `layout.get`'s params: whose panes, by the name its interface gives in hello; the connection's
/// own when nil.
public struct LayoutGet: Encodable, Sendable, Hashable {
    public var interface: String?
}

/// What jawd keeps of an interface's panes, as `layout.get` answers: the interface, the version
/// kept (0 for none), the panes, unless none are kept, none are left or they're in a format this
/// Jaw doesn't read, and the chats of the panes jawd left out, which are gone or shown by a pane
/// before them.
public struct KeptLayout: Decodable, Sendable, Hashable {
    public var interface: String
    public var v: UInt64
    public var layout: SplitLayout?
    public var dropped: [String]

    public init(interface: String, v: UInt64, layout: SplitLayout? = nil, dropped: [String] = []) {
        self.interface = interface
        self.v = v
        self.layout = layout
        self.dropped = dropped
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        interface = try c.decode(String.self, forKey: .interface)
        v = try c.decode(UInt64.self, forKey: .v)
        dropped = try c.decodeIfPresent([String].self, forKey: .dropped) ?? []
        if try c.decodeIfPresent(Format.self, forKey: .layout)?.format == 1 {
            layout = try c.decode(SplitLayout.self, forKey: .layout)
        } else {
            layout = nil
        }
    }

    /// A layout's format alone, read before the rest: format 1's tree is read whole, failing the
    /// answer if it's malformed, and any other format is one this Jaw doesn't read.
    struct Format: Decodable {
        let format: Int
    }

    enum Keys: String, CodingKey {
        case interface, v, layout, dropped
    }
}

/// Panes as a tree of splits, format 1, as the terminal keeps them, and the pane with the keys,
/// counted from 0 in the tree's order, a split's first side before its second.
public struct SplitLayout: Decodable, Sendable, Hashable {
    public var focused: Int
    public var tree: SplitTree

    public init(focused: Int, tree: SplitTree) {
        self.focused = focused
        self.tree = tree
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let format = try c.decode(Int.self, forKey: .format)
        guard format == 1 else {
            throw DecodingError.dataCorruptedError(forKey: .format, in: c,
                                                   debugDescription: "this Jaw reads format 1's tree, not format \(format)")
        }
        focused = try c.decode(Int.self, forKey: .focused)
        tree = try c.decode(SplitTree.self, forKey: .tree)
    }

    enum Keys: String, CodingKey {
        case format, focused, tree
    }
}

/// A pane, by the JID of the chat it shows, or a split of two: `across` puts its first side on the
/// left, `down` on top, and `first` is that side's share in thousandths, 1 to 999.
public indirect enum SplitTree: Decodable, Sendable, Hashable {
    case chat(String)
    case split(How, first: Int, SplitTree, SplitTree)

    public enum How: String, Decodable, Sendable, Hashable {
        case across, down
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        if let chat = try c.decodeIfPresent(String.self, forKey: .chat) {
            self = .chat(chat)
            return
        }
        let split = try c.nestedContainer(keyedBy: SplitKeys.self, forKey: .split)
        self = try .split(split.decode(How.self, forKey: .how), first: split.decode(Int.self, forKey: .first),
                          split.decode(SplitTree.self, forKey: .a), split.decode(SplitTree.self, forKey: .b))
    }

    enum Keys: String, CodingKey {
        case chat, split
    }

    enum SplitKeys: String, CodingKey {
        case how, first, a, b
    }
}
```

- [ ] **Step 4: Read the answer as one**

In `mac/JawKit/Sources/Client/Incoming.swift`, `Expect` and `Reply` become:

```swift
/// What a request's result is read as, so each line is decoded once, into the value it is.
public enum Expect: Sendable, Hashable {
    case json, sent, chat, info, chatInfo, status, layout
}

/// A request's answer, read as what it was asked for.
public enum Reply: Sendable, Hashable {
    case json(JSONValue)
    case sent(Sent)
    case chat(ChatRow)
    case info(MessageInfo)
    case chatInfo(ChatInfo)
    case status(Status)
    case layout(KeptLayout)
}
```

In `IncomingLine.init(from:)`, add this after the `.reply(.status)` case:

```swift
                case .reply(.layout): value = try .reply(id, .layout(c.decode(KeptLayout.self, forKey: .result)))
```

- [ ] **Step 5: Ask for it**

In `mac/JawKit/Sources/Client/Jawd.swift`, make five changes.

After `status()`:

```swift
    /// Asks for the panes jawd keeps for `interface`, by the name it gives in hello. They come as
    /// `.layout`; what stops them is said as `what` not being done.
    public func layout(of interface: String, what: String) {
        ask("layout.get", LayoutGet(interface: interface), .layout(what), expect: .layout)
    }
```

In `answered(_:_:_:)`, before `default:`:

```swift
        case let (.layout, .layout(kept)):
            out.append(.layout(kept))
```

In `enum Asked`, after `case status`:

```swift
        /// Panes asked for, named as opening them is said: "opening the terminal's panes".
        case layout(String)
```

In `refused(_:_:)`, before `case .status, .quiet:`:

```swift
        case let .layout(what):
            return [.notice(Self.whyNot(failure, what), .warning)]
```

In `JawUpdate`, after `case status(Status)`:

```swift
    /// The panes jawd keeps for an interface, as `layout.get` answered.
    case layout(KeptLayout)
```

- [ ] **Step 6: The app lets it go until Task 5**

In `mac/JawApp/Sources/AppModel.swift`, the last case of `apply(_:)` becomes:

```swift
        case .push, .chats, .layout:
            break
```

- [ ] **Step 7: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawKit`, then `mac/scripts/build-mac build`.
Expected: both exit 0, and JawKit passes with two more tests than before.

- [ ] **Step 8: The parity list loses the row**

Run: `make check-parity`
Expected: exit 1, with `check-parity: the Mac sends layout.get now: take it out of mac/parity.toml (step 1.1)`.

In `mac/parity.toml`, delete this line from `[requests]`. `"layout.set" = "1.1"` and, under `[pushes]`, `"layout" = "1.1"` stay, for Part B:

```toml
"layout.get" = "1.1"
```

Run `make check-parity` again.
Expected: exit 0, its list holding one row fewer than before, with step 1.1 reading `1.1 The panes kept by jawd: layout push, layout.set`.

- [ ] **Step 9: Commit**

```bash
git add mac/JawKit/Sources/Protocol/Layouts.swift mac/JawKit/Sources/Client/Incoming.swift mac/JawKit/Sources/Client/Jawd.swift \
    mac/JawApp/Sources/AppModel.swift mac/Tests/JawKitTests/FixtureTests.swift mac/Tests/JawKitTests/ClientTests.swift mac/parity.toml
git commit -F- <<'EOF'
JawKit asks jawd for the panes it keeps for an interface, layout.get with the interface's name, and reads the answer once into KeptLayout: the version kept, format 1's tree of splits with the pane that has the keys, and the chats of the panes jawd left out, panes in another format being none this Jaw reads and a malformed tree failing the answer; the Mac sends layout.get now, so its row leaves mac/parity.toml and make check-parity passes; tests: the fixtures decode and the request equals its fixture, a fake jawd's answer comes whole, and a jawd that doesn't know layout.get says opening the terminal's panes comes with the next step
EOF
```

---

### Task 2: A chat's row from outside the app's windows

`chats.watch` around a chat's `id` finds the chat by that JID, whatever its list (`read::chat_place` in `crates/jawd/src/store/read.rs`). If the chat is in the folder watched, a window with `before` 0 and `after` 0 holds exactly its row. If it's in the other list, archived when `all` is watched, the watch succeeds with no rows. If jawd has no chat by that JID, the watch is error 3, "jawd has no chat …", and no window is made. Unlike `layout.get`, it doesn't follow a merged chat or a person's other JID. So a lookup tries `all`, then `archived`, and calls a chat in neither gone.

**Files:**
- Create: `mac/Tests/JawKitTests/LookUpTests.swift`
- Modify: `mac/JawKit/Sources/Client/Jawd.swift` (a section after `find(id:query:kinds:chat:limit:)` at 167, `drain()` at 180, `take(_:_:)`'s `.window` case at 224, `Lookup` beside `Sending` at 262, `Looked` and `JawUpdate.looked`); `mac/Tests/JawKitTests/LiveTests.swift`; `mac/JawApp/Sources/AppModel.swift` (`apply`'s last case)

**Interfaces:**
- Consumes: `Jawd.layout(of:what:)`, `KeptLayout`, `SplitLayout` and `SplitTree` (Task 1), in the live test.
- Produces, in JawKit:
  - `public enum Looked: Sendable, Hashable { case row(ChatRow), gone, refused(String) }`.
  - `Jawd.lookUp(_ chats: [String])`: each chat's row is looked up in turn, and each answer comes as `JawUpdate.looked(chat: String, Looked)`. Another call replaces the chats not yet looked up; the one being looked up still answers.

- [ ] **Step 1: Write the failing tests**

`mac/Tests/JawKitTests/LookUpTests.swift`:

```swift
import Foundation
@testable import JawKit
import Testing

/// Chats looked up outside the caller's windows: a window of one row around each, in all and then
/// in archived, let go as soon as jawd answers, one chat at a time, so a lookup holds one of jawd's
/// 32 windows at most.

/// A jawd whose all holds ana and whose archived holds old, which has no chat gone and no window
/// left for busy; each watch has an id of its own.
func listing(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    guard method == "chats.watch", let folder = params["folder"]?.string, let around = params["around"]?.string, around != "top" else {
        return ordinary(method, params, id)
    }
    switch around {
    case "gone":
        return ["{\"id\":\(id),\"error\":{\"code\":3,\"message\":\"jawd has no chat gone\"}}"]
    case "busy":
        return ["{\"id\":\(id),\"error\":{\"code\":5,\"message\":\"a connection can watch at most 32 windows at once, so one has to be unwatched first\",\"data\":{\"limit\":\"windows\",\"most\":32}}}"]
    default:
        let holds = (folder, around) == ("all", "ana") || (folder, around) == ("archived", "old")
        let rows = holds ? FakeJawd.chat(around, v: 7, sort: "0~5", name: around) : ""
        return ["{\"id\":\(id),\"result\":{\"watch\":\"w\(id)\",\"seq\":10,\"total\":5,\"rows\":[\(rows)]}}"]
    }
}

/// What the lookups found, in the order they answered.
func lookedUp(_ all: [JawUpdate]) -> [(String, Looked)] {
    all.compactMap { if case let .looked(chat, found) = $0 { (chat, found) } else { nil } }
}

/// The windows the fake was asked for, in order: each watch's folder, chat and sides, and each unwatch.
func windows(_ fake: FakeJawd) -> [String] {
    fake.requests.compactMap { r -> String? in
        switch r.method {
        case "chats.watch":
            "\(r.params["folder"]?.string ?? "?") \(r.params["around"]?.string ?? "?") \(r.params["before"]?.int ?? -1)/\(r.params["after"]?.int ?? -1)"
        case "unwatch": "unwatch"
        default: nil
        }
    }
}

@MainActor
@Suite(.serialized) struct LookUpTests {
    @Test func `a chat outside the list is found in all, then in archived, and one jawd has none of is gone`() async throws {
        let fake = try FakeJawd(script: listing)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        jawd.lookUp(["ana", "old", "gone"])
        let found = lookedUp(await drain(jawd) { lookedUp($0).count == 3 })
        #expect(found.map { $0.0 } == ["ana", "old", "gone"], "in turn")
        if case let .row(row)? = found.first?.1 { #expect(row.id == "ana") } else { Issue.record("ana wasn't found: \(found)") }
        if case let .row(row)? = found.dropFirst().first?.1 { #expect(row.id == "old") } else { Issue.record("old wasn't found in archived: \(found)") }
        #expect(found.last?.1 == .gone)
        #expect(windows(fake) == ["all ana 0/0", "unwatch", "all old 0/0", "unwatch", "archived old 0/0", "unwatch", "all gone 0/0"],
                "one window at a time, each let go once it answered, and archived not asked about a chat jawd has none of")
    }

    @Test func `a window jawd refuses is said in its words, and the next chat is looked up`() async throws {
        let fake = try FakeJawd(script: listing)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        jawd.lookUp(["busy", "ana"])
        let found = lookedUp(await drain(jawd) { lookedUp($0).count == 2 })
        #expect(found.first?.0 == "busy")
        #expect(found.first?.1 == .refused("a connection can watch at most 32 windows at once, so one has to be unwatched first"))
        if case let .row(row)? = found.last?.1 { #expect(row.id == "ana") } else { Issue.record("ana wasn't looked up after: \(found)") }
    }

    @Test func `asking again replaces the chats still to look up, and the one under way still answers`() async throws {
        let fake = try FakeJawd(script: listing)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        jawd.lookUp(["ana", "old"])
        jawd.lookUp(["gone"])
        let found = lookedUp(await drain(jawd) { lookedUp($0).count == 2 })
        #expect(found.map { $0.0 } == ["ana", "gone"])
        #expect(!windows(fake).contains { $0.contains(" old ") }, "old was never looked for")
    }
}
```

In `mac/Tests/JawKitTests/LiveTests.swift`, add this after the `Held` class:

```swift
/// Keeps `params` as the terminal's panes, as jaw does: a connection that says hello as jaw, then
/// layout.set. Says whether jawd kept them.
func keepTerminalPanes(_ socket: String, _ params: JSONValue) async -> Bool {
    let terminal = Client(config: .init(socket: socket, client: "jaw", version: "0"), wake: { _ in })
    terminal.start()
    defer { terminal.close() }
    var asked: UInt64?
    let end = Date().addingTimeInterval(10)
    while Date() < end {
        for event in terminal.drain().events {
            switch event {
            case .connected where asked == nil:
                if case let .success(id) = terminal.request("layout.set", params) { asked = id }
            case let .reply(id, .success(.json(answer))) where id == asked:
                return answer["kept"]?.bool == true
            default:
                break
            }
        }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return false
}
```

Then add this at the end of the `LiveTests` suite:

```swift
    @Test func `the terminal's panes come from a real jawd as it keeps them, and a chat is looked up by its JID`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let held = Held(demo.socket)
        defer { held.jawd.close() }
        let ana = "351912345678@s.whatsapp.net", climbing = "120363012345678901@g.us", unknown = "351919999999@s.whatsapp.net"
        #expect(await held.until { h in [ana, climbing].allSatisfy { id in h.chats.list.contains { $0.id == id } } }, "the demo's chats come")
        let params = try JSONValue.parse(Data("""
        {"v":1,"layout":{"format":1,"focused":2,"tree":{"split":{"how":"across","first":600,"a":{"chat":"\(ana)"},\
        "b":{"split":{"how":"down","first":500,"a":{"chat":"\(climbing)"},"b":{"chat":"\(unknown)"}}}}}}}
        """.utf8))
        #expect(await keepTerminalPanes(demo.socket, params), "the terminal keeps its panes, as jaw does")

        held.jawd.layout(of: "jaw", what: "opening the terminal's panes")
        #expect(await held.until { h in h.updates.contains { if case .layout = $0 { true } else { false } } })
        let kept = held.updates.compactMap { if case let .layout(k) = $0 { k } else { nil } }.first
        #expect(kept?.interface == "jaw" && kept?.v == 1)
        #expect(kept?.layout == SplitLayout(focused: 1, tree: .split(.across, first: 600, .chat(ana), .chat(climbing))),
                "the pane of a chat jawd has none of is left out, its split closed, and the keys go to the pane before it")
        #expect(kept?.dropped == [unknown])

        held.jawd.lookUp([climbing, unknown])
        #expect(await held.until { h in h.updates.count { if case .looked = $0 { true } else { false } } == 2 })
        let found = held.updates.compactMap { if case let .looked(chat, f) = $0 { (chat, f) } else { nil } }
        if case let .row(row)? = found.first?.1 { #expect(row.id == climbing) } else { Issue.record("the group wasn't found: \(found)") }
        #expect(found.last?.0 == unknown && found.last?.1 == .gone, "jawd has no chat by that JID")
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 1, with the build failing on `value of type 'Jawd' has no member 'lookUp'` and `cannot find type 'Looked' in scope`.

- [ ] **Step 3: Look chats up, one at a time**

In `mac/JawKit/Sources/Client/Jawd.swift`, make six changes.

After `find(id:query:kinds:chat:limit:)` and the `pending` property under it, add a section:

```swift
    // MARK: - Chats no window holds

    /// Where a lookup looks, in turn: the chats not archived, then the archived ones, the two lists
    /// jawd serves.
    static let lookIn = ["all", "archived"]

    /// The chats still to look up, in turn.
    var toLookUp: [String] = []
    /// The chat being looked up, and its window of one row.
    var lookup: Lookup?

    /// Looks up the row of each of `chats`, which none of the caller's windows holds: a window of
    /// that one row around it in `all`, then in `archived` when `all` doesn't hold it, let go as soon
    /// as jawd answers, one chat at a time, so a lookup holds one of jawd's 32 windows at most. Each
    /// comes as `.looked`. The chats asked for before and not yet looked up are forgotten; the one
    /// being looked up still answers.
    public func lookUp(_ chats: [String]) {
        toLookUp = chats.filter { $0 != lookup?.chat }
        lookNext()
    }

    func lookNext() {
        guard lookup == nil, !toLookUp.isEmpty else { return }
        look(toLookUp.removeFirst(), in: Self.lookIn[0])
    }

    func look(_ chat: String, in folder: String) {
        lookup = Lookup(window: client.watchChats(folder, .around(chat, before: 0, after: 0)), chat: chat, folder: folder)
    }

    /// What a lookup's window answered: the chat's row, or that the list doesn't hold it, when the
    /// next list is looked in, and after the last, that it's gone. Its window is let go either way.
    func looked(_ update: Update<ChatRow>, _ out: inout [JawUpdate]) {
        guard let lookup else { return }
        let row = update.upsert.first { $0.id == lookup.chat }
        guard row != nil || update.moved != nil else { return }
        client.unwatch(lookup.window)
        self.lookup = nil
        if let row {
            out.append(.looked(chat: lookup.chat, .row(row)))
        } else if let next = Self.lookIn.drop(while: { $0 != lookup.folder }).dropFirst().first {
            look(lookup.chat, in: next)
            return
        } else {
            out.append(.looked(chat: lookup.chat, .gone))
        }
        lookNext()
    }

    /// A lookup's window that jawd refused: a chat it has no row for is gone, and any other refusal,
    /// such as every window being taken, is said in jawd's words.
    func lookFailed(_ failure: Failure, _ out: inout [JawUpdate]) {
        guard let lookup else { return }
        client.unwatch(lookup.window)
        self.lookup = nil
        out.append(.looked(chat: lookup.chat, failure.code == ErrorCode.notFound ? .gone : .refused(failure.description)))
        lookNext()
    }
```

In `drain()`, the chat lists' loop becomes:

```swift
        for (window, update) in frame.chats.sorted(by: { $0.key < $1.key }) {
            if window == lookup?.window { looked(update, &out) } else { out.append(.chats(window, update)) }
        }
```

In `take(_:_:)`, the `.window` case becomes:

```swift
        case let .window(window, failure):
            if window == lookup?.window { lookFailed(failure, &out) } else { out.append(.windowFailed(window, failure.description)) }
```

Under "Inside", after `struct Sending`, add:

```swift
    /// A chat being looked up: its window of one row, and the list it's looked for in.
    struct Lookup {
        let window: WindowID
        let chat: String
        let folder: String
    }
```

Before `public enum JawUpdate`, add:

```swift
/// What a lookup found of a chat no window of the caller's holds.
public enum Looked: Sendable, Hashable {
    /// Its row, from `all` or from `archived`.
    case row(ChatRow)
    /// In neither list, as for a chat jawd has no row for.
    case gone
    /// jawd refused its window, in jawd's words.
    case refused(String)
}
```

In `JawUpdate`, after `case layout(KeptLayout)`, add:

```swift
    /// A chat looked up outside the caller's windows, and what was found of it.
    case looked(chat: String, Looked)
```

- [ ] **Step 4: The app lets it go until Task 5**

In `mac/JawApp/Sources/AppModel.swift`, the last case of `apply(_:)` becomes:

```swift
        case .push, .chats, .layout, .looked:
            break
```

- [ ] **Step 5: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawKit`, then `mac/scripts/build-mac build`.
Expected: both exit 0, and JawKit passes with four more tests than before. Check that the live test ran rather than being skipped: `ssh -o BatchMode=yes build-mac "grep -E 'real jawd as it keeps them' ~/src/build-mac/jaw/jaw-panes/mac/build/check.log"` shows it passed. If it says skipped, `target/debug/jawd` is missing: copy the Mac builder's as it is, as the Global Constraints say, never building or checking anything out in its checkout. If that one is missing, older than its checkout's HEAD commit, or that HEAD isn't origin/main's, stop and ask the controller, who keeps it current.

- [ ] **Step 6: Commit**

```bash
git add mac/JawKit/Sources/Client/Jawd.swift mac/JawApp/Sources/AppModel.swift mac/Tests/JawKitTests/LookUpTests.swift \
    mac/Tests/JawKitTests/LiveTests.swift
git commit -F- <<'EOF'
JawKit looks up the row of a chat no window of the app's holds with a window of that one row around it, in all and then in archived, let go as soon as jawd answers, one chat at a time so a lookup holds one of jawd's 32 windows at most: a chat in neither list, or one jawd has no row for, is gone, and a window jawd refuses is said in its words; tests against a fake jawd, and against jawd --engine demo, where the terminal's panes come as it keeps them with the pane whose chat jawd lacks left out, and a lookup finds the demo's group and calls a made-up JID gone
EOF
```

---

### Task 3: The terminal's tree read as a grid

**Files:**
- Create: `mac/JawUI/Sources/Panes/TerminalPanes.swift`
- Test: `mac/Tests/JawUITests/TerminalPanesTests.swift`

**Interfaces:**
- Consumes: `KeptLayout`, `SplitLayout` and `SplitTree` (Task 1); `PaneGrid.from(_:)`, `PaneGrid.most`, `PaneGrid.holds` and `PaneLayout`, as they are.
- Produces, in JawUI:
  - `public struct TerminalPanes: Equatable, Sendable`, with `public static let interface = "jaw"`, `public let grid: PaneGrid`, `public let focus: Int` (a slot's id), `public let said: String` and `public static func read(_ kept: KeptLayout) -> Result<TerminalPanes, Refusal>`.
  - `public enum TerminalPanes.Refusal: Error, Equatable, Sendable { case nothing, gone(Int), most(Int), unreadable }`, with `public var said: String`.
  - `extension PaneLayout { public init(_ tree: SplitTree) }`.
- `dropped` holds both a pane whose chat is gone and one showing a chat a pane before it shows (a person's phone JID and LID resolving to one chat), so what's said of the panes jawd left out calls them that, left out. When every pane is left out, every chat is gone, since the first pane showing a chat always stays.

- [ ] **Step 1: Write the failing tests**

`mac/Tests/JawUITests/TerminalPanesTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The terminal's panes read as a grid: its splits as the grid's lines, its panes as slots numbered
/// in the tree's order, its pane with the keys as the slot with the focus, and what can't open said;
/// and a slot whose chat won't have a pane saying why.

let protocolFixtures = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .appending(path: "protocol/fixtures")

/// A fixture's answer, as `{"id": …, "result": …}`.
struct Answered<R: Decodable>: Decodable {
    let result: R
}

@Test func `the fixture's two panes open side by side, the keys on the second, and the pane jawd left out is said`() throws {
    let data = try Data(contentsOf: protocolFixtures.appending(path: "layout.get.result.json"))
    let kept = try JSONDecoder().decode(Answered<KeptLayout>.self, from: data).result
    let terminal = try TerminalPanes.read(kept).get()
    #expect(terminal.grid == PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "351912345678@s.whatsapp.net"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "120363012345678901@g.us"),
    ]))
    #expect(terminal.focus == 2, "the tree counts its panes from 0, the grid its slots from 1")
    #expect(terminal.said == "the terminal's 2 panes, and 1 more that jawd left out")
}

@Test func `a split inside a split opens as one pane beside two, each where the tree draws it`() throws {
    let tree = SplitTree.split(.across, first: 500, .chat("ana"), .split(.down, first: 600, .chat("climbing"), .chat("rui")))
    let terminal = try TerminalPanes.read(KeptLayout(interface: "jaw", v: 1, layout: SplitLayout(focused: 2, tree: tree))).get()
    #expect(terminal.grid.cols == [0, 0.5, 1] && terminal.grid.rows == [0, 0.6, 1] && terminal.grid.holds)
    #expect(terminal.grid.reading.map(\.chat) == ["ana", "climbing", "rui"])
    #expect(span(terminal.grid.slot(1)) == [0, 1, 0, 2], "ana takes both rows")
    #expect(terminal.focus == 3 && terminal.grid.slot(3)?.chat == "rui")
    #expect(terminal.said == "the terminal's 3 panes")
}

@Test func `what can't open says why: nothing kept, every pane's chat gone, more panes than the grid holds, or no grid`() {
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 0)) == .failure(.nothing))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, dropped: ["351911111111@s.whatsapp.net", "351922222222@s.whatsapp.net"]))
        == .failure(.gone(2)))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5)) == .failure(.unreadable), "kept in a format this Jaw doesn't read")
    var many = SplitTree.chat("0")
    for i in 1 ... PaneGrid.most {
        many = .split(.across, first: 500, many, .chat("\(i)"))
    }
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: many))) == .failure(.most(13)))
    let twice = SplitTree.split(.down, first: 500, .chat("ana"), .chat("ana"))
    #expect(TerminalPanes.read(KeptLayout(interface: "jaw", v: 5, layout: SplitLayout(focused: 0, tree: twice))) == .failure(.unreadable))
    #expect(TerminalPanes.Refusal.nothing.said == "the terminal hasn't kept any panes")
    #expect(TerminalPanes.Refusal.gone(1).said == "the terminal's pane showed a chat that's gone")
    #expect(TerminalPanes.Refusal.gone(2).said == "the terminal's 2 panes showed chats that are gone")
    #expect(TerminalPanes.Refusal.most(13).said == "the terminal has 13 panes, and Jaw shows 12 at most")
    #expect(TerminalPanes.Refusal.unreadable.said == "the terminal's panes don't make a layout this Jaw can show")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with the build failing on `cannot find 'TerminalPanes' in scope`.

- [ ] **Step 3: Read the tree as a grid**

`mac/JawUI/Sources/Panes/TerminalPanes.swift`:

```swift
import JawKit

/// The terminal's panes as jawd keeps them for it, read as a grid to open in place of the unsaved
/// panes: its tree's splits as the grid's lines, its panes as slots numbered 1, 2, … in the tree's
/// order, and its pane with the keys as the slot with the focus. The terminal makes six panes at
/// most and the grid holds twelve, so a tree with more was kept by something else under its name.
public struct TerminalPanes: Equatable, Sendable {
    /// The terminal's name in hello, which jawd keeps its panes under.
    public static let interface = "jaw"

    public let grid: PaneGrid
    public let focus: Int
    /// What's said once they're open: how many, and how many more jawd left out.
    public let said: String

    /// Why the terminal's panes don't open, as it's said.
    public enum Refusal: Error, Equatable, Sendable {
        /// The terminal hasn't kept any.
        case nothing
        /// Every pane's chat is gone, this many.
        case gone(Int)
        /// More panes than the grid holds.
        case most(Int)
        /// Kept in a format this Jaw doesn't read, or a tree that makes no grid.
        case unreadable

        public var said: String {
            switch self {
            case .nothing: "the terminal hasn't kept any panes"
            case let .gone(n): n == 1 ? "the terminal's pane showed a chat that's gone" : "the terminal's \(n) panes showed chats that are gone"
            case let .most(n): "the terminal has \(n) panes, and Jaw shows \(PaneGrid.most) at most"
            case .unreadable: "the terminal's panes don't make a layout this Jaw can show"
            }
        }
    }

    /// The panes `kept` holds, or why they don't open.
    public static func read(_ kept: KeptLayout) -> Result<TerminalPanes, Refusal> {
        guard kept.v > 0 else { return .failure(.nothing) }
        guard let layout = kept.layout else { return .failure(kept.dropped.isEmpty ? .unreadable : .gone(kept.dropped.count)) }
        let tree = PaneLayout(layout.tree)
        let count = tree.panes.count
        guard count <= PaneGrid.most else { return .failure(.most(count)) }
        let grid = PaneGrid.from(tree)
        guard grid.holds else { return .failure(.unreadable) }
        let focus = min(max(layout.focused, 0), count - 1) + 1
        return .success(TerminalPanes(grid: grid, focus: focus, said: saying(count, left: kept.dropped.count)))
    }

    static func saying(_ count: Int, left: Int) -> String {
        let panes = count == 1 ? "the terminal's pane" : "the terminal's \(count) panes"
        return left == 0 ? panes : "\(panes), and \(left) more that jawd left out"
    }
}

extension PaneLayout {
    /// A tree of splits as jawd keeps it, each split's share turned from thousandths to a fraction.
    public init(_ tree: SplitTree) {
        switch tree {
        case let .chat(id):
            self = .pane(id)
        case let .split(how, first, a, b):
            self = .split(how == .across ? .across : .down, Double(first) / 1000, PaneLayout(a), PaneLayout(b))
        }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 0, with three more tests than before.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources/Panes/TerminalPanes.swift mac/Tests/JawUITests/TerminalPanesTests.swift
git commit -F- <<'EOF'
The terminal's panes read as a grid, TerminalPanes in JawUI: its tree's splits become the grid's lines, its panes slots numbered 1, 2, … in the tree's order and its pane with the keys the slot with the focus, through PaneGrid.from, with what's said once they open, how many and how many more jawd left out; nothing kept, every pane's chat gone, more than 12 panes and a tree that makes no grid are refused, each in words; tests on the protocol's fixture, on three panes, and on each refusal
EOF
```

---

### Task 4: A slot whose chat won't have a pane says why

The grid layouts design's "When things go wrong" has a gone chat's slot say the chat isn't there any more, and a slot whose window jawd refuses say so. Both come as words for the slot, by its chat, carried with the grid in `WindowContent.Panes`, so the harness can draw such a slot from values as the app does.

**Files:**
- Modify: `mac/JawUI/Sources/Panes/GridView.swift` (the class's comment at 3-7, its statics at 55, `show` at 76-94, `EmptyPane` at 289-316); `mac/JawUI/Sources/Window/DetailController.swift` (mac-harness's: `drawn` at 35, `show(panes:…)` at 37-52); `mac/JawUI/Sources/Window/WindowContent.swift` (mac-harness's: `Panes` at 27-44, `show(_:roles:)` at 177-180)
- Test: `mac/Tests/JawUITests/TerminalPanesTests.swift`

**Interfaces:**
- Consumes: `content(width:)` from `mac/Tests/JawUITests/WindowContentTests.swift` (mac-harness's).
- Produces, in JawUI:
  - `GridView.show(_ grid: PaneGrid, views: [Int: NSView], unopened: [String: String] = [:], focused: Int?, roles: Roles, look: PaneLook = PaneLook())`. A slot whose chat has no view and has words in `unopened` says them in place of "⌘K or drag a chat here", cut short within its well, and VoiceOver hears "An empty pane, ⟨the words⟩: ⌘K or drag a chat here".
  - `public static let GridView.gone = "this chat isn't here any more"`, what a slot says whose chat jawd has none of.
  - `DetailController.show(panes:views:unopened:focused:box:roles:look:force:)`, with `unopened` defaulting to `[:]`, drawn again when only the words change.
  - `WindowContent.Panes.unopened: [String: String]` and `init(grid:views:unopened:focused:box:look:)`, `unopened` defaulting to `[:]`, so the harness's `Screens.swift` and the existing tests build as they are.
- The layout isn't changed or marked edited by any of it, as the grid layouts design has a gone chat's slot.

- [ ] **Step 1: Write the failing tests**

Add these at the end of `mac/Tests/JawUITests/TerminalPanesTests.swift`:

```swift
@MainActor
@Test func `a slot whose chat won't have a pane says why, that it's gone or jawd's words, and one waiting says how to give it one`() throws {
    let (two, second) = try split(.single("ana"), 1, .across, "old")
    let (grid, third) = try split(two, second, .down, "busy")
    let refused = "a connection can watch at most 32 windows at once, so one has to be unwatched first"
    let unopened = ["old": GridView.gone, "busy": refused]
    let roles = Catalog.jaw.roles(dark: true)
    let view = GridView(frame: NSRect(x: 0, y: 0, width: 1000, height: 600))
    view.show(grid, views: [:], unopened: unopened, focused: 1, roles: roles)
    #expect(view.frames[second]?.empty.prompt.stringValue == "this chat isn't here any more")
    #expect(view.frames[second]?.empty.accessibilityLabel() == "An empty pane, this chat isn't here any more: ⌘K or drag a chat here")
    #expect(view.frames[third]?.empty.prompt.stringValue == refused, "jawd's words for the window it refused")
    #expect(view.frames[1]?.empty.prompt.stringValue == "⌘K or drag a chat here", "ana's pane only waits for its row")
    view.show(grid.with(chat: "mum", in: second), views: [:], unopened: unopened, focused: second, roles: roles)
    #expect(view.frames[second]?.empty.prompt.stringValue == "⌘K or drag a chat here", "another chat put in its place says nothing of the one before")
}

@MainActor
@Test func `the words for a slot whose chat won't have a pane travel in the window's panes, and are drawn again when they change`() {
    let grid = PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "ana"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "old"),
    ])
    let pane = NSView()
    let c = content(width: 1180)
    c.show(WindowContent.Panes(grid: grid, views: [1: pane], unopened: ["old": GridView.gone], focused: 1, box: nil, look: PaneLook()),
           roles: Catalog.jaw.dark)
    #expect(c.detail.panes.frames[2]?.empty.prompt.stringValue == "this chat isn't here any more")
    c.show(WindowContent.Panes(grid: grid, views: [1: pane], focused: 1, box: nil, look: PaneLook()), roles: Catalog.jaw.dark)
    #expect(c.detail.panes.frames[2]?.empty.prompt.stringValue == "⌘K or drag a chat here", "the same panes with no words for it")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with the build failing on `extra argument 'unopened' in call` and `type 'GridView' has no member 'gone'`.

- [ ] **Step 3: The grid says why**

In `mac/JawUI/Sources/Panes/GridView.swift`, make four changes.

The class's doc comment becomes:

```swift
/// The panes drawn: each slot of the grid in its place, set by hand in `layout()` so the panes
/// follow a live resize in the same frame, with a hairline gutter between them that's dragged as a
/// whole line. The focused pane has the accent's outline when there's more than one; an empty pane
/// waits for a chat, or says why its chat has none. Slots keep their views from one change to the
/// next, so nothing is rebuilt.
```

After `static let grip: CGFloat = 4`, add:

```swift
    /// What a slot says whose chat jawd has none of.
    public static let gone = "this chat isn't here any more"
```

`show`'s doc comment and signature become the ones below, and in its loop, the line before `frame.show(…)` sets the empty pane's words:

```swift
    /// Shows `grid`, each slot holding its chat's view, or the empty pane, which says what `unopened`
    /// says for its chat when it says anything there, the focused one as `look` says.
    public func show(_ grid: PaneGrid, views: [Int: NSView], unopened: [String: String] = [:], focused: Int?, roles: Roles,
                     look: PaneLook = PaneLook())
    {
```

```swift
            frame.empty.says = slot.chat.flatMap { unopened[$0] }
            frame.show(views[slot.id], roles: roles, focused: slot.id == focused, many: many, look: look)
```

`EmptyPane`, from its doc comment to the end of `init(frame:)`, becomes the version below. Its prompt is kept within the well and cut short there, since jawd's words can be longer than a 240-point slot is wide; the waiting words fit as before, so a slot that says nothing more is drawn as it was:

```swift
/// A pane with no chat's view: it says how to give it a chat, or why its chat has none, and clicking
/// it gives it the keys.
@MainActor
final class EmptyPane: NSView {
    static let waitingWords = "⌘K or drag a chat here"
    let well = Surface(.well, radius: Radius.field)
    let prompt = Labels.make(EmptyPane.waitingWords, .callout)
    var picked: (() -> Void)?
    var roles: Roles? {
        didSet { colour() }
    }

    /// Why its chat has no view, said in place of how to give it one: that the chat is gone, or
    /// jawd's words for refusing it; nil while it waits.
    var says: String? {
        didSet {
            guard says != oldValue else { return }
            prompt.stringValue = says ?? Self.waitingWords
            setAccessibilityLabel(says.map { "An empty pane, \($0): \(Self.waitingWords)" } ?? "An empty pane: \(Self.waitingWords)")
        }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        for v in [well, prompt] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        prompt.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        NSLayoutConstraint.activate([
            well.topAnchor.constraint(equalTo: topAnchor, constant: Space.l),
            well.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.l),
            well.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.l),
            well.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Space.l),
            prompt.centerXAnchor.constraint(equalTo: well.centerXAnchor),
            prompt.centerYAnchor.constraint(equalTo: well.centerYAnchor),
            prompt.leadingAnchor.constraint(greaterThanOrEqualTo: well.leadingAnchor, constant: Space.m),
            prompt.trailingAnchor.constraint(lessThanOrEqualTo: well.trailingAnchor, constant: -Space.m),
        ])
        setAccessibilityRole(.group)
        setAccessibilityLabel("An empty pane: \(Self.waitingWords)")
    }
```

- [ ] **Step 4: The window's panes carry the words**

In `mac/JawUI/Sources/Window/DetailController.swift`, `drawn` and `show(panes:…)` become:

```swift
    var drawn: (PaneGrid, Int?, [Int: ObjectIdentifier], ObjectIdentifier?, Roles, PaneLook, [String: String])?

    /// Lays the panes out with the focused chat's box under them, unless they're already so, with what
    /// `unopened` says for a slot whose chat won't have a pane.
    public func show(panes grid: PaneGrid, views: [Int: NSView], unopened: [String: String] = [:], focused: Int?,
                     box composer: NSView?, roles: Roles, look: PaneLook, force: Bool = false)
    {
        let ids = views.mapValues(ObjectIdentifier.init)
        let boxID = composer.map(ObjectIdentifier.init)
        if !force, shown === panesArea, let drawn, drawn.0 == grid, drawn.1 == focused, drawn.2 == ids, drawn.3 == boxID, drawn.4 == roles,
           drawn.5 == look, drawn.6 == unopened
        {
            return
        }
        place(panesArea)
        panes.show(grid, views: views, unopened: unopened, focused: focused, roles: roles, look: look)
        box.show(composer, roles: roles)
        drawn = (grid, focused, ids, boxID, roles, look, unopened)
    }
```

In `mac/JawUI/Sources/Window/WindowContent.swift`, `Panes` and `show(_:roles:)` become:

```swift
    /// The panes as the window shows them: the layout's grid, each slot's view (none yet for a chat the
    /// list doesn't hold), what a slot says whose chat won't have one, by that chat, the slot with the
    /// keys, the focused chat's box under them, and how the panes say which has the keys.
    public struct Panes {
        public var grid: PaneGrid
        public var views: [Int: NSView]
        public var unopened: [String: String]
        public var focused: Int?
        public var box: NSView?
        public var look: PaneLook

        public init(grid: PaneGrid, views: [Int: NSView], unopened: [String: String] = [:], focused: Int?, box: NSView?,
                    look: PaneLook)
        {
            self.grid = grid
            self.views = views
            self.unopened = unopened
            self.focused = focused
            self.box = box
            self.look = look
        }
    }
```

```swift
    /// Lays the panes out in the panes' place, with the focused chat's box under them.
    public func show(_ panes: Panes, roles: Roles) {
        detail.show(panes: panes.grid, views: panes.views, unopened: panes.unopened, focused: panes.focused, box: panes.box,
                    roles: roles, look: panes.look)
    }
```

- [ ] **Step 5: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac test JawHarnessTests` and `mac/scripts/build-mac build`.
Expected: each exits 0, and JawUITests passes with two more tests. The harness's scenes with empty panes draw as they did, since a slot with no words says what it said before.

- [ ] **Step 6: Commit**

```bash
git add mac/JawUI/Sources/Panes/GridView.swift mac/JawUI/Sources/Window/DetailController.swift mac/JawUI/Sources/Window/WindowContent.swift \
    mac/Tests/JawUITests/TerminalPanesTests.swift
git commit -F- <<'EOF'
A slot whose chat won't have a pane says why where its chat would be, "this chat isn't here any more" for a chat jawd has none of and jawd's own words for a window it refused, as the grid layouts design has both, without changing the layout or marking it edited, while one waiting for its chat still says how to give it one, and VoiceOver hears which; the words travel by chat in WindowContent.Panes, through DetailController to GridView, so the harness can draw such a slot from values, and the prompt is cut short within its well; tests: the grid says each, and the window's panes carry them and draw again when only they change
EOF
```

---

### Task 5: Open the Terminal's Panes

Two things are worth knowing before the code. First, a looked-up row isn't kept current: the pane's header shows the chat as jawd found it, as the terminal shows a chat found outside its list "as it was found" (`crates/jaw/src/view/palette.rs`), until the chat comes into the chat list, whose upserts reach every pane. Second, `chats.watch` doesn't follow merged JIDs, so a pane the app's preferences kept under a person's other JID says its chat is gone until P10 has jawd keep the app's layouts.

**Files:**
- Modify: `mac/JawApp/Sources/AppModel.swift` (`unopened` after `restored` at 42; `apply(_:)` at 200-203; `openTerminalPanes()`, `terminalPanes(_:)` and `lookedUp(_:_:)` after `deleteLayout(_:)` at 583; `reconcile(focus:)` at 589-609); `mac/JawApp/Sources/MainWindowController.swift` (mac-harness's `panesShown`, at 114-123); `mac/JawApp/Sources/Commands.swift:161-162` (after Delete Layout…); `mac/JawApp/Sources/Bench.swift:94-109` (`press`)

**Interfaces:**
- Consumes: `Jawd.layout(of:what:)` and `JawUpdate.layout(KeptLayout)` (Task 1); `Jawd.lookUp(_:)`, `Looked` and `JawUpdate.looked(chat:_:)` (Task 2); `TerminalPanes.read(_:)`, `TerminalPanes.interface`, `.grid`, `.focus`, `.said` and `Refusal.said` (Task 3); `WindowContent.Panes(grid:views:unopened:focused:box:look:)` and `GridView.gone` (Task 4).
- Produces: the command `layout.terminal`, "Open the Terminal's Panes", in the Layouts menu and the palette; `AppModel.openTerminalPanes()`, `terminalPanes(_ kept: KeptLayout)`, `lookedUp(_ chat: String, _ found: Looked)` and `unopened: [String: String]`; and the snapshot's `--keys` step `terminal-panes`.

- [ ] **Step 1: See the step missing**

With jawd copied (see the Global Constraints), run:

```sh
mkdir -p mac/build/panes
mac/scripts/build-mac snapshot mac/build/panes/before.png --keys terminal-panes
```

Expected: a line `terminal-panes: 1:351912@0-1x0-1 · focus 1 · layout unsaved · panes …` with no `said`. The step isn't one yet, so its name was typed into the box as keys.

- [ ] **Step 2: The command**

In `mac/JawApp/Sources/Commands.swift`, add this after the `layout.delete` command, so it sits in a group of its own at the end of the Layouts menu. It has no key, so no two commands share one, and its detail says it takes the unsaved panes' place:

```swift
        add(Command("layout.terminal", "Open the Terminal's Panes", phrase: "Open the terminal's panes",
                    detail: "as the terminal left them, in place of the unsaved panes", symbol: "terminal", place: .layouts, group: 3)
            { [weak self] in self?.openTerminalPanes() })
```

- [ ] **Step 3: The model opens them and looks up their chats**

In `mac/JawApp/Sources/AppModel.swift`, make four changes.

After `var restored = false`, add:

```swift
    /// What a slot of the layout on screen says whose chat won't have a pane, by that chat: that it's
    /// gone, or jawd's words for refusing to look it up.
    var unopened: [String: String] = [:]
```

In `apply(_:)`, the switch's end, from the `.windowFailed` case on, becomes:

```swift
        case let .windowFailed(_, why):
            say("a window couldn't be watched: \(why)", warn: true)
        case let .layout(kept) where kept.interface == TerminalPanes.interface:
            terminalPanes(kept)
        case let .looked(chat, found):
            lookedUp(chat, found)
        case .push, .chats, .layout:
            break
        }
```

After `deleteLayout(_:)`, add the three functions below. `terminalPanes(_:)` leaves the layout that was on screen as it was, as switching does, but it doesn't go back to the unsaved panes: it replaces what they held.

```swift
    /// Asks jawd for the terminal's panes, which open as the unsaved panes once it answers.
    func openTerminalPanes() {
        jawd.layout(of: TerminalPanes.interface, what: "opening the terminal's panes")
    }

    /// The terminal's panes as jawd keeps them for it, read as a grid in place of what the unsaved
    /// panes held, its pane with the keys taking them; the layout on screen is left as switching
    /// leaves it, and what can't open is said.
    func terminalPanes(_ kept: KeptLayout) {
        switch TerminalPanes.read(kept) {
        case let .failure(refusal):
            say(refusal.said, warn: refusal != .nothing)
        case let .success(terminal):
            book.focus = focusedSlot
            conversation?.leave()
            book.switchTo(nil)
            book.update(terminal.grid)
            reconcile(focus: terminal.focus)
            say(terminal.said)
        }
    }

    /// A chat the chat list didn't hold, as jawd found it: its pane made where the layout on screen
    /// shows it, taking the keys if its slot has them, or its slot saying why it won't have one.
    func lookedUp(_ chat: String, _ found: Looked) {
        guard let slot = grid?.slot(showing: chat), panes[chat] == nil else { return }
        switch found {
        case let .row(row):
            _ = makePane(row, place: ConversationController.place(for: row))
            if focusedSlot == slot.id {
                focus(slot: slot.id, force: true)
                return
            }
        case .gone:
            unopened[chat] = GridView.gone
        case let .refused(why):
            unopened[chat] = why
            say("a pane's chat couldn't be looked up: \(why)", warn: true)
        }
        window?.showPanes()
    }
```

`reconcile(focus:)`, with its doc comment, becomes the version below. It looks up the focused slot's chat first, so the pane with the keys fills first. It runs on every switch, Revert, the editor's Save and the launch's restore, so the app's own layouts get their panes this way too, where a chat the chat list didn't hold used to leave its pane waiting for good:

```swift
    /// The panes for the layout on screen: windows let go for chats it doesn't show, opened for
    /// the ones it does; a chat the list doesn't hold is looked up, the focused slot's first, its
    /// pane made once jawd finds it (`lookedUp`), and a slot whose chat won't have one says why.
    func reconcile(focus at: Int?) {
        let shown = Set(grid?.chats ?? [])
        for chat in panes.keys where !shown.contains(chat) {
            closePane(chat)
        }
        let reading = grid?.reading ?? []
        var outside: [String] = []
        for slot in reading.filter({ $0.id == at }) + reading.filter({ $0.id != at }) {
            guard let id = slot.chat, panes[id] == nil else { continue }
            if let row = window?.chatList.row(id) {
                _ = makePane(row, place: ConversationController.place(for: row))
            } else {
                outside.append(id)
            }
        }
        unopened = [:]
        jawd.lookUp(outside)
        focusedSlot = nil
        if let grid {
            focus(slot: at.flatMap { grid.slot($0)?.id } ?? grid.reading.first!.id, force: true)
        } else {
            window?.refresh()
        }
        savePanes()
        window?.layoutsChanged()
    }
```

- [ ] **Step 4: The window's panes carry the model's words**

In `mac/JawApp/Sources/MainWindowController.swift`, `panesShown` becomes:

```swift
    /// The panes as the model's layout has them, the focused one outlined, or nil with no layout.
    var panesShown: WindowContent.Panes? {
        guard let grid = model.grid else { return nil }
        var views: [Int: NSView] = [:]
        for slot in grid.slots {
            if let chat = slot.chat, let c = model.panes[chat] { views[slot.id] = c.view }
        }
        return WindowContent.Panes(grid: grid, views: views, unopened: model.unopened, focused: model.focusedSlot,
                                   box: model.conversation?.composer.view, look: model.preferences.paneLook(model.roles))
    }
```

- [ ] **Step 5: The snapshot's step**

In `mac/JawApp/Sources/Bench.swift`, in `press(_:model:then:)`, `step` becomes:

```swift
            let step = key.hasPrefix("save:") || key == "revert" || key == "settings" || key.hasPrefix("size:") || key.hasPrefix("drop:")
                || key.hasPrefix("menu:") || key == "terminal-panes"
```

Then add this after `if key == "settings" { model.window?.showSettings() }`:

```swift
            if key == "terminal-panes" { model.openTerminalPanes() }
```

- [ ] **Step 6: Build and test**

Run: `mac/scripts/build-mac build`, then `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests` and `mac/scripts/build-mac test JawKit`.
Expected: each exits 0. A debug build stops at launch on two commands with one key, and the snapshots below launch one.

- [ ] **Step 7: Nothing kept for the terminal**

Run: `mac/scripts/build-mac snapshot mac/build/panes/nothing.png --keys terminal-panes`
Expected: `terminal-panes: 1:351912@0-1x0-1 · focus 1 · layout unsaved · said "the terminal hasn't kept any panes" · panes …`. A fresh demo folder keeps no terminal layout, so jawd answers `v` 0: this shows only that an answer came back and was read. `ClientTests` pins the request's params.

- [ ] **Step 8: The live check, on panes the terminal kept**

This check runs on the build Mac against `jawd demo`, on an account of 150 chats with every twentieth archived, so `all` has more than the chat list's 120. A connection that says hello as `jaw` keeps the terminal's panes on it, as jaw keeps them: the chat list's first chat, its last chat past row 120, and an archived chat, with the keys on the archived one. The app opens them, saves them as a layout of its own, closes the archived chat's pane and reverts, which makes `reconcile` look that chat up again.

Write `mac/build/panes/try-terminal-panes.sh`:

```sh
#!/bin/sh
# The live check of Open the Terminal's Panes, on the build Mac: jawd demo with chats past the chat
# list's 120 and archived ones, the terminal's panes kept on it as jaw keeps them, and the app
# opening them, saving them, closing the archived chat's pane and reverting. One folder for all of
# it, removed on the way out, a dropped ssh session included.
set -eu
command -v python3 > /dev/null || { echo "the build Mac has no python3" >&2; exit 1; }
cd ~/src/build-mac/jaw/jaw-panes/mac
home=$(mktemp -d "${TMPDIR:?}/jaw-panes.XXXXXX")
demo=""
cleanup() {
    if [ -n "$demo" ]; then
        kill "$demo" 2> /dev/null || true
        wait "$demo" 2> /dev/null || true
    fi
    rm -rf "$home"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
../target/debug/jawd demo --folder "$home" --chats 150 --people 200 --messages 3000 --blob 500 --first-run --catch-up 20 \
    > "$home/demo.log" 2>&1 &
demo=$!
tries=0
until grep -q 'serving until Ctrl-C' "$home/demo.log"; do
    tries=$((tries + 1))
    if [ "$tries" -gt 600 ] || ! kill -0 "$demo" 2> /dev/null; then
        tail -5 "$home/demo.log" >&2
        exit 1
    fi
    sleep 0.2
done
python3 - "$home/jaw.sock" << 'PY'
import json, socket, sys

connection = socket.socket(socket.AF_UNIX)
connection.connect(sys.argv[1])
lines = connection.makefile("rw")
asked = 0

def ask(method, params):
    global asked
    asked += 1
    lines.write(json.dumps({"id": asked, "method": method, "params": params}) + "\n")
    lines.flush()
    while True:
        line = json.loads(lines.readline())
        if line.get("id") == asked:
            if "error" in line:
                sys.exit(f"{method}: {line['error']['message']}")
            return line["result"]

def rows(folder):
    return ask("chats.watch", {"folder": folder, "around": "top", "before": 0, "after": 1000})["rows"]

def chats(listed):
    return [row["id"] for row in listed if row["kind"] in ("direct", "group")]

ask("hello", {"protocol": 1, "client": "jaw", "version": "0.1.0", "focus": False})
listed, archived = rows("all"), rows("archived")
names = {row["id"]: row["name"] for row in listed + archived}
places = {row["id"]: n for n, row in enumerate(listed, 1)}
first, past = chats(listed)[0], chats(listed)[-1]
if places[past] <= 120 or not chats(archived):
    sys.exit(f"the last chat is row {places[past]} of all, and archived holds {len(chats(archived))}: nothing to look up")
filed = chats(archived)[0]
tree = {"split": {"how": "across", "first": 500, "a": {"chat": first},
                  "b": {"split": {"how": "down", "first": 500, "a": {"chat": past}, "b": {"chat": filed}}}}}
ask("layout.set", {"v": 1, "layout": {"format": 1, "focused": 2, "tree": tree}})
print(f"slot 1, on the left: {names[first]} ({first}), row {places[first]} of all")
print(f"slot 2, top right: {names[past]} ({past}), row {places[past]} of all")
print(f"slot 3, bottom right: {names[filed]} ({filed}), archived")
print(f"so terminal-panes should begin: 1:{first[:6]}@0-1x0-2 2:{past[:6]}@1-2x0-1 3:{filed[:6]}@1-2x1-2 · focus 3")
PY
build/DerivedData/Build/Products/Debug/Jaw.app/Contents/MacOS/Jaw --home "$home" --snapshot build/terminal-panes.png \
    --keys terminal-panes,save:Terminal,cmd+w,revert < /dev/null > build/terminal-panes.log 2>&1 || true
grep -E '^(terminal-panes|save:Terminal|cmd\+w|revert):|^\{' build/terminal-panes.log
```

Run it on the build Mac after Step 6's build, which copied the checkout and built the app there. Then copy its picture back:

```sh
ssh -o BatchMode=yes build-mac 'sh -s' < mac/build/panes/try-terminal-panes.sh
scp -q -o BatchMode=yes build-mac:src/build-mac/jaw/jaw-panes/mac/build/terminal-panes.png mac/build/panes/terminal-panes.png
```

Expected, with each chat's name and JID in place of the angle brackets. A panes line says the latest thing the app said, however long ago, so the last two still say "saved as Terminal":

```
slot 1, on the left: <name> (<JID>), row <n> of all
slot 2, top right: <name> (<JID>), row <past 120> of all
slot 3, bottom right: <name> (<JID>), archived
so terminal-panes should begin: 1:<six>@0-1x0-2 2:<six>@1-2x0-1 3:<six>@1-2x1-2 · focus 3
terminal-panes: 1:<six>@0-1x0-2 2:<six>@1-2x0-1 3:<six>@1-2x1-2 · focus 3 · layout unsaved · said "the terminal's 3 panes" · panes …×… …×… …×…
save:Terminal: 1:<six>@0-1x0-2 2:<six>@1-2x0-1 3:<six>@1-2x1-2 · focus 3 · layout Terminal · said "saved as Terminal" · panes …×… …×… …×…
cmd+w: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 · focus 2 · layout Terminal • · said "saved as Terminal" · panes …×… …×…
revert: 1:<six>@0-1x0-2 2:<six>@1-2x0-1 3:<six>@1-2x1-2 · focus 2 · layout Terminal · said "saved as Terminal" · panes …×… …×… …×…
{"panes":3.0,"snapshot":1.0}
```

How to read it:

- **Slots.** Each slot in a panes line is its id, the first six characters of its chat's JID, and the lines it spans, columns then rows. jawd demo's JIDs share those six characters by kind, `351900` for a person and `120363` for a group, so a panes line shows each slot's place and kind. The `terminal-panes` line must begin as the Python's last line does. The Python's first three lines say which chat each slot was given, by name, and the picture's headers show those names in those places.
- **`terminal-panes`.** Three pane sizes mean all three chats got their rows: the second from `all` past the chat list's 120, and the third from `archived`, both through the lookups. `focus 3` is the terminal's focused pane.
- **`save:Terminal`, `cmd+w`, `revert`.** `save:Terminal` keeps the panes as a layout of the app's own. `cmd+w` closes the archived chat's pane, which had the keys, so the layout reads `Terminal •`, edited, with two panes and the keys on the second. `revert` brings back the saved shape, and its third pane size means `reconcile` looked the archived chat up again and opened its pane: a layout of the app's own filling a pane whose chat the chat list doesn't hold.

Look at `mac/build/panes/terminal-panes.png` with the Read tool. It shows the `Terminal` layout after the revert: three panes, each with its chat's name in its header and its messages, and the top-right one has the keys. If the build Mac has no `python3`, the script says so; stop and say so in the report rather than running anything here. The script removes its folder and stops its jawd demo on its way out, on HUP, INT and TERM too, so a dropped ssh session leaves nothing running.

- [ ] **Step 9: Commit**

```bash
git add mac/JawApp/Sources/AppModel.swift mac/JawApp/Sources/MainWindowController.swift mac/JawApp/Sources/Commands.swift \
    mac/JawApp/Sources/Bench.swift
git commit -F- <<'EOF'
Open the Terminal's Panes, in the Layouts menu and the palette, opens the panes jawd keeps for the terminal in place of the unsaved panes with its focused pane taking the keys, as parity step 1.1 has it; a pane whose chat the chat list doesn't hold, archived or past its first 120, gets its row from jawd's lookup, the focused slot's first, rather than waiting for good, so the app's own layouts, restored, switched to or reverted, fill every pane too, and a slot whose chat won't have one says why, gone or jawd's words, from words the window's panes carry; the snapshot's terminal-panes step opens them, and on jawd demo, with the terminal's panes kept on a chat past the list's 120 and an archived one, all three opened, and a revert after closing the archived chat's pane opened it again
EOF
```

---

### Task 6: The step's first half in the docs

**Files:**
- Modify: `mac/README.md`, `docs/plans/2026-10-27-jaw-mac-parity-plan.md`

**Interfaces:**
- Consumes: everything above.
- Produces: the README and the parity plan saying what Part A does, and where Part B stands.

- [ ] **Step 1: The README**

In `mac/README.md`, make two changes.

In the paragraph on the app's options, add `terminal-panes` to the `--keys` steps, after `settings`, as `` `terminal-panes` (Open the Terminal's Panes), ``.

At the end of the "Panes and layouts" bullet, add:

```markdown
Open the Terminal's Panes (the Layouts menu and the palette) reads the panes jawd keeps for the terminal, `layout.get` for `jaw`, as a grid in place of the unsaved panes (`TerminalPanes`). A pane whose chat the chat list doesn't hold, in any layout, gets its row from jawd through a window of one row around the chat, in `all` and then `archived`, one at a time and the focused slot's first (`Jawd.lookUp`), and a slot whose chat won't have a pane says why, that it's gone or jawd's words for refusing the lookup, from words the window's panes carry (`WindowContent.Panes.unopened`).
```

- [ ] **Step 2: The parity plan**

In `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, step 1.1, this bullet:

```markdown
- **Now:** Open the Terminal's Panes, in the Layouts menu and the palette: `layout.get` with `interface` `jaw`, the tree read as a grid into the unsaved panes. This is the other interface's layout, which neither interface opens yet (Owner, 26 Oct, "each able to open the other's").
```

becomes:

```markdown
- **Now, done** ([its plan](2026-10-28-jaw-mac-panes-kept-by-jawd-plan.md)): Open the Terminal's Panes, in the Layouts menu and the palette: `layout.get` with `interface` `jaw`, the tree read as a grid into the unsaved panes, its focused pane taking the keys. This is the other interface's layout, which the terminal doesn't open yet (Owner, 26 Oct, "each able to open the other's"). A pane whose chat the chat list doesn't hold, archived or past its first 120, gets its row from a window of one row around the chat, in `all` and then `archived`, so no pane waits for good, the app's own layouts' included: it opens, or its slot says why, that the chat is gone or jawd's words for refusing the lookup.
```

At the end of the "With P10" bullet, add: `P10 is on main since 817bc02, its protocol text and fixtures with it; this half has a plan of its own, [the layout book plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md), built after the first half once the harness's second phase is on main.` Leave "Where it starts" as it is, since it says where the plan started.

- [ ] **Step 3: Run the check**

Run: `make check-parity`, then `python3 scripts/check-parity.py --table | grep -E '^r layout|^p layout '`.
Expected: the first exits 0, as it has since Task 1, its step 1.1 still reading `1.1 The panes kept by jawd: layout push, layout.set`: the check reads the parity plan's step headings, which stay as they are. The table shows `layout.get` used by both the terminal and the Mac with no step, and `layout.set` and the `layout` push used by the terminal alone, at step 1.1.

- [ ] **Step 4: Commit**

```bash
git add mac/README.md docs/plans/2026-10-27-jaw-mac-parity-plan.md
git commit -F- <<'EOF'
Parity step 1.1's first half is done: the parity plan says what Open the Terminal's Panes does and how a pane whose chat the chat list doesn't hold gets its row or says why it can't, and that the second half, with P10 on main since 817bc02, has a plan of its own, the layout book plan, built after the first once the harness's second phase is on main; the Mac README says how the terminal's tree is read, how a pane's chat is looked up and what a slot whose chat won't have a pane says; make check-parity passes, as it has since layout.get's row left mac/parity.toml
EOF
```

- [ ] **Step 5: Clean up and report**

Run: `rm -rf mac/build/panes`, then `make prune-builds`.
Report each task's runs, the pictures looked at (`before.png`, `nothing.png` and `terminal-panes.png`), the live check's lines, and what stays: the branch's copy on the build Mac (`~/src/build-mac/jaw/jaw-panes`, removed once the branch has landed) and `target/debug/jawd` in the worktree, which goes with it.

---

## Part B, with P10: its own plan

The step's second half has no code here. P10 is on main since 817bc02, its audit approved (`docs/audits/2026-10-28-layout-book.md`): its "Layouts" section in both formats (d8fe2b6, with 1d378f5's change to a round trip between formats) and the books' fixtures, `protocol/fixtures/layout.*.book.json` (6f65045). Part B's own plan, [the layout book plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md), is built against those fixtures and `FakeJawd` after Part A, once the harness's second phase is on main, and tries it end to end on `jawd demo`; the owner tries it on his account. This is what that text settles, as this plan read it before P10 landed; where the layout book plan says otherwise, it's right:

- **Part A still works after P10.** `layout.get` takes an optional `format`, 1 or 2, and answers by default in the format the layout was kept in, so Part A's request, which names none, still gets the terminal's tree.
- **jawd reads the terminal's tree as a grid.** Asked for `format` 2 (`layout.get.request.book.json`), jawd lays the tree out as `PaneGrid.from` does and answers a book holding only its unnamed layout: that grid, its slots numbered in the tree's order, and the focused pane's slot as `unnamedFocus` (`layout.get.result.book.json`). Part B can ask for that, and keep `TerminalPanes` for an answer that's still a tree, from an older jawd that doesn't know `format`.
- **The book on the wire.** Format 2 is `LayoutBook` key for key, with `format` where the app keeps its own version, `v`. jawd refuses a key it doesn't know, so the app writes exactly those keys, and jawd answers the lines as the very numbers the app gave. `layout.set` takes a version newer than any the app has been told of, such as the time in milliseconds; its answer has `kept`, the book kept, in format 2, and `dropped`. The `layout` push comes in the format the layout was kept in. It's typed in `IncomingLine`, the first time JawKit's sources name it, and the app keeps the newest by `v`, whichever of a push and an answer comes first. Part A's `KeptLayout` and `SplitTree` stay as format 1's.
- **Keeping the book.** The app keeps its whole `LayoutBook` with `layout.set` once each change has settled, as the terminal keeps its tree once it has stayed for two seconds (`crates/jaw/src/view/panes/kept.rs`). A change is a chat put in a pane, a shape edited, saved or reverted, a layout made, renamed or deleted, or another one switched to. This takes the place of `savePanes()`'s writes to the preferences.
- **Reading it at launch.** The app asks `layout.get` for its own interface once hello is answered, in place of `restorePanes()`'s read of the preferences, and its panes are made as the chat list's rows and Part A's lookups come. jawd follows merged and aliased JIDs in every slot, which the preferences never did.
- **A slot jawd emptied.** jawd empties a slot whose chat is gone, or that shows a chat a slot before it in the same shape shows, keeping its place, its id and the focus, and lists the JID in `dropped`, once, without saying which slot held it. The app can tell only by comparing. After a `layout.set`, it compares the slots it sent with those in the answer, which keep their ids. At launch, `layout.get` gives it nothing to compare with, so a slot emptied then can't say "this chat isn't here any more" as Part A's slots do. It waits for a chat like any empty slot, and the status line can say how many chats `dropped` names. Part B's plan settles that, or proposes that jawd say which slot each dropped JID came from.
- **The `layout` push.** When another of the app's connections, such as a second Jaw on the same folder, keeps a newer book, this one moves to it, its panes reconciled as switching does.
- **The preferences, read once into jawd.** On the first launch with P10, when jawd keeps nothing for `jaw-mac` (`v` 0), the app keeps the book from its preferences with `layout.set` (`layouts:<folder>`, or the older `grid:` and `panes:` keys), then leaves the preferences where they are, so an earlier Jaw still opens them, as the grid layouts design reads mac-m1's panes once.
- **Limits.** P10's are 12 slots in each shape, which `PaneGrid.most` already is, and a book of ten layouts, as the owner chose on 28 October: the unnamed layout and the nine named that ⌃⌘1 to ⌃⌘9 reach. `LayoutBook.most` is 30 today, as the grid layouts design says ("at most 30 layouts"). Part B makes it 9 and changes the design to say ten. That plan says what becomes of a book the preferences kept with more than nine named layouts before, and none of them goes without the app saying so.
- **The rows.** `layout.set` and the `layout` push leave `mac/parity.toml` in its last task, with `make check-parity` passing. Then step 1.1's "Done when" holds whole: a relaunch opens the layouts the app was left with, kept by jawd, and the terminal's panes open in the Mac.

The Mac's half touches none of jaw's data paths. jawd's half follows "Data integrity" and is audited on its own branch.
