# Jaw for the Mac: folders, favourites and the roster, implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The terminal's folders, favourites and roster in the Mac app's sidebar and menus, as [the menus and folders design](2026-10-27-jaw-mac-menus-and-folders-design.md)'s build-order steps 4 and 5 have them, with the folder keys ⌥⇧↑ and ⌥⇧↓ that [the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 2.1 adds (on main: `git show origin/main:docs/plans/2026-10-27-jaw-mac-parity-plan.md`), built ahead of jawd's `folders` branch against its protocol text and fixtures, and tried end to end on `jawd demo` once `folders` lands.

**Architecture:** JawKit gains the tree's, the roster's and the folder requests' types, checked against the `folders` branch's twelve fixtures, which are copied into its tests until they're on main, and two window kinds: the tree (`folders.watch`, one window watched whole) and a roster (`roster.watch`, a window like the chat list's). JawUI holds everything drawn and decided, made from values and tested without the app: `FolderTree` (the tree as jawd keeps it, laid out by parent and position, with the nine favourite slots), `FolderMenu` (a chat's, someone's and a folder's folder items as `MenuEntry` values and the commands they name, each asking for a `FolderAct`), `RosterLines`, and the views (the sidebar's outline of the tree with its naming in place and its drops, and the roster under a divider). The app watches the two windows, applies what comes, turns each `FolderAct` into a request, and makes the menu bar's and the palette's folder commands from the same entries and commands.

**Tech Stack:** Swift 6 (language mode 6, warnings as errors), AppKit (`NSOutlineView` on the sidebar's glass, `NSTableView` for the roster, `NSSplitView` between them), Swift Testing, Tuist through mise; builds and tests on the build Mac through `mac/scripts/build-mac`.

## Questions

What the design leaves open, each with its options and the lean the plan builds; none blocks a task, and each is a small change in the one place named. Questions 5 to 7 were decided after the plan's review, as the Mac's conventions have them; the others are built as their leans.

1. **Move to when the chat is in this folder only through a subfolder.** The design greys Remove from then, and says nothing of Move to, which would file the chat in the other folder and take it out of one it isn't directly in, leaving it here through the subfolder. A: greyed with Remove from, since neither can take it out of this folder (lean); B: Move to takes it out of the subfolder that holds it here; C: offered, and the chat stays here through the subfolder. Task 6 builds A, in `FolderMenu.commands`.
2. **Who's folded under "N offline".** The design lists a folder's people "online, typing and recording first, then by when last seen, as jawd sorts them, the rest folded under N offline", and the wireframe lists Mãe, last seen 12 minutes ago, above "▸ 3 offline"; the terminal, under All, folds everyone who isn't online, typing or recording, and under a folder folds nobody. A: under a folder, those jawd has no last seen for (who hide it, or whom WhatsApp hasn't told jawd about) are folded and the rest listed as jawd sorts them; under All, as the terminal, everyone not online, typing or recording (lean, as the wireframe and the terminal draw them); B: everyone not online, typing or recording, everywhere; C: under a folder nobody, as the terminal. Whichever, someone typing or recording in their chat is listed, never folded: jawd's presence is `online`, `seen` or `hidden`, and typing shows from the chat's typing (the protocol's "Presence and typing in chats"), so it doesn't move them in jawd's order. Task 13 builds A, in `RosterLines.lines`.
3. **Saying which folder the list shows when the sidebar's folded.** Below 960 points, after ⌘B and in focus mode, nothing on screen names the list's folder, and ⌥⇧↓ or ⌘1 can change it; the terminal names it in its footer when neither its list nor its sidebar is drawn, with the key that shows its chats. A: the status line names a folder a key picks while the sidebar isn't shown, adding the key that shows its chats when the list isn't shown either (lean); B: a title over the chat list, always; C: nothing. Task 12 builds A, in `FolderTree.pickedLine`.
4. **Move to and Remove from beyond the chat list.** The design gives the pane's menu "the chat's actions as the chat list's menu has them", and the wireframe draws the pane's menu with Add to Folder and Favourite only. A: every chat menu (the list's, the pane's, the menu bar's Chat menu) has Move to and Remove from while the list shows one of your folders that holds the chat, directly or through a subfolder, by one rule (lean: each action written once); B: the chat list's menu only. Task 6 builds A, in `FolderMenu.entries` and `ChatMenu.folderItems`.
5. **A folder made in place.** Decided: a folder made from the sidebar (⇧⌘N, the sidebar's New Folder… and a folder's New Folder Inside…) is picked, so the list shows it, as Notes and Reminders select a new item in their sidebars; one made from a chat's Add to Folder ▸ New Folder… isn't, and the list keeps what it showed, since that files the chat there and shouldn't take the list away. Task 7 builds it, in `SidebarController.named(_:)`.
6. **Clicking away from the name field.** The design has Return keep the name and Esc drop it. Decided: anything else that ends the editing, such as a click elsewhere, keeps what's typed as Return does, as Finder, Notes and Xcode keep a name, unless it's empty or unchanged; Esc alone drops it. If jawd turns the name down once the keys have gone elsewhere, the row shows jawd's words without taking the keys back, and Esc or another click away drops it. Task 7 builds it, in `SidebarController.controlTextDidEndEditing(_:)`.
7. **Communities dragged onto folders.** The chat list lets no community be dragged today, since one can't go in a pane. Decided: a community's row drags onto your folders like any chat, as the design has "a chat from the chat list" dragged there and a community's row is one, carrying a pasteboard type of its own that the panes don't register, so it can't be dropped on a pane. Task 9 builds it.
8. **The favourites in the Find menu.** "The Find menu lists them with what's on each", and an empty slot's key has to answer, since it says so in the status line, so it needs an item. A: a Favourites submenu of the Find menu with the nine slots, each titled by what's on it or Empty, ⌘1 to ⌘9 beside them (lean); B: the nine in the Find menu itself; C: only the slots held, the empty ones' keys answered by hidden items. Task 10 builds A.

## Global Constraints

- macOS 26 and later, Swift 6 language mode, warnings as errors (`mac/Project.swift`).
- No font, radius or line width set by hand outside `JawUI/Sources/Components`: `mac/scripts/check-components` fails the build on one.
- Everything drawn lives in JawUI and is made from values; the app target defines no view or view controller of its own (the harness's second rule, which `mac-harness` adds to `scripts/check-harness`, fails the build on one once it's on main); `AppModel` is the only code that talks to jawd. The harness names neither `Jawd` nor `Client` (`scripts/check-harness`).
- Every new screen has a harness scene (`JawHarness/Sources/Scenes/`, registered in `BuiltInScenes.swift`), checked in its sweep. Menus aren't screens a harness picture can hold (AppKit draws them in windows of their own); they're checked by JawUI's tests and the snapshot's `menu:` steps.
- Each command is written once, with its menu title, palette phrase, key and menu; right-click menus are made from `MenuEntry` values worked out from rows, by `Menus.menu(_:command:)` and `Menus.items(_:command:)` (`mac/README.md`; the menus plan's task 1). An item under a title of its own has no tick; one whose title is its command's takes the command's tick.
- The words are the design's: "Add to Folder", "New Folder…", "Move to", "Remove from <folder>", "Favourite", "Remove from Favourites", "New Folder Inside…", "Rename", "Delete…", "Folders", "Smart Folders", "Open Chat", "Start Chat", "Open in New Pane", "typing…", "recording…" (once jawd says which: a chat row's `typing` says only who, task 13), "online", "N offline"; in the menu bar "New Folder" ⇧⌘N, "Rename Folder", "Delete Folder…" ⌘⌫, "Previous Folder" ⌥⇧↑, "Next Folder" ⌥⇧↓ and "Favourites" with ⌘1 to ⌘9. Where the design gives none, the terminal's are the reference (the parity plan's rule 1) with the Mac's keys in the terminal's place: "Ana Ribeiro filed in Family", "Ana Ribeiro taken out of Family", "Family moved to the top", "⌘3 goes to Family", "⌘3 is free", "nothing is on ⌘3; …".
- What jawd refuses is said in jawd's words: under the name field for a name, in the status line for anything else.
- Nothing asks jawd as a menu opens: every menu is made from rows the app holds (the design's Budgets).
- Before jawd serves folders (a jawd from main answers `folders.watch` with -32601), the sidebar keeps All and Archived, no menu has a folder item, nothing says a window couldn't be watched, and ⇧⌘N, ⌘1 to ⌘9, ⌥⇧↑ and ⌥⇧↓ say what they wait on when chosen ("folders come with jawd's M6", "favourites come with jawd's M6"), as the parity plan's rule 2 has a step land before jawd's part.
- `folders`' fixtures are copied with `git show folders:protocol/fixtures/<file>`, never from another checkout. Never touch `~/src/jaw-mac-m1`, `~/src/jaw-folders` or `~/src/jaw-harness`, nor `mac-menus`' checkout, `~/src/jaw-mac-menus`; never open `~/Library/Application Support/jaw`; never stop a process by name; the app's snapshot runs `--demo`.
- Nothing here is jawd's data layer (`scripts/check-data-audit.py`'s paths), so it needs no database audit.
- Tests run on the build Mac, never on this one: `mac/scripts/build-mac test JawKit`, `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests` and `mac/scripts/build-mac build jaw`, each printing only jaw's diagnostics and `** TEST SUCCEEDED **` or `** BUILD SUCCEEDED **`. Before each `build-mac` command, check the build Mac answers, `ssh -o BatchMode=yes -o ConnectTimeout=5 build-mac true`, and if it doesn't, stop and report that the build Mac is down: `build-mac` would otherwise fall back to building and running the app on this Mac (`==> build-mac doesn't answer; running here`), where the Mac app is never built.
- Commits are on `mac-folders`, in the repository's style: one or more plain sentences saying what changed and why, no prefix, British spelling.
- Scratch goes in `/tmp/jaw-mac-folders/`; a Rust build runs `df -h` first and uses `CARGO_INCREMENTAL=0`. Each task deletes the pictures and logs it made there once it has looked at them, before it reports, naming in its report any it leaves for the owner to see.

## Where it's built, and what it waits on

- **The branch.** `mac-folders` is made from main, which has the menus this plan builds on (`654037e` to `e7999d6`, then the palette's ⌘↩ through `openInNewPane` and VoiceOver's press on a button); its commits of its own before task 1 are this plan and its amendments, rebased onto main at f917587, whose Mac code the tasks' sketches are written against. The menus' code the tasks name (`MenuEntry`, `Menus`, `ChatMenu`, `chatCommands(for:)`, `openInNewPane(_:)`, `MenuPopUp`, the snapshot's `menu:` steps) is main's, as the menus plan built it. Main since has the menu bar as a `MenuBar`, which runs a command on Return only for its own modifiers, the palette's `.newPane` pick, and a snapshot that waits for the chat list to settle before pressing its keys; none of them changes what a task does. The branch is built in a worktree made from it, `~/src/jaw-mac-folders` (`git -C ~/src/jaw-mac-menus worktree add ~/src/jaw-mac-folders mac-folders`), and `build-mac` copies that to `~/src/build-mac/jaw/jaw-mac-folders` on the build Mac.
- **Landing.** As the menus did, `mac-folders` lands on main once its tasks are done and reviewed, each task and then the whole branch, rather than being merged by the Mac builder: rebased onto main's tip, again if main moves first, and pushed from `~/src/jaw-mac-folders`, the worktree it was built in ("After the plan" has the steps). Its last commit before it lands takes step 2.1's rows out of `mac/parity.toml`, so main has `make check-parity` passing, as the parity plan's working rules have a step land: from task 2 on the Mac sends `folders.watch` and the folder requests, and the check fails on a gap the Mac has closed that the list still holds. Tasks 1 to 14 can land before `folders` is on main, as the parity plan's rule 2 has a step land before jawd's part, each folder action saying what it waits on; task 15 then follows on `mac-folders` rebased onto a main that has `folders`, and lands the same way. Each time the branch lands, the build Mac's folder (`ssh build-mac rm -rf src/build-mac/jaw/jaw-mac-folders`) and `target` in the worktree (jawd's debug build, several gigabytes) are deleted, which `build-mac` and task 15 make again if it follows; once task 15 has landed, so is the worktree.
- **The harness's second phase.** `mac-harness` (not on main yet) moves the window's content into JawUI as `WindowContent`, which takes a `SidebarController` and leaves `MainWindowController` to wire it, as it does here. Every change to the sidebar is inside `SidebarController`; where a task changes `MainWindowController` or `AppModel.stateView`, it says what the same change is if `WindowContent` is in the checkout by then.
- **jawd.** The protocol is `folders` at 53981ad, rebased onto main's 8591529, its audit under way after two looks (`git show folders:protocol/README.md`: "Folders and the roster", "roster.watch", "Presence" and "Presence and typing in chats"). Since the plan was first written there, someone's presence is `online`, `seen` (with `at`) or `hidden`, and `known`; who's typing or recording isn't a presence any more but shows from the chat's typing, which jaw.db never holds, so it never moves a roster row (the roster fixture's Ana is `online` where she was `typing`). jawd sends no typing yet: a chat row's `typing` is always empty, and the `typing` push the protocol describes isn't sent; when typing comes it comes as that push, and the row's field goes. The Mac reads who's typing from the chat's row, as a pane reads it today, and the parity plan's step 3.1, which moves the panes' typing onto the push, moves the roster's with it, so typing shows in the roster on a real jawd from then. Each task says what it waits on and what it can't try until `folders` lands; nothing is tried end to end until `folders` is on main and `mac-folders` is rebased onto it, which is task 15. Before then, the snapshot on `--demo`, whose jawd is built from the worktree, main's without folders, checks that the app falls back as it should.

## Files

- `mac/Tests/JawKitTests/Ahead/` (create, task 1): the twelve fixtures copied from `folders`, deleted in task 15.
- `mac/JawKit/Sources/Protocol/Folders.swift` (create, task 1): `FolderRow`, `Favourite`, `FoldersWatched`, `FoldersPush`, `PresenceState`, `Presence`, `RosterRow`, `RosterWatched`, `RosterPush`, and the requests' params.
- `mac/JawKit/Sources/Protocol/Rows.swift` (modify, task 1): a chat row's `folders` and `favourite`.
- `mac/JawKit/Sources/Client/Incoming.swift`, `Client.swift`, `Jawd.swift`, `mac/JawKit/Sources/Windows/Window.swift` (modify, task 2): the tree's and a roster's windows, `People`, the slots in `Update` and `Merged`, the folder requests, Move to finished after a drop, `JawUpdate`'s new cases.
- `mac/JawUI/Sources/Sidebar/FolderTree.swift` (create, task 3; grown in 5, 9 and 12): the tree as jawd keeps it and what's read from it.
- `mac/JawUI/Sources/Sidebar/SidebarController.swift` (rewrite, task 4; grown in 7, 8, 9, 12 and 13): the outline of the tree, picking, and the roster under it.
- `mac/JawUI/Sources/Sidebar/SidebarCells.swift` (create, task 4; grown in 7 and 8): a folder's row, a heading's, the name field's.
- `mac/JawUI/Sources/Sidebar/SidebarNaming.swift` (create, task 7): naming in place.
- `mac/JawUI/Sources/Sidebar/SidebarDrops.swift` (create, task 8; grown in 9): a folder's menu and the drops.
- `mac/JawUI/Sources/Sidebar/FolderMenu.swift` (create, task 6; grown in 7, 8, 10 and 11): `Filed`, `FolderAct`, the folder items' entries and commands, the palette's folder actions, and what's said once jawd has done each.
- `mac/JawUI/Sources/Sidebar/RosterLines.swift` and `RosterController.swift` (create, task 13; grown in 14): the roster's lines, words and menu, and its view.
- `mac/JawUI/Sources/ChatList/ChatMenu.swift` (modify, tasks 6 and 10): the chat's folder items, the same in each of its menus.
- `mac/JawUI/Sources/ChatList/ChatListController.swift` (modify, tasks 5, 6, 9, 12 and 15): the empty folder's line, a community's menu and drag, the pasteboard types, `isShown`, `drawn`.
- `mac/JawUI/Sources/Commands/Command.swift` (modify, task 6): a command's indent.
- `mac/JawUI/Sources/Conversation/ConversationController.swift` (modify, task 6): m.
- `mac/JawUI/Sources/Samples.swift` (modify, tasks 3 and 13): a folder tree, its slots, and people.
- `mac/JawApp/Sources/Folders.swift` (create, task 4; grown in 6 to 14): the sidebar's and the roster's host, the folder acts, the menu bar's folder items, the favourites.
- `mac/JawApp/Sources/AppModel.swift`, `ChatMenus.swift`, `Commands.swift`, `MainWindowController.swift`, `Bench.swift` (modify, from task 2 on): the windows, the acts, the commands, the snapshot's steps.
- `mac/JawHarness/Sources/Scenes/SidebarScenes.swift` (create, task 4; grown in 5, 7, 13 and 15), `BuiltInScenes.swift`, `Fixtures/Fixtures.swift` (task 15): the sidebar, an empty folder, naming in place, the roster, jawd's own tree.
- `mac/README.md` (modify, tasks 4, 7, 8, 13, 14 and 15): the snapshot's new steps, and what the sidebar is.
- `mac/parity.toml` (modify, in the branch's last commit before it lands): step 2.1's rows out.
- Tests (create): `mac/Tests/JawKitTests/FolderClientTests.swift`; `mac/Tests/JawUITests/FolderTreeTests.swift`, `SidebarTests.swift`, `FolderMenuTests.swift`, `NamingTests.swift`, `FolderPaletteTests.swift`, `RosterTests.swift`, `SidebarPictureTests.swift` and its references; (modify) `FixtureTests.swift`, `FakeJawd.swift`, `LiveTests.swift`, `CommandTests.swift`, `ChatListTests.swift`, `ChatListMenuTests.swift`, `PaneMenuTests.swift`, `ScenesTests.swift`.

---

### Task 1: JawKit's folder, slot and roster types, against the folders branch's fixtures

**Waits on jawd:** nothing to build: the twelve fixtures are `folders`' at 53981ad, where someone's presence is `online`, `seen` or `hidden` and the roster fixture's Ana is `online`. Nothing here meets a real jawd until task 15.

**Files:**
- Create: `mac/Tests/JawKitTests/Ahead/` with `folders.watch.request.json`, `folders.watch.result.json`, `folders.push.json`, `roster.watch.request.json`, `roster.watch.result.json`, `roster.push.json`, `folder.create.request.json`, `folder.create.result.json`, `folder.add.request.json`, `folder.add.result.json`, `favourite.set.request.json`, `favourite.set.result.json`
- Create: `mac/JawKit/Sources/Protocol/Folders.swift`
- Modify: `mac/JawKit/Sources/Protocol/Rows.swift` (`ChatRow`)
- Test: `mac/Tests/JawKitTests/FixtureTests.swift`

**Interfaces:**
- Produces: `public struct FolderRow: WindowRow, Decodable, Hashable` (`id`, `v`, `parent: String?`, `name`, `position: Int`, `smart: JSONValue?`, `unread`, `mentions`, `count: Int`, `favourite: Int?`; `sort` is `""`; `static let builtIn: Set<String>`; `isBuiltIn`, `isSmart`, `isYours`; `init(id:v:parent:name:position:smart:unread:mentions:count:favourite:)` with every parameter but `id` and `name` defaulted); `public struct Favourite: Decodable, Sendable, Hashable` (`slot: Int`, `folder`, `chat`, `name: String?`, `target: String?`; `init(slot:folder:chat:name:)`); `FoldersWatched` (`watch`, `seq`, `rows`, `remove`, `favourites: [Favourite]`); `FoldersPush` (`watch`, `seq`, `upsert`, `remove`, `favourites: [Favourite]?`); `public enum PresenceState: String` (`online`, `seen`, `hidden`; typing and recording aren't among them, as they show from the chat's typing); `public struct Presence` (`state`, `at: Int64?`, `known: Bool`, `around: Bool`, which is online; `init(state:at:known:)`); `public struct RosterRow: WindowRow, Decodable, Hashable` (`id`, `v`, `sort`, `name`, `phone`, `about`, `avatar`, `presence`, `chat: String?`, `folders: [String]`; `init(id:v:sort:name:phone:presence:chat:folders:)`); `RosterWatched` (`watch`, `seq`, `total`, `rows`, `remove`), `RosterPush` (`watch`, `seq`, `total?`, `upsert`, `remove`); the params `FoldersWatch(have:)` and `RosterWatch(folder:around:before:after:have:)` (internal), `FolderCreate(name:parent:)`, `FolderRename(folder:name:)`, `FolderMove(folder:parent:position:)`, `FolderID(folder:)`, `FolderItem(folder:item:)`, `FavouriteSet(slot:target:)` (public), and `FolderMade` (`folder`, internal), `folder.create`'s answer. `ChatRow` gains `folders: [String]` and `favourite: Int?`, and its memberwise init a last parameter `folders: [String] = []`.

- [ ] **Step 1: Copy the fixtures**

Run, from the worktree's root:

```bash
mkdir -p mac/Tests/JawKitTests/Ahead
for f in folders.watch.request folders.watch.result folders.push roster.watch.request roster.watch.result roster.push \
         folder.create.request folder.create.result folder.add.request folder.add.result favourite.set.request favourite.set.result; do
    git show folders:protocol/fixtures/$f.json > mac/Tests/JawKitTests/Ahead/$f.json
done
for f in mac/Tests/JawKitTests/Ahead/*.json; do git show folders:protocol/fixtures/$(basename $f) | cmp - $f; done
ls mac/Tests/JawKitTests/Ahead | wc -l
```

Expected: no output from `cmp`, and `12`. Only these twelve: `folders`' other fixtures are main's at 8591529, which it's rebased onto, so main's own are as new or newer (main has since added layout book fixtures, which `folders` hasn't), and its `status.result.json` only adds the `presence` and `folders` counts, which JawKit ignores.

- [ ] **Step 2: Write the failing tests**

In `mac/Tests/JawKitTests/FixtureTests.swift`, replace

```swift
func fixture(_ name: String) throws -> Data {
    try Data(contentsOf: fixtures.appending(path: name))
}
```

with

```swift
/// Fixtures copied from a jawd branch whose protocol isn't on main yet, as the parity plan builds the
/// Mac ahead of jawd: the `folders` branch's twelve, until it lands. A copy is read only while
/// protocol/fixtures has no file of its name.
let ahead = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appending(path: "Ahead")

func fixture(_ name: String) throws -> Data {
    let landed = fixtures.appending(path: name)
    return try Data(contentsOf: FileManager.default.fileExists(atPath: landed.path) ? landed : ahead.appending(path: name))
}

/// Every fixture's name: protocol/fixtures', and the copies' still to land.
func fixtureNames() throws -> [String] {
    let landed = try FileManager.default.contentsOfDirectory(atPath: fixtures.path)
    let copies = (try? FileManager.default.contentsOfDirectory(atPath: ahead.path)) ?? []
    return Set(landed + copies).filter { $0.hasSuffix(".json") }.sorted()
}

/// A fixture's `result`, or its push's `params`, read as one type.
struct Answer<T: Decodable>: Decodable {
    let result: T
}

struct Pushed<T: Decodable>: Decodable {
    let params: T
}
```

In `every result and push fixture decodes`, replace

```swift
    let names = try FileManager.default.contentsOfDirectory(atPath: fixtures.path).filter { $0.hasSuffix(".json") }
```

with

```swift
    let names = try fixtureNames()
```

and add, after `each request JawKit writes equals its fixture`:

```swift
@Test func `a fixture copied ahead of jawd is byte for byte the one that lands`() throws {
    let copies = ((try? FileManager.default.contentsOfDirectory(atPath: ahead.path)) ?? []).filter { $0.hasSuffix(".json") }
    for name in copies {
        let landed = fixtures.appending(path: name)
        guard FileManager.default.fileExists(atPath: landed.path) else { continue }
        #expect(try Data(contentsOf: landed) == Data(contentsOf: ahead.appending(path: name)),
                "\(name) changed on its way to main: copy it again, or delete the copies once they've all landed")
    }
}

@Test func `the tree, its slots and the roster read as the folders branch's fixtures have them`() throws {
    let tree = try JSONDecoder().decode(Answer<FoldersWatched>.self, from: fixture("folders.watch.result.json")).result
    #expect(tree.watch == "w5" && tree.seq == 81240 && tree.remove.isEmpty && tree.unread == 0)
    #expect(tree.rows.map(\.id) == ["all", "unread", "folder:9", "folder:10", "smart:7"])
    #expect(tree.rows.map(\.isBuiltIn) == [true, true, false, false, false])
    #expect(tree.rows.map(\.isYours) == [false, false, true, true, false])
    #expect(tree.rows[4].isSmart && tree.rows[4].smart == .object(["mentions": .bool(true)]))
    #expect(tree.rows[2].parent == nil && tree.rows[3].parent == "folder:9" && tree.rows[3].position == 0)
    #expect(tree.rows[2].unread == 3 && tree.rows[2].mentions == 1 && tree.rows[2].count == 6 && tree.rows[2].favourite == 1)
    #expect(tree.favourites == [Favourite(slot: 1, folder: "folder:9"), Favourite(slot: 2, folder: "unread"),
                                 Favourite(slot: 3, chat: "351912345678@s.whatsapp.net", name: "Ana Ribeiro")])
    #expect(tree.favourites.map(\.target) == ["folder:9", "unread", "351912345678@s.whatsapp.net"])

    let push = try JSONDecoder().decode(Pushed<FoldersPush>.self, from: fixture("folders.push.json")).params
    #expect(push.upsert.map(\.unread) == [4] && push.upsert.first?.v == 81241)
    #expect(push.favourites == [Favourite(slot: 1, folder: "folder:9")], "the slots whole, as one changed")
    let quiet = try JSONDecoder().decode(FoldersPush.self, from: Data(#"{"watch":"w5","seq":81242,"upsert":[]}"#.utf8))
    #expect(quiet.favourites == nil, "no slot changed, so the slots stay as they were")

    let made = try JSONDecoder().decode(Answer<FolderMade>.self, from: fixture("folder.create.result.json")).result.folder
    #expect(made.id == "folder:10" && made.parent == "folder:9" && made.name == "Cousins" && made.count == 0)

    let roster = try JSONDecoder().decode(Answer<RosterWatched>.self, from: fixture("roster.watch.result.json")).result
    #expect(roster.total == 6 && roster.rows.map(\.id) == ["person:1", "person:4", "person:5"])
    #expect(roster.rows.map(\.presence.state) == [.online, .seen, .hidden], "Ana online: typing shows from her chat, not here")
    #expect(roster.rows.map(\.presence.around) == [true, false, false])
    #expect(roster.rows[1].presence.at == 1_792_145_000_000 && !roster.rows[2].presence.known)
    #expect(roster.rows[0].chat == "351912345678@s.whatsapp.net" && roster.rows[1].chat == nil)
    #expect(roster.rows[1].folders == ["folder:10"] && roster.rows[0].about == "Lisboa")
    let came = try JSONDecoder().decode(Pushed<RosterPush>.self, from: fixture("roster.push.json")).params
    #expect(came.upsert.map(\.presence.state) == [.online] && came.upsert.first?.sort.hasPrefix("0~") == true)
    let odd = try JSONDecoder().decode(Presence.self, from: Data(#"{"state":"away"}"#.utf8))
    #expect(odd.state == .hidden && odd.known, "a state jawd may add later says nothing")
}

@Test func `a chat row says the folders it sits in and its slot, as the protocol's text has them`() throws {
    // No fixture on the folders branch shows them, so this is the protocol's chat row with them added.
    let row = try JSONDecoder().decode(ChatRow.self, from: Data(#"""
    {"id":"351912345678@s.whatsapp.net","v":42,"sort":"1~1792145060000~000000000017","kind":"direct","name":"Ana Ribeiro",
     "unread":2,"mentions":0,"pinned":true,"muted_until":null,"archived":false,"marked_unread":false,"draft":null,
     "community":null,"folders":["folder:9"],"favourite":3}
    """#.utf8))
    #expect(row.folders == ["folder:9"] && row.favourite == 3)
    let bare = try JSONDecoder().decode(ChatRow.self, from: Data(#"{"id":"x@s.whatsapp.net","v":1,"sort":"1"}"#.utf8))
    #expect(bare.folders.isEmpty && bare.favourite == nil, "absent when it's in none and on no slot")
}

@Test func `each folder request JawKit writes equals its fixture, or the protocol's text where none shows it`() throws {
    func same(_ name: String, _ written: JSONValue) throws {
        #expect(written == (try JSONValue.parse(fixture(name))), "\(name)")
    }
    try same("folders.watch.request.json", written(id: 50, "folders.watch", FoldersWatch(have: [])))
    try same("roster.watch.request.json", written(id: 51, "roster.watch", RosterWatch(
        folder: "folder:9", around: .top, before: 0, after: 200, have: [],
    )))
    try same("folder.create.request.json", written(id: 52, "folder.create", FolderCreate(name: "Cousins", parent: "folder:9")))
    try same("folder.add.request.json", written(id: 53, "folder.add", FolderItem(folder: "folder:10", item: "person:4")))
    try same("favourite.set.request.json", written(id: 54, "favourite.set", FavouriteSet(slot: 3, target: "351912345678@s.whatsapp.net")))

    func params(_ p: some Encodable & Sendable) throws -> JSONValue? {
        try written(id: 1, "x", p)["params"]
    }
    #expect(try params(FolderCreate(name: "Work", parent: nil)) == .object(["name": .string("Work")]), "at the top")
    #expect(try params(FolderRename(folder: "folder:9", name: "Família")) == .object(["folder": .string("folder:9"), "name": .string("Família")]))
    #expect(try params(FolderMove(folder: "folder:10", parent: nil, position: nil)) == .object(["folder": .string("folder:10"), "parent": .null]),
            "back to the top, last among its new siblings")
    #expect(try params(FolderMove(folder: "folder:10", parent: "folder:4", position: 0))
        == .object(["folder": .string("folder:10"), "parent": .string("folder:4"), "position": .number(0)]))
    #expect(try params(FolderID(folder: "folder:10")) == .object(["folder": .string("folder:10")]))
    #expect(try params(FavouriteSet(slot: 3, target: nil)) == .object(["slot": .number(3), "target": .null]), "the slot emptied")
    #expect(try params(FoldersWatch(have: ["all", "folder:9"])) == .object(["have": .array([.string("all"), .string("folder:9")])]))
}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: the build fails in `FixtureTests.swift` with `cannot find type 'FoldersWatched' in scope` and `cannot find 'FolderCreate' in scope`, among others.

- [ ] **Step 4: Write the types**

Create `mac/JawKit/Sources/Protocol/Folders.swift`:

```swift
import Foundation

// The folder tree, the favourite slots, the roster and the folder requests, as jawd's M6 sends and
// takes them: protocol/README.md's "Folders and the roster", and jaw-proto's windows.rs.

/// A folder in the tree, as `folders.watch` sends it: built in (`all`, `unread`, `groups`,
/// `communities`, `archived`), smart (`smart:` and a number) or yours (`folder:` and a number).
public struct FolderRow: WindowRow, Decodable, Hashable {
    public var id: String
    /// Its version. jawd keeps it as the folder's counts change, so a row sent again with the same
    /// `v` is the newer, as any window's upsert is.
    public var v: UInt64
    /// The folder it's in; nil at the top.
    public var parent: String?
    public var name: String
    /// Its place among its siblings of its kind.
    public var position: Int
    /// A smart folder's rule; nil for the others.
    public var smart: JSONValue?
    /// Its chats' and its subfolders' chats' unread messages, and those that mention you, each chat
    /// counted once.
    public var unread: Int
    public var mentions: Int
    /// The chats and people in it and under it, each once; a smart folder's counted up to 1,000.
    public var count: Int
    /// Its favourite slot, 1 to 9, as the row has it; the tree's slots say the same, and the sidebar
    /// reads those.
    public var favourite: Int?

    /// A folder has no place in a list of its own: the tree is laid out by `parent` and `position`.
    public var sort: String {
        ""
    }

    /// The folders jaw always has.
    public static let builtIn: Set<String> = ["all", "unread", "groups", "communities", "archived"]

    public var isBuiltIn: Bool {
        Self.builtIn.contains(id)
    }

    public var isSmart: Bool {
        smart != nil
    }

    /// One of yours, which you name, nest, move and file into.
    public var isYours: Bool {
        !isBuiltIn && !isSmart
    }

    public init(id: String, v: UInt64 = 0, parent: String? = nil, name: String, position: Int = 0, smart: JSONValue? = nil,
                unread: Int = 0, mentions: Int = 0, count: Int = 0, favourite: Int? = nil)
    {
        self.id = id
        self.v = v
        self.parent = parent
        self.name = name
        self.position = position
        self.smart = smart
        self.unread = unread
        self.mentions = mentions
        self.count = count
        self.favourite = favourite
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        v = try c.decodeIfPresent(UInt64.self, forKey: .v) ?? 0
        parent = try c.decodeIfPresent(String.self, forKey: .parent)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        position = try c.decodeIfPresent(Int.self, forKey: .position) ?? 0
        smart = try c.decodeIfPresent(JSONValue.self, forKey: .smart)
        unread = try c.decodeIfPresent(Int.self, forKey: .unread) ?? 0
        mentions = try c.decodeIfPresent(Int.self, forKey: .mentions) ?? 0
        count = try c.decodeIfPresent(Int.self, forKey: .count) ?? 0
        favourite = try c.decodeIfPresent(Int.self, forKey: .favourite)
    }

    enum Keys: String, CodingKey {
        case id, v, parent, name, position, smart, unread, mentions, count, favourite
    }
}

/// A favourite slot: what ⌘ and its digit go to, a folder, or a chat with its name.
public struct Favourite: Decodable, Sendable, Hashable {
    public var slot: Int
    public var folder: String?
    public var chat: String?
    /// The chat's name, for a chat.
    public var name: String?

    public init(slot: Int, folder: String? = nil, chat: String? = nil, name: String? = nil) {
        self.slot = slot
        self.folder = folder
        self.chat = chat
        self.name = name
    }

    /// The folder's or the chat's id.
    public var target: String? {
        folder ?? chat
    }
}

/// What `folders.watch` returns: every folder, and the slots held, the empty ones left out.
public struct FoldersWatched: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var rows: [FolderRow]
    public var remove: [String]
    public var favourites: [Favourite]
    /// Rows that weren't jaw's protocol, left out, and why the first wasn't.
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        rows = try c.rows(.rows, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
        favourites = try c.decodeIfPresent([Favourite].self, forKey: .favourites) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, rows, remove, favourites
    }
}

/// The `folders` push: the folders that changed, and the slots whole when one of them changed, nil
/// when none did.
public struct FoldersPush: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var upsert: [FolderRow]
    public var remove: [String]
    public var favourites: [Favourite]?
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        upsert = try c.rows(.upsert, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
        favourites = try c.decodeIfPresent([Favourite].self, forKey: .favourites)
    }

    enum Keys: String, CodingKey {
        case watch, seq, upsert, remove, favourites
    }
}

/// How someone is: online, last seen, or hidden, when they hide it or WhatsApp hasn't told jawd.
/// Typing and recording aren't among them: they show from the chat's typing, which jaw.db never
/// holds, so they never move someone in a roster.
public enum PresenceState: String, Decodable, Sendable, Hashable {
    case online, seen, hidden

    /// A state jawd may add later reads as hidden, which says nothing.
    public init(from decoder: any Decoder) throws {
        self = try PresenceState(rawValue: decoder.singleValueContainer().decode(String.self)) ?? .hidden
    }
}

public struct Presence: Decodable, Sendable, Hashable {
    public var state: PresenceState
    /// When they were last seen, for `seen`.
    public var at: Int64?
    /// False while jawd isn't following them, so it's what was last known, drawn faintly.
    public var known: Bool

    public init(state: PresenceState, at: Int64? = nil, known: Bool = true) {
        self.state = state
        self.at = at
        self.known = known
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        state = (try? c.decodeIfPresent(PresenceState.self, forKey: .state)) ?? .hidden
        at = try c.decodeIfPresent(Int64.self, forKey: .at)
        known = try c.decodeIfPresent(Bool.self, forKey: .known) ?? true
    }

    enum Keys: String, CodingKey {
        case state, at, known
    }

    /// Online. Someone typing or recording in their chat is around too, which the roster reads from
    /// the chat.
    public var around: Bool {
        state == .online
    }
}

/// Someone in a roster, which reads from the smallest `sort` up: online first, then by when last
/// seen, then those jawd has no last seen for, each by name.
public struct RosterRow: WindowRow, Decodable, Hashable {
    /// `person:` and a number.
    public var id: String
    public var v: UInt64
    public var sort: String
    public var name: String
    public var phone: String?
    public var about: String?
    /// Their picture's path, relative to jaw's folder, once fetched.
    public var avatar: String?
    public var presence: Presence
    /// Their one-to-one chat's id, if there is one.
    public var chat: String?
    /// Your folders they're filed in, themselves or through their chat.
    public var folders: [String]

    public init(id: String, v: UInt64, sort: String, name: String, phone: String? = nil, presence: Presence,
                chat: String? = nil, folders: [String] = [])
    {
        self.id = id
        self.v = v
        self.sort = sort
        self.name = name
        self.phone = phone
        about = nil
        avatar = nil
        self.presence = presence
        self.chat = chat
        self.folders = folders
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        v = try c.decode(UInt64.self, forKey: .v)
        sort = try c.decode(String.self, forKey: .sort)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        phone = try c.decodeIfPresent(String.self, forKey: .phone)
        about = try c.decodeIfPresent(String.self, forKey: .about)
        avatar = try c.decodeIfPresent(String.self, forKey: .avatar)
        presence = (try? c.decodeIfPresent(Presence.self, forKey: .presence)) ?? Presence(state: .hidden)
        chat = try c.decodeIfPresent(String.self, forKey: .chat)
        folders = (try? c.decodeIfPresent([String].self, forKey: .folders)) ?? []
    }

    enum Keys: String, CodingKey {
        case id, v, sort, name, phone, about, avatar, presence, chat, folders
    }
}

/// What `roster.watch` returns, and `watch.move` on a roster.
public struct RosterWatched: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64
    public var rows: [RosterRow]
    public var remove: [String]
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total) ?? 0
        rows = try c.rows(.rows, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, rows, remove
    }
}

/// The `roster` push.
public struct RosterPush: Decodable, Sendable {
    public var watch: String
    public var seq: UInt64
    public var total: UInt64?
    public var upsert: [RosterRow]
    public var remove: [String]
    public var unread = 0
    public var why: String?

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        watch = try c.decode(String.self, forKey: .watch)
        seq = try c.decode(UInt64.self, forKey: .seq)
        total = try c.decodeIfPresent(UInt64.self, forKey: .total)
        upsert = try c.rows(.upsert, into: &unread, why: &why)
        remove = try c.decodeIfPresent([String].self, forKey: .remove) ?? []
    }

    enum Keys: String, CodingKey {
        case watch, seq, total, upsert, remove
    }
}

/// `folders.watch`'s params. The tree is one window, watched whole; watched again, it says the rows
/// held, and the answer's `remove` says which went. jawd compares the tree row by row whatever
/// `since` says, as a folder's counts change without its version, so `since` isn't sent.
struct FoldersWatch: Encodable, Sendable {
    var have: [String]

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        if !have.isEmpty { try c.encode(have, forKey: .have) }
    }

    enum Keys: String, CodingKey {
        case have
    }
}

/// `roster.watch`'s params: a folder's people from the top. `offline` is always true, since the
/// sidebar counts the people it folds.
struct RosterWatch: Encodable, Sendable {
    var folder: String
    var around: Around
    var before: UInt32
    var after: UInt32
    var have: [String]

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(folder, forKey: .folder)
        try c.encode(around, forKey: .around)
        try c.encode(before, forKey: .before)
        try c.encode(after, forKey: .after)
        try c.encode(true, forKey: .offline)
        if !have.isEmpty { try c.encode(have, forKey: .have) }
    }

    enum Keys: String, CodingKey {
        case folder, around, before, after, offline, have
    }
}

/// `folder.create`'s params: a folder of yours in `parent`, or at the top when it's nil.
public struct FolderCreate: Encodable, Sendable, Hashable {
    public var name: String
    public var parent: String?

    public init(name: String, parent: String?) {
        self.name = name
        self.parent = parent
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(name, forKey: .name)
        try c.encodeIfPresent(parent, forKey: .parent)
    }

    enum Keys: String, CodingKey {
        case name, parent
    }
}

/// `folder.rename`'s params.
public struct FolderRename: Encodable, Sendable, Hashable {
    public var folder: String
    public var name: String

    public init(folder: String, name: String) {
        self.folder = folder
        self.name = name
    }
}

/// `folder.move`'s params: into `parent`, or back to the top with nil, which is sent as null; at
/// `position` among its new siblings, or last without one.
public struct FolderMove: Encodable, Sendable, Hashable {
    public var folder: String
    public var parent: String?
    public var position: Int?

    public init(folder: String, parent: String?, position: Int?) {
        self.folder = folder
        self.parent = parent
        self.position = position
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(folder, forKey: .folder)
        if let parent { try c.encode(parent, forKey: .parent) } else { try c.encodeNil(forKey: .parent) }
        try c.encodeIfPresent(position, forKey: .position)
    }

    enum Keys: String, CodingKey {
        case folder, parent, position
    }
}

/// `folder.delete`'s params.
public struct FolderID: Encodable, Sendable, Hashable {
    public var folder: String

    public init(folder: String) {
        self.folder = folder
    }
}

/// `folder.add`'s and `folder.remove`'s params: the folder, and a chat's `id` or a `person:` id.
public struct FolderItem: Encodable, Sendable, Hashable {
    public var folder: String
    public var item: String

    public init(folder: String, item: String) {
        self.folder = folder
        self.item = item
    }
}

/// `favourite.set`'s params: a folder's or a chat's id on `slot`, or nil, sent as null, to empty it.
public struct FavouriteSet: Encodable, Sendable, Hashable {
    public var slot: Int
    public var target: String?

    public init(slot: Int, target: String?) {
        self.slot = slot
        self.target = target
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(slot, forKey: .slot)
        if let target { try c.encode(target, forKey: .target) } else { try c.encodeNil(forKey: .target) }
    }

    enum Keys: String, CodingKey {
        case slot, target
    }
}

/// `folder.create`'s answer: the folder made.
struct FolderMade: Decodable, Sendable {
    var folder: FolderRow
}
```

In `mac/JawKit/Sources/Protocol/Rows.swift`, in `ChatRow`, after `public var childrenUnread: Int?` add

```swift
    /// Your folders it sits in directly, or its person does, for a one-to-one chat.
    public var folders: [String]
    /// Its favourite slot, 1 to 9, when the chat itself is a favourite.
    public var favourite: Int?
```

in `init(from:)`, after `childrenUnread = …`,

```swift
        folders = (try? c.decodeIfPresent([String].self, forKey: .folders)) ?? []
        favourite = try c.decodeIfPresent(Int.self, forKey: .favourite)
```

in the memberwise init, add a last parameter `folders: [String] = []` after `community: String? = nil`, and after `childrenUnread = nil`,

```swift
        self.folders = folders
        favourite = nil
```

and in `Keys`, add `folders, favourite` to the first `case` line.

- [ ] **Step 5: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawKit`
Expected: `** TEST SUCCEEDED **`, with the four new tests among those passed.

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 6: Commit**

```bash
git add mac/Tests/JawKitTests/Ahead mac/JawKit/Sources/Protocol/Folders.swift mac/JawKit/Sources/Protocol/Rows.swift mac/Tests/JawKitTests/FixtureTests.swift
git commit -m "JawKit reads jawd's folder tree, its favourite slots, the roster and the folder requests as the folders branch's protocol has them, ahead of its landing: a folder row knows a built-in folder from a smart one and one of yours, a push's slots are nil when none changed, someone's presence is online, last seen or hidden, typing and recording showing from the chat rather than as a presence, and reads as hidden when jawd says a state it doesn't know, a chat row says the folders it sits in and its slot, and folder.move and favourite.set send a null parent or target where the protocol takes one; the branch's twelve fixtures are copied into JawKit's tests and read only until protocol/fixtures has them, a test checking each against the file that lands, and only those twelve, since the branch's others are main's from before it was rebased"
```

---

### Task 2: The client watches the tree and a roster, and asks for folder changes

**Waits on jawd:** nothing to build; `FakeJawd` answers from the copied fixtures. A real jawd's tree, and its refusal of a request in its own words, are tried in task 15.

**Files:**
- Modify: `mac/JawKit/Sources/Client/Incoming.swift` (`Expect`, `Reply`, `Want`, `Incoming`, `IncomingLine`)
- Modify: `mac/JawKit/Sources/Windows/Window.swift` (`Update`, `Merged`, and a new `People`)
- Modify: `mac/JawKit/Sources/Client/Client.swift` (the two window kinds)
- Modify: `mac/JawKit/Sources/Client/Jawd.swift` (the windows, the folder requests, `JawUpdate`)
- Modify: `mac/JawApp/Sources/AppModel.swift` (`apply(_:)`, for the new cases)
- Modify: `mac/Tests/JawKitTests/FakeJawd.swift`, `mac/Tests/JawKitTests/FixtureTests.swift`
- Test: `mac/Tests/JawKitTests/FolderClientTests.swift`

**Interfaces:**
- Consumes: task 1's types.
- Produces: `Client.watchFolders() -> WindowID`, `Client.watchRoster(_ folder: String, _ place: Place) -> WindowID`; `Frame.folders: [WindowID: Update<FolderRow>]` and `Frame.roster: [WindowID: Update<RosterRow>]`; `Update.favourites: [Favourite]?`, the init's last parameter `favourites:`, and `Merged` keeping the newest that isn't nil; `public struct People<R: WindowRow>` (`folder`, `place`, `total`, `seq`, `loaded`, `apply(_:) -> [String]`, `get(_:)`, `count`, `list`, from the smallest `sort` up); `Expect.folder` and `Reply.folder(FolderRow)`; on `Jawd`: `watchFolders() -> WindowID`, `watchRoster(_:_:) -> WindowID`, `folderCreate(name: String, parent: String?)`, `folderRename(_ folder: String, name: String)`, `folderMove(_ folder: String, parent: String?, position: Int?, said: String?)`, `folderDelete(_ folder: String, said: String?)`, `folderAdd(_ folder: String, item: String, said: String?)`, `folderRemove(_ folder: String, item: String, said: String?)`, `moveItem(_ item: String, from: String, to: String, said: String?, both: String)` (`both` is what's said if the chat ends in both folders: "Ana Ribeiro is now in both Cousins and Family"), `favouriteSet(slot: Int, target: String?, said: String?)`; `JawUpdate.folders(WindowID, Update<FolderRow>)`, `.roster(WindowID, Update<RosterRow>)`, `.named(FolderRow?)` (made, with its row, or renamed, with none) and `.notNamed(String)` (jawd's words); `JawUpdate.windowFailed(WindowID, Failure)` in place of `(WindowID, String)`. What's `said` comes as `.notice(said, .info)` once jawd has answered. A move whose answer a drop lost, in either half, is asked again once jawd is back, as a lost send or action is; a window whose watch again after a drop jawd refuses as gone, at the place it would fall back to, fails with `.windowFailed` rather than being asked again for ever.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawKitTests/FakeJawd.swift`, add to `FakeJawd`, after `message(_:v:sort:text:)`:

```swift
    /// What jawd answers request `id` with, from a fixture: its line with the id changed.
    static func answer(_ name: String, id: UInt64) -> String {
        guard case var .object(line)? = try? JSONValue.parse(fixture(name)) else { return "" }
        line["id"] = .number(Double(id))
        return (try? JSONEncoder().encode(JSONValue.object(line))).map { String(decoding: $0, as: UTF8.self) } ?? ""
    }

    /// A push from a fixture, as it is.
    static func pushed(_ name: String) -> String {
        ((try? fixture(name)).map { String(decoding: $0, as: UTF8.self) } ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// jawd turning request `id` down.
    static func refusal(_ id: UInt64, code: Int, _ message: String) -> String {
        "{\"id\":\(id),\"error\":{\"code\":\(code),\"message\":\"\(message)\"}}"
    }
```

In `mac/Tests/JawKitTests/FixtureTests.swift`, in `every result and push fixture decodes`, add to the `want` switch, before `default:`,

```swift
        case "folders.watch": .folders
        case "roster.watch": .roster
        case "folder.create": .reply(.folder)
```

and add `"folders", "roster"` to the list of pushes that mustn't decode only as untyped pushes.

Create `mac/Tests/JawKitTests/FolderClientTests.swift`:

```swift
import Foundation
@testable import JawKit
import Synchronization
import Testing

/// The client watches jawd's folder tree and a roster, and asks for what the folder items do: the
/// tree whole with its slots, its pushes merged, watched again whole with the folders held; a roster
/// like a chat list; a jawd without folders refusing the tree; a deleted folder's list failing once
/// it's watched again; Move to's two requests in their order, finished after a drop, and what's said
/// when it ends in both folders; a name's answer and refusal; and what's said once jawd has done each.

/// A script that answers as jawd's folders branch does: the tree, a roster and a made folder from its
/// fixtures, a roster's move as its watch, and every other request done.
func foldering(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    switch method {
    case "folders.watch": [FakeJawd.answer("folders.watch.result.json", id: id)]
    case "roster.watch", "watch.move": [FakeJawd.answer("roster.watch.result.json", id: id)]
    case "folder.create": [FakeJawd.answer("folder.create.result.json", id: id)]
    default: ordinary(method, params, id)
    }
}

/// A request's id, held back unanswered until the test answers it.
final class HeldBack: Sendable {
    let id = Mutex<UInt64?>(nil)
}

/// How many times the fake was asked for something; a class, since a script can't hold a `Mutex`.
final class Counted: Sendable {
    let n = Mutex(0)

    /// Counts one more, and says how many there have been.
    func next() -> Int {
        n.withLock { n in
            n += 1
            return n
        }
    }
}

func trees(_ all: [JawUpdate], _ window: WindowID) -> [Update<FolderRow>] {
    all.compactMap { if case let .folders(w, u) = $0, w == window { u } else { nil } }
}

func rosters(_ all: [JawUpdate], _ window: WindowID) -> [Update<RosterRow>] {
    all.compactMap { if case let .roster(w, u) = $0, w == window { u } else { nil } }
}

func notices(_ all: [JawUpdate]) -> [String] {
    all.compactMap { if case let .notice(text, _) = $0 { text } else { nil } }
}

/// Waits until jawd has everything asked before now: requests go out in order on the one socket, so
/// once a marker asked after them arrives, anything they'd have sent with them has arrived too.
@MainActor
func settled(_ jawd: Jawd, _ fake: FakeJawd) async -> Bool {
    let before = requests(fake, "favourite.set").count
    jawd.favouriteSet(slot: 9, target: nil, said: nil)
    return await eventually { requests(fake, "favourite.set").count == before + 1 }
}

@MainActor
@Suite(.serialized) struct FolderClientTests {
    @Test func `the tree comes whole with its slots, and a push changes a row and the slots`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        let first = await drain(jawd) { !trees($0, window).isEmpty }
        let tree = try #require(trees(first, window).first)
        #expect(Set(tree.upsert.map(\.id)) == ["all", "unread", "folder:9", "folder:10", "smart:7"])
        #expect(tree.favourites?.map(\.slot) == [1, 2, 3])
        #expect(requests(fake, "folders.watch").first == .object([:]), "watched with nothing held")

        fake.send(FakeJawd.pushed("folders.push.json"))
        let pushed = await drain(jawd) { !trees($0, window).isEmpty }
        let update = try #require(trees(pushed, window).first)
        #expect(update.upsert.map(\.unread) == [4])
        #expect(update.favourites == [Favourite(slot: 1, folder: "folder:9")], "the slots whole, as one changed")
    }

    @Test func `pushes between two frames merge into the newest row, though its v is the same, and the newest slots`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        _ = await drain(jawd) { !trees($0, window).isEmpty }
        fake.send(FakeJawd.pushed("folders.push.json"))
        fake.send(#"{"method":"folders","params":{"watch":"w5","seq":81242,"upsert":[{"id":"folder:9","v":81241,"parent":null,"name":"Family","position":0,"smart":null,"unread":5,"mentions":1,"count":6,"favourite":1}]}}"#)
        #expect(await eventually { jawd.client.state.withLock { $0.windows[window]?.seq } == 81242 }, "both pushes read")
        let merged = trees(jawd.drain(), window)
        #expect(merged.count == 1, "one update for the frame")
        #expect(merged.first?.upsert.map(\.unread) == [5], "a folder's counts change without its v, so the later row wins")
        #expect(merged.first?.favourites == [Favourite(slot: 1, folder: "folder:9")], "a push without slots leaves the last ones")
    }

    @Test func `the tree is watched again whole after jawd drops, saying the folders it holds and no since`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        _ = await drain(jawd) { !trees($0, window).isEmpty }
        fake.drop()
        #expect(await eventually { requests(fake, "folders.watch").count == 2 })
        let again = try #require(requests(fake, "folders.watch").last)
        #expect(Set(again["have"]?.array?.compactMap(\.string) ?? []) == ["all", "unread", "folder:9", "folder:10", "smart:7"])
        #expect(again["since"] == nil, "jawd compares the tree row by row, whatever since says")
    }

    @Test func `a jawd without folders refuses the tree with its code, and is asked again once it's back`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folders.watch" ? [FakeJawd.refusal(id, code: ErrorCode.unknownMethod, "jawd doesn't know folders.watch")]
                : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchFolders()
        let all = await drain(jawd) { $0.contains { if case .windowFailed = $0 { true } else { false } } }
        let failure = try #require(all.compactMap { if case let .windowFailed(w, f) = $0, w == window { f } else { nil } }.first)
        #expect(failure.code == ErrorCode.unknownMethod)
        fake.drop()
        #expect(await eventually { requests(fake, "folders.watch").count == 2 }, "a jawd back with folders gives the tree")
    }

    @Test func `a folder's list that jawd no longer has, watched again after a drop, fails rather than asking for ever`() async throws {
        let watched = Counted()
        let fake = try FakeJawd { method, params, id in
            guard method == "chats.watch" else { return ordinary(method, params, id) }
            return watched.next() == 1 ? ordinary(method, params, id)
                : [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 9")]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchChats("folder:9", .top(50))
        _ = await drain(jawd) { $0.contains { if case .chats = $0 { true } else { false } } }
        fake.drop()
        let all = await drain(jawd) { $0.contains { if case .windowFailed = $0 { true } else { false } } }
        let failure = try #require(all.compactMap { if case let .windowFailed(w, f) = $0, w == window { f } else { nil } }.first)
        #expect(failure.code == ErrorCode.notFound)
        #expect(requests(fake, "chats.watch").count <= 3, "the first watch, and at most two after the drop")
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "chats.watch").count <= 3, "and none after it failed")
    }

    @Test func `a roster's people come from the smallest sort up, then its pushes, and it grows when moved`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        let window = jawd.watchRoster("folder:9", .top(200))
        var people = People<RosterRow>(folder: "folder:9", place: .top(200))
        for update in rosters(await drain(jawd) { !rosters($0, window).isEmpty }, window) {
            people.apply(update)
        }
        #expect(people.list.map(\.id) == ["person:1", "person:4", "person:5"] && people.total == 6)
        #expect(requests(fake, "roster.watch").first == (try JSONValue.parse(fixture("roster.watch.request.json")))["params"])
        fake.send(FakeJawd.pushed("roster.push.json"))
        for update in rosters(await drain(jawd) { !rosters($0, window).isEmpty }, window) {
            people.apply(update)
        }
        #expect(people.list.map(\.id) == ["person:1", "person:5", "person:4"], "Sofia, online now, before Inês, last seen")
        jawd.move(window, .top(400))
        #expect(await eventually { requests(fake, "watch.move").first?["after"]?.int == 400 })
    }

    @Test func `Move to files the chat in the other folder, and takes it out of this one only once jawd has it there`() async throws {
        let add = HeldBack()
        let fake = try FakeJawd { method, params, id in
            if method == "folder.add" {
                add.id.withLock { $0 = id }
                return []
            }
            return ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        #expect(await eventually { add.id.withLock { $0 } != nil })
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "folder.add") == [.object(["folder": .string("folder:10"), "item": .string("351912345678@s.whatsapp.net")])])
        #expect(requests(fake, "folder.remove").isEmpty, "nothing taken out before jawd has it in the other folder")
        let id = try #require(add.id.withLock { $0 })
        fake.send("{\"id\":\(id),\"result\":{}}")
        let all = await drain(jawd) { notices($0).contains("Ana Ribeiro moved from Family to Cousins") }
        #expect(requests(fake, "folder.remove") == [.object(["folder": .string("folder:9"), "item": .string("351912345678@s.whatsapp.net")])])
        #expect(notices(all) == ["Ana Ribeiro moved from Family to Cousins"], "said once it's out of the old one")
    }

    @Test func `a move whose filing jawd refuses takes nothing out, and says why in jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folder.add" ? [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 10")] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        let all = await drain(jawd) { !notices($0).isEmpty }
        #expect(notices(all) == ["jawd has no folder 10"])
        #expect(await settled(jawd, fake))
        #expect(requests(fake, "folder.remove").isEmpty, "left where it was, never in neither")
    }

    @Test(arguments: ["folder.add", "folder.remove"])
    func `a move cut off by a drop in either half is finished once jawd is back`(half: String) async throws {
        let asked = Counted()
        let fake = try FakeJawd { method, params, id in
            method == half && asked.next() == 1 ? [] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        #expect(await eventually { requests(fake, half).count == 1 })
        fake.drop()
        let all = await drain(jawd) { notices($0).contains("Ana Ribeiro moved from Family to Cousins") }
        #expect(requests(fake, half).count == 2, "asked again once jawd is back: adding and removing are idempotent")
        #expect(requests(fake, "folder.remove").last == .object(["folder": .string("folder:9"), "item": .string("351912345678@s.whatsapp.net")]))
        #expect(notices(all).filter { $0.hasPrefix("Ana Ribeiro") } == ["Ana Ribeiro moved from Family to Cousins"])
    }

    @Test func `a move whose taking out jawd refuses says the chat is in both folders now, in jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "folder.remove" ? [FakeJawd.refusal(id, code: ErrorCode.notFound, "jawd has no folder 9")] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.moveItem("351912345678@s.whatsapp.net", from: "folder:9", to: "folder:10",
                      said: "Ana Ribeiro moved from Family to Cousins", both: "Ana Ribeiro is now in both Cousins and Family")
        let all = await drain(jawd) { !notices($0).isEmpty }
        #expect(notices(all) == ["Ana Ribeiro is now in both Cousins and Family: jawd has no folder 9"])
    }

    @Test func `a name comes back made with its row, renamed with none, or turned down in jawd's words`() async throws {
        let fake = try FakeJawd { method, params, id in
            if method == "folder.create", params["name"]?.string == "Family" {
                return [FakeJawd.refusal(id, code: ErrorCode.notAllowed, "a folder there is called Family already")]
            }
            return foldering(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.folderCreate(name: "Cousins", parent: "folder:9")
        jawd.folderCreate(name: "Family", parent: nil)
        jawd.folderRename("folder:10", name: "Primos")
        let all = await drain(jawd) { all in
            all.count { if case .named = $0 { true } else if case .notNamed = $0 { true } else { false } } == 3
        }
        #expect(all.compactMap { if case let .named(row) = $0 { row?.id ?? "renamed" } else { nil } } == ["folder:10", "renamed"])
        #expect(all.compactMap { if case let .notNamed(why) = $0 { why } else { nil } } == ["a folder there is called Family already"])
        #expect(requests(fake, "folder.create").first == (try JSONValue.parse(fixture("folder.create.request.json")))["params"])
    }

    @Test func `filing and the slots say what was done once jawd has done it, and an emptied slot is sent as null`() async throws {
        let fake = try FakeJawd(script: foldering)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.folderAdd("folder:9", item: "person:4", said: "Inês Gomes filed in Family")
        jawd.favouriteSet(slot: 3, target: nil, said: "⌘3 is free")
        let all = await drain(jawd) { notices($0).count == 2 }
        #expect(notices(all) == ["Inês Gomes filed in Family", "⌘3 is free"])
        #expect(requests(fake, "favourite.set").first == .object(["slot": .number(3), "target": .null]))
    }
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: the build fails with `value of type 'Jawd' has no member 'watchFolders'`, `cannot find 'People' in scope` and `type 'Want' has no member 'folders'`, among others.

- [ ] **Step 3: Write the reading**

In `mac/JawKit/Sources/Client/Incoming.swift`: add `folder` to `Expect`'s cases; add `case folder(FolderRow)` to `Reply`; make `Want`'s first line `case hello, find, chats, messages, folders, roster, nothing`; add to `Incoming`, after `case messages(UInt64, MessagesWatched)`,

```swift
    case folders(UInt64, FoldersWatched)
    case roster(UInt64, RosterWatched)
```

and after `case messagesPush(MessagesPush)`,

```swift
    case foldersPush(FoldersPush)
    case rosterPush(RosterPush)
```

In `IncomingLine.init(from:)`, add to the reply's switch, after `case .messages: …`,

```swift
                case .folders: value = try .folders(id, c.decode(FoldersWatched.self, forKey: .result))
                case .roster: value = try .roster(id, c.decode(RosterWatched.self, forKey: .result))
```

and after `case .reply(.status): …`,

```swift
                case .reply(.folder): value = try .reply(id, .folder(c.decode(FolderMade.self, forKey: .result).folder))
```

and to the push's switch, after `case "messages": …`,

```swift
            case "folders": value = try .foldersPush(c.decode(FoldersPush.self, forKey: .params))
            case "roster": value = try .rosterPush(c.decode(RosterPush.self, forKey: .params))
```

- [ ] **Step 4: Write the windows**

In `mac/JawKit/Sources/Windows/Window.swift`: in `Update`, add after `public var moved: Moved?`

```swift
    /// The folder tree's slots, whole, when they changed.
    public var favourites: [Favourite]?
```

and a last parameter `favourites: [Favourite]? = nil` to its init, set with `self.favourites = favourites`. In `Merged`, add `var favourites: [Favourite]?` after `var moved: Moved?`; in `add(_:ids:)`, after `moved = update.moved ?? moved`, add `favourites = update.favourites ?? favourites`; in `isEmpty`, add `&& favourites == nil`; and in `update()` pass `favourites: favourites`. After `Chats`, add:

```swift
/// A roster: a folder's people from the smallest `sort` up, online first, then by when last seen,
/// then those jawd has no last seen for, as jawd sorts them.
public struct People<R: WindowRow>: Sendable {
    public var folder: String
    public var place: Place
    var rows = Rows<R>()
    /// How many people the folder has.
    public var total: UInt64 = 0
    public var seq: UInt64 = 0
    public var loaded = false

    public init(folder: String, place: Place) {
        self.folder = folder
        self.place = place
    }

    /// Applies an update; returns the ids that left.
    @discardableResult
    public mutating func apply(_ update: Update<R>) -> [String] {
        let gone = rows.apply(remove: update.remove, upsert: update.upsert)
        if let total = update.total { self.total = total }
        if let moved = update.moved { place = moved.place }
        seq = max(seq, update.seq)
        loaded = true
        return gone
    }

    public func get(_ id: String) -> R? {
        rows.byID[id]
    }

    public var count: Int {
        rows.byID.count
    }

    /// From the smallest `sort` up.
    public var list: [R] {
        rows.order.map { rows.byID[$0.id]! }
    }
}
```

In `mac/JawKit/Sources/Client/Client.swift`:

1. After `watchMessages(_:_:)`, add

```swift
    /// Watches the folder tree: every folder and the favourite slots, one window, watched whole.
    public func watchFolders() -> WindowID {
        open(.folders, "", .top(0))
    }

    /// Watches a folder's roster, its people from the top.
    public func watchRoster(_ folder: String, _ place: Place) -> WindowID {
        open(.roster, folder, place)
    }
```

2. In `unwatch(_:)`, after `s.messages[window] = nil`, add `s.folders[window] = nil` and `s.roster[window] = nil`.

3. In `drain()`, make the frame `Frame(events: s.events, chats: s.chats.mapValues { $0.update() }, messages: s.messages.mapValues { $0.update() }, folders: s.folders.mapValues { $0.update() }, roster: s.roster.mapValues { $0.update() })` and add `s.folders.removeAll(keepingCapacity: true)` and `s.roster.removeAll(keepingCapacity: true)` after the other two.

4. In `asking(_:_:again:)`, make the first condition `if let watch = w.watch, !again, w.kind != .folders {`, since `watch.move` doesn't take the tree, and add to the kind's switch, after `case .messages: …`,

```swift
            case .folders:
                data = Lines.encode(id: id, method: "folders.watch", params: FoldersWatch(have: have))
            case .roster:
                // As a chat list, a roster comes whole, its have saying which people left.
                let watch = RosterWatch(folder: w.of, around: place.around, before: place.before, after: place.after, have: have)
                data = Lines.encode(id: id, method: "roster.watch", params: watch)
```

5. In `read(_:_:)`, replace `s.windows[landing.window]?.kind == .messages ? .messages : .chats` with

```swift
                    switch s.windows[landing.window]?.kind {
                    case .messages?: .messages
                    case .folders?: .folders
                    case .roster?: .roster
                    case .chats?, nil: .chats
                    }
```

6. In `take(_:_:)`, after `case let .messagesPush(push): …`, add

```swift
        case let .foldersPush(push):
            if push.unread > 0 { _ = event(.unreadable(Self.unread(push.unread, "folders", push.why)), generation, urgent: false) }
            let update = Update<FolderRow>(seq: push.seq, remove: push.remove, upsert: push.upsert, favourites: push.favourites)
            return windowPush(push.watch, generation) { s, window in s.mergeFolders(window, update) }
        case let .rosterPush(push):
            if push.unread > 0 { _ = event(.unreadable(Self.unread(push.unread, "people", push.why)), generation, urgent: false) }
            let update = Update<RosterRow>(seq: push.seq, remove: push.remove, upsert: push.upsert, total: push.total)
            return windowPush(push.watch, generation) { s, window in s.mergeRoster(window, update) }
```

and after `case let .messages(id, watched): …`,

```swift
        case let .folders(id, watched):
            if watched.unread > 0 { _ = event(.unreadable(Self.unread(watched.unread, "folders", watched.why)), generation, urgent: false) }
            return reply(id, .success(.folders(watched)), generation)
        case let .roster(id, watched):
            if watched.unread > 0 { _ = event(.unreadable(Self.unread(watched.unread, "people", watched.why)), generation, urgent: false) }
            return reply(id, .success(.roster(watched)), generation)
```

7. Add `case folders(FoldersWatched)` and `case roster(RosterWatched)` to `Answer`.

8. In `landed(_:_:_:)`, add to the switch that takes `watch`, after `case let .messages(watched): watch = watched.watch`,

```swift
                case let .folders(watched):
                    watch = watched.watch
                case let .roster(watched):
                    watch = watched.watch
```

and to the switch that merges, after its `.messages` case,

```swift
                case let .folders(watched):
                    s.mergeFolders(window, Update<FolderRow>(seq: watched.seq, remove: watched.remove, upsert: watched.rows,
                                                             favourites: watched.favourites))
                case let .roster(watched):
                    var update = Update<RosterRow>(seq: watched.seq, remove: watched.remove, upsert: watched.rows, total: watched.total)
                    if moved { update.moved = Moved(place: place, newer: 0) }
                    s.mergeRoster(window, update)
```

9. In `fallback(_:_:)`, add `case .folders: return place` and `case .roster: return .top(rows)`. In `landed(_:_:_:)`'s failure case, make the first condition

```swift
                if landing.again == true, gone, Self.fallback(w.kind, place) != place {
```

so a window jawd calls gone at the very place it would fall back to fails, through the `else` below, as `.window`, rather than being asked again at that place for ever: a deleted folder's list, which is at the top or falls back to it, now fails after at most two asks, and the app takes the list back to All (task 5).

10. Make `Kind` `case chats, messages, folders, roster`.

11. In `Frame`, add after `messages`

```swift
    public var folders: [WindowID: Update<FolderRow>] = [:]
    public var roster: [WindowID: Update<RosterRow>] = [:]
```

and make `isEmpty` `events.isEmpty && chats.isEmpty && messages.isEmpty && folders.isEmpty && roster.isEmpty`.

12. In `State`, add `var folders: [WindowID: Merged<FolderRow>] = [:]` and `var roster: [WindowID: Merged<RosterRow>] = [:]` after `messages`, and after `mergeMessages(_:_:)`

```swift
    mutating func mergeFolders(_ window: WindowID, _ update: Update<FolderRow>) {
        guard var w = windows[window], !w.closing else { return }
        w.seq = max(w.seq ?? update.seq, update.seq)
        folders[window, default: Merged()].add(update, ids: &w.ids)
        windows[window] = w
    }

    mutating func mergeRoster(_ window: WindowID, _ update: Update<RosterRow>) {
        guard var w = windows[window], !w.closing else { return }
        w.seq = max(w.seq ?? update.seq, update.seq)
        roster[window, default: Merged()].add(update, ids: &w.ids)
        windows[window] = w
    }
```

- [ ] **Step 5: Write the requests**

In `mac/JawKit/Sources/Client/Jawd.swift`, after `watchMessages(_:_:)`, add

```swift
    public func watchFolders() -> WindowID {
        client.watchFolders()
    }

    public func watchRoster(_ folder: String, _ place: Place) -> WindowID {
        client.watchRoster(folder, place)
    }
```

after `historyMore(_:)`, add

```swift
    // MARK: - Folders and favourites

    /// Makes a folder of yours, named in place: `named` with its row once it's made, or `notNamed`
    /// with jawd's words for the field.
    public func folderCreate(name: String, parent: String?) {
        ask("folder.create", FolderCreate(name: name, parent: parent), .naming, expect: .folder)
    }

    public func folderRename(_ folder: String, name: String) {
        ask("folder.rename", FolderRename(folder: folder, name: name), .naming)
    }

    /// Nests a folder of yours in `parent`, or puts it back at the top, at `position` among its new
    /// siblings or last; `said` once jawd has done it.
    public func folderMove(_ folder: String, parent: String?, position: Int?, said: String?) {
        ask("folder.move", FolderMove(folder: folder, parent: parent, position: position), .filed(said))
    }

    public func folderDelete(_ folder: String, said: String?) {
        ask("folder.delete", FolderID(folder: folder), .filed(said))
    }

    public func folderAdd(_ folder: String, item: String, said: String?) {
        ask("folder.add", FolderItem(folder: folder, item: item), .filed(said))
    }

    public func folderRemove(_ folder: String, item: String, said: String?) {
        ask("folder.remove", FolderItem(folder: folder, item: item), .filed(said))
    }

    /// Files `item` in `to`, and takes it out of `from` only once jawd has it there, so a failure
    /// between the two leaves it in both, never in neither, and says so with `both`; a half whose
    /// answer a drop lost is asked again once jawd is back, since adding and removing are idempotent.
    public func moveItem(_ item: String, from: String, to: String, said: String?, both: String) {
        ask(Moving(item: item, from: from, to: to, said: said, both: both))
    }

    func ask(_ moving: Moving) {
        let folder = moving.removing ? moving.from : moving.to
        ask(moving.removing ? "folder.remove" : "folder.add", FolderItem(folder: folder, item: moving.item), .moving(moving))
    }

    /// Puts a folder or a chat on `slot`, taking it off the slot it had and putting out what the slot
    /// held; nil empties the slot.
    public func favouriteSet(slot: Int, target: String?, said: String?) {
        ask("favourite.set", FavouriteSet(slot: slot, target: target), .filed(said))
    }
```

In `take(_:_:)`, make the window's case `out.append(.windowFailed(window, failure))`. After `Acting`, add

```swift
    /// Move to: `item` filed in `to`, then taken out of `from` once jawd has it there.
    struct Moving {
        let item: String
        let from: String
        let to: String
        let said: String?
        /// What's said if it ends in both folders.
        let both: String
        /// Whether jawd has it in `to`, so taking it out of `from` is what's left.
        var removing = false
    }
```

add `case move(Moving)` to `Again`, and in `askAgain(_:)`, after `case let .act(acting): ask(acting)`,

```swift
            case let .move(moving):
                ask(moving)
```

with no day's limit, since asking again can't do it twice. In `Asked`, add after `case status`

```swift
        /// A folder's name, made or changed in place.
        case naming
        /// A folder change, and what's said once jawd has done it.
        case filed(String?)
        /// A half of Move to.
        case moving(Moving)
```

In `answered(_:_:_:)`, add before `default:`

```swift
        case let (.naming, .folder(row)):
            out.append(.named(row))
        case (.naming, _):
            out.append(.named(nil))
        case let (.filed(said), _):
            if let said { out.append(.notice(said, .info)) }
        case let (.moving(moving), _) where !moving.removing:
            var rest = moving
            rest.removing = true
            ask(rest)
        case let (.moving(moving), _):
            if let said = moving.said { out.append(.notice(said, .info)) }
```

In `refused(_:_:)`, add before `case .status, .quiet:`

```swift
        case .naming:
            return [.notNamed(Self.whyNot(failure, "naming folders"))]
        case .filed:
            return [.notice(Self.whyNot(failure, "filing"), .warning)]
        case let .moving(moving):
            // The second half is asked as the first's answer is read, so a drop can cut it off unsent.
            if failure == .lost || (moving.removing && failure == .unreachable) {
                again.append(.move(moving))
                return []
            }
            if moving.removing {
                return [.notice("\(moving.both): \(Self.whyNot(failure, "taking it out"))", .warning)]
            }
            return [.notice(Self.whyNot(failure, "moving between folders"), .warning)]
```

In `drain()`, after the messages' loop, add

```swift
        for (window, update) in frame.folders.sorted(by: { $0.key < $1.key }) {
            out.append(.folders(window, update))
        }
        for (window, update) in frame.roster.sorted(by: { $0.key < $1.key }) {
            out.append(.roster(window, update))
        }
```

In `JawUpdate`, make `windowFailed` `case windowFailed(WindowID, Failure)`, and add after `case messages(WindowID, Update<MessageRow>)`

```swift
    case folders(WindowID, Update<FolderRow>)
    case roster(WindowID, Update<RosterRow>)
    /// A folder named in place: made, with its row, or renamed, with none.
    case named(FolderRow?)
    /// A name jawd turned down, in its words, for under the field.
    case notNamed(String)
```

In `mac/JawApp/Sources/AppModel.swift`, in `apply(_:)`, replace

```swift
        case let .windowFailed(_, why):
            say("a window couldn't be watched: \(why)", warn: true)
        case .push, .chats:
            break
```

with

```swift
        case let .windowFailed(_, failure):
            say("a window couldn't be watched: \(failure)", warn: true)
        case .push, .chats, .folders, .roster, .named, .notNamed:
            break
```

- [ ] **Step 6: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawKit`
Expected: `** TEST SUCCEEDED **`, with the twelve new tests among those passed and every earlier one still passing.

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 7: Commit**

```bash
git add mac/JawKit/Sources mac/JawApp/Sources/AppModel.swift mac/Tests/JawKitTests
git commit -m "The client watches jawd's folder tree and a folder's roster as windows of their own, and asks for what the folder items do: the tree comes whole with its slots, its pushes merged between frames keeping the newest row, though a folder's counts change without its version, and the newest slots, which a push leaves out when none changed; it's watched again whole after a drop or a resync, saying the folders held and no since, which jawd ignores for the tree; a roster is a window like the chat list's, its people from the smallest sort up in People; a window jawd calls gone at the place it would fall back to, as a deleted folder's list is, fails rather than being asked again there for ever; Move to files an item in the other folder and takes it out of this one only once jawd has answered for the first, so a failure between them leaves it in both, never in neither, a half that a drop cut off is asked again once jawd is back, as lost sends and actions are, since both halves are idempotent, and a second half jawd refuses says the chat is in both folders now; a folder named in place comes back with its row, or with jawd's refusal for the field; filing and the slots say what was done once jawd has done it; and a window jawd refuses carries its failure, so the app can tell a jawd without folders, which turns the tree down with -32601 and is asked again when it's back"
```

---

### Task 3: The folder tree's model

**Waits on jawd:** nothing to build; the tree is made from `Samples` and from updates written as the fixtures have them. A real jawd's tree is task 15's.

**Files:**
- Create: `mac/JawUI/Sources/Sidebar/FolderTree.swift`
- Modify: `mac/JawUI/Sources/Samples.swift` (a tree, its slots, and the chats' folders)
- Test: `mac/Tests/JawUITests/FolderTreeTests.swift`

**Interfaces:**
- Consumes: `FolderRow`, `Favourite`, `Update.favourites` (tasks 1 and 2).
- Produces: `public struct FolderTree: Sendable, Equatable` with `init()` (All and Archived), `init(_ rows: [FolderRow], favourites: [Favourite])`, `rows`, `favourites`, `served`, `apply(_ update: Update<FolderRow>)`, `row(_:)`, `builtIn`, `smart`, `children(of parent: String?) -> [FolderRow]`, `yours: [(row: FolderRow, depth: Int)]`, `order: [String]`, `subtree(_:) -> Set<String>`, `ancestors(_:) -> [String]` (nearest first), `label(_:) -> String`, `slot(of target: String) -> Int?`, `favourite(_ slot: Int) -> Favourite?`, `slotName(_ slot: Int) -> String?`, `badges(_:) -> Badges` (`digit: Int?`, `mention: Bool`, `unread: Int`), and internally `shape`; `Samples.folders`, `Samples.favourites`, `Samples.tree`, `Samples.folderUpdate`.

- [ ] **Step 1: Write the failing tests**

In `mac/JawUI/Sources/Samples.swift`, give the chats folders: `direct` gets a last argument `folders: ["folder:1"]`, `group` `folders: ["folder:3"]`, and Mãe, `muted`, `folders: ["folder:1"]`. Then add, after `chats`:

```swift
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
```

Create `mac/Tests/JawUITests/FolderTreeTests.swift`:

```swift
@testable import JawKit
@testable import JawUI
import Testing

/// The tree as jawd keeps it: the built-in folders, yours nested by parent and position, the smart
/// ones, the nine slots, and what's read from it.

@Test func `before jawd sends its tree there are All and Archived, and its first update replaces them`() {
    var tree = FolderTree()
    #expect(!tree.served && tree.builtIn.map(\.id) == ["all", "archived"] && tree.yours.isEmpty && tree.smart.isEmpty)
    tree.apply(Samples.folderUpdate)
    #expect(tree.served && tree.builtIn.map(\.name) == ["All", "Unread", "Groups", "Communities", "Archived"])
}

@Test func `your folders nest by parent and sit by position, and the smart ones come by theirs`() {
    let tree = Samples.tree
    #expect(tree.children(of: nil).map(\.name) == ["Family", "Climbing", "Friends", "Work"])
    #expect(tree.children(of: "folder:1").map(\.name) == ["Cousins"])
    #expect(tree.yours.map { String(repeating: "·", count: $0.depth) + $0.row.name } == ["Family", "·Cousins", "Climbing", "Friends", "Work", "·Clients"])
    #expect(tree.smart.map(\.name) == ["Waiting on me", "Mentions me", "Muted, unread"])
    #expect(tree.order == ["all", "unread", "groups", "communities", "archived", "folder:1", "folder:2", "folder:3", "folder:4",
                           "folder:5", "folder:6", "smart:1", "smart:2", "smart:3"])
    #expect(tree.subtree("folder:5") == ["folder:5", "folder:6"] && tree.ancestors("folder:2") == ["folder:1"])
}

@Test func `a row's badges: its digit from the slots, an @ for its mentions, its unread total but none for Archived`() {
    let tree = Samples.tree
    #expect(tree.badges("folder:1") == FolderTree.Badges(digit: 1, mention: true, unread: 3))
    #expect(tree.badges("folder:3") == FolderTree.Badges(digit: 3, mention: false, unread: 0))
    #expect(tree.badges("archived").unread == 0, "Archived has no unread total, as the terminal's")
}

@Test func `the slots come whole when one changes, and stay as they are when none did`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, favourites: [Favourite(slot: 1, folder: "folder:3")]))
    #expect(tree.slot(of: "folder:3") == 1 && tree.slot(of: "folder:1") == nil && tree.favourite(2) == nil)
    tree.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:4", v: 60, name: "Friends", position: 2, unread: 5)]))
    #expect(tree.slot(of: "folder:3") == 1, "a push with no slots leaves them")
    #expect(tree.badges("folder:4").unread == 5 && tree.badges("folder:3").digit == 1)
}

@Test func `a row sent again with the same version is the newer, an older one changes nothing, and a removed one goes`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:1", v: 50, name: "Family", position: 0, unread: 9)]))
    #expect(tree.row("folder:1")?.unread == 9)
    tree.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:1", v: 40, name: "Old", position: 0)]))
    #expect(tree.row("folder:1")?.name == "Family")
    tree.apply(Update(seq: 303, remove: ["folder:2"]))
    #expect(tree.children(of: "folder:1").isEmpty)
}

@Test func `a slot says what's on it, and a folder of yours is named by its path where another has its name`() {
    var tree = Samples.tree
    #expect(tree.slotName(1) == "Family" && tree.slotName(2) == "Ana Ribeiro" && tree.slotName(4) == nil)
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:7", v: 61, parent: "folder:4", name: "Cousins", position: 0)]))
    #expect(tree.label("folder:2") == "Family › Cousins" && tree.label("folder:7") == "Friends › Cousins")
    #expect(tree.label("folder:3") == "Climbing" && tree.label("all") == "All")
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `cannot find 'FolderTree' in scope`.

- [ ] **Step 3: Write the tree**

Create `mac/JawUI/Sources/Sidebar/FolderTree.swift`:

```swift
import JawKit

/// The folder tree as jawd keeps it, and what the sidebar, the menus and the keys read from it: the
/// built-in folders, yours nested by parent and position, the smart ones, and the nine favourite
/// slots. Until jawd serves folders (its M6) it has All and Archived, as jawd serves them now.
public struct FolderTree: Sendable, Equatable {
    public private(set) var rows: [String: FolderRow]
    /// The slots held, in their order; the empty ones aren't there.
    public private(set) var favourites: [Favourite] = []
    /// Whether jawd has sent its tree: until it has, the menus have no folder items.
    public private(set) var served = false

    /// All and Archived, before jawd has sent its tree.
    public init() {
        rows = Dictionary(uniqueKeysWithValues: Self.unserved.map { ($0.id, $0) })
    }

    public init(_ rows: [FolderRow], favourites: [Favourite]) {
        self.rows = Dictionary(rows.map { ($0.id, $0) }) { $1 }
        self.favourites = favourites.sorted { $0.slot < $1.slot }
        served = true
    }

    static let unserved = [FolderRow(id: "all", name: "All", position: 0), FolderRow(id: "archived", name: "Archived", position: 4)]

    /// Takes the tree's window's update: rows gone; rows changed, one sent again with the same version
    /// being the newer, as a folder's counts change without it; and the slots, whole, when they
    /// changed. The first replaces All and Archived with jawd's tree.
    public mutating func apply(_ update: Update<FolderRow>) {
        if !served {
            rows = [:]
            served = true
        }
        for id in update.remove {
            rows[id] = nil
        }
        for row in update.upsert {
            if let held = rows[row.id], held.v > row.v { continue }
            rows[row.id] = row
        }
        if let slots = update.favourites { favourites = slots.sorted { $0.slot < $1.slot } }
    }

    public func row(_ id: String) -> FolderRow? {
        rows[id]
    }

    static func ordered(_ rows: some Sequence<FolderRow>) -> [FolderRow] {
        rows.sorted { ($0.position, $0.name, $0.id) < ($1.position, $1.name, $1.id) }
    }

    /// The built-in folders, in their order.
    public var builtIn: [FolderRow] {
        Self.ordered(rows.values.filter(\.isBuiltIn))
    }

    /// The smart folders, in their order.
    public var smart: [FolderRow] {
        Self.ordered(rows.values.filter(\.isSmart))
    }

    /// Your folders in `parent`, or at the top for nil, in their order.
    public func children(of parent: String?) -> [FolderRow] {
        Self.ordered(rows.values.filter { $0.isYours && $0.parent == parent })
    }

    /// Your folders in the tree's order with every one open, each with how deep it is: Add to Folder's
    /// and Move to's order.
    public var yours: [(row: FolderRow, depth: Int)] {
        var out: [(row: FolderRow, depth: Int)] = []
        var seen: Set<String> = []
        func walk(_ parent: String?, _ depth: Int) {
            for row in children(of: parent) where seen.insert(row.id).inserted {
                out.append((row, depth))
                walk(row.id, depth + 1)
            }
        }
        walk(nil, 0)
        return out
    }

    /// Every folder in the tree's order with every one open: the built-in ones, yours, the smart ones.
    public var order: [String] {
        builtIn.map(\.id) + yours.map(\.row.id) + smart.map(\.id)
    }

    /// A folder and every folder under it.
    public func subtree(_ id: String) -> Set<String> {
        var out: Set<String> = [id]
        var next = [id]
        while let parent = next.popLast() {
            for row in rows.values where row.parent == parent && out.insert(row.id).inserted {
                next.append(row.id)
            }
        }
        return out
    }

    /// The folders a folder is in, nearest first.
    public func ancestors(_ id: String) -> [String] {
        var out: [String] = []
        var at = rows[id]?.parent
        while let parent = at, parent != id, !out.contains(parent) {
            out.append(parent)
            at = rows[parent]?.parent
        }
        return out
    }

    /// A folder as the palette and the status line name it: its name, or, for one of yours whose name
    /// another of yours has, its path from the top, as "Family › Cousins".
    public func label(_ id: String) -> String {
        guard let row = rows[id] else { return id }
        guard row.isYours, rows.values.contains(where: { $0.isYours && $0.id != id && $0.name == row.name }) else { return row.name }
        return (ancestors(id).reversed().compactMap { rows[$0]?.name } + [row.name]).joined(separator: " › ")
    }

    /// The slot that holds a folder or a chat.
    public func slot(of target: String) -> Int? {
        favourites.first { $0.target == target }?.slot
    }

    public func favourite(_ slot: Int) -> Favourite? {
        favourites.first { $0.slot == slot }
    }

    /// What's on a slot, as its menus say it: the folder's name, or the chat's.
    public func slotName(_ slot: Int) -> String? {
        guard let f = favourite(slot) else { return nil }
        if let folder = f.folder { return rows[folder] != nil ? label(folder) : folder }
        return f.name ?? f.chat
    }

    /// What a folder's row shows beside its name.
    public struct Badges: Equatable, Sendable {
        /// Its ⌘ digit, from the slots.
        public var digit: Int?
        public var mention: Bool
        /// Its unread total; none for Archived, as the terminal's.
        public var unread: Int
    }

    public func badges(_ id: String) -> Badges {
        let row = rows[id]
        return Badges(digit: slot(of: id), mention: (row?.mentions ?? 0) > 0, unread: id == "archived" ? 0 : row?.unread ?? 0)
    }

    /// What lays the tree out: each folder's place and name, and whether it's smart. A change to
    /// anything else, such as its counts, draws its row again where it is.
    var shape: [String: String] {
        rows.mapValues { "\($0.parent ?? "")\u{1}\($0.position)\u{1}\($0.name)\u{1}\($0.isSmart)" }
    }
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, with the six new tests among those passed, and `** BUILD SUCCEEDED **`.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources/Sidebar/FolderTree.swift mac/JawUI/Sources/Samples.swift mac/Tests/JawUITests/FolderTreeTests.swift
git commit -m "JawUI keeps jawd's folder tree as a value that the sidebar, the menus and the keys read: All and Archived until jawd sends it, then the built-in folders, yours nested by parent and position, the smart ones by theirs, and the nine favourite slots, which come whole when one changes; a row sent again with the same version is the newer, as a folder's counts change without it, an older one changes nothing and a removed one goes; a slot says what's on it, a folder of yours whose name another of yours has is named by its path, and a row's badges are its ⌘ digit, an @ for its mentions and its unread total, none for Archived; Samples gains the tree the tests and the harness draw"
```

---

### Task 4: The tree in the sidebar

**Waits on jawd:** nothing to build; the sidebar is drawn from `Samples`. On `--demo` before `folders` lands, the snapshot checks the fallback: All and Archived, nothing said. A real jawd's tree in the sidebar is task 15's.

**Files:**
- Rewrite: `mac/JawUI/Sources/Sidebar/SidebarController.swift`
- Create: `mac/JawUI/Sources/Sidebar/SidebarCells.swift`
- Create: `mac/JawApp/Sources/Folders.swift` (the app's folders: the sidebar's host)
- Modify: `mac/JawApp/Sources/AppModel.swift` (watching the tree, `pick(folder:)`), `MainWindowController.swift` (the sidebar's host and colours, the palette's folders), `Commands.swift` (the sidebar command's words), `Bench.swift` (`sidebar`, `folder:<row>`), `mac/README.md`
- Create: `mac/JawHarness/Sources/Scenes/SidebarScenes.swift`; modify `mac/JawHarness/Sources/BuiltInScenes.swift`, `mac/Tests/JawHarnessTests/ScenesTests.swift`
- Test: `mac/Tests/JawUITests/SidebarTests.swift`

**Interfaces:**
- Consumes: `FolderTree`, `Samples.tree`, `Samples.folderUpdate` (task 3); `Jawd.watchFolders()`, `JawUpdate.folders`, `JawUpdate.windowFailed(WindowID, Failure)` (task 2).
- Produces: `public protocol SidebarHost` with `sidebar(_:pick:)`; `SidebarController` with `host`, `tree`, `picked`, `roles`, `keepsOpen: String?`, `apply(_:)`, `select(_:)`, `pick(row:) -> Bool`, `expand(_:)`, `said: [String]`, and internally `outline`, `items`, `item(_:)`, `children(_:)`, `restoreOpen()`, `reselect()`, `foldersHeading`, `smartHeading`; `FolderCell`, `HeadingCell`; `AppModel.foldersWindow`, `AppModel.tree`; the snapshot's `sidebar` and `folder:<row>` steps.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/SidebarTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The sidebar draws jawd's folder tree: the built-in folders, then Folders, yours nested, then Smart
/// Folders, each with its counts, an @ and its ⌘ digit; picking a row picks its folder.

@MainActor
final class SideHost: SidebarHost {
    var picked: [String] = []

    func sidebar(_: SidebarController, pick folder: String) {
        picked.append(folder)
    }
}

@MainActor
func sidebar(_ host: SideHost? = nil) -> SidebarController {
    let s = SidebarController()
    s.host = host
    s.roles = Catalog.jaw.roles(dark: true)
    _ = s.view
    s.view.frame = NSRect(x: 0, y: 0, width: 220, height: 600)
    return s
}

@MainActor
@Test func `before jawd serves folders, the sidebar has All and Archived`() {
    let s = sidebar()
    #expect(s.said == ["All", "Archived"] && s.picked == "all")
}

@MainActor
@Test func `the tree comes under its headings, yours nested, with counts, an @ and the ⌘ digits from the slots`() {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    #expect(s.said == ["All @ 12", "Unread @ 12", "Groups @ 5", "Communities 1", "Archived", "Folders:", "Family ⌘1 @ 3 ▸", "Climbing ⌘3",
                       "Friends 2", "Work ▸", "Smart Folders:", "Waiting on me 2", "Mentions me @ 1", "Muted, unread"])
    s.expand("folder:1")
    #expect(Array(s.said[6 ... 7]) == ["Family ⌘1 @ 3 ▾", "  Cousins 1"])
}

@MainActor
@Test func `picking a row picks its folder, and the app's own pick selects it without asking again`() {
    let host = SideHost()
    let s = sidebar(host)
    s.apply(Samples.folderUpdate)
    #expect(s.pick(row: 7))
    #expect(host.picked == ["folder:3"] && s.picked == "folder:3")
    #expect(!s.pick(row: 5), "a heading isn't a folder")
    s.select("folder:2")
    #expect(s.picked == "folder:2" && host.picked == ["folder:3"], "not asked for again")
    #expect(s.said[6] == "Family ⌘1 @ 3 ▾", "the folder it's in opened to show it")
}

@MainActor
@Test func `counts that change draw their rows again, and a folder that comes lays the tree out again, keeping what's open`() {
    let s = sidebar()
    let window = NSWindow(contentViewController: s)
    defer { withExtendedLifetime(window) {} }
    s.apply(Samples.folderUpdate)
    s.expand("folder:1")
    s.view.layoutSubtreeIfNeeded()
    // `said` reads the tree, so the drawn cell is what shows the row was drawn again.
    let cousins = { (s.outline.view(atColumn: 0, row: 7, makeIfNecessary: true) as? FolderCell)?.count.stringValue }
    #expect(cousins() == "1" && s.outline.rowView(atRow: 7, makeIfNecessary: false) != nil, "Cousins' row is on screen")
    s.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:2", v: 50, parent: "folder:1", name: "Cousins", position: 0, unread: 4)]))
    #expect(cousins() == "4", "the row on screen drawn again, not only the tree changed")
    s.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:8", v: 62, parent: "folder:1", name: "Parents", position: 1)]))
    #expect(Array(s.said[6 ... 8]) == ["Family ⌘1 @ 3 ▾", "  Cousins 4", "  Parents"])
}
```

In `mac/Tests/JawHarnessTests/ScenesTests.swift`, put `"sidebar"` after `"chat-list"` in the list `every scene draws as wide as it says…` expects.

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `cannot find type 'SidebarHost' in scope`.

- [ ] **Step 3: Write the sidebar**

Replace `mac/JawUI/Sources/Sidebar/SidebarController.swift` with:

```swift
import AppKit
import JawKit
import JawThemes

@MainActor
public protocol SidebarHost: AnyObject {
    /// A folder picked in the sidebar: the chat list becomes its window.
    func sidebar(_ sidebar: SidebarController, pick folder: String)
}

/// The sidebar: the folder tree, as jawd's M6 serves it, on the sidebar's glass. The built-in folders
/// come first, without a heading; then Folders, yours, nested, each opened and closed with its
/// triangle; then Smart Folders. Each row has its unread total (none for Archived), an @ when it has
/// mentions, and its ⌘ digit when it's a favourite. Picking a row makes the chat list its window.
/// Until jawd serves folders it has All and Archived, as jawd serves them now.
@MainActor
public final class SidebarController: NSViewController, NSOutlineViewDataSource, NSOutlineViewDelegate {
    public weak var host: SidebarHost?
    public internal(set) var tree = FolderTree()
    /// The folder the chat list shows, selected whenever its row is in sight.
    public internal(set) var picked = "all"
    public var roles: Roles? {
        didSet {
            guard isViewLoaded else { return }
            outline.reloadData()
            restoreOpen()
        }
    }

    /// The defaults key your folders left open are kept under from launch to launch: the app gives one,
    /// the harness and the tests none, so what they draw doesn't depend on what was opened before.
    public var keepsOpen: String? {
        didSet {
            guard let keepsOpen, let kept = UserDefaults.standard.stringArray(forKey: keepsOpen) else { return }
            open = Set(kept)
            if isViewLoaded { restoreOpen() }
        }
    }

    let outline = NSOutlineView()
    let scroll = NSScrollView()
    /// One object for each row's id for as long as the sidebar lives: the outline knows its rows by
    /// identity, so what's open stays open as the tree is laid out again.
    var items: [String: Item] = [:]
    /// Your folders left open.
    var open: Set<String> = []
    /// While a folder closes, the selection AppKit moves isn't a pick.
    var collapsing = false

    final class Item: NSObject {
        let id: String

        init(_ id: String) {
            self.id = id
        }
    }

    static let foldersHeading = "heading:folders"
    static let smartHeading = "heading:smart"

    override public func loadView() {
        let column = NSTableColumn(identifier: .init("folder"))
        outline.addTableColumn(column)
        outline.outlineTableColumn = column
        outline.headerView = nil
        outline.style = .sourceList
        outline.rowHeight = 28
        outline.floatsGroupRows = false
        outline.dataSource = self
        outline.delegate = self
        outline.setAccessibilityLabel("Folders")
        Scrolling.style(scroll)
        scroll.documentView = outline
        scroll.drawsBackground = false
        view = scroll
        outline.reloadData()
        restoreOpen()
    }

    // MARK: The tree

    /// jawd's tree, changed. A row whose counts alone changed is drawn again where it is; a change to
    /// the tree's shape lays it out again, keeping what's open, so a flood of messages costs the
    /// sidebar a few rows' drawing.
    public func apply(_ update: Update<FolderRow>) {
        let before = tree
        tree.apply(update)
        guard isViewLoaded else { return }
        if before.shape != tree.shape || before.served != tree.served {
            outline.reloadData()
            restoreOpen()
            return
        }
        for id in tree.rows.keys where before.row(id) != tree.row(id) || before.badges(id) != tree.badges(id) {
            if let item = items[id] { outline.reloadItem(item) }
        }
    }

    /// The app picked `folder`, from a key, a favourite or the palette: its row is selected, the
    /// folders it's in opened, and the host isn't asked again.
    public func select(_ folder: String) {
        picked = folder
        guard isViewLoaded else { return }
        for id in tree.ancestors(folder).reversed() {
            outline.expandItem(item(id))
        }
        reselect()
    }

    /// Picks the folder on `row`, as a click does; false when the row isn't a folder.
    @discardableResult
    public func pick(row: Int) -> Bool {
        guard row >= 0, let id = (outline.item(atRow: row) as? Item)?.id, tree.row(id) != nil else { return false }
        outline.selectRowIndexes([row], byExtendingSelection: false)
        return true
    }

    /// Opens a folder of yours to show the folders in it, the ones it's in first.
    public func expand(_ id: String) {
        for a in tree.ancestors(id).reversed() + [id] {
            outline.expandItem(item(a))
        }
    }

    /// The tree as it's drawn, a row a line, for the tests and the snapshot: a heading with a colon, a
    /// folder indented under the one it's in, then its ⌘ digit, an @, its unread total, and ▸ or ▾ when
    /// it has folders in it.
    public var said: [String] {
        (0 ..< outline.numberOfRows).compactMap { row in
            guard let id = (outline.item(atRow: row) as? Item)?.id else { return nil }
            if id == Self.foldersHeading { return "Folders:" }
            if id == Self.smartHeading { return "Smart Folders:" }
            guard let folder = tree.row(id) else { return nil }
            let b = tree.badges(id)
            var line = String(repeating: "  ", count: max(outline.level(forRow: row) - 1, 0)) + folder.name
            if let digit = b.digit { line += " ⌘\(digit)" }
            if b.mention { line += " @" }
            if b.unread > 0 { line += " \(Format.num(b.unread))" }
            if outline.isExpandable(items[id]) { line += outline.isItemExpanded(items[id]) ? " ▾" : " ▸" }
            return line
        }
    }

    func reselect() {
        let row = outline.row(forItem: items[picked])
        if row >= 0 {
            outline.selectRowIndexes([row], byExtendingSelection: false)
        } else {
            outline.deselectAll(nil)
        }
    }

    /// The headings open, your folders as they were left, and the picked folder selected.
    func restoreOpen() {
        for heading in [Self.foldersHeading, Self.smartHeading] {
            outline.expandItem(item(heading))
        }
        for (row, _) in tree.yours where open.contains(row.id) {
            outline.expandItem(item(row.id))
        }
        reselect()
    }

    // MARK: The outline

    func item(_ id: String) -> Item {
        if let held = items[id] { return held }
        let made = Item(id)
        items[id] = made
        return made
    }

    static func isHeading(_ id: String?) -> Bool {
        id == foldersHeading || id == smartHeading
    }

    /// The ids under `parent`: the built-in folders and the headings at the top, your folders under
    /// Folders and under each other, the smart ones under Smart Folders.
    func children(_ parent: String?) -> [String] {
        switch parent {
        case nil:
            tree.builtIn.map(\.id) + (tree.children(of: nil).isEmpty ? [] : [Self.foldersHeading])
                + (tree.smart.isEmpty ? [] : [Self.smartHeading])
        case .some(Self.foldersHeading): tree.children(of: nil).map(\.id)
        case .some(Self.smartHeading): tree.smart.map(\.id)
        case let .some(id): tree.children(of: id).map(\.id)
        }
    }

    public func outlineView(_: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        children((item as? Item)?.id).count
    }

    public func outlineView(_: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        self.item(children((item as? Item)?.id)[index])
    }

    public func outlineView(_: NSOutlineView, isItemExpandable item: Any) -> Bool {
        !children((item as? Item)?.id).isEmpty
    }

    public func outlineView(_: NSOutlineView, isGroupItem item: Any) -> Bool {
        Self.isHeading((item as? Item)?.id)
    }

    public func outlineView(_: NSOutlineView, shouldSelectItem item: Any) -> Bool {
        !Self.isHeading((item as? Item)?.id)
    }

    public func outlineView(_ outlineView: NSOutlineView, viewFor _: NSTableColumn?, item: Any) -> NSView? {
        guard let id = (item as? Item)?.id else { return nil }
        if Self.isHeading(id) {
            let cell = (outlineView.makeView(withIdentifier: HeadingCell.id, owner: self) as? HeadingCell) ?? HeadingCell()
            cell.show(id == Self.foldersHeading ? "Folders" : "Smart Folders", roles: roles)
            return cell
        }
        let cell = (outlineView.makeView(withIdentifier: FolderCell.id, owner: self) as? FolderCell) ?? FolderCell()
        cell.show(tree.row(id), badges: tree.badges(id), roles: roles)
        return cell
    }

    public func outlineViewSelectionDidChange(_: Notification) {
        guard !collapsing, let id = (outline.item(atRow: outline.selectedRow) as? Item)?.id, id != picked, tree.row(id) != nil else {
            return
        }
        picked = id
        host?.sidebar(self, pick: id)
    }

    public func outlineViewItemWillCollapse(_: Notification) {
        collapsing = true
    }

    public func outlineViewItemDidCollapse(_ note: Notification) {
        collapsing = false
        keep(note, open: false)
        reselect()
    }

    public func outlineViewItemDidExpand(_ note: Notification) {
        keep(note, open: true)
    }

    func keep(_ note: Notification, open now: Bool) {
        guard let id = (note.userInfo?["NSObject"] as? Item)?.id, tree.row(id)?.isYours == true else { return }
        if now { open.insert(id) } else { open.remove(id) }
        if let keepsOpen { UserDefaults.standard.set(open.sorted(), forKey: keepsOpen) }
    }
}
```

Create `mac/JawUI/Sources/Sidebar/SidebarCells.swift`:

```swift
import AppKit
import JawKit
import JawThemes

/// A folder's row: its symbol and its name, then, on the right, its ⌘ digit, an @ when it has
/// mentions and its unread total.
final class FolderCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("folder")
    let symbol = NSImageView()
    let name = Labels.make("", .body)
    let digit = Labels.make("", .caption)
    let mention = Labels.make("@", .caption)
    let count = Labels.make("", .caption)

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        imageView = symbol
        textField = name
        mention.font = TextStyle.caption.emphasised
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let stack = NSStackView(views: [symbol, name, digit, mention, count])
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ row: FolderRow?, badges: FolderTree.Badges, roles: Roles?) {
        guard let row else { return }
        symbol.image = NSImage(systemSymbolName: Self.symbol(for: row), accessibilityDescription: nil)
        name.stringValue = row.name
        digit.stringValue = badges.digit.map { "⌘\($0)" } ?? ""
        digit.isHidden = badges.digit == nil
        mention.isHidden = !badges.mention
        count.stringValue = badges.unread > 0 ? Format.num(badges.unread) : ""
        count.isHidden = badges.unread == 0
        if let roles {
            Labels.style(name, .primary, roles: roles)
            Labels.style(digit, .tertiary, roles: roles)
            mention.textColor = roles.mention.nsColor
            Labels.style(count, .secondary, roles: roles)
        }
        setAccessibilityLabel([row.name, badges.unread > 0 ? "\(badges.unread) unread" : nil, badges.mention ? "mentions you" : nil,
                               badges.digit.map { "favourite \($0)" }].compactMap(\.self).joined(separator: ", "))
    }

    /// A built-in folder's symbol, a smart folder's, or a folder of yours'.
    static func symbol(for row: FolderRow) -> String {
        switch row.id {
        case "all": "bubble.left.and.bubble.right"
        case "unread": "message.badge"
        case "groups": "person.3"
        case "communities": "person.2.circle"
        case "archived": "archivebox"
        default: row.isSmart ? "folder.badge.gearshape" : "folder"
        }
    }
}

/// Folders or Smart Folders, over their rows; the roster's title too.
final class HeadingCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("heading")
    let title = Labels.make("", .caption)

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        title.font = TextStyle.caption.emphasised
        title.translatesAutoresizingMaskIntoConstraints = false
        addSubview(title)
        NSLayoutConstraint.activate([
            title.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            title.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Space.s),
            title.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ text: String, roles: Roles?) {
        title.stringValue = text
        if let roles { Labels.style(title, .tertiary, roles: roles) }
    }
}
```

- [ ] **Step 4: Run the JawUI tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the four new tests among those passed.

- [ ] **Step 5: Wire the app**

Create `mac/JawApp/Sources/Folders.swift`:

```swift
import AppKit
import JawKit
import JawUI

/// The app's half of the folders: the tree and the roster watched, the sidebar's host, and what the
/// folder items do, asked of jawd.
extension AppModel: SidebarHost {
    /// The tree the sidebar holds, which the menus, the keys and the palette read.
    var tree: FolderTree {
        window?.sidebar.tree ?? FolderTree()
    }

    func sidebar(_: SidebarController, pick folder: String) {
        pick(folder: folder)
    }
}
```

In `mac/JawApp/Sources/AppModel.swift`: add `var foldersWindow: WindowID?` after `var listWindow: WindowID?`; in `start()`, after `listWindow = jawd.watchChats(…)`, add `foldersWindow = jawd.watchFolders()`; in `apply(_:)`, before `case let .windowFailed(_, failure):`, add

```swift
        case let .folders(w, u) where w == foldersWindow:
            window?.sidebar.apply(u)
        case let .windowFailed(w, failure) where w == foldersWindow:
            // A jawd from before M6 doesn't know folders.watch: the sidebar keeps All and Archived.
            if failure.code != ErrorCode.unknownMethod { say("the folders couldn't be watched: \(failure)", warn: true) }
```

and in `pick(folder:)`, after `folder = id`, add `window?.sidebar.select(id)`.

In `mac/JawApp/Sources/MainWindowController.swift`, replace `sidebar.picked = { [weak model] folder in model?.pick(folder: folder.id) }` with

```swift
        sidebar.host = model
        sidebar.keepsOpen = "jaw.sidebar.open"
```

in `restyle()` add `sidebar.roles = model.roles`, and in `picked(_:)` make the folder's case

```swift
        case let .folder(f):
            if sidebar.tree.row(f.id) != nil { model.pick(folder: f.id) } else { model.say("folders come with jawd's M6") }
```

If `WindowContent` is in the checkout by then, the same three changes go in the same places: `MainWindowController.init`'s wiring, its `restyle()`, and `picked(_:)`.

In `mac/JawApp/Sources/Commands.swift`, make the `sidebar` command's detail `"the folders and the people"`.

In `mac/JawApp/Sources/Bench.swift`, in `press(_:model:then:)`: add `|| key == "sidebar" || key.hasPrefix("folder:")` to `step`; after the `drop:` block, add

```swift
            if key.hasPrefix("folder:"), let i = Int(key.dropFirst(7)), !(model.window?.sidebar.pick(row: i) ?? false) {
                FileHandle.standardError.write(Data("\(key): no folder at row \(i)\n".utf8))
            }
```

in the log, between the `menu:` branch and the panes' line, add

```swift
                } else if key == "sidebar", let sidebar = model.window?.sidebar {
                    FileHandle.standardError.write(Data("sidebar: \(sidebar.said.joined(separator: " | ")) · picked \(sidebar.picked)\n".utf8))
```

and add `" · folder \(model.folder)"` to the end of the panes' line, before its `\(said)`.

In `mac/README.md`, in the paragraph on the app's options, after the `menu:` steps, add: "`sidebar`, which prints the folder tree as it's drawn, a row a line, and the folder picked, and `folder:<row>`, which picks the tree's row as a click does".

- [ ] **Step 6: Write the harness's scene**

Create `mac/JawHarness/Sources/Scenes/SidebarScenes.swift`:

```swift
import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var sidebar: HarnessScene {
        HarnessScene(
            id: "sidebar", title: "Sidebar", symbol: "sidebar.left",
            synopsis: "The folder tree: the built-in folders, yours nested under Folders, the smart ones, each with its counts, an @ and its ⌘ digit",
            section: .chatList, covers: [SidebarController.self], width: 520,
        ) { SidebarScene(roles: $0.roles) }
    }
}

/// The sidebar before jawd serves folders, and with `Samples`' tree, Family picked and open.
private struct SidebarScene: View {
    let roles: Roles

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "Before jawd serves folders",
                note: "All and Archived, as a jawd from main serves them, with no counts and nothing else.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    return s
                }
                .frame(width: 220, height: 100)
            }
            SpecimenGroup(
                title: "The tree, Family picked",
                note: "All with an @ and 12, Unread with an @ and 12, Groups with an @ and 5, Communities with 1, Archived with no count; under Folders, Family with ⌘1, an @ and 3, open to show Cousins with 1, then Climbing with ⌘3, Friends with 2, and Work, closed, with a triangle for Clients in it; under Smart Folders, Waiting on me with 2, Mentions me with an @ and 1, and Muted, unread. The digits, @s and counts line up on the right, and nothing is cut off. The selection's colour is the window's, which a picture may not show.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    s.apply(Samples.folderUpdate)
                    s.expand("folder:1")
                    s.select("folder:1")
                    return s
                }
                .frame(width: 220, height: 440)
            }
        }
    }
}
```

In `mac/JawHarness/Sources/BuiltInScenes.swift`, add `catalog.register(.sidebar)` after `catalog.register(.chatList)`.

- [ ] **Step 7: Run every suite and build the app**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac test JawHarnessTests`, then `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, `** TEST SUCCEEDED **` and `** BUILD SUCCEEDED **`.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac shot sidebar /tmp/jaw-mac-folders/sidebar.png`, and look at the picture: both bands as their notes say.

- [ ] **Step 8: See the fallback on the demo**

The snapshot runs `target/debug/jawd --engine demo`, built here from this checkout, whose jawd is main's and doesn't serve folders. If `target/debug/jawd` isn't in the worktree, run `df -h .` (a debug build of jawd and its dependencies takes several gigabytes), then `CARGO_INCREMENTAL=0 cargo build -p jawd --bin jawd`; it's kept for the later tasks' snapshots and deleted when tasks 1 to 14 land (task 15 builds it again from a main with folders). Never copy another checkout's.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/sidebar-demo.png --keys "sidebar,folder:1,sidebar"`
Expected, among the log's lines: `sidebar: All | Archived · picked all`; then the panes' line for `folder:1`, with `· folder archived` in it; then `sidebar: All | Archived · picked archived`; and no line with `a window couldn't be watched` or `the folders couldn't be watched`.

- [ ] **Step 9: Commit**

```bash
git add mac/JawUI/Sources/Sidebar mac/JawApp/Sources mac/README.md mac/JawHarness/Sources mac/Tests/JawUITests/SidebarTests.swift mac/Tests/JawHarnessTests/ScenesTests.swift
git commit -m "The sidebar draws jawd's folder tree: the built-in folders first without a heading, then Folders, yours nested by parent and position, each opened and closed with its triangle and kept so from launch to launch, then Smart Folders, each row with its unread total, none for Archived, an @ when it has mentions and its ⌘ digit from the favourite slots; a push that changes only counts draws those rows again and one that changes the tree's shape lays it out again; picking a row makes the chat list its folder's window, and a folder the app picks is selected, the folders it's in opened; the app watches the tree at launch, and from a jawd that doesn't serve it yet keeps All and Archived and says nothing; the palette's folders are picked as the sidebar's are; the harness draws the sidebar before jawd's tree and with it; and the snapshot's sidebar and folder:<row> steps print the tree and pick a row"
```

---

### Task 5: A folder's chats

**Waits on jawd:** nothing to build. A folder of yours filling and emptying on a real jawd, and one deleted from another interface leaving the list on All, are tried in task 15.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/FolderTree.swift` (`emptyLine(_:)`, `showing(_:)`)
- Modify: `mac/JawUI/Sources/ChatList/ChatListController.swift` (`emptyLine`, `emptyLabel`)
- Modify: `mac/JawApp/Sources/AppModel.swift` (`pick(folder:)`, `apply(_:)`, `stateView`)
- Modify: `mac/JawHarness/Sources/Scenes/SidebarScenes.swift`, `mac/JawHarness/Sources/BuiltInScenes.swift`, `mac/Tests/JawHarnessTests/ScenesTests.swift`
- Test: `mac/Tests/JawUITests/FolderTreeTests.swift`, `mac/Tests/JawUITests/ChatListTests.swift`

**Interfaces:**
- Consumes: `FolderTree` (task 3), `SidebarController.select(_:)`, `AppModel.tree` (task 4).
- Produces: `FolderTree.emptyLine(_ id: String) -> String?` and `FolderTree.showing(_ folder: String) -> String`; `ChatListController.emptyLine: String?` and its internal `emptyLabel`; the harness's `empty-folder` scene.

- [ ] **Step 1: Write the failing tests**

Add to `mac/Tests/JawUITests/FolderTreeTests.swift`:

```swift
@Test func `an empty folder says so in the terminal's words, and All says nothing, the panes' place speaking for it`() {
    let tree = Samples.tree
    #expect(tree.emptyLine("folder:1") == "No chats in this folder." && tree.emptyLine("unread") == "No chats in this folder.")
    #expect(tree.emptyLine("smart:2") == "No chats in this folder.")
    #expect(tree.emptyLine("all") == nil && tree.emptyLine("folder:99") == nil)
}

@Test func `the list keeps its folder while the tree has it, and goes back to All once the folder's gone`() {
    var tree = FolderTree()
    #expect(tree.showing("archived") == "archived" && tree.showing("folder:1") == "folder:1", "nothing's gone before jawd's tree")
    tree.apply(Samples.folderUpdate)
    #expect(tree.showing("folder:1") == "folder:1" && tree.showing("smart:2") == "smart:2")
    tree.apply(Update(seq: 301, remove: ["folder:1"]))
    #expect(tree.showing("folder:1") == "all", "deleted from another interface")
    #expect(tree.showing("folder:2") == "folder:2", "a folder that was in it is still there")
}
```

Add to `mac/Tests/JawUITests/ChatListTests.swift`:

```swift
@MainActor
@Test func `an empty folder's list says so once its window has come, and not while it comes or once it has chats`() {
    let list = ChatListController(folder: "folder:1", roles: Catalog.jaw.roles(dark: true))
    _ = list.view
    list.emptyLine = Samples.tree.emptyLine("folder:1")
    #expect(list.emptyLabel.isHidden, "nothing said before the window comes")
    list.apply(Update(seq: 1, upsert: [], total: 0))
    #expect(!list.emptyLabel.isHidden && list.emptyLabel.stringValue == "No chats in this folder.")
    list.apply(Update(seq: 2, upsert: [Samples.direct], total: 1))
    #expect(list.emptyLabel.isHidden)
}
```

and add `@testable import JawKit` to that file's imports if it isn't there. In `mac/Tests/JawHarnessTests/ScenesTests.swift`, put `"empty-folder"` after `"sidebar"` in the expected list.

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `value of type 'FolderTree' has no member 'emptyLine'`, `value of type 'FolderTree' has no member 'showing'` and `value of type 'ChatListController' has no member 'emptyLine'`.

- [ ] **Step 3: Write the code**

Add to `FolderTree`, after `badges(_:)`:

```swift
    /// What the chat list says while a folder has no chats, in the terminal's words; none for All,
    /// whose empty list the panes' place speaks for.
    public func emptyLine(_ id: String) -> String? {
        id == "all" || rows[id] == nil ? nil : "No chats in this folder."
    }

    /// The folder the chat list shows once the tree has changed: the one it showed, or All once that
    /// one's gone, as when another interface deletes it.
    public func showing(_ folder: String) -> String {
        !served || rows[folder] != nil ? folder : "all"
    }
```

In `mac/JawUI/Sources/ChatList/ChatListController.swift`, add after `var avatars = AvatarCache()`:

```swift
    /// What the list says once its window has come with no chats in it: its folder's empty line.
    public var emptyLine: String? {
        didSet { showEmpty() }
    }

    let emptyLabel = Labels.make("", .callout, .secondary)
```

In `loadView()`, after `root.addSubview(scroll)`, add

```swift
        emptyLabel.alignment = .center
        emptyLabel.maximumNumberOfLines = 0
        emptyLabel.lineBreakMode = .byWordWrapping
        emptyLabel.isHidden = true
        emptyLabel.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(emptyLabel)
```

and to its constraints

```swift
            emptyLabel.topAnchor.constraint(equalTo: root.safeAreaLayoutGuide.topAnchor, constant: Space.xxl),
            emptyLabel.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: Space.xl),
            emptyLabel.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -Space.xl),
```

In `applyTheme()`, add `Labels.style(emptyLabel, .secondary, roles: roles)`; at the end of `rebuild()`, add `showEmpty()`; and add

```swift
    func showEmpty() {
        guard isViewLoaded else { return }
        emptyLabel.stringValue = emptyLine ?? ""
        emptyLabel.isHidden = !(chats.loaded && shown.isEmpty && emptyLine != nil)
    }
```

In `mac/JawApp/Sources/AppModel.swift`: in `pick(folder:)`, before `window?.chatList.show(folder: id)`, add `window?.chatList.emptyLine = tree.emptyLine(id)`; make the `.folders` case for the tree

```swift
        case let .folders(w, u) where w == foldersWindow:
            window?.sidebar.apply(u)
            let showing = tree.showing(folder)
            if showing != folder {
                pick(folder: showing)
            } else {
                window?.chatList.emptyLine = tree.emptyLine(folder)
            }
```

add, after the tree's `.windowFailed` case,

```swift
        case let .windowFailed(w, failure) where w == listWindow && folder != "all":
            say("\(failure)", warn: true)
            pick(folder: "all")
```

and in `stateView`, make the no-chats state All's alone: `return (window?.chatList.all.isEmpty ?? true) && reach == .up && folder == "all" ? .noChats : .noChat`. If `WindowContent` is in the checkout by then, give `WindowContent.show(_:chats:focused:panes:roles:now:actions:)` a `hasChats: Bool` parameter that it passes to `situation.stateView(hasChats:hasPanes:)` in place of `!chats.isEmpty`, and have `MainWindowController.refresh()` pass `!chatList.all.isEmpty || model.folder != "all"`.

Add to `mac/JawHarness/Sources/Scenes/SidebarScenes.swift`:

```swift
extension HarnessScene {
    static var emptyFolder: HarnessScene {
        HarnessScene(
            id: "empty-folder", title: "An empty folder", symbol: "folder",
            synopsis: "A folder's chat list with nothing in it, saying so in the terminal's words",
            section: .chatList, covers: [ChatListController.self], width: 640,
        ) { EmptyFolderScene(roles: $0.roles) }
    }
}

/// Family and Unread, each with nothing in it.
private struct EmptyFolderScene: View {
    let roles: Roles

    var body: some View {
        SpecimenGroup(
            title: "Family and Unread, empty",
            note: "\"No chats in this folder.\" in each, as the terminal says it, centred near the top of the list in the secondary colour, not cut off.",
            roles: roles,
        ) {
            HStack(spacing: Space.l) {
                ForEach(["folder:1", "unread"], id: \.self) { id in
                    ControllerSpecimen {
                        let list = ChatListController(folder: id, roles: roles)
                        _ = list.view
                        list.emptyLine = Samples.tree.emptyLine(id)
                        list.apply(Update(seq: 1, upsert: [], total: 0))
                        return list
                    }
                    .frame(width: 300, height: 180)
                }
            }
        }
    }
}
```

and register it in `BuiltInScenes.swift` after `.sidebar`: `catalog.register(.emptyFolder)`.

- [ ] **Step 4: Run every suite and build the app**

Run: `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests`, `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, `** TEST SUCCEEDED **`, `** BUILD SUCCEEDED **`.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources/AppModel.swift mac/JawHarness/Sources mac/Tests/JawUITests mac/Tests/JawHarnessTests/ScenesTests.swift
git commit -m "A folder's chat list says when it's empty, in the terminal's words, No chats in this folder., once its window has come and not while it comes; the panes' place says there are no chats yet only while the list shows All, where it said so for any empty folder; a folder that goes, deleted from another interface, leaves the list on All, as FolderTree.showing decides from the tree, and one jawd won't show, as a deleted folder's list watched again, says why in jawd's words and does the same; and the harness draws an empty folder of yours and an empty Unread"
```

---

### Task 6: Filing from a chat's menus

> **Since the plan's last review:** a pane's chat row is refreshed only by the chat list's pushes, so a chat a pane shows but the list doesn't hold keeps the `folders` it was opened with, and after it's filed from the pane its ticks, the menu bar's and Move to or Remove from go stale. When jawd answers a filing of a chat a pane shows, update that pane's row's `folders` as the answer implies (added, removed, or moved), so its menus read right; test it with a pane showing a chat the list doesn't hold.

**Waits on jawd:** nothing to build; the menus are tested from `Samples`' tree. Before `folders` lands, the snapshot checks that no menu has a folder item; a chat filed, moved and taken out on a real jawd, with jawd's words for a refusal, is task 15's.

**Files:**
- Modify: `mac/JawUI/Sources/Commands/Command.swift` (a command's `indent`)
- Create: `mac/JawUI/Sources/Sidebar/FolderMenu.swift`
- Modify: `mac/JawUI/Sources/ChatList/ChatMenu.swift` (`folderItems`, and the menus with them)
- Modify: `mac/JawUI/Sources/ChatList/ChatListController.swift` (two doc comments: a community's row has a menu now)
- Modify: `mac/JawUI/Sources/Conversation/ConversationController.swift` (`folderMenu`, m)
- Modify: `mac/JawApp/Sources/Folders.swift`, `ChatMenus.swift`, `AppModel.swift` (`makePane`, `folderItemsMade`), `MainWindowController.swift` (`refresh()`), `Commands.swift` (m in the Message menu), `Bench.swift` (m's key)
- Test: `mac/Tests/JawUITests/FolderMenuTests.swift`; modify `CommandTests.swift`, `PaneMenuTests.swift`, `ChatListMenuTests.swift` (a comment)

**Interfaces:**
- Consumes: `FolderTree` (task 3), `Jawd.folderAdd`, `folderRemove`, `moveItem` (task 2), `ChatMenu`, `MenuEntry`, `Menus.items(_:command:)`, `MenuPopUp` (the menus plan, on main).
- Produces: `Command.indent: Int` and the convenience init's `indent: Int = 0`, after `palette:`; `public struct Filed: Sendable, Hashable` (`id`, `name`, `folders`, `chat: String?`; `init(_: ChatRow)`, `init(_: RosterRow)`); `public enum FolderAct: Sendable, Equatable` with `add(item:to:)`, `remove(item:from:)`, `move(item:from:to:)` (later tasks add cases); `FolderMenu.addTo(_ tree:) -> [MenuEntry]`, `FolderMenu.entries(_ item: Filed, tree:, list:) -> [MenuEntry]`, `@MainActor FolderMenu.commands(_ item: Filed, tree:, list:, act:) -> [Command]` (ids `file:<folder>`, `move:<folder>`, `unfile`), `FolderMenu.said(_:name:tree:) -> String?`, `FolderMenu.inBoth(_ name: String, from:, to:, tree:) -> String`; `ChatMenu.folderItems(_:tree:in:)`, `ChatMenu.list(_:tree:in:)`, `ChatMenu.pane(_:tree:in:)`, `ChatMenu.with(_:_:)`; `ConversationController.folderMenu: (() -> NSMenu?)?`; in the app `folderCommands(for:)`, `focusedFolderItems()`, `folderItemsKey`, `chatMenuFoldersChanged()`, `addToFolderMenu(for:)`, `folderAct(_:name:)`.

`FolderMenu` stays a plain enum, whose `entries`, `said` and `inBoth` tests call without the main actor; `commands` is `@MainActor`, since it makes `Command`s, which are (`Command.swift`), and the project has no default isolation (`Project.swift`). Later tasks' `folderCommands`, `favouriteCommands` and `paletteCommands` are `@MainActor` for the same reason.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/FolderMenuTests.swift`:

```swift
import AppKit
@testable import JawKit
@testable import JawUI
import Testing

/// A chat's folder items say where it's filed and do what they say: Add to Folder, ticked where it
/// sits directly; Move to and Remove from only in one of your folders that holds it, greyed while it's
/// there only through a folder in it; the same in each of its menus; none before jawd serves folders.

/// A menu made from entries and the commands bound to them, as the app makes one as it opens.
@MainActor
func folderMenu(_ entries: [MenuEntry], _ commands: [Command]) -> NSMenu {
    Menus.menu(entries) { id in commands.first { $0.id == id } }
}

/// What a menu shows: each item's title, after a "· " for each level it's indented and a "✓ " when
/// it's ticked, in brackets when it's greyed, and a submenu's items after it.
@MainActor
func shows(_ menu: NSMenu) -> [String] {
    menu.items.filter { !$0.isHidden }.map { item in
        if item.isSeparatorItem { return "—" }
        let title = String(repeating: "· ", count: item.indentationLevel) + (item.state == .on ? "✓ " : "") + item.title
        let shown = item.isEnabled ? title : "[\(title)]"
        guard let sub = item.submenu else { return shown }
        return "\(shown) ▸ " + shows(sub).joined(separator: ", ")
    }
}

/// Runs a menu item, as a click does.
@MainActor
func run(_ item: NSMenuItem?) {
    guard let item else { return }
    (item.target as? CommandTarget)?.runCommand(item)
}

/// An entry's title, or its command's id when it has none.
func head(_ entry: MenuEntry) -> String {
    switch entry {
    case let .command(id, title): title ?? id
    case let .submenu(title, _): title
    case .separator: "—"
    }
}

@MainActor
final class Acts {
    var done: [FolderAct] = []
}

func ana(in folders: [String]) -> ChatRow {
    var chat = Samples.direct
    chat.folders = folders
    return chat
}

@MainActor
@Test func `Add to Folder lists your folders as a tree, ticked where the chat sits directly, and a click files it or takes it out`() {
    let acts = Acts()
    let item = Filed(ana(in: ["folder:1"]))
    let menu = folderMenu(FolderMenu.entries(item, tree: Samples.tree, list: "all"),
                          FolderMenu.commands(item, tree: Samples.tree, list: "all") { acts.done.append($0) })
    #expect(shows(menu) == ["Add to Folder ▸ ✓ Family, · Cousins, Climbing, Friends, Work, · Clients"])
    let add = menu.items.first?.submenu
    run(add?.items[0])
    run(add?.items[2])
    #expect(acts.done == [.remove(item: Samples.direct.id, from: "folder:1"), .add(item: Samples.direct.id, to: "folder:3")])
}

@MainActor
@Test func `while the list shows one of your folders, Move to files the chat in another and Remove from takes it out of this one`() {
    let acts = Acts()
    let item = Filed(ana(in: ["folder:1"]))
    let menu = folderMenu(FolderMenu.entries(item, tree: Samples.tree, list: "folder:1"),
                          FolderMenu.commands(item, tree: Samples.tree, list: "folder:1") { acts.done.append($0) })
    #expect(Array(shows(menu).dropFirst()) == ["Move to ▸ [Family], · Cousins, Climbing, Friends, Work, · Clients", "Remove from Family"])
    run(menu.items[1].submenu?.items[4])
    run(menu.items[2])
    #expect(acts.done == [.move(item: Samples.direct.id, from: "folder:1", to: "folder:5"), .remove(item: Samples.direct.id, from: "folder:1")])
}

@MainActor
@Test func `Move to and Remove from are greyed while the chat is in the list's folder only through a folder in it`() {
    let item = Filed(ana(in: ["folder:2"]))
    let menu = folderMenu(FolderMenu.entries(item, tree: Samples.tree, list: "folder:1"),
                          FolderMenu.commands(item, tree: Samples.tree, list: "folder:1") { _ in })
    #expect(Array(shows(menu).dropFirst())
        == ["[Move to] ▸ [Family], [· Cousins], [Climbing], [Friends], [Work], [· Clients]", "[Remove from Family]"])
}

@Test func `in All, a built-in or a smart folder, or one of yours that doesn't hold the chat, there's no Move to or Remove from`() {
    let item = Filed(ana(in: ["folder:1"]))
    for list in ["all", "unread", "smart:2", "folder:3"] {
        #expect(FolderMenu.entries(item, tree: Samples.tree, list: list).map(head) == ["Add to Folder"], "\(list)")
    }
}

@Test func `before jawd serves folders the menus have no folder items`() {
    let chat = ana(in: ["folder:1"])
    #expect(FolderMenu.entries(Filed(chat), tree: FolderTree(), list: "all").isEmpty)
    #expect(ChatMenu.list(chat, tree: FolderTree(), in: "all") == ChatMenu.list(chat))
    #expect(ChatMenu.pane(chat, tree: FolderTree(), in: "all") == ChatMenu.pane(chat))
}

@Test func `a community's row has the folder items alone, and a chat's menus have them after its actions`() {
    let community = ChatRow(id: "c@g.us", v: 1, sort: "1", kind: .community, name: "Bairro Alto", folders: ["folder:1"])
    #expect(ChatMenu.list(community, tree: Samples.tree, in: "folder:1").map(head) == ["Add to Folder", "Move to", "unfile"])
    let chat = ana(in: ["folder:1"])
    #expect(ChatMenu.list(chat, tree: Samples.tree, in: "folder:1").map(head)
        == ["chat.open.pane", "—", "Unpin", "Mute", "Archive", "Mark as Unread", "—", "Add to Folder", "Move to", "unfile"])
    #expect(ChatMenu.pane(chat, tree: Samples.tree, in: "all").map(head)
        == ["split.across", "split.down", "pane.close", "Swap", "—", "Unpin", "Mute", "Archive", "Mark as Unread", "—", "Add to Folder", "—",
            "search.chat"])
}

@Test func `a chat's folder items are one list, which the chat list's menu, the pane's and the menu bar's Chat menu all have`() {
    let chat = ana(in: ["folder:1"])
    let items = ChatMenu.folderItems(chat, tree: Samples.tree, in: "folder:1")
    #expect(items.map(head) == ["Add to Folder", "Move to", "unfile"])
    #expect(Array(ChatMenu.list(chat, tree: Samples.tree, in: "folder:1").suffix(items.count)) == items)
    #expect(Array(ChatMenu.pane(chat, tree: Samples.tree, in: "folder:1").dropLast(2).suffix(items.count)) == items, "before Search This Chat")
    #expect(ChatMenu.folderItems(chat, tree: Samples.tree, in: "all").map(head) == ["Add to Folder"])
}

@Test func `what's said once jawd has filed a chat, taken it out or moved it, and when a move leaves it in both folders`() {
    let tree = Samples.tree
    #expect(FolderMenu.said(.add(item: "x", to: "folder:1"), name: "Ana Ribeiro", tree: tree) == "Ana Ribeiro filed in Family")
    #expect(FolderMenu.said(.remove(item: "x", from: "folder:2"), name: "Ana Ribeiro", tree: tree) == "Ana Ribeiro taken out of Cousins")
    #expect(FolderMenu.said(.move(item: "x", from: "folder:1", to: "folder:5"), name: "Ana Ribeiro", tree: tree)
        == "Ana Ribeiro moved from Family to Work")
    #expect(FolderMenu.inBoth("Ana Ribeiro", from: "folder:1", to: "folder:5", tree: tree) == "Ana Ribeiro is now in both Work and Family")
}
```

In `mac/Tests/JawUITests/ChatListMenuTests.swift`, make the comment on `ListHost`'s `chatList(_:menuItemsFor:)` `/// None for a community's row, as the app's host has none before jawd serves folders; after, it has the folder items alone.`, where it said the app's host has none: the test host stands in for the app before jawd serves folders, as the VoiceOver test's community row needs.

Add to `mac/Tests/JawUITests/CommandTests.swift`:

```swift
@MainActor
@Test func `a command's indent is its menu item's, for a tree of folders`() {
    #expect(Command("file:folder:2", "Cousins", place: nil, indent: 1) {}.menuItem().indentationLevel == 1)
    #expect(Command("x", "X", place: nil) {}.menuItem().indentationLevel == 0)
}
```

Add to `mac/Tests/JawUITests/PaneMenuTests.swift`:

```swift
@MainActor
@Test func `m with a message selected opens its chat's Add to Folder under the header, and is taken with no menu to show`() async throws {
    let host = PaneHost()
    let (c, window) = await shown(Samples.messages, host)
    defer { window.close() }
    let text = try #require(Samples.messages.first { $0.kind == .text && !$0.deleted })
    c.select(text.id)
    let menu = NSMenu()
    menu.addItem(withTitle: "Family", action: nil, keyEquivalent: "")
    c.folderMenu = { menu }
    var popped: [(NSMenu, NSPoint)] = []
    c.header.popUp.run = { m, point, _ in popped.append((m, point)) }
    let m = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                          context: nil, characters: "m", charactersIgnoringModifiers: "m", isARepeat: false, keyCode: 46))
    #expect(c.selectionKey(m))
    #expect(popped.first?.0 === menu)
    #expect(popped.first?.1 == NSPoint(x: c.header.avatar.frame.minX, y: c.header.bounds.minY))
    c.folderMenu = { nil }
    #expect(c.selectionKey(m), "m is taken, though there's no menu: the app says why")
    #expect(popped.count == 1)
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `cannot find 'FolderMenu' in scope`, `type 'ChatMenu' has no member 'folderItems'`, `extra argument 'indent' in call` and `value of type 'ConversationController' has no member 'folderMenu'`.

- [ ] **Step 3: Write the entries and the commands**

In `mac/JawUI/Sources/Commands/Command.swift`: add, after `public let inPalette: Bool`,

```swift
    /// How far its menu item is indented, for a tree of folders.
    public let indent: Int
```

add `indent: Int = 0,` after `palette: Bool = true,` in the public convenience init and pass `indent: indent` to `self.init`; pass `indent: 0` from the `send:` convenience init; add `indent: Int,` after `palette: Bool,` in the designated init with `self.indent = indent`; and in `menuItem(title:)`, after `item.target = …`, add `item.indentationLevel = indent`.

Create `mac/JawUI/Sources/Sidebar/FolderMenu.swift`:

```swift
import JawKit

/// Something filed in folders: a chat, or someone from the roster.
public struct Filed: Sendable, Hashable {
    /// A chat's id, or a `person:` id.
    public var id: String
    public var name: String
    /// Your folders it sits in directly.
    public var folders: [String]
    /// The chat a favourite slot holds for it: the chat's own id, or someone's one-to-one chat; nil for
    /// someone with no chat yet.
    public var chat: String?

    public init(_ chat: ChatRow) {
        id = chat.id
        name = chat.name
        folders = chat.folders
        self.chat = chat.id
    }

    public init(_ person: RosterRow) {
        id = person.id
        name = person.name
        folders = person.folders
        chat = person.chat
    }
}

/// What a folder item asks for, which the app asks jawd for.
public enum FolderAct: Sendable, Equatable {
    /// Files an item in a folder of yours, or takes it out.
    case add(item: String, to: String)
    case remove(item: String, from: String)
    /// Files an item in `to`, then, once jawd has it there, takes it out of `from`.
    case move(item: String, from: String, to: String)
}

/// The folder items of a chat's menus and of someone's in the roster: entries worked out from the tree
/// and the item's row as a menu opens, and the commands they name, bound to the item.
public enum FolderMenu {
    /// Add to Folder's entries: your folders as a tree, then New Folder… after a separator.
    public static func addTo(_ tree: FolderTree) -> [MenuEntry] {
        tree.yours.map { MenuEntry.command("file:\($0.row.id)") } + [.separator, .command("file.new")]
    }

    /// A chat's or someone's folder items: Add to Folder ▸; and while the list shows one of your
    /// folders that holds the item, directly or through a folder in it, Move to ▸ and Remove from it.
    /// Move to lists every folder of yours, as Add to Folder does, the list's own greyed: left out, the
    /// folders in it would be indented under nothing. None before jawd serves folders.
    public static func entries(_ item: Filed, tree: FolderTree, list: String) -> [MenuEntry] {
        guard tree.served else { return [] }
        let add = MenuEntry.submenu("Add to Folder", addTo(tree))
        guard holds(list, item, tree) else { return [add] }
        return [add, .submenu("Move to", tree.yours.map { .command("move:\($0.row.id)") }), .command("unfile")]
    }

    /// Whether `list` is one of your folders holding `item`, directly or through a folder in it.
    static func holds(_ list: String, _ item: Filed, _ tree: FolderTree) -> Bool {
        tree.row(list)?.isYours == true && !tree.subtree(list).isDisjoint(with: item.folders)
    }

    /// The commands those entries name, bound to `item`: each folder of yours, ticked where the item
    /// sits directly, filing it there or taking it out; Move to's, filing it in another and taking it
    /// out of the list's; and Remove from. Move to's and Remove from are greyed while the item is in
    /// the list's folder only through a folder in it, since neither can take it out of this one.
    @MainActor
    public static func commands(_ item: Filed, tree: FolderTree, list: String,
                                act: @escaping @MainActor @Sendable (FolderAct) -> Void) -> [Command]
    {
        let here = holds(list, item, tree), direct = item.folders.contains(list)
        var out: [Command] = []
        for (row, depth) in tree.yours {
            let folder = row.id, filed = item.folders.contains(folder), label = tree.label(folder)
            out.append(Command("file:\(folder)", row.name, phrase: filed ? "Take this chat out of \(label)" : "Add this chat to \(label)",
                               place: nil, indent: depth, on: { filed })
            {
                act(filed ? .remove(item: item.id, from: folder) : .add(item: item.id, to: folder))
            })
            if here {
                out.append(Command("move:\(folder)", row.name, phrase: "Move this chat to \(label)", place: nil, indent: depth,
                                   enabled: { direct && folder != list })
                {
                    act(.move(item: item.id, from: list, to: folder))
                })
            }
        }
        if here, let shown = tree.row(list) {
            out.append(Command("unfile", "Remove from \(shown.name)", place: nil, palette: false, enabled: { direct }) {
                act(.remove(item: item.id, from: list))
            })
        }
        return out
    }

    /// What the status line says once jawd has done it, as the terminal says it.
    public static func said(_ act: FolderAct, name: String, tree: FolderTree) -> String? {
        switch act {
        case let .add(_, to): "\(name) filed in \(tree.label(to))"
        case let .remove(_, from): "\(name) taken out of \(tree.label(from))"
        case let .move(_, from, to): "\(name) moved from \(tree.label(from)) to \(tree.label(to))"
        }
    }

    /// What the status line says, before jawd's words, when Move to leaves the item in both folders:
    /// jawd filed it in `to` and didn't take it out of `from`.
    public static func inBoth(_ name: String, from: String, to: String, tree: FolderTree) -> String {
        "\(name) is now in both \(tree.label(to)) and \(tree.label(from))"
    }
}
```

At the end of `mac/JawUI/Sources/ChatList/ChatMenu.swift`, add:

```swift
/// A chat's menus with their folder items, from the tree as they open.
public extension ChatMenu {
    /// A chat's folder items, one list for the chat list's menu, the pane's and the menu bar's Chat
    /// menu: Add to Folder, and Move to and Remove from while the list shows one of your folders that
    /// holds it.
    static func folderItems(_ chat: ChatRow, tree: FolderTree, in folder: String) -> [MenuEntry] {
        FolderMenu.entries(Filed(chat), tree: tree, list: folder)
    }

    /// The chat list's menu: a chat's actions, then its folder items; a community's row has the folder
    /// items alone, and its groups the whole menu.
    static func list(_ chat: ChatRow, tree: FolderTree, in folder: String) -> [MenuEntry] {
        with(list(chat), folderItems(chat, tree: tree, in: folder))
    }

    /// The pane's menu: its chat's folder items after its actions, before Search This Chat.
    static func pane(_ chat: ChatRow, tree: FolderTree, in folder: String) -> [MenuEntry] {
        var out = pane(chat)
        let folders = folderItems(chat, tree: tree, in: folder)
        if !folders.isEmpty { out.insert(contentsOf: [.separator] + folders, at: out.count - 2) }
        return out
    }

    /// `entries`, then `more` after a separator; either alone when the other is empty.
    static func with(_ entries: [MenuEntry], _ more: [MenuEntry]) -> [MenuEntry] {
        entries.isEmpty ? more : more.isEmpty ? entries : entries + [.separator] + more
    }
}
```

In `mac/JawUI/Sources/ChatList/ChatListController.swift`, a community's row has a menu now, so make the end of `menu(for:)`'s doc comment "the table's menu to be filled for; none when the host has none." and the end of `ChatCell.accessibilityPerformShowMenu()`'s "the chat's menu under its row, none when the host has none.", where both said there was none on a community's row.

- [ ] **Step 4: Write m**

In `mac/JawUI/Sources/Conversation/ConversationController.swift`, after `public var messageMenuTail: (() -> [NSMenuItem])?`, add

```swift
    /// The chat's Add to Folder, for m, made as it opens.
    public var folderMenu: (() -> NSMenu?)?
```

in `selectionKey(_:)`, add `case "m": fileChat()` after `case "i": …`; in `act(_:)`, add `case "m": fileChat()` after its `case "i": …`; and after `act(_:)` add

```swift
    /// m: the chat's Add to Folder under the pane's header, as the terminal's m files the open chat.
    func fileChat() {
        _ = header.popUp.show(folderMenu?(), at: NSPoint(x: header.avatar.frame.minX, y: header.bounds.minY), in: header)
    }
```

- [ ] **Step 5: Run the JawUI tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the ten new tests among those passed.

- [ ] **Step 6: Wire the app**

Add to `mac/JawApp/Sources/Folders.swift`, inside the extension:

```swift
    /// A chat's or someone's folder items' commands, bound to it.
    func folderCommands(for item: Filed) -> [Command] {
        FolderMenu.commands(item, tree: tree, list: folder) { [weak self] act in self?.folderAct(act, name: item.name) }
    }

    /// The focused chat's folder items, for the end of the menu bar's Chat menu.
    func focusedFolderItems() -> [NSMenuItem] {
        guard let chat = conversation?.chat else { return [] }
        let bound = folderCommands(for: Filed(chat))
        return Menus.items(ChatMenu.folderItems(chat, tree: tree, in: folder)) { id in bound.first { $0.id == id } }
    }

    static let folderItemTag = 7301

    /// What the menu bar's folder items are made from: the focused chat, the folders it's in, the
    /// list's folder, and your folders' places and names.
    var folderItemsKey: [String] {
        guard let chat = conversation?.chat, tree.served else { return [] }
        return [chat.id, folder, chat.folders.joined(separator: ",")] + tree.yours.map { "\($0.depth) \($0.row.id) \($0.row.name)" }
    }

    /// The menu bar's Chat menu ends with the focused chat's folder items, made again only when what
    /// they're made from changes, as the Layouts menu is made again when the layouts change: a delegate
    /// filling them as the menu opens would be asked again each time AppKit looks for a key's item.
    func chatMenuFoldersChanged() {
        let key = folderItemsKey
        guard key != folderItemsMade, let menu = NSApp.mainMenu?.item(withTitle: MenuPlace.chat.rawValue)?.submenu else { return }
        folderItemsMade = key
        for item in menu.items where item.tag == Self.folderItemTag {
            menu.removeItem(item)
        }
        let items = focusedFolderItems()
        guard !items.isEmpty else { return }
        for item in [NSMenuItem.separator()] + items {
            item.tag = Self.folderItemTag
            menu.addItem(item)
        }
    }

    /// m: the chat's Add to Folder, as a menu of its own under the pane's header.
    func addToFolderMenu(for chat: ChatRow) -> NSMenu? {
        guard tree.served else {
            say("folders come with jawd's M6")
            return nil
        }
        let bound = folderCommands(for: Filed(chat))
        return Menus.menu(FolderMenu.addTo(tree)) { id in bound.first { $0.id == id } }
    }

    /// Asks jawd for what a folder item does, saying what it did once jawd has done it.
    func folderAct(_ act: FolderAct, name: String) {
        let said = FolderMenu.said(act, name: name, tree: tree)
        switch act {
        case let .add(item, to): jawd.folderAdd(to, item: item, said: said)
        case let .remove(item, from): jawd.folderRemove(from, item: item, said: said)
        case let .move(item, from, to):
            jawd.moveItem(item, from: from, to: to, said: said, both: FolderMenu.inBoth(name, from: from, to: to, tree: tree))
        }
    }
```

In `mac/JawApp/Sources/ChatMenus.swift`: in `chatMenuItems(for:)`, make `bound` `chatCommands(for: { chat }) + [inNewPane] + folderCommands(for: Filed(chat))` and its entries `ChatMenu.list(chat, tree: tree, in: folder)`; in `paneMenu(for:)`, make `bound` `chatCommands(for: { chat }) + folderCommands(for: Filed(chat))` and its entries `ChatMenu.pane(chat, tree: tree, in: folder)`.

In `mac/JawApp/Sources/AppModel.swift`: add `var folderItemsMade: [String] = []` after `var foldersWindow: WindowID?`, the key the menu bar's folder items were last made from, and in `makePane(_:place:)`, after `c.messageMenuTail = …`,

```swift
        c.folderMenu = { [weak self, weak c] in
            guard let self, let c else { return nil }
            return addToFolderMenu(for: c.chat)
        }
```

In `mac/JawApp/Sources/MainWindowController.swift`, add `model.chatMenuFoldersChanged()` at the end of `refresh()`, which runs after each frame of jawd's updates and each change of focus, so the Chat menu's folder items follow a push, a pane focused or a folder picked by the next frame and cost a comparison of a few strings otherwise. If `WindowContent` is in the checkout by then, the call still goes at the end of `MainWindowController.refresh()`.

In `mac/JawApp/Sources/Commands.swift`, add `("file", "Add to Folder…", "Add this chat to a folder", "m")` to the Message section's list, after `("copy.text", …)`.

In `mac/JawApp/Sources/Bench.swift`, add `"m": 46, "n": 45` to `codes`.

- [ ] **Step 7: Build the app and see the fallback on the demo**

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/menus.png --keys "menu:list:0,menu:header,up,m"`
Expected, among the log's lines: `menu:list:0:` and `menu:header:` as the menus plan's task 6 has them, neither with `Add to Folder`; and for `m`, the panes' line with `· said "folders come with jawd's M6"` in it.

- [ ] **Step 8: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/Tests/JawUITests
git commit -m "A chat's menus file it in folders: Add to Folder lists your folders as a tree, indented, with a tick on each the chat sits in directly, a click filing it there or taking it out; while the list shows one of your folders that holds the chat, Move to files it in another and takes it out of this one, listing the list's own folder greyed so the tree keeps its shape, and Remove from takes it out, both greyed while it's there only through a folder in it; the chat list's menu, the pane's and the menu bar's Chat menu have the same folder items, ChatMenu.folderItems, the menu bar's made again only when the focused chat, its folders, the list's folder or the tree changes rather than each time AppKit looks for a key, a community's row has them alone, and none has them before jawd serves folders; m with a message selected opens its chat's Add to Folder under the pane's header; and what jawd has done is said in the terminal's words once it has, or, when a move leaves the chat in both folders, that it's in both"
```

---

### Task 7: Naming in place

> **Since the plan's last review:** if another interface deletes the folder being renamed, or a new folder's parent, the field's row goes but the naming state stays, so Return on the tree does nothing until Esc. In `apply`, cancel the naming when the folder it renames, or the parent a new folder is being made in, leaves the tree; test both.

**Waits on jawd:** nothing to build; the field is tested against a host standing in for the app. Before `folders` lands, ⇧⌘N, and the palette's New folder, which the palette lists then as it lists any command of the app's, say it comes with jawd's M6; jawd's own refusals under the field (a name a sibling has, error 4; an empty one, -32602) are seen in task 15.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/SidebarController.swift` (`naming`, the field's row, `FolderOutline`, `SidebarHost`, `apply(_:)` and `roles` keeping the field)
- Create: `mac/JawUI/Sources/Sidebar/SidebarNaming.swift`
- Modify: `mac/JawUI/Sources/Sidebar/SidebarCells.swift` (`NamingCell`)
- Modify: `mac/JawUI/Sources/Sidebar/FolderMenu.swift` (`newFolder` and `rename` acts; New Folder…)
- Modify: `mac/JawApp/Sources/Folders.swift`, `AppModel.swift` (`.named`, `.notNamed`), `MainWindowController.swift` (`showSidebar()`), `Commands.swift` (New Folder, Rename Folder), `Bench.swift` (`type:`, `esc`), `mac/README.md`
- Modify: `mac/JawHarness/Sources/Scenes/SidebarScenes.swift`
- Test: `mac/Tests/JawUITests/NamingTests.swift`; modify `SidebarTests.swift` (`SideHost`), `FolderMenuTests.swift`

**Interfaces:**
- Consumes: `FolderTree` (task 3), `SidebarController` (task 4), `FolderMenu`, `FolderAct` (task 6), `Jawd.folderCreate`, `folderRename`, `JawUpdate.named`, `.notNamed` (task 2).
- Produces: `SidebarController.Naming` (`parent`, `renaming`, `filing: Filed?`, `text`, `refused`, `asked`, `away`), `naming: Naming?`, `beginNaming(parent:filing:text:)`, `beginRenaming(_:)`, `cancelNaming()`, `named(_ row: FolderRow?)`, `notNamed(_ why: String)`, and internally `keepingNaming(_:)` and `focusField(_:)`; `SidebarHost.sidebar(_:name:as:)`; `FolderAct.newFolder(parent: String?, filing: Filed?)` and `.rename(String)`; the command `file.new`; the app's `folder.new` (⇧⌘N) and `folder.rename` (↩) commands and `newFolder()`; `MainWindowController.showSidebar()`; the snapshot's `type:<text>` step and `esc` key.

The Questions' 5 and 6 were decided after the plan's review, and this task builds them: a folder made from the sidebar, with ⇧⌘N, New Folder… or New Folder Inside…, is picked, as Notes and Reminders select a new item in their sidebars, while one made from a chat's Add to Folder isn't; and a click away keeps the name typed, as Finder keeps it, unless it's empty or unchanged, Esc alone dropping it. A push from jawd while a name is typed, a folder's counts or another interface's new folder, leaves the field with the keys, what's typed and the caret where they were.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawUITests/SidebarTests.swift`, add to `SideHost`:

```swift
    var named: [(String, SidebarController.Naming)] = []

    func sidebar(_: SidebarController, name: String, as naming: SidebarController.Naming) {
        named.append((name, naming))
    }
```

Create `mac/Tests/JawUITests/NamingTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// New Folder and Rename open a field in place in the tree: Return, or a click elsewhere, asks for the
/// name typed, once while jawd answers, unless it's empty or unchanged; jawd's refusal is said under
/// the field, which stays open, and doesn't take back keys that have gone elsewhere; Esc drops it; a
/// push from jawd leaves it as it is; once jawd has made the folder its row is there, picked when it
/// was made from the sidebar.

/// A sidebar with the sample tree in an offscreen window, so its field can take the keys.
@MainActor
func namingSidebar(_ host: SideHost) -> (SidebarController, NSWindow) {
    let s = SidebarController()
    s.host = host
    s.roles = Catalog.jaw.roles(dark: true)
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 220, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = s.view
    s.apply(Samples.folderUpdate)
    s.view.layoutSubtreeIfNeeded()
    return (s, window)
}

/// The field's editor, which has the keys while a name is typed.
@MainActor
func fieldEditor(_ window: NSWindow) throws -> NSTextView {
    try #require(window.firstResponder as? NSTextView)
}

@MainActor
@Test func `New Folder at the top opens a field at the end of Folders, and Return asks for what's typed, once while jawd answers`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: nil)
    #expect(s.said[10] == "[]")
    try fieldEditor(window).insertText("Parents", replacementRange: NSRange(location: NSNotFound, length: 0))
    #expect(s.said[10] == "[Parents]")
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    #expect(host.named.map(\.0) == ["Parents"], "asked once while jawd answers")
    #expect(host.named.first?.1.parent == nil && host.named.first?.1.renaming == nil)
    s.named(FolderRow(id: "folder:7", v: 70, name: "Parents", position: 4))
    #expect(s.naming == nil && s.said[10] == "Parents")
    #expect(s.picked == "folder:7" && host.picked == ["folder:7"], "made from the sidebar, it's picked, as Notes picks a new folder")
}

@MainActor
@Test func `a folder made from a chat's Add to Folder isn't picked, so the list keeps what it showed`() {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: nil, filing: Filed(Samples.direct), text: "Parents")
    s.named(FolderRow(id: "folder:7", v: 70, name: "Parents", position: 4))
    #expect(s.naming == nil && s.picked == "all" && host.picked.isEmpty)
}

@MainActor
@Test func `New Folder in Family opens Family and puts the field after Cousins`() {
    let (s, window) = namingSidebar(SideHost())
    defer { window.close() }
    s.beginNaming(parent: "folder:1")
    #expect(Array(s.said[6 ... 8]) == ["Family ⌘1 @ 3 ▾", "  Cousins 1", "  []"])
}

@MainActor
@Test func `a name jawd turns down is said under the field, which keeps the keys and what's typed, and Return asks again`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: "folder:1")
    try fieldEditor(window).insertText("Cousins", replacementRange: NSRange(location: NSNotFound, length: 0))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    s.notNamed("a folder there is called Cousins already")
    #expect(s.said[8] == "  [Cousins] · a folder there is called Cousins already")
    #expect(window.firstResponder is NSTextView, "the field still has the keys")
    #expect(s.outline.rect(ofRow: 8).height == 46, "room under the field for jawd's words")
    try fieldEditor(window).insertText(" 2", replacementRange: NSRange(location: NSNotFound, length: 0))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    #expect(host.named.map(\.0) == ["Cousins", "Cousins 2"])
}

@MainActor
@Test func `Esc drops the field, and a click elsewhere keeps the name typed, as Finder does, unless it's empty or unchanged`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: nil)
    try fieldEditor(window).insertText("Temp", replacementRange: NSRange(location: NSNotFound, length: 0))
    try fieldEditor(window).doCommand(by: #selector(NSResponder.cancelOperation(_:)))
    #expect(s.naming == nil && !s.said.contains("[Temp]") && host.named.isEmpty)
    s.beginNaming(parent: nil)
    window.makeFirstResponder(s.outline)
    #expect(s.naming == nil && host.named.isEmpty, "an empty name given up is dropped")
    s.beginRenaming("folder:3")
    window.makeFirstResponder(s.outline)
    #expect(s.naming == nil && host.named.isEmpty, "and an unchanged one")
    s.beginNaming(parent: nil)
    try fieldEditor(window).insertText("Parents", replacementRange: NSRange(location: NSNotFound, length: 0))
    window.makeFirstResponder(s.outline)
    #expect(host.named.map(\.0) == ["Parents"], "a click elsewhere asks for what's typed")
    #expect(s.naming?.asked == true && s.said.contains("[Parents]"), "the row waits for jawd's answer")
}

@MainActor
@Test func `a name a click elsewhere kept that jawd turns down is said without taking the keys back, and Esc or another click away drops it`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginNaming(parent: "folder:1")
    try fieldEditor(window).insertText("Cousins", replacementRange: NSRange(location: NSNotFound, length: 0))
    window.makeFirstResponder(s.outline)
    s.notNamed("a folder there is called Cousins already")
    #expect(window.firstResponder === s.outline, "the keys stay where they went")
    #expect(s.said[8] == "  [Cousins] · a folder there is called Cousins already")
    let esc = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                            context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53))
    s.outline.keyDown(with: esc)
    #expect(s.naming == nil && !s.said.contains { $0.contains("[Cousins]") }, "Esc in the tree drops it")
    s.beginNaming(parent: "folder:1")
    try fieldEditor(window).insertText("Cousins", replacementRange: NSRange(location: NSNotFound, length: 0))
    window.makeFirstResponder(s.outline)
    s.notNamed("a folder there is called Cousins already")
    s.focusField()
    window.makeFirstResponder(s.outline)
    #expect(s.naming == nil && host.named.count == 2, "given up again once jawd has turned it down: dropped, not asked again")
}

@MainActor
@Test func `a push while a name is typed leaves the field with the keys, what's typed and the caret where it was`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    s.beginRenaming("folder:3")
    try fieldEditor(window).insertText("Climbing club", replacementRange: NSRange(location: NSNotFound, length: 0))
    s.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:3", v: 50, name: "Climbing", position: 1, unread: 4, count: 8, favourite: 3)]))
    #expect(window.firstResponder is NSTextView && s.said[7] == "[Climbing club]", "the renamed folder's counts changed")
    s.apply(Update(seq: 302, upsert: [FolderRow(id: "folder:8", v: 62, name: "Neighbours", position: 4)]))
    #expect(try fieldEditor(window).string == "Climbing club" && s.said[7] == "[Climbing club]", "another interface made a folder")
    #expect(try fieldEditor(window).selectedRange() == NSRange(location: 13, length: 0), "the caret where it was")
    #expect(s.naming?.renaming == "folder:3" && host.named.isEmpty, "nothing asked of jawd")
}

@MainActor
@Test func `Return on a folder of yours opens its name to change, and on a built-in one does nothing`() throws {
    let host = SideHost()
    let (s, window) = namingSidebar(host)
    defer { window.close() }
    let ret = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                            context: nil, characters: "\r", charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 36))
    s.select("folder:3")
    window.makeFirstResponder(s.outline)
    s.outline.keyDown(with: ret)
    #expect(s.naming?.renaming == "folder:3" && s.said[7] == "[Climbing]")
    try fieldEditor(window).doCommand(by: #selector(NSResponder.insertNewline(_:)))
    #expect(host.named.map(\.0) == ["Climbing"] && host.named.first?.1.renaming == "folder:3")
    s.named(nil)
    #expect(s.naming == nil && s.said[7] == "Climbing ⌘3")
    s.select("all")
    window.makeFirstResponder(s.outline)
    s.outline.keyDown(with: ret)
    #expect(s.naming == nil, "the built-in folders keep their names")
}
```

In `mac/Tests/JawUITests/FolderMenuTests.swift`, make the first test's expected menu `["Add to Folder ▸ ✓ Family, · Cousins, Climbing, Friends, Work, · Clients, —, New Folder…"]`, and the expected `Move to` lines unchanged; and add:

```swift
@MainActor
@Test func `New Folder… ends Add to Folder, making a folder at the top to file the chat in`() {
    let acts = Acts()
    let item = Filed(ana(in: []))
    let menu = folderMenu(FolderMenu.entries(item, tree: Samples.tree, list: "all"),
                          FolderMenu.commands(item, tree: Samples.tree, list: "all") { acts.done.append($0) })
    run(menu.items.first?.submenu?.items.last)
    #expect(acts.done == [.newFolder(parent: nil, filing: item)])
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `value of type 'SidebarController' has no member 'beginNaming'`, `'Naming' is not a member type of class 'SidebarController'` and `type 'FolderAct' has no member 'newFolder'`.

- [ ] **Step 3: Write the field**

In `mac/JawUI/Sources/Sidebar/SidebarController.swift`:

1. Add to `SidebarHost`:

```swift
    /// Return in the name field: make the folder, or rename it, with `name`.
    func sidebar(_ sidebar: SidebarController, name: String, as naming: SidebarController.Naming)
```

2. Add after `var collapsing = false`:

```swift
    /// A folder being named in place, while its field is open.
    public internal(set) var naming: Naming?
    /// While the field is redrawn, its editing ending isn't the field given up.
    var keepingField = false
    static let namingRow = "naming"
```

3. Make the outline `let outline = FolderOutline()` and, in `loadView()`, add `outline.sidebar = self` after `outline.delegate = self`.

4. Make `children(_:)` add the new folder's row where it will go, and show Folders for one at the top before you have any:

```swift
    func children(_ parent: String?) -> [String] {
        let new = naming.map { $0.renaming == nil } ?? false
        var out: [String] = switch parent {
        case nil:
            tree.builtIn.map(\.id) + (tree.children(of: nil).isEmpty && !(new && naming?.parent == nil) ? [] : [Self.foldersHeading])
                + (tree.smart.isEmpty ? [] : [Self.smartHeading])
        case .some(Self.foldersHeading): tree.children(of: nil).map(\.id)
        case .some(Self.smartHeading): tree.smart.map(\.id)
        case let .some(id): tree.children(of: id).map(\.id)
        }
        if new, let parent, parent == (naming?.parent ?? Self.foldersHeading) { out.append(Self.namingRow) }
        return out
    }
```

5. In `outlineView(_:viewFor:item:)`, before the heading's branch, add

```swift
        if id == Self.namingRow || id == naming?.renaming, let naming {
            let cell = (outlineView.makeView(withIdentifier: NamingCell.id, owner: self) as? NamingCell) ?? NamingCell()
            cell.show(naming, delegate: self, roles: roles)
            return cell
        }
```

and add the row's height:

```swift
    public func outlineView(_: NSOutlineView, heightOfRowByItem item: Any) -> CGFloat {
        let id = (item as? Item)?.id
        return (id == Self.namingRow || id == naming?.renaming) && naming?.refused != nil ? 46 : outline.rowHeight
    }
```

6. In `said`, before `guard let folder = tree.row(id)`, add

```swift
            if id == Self.namingRow || id == naming?.renaming, let naming {
                let indent = String(repeating: "  ", count: max(outline.level(forRow: row) - 1, 0))
                return indent + "[\(naming.text)]" + (naming.refused.map { " · \($0)" } ?? "")
            }
```

7. At the end of the file, add:

```swift
/// The tree's outline: Return on a folder of yours renames it, as Finder's does, and Esc drops a name
/// left in the tree without the keys, as one jawd turned down after a click away.
final class FolderOutline: NSOutlineView {
    weak var sidebar: SidebarController?

    override func keyDown(with event: NSEvent) {
        let plain = event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty
        if event.keyCode == 53, plain, let sidebar, sidebar.naming?.asked == false {
            sidebar.cancelNaming()
            return
        }
        if event.keyCode == 36 || event.keyCode == 76, plain, let sidebar, sidebar.naming == nil, sidebar.tree.row(sidebar.picked)?.isYours == true {
            sidebar.beginRenaming(sidebar.picked)
            return
        }
        super.keyDown(with: event)
    }
}
```

8. Make `apply(_:)` keep a name being typed. Drawing a row again replaces its cell, which ends the editing in it, so the folder being renamed isn't drawn again while its field is open, its counts waiting for the field to close; and a push that lays the tree out again goes through `keepingNaming(_:)`, below, which gives the field back the keys, what's typed and its selection:

```swift
    /// jawd's tree, changed. A row whose counts alone changed is drawn again where it is; a change to
    /// the tree's shape lays it out again, keeping what's open, so a flood of messages costs the
    /// sidebar a few rows' drawing. A name being typed keeps its field, its text and its caret.
    public func apply(_ update: Update<FolderRow>) {
        let before = tree
        tree.apply(update)
        guard isViewLoaded else { return }
        if before.shape != tree.shape || before.served != tree.served {
            keepingNaming {
                outline.reloadData()
                restoreOpen()
            }
            return
        }
        for id in tree.rows.keys where id != naming?.renaming && (before.row(id) != tree.row(id) || before.badges(id) != tree.badges(id)) {
            if let item = items[id] { outline.reloadItem(item) }
        }
    }
```

and make `roles`' `didSet` lay the outline out the same way, `keepingNaming { outline.reloadData(); restoreOpen() }`, after its `guard`.

Create `mac/JawUI/Sources/Sidebar/SidebarNaming.swift`:

```swift
import AppKit
import JawKit

public extension SidebarController {
    /// A folder being named in place: a new one, in `parent` or at the top, with what to file in it once
    /// it's made; or one of yours being renamed. What's typed, and jawd's refusal of the name last
    /// asked for, which is said under the field.
    struct Naming: Equatable, Sendable {
        public var parent: String?
        public var renaming: String?
        public var filing: Filed?
        public var text: String
        public var refused: String?
        /// Asked of jawd and not answered yet, so Return doesn't ask again meanwhile.
        public var asked = false
        /// jawd turned down what's typed while the keys were elsewhere, so giving the field up again
        /// drops it rather than asking for it again; typing clears it.
        public var away = false
    }
}

extension SidebarController: NSTextFieldDelegate {
    /// New Folder: a row with its name field open where the folder will go, last in `parent` or at the
    /// top, the folders it's in opened; `filing` is filed in it once it's made.
    public func beginNaming(parent: String?, filing: Filed? = nil, text: String = "") {
        guard tree.served, isViewLoaded else { return }
        keepingField = true
        naming = Naming(parent: parent, renaming: nil, filing: filing, text: text, refused: nil)
        outline.reloadData()
        restoreOpen()
        if let parent { expand(parent) }
        keepingField = false
        focusField()
    }

    /// Rename: the same field, on a folder of yours, holding its name.
    public func beginRenaming(_ folder: String) {
        guard let row = tree.row(folder), row.isYours, isViewLoaded else { return }
        keepingField = true
        naming = Naming(parent: row.parent, renaming: folder, filing: nil, text: row.name, refused: nil)
        outline.reloadData()
        restoreOpen()
        keepingField = false
        focusField()
    }

    /// Esc, or a name given up empty, unchanged or as jawd last turned it down: the folder isn't made,
    /// or keeps its name.
    public func cancelNaming() {
        guard naming != nil else { return }
        naming = nil
        outline.reloadData()
        restoreOpen()
    }

    /// jawd made the folder or renamed it: the field closes, and a folder made is there at once, before
    /// the tree's push brings it too. One made from the sidebar, with nothing to file in it, is picked,
    /// as Notes and Reminders select a new item in their sidebars; one made from a chat's Add to Folder
    /// leaves the list where it was.
    public func named(_ row: FolderRow?) {
        let picks = naming.map { $0.renaming == nil && $0.filing == nil } ?? false
        naming = nil
        if let row { tree.apply(Update(upsert: [row])) }
        outline.reloadData()
        restoreOpen()
        guard let row, picks else { return }
        picked = row.id
        reselect()
        host?.sidebar(self, pick: row.id)
    }

    /// jawd turned the name down: its words go under the field, which keeps what's typed, and the keys
    /// if it has them; keys gone elsewhere aren't taken back.
    public func notNamed(_ why: String) {
        guard var n = naming else { return }
        n.refused = why
        n.asked = false
        n.away = !typingName
        naming = n
        let row = outline.row(forItem: items[n.renaming ?? Self.namingRow])
        guard row >= 0, let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: false) as? NamingCell else { return }
        cell.show(n, delegate: self, roles: roles)
        outline.noteHeightOfRows(withIndexesChanged: [row])
    }

    /// Return: asks the host for the name typed, unless jawd hasn't answered for the last.
    func commitNaming() {
        guard var n = naming, !n.asked else { return }
        n.asked = true
        naming = n
        host?.sidebar(self, name: n.text, as: n)
    }

    /// Gives the field the keys, its text all selected, or `selection`, as it was before the tree was
    /// laid out again.
    func focusField(_ selection: NSRange? = nil) {
        let row = outline.row(forItem: items[naming?.renaming ?? Self.namingRow])
        guard row >= 0, let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: true) as? NamingCell else { return }
        outline.scrollRowToVisible(row)
        keepingField = true
        view.window?.makeFirstResponder(cell.field)
        keepingField = false
        if let selection {
            cell.field.currentEditor()?.selectedRange = selection
        } else {
            cell.field.currentEditor()?.selectAll(nil)
        }
    }

    /// Whether the name field has the keys.
    var typingName: Bool {
        naming != nil && ((view.window?.firstResponder as? NSTextView)?.delegate as? NSTextField)?.delegate === self
    }

    /// Lays the tree out again without giving up a name being typed: the field is drawn again with
    /// what's typed and, when it had the keys, given them back with its selection as it was.
    func keepingNaming(_ change: () -> Void) {
        let selection = typingName ? (view.window?.firstResponder as? NSTextView)?.selectedRange() : nil
        keepingField = true
        change()
        keepingField = false
        if let selection { focusField(selection) }
    }

    public func controlTextDidChange(_ note: Notification) {
        guard let field = note.object as? NSTextField else { return }
        naming?.text = field.stringValue
        naming?.away = false
    }

    public func control(_: NSControl, textView _: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(NSResponder.insertNewline(_:)):
            commitNaming()
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            cancelNaming()
            return true
        default:
            return false
        }
    }

    /// The field given up other than by Return or Esc, as a click elsewhere gives it up: the name typed
    /// is asked for as Return asks, as Finder, Notes and Xcode keep a name, unless it's empty, unchanged
    /// or what jawd turned down while the keys were elsewhere, which drop it as Esc does. Nothing while
    /// jawd is answering for it.
    public func controlTextDidEndEditing(_: Notification) {
        guard !keepingField, let n = naming, !n.asked else { return }
        let text = n.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, text != n.renaming.flatMap({ tree.row($0)?.name }), !n.away else { return cancelNaming() }
        commitNaming()
    }
}
```

Add to `mac/JawUI/Sources/Sidebar/SidebarCells.swift`:

```swift
/// A folder's name being typed in place, and under it jawd's refusal of the name last asked for.
final class NamingCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("naming")
    let symbol = NSImageView(image: NSImage(systemSymbolName: "folder", accessibilityDescription: nil) ?? NSImage())
    let field = NSTextField()
    let refused = Labels.make("", .caption)

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        field.font = TextStyle.body.nsFont
        field.placeholderString = "Folder name"
        field.lineBreakMode = .byTruncatingTail
        refused.maximumNumberOfLines = 2
        refused.lineBreakMode = .byWordWrapping
        let column = NSStackView(views: [field, refused])
        column.orientation = .vertical
        column.alignment = .leading
        column.spacing = Space.xxs
        let row = NSStackView(views: [symbol, column])
        row.spacing = Space.s
        row.alignment = .top
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            row.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            row.topAnchor.constraint(equalTo: topAnchor, constant: Space.xxs),
            field.trailingAnchor.constraint(equalTo: row.trailingAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ naming: SidebarController.Naming, delegate: NSTextFieldDelegate, roles: Roles?) {
        // Set only when it differs, so a refusal drawn under a field being typed in leaves its caret be.
        if field.stringValue != naming.text { field.stringValue = naming.text }
        field.delegate = delegate
        field.setAccessibilityLabel(naming.renaming == nil ? "The new folder's name" : "The folder's new name")
        refused.stringValue = naming.refused ?? ""
        refused.isHidden = naming.refused == nil
        if let roles { Labels.style(refused, .secondary, roles: roles) }
    }
}
```

In `mac/JawUI/Sources/Sidebar/FolderMenu.swift`, add to `FolderAct`

```swift
    /// A new folder, in `parent` or at the top, named in place, and what to file in it once it's made.
    case newFolder(parent: String?, filing: Filed?)
    /// A folder of yours, its name opened in place.
    case rename(String)
```

in `commands(_:tree:list:act:)`, before `return out`, add

```swift
        out.append(Command("file.new", "New Folder…", phrase: "Add this chat to a new folder", place: nil) {
            act(.newFolder(parent: nil, filing: item))
        })
```

and in `said(_:name:tree:)`, add `case .newFolder, .rename: nil`.

- [ ] **Step 4: Run the JawUI tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the nine new tests among those passed.

- [ ] **Step 5: Wire the app**

Add to the extension in `mac/JawApp/Sources/Folders.swift`:

```swift
    func sidebar(_: SidebarController, name: String, as naming: SidebarController.Naming) {
        if let folder = naming.renaming {
            jawd.folderRename(folder, name: name)
        } else {
            jawd.folderCreate(name: name, parent: naming.parent)
        }
    }

    /// ⇧⌘N: a new folder, in the folder picked when it's yours, or at the top, named in place.
    func newFolder() {
        guard tree.served else { return say("folders come with jawd's M6") }
        folderAct(.newFolder(parent: tree.row(folder)?.isYours == true ? folder : nil, filing: nil), name: "")
    }
```

and to `folderAct(_:name:)`'s switch

```swift
        case let .newFolder(parent, filing):
            window?.showSidebar()
            window?.sidebar.beginNaming(parent: parent, filing: filing)
        case let .rename(id):
            window?.showSidebar()
            window?.sidebar.beginRenaming(id)
```

In `mac/JawApp/Sources/AppModel.swift`, add to `apply(_:)`, before the case that ignores `.named` and `.notNamed` (take them out of it):

```swift
        case let .named(row):
            let filing = window?.sidebar.naming?.filing
            window?.sidebar.named(row)
            if let row, let filing { folderAct(.add(item: filing.id, to: row.id), name: filing.name) }
        case let .notNamed(why):
            window?.sidebar.notNamed(why)
```

In `mac/JawApp/Sources/MainWindowController.swift`, after `toggleSidebar()`, add

```swift
    /// The sidebar unfolded, as naming in place needs it.
    func showSidebar() {
        if sidebarItem.isCollapsed { sidebarItem.animator().isCollapsed = false }
    }
```

If `WindowContent` is in the checkout by then, the same function goes in `WindowContent` as `public func showSidebar() { collapse(sidebarItem, false, animated: true) }`, and `MainWindowController.showSidebar()` calls it.

In `mac/JawApp/Sources/Commands.swift`, add right after the `chat.new` command

```swift
        add(Command("folder.new", "New Folder", phrase: "New folder", detail: "in the folder picked, or at the top", symbol: "folder.badge.plus",
                    key: Shortcut("n", [.command, .shift]), place: .file) { [weak self] in self?.newFolder() })
```

and at the end of the File section

```swift
        add(Command("folder.rename", "Rename Folder", phrase: "Rename this folder", detail: "the folder picked", hint: "↩", place: .file,
                    group: 2, palette: false, enabled: { [weak self] in
                        guard let self else { return false }
                        return tree.row(folder)?.isYours == true
                    })
        { [weak self] in
            guard let self else { return }
            folderAct(.rename(folder), name: "")
        })
```

In `mac/JawApp/Sources/Bench.swift`: add `|| key.hasPrefix("type:")` to `step`; after the `folder:` block, add `if key.hasPrefix("type:") { NSApp.keyWindow?.firstResponder?.insertText(String(key.dropFirst(5))) }`; add `"esc": 53` to `codes`; and make `typed` give `"\u{1b}"` for `esc`, as it gives `"\r"` for `return`.

In `mac/README.md`, add to the steps: "`type:<text>`, which types into what has the keys, such as a folder's name field, and `esc`".

Add a band to `SidebarScene` in `mac/JawHarness/Sources/Scenes/SidebarScenes.swift`, after the tree's:

```swift
            SpecimenGroup(
                title: "Naming in place",
                note: "A new folder's field open under Family, after Cousins, holding \"Cousins\", with jawd's refusal under it in the secondary colour: \"a folder there is called Cousins already\". The row is taller to hold it, and nothing under it is covered.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    s.apply(Samples.folderUpdate)
                    s.beginNaming(parent: "folder:1", text: "Cousins")
                    s.notNamed("a folder there is called Cousins already")
                    return s
                }
                .frame(width: 220, height: 300)
            }
```

- [ ] **Step 6: Run every suite, build the app, and see the fallback on the demo**

Run: `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests`, `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, `** TEST SUCCEEDED **`, `** BUILD SUCCEEDED **`.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/naming.png --keys "cmd+shift+n,sidebar"`
Expected: the panes' line for `cmd+shift+n` with `· said "folders come with jawd's M6"` in it, and `sidebar: All | Archived · picked all`.

- [ ] **Step 7: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/README.md mac/JawHarness/Sources mac/Tests/JawUITests
git commit -m "Folders are named in place in the sidebar: New Folder, ⇧⌘N, in the folder picked when it's yours or at the top, and New Folder… at the end of a chat's Add to Folder, which files the chat in it once jawd has made it, open a field in a row where the folder will go, the folders it's in opened; Return, or a click elsewhere, as Finder, Notes and Xcode keep a name, asks jawd for it, once while jawd answers, unless it's empty or unchanged, and Esc drops it; Return on a folder of yours, or Rename Folder, opens the same field on its row; jawd's refusal of a name is said under the field in jawd's words, the row growing to hold it, and the field keeps what's typed without taking back keys that have gone elsewhere, Esc or giving it up again then dropping it; a push from jawd while a name is typed, its folder's counts or another interface's new folder, leaves the field with the keys, its text and its caret; a folder made is in the tree at once, picked when it was made from the sidebar, as Notes and Reminders select a new item, and not when made from a chat's Add to Folder, which leaves the list where it was; and the snapshot's type: step and esc key reach the field"
```

---

### Task 8: A folder's menu

**Waits on jawd:** nothing to build. Before `folders` lands, the snapshot checks that a folder has no menu; nesting, moving and deleting on a real jawd, and its refusals (moving a folder into itself, a name a sibling has, deleting a folder whose subfolder would then sit beside one of its name, each error 4 and said in the status line in jawd's words), are task 15's.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/SidebarController.swift` (`SidebarHost`, the outline's menu, `hasKeys`)
- Create: `mac/JawUI/Sources/Sidebar/SidebarDrops.swift` (the menu now, the drops in task 9)
- Modify: `mac/JawUI/Sources/Sidebar/SidebarCells.swift` (VoiceOver's Show Menu on a folder)
- Modify: `mac/JawUI/Sources/Sidebar/FolderMenu.swift` (a folder's entries and commands, `moveFolder` and `delete`)
- Modify: `mac/JawApp/Sources/Folders.swift`, `MainWindowController.swift` (`askFolderDelete(_:)`), `Commands.swift` (Delete Folder…), `Bench.swift` (`menu:folder:<row>`, `menu:sidebar`), `mac/README.md`
- Test: `mac/Tests/JawUITests/FolderMenuTests.swift`, `SidebarTests.swift`

**Interfaces:**
- Consumes: `SidebarController`, `FolderCell` (task 4), `FolderMenu`, `FolderAct`, `folderAct(_:name:)` (tasks 6 and 7), `Jawd.folderMove`, `folderDelete` (task 2), `MenuPopUp`.
- Produces: `SidebarHost.sidebar(_:menuFor folder: String?) -> [NSMenuItem]` (nil for a heading, the field or the space under the rows); `SidebarController.menuItems(forRow:) -> [NSMenuItem]` and `hasKeys: Bool`; `FolderAct.moveFolder(String, to: String?, position: Int?)` and `.delete(String)`; `FolderMenu.folderEntries(_ folder: String?, tree:) -> [MenuEntry]` and `@MainActor FolderMenu.folderCommands(_ folder: String?, tree:, act:) -> [Command]`, on the main actor as `commands` is, since it makes `Command`s, while the enum and `folderEntries` stay off it for the tests that call them without it (ids `folder.new.top`, `folder.new.inside`, `folder.rename`, `folder.to.top`, `folder.to:<folder>`, `folder.delete`); `MainWindowController.askFolderDelete(_:)`; the app's `folder.delete` command (⌘⌫); the snapshot's `menu:folder:<row>` and `menu:sidebar`.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawUITests/SidebarTests.swift`, add to `SideHost`:

```swift
    var menusFor: [String?] = []

    func sidebar(_: SidebarController, menuFor folder: String?) -> [NSMenuItem] {
        menusFor.append(folder)
        return [NSMenuItem(title: folder ?? "New Folder…", action: nil, keyEquivalent: "")]
    }
```

and the tests:

```swift
@MainActor
@Test func `a right click on a folder asks for its menu, and on a heading or the space under the rows for the sidebar's`() throws {
    let host = SideHost()
    let s = sidebar(host)
    s.apply(Samples.folderUpdate)
    let menu = try #require(s.outline.menu)
    #expect(menu.delegate === s, "filled for the row right-clicked as it opens")
    #expect(s.menuItems(forRow: 6).map(\.title) == ["folder:1"])
    #expect(s.menuItems(forRow: 5).map(\.title) == ["New Folder…"] && s.menuItems(forRow: -1).map(\.title) == ["New Folder…"])
    #expect(host.menusFor == ["folder:1", nil, nil])
}

@MainActor
@Test func `VoiceOver's Show Menu on a folder's row opens its menu under the row`() throws {
    let host = SideHost()
    let s = sidebar(host)
    s.apply(Samples.folderUpdate)
    let cell = try #require(s.outline.view(atColumn: 0, row: 6, makeIfNecessary: true) as? FolderCell)
    var popped: [(NSMenu, NSPoint)] = []
    cell.popUp.run = { menu, point, _ in popped.append((menu, point)) }
    #expect(cell.accessibilityPerformShowMenu())
    #expect(popped.first?.0.items.map(\.title) == ["folder:1"] && popped.first?.1 == NSPoint(x: cell.bounds.minX, y: cell.bounds.minY))
}
```

Add to `mac/Tests/JawUITests/FolderMenuTests.swift`:

```swift
@MainActor
@Test func `a folder of yours has New Folder Inside, Rename, Move to and Delete, and the others New Folder alone`() {
    let tree = Samples.tree
    let family = folderMenu(FolderMenu.folderEntries("folder:1", tree: tree), FolderMenu.folderCommands("folder:1", tree: tree) { _ in })
    #expect(shows(family) == ["New Folder Inside…", "Rename", "Move to ▸ [The Top], —, Climbing, Friends, Work, · Clients", "Delete…"])
    #expect(family.items.last?.keyEquivalent == "\u{8}" && family.items.last?.keyEquivalentModifierMask == .command, "⌘⌫ shown")
    for id in ["all", "smart:1", nil] as [String?] {
        #expect(shows(folderMenu(FolderMenu.folderEntries(id, tree: tree), FolderMenu.folderCommands(id, tree: tree) { _ in })) == ["New Folder…"])
    }
    #expect(FolderMenu.folderEntries("folder:1", tree: FolderTree()).isEmpty && FolderMenu.folderEntries(nil, tree: FolderTree()).isEmpty)
}

@MainActor
@Test func `a folder moves to the top or into one of yours that isn't it or under it, where it is greyed, and each item does what it says`() {
    let acts = Acts()
    let tree = Samples.tree
    let cousins = folderMenu(FolderMenu.folderEntries("folder:2", tree: tree), FolderMenu.folderCommands("folder:2", tree: tree) { acts.done.append($0) })
    #expect(shows(cousins)[2] == "Move to ▸ The Top, —, [Family], Climbing, Friends, Work, · Clients")
    run(cousins.items[0])
    run(cousins.items[1])
    run(cousins.items[2].submenu?.items[0])
    run(cousins.items[2].submenu?.items[5])
    run(cousins.items[3])
    #expect(acts.done == [.newFolder(parent: "folder:2", filing: nil), .rename("folder:2"), .moveFolder("folder:2", to: nil, position: nil),
                          .moveFolder("folder:2", to: "folder:5", position: nil), .delete("folder:2")])
    let none = Acts()
    run(folderMenu(FolderMenu.folderEntries(nil, tree: tree), FolderMenu.folderCommands(nil, tree: tree) { none.done.append($0) }).items.first)
    #expect(none.done == [.newFolder(parent: nil, filing: nil)], "New Folder… makes one at the top")
}

@Test func `what's said once jawd has moved a folder or deleted it`() {
    let tree = Samples.tree
    #expect(FolderMenu.said(.moveFolder("folder:2", to: nil, position: nil), name: "", tree: tree) == "Cousins moved to the top")
    #expect(FolderMenu.said(.moveFolder("folder:2", to: "folder:5", position: nil), name: "", tree: tree) == "Cousins moved to Work")
    #expect(FolderMenu.said(.delete("folder:2"), name: "", tree: tree) == "Cousins deleted; its chats stay wherever else they're filed")
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `value of type 'SidebarController' has no member 'menuItems'`, `type 'FolderMenu' has no member 'folderEntries'` and `type 'FolderAct' has no member 'moveFolder'`.

- [ ] **Step 3: Write the menu**

In `mac/JawUI/Sources/Sidebar/SidebarController.swift`: add to `SidebarHost`

```swift
    /// The menu of a folder right-clicked, made as it opens; for nil, on a heading or the space under
    /// the rows, the sidebar's own.
    func sidebar(_ sidebar: SidebarController, menuFor folder: String?) -> [NSMenuItem]
```

in `loadView()`, after `outline.sidebar = self`,

```swift
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        outline.menu = menu
```

in `outlineView(_:viewFor:item:)`, after the folder cell is shown, `cell.menuFor = { [weak self] cell in self?.menu(for: cell) }`; and after `said`,

```swift
    /// Whether the tree has the keys, as ⌘⌫ needs it to.
    public var hasKeys: Bool {
        isViewLoaded && view.window?.firstResponder === outline
    }
```

Create `mac/JawUI/Sources/Sidebar/SidebarDrops.swift`:

```swift
import AppKit
import JawKit

/// A folder's menu, and what's dropped on the tree.
extension SidebarController: NSMenuDelegate {
    /// Fills the outline's menu for the row right-clicked, which AppKit outlines while it's open.
    public func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        for item in menuItems(forRow: outline.clickedRow) {
            menu.addItem(item)
        }
    }

    /// The menu of the folder on row `i`, or the sidebar's own on a heading, the field or no row.
    public func menuItems(forRow i: Int) -> [NSMenuItem] {
        let id = i >= 0 ? (outline.item(atRow: i) as? Item)?.id : nil
        return host?.sidebar(self, menuFor: id.flatMap { tree.row($0) != nil ? $0 : nil }) ?? []
    }

    /// The menu of the folder on `cell`'s row, for VoiceOver's Show Menu, which makes no click for the
    /// outline's menu to be filled for.
    func menu(for cell: NSView) -> NSMenu? {
        let items = menuItems(forRow: outline.row(for: cell))
        guard !items.isEmpty else { return nil }
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in items {
            menu.addItem(item)
        }
        return menu
    }
}
```

In `mac/JawUI/Sources/Sidebar/SidebarCells.swift`, add to `FolderCell`

```swift
    /// The menu of the folder on this row, from the sidebar.
    var menuFor: ((FolderCell) -> NSMenu?)?
    var popUp = MenuPopUp()

    /// VoiceOver's Show Menu: the folder's menu under its row.
    override func accessibilityPerformShowMenu() -> Bool {
        popUp.show(menuFor?(self), at: NSPoint(x: bounds.minX, y: bounds.minY), in: self)
    }
```

In `mac/JawUI/Sources/Sidebar/FolderMenu.swift`, add to `FolderAct`

```swift
    /// A folder of yours nested in `to`, or put back at the top, at `position` among its new siblings
    /// or last.
    case moveFolder(String, to: String?, position: Int?)
    /// A folder of yours deleted, after asking.
    case delete(String)
```

add to `FolderMenu`

```swift
    /// A folder's menu in the sidebar: for one of yours, New Folder Inside…, Rename, Move to ▸ (the top,
    /// or a folder of yours that isn't it or under it, where it is now greyed) and Delete…; for a
    /// built-in or smart folder, and on the sidebar's space, New Folder…. None before jawd serves folders.
    public static func folderEntries(_ folder: String?, tree: FolderTree) -> [MenuEntry] {
        guard tree.served else { return [] }
        guard let folder, tree.row(folder)?.isYours == true else { return [.command("folder.new.top")] }
        let under = tree.subtree(folder)
        let targets = tree.yours.filter { !under.contains($0.row.id) }.map { MenuEntry.command("folder.to:\($0.row.id)") }
        return [.command("folder.new.inside"), .command("folder.rename"), .submenu("Move to", [.command("folder.to.top"), .separator] + targets),
                .command("folder.delete")]
    }

    /// The commands those entries name, bound to `folder`.
    @MainActor
    public static func folderCommands(_ folder: String?, tree: FolderTree,
                                      act: @escaping @MainActor @Sendable (FolderAct) -> Void) -> [Command]
    {
        var out = [Command("folder.new.top", "New Folder…", place: nil, palette: false) { act(.newFolder(parent: nil, filing: nil)) }]
        guard let folder, let row = tree.row(folder), row.isYours else { return out }
        let label = tree.label(folder), under = tree.subtree(folder)
        out.append(Command("folder.new.inside", "New Folder Inside…", phrase: "New folder in \(label)", detail: "nested in it", place: nil) {
            act(.newFolder(parent: folder, filing: nil))
        })
        out.append(Command("folder.rename", "Rename", phrase: "Rename \(label)", hint: "↩", place: nil) { act(.rename(folder)) })
        out.append(Command("folder.to.top", "The Top", phrase: "Move \(label) to the top", place: nil, enabled: { row.parent != nil }) {
            act(.moveFolder(folder, to: nil, position: nil))
        })
        for (target, depth) in tree.yours where !under.contains(target.id) {
            out.append(Command("folder.to:\(target.id)", target.name, phrase: "Move \(label) into \(tree.label(target.id))", place: nil,
                               indent: depth, enabled: { row.parent != target.id })
            {
                act(.moveFolder(folder, to: target.id, position: nil))
            })
        }
        out.append(Command("folder.delete", "Delete…", phrase: "Delete the folder \(label)", detail: "its chats stay wherever else they're filed",
                           key: Shortcut("⌫"), place: nil) { act(.delete(folder)) })
        return out
    }
```

and to `said(_:name:tree:)`

```swift
        case let .moveFolder(folder, to, _): "\(tree.label(folder)) moved to \(to.map { tree.label($0) } ?? "the top")"
        case let .delete(folder): "\(tree.label(folder)) deleted; its chats stay wherever else they're filed"
```

- [ ] **Step 4: Run the JawUI tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the five new tests among those passed.

- [ ] **Step 5: Wire the app**

Add to the extension in `mac/JawApp/Sources/Folders.swift`:

```swift
    func sidebar(_: SidebarController, menuFor folder: String?) -> [NSMenuItem] {
        let bound = FolderMenu.folderCommands(folder, tree: tree) { [weak self] act in self?.folderAct(act, name: "") }
        return Menus.items(FolderMenu.folderEntries(folder, tree: tree)) { id in bound.first { $0.id == id } }
    }
```

and to `folderAct(_:name:)`'s switch

```swift
        case let .moveFolder(id, to, position):
            jawd.folderMove(id, parent: to, position: position, said: said)
        case let .delete(id):
            window?.askFolderDelete(id)
```

In `mac/JawApp/Sources/MainWindowController.swift`, add to the Layouts extension, after `askLayoutDelete()`:

```swift
    /// Delete…: asks first, saying the folder's chats stay wherever else they're filed and its
    /// subfolders take its place.
    func askFolderDelete(_ folder: String) {
        guard let window, let row = model.tree.row(folder), row.isYours else { return }
        let alert = NSAlert()
        alert.messageText = "Delete \(row.name)?"
        alert.informativeText = "Its chats stay wherever else they're filed, and its subfolders take its place."
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { [weak model] response in
            guard response == .alertFirstButtonReturn, let model else { return }
            model.jawd.folderDelete(folder, said: FolderMenu.said(.delete(folder), name: row.name, tree: model.tree))
        }
    }
```

In `mac/JawApp/Sources/Commands.swift`, after `folder.rename`, add

```swift
        // ⌘⌫ deletes to a line's start in the box, so it's a folder's only while the tree has the keys.
        add(Command("folder.delete", "Delete Folder…", phrase: "Delete this folder", detail: "after asking", key: Shortcut("⌫"), place: .file,
                    group: 2, palette: false, enabled: { [weak self] in
                        guard let self, let sidebar = window?.sidebar else { return false }
                        return sidebar.hasKeys && tree.row(folder)?.isYours == true
                    })
        { [weak self] in
            guard let self else { return }
            window?.askFolderDelete(folder)
        })
```

In `mac/JawApp/Sources/Bench.swift`, in `menuSaid(_:model:)`, add before `default:`

```swift
        case let ("folder", i?):
            return words(model.window?.sidebar.menuItems(forRow: i) ?? [])
        case ("sidebar", _) where parts.count == 1:
            return words(model.window?.sidebar.menuItems(forRow: -1) ?? [])
```

and add `menu:folder:<row>, menu:sidebar` to the unknown step's list. In `mac/README.md`, add to the `menu:` steps: "`menu:folder:<row>`, the menu of the sidebar's row, and `menu:sidebar`, the sidebar's own".

- [ ] **Step 6: Build the app and see the fallback on the demo**

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/folder-menu.png --keys "menu:folder:0,menu:sidebar"`
Expected: `menu:folder:0: ` and `menu:sidebar: `, each with nothing after it: no menu before jawd serves folders.

- [ ] **Step 7: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/README.md mac/Tests/JawUITests
git commit -m "A folder in the sidebar has a menu: one of yours has New Folder Inside…, Rename, Move to, with the top and every folder of yours that isn't it or under it, where it is now greyed, and Delete…, which asks first, saying its chats stay wherever else they're filed and its subfolders take its place; a built-in or smart folder, a heading and the space under the rows have New Folder…; the outline's menu is filled for the row right-clicked as it opens, and VoiceOver's Show Menu opens it under the row; Delete Folder…'s ⌘⌫ is the tree's only while the tree has the keys, so the box keeps it; and the snapshot prints a folder's menu and the sidebar's"
```

---

### Task 9: Dragging onto the tree

**Waits on jawd:** nothing to build; the drops' rules are tested on their own and the outline's answers to AppKit with a private pasteboard. A chat dragged onto a folder on a real jawd, and a folder nested by a drag, are task 15's.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/FolderTree.swift` (`Dragged`, `drop(_:onto:at:)`)
- Modify: `mac/JawUI/Sources/Sidebar/SidebarDrops.swift` (the outline's drag and drop), `SidebarController.swift` (`SidebarHost`, the types registered)
- Modify: `mac/JawUI/Sources/ChatList/ChatListController.swift` (the pasteboard types, a community's row dragged)
- Modify: `mac/JawApp/Sources/Folders.swift`
- Test: `mac/Tests/JawUITests/FolderTreeTests.swift`, `SidebarTests.swift`, `ChatListTests.swift`

**Interfaces:**
- Consumes: `FolderTree` (task 3), `SidebarController` (task 4), `FolderAct.add`, `.moveFolder` (tasks 6 and 8), `folderAct(_:name:)`.
- Produces: `FolderTree.Dragged` (`item(String, name: String)`, `folder(String)`) and `FolderTree.drop(_:onto:at:) -> FolderAct?`; `SidebarHost.sidebar(_:do act: FolderAct, name: String)`; `SidebarController.place(_:_:) -> (target: String?, at: Int?)?`, `SidebarController.dropTarget(_:row:) -> Item?` and `SidebarController.dragged(_ board: NSPasteboard) -> FolderTree.Dragged?`; `NSPasteboard.PasteboardType.jawPerson`, `.jawFolder` and `.jawCommunity`.

The Questions' 7 was decided after the plan's review: a community's row drags onto your folders like any chat, as the design has "a chat from the chat list" dragged there, carrying `.jawCommunity` rather than `.jawChat`, which the panes register alone (`GridView`), so it can't be dropped on a pane. And a chat dropped near a row's edge, where AppKit proposes a drop between the rows, which a chat can't take, goes onto the folder under the pointer, as `setDropItem(_:dropChildIndex:)` retargets it, rather than being refused.

- [ ] **Step 1: Write the failing tests**

Add to `mac/Tests/JawUITests/FolderTreeTests.swift`:

```swift
@Test func `a chat dropped onto a folder of yours is filed there, and anywhere else isn't a drop`() {
    let tree = Samples.tree
    let chat = FolderTree.Dragged.item("ana@s.whatsapp.net", name: "Ana")
    #expect(tree.drop(chat, onto: "folder:1", at: nil) == .add(item: "ana@s.whatsapp.net", to: "folder:1"))
    #expect(tree.drop(chat, onto: "all", at: nil) == nil && tree.drop(chat, onto: "smart:1", at: nil) == nil)
    #expect(tree.drop(chat, onto: "folder:1", at: 0) == nil && tree.drop(chat, onto: nil, at: 2) == nil, "between rows isn't a folder")
}

@Test func `a folder dropped onto another nests in it, between rows goes there, and never into itself or a folder under it`() {
    let tree = Samples.tree
    #expect(tree.drop(.folder("folder:3"), onto: "folder:1", at: nil) == .moveFolder("folder:3", to: "folder:1", position: nil))
    #expect(tree.drop(.folder("folder:1"), onto: "folder:2", at: nil) == nil, "Cousins is under Family")
    #expect(tree.drop(.folder("folder:1"), onto: "folder:1", at: nil) == nil)
    #expect(tree.drop(.folder("folder:5"), onto: nil, at: 0) == .moveFolder("folder:5", to: nil, position: 0), "Work first at the top")
    #expect(tree.drop(.folder("folder:1"), onto: nil, at: 3) == .moveFolder("folder:1", to: nil, position: 2),
            "before Work, among siblings that don't count Family")
    #expect(tree.drop(.folder("all"), onto: "folder:1", at: nil) == nil && tree.drop(.folder("folder:3"), onto: "smart:1", at: nil) == nil)
}
```

Add to `mac/Tests/JawUITests/SidebarTests.swift`'s `SideHost`

```swift
    var did: [(FolderAct, String)] = []

    func sidebar(_: SidebarController, do act: FolderAct, name: String) {
        did.append((act, name))
    }
```

and the tests:

```swift
@MainActor
@Test func `a drop lands onto a folder, between the rows in one or under Folders, and nowhere among the built-in rows`() {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    let onto = s.place(s.item("folder:1"), NSOutlineViewDropOnItemIndex)
    #expect(onto?.target == "folder:1" && onto?.at == nil)
    let between = s.place(s.item(SidebarController.foldersHeading), 2)
    #expect(between != nil && between?.target == nil && between?.at == 2)
    #expect(s.place(s.item(SidebarController.foldersHeading), NSOutlineViewDropOnItemIndex)?.at == 4, "onto Folders: last at the top")
    #expect(s.place(nil, 0) == nil && s.place(s.item(SidebarController.smartHeading), 0) == nil)
}

@MainActor
@Test func `a folder of yours drags as itself, a built-in one doesn't, and a drag says what it carries`() throws {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    let family = try #require(s.outlineView(s.outline, pasteboardWriterForItem: s.item("folder:1")) as? NSPasteboardItem)
    #expect(family.string(forType: .jawFolder) == "folder:1" && family.string(forType: .string) == "Family")
    #expect(s.outlineView(s.outline, pasteboardWriterForItem: s.item("all")) == nil)
    let board = NSPasteboard(name: .init("jaw-test-\(UUID().uuidString)"))
    defer { board.releaseGlobally() }
    board.clearContents()
    let chat = NSPasteboardItem()
    chat.setString("ana@s.whatsapp.net", forType: .jawChat)
    chat.setString("Ana Ribeiro", forType: .string)
    board.writeObjects([chat])
    #expect(SidebarController.dragged(board) == .item("ana@s.whatsapp.net", name: "Ana Ribeiro"))
    board.clearContents()
    board.writeObjects([family])
    #expect(SidebarController.dragged(board) == .folder("folder:1"))
    board.clearContents()
    let community = NSPasteboardItem()
    community.setString("120363000000000000@g.us", forType: .jawCommunity)
    community.setString("Bairro Alto", forType: .string)
    board.writeObjects([community])
    #expect(SidebarController.dragged(board) == .item("120363000000000000@g.us", name: "Bairro Alto"), "a community files as a chat does")
}

@MainActor
@Test func `a chat dropped near a row's edge goes onto the folder under the pointer, and a folder dropped there goes between the rows`() {
    let s = sidebar()
    s.apply(Samples.folderUpdate)
    let chat = FolderTree.Dragged.item("ana@s.whatsapp.net", name: "Ana")
    #expect(s.dropTarget(chat, row: 6)?.id == "folder:1", "Family's row")
    #expect(s.dropTarget(chat, row: 0) == nil && s.dropTarget(chat, row: 5) == nil, "not All, nor the Folders heading")
    #expect(s.dropTarget(.folder("folder:3"), row: 6) == nil, "a folder near a row's edge goes between the rows, as AppKit proposes")
}
```

Add to `mac/Tests/JawUITests/ChatListTests.swift`:

```swift
@MainActor
@Test func `a community's row drags with a type of its own, which no pane takes, and a chat's as before`() throws {
    let list = ChatListController(folder: "all", roles: Catalog.jaw.roles(dark: true))
    _ = list.view
    list.shown = Samples.chats
    list.table.reloadData()
    let at = try #require(Samples.chats.firstIndex { $0.kind == .community })
    let community = try #require(list.tableView(list.table, pasteboardWriterForRow: at) as? NSPasteboardItem)
    #expect(community.string(forType: .jawCommunity) == Samples.chats[at].id && community.string(forType: .jawChat) == nil)
    #expect(community.string(forType: .string) == Samples.chats[at].name)
    let direct = try #require(Samples.chats.firstIndex { $0.kind == .direct })
    let chat = try #require(list.tableView(list.table, pasteboardWriterForRow: direct) as? NSPasteboardItem)
    #expect(chat.string(forType: .jawChat) == Samples.chats[direct].id && chat.string(forType: .jawCommunity) == nil)
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `type 'FolderTree' has no member 'Dragged'`, `value of type 'SidebarController' has no member 'place'` and `type 'NSPasteboard.PasteboardType' has no member 'jawCommunity'`.

- [ ] **Step 3: Write the drops**

Add to `FolderTree`, after `emptyLine(_:)`:

```swift
    /// What's dragged onto the tree: a chat or someone, by id and name, or a folder.
    public enum Dragged: Sendable, Equatable {
        case item(String, name: String)
        case folder(String)
    }

    /// What dropping `dragged` does, onto `target` (`at` nil) or between the rows in `target` (`at`
    /// their place, a nil `target` being the top): a chat or someone dropped onto a folder of yours is
    /// filed there; a folder of yours dropped onto another is nested in it, last, and between rows goes
    /// there. Built-in and smart folders take nothing, and no folder goes into itself or a folder under
    /// it. Nil when it isn't a drop.
    public func drop(_ dragged: Dragged, onto target: String?, at index: Int?) -> FolderAct? {
        if let target, rows[target]?.isYours != true { return nil }
        switch dragged {
        case let .item(id, _):
            guard let target, index == nil else { return nil }
            return .add(item: id, to: target)
        case let .folder(id):
            guard rows[id]?.isYours == true else { return nil }
            if let target, subtree(id).contains(target) { return nil }
            guard let index else { return target.map { .moveFolder(id, to: $0, position: nil) } }
            // jawd counts a folder's place among its new siblings, which don't count it.
            let at = children(of: target).firstIndex { $0.id == id }.map { $0 < index ? index - 1 : index } ?? index
            return .moveFolder(id, to: target, position: at)
        }
    }
```

In `mac/JawUI/Sources/ChatList/ChatListController.swift`, add to the pasteboard types' extension

```swift
    /// Someone's `person:` id, dragged from the roster.
    static let jawPerson = NSPasteboard.PasteboardType("dev.badcoffee.jaw.person")
    /// A folder of yours' id, dragged in the sidebar.
    static let jawFolder = NSPasteboard.PasteboardType("dev.badcoffee.jaw.folder")
    /// A community's id, dragged from the chat list onto a folder; no pane registers it, as a community
    /// can't go in a pane.
    static let jawCommunity = NSPasteboard.PasteboardType("dev.badcoffee.jaw.community")
```

and replace `tableView(_:pasteboardWriterForRow:)` with

```swift
    /// A chat dragged out of the list, onto a pane or a folder; a community's row, onto a folder only.
    public func tableView(_: NSTableView, pasteboardWriterForRow row: Int) -> (any NSPasteboardWriting)? {
        guard row < shown.count else { return nil }
        let item = NSPasteboardItem()
        item.setString(shown[row].id, forType: shown[row].kind == .community ? .jawCommunity : .jawChat)
        item.setString(shown[row].name, forType: .string)
        return item
    }
```

In `mac/JawUI/Sources/Sidebar/SidebarController.swift`: add to `SidebarHost`

```swift
    /// Something dropped on the tree: what it does, and the name of what was dropped, for what's said.
    func sidebar(_ sidebar: SidebarController, do act: FolderAct, name: String)
```

and in `loadView()`, after the menu,

```swift
        outline.registerForDraggedTypes([.jawChat, .jawCommunity, .jawPerson, .jawFolder])
        outline.setDraggingSourceOperationMask(.move, forLocal: true)
```

Add to `mac/JawUI/Sources/Sidebar/SidebarDrops.swift`:

```swift
extension SidebarController {
    /// Where a drop the outline proposes lands: onto a folder (`at` nil), or between the rows in a folder
    /// of yours, or at the top under Folders (`at` their place); nil anywhere else.
    func place(_ item: Any?, _ index: Int) -> (target: String?, at: Int?)? {
        guard let id = (item as? Item)?.id else { return nil }
        if id == Self.foldersHeading {
            return (nil, index == NSOutlineViewDropOnItemIndex ? tree.children(of: nil).count : index)
        }
        guard tree.row(id) != nil else { return nil }
        return (id, index == NSOutlineViewDropOnItemIndex ? nil : index)
    }

    /// What a drag carries: a folder of yours, or a chat, a community or someone with their name.
    static func dragged(_ board: NSPasteboard) -> FolderTree.Dragged? {
        if let id = board.string(forType: .jawFolder) { return .folder(id) }
        guard let id = board.string(forType: .jawChat) ?? board.string(forType: .jawCommunity) ?? board.string(forType: .jawPerson)
        else { return nil }
        return .item(id, name: board.string(forType: .string) ?? id)
    }

    /// Where a chat, a community or someone dropped between the rows goes: onto the folder of yours on
    /// `row`, the one under the pointer, as AppKit proposes between the rows near a row's edge and only a
    /// folder takes them; nil when that row isn't one of yours, and for a folder, which goes between.
    func dropTarget(_ dragged: FolderTree.Dragged, row: Int) -> Item? {
        guard case .item = dragged, row >= 0, let item = outline.item(atRow: row) as? Item, tree.drop(dragged, onto: item.id, at: nil) != nil
        else { return nil }
        return item
    }

    public func outlineView(_: NSOutlineView, pasteboardWriterForItem item: Any) -> (any NSPasteboardWriting)? {
        guard naming == nil, let id = (item as? Item)?.id, let row = tree.row(id), row.isYours else { return nil }
        let board = NSPasteboardItem()
        board.setString(id, forType: .jawFolder)
        board.setString(row.name, forType: .string)
        return board
    }

    public func outlineView(_ outlineView: NSOutlineView, validateDrop info: any NSDraggingInfo, proposedItem item: Any?,
                            proposedChildIndex index: Int) -> NSDragOperation
    {
        guard let dragged = Self.dragged(info.draggingPasteboard) else { return [] }
        if index != NSOutlineViewDropOnItemIndex,
           let onto = dropTarget(dragged, row: outline.row(at: outline.convert(info.draggingLocation, from: nil)))
        {
            outlineView.setDropItem(onto, dropChildIndex: NSOutlineViewDropOnItemIndex)
            return .move
        }
        guard let spot = place(item, index), tree.drop(dragged, onto: spot.target, at: spot.at) != nil else { return [] }
        return .move
    }

    public func outlineView(_: NSOutlineView, acceptDrop info: any NSDraggingInfo, item: Any?, childIndex index: Int) -> Bool {
        guard let dragged = Self.dragged(info.draggingPasteboard), let spot = place(item, index),
              let act = tree.drop(dragged, onto: spot.target, at: spot.at)
        else { return false }
        let name = switch dragged {
        case let .item(_, name): name
        case let .folder(id): tree.label(id)
        }
        host?.sidebar(self, do: act, name: name)
        return true
    }
}
```

Add to the extension in `mac/JawApp/Sources/Folders.swift`:

```swift
    func sidebar(_: SidebarController, do act: FolderAct, name: String) {
        folderAct(act, name: name)
    }
```

- [ ] **Step 4: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the six new tests among those passed.

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources/Folders.swift mac/Tests/JawUITests
git commit -m "Chats and folders are dragged onto the sidebar's tree: a chat from the list dropped onto a folder of yours is filed there, near a row's edge too, where it goes onto the folder under the pointer, and a folder of yours dropped onto another is nested in it, or between two rows goes there, counted among its new siblings as jawd counts them; a community's row drags onto your folders as a chat's does, with a pasteboard type of its own that no pane takes, so it can't be dropped on a pane; built-in and smart folders take no drops, no folder goes into itself or a folder under it, and a drag onto a pane works as it did; someone from the roster will drop the same way, by their person id"
```

---

### Task 10: Favourites

Favourite ▸ comes here, with ⌘1 to ⌘9, rather than with the other folder items in task 6, so the slots are built and tested in one place: a chat's menus, a folder's, the keys and the Find menu all read `FolderTree`'s slots.

**Waits on jawd:** nothing to build. Before `folders` lands, ⌘1 to ⌘9 say favourites come with jawd's M6; a slot set from the Mac showing in the terminal's Alt-digit, and a favourite chat opened that the list doesn't hold, are task 15's.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/FolderMenu.swift` (`favourites`, `favouriteCommands`, the act, its words; Favourite in a folder's menu; the slots' commands in `commands` and `folderCommands`)
- Modify: `mac/JawUI/Sources/ChatList/ChatMenu.swift` (Favourite in `folderItems`, so in each of a chat's menus)
- Modify: `mac/JawApp/Sources/Folders.swift` (`goToFavourite`, `openChat`, `Lookup`, the act, the slots in `folderItemsKey`), `AppModel.swift` (`lookingUp`, `apply(_:)`), `Commands.swift` (`favouriteCommands()`, `allCommands`), `MainWindowController.swift` (`favouritesChanged()`), `Bench.swift` (the digits' keys)
- Test: `mac/Tests/JawUITests/FolderMenuTests.swift`

**Interfaces:**
- Consumes: `FolderTree.slot(of:)`, `favourite(_:)`, `slotName(_:)` (task 3), `FolderMenu`, `FolderAct` (tasks 6 to 8), `ChatMenu.folderItems` and the app's `folderItemsKey` (task 6), `Jawd.favouriteSet` (task 2), `AppModel.open(_:opened:)`, `openInNewPane(_:)` (the menus plan, on main).
- Produces: `FolderAct.favourite(slot: Int, target: String?)`; `FolderMenu.favourites(_ target: String?, tree:) -> [MenuEntry]` and `@MainActor FolderMenu.favouriteCommands(_ target: String?, name: String, tree:, act:) -> [Command]`, on the main actor as `commands` is (ids `favourite.slot:<n>`, `favourite.off`), which `commands` (for the item's chat, as "this chat") and `folderCommands` (for the folder, by its label) now include; the app's `favourite.1` to `favourite.9` commands (⌘1 to ⌘9, Find ▸ Favourites), `goToFavourite(_:)`, `openChat(_:name:inNewPane:)`, `favouritesChanged()`.

- [ ] **Step 1: Write the failing tests**

Add to `mac/Tests/JawUITests/FolderMenuTests.swift`:

```swift
@MainActor
@Test func `Favourite lists the nine slots saying what's on each, ticked where the chat is, and picking one puts it there`() {
    let acts = Acts()
    let menu = folderMenu(FolderMenu.favourites(Samples.direct.id, tree: Samples.tree),
                          FolderMenu.favouriteCommands(Samples.direct.id, name: "this chat", tree: Samples.tree) { acts.done.append($0) })
    #expect(shows(menu) == ["Favourite ▸ ⌘1  Family, ✓ ⌘2  Ana Ribeiro, ⌘3  Climbing, ⌘4  empty, ⌘5  empty, ⌘6  empty, ⌘7  empty, ⌘8  empty, "
            + "⌘9  empty, —, Remove from Favourites"])
    let slots = menu.items.first?.submenu
    run(slots?.items[3])
    run(slots?.items.last)
    #expect(acts.done == [.favourite(slot: 4, target: Samples.direct.id), .favourite(slot: 2, target: nil)])
}

@MainActor
@Test func `a chat on no slot has no Remove from Favourites, nothing a slot holds has no Favourite, and a folder's puts the folder there`() {
    let tree = Samples.tree
    #expect(!FolderMenu.favouriteCommands(Samples.group.id, name: "this chat", tree: tree) { _ in }.contains { $0.id == "favourite.off" })
    #expect(FolderMenu.favourites(nil, tree: tree).isEmpty && FolderMenu.favourites(Samples.direct.id, tree: FolderTree()).isEmpty)
    let acts = Acts()
    let friends = folderMenu(FolderMenu.favourites("folder:4", tree: tree),
                             FolderMenu.favouriteCommands("folder:4", name: "Friends", tree: tree) { acts.done.append($0) })
    run(friends.items.first?.submenu?.items[0])
    #expect(acts.done == [.favourite(slot: 1, target: "folder:4")])
}

@MainActor
@Test func `the palette puts a chat on a slot by name, saying what's there now, and takes it off its own`() {
    let commands = FolderMenu.favouriteCommands(Samples.direct.id, name: "this chat", tree: Samples.tree) { _ in }
    let listed = commands.filter(\.inPalette).map { "\($0.phrase): \($0.paletteAction.detail)" }
    #expect(listed.first == "Put this chat on ⌘1: Family now" && listed[2] == "Put this chat on ⌘4: nothing on it")
    #expect(!listed.contains { $0.hasPrefix("Put this chat on ⌘2") }, "not the slot it's on")
    #expect(listed.last == "Take this chat off ⌘2: ")
}

@Test func `what's said once a slot is set or emptied`() {
    #expect(FolderMenu.said(.favourite(slot: 3, target: "folder:4"), name: "Friends", tree: Samples.tree) == "⌘3 goes to Friends")
    #expect(FolderMenu.said(.favourite(slot: 2, target: nil), name: "Ana Ribeiro", tree: Samples.tree) == "⌘2 is free")
}
```

and update the three tests whose menus now end with Favourite: in `a community's row has the folder items alone…`, expect `["Add to Folder", "Move to", "unfile", "—", "Favourite"]` for the community, `[…, "Add to Folder", "Move to", "unfile", "—", "Favourite"]` for the chat in the list, and `[…, "Mark as Unread", "—", "Add to Folder", "—", "Favourite", "—", "search.chat"]` for the pane; in `a chat's folder items are one list…`, expect `["Add to Folder", "Move to", "unfile", "—", "Favourite"]` in Family and `["Add to Folder", "—", "Favourite"]` in All, the menu bar's Chat menu having Favourite as the right-click menus do; and in `a folder of yours has New Folder Inside…`, expect

```swift
    #expect(Array(shows(family).prefix(5)) == ["New Folder Inside…", "Rename", "Move to ▸ [The Top], —, Climbing, Friends, Work, · Clients", "Delete…", "—"])
    #expect(shows(family).last?.hasPrefix("Favourite ▸ ✓ ⌘1  Family, ⌘2  Ana Ribeiro") == true)
    #expect(family.items[3].keyEquivalent == "\u{8}" && family.items[3].keyEquivalentModifierMask == .command, "⌘⌫ shown")
```

with, for `all` and `smart:1`, `shows(…).first == "New Folder…"` and `shows(…).last?.hasPrefix("Favourite ▸") == true`, and for nil still `["New Folder…"]`.

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `type 'FolderMenu' has no member 'favourites'` and `type 'FolderAct' has no member 'favourite'`.

- [ ] **Step 3: Write the slots**

In `mac/JawUI/Sources/Sidebar/FolderMenu.swift`, add to `FolderAct`

```swift
    /// A folder or a chat put on a slot, or the slot emptied with nil.
    case favourite(slot: Int, target: String?)
```

add to `FolderMenu`

```swift
    /// Favourite ▸: the nine slots, each saying what's on it, ticked on the one holding `target`, and
    /// Remove from Favourites when one does. None for nothing a slot can hold, nor before jawd serves
    /// folders.
    public static func favourites(_ target: String?, tree: FolderTree) -> [MenuEntry] {
        guard tree.served, target != nil else { return [] }
        return [.submenu("Favourite", (1 ... 9).map { .command("favourite.slot:\($0)") } + [.separator, .command("favourite.off")])]
    }

    /// The commands those entries name: a slot puts `target` there, which takes it off its old slot and
    /// puts out what the slot held, as `favourite.set` does; Remove from Favourites empties its slot.
    /// `name` is what the palette calls it: "this chat", or a folder's name.
    @MainActor
    public static func favouriteCommands(_ target: String?, name: String, tree: FolderTree,
                                         act: @escaping @MainActor @Sendable (FolderAct) -> Void) -> [Command]
    {
        guard let target else { return [] }
        let held = tree.slot(of: target)
        var out = (1 ... 9).map { slot in
            Command("favourite.slot:\(slot)", "⌘\(slot)  \(tree.slotName(slot) ?? "empty")", phrase: "Put \(name) on ⌘\(slot)",
                    detail: tree.slotName(slot).map { "\($0) now" } ?? "nothing on it", place: nil, palette: held != slot,
                    on: { held == slot }) { act(.favourite(slot: slot, target: target)) }
        }
        if let held {
            out.append(Command("favourite.off", "Remove from Favourites", phrase: "Take \(name) off ⌘\(held)", place: nil) {
                act(.favourite(slot: held, target: nil))
            })
        }
        return out
    }
```

make `commands(_:tree:list:act:)` end `return out + favouriteCommands(item.chat, name: "this chat", tree: tree, act: act)`; in `folderCommands(_:tree:act:)`, replace its first guard, `guard let folder, let row = tree.row(folder), row.isYours else { return out }`, with

```swift
        guard let folder else { return out }
        guard let row = tree.row(folder), row.isYours else {
            return out + favouriteCommands(folder, name: tree.label(folder), tree: tree, act: act)
        }
```

and make its last line `return out + favouriteCommands(folder, name: label, tree: tree, act: act)`;

make `folderEntries(_:tree:)` add the folder's Favourite after a separator, for every folder but nil:

```swift
    public static func folderEntries(_ folder: String?, tree: FolderTree) -> [MenuEntry] {
        guard tree.served else { return [] }
        guard let folder else { return [.command("folder.new.top")] }
        guard tree.row(folder)?.isYours == true else { return ChatMenu.with([.command("folder.new.top")], favourites(folder, tree: tree)) }
        let under = tree.subtree(folder)
        let targets = tree.yours.filter { !under.contains($0.row.id) }.map { MenuEntry.command("folder.to:\($0.row.id)") }
        return ChatMenu.with([.command("folder.new.inside"), .command("folder.rename"),
                              .submenu("Move to", [.command("folder.to.top"), .separator] + targets), .command("folder.delete")],
                             favourites(folder, tree: tree))
    }
```

and add to `said(_:name:tree:)`

```swift
        case let .favourite(slot, target): target == nil ? "⌘\(slot) is free" : "⌘\(slot) goes to \(name)"
```

In `mac/JawUI/Sources/ChatList/ChatMenu.swift`, make `folderItems(_:tree:in:)` `with(FolderMenu.entries(Filed(chat), tree: tree, list: folder), FolderMenu.favourites(chat.id, tree: tree))`, so the chat list's menu, the pane's and the menu bar's Chat menu all end with Favourite, and its doc comment say so.

- [ ] **Step 4: Run the JawUI tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the four new tests and the three updated ones among those passed.

- [ ] **Step 5: Wire the app**

In `mac/JawApp/Sources/Folders.swift`, give `sidebar(_:menuFor:)` the folder's name for what's said, so "⌘3 goes to Family" names it:

```swift
    func sidebar(_: SidebarController, menuFor folder: String?) -> [NSMenuItem] {
        let name = folder.map { tree.label($0) } ?? ""
        let bound = FolderMenu.folderCommands(folder, tree: tree) { [weak self] act in self?.folderAct(act, name: name) }
        return Menus.items(FolderMenu.folderEntries(folder, tree: tree)) { id in bound.first { $0.id == id } }
    }
```

add to `folderAct(_:name:)`'s switch

```swift
        case let .favourite(slot, target):
            jawd.favouriteSet(slot: slot, target: target, said: said)
```

make `folderItemsKey` end with the slots, since the menu bar's Favourite says what's on each and ticks the chat's,

```swift
        return [chat.id, folder, chat.folders.joined(separator: ",")] + tree.yours.map { "\($0.depth) \($0.row.id) \($0.row.name)" }
            + (1 ... 9).map { "\($0) \(tree.favourite($0)?.target ?? "") \(tree.slotName($0) ?? "")" }
```

and add to the extension

```swift
    /// ⌘1 to ⌘9: the slot's folder, picked, or its chat, opened in the focused pane.
    func goToFavourite(_ slot: Int) {
        guard tree.served else { return say("favourites come with jawd's M6") }
        guard let f = tree.favourite(slot) else {
            return say("nothing is on ⌘\(slot); a chat's or a folder's Favourite menu puts one there")
        }
        if let folder = f.folder {
            pick(folder: folder)
        } else if let chat = f.chat {
            openChat(chat, name: f.name ?? chat)
        }
    }

    /// A chat the app may hold no row for, as a favourite's or someone's in the roster, to open: at once
    /// when the list or a pane has its row, or else once jawd sends it in a one-row window of All around
    /// it, then of Archived.
    func openChat(_ id: String, name: String, inNewPane: Bool = false) {
        if let row = window?.chatList.row(id) ?? panes[id]?.chat {
            return inNewPane ? openInNewPane(row) : open(row, opened: true)
        }
        if let l = lookingUp { jawd.unwatch(l.window) }
        lookingUp = Lookup(window: jawd.watchChats("all", .around(id, before: 0, after: 0)), chat: id, name: name, archived: false,
                           inNewPane: inNewPane)
    }

    /// A chat being looked for, to open once its row comes.
    struct Lookup {
        var window: WindowID
        var chat: String
        var name: String
        var archived: Bool
        var inNewPane: Bool
    }

    func lookedUp(_ update: Update<ChatRow>) {
        guard var l = lookingUp else { return }
        jawd.unwatch(l.window)
        lookingUp = nil
        if let row = update.upsert.first(where: { $0.id == l.chat }) {
            return l.inNewPane ? openInNewPane(row) : open(row, opened: true)
        }
        guard !l.archived else { return say("\(l.name) isn't among your chats now", warn: true) }
        l.archived = true
        l.window = jawd.watchChats("archived", .around(l.chat, before: 0, after: 0))
        lookingUp = l
    }
```

In `mac/JawApp/Sources/AppModel.swift`: add `var lookingUp: Lookup?` after `var foldersWindow: WindowID?`; in `apply(_:)`, after the list's `.chats` case, add

```swift
        case let .chats(w, u) where w == lookingUp?.window:
            lookedUp(u)
        case let .windowFailed(w, failure) where w == lookingUp?.window:
            lookingUp = nil
            jawd.unwatch(w)
            say("\(failure)", warn: true)
```

and in the tree's `.folders` case, keep what the slots say before `window?.sidebar.apply(u)`, `let slots = (1 ... 9).map { tree.slotName($0) }`, and after it add `if (1 ... 9).map({ tree.slotName($0) }) != slots { window?.favouritesChanged() }`, so the Find menu's Favourites are made again when a slot changes and when a folder on one is renamed, which leaves the slots out of the push.

In `mac/JawApp/Sources/Commands.swift`, add after `layoutCommands()`:

```swift
    /// ⌘1 to ⌘9 in the Find menu's Favourites, each titled by what's on its slot; made again when the
    /// slots change.
    func favouriteCommands() -> [Command] {
        (1 ... 9).map { slot in
            let name = tree.slotName(slot)
            return Command("favourite.\(slot)", name ?? "Empty", phrase: name.map { "Go to \($0)" }, detail: "a favourite",
                           key: Shortcut("\(slot)"), place: .find, submenu: "Favourites", group: 2, palette: name != nil)
            { [weak self] in self?.goToFavourite(slot) }
        }
    }
```

and make `allCommands` `layoutCommands() + favouriteCommands() + commands`.

In `mac/JawApp/Sources/MainWindowController.swift`, add after `layoutsChanged()`:

```swift
    /// The slots changed, or a folder on one was renamed: the Find menu's Favourites say what's on each now.
    func favouritesChanged() {
        let find = model.allCommands.filter { $0.place == .find }
        NSApp.mainMenu?.item(withTitle: MenuPlace.find.rawValue)?.submenu = Menus.menu(find, title: MenuPlace.find.rawValue)
    }
```

In `mac/JawApp/Sources/Bench.swift`, add `"4": 21, "5": 23, "6": 22, "7": 26, "8": 28, "9": 25` to `codes`.

- [ ] **Step 6: Build the app and see the fallback on the demo**

Run: `mac/scripts/build-mac build jaw`
Expected: `** BUILD SUCCEEDED **`, and a debug launch doesn't stop on `Menus.problems`: ⌘1 to ⌘9 were free.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/favourites.png --keys "cmd+1,cmd+9"`
Expected: both panes' lines with `· said "favourites come with jawd's M6"` in them, and the app running on, so the launch's check of the keys passed.

- [ ] **Step 7: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/Tests/JawUITests
git commit -m "Favourites: a chat's menus, the menu bar's Chat menu among them, a community's row and a folder's menu end with Favourite, the nine slots each saying what's on it and ticked on the one holding it, picking one putting it there, which takes it off its old slot and puts out what the slot held, and Remove from Favourites emptying its slot; ⌘1 to ⌘9, in the Find menu's Favourites and titled by what's on each, made again when a slot changes or a folder on one is renamed, pick the slot's folder or open its chat in the focused pane, a chat the list doesn't hold being opened once jawd sends its row in a one-row window of All, then of Archived; an empty slot says so in the status line, and before jawd serves folders each says favourites come with jawd's M6; the sidebar's digits, the menus and the keys all read the slots jawd pushes whole when one changes"
```

---

### Task 11: The palette's folder actions

**Waits on jawd:** nothing to build; the actions are tested by name from `Samples`' tree. Before `folders` lands the palette lists none of them, since they come from the tree, though it lists New folder, ⇧⌘N's, as it lists every command of the app's, which says what it waits on when chosen (task 7); a filing from the palette on a real jawd is task 15's.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/FolderMenu.swift` (`paletteCommands`)
- Modify: `mac/JawApp/Sources/Folders.swift` (`folderPaletteCommands()`), `AppModel.swift` (`actions()`)
- Test: `mac/Tests/JawUITests/FolderPaletteTests.swift`; modify `FolderMenuTests.swift` (`Acts`)

**Interfaces:**
- Consumes: `FolderMenu.commands`, `folderCommands` with the slots' (tasks 6, 8 and 10), `FolderTree.label(_:)` (task 3), `PaletteModel` and `PaletteAction`.
- Produces: `@MainActor FolderMenu.paletteCommands(_ chat: Filed?, folder: String, tree:, act: (FolderAct, String) -> Void) -> [Command]`, on the main actor as the commands it gathers are; `AppModel.folderPaletteCommands() -> [Command]`, `paletteCommands` bound to the focused chat, the folder picked and `folderAct(_:name:)`, listed by `actions()` after the app's own, so they never crowd the palette when nothing's typed.

The palette's composition is JawUI's, so its tests fail without it: what it lists, nothing before jawd serves folders, and what each action asks for.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawUITests/FolderMenuTests.swift`, add `var names: [String] = []` to `Acts`, after `done`.

Create `mac/Tests/JawUITests/FolderPaletteTests.swift`:

```swift
@testable import JawKit
@testable import JawUI
import Testing

/// The palette lists each folder action by name, for the focused chat and the folder picked, so it
/// reaches them without a mode: "Add this chat to Family", "Move this chat to Work", "Put this chat on
/// ⌘3"; each name once, a folder of yours named by its path where another has its name; none before
/// jawd serves folders.

@MainActor
@Test func `the palette names each folder action for the focused chat and the folder picked, each name once`() {
    var tree = Samples.tree
    tree.apply(Update(seq: 301, upsert: [FolderRow(id: "folder:7", v: 61, parent: "folder:4", name: "Cousins", position: 0)]))
    let names = FolderMenu.paletteCommands(Filed(ana(in: ["folder:1"])), folder: "folder:1", tree: tree) { _, _ in }.map(\.phrase)
    #expect(Set(names).count == names.count, "each once: \(names)")
    #expect(names.contains("Take this chat out of Family") && names.contains("Add this chat to Family › Cousins")
        && names.contains("Add this chat to Friends › Cousins") && names.contains("Add this chat to a new folder"))
    #expect(names.contains("Move this chat to Work") && !names.contains("Move this chat to Family"))
    #expect(names.contains("Put this chat on ⌘4") && !names.contains("Put this chat on ⌘2") && names.contains("Take this chat off ⌘2"))
    #expect(names.contains("New folder in Family") && names.contains("Rename Family") && names.contains("Move Family into Work")
        && names.contains("Delete the folder Family") && !names.contains("Move Family to the top"))
    #expect(names.contains("Put Family on ⌘3") && names.contains("Take Family off ⌘1"))
    #expect(!names.contains("Remove from Family") && !names.contains("New Folder…"), "the menus' own, which the palette has by other names")
}

@MainActor
@Test func `the palette lists no folder action before jawd serves folders, and with no chat focused only the folder picked's`() {
    #expect(FolderMenu.paletteCommands(Filed(ana(in: [])), folder: "all", tree: FolderTree()) { _, _ in }.isEmpty)
    let names = FolderMenu.paletteCommands(nil, folder: "folder:3", tree: Samples.tree) { _, _ in }.map(\.phrase)
    #expect(names.contains("Rename Climbing") && !names.contains { $0.contains("this chat") })
}

@MainActor
@Test func `each palette action asks for what it says, naming what it's done to for what's said`() {
    let acts = Acts()
    let commands = FolderMenu.paletteCommands(Filed(ana(in: ["folder:1"])), folder: "folder:3", tree: Samples.tree) { act, name in
        acts.done.append(act)
        acts.names.append(name)
    }
    commands.first { $0.phrase == "Add this chat to Friends" }?.run()
    commands.first { $0.phrase == "Rename Climbing" }?.run()
    #expect(acts.done == [.add(item: Samples.direct.id, to: "folder:4"), .rename("folder:3")])
    #expect(acts.names == ["Ana Ribeiro", "Climbing"])
}

@MainActor
@Test func `a folder's name typed in the palette finds its actions first`() {
    let model = PaletteModel()
    let chat = ana(in: ["folder:1"])
    model.actions = { FolderMenu.paletteCommands(Filed(chat), folder: "all", tree: Samples.tree) { _, _ in }.map(\.paletteAction) }
    model.query = "fri"
    #expect(model.items.first.map { $0.title.map(\.s).joined() } == "Add this chat to Friends")
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `type 'FolderMenu' has no member 'paletteCommands'` and `value of type 'Acts' has no member 'names'`.

- [ ] **Step 3: List them in the palette**

Add to `FolderMenu`, in `mac/JawUI/Sources/Sidebar/FolderMenu.swift`, after `favouriteCommands(_:name:tree:act:)`:

```swift
    /// The folder actions the palette lists by name, made as it opens: for the focused chat, filing it,
    /// moving it while the list shows one of your folders, a new folder for it and its slot; for the
    /// folder picked, a folder inside it, renaming, moving and deleting it, and its slot. Only those the
    /// palette lists and that can be done now; none before jawd serves folders. `act` is told what to
    /// do and the name of what it's done to, for what's said.
    @MainActor
    public static func paletteCommands(_ chat: Filed?, folder: String, tree: FolderTree,
                                       act: @escaping @MainActor @Sendable (FolderAct, String) -> Void) -> [Command]
    {
        guard tree.served else { return [] }
        var out: [Command] = []
        if let chat { out += commands(chat, tree: tree, list: folder) { act($0, chat.name) } }
        let name = tree.label(folder)
        out += folderCommands(folder, tree: tree) { act($0, name) }
        return out.filter { $0.inPalette && $0.enabled() }
    }
```

Add to the extension in `mac/JawApp/Sources/Folders.swift`:

```swift
    /// The folder actions the palette lists, for the focused chat and the folder picked.
    func folderPaletteCommands() -> [Command] {
        FolderMenu.paletteCommands(conversation.map { Filed($0.chat) }, folder: folder, tree: tree) { [weak self] act, name in
            self?.folderAct(act, name: name)
        }
    }
```

In `mac/JawApp/Sources/AppModel.swift`, make `actions()`

```swift
    func actions() -> [PaletteAction] {
        (allCommands + folderPaletteCommands()).filter { $0.inPalette && $0.enabled() }.map(\.paletteAction)
    }
```

- [ ] **Step 4: Run the tests to see them pass, and build the app**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, with the four new tests among those passed, and `** BUILD SUCCEEDED **`. If an expectation on a phrase fails, the phrase is wrong in `FolderMenu`, where tasks 6 to 10 wrote it; correct it there, not here.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/Tests/JawUITests
git commit -m "The palette lists the folder actions by name, made as it opens, so it reaches them without a mode: for the focused chat, Add this chat to and Take this chat out of each folder of yours, Move this chat to another while the list shows one of yours, a new folder, and Put this chat on each slot with what's there now or Take this chat off its own; for the folder picked, a new folder in it, Rename, Move into or to the top, Delete the folder, and its slot; each name once, a folder of yours named by its path where another of yours has its name, after the app's own actions so nothing crowds them when nothing's typed; JawUI's FolderMenu.paletteCommands gathers them, tested for what it lists, for listing none before jawd serves folders and for what each asks"
```

---

### Task 12: The folder keys

**Waits on jawd:** nothing to build. Before `folders` lands, ⌥⇧↑ and ⌥⇧↓ say folders come with jawd's M6; stepping a real jawd's tree is task 15's.

The menu bar's keys come before the box's, so ⌥⇧↑ and ⌥⇧↓ take the box's select to the paragraph's start and end, as ⌥↑ and ⌥↓, Previous Chat and Next Chat, already take its move to them: from anywhere, the keys step the folders, as the terminal's Alt-Shift-Up and Alt-Shift-Down do.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/FolderTree.swift` (`step(from:by:)`, `pickedLine(_:sidebarShown:listShown:focusMode:)`)
- Modify: `mac/JawUI/Sources/Sidebar/SidebarController.swift`, `mac/JawUI/Sources/ChatList/ChatListController.swift` (`isShown`)
- Modify: `mac/JawApp/Sources/Folders.swift` (`stepFolder`), `AppModel.swift` (`pick(folder:saying:)`), `Commands.swift` (Previous Folder, Next Folder)
- Test: `mac/Tests/JawUITests/FolderTreeTests.swift`, `SidebarTests.swift`

**Interfaces:**
- Consumes: `FolderTree.order` (task 3), `SidebarController.select(_:)` (task 4), `AppModel.focusMode`, `goToFavourite(_:)` (task 10).
- Produces: `FolderTree.step(from:by:) -> String`; `FolderTree.pickedLine(_ name: String, sidebarShown: Bool, listShown: Bool, focusMode: Bool) -> String?` (the Questions' 3, A); `SidebarController.isShown` and `ChatListController.isShown`; `AppModel.pick(folder:saying:)` and `stepFolder(_:)`; the commands `folder.previous` (⌥⇧↑) and `folder.next` (⌥⇧↓), in the View menu beside Previous Chat and Next Chat.

- [ ] **Step 1: Write the failing tests**

Add to `mac/Tests/JawUITests/FolderTreeTests.swift`:

```swift
@Test func `the folder before or after, in the tree's order with every folder open, staying at either end`() {
    let tree = Samples.tree
    #expect(tree.step(from: "archived", by: 1) == "folder:1" && tree.step(from: "folder:1", by: 1) == "folder:2", "into Family's Cousins")
    #expect(tree.step(from: "folder:6", by: 1) == "smart:1" && tree.step(from: "smart:3", by: 1) == "smart:3")
    #expect(tree.step(from: "all", by: -1) == "all" && tree.step(from: "folder:5", by: -1) == "folder:4")
    #expect(FolderTree().step(from: "all", by: 1) == "archived")
}

@Test func `a folder a key picks is named while the sidebar isn't shown, with the key that shows its chats when the list isn't either`() {
    #expect(FolderTree.pickedLine("Family", sidebarShown: true, listShown: true, focusMode: false) == nil)
    #expect(FolderTree.pickedLine("Family", sidebarShown: false, listShown: true, focusMode: false) == "Family")
    #expect(FolderTree.pickedLine("Family", sidebarShown: false, listShown: false, focusMode: false) == "Family: ⇧⌘L shows its chats")
    #expect(FolderTree.pickedLine("Family", sidebarShown: false, listShown: false, focusMode: true) == "Family: ⇧⌘↩ shows its chats")
}
```

Add to `mac/Tests/JawUITests/SidebarTests.swift`:

```swift
@MainActor
@Test func `the sidebar is shown only on screen and not folded away`() {
    let s = sidebar()
    #expect(!s.isShown, "in no window")
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 220, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    defer { window.close() }
    window.contentView = s.view
    #expect(s.isShown)
    s.view.isHidden = true
    #expect(!s.isShown)
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `value of type 'FolderTree' has no member 'step'` and `value of type 'SidebarController' has no member 'isShown'`.

- [ ] **Step 3: Write the code**

Add to `FolderTree`:

```swift
    /// The folder before or after `folder`, in the tree's order with every folder open, nested ones
    /// included, as ⌥⇧↑ and ⌥⇧↓ step through it; at either end it stays, as ⌥↑ and ⌥↓ do.
    public func step(from folder: String, by: Int) -> String {
        let order = self.order
        guard !order.isEmpty else { return folder }
        let at = order.firstIndex(of: folder) ?? 0
        return order[min(max(at + by, 0), order.count - 1)]
    }

    /// What the status line says of a folder a key or a favourite picks: nothing while the sidebar shows
    /// it; its name while the chat list shows its chats; and the key that shows them when neither is
    /// shown, as the terminal's footer says it.
    public static func pickedLine(_ name: String, sidebarShown: Bool, listShown: Bool, focusMode: Bool) -> String? {
        if sidebarShown { return nil }
        if listShown { return name }
        return focusMode ? "\(name): ⇧⌘↩ shows its chats" : "\(name): ⇧⌘L shows its chats"
    }
```

Add to `SidebarController`, after `hasKeys`, and to `ChatListController`, after `all`:

```swift
    /// Whether it's on screen: in a window, and not folded away.
    public var isShown: Bool {
        isViewLoaded && view.window != nil && !view.isHiddenOrHasHiddenAncestor && view.frame.width >= 1
    }
```

In `mac/JawApp/Sources/AppModel.swift`, make `pick(folder:)`'s signature `func pick(folder id: String, saying: Bool = false)` and add at its end

```swift
        if saying, let line = FolderTree.pickedLine(tree.label(id), sidebarShown: window?.sidebar.isShown ?? false,
                                                     listShown: window?.chatList.isShown ?? false, focusMode: focusMode)
        {
            say(line)
        }
```

In `mac/JawApp/Sources/Folders.swift`, make `goToFavourite(_:)` call `pick(folder: folder, saying: true)`, and add

```swift
    /// ⌥⇧↑ and ⌥⇧↓: the folder before or after, from anywhere ⌥↑ and ⌥↓ work, picked as a click picks it,
    /// the folders it's in opened.
    func stepFolder(_ by: Int) {
        guard tree.served else { return say("folders come with jawd's M6") }
        let next = tree.step(from: folder, by: by)
        if next != folder { pick(folder: next, saying: true) }
    }
```

In `mac/JawApp/Sources/Commands.swift`, add after `chat.next`

```swift
        add(Command("folder.previous", "Previous Folder", phrase: "The folder before", detail: "in the sidebar",
                    key: Shortcut("↑", [.option, .shift]), place: .view, group: 3, palette: false) { [weak self] in self?.stepFolder(-1) })
        add(Command("folder.next", "Next Folder", phrase: "The folder after", detail: "in the sidebar",
                    key: Shortcut("↓", [.option, .shift]), place: .view, group: 3, palette: false) { [weak self] in self?.stepFolder(1) })
```

- [ ] **Step 4: Run the tests, build the app and see the fallback on the demo**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, with the three new tests among those passed, and `** BUILD SUCCEEDED **`.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/keys.png --keys "opt+shift+down,opt+down"`
Expected: the first panes' line with `· said "folders come with jawd's M6"` in it, and the second's focus moved to the list's next chat, as ⌥↓ did before.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/Tests/JawUITests
git commit -m "⌥⇧↑ and ⌥⇧↓ go to the folder before or after from anywhere ⌥↑ and ⌥↓ work, as the terminal's Alt-Shift-Up and Alt-Shift-Down do, in the tree's order with every folder open, nested ones included, staying at either end, the folders it's in opened in the sidebar; Previous Folder and Next Folder sit beside Previous Chat and Next Chat in the View menu; and while the sidebar isn't shown, a folder a key or a favourite picks is named in the status line, with the key that shows its chats when the chat list isn't shown either, as the terminal's footer names it"
```

---

### Task 13: The roster under the tree

**Waits on jawd:** nothing to build; the roster is drawn from `Samples.people`, and its window is task 2's, against `FakeJawd`. Before `folders` lands the roster isn't shown and isn't watched; a real jawd's people, their presence coming and going and jawd following them as the roster is watched, are task 15's. Who's typing in the roster waits on jawd sending typing at all: today's chat rows carry `typing` empty and the `typing` push isn't sent ("Presence and typing in chats"), so it's tested here from `Samples`, and seen on a real jawd once jawd sends the push and the parity plan's step 3.1 reads it for the panes and the roster alike.

**Files:**
- Create: `mac/JawUI/Sources/Sidebar/RosterLines.swift`, `mac/JawUI/Sources/Sidebar/RosterController.swift`
- Modify: `mac/JawUI/Sources/Sidebar/SidebarController.swift` (the split, `roster`, `keepsDivider`)
- Modify: `mac/JawUI/Sources/Samples.swift` (`people`)
- Modify: `mac/JawApp/Sources/Folders.swift` (`watchRoster()`, the roster's host, `rosterTyping()`), `AppModel.swift` (`rosterWindow`, `apply(_:)`, `pick(folder:saying:)`), `MainWindowController.swift`, `Bench.swift` (`roster`), `mac/README.md`
- Modify: `mac/JawHarness/Sources/Scenes/SidebarScenes.swift` (the roster's scene, and the roster hidden in the tree's bands), `BuiltInScenes.swift`, `mac/Tests/JawHarnessTests/ScenesTests.swift`
- Test: `mac/Tests/JawUITests/RosterTests.swift`; modify `SidebarTests.swift`

**Interfaces:**
- Consumes: `People`, `RosterRow`, `Presence`, `Jawd.watchRoster`, `JawUpdate.roster` (task 2); `FolderTree` (task 3); `HeadingCell` (task 4); `AvatarCache`, `AvatarView`, `Format.ago(_:now:)`; `ChatRow.typing`, `ChatListController.row(_:)` and a pane's `chat`, on main.
- Produces: `public enum RosterLine` (`title(String)`, `heading(String, around: Int, of: Int)`, `person(RosterRow, group: String)`, `offline(group: String, count: Int, open: Bool)`); `RosterLines.lines(_:folder:tree:open:typing:) -> [RosterLine]` and `RosterLines.said(_:typing:now:) -> RosterLines.Said` (`text`, `online`); `public protocol RosterHost` with `roster(_:move:)`; `RosterController` with `host`, `people`, `tree`, `typing: Set<String>`, `roles`, `root`, `now`, `show(folder:)`, `apply(_:)`, `toggle(_:)`, `said`, `rows` (200), `most` (1,000, the most jawd keeps after the top), and internally `lines`, `table`, `grow(visibleTo:)`; `SidebarController.roster` and `keepsDivider`; `Samples.people`; `AppModel.rosterWindow`, `watchRoster()`, `rosterTyping()`; the snapshot's `roster` step.

Someone's presence is `online`, `seen` or `hidden` (task 1); who's typing or recording shows from the chat's typing, which never moves them in jawd's order (the protocol's "Presence and typing in chats"). The roster reads it from each person's one-to-one chat, from the chat's row where the chat list or a pane holds it, as a pane reads its typing on main (`ChatRow.typing`, person ids): someone typing there is around, listed rather than folded, and says typing… in the online colour, never faintly, as typing shows only while jawd hears it. A chat row says who's typing, not whether they're recording, so recording reads typing… too, as the chat list and a pane's header say it, until jawd's `typing` push says which (the parity plan's step 3.1, which moves the panes' typing and this onto it).

The roster grows only once the last person it holds is on a line in sight, never past the 1,000 people jawd keeps after the top (it refuses more with -32602): with its folds closed its lines are few, so the lines in sight say nothing of how near the end of its people it is.

- [ ] **Step 1: Write the failing tests**

Add to `mac/JawUI/Sources/Samples.swift`, after `folderUpdate`:

```swift
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
            RosterRow(id: "person:7", v: 1, sort: "2~avó lurdes\u{1}000000000007", name: "Avó Lurdes", presence: Presence(state: .hidden),
                      chat: "351213770007@s.whatsapp.net", folders: ["folder:1"]),
            RosterRow(id: "person:21", v: 1, sort: "2~joão\u{1}000000000021", name: "João", presence: Presence(state: .hidden),
                      folders: ["folder:3"]),
            RosterRow(id: "person:22", v: 1, sort: "2~leonor\u{1}000000000022", name: "Leonor", presence: Presence(state: .hidden)),
            RosterRow(id: "person:5", v: 1, sort: "2~sofia martins\u{1}000000000005", name: "Sofia Martins", presence: Presence(state: .hidden),
                      chat: "351913550005@s.whatsapp.net", folders: ["folder:2"]),
        ]
    }
```

Create `mac/Tests/JawUITests/RosterTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The roster: under All, grouped by your top folders, then Others, each with who's around listed and
/// the rest folded; under a folder, its people as jawd sorts them, those jawd has no last seen for
/// folded; someone typing in their chat around and listed; faint where jawd isn't following someone;
/// how each is, in words; and more people asked for as it scrolls.

/// The roster's lines as words.
func lineWords(_ lines: [RosterLine]) -> [String] {
    lines.map { line in
        switch line {
        case let .title(text): "\(text):"
        case let .heading(name, around, of): "\(name) \(around) of \(of)"
        case let .person(p, _): p.name
        case let .offline(_, count, open): "\(open ? "▾" : "▸") \(count) offline"
        }
    }
}

/// Family's people, as a roster of Family holds them.
var familyPeople: [RosterRow] {
    Samples.people.filter { !Samples.tree.subtree("folder:1").isDisjoint(with: $0.folders) }
}

@MainActor
final class RosterStub: RosterHost {
    var moved: [Place] = []

    func roster(_: RosterController, move place: Place) {
        moved.append(place)
    }
}

@MainActor
func roster(_ host: RosterStub, folder: String = "all", people: [RosterRow] = Samples.people, total: UInt64 = 400) -> RosterController {
    let r = RosterController(folder: folder)
    r.host = host
    r.tree = Samples.tree
    r.now = { Samples.now }
    r.roles = Catalog.jaw.roles(dark: true)
    _ = r.view
    r.apply(Update(seq: 1, upsert: people, total: total))
    return r
}

@Test func `under All the roster is grouped by your top folders, then Others, each with who's around and the rest folded`() {
    #expect(lineWords(RosterLines.lines(Samples.people, folder: "all", tree: Samples.tree, open: []))
        == ["People:", "Family 1 of 5", "Ana Ribeiro", "▸ 4 offline", "Climbing 2 of 3", "Marta", "Rui Costa", "▸ 1 offline",
            "Work 0 of 1", "▸ 1 offline", "Others 0 of 1", "▸ 1 offline"])
}

@Test func `a fold opened lists its people after it, as jawd sorts them`() {
    let lines = RosterLines.lines(Samples.people, folder: "all", tree: Samples.tree, open: ["folder:1"])
    #expect(Array(lineWords(lines)[1 ... 7]) == ["Family 1 of 5", "Ana Ribeiro", "▾ 4 offline", "Mãe", "Inês Gomes", "Avó Lurdes", "Sofia Martins"])
}

@Test func `under a folder its people are listed as jawd sorts them, and those jawd has no last seen for are folded`() {
    #expect(lineWords(RosterLines.lines(familyPeople, folder: "folder:1", tree: Samples.tree, open: []))
        == ["People in Family:", "Ana Ribeiro", "Mãe", "Inês Gomes", "▸ 2 offline"])
    #expect(lineWords(RosterLines.lines([], folder: "groups", tree: Samples.tree, open: [])) == ["People in Groups:"], "Groups holds nobody")
}

@Test func `someone typing in their chat is around and listed, never folded, though jawd has no last seen for them`() {
    let typing: Set<String> = ["person:7"]
    #expect(Array(lineWords(RosterLines.lines(Samples.people, folder: "all", tree: Samples.tree, open: [], typing: typing))[1 ... 4])
        == ["Family 2 of 5", "Ana Ribeiro", "Avó Lurdes", "▸ 3 offline"], "Avó Lurdes hides her presence, and her chat says she's typing")
    #expect(lineWords(RosterLines.lines(familyPeople, folder: "folder:1", tree: Samples.tree, open: [], typing: typing))
        == ["People in Family:", "Ana Ribeiro", "Mãe", "Inês Gomes", "Avó Lurdes", "▸ 1 offline"], "where jawd sorts her, after those last seen")
}

@Test func `how someone is: typing in the online colour while their chat says so, online, a time ago, and nothing when they hide it`() {
    let now = Samples.now
    #expect(RosterLines.said(Presence(state: .seen, at: now - 60000, known: false), typing: true, now: now) == RosterLines.Said(text: "typing…", online: true),
            "their chat says they're typing or recording, whatever jawd last knew of their presence")
    #expect(RosterLines.said(Presence(state: .online), now: now) == RosterLines.Said(text: "online", online: false))
    #expect(RosterLines.said(Presence(state: .seen, at: now - 12 * 60000), now: now).text == "12 min ago")
    #expect(RosterLines.said(Presence(state: .hidden), now: now).text.isEmpty)
}

@MainActor
@Test func `the roster draws its lines, typing from the chat and never faint, faint where jawd isn't following someone, and opens a fold`() {
    let r = roster(RosterStub())
    #expect(Array(r.said.prefix(4)) == ["People:", "Family 1 of 5", "Ana Ribeiro online", "▸ 4 offline"])
    r.typing = ["person:1"]
    #expect(r.said[2] == "Ana Ribeiro typing…", "her chat says she's typing")
    r.toggle("folder:1")
    #expect(r.said[3] == "▾ 4 offline" && r.said[4] == "Mãe 12 min ago (faint)")
    r.typing = ["person:1", "person:2"]
    #expect(Array(r.said[1 ... 4]) == ["Family 2 of 5", "Ana Ribeiro typing…", "Mãe typing…", "▾ 3 offline"],
            "typing shows only while jawd hears it, so it's never faint")
}

@MainActor
@Test func `the roster asks for more only once the last person it holds is in sight, and never past the 1,000 jawd keeps`() throws {
    let host = RosterStub()
    let r = roster(host)
    r.grow(visibleTo: r.said.count)
    #expect(host.moved.isEmpty, "Sofia, the last it holds, is folded under Family's 4 offline")
    r.toggle("folder:1")
    let sofia = try #require(r.said.firstIndex { $0.hasPrefix("Sofia Martins") })
    r.grow(visibleTo: sofia)
    #expect(host.moved.isEmpty, "the lines in sight end just before her")
    r.grow(visibleTo: sofia + 1)
    #expect(host.moved == [.top(210)], "200 more than it holds")

    let many = (1 ... 900).map { i in
        RosterRow(id: "person:\(100 + i)", v: 1, sort: "0~\(100_000 + i)", name: "Someone \(i)", presence: Presence(state: .online),
                  folders: ["folder:1"])
    }
    let full = RosterStub()
    let big = roster(full, folder: "folder:1", people: many, total: 5000)
    big.grow(visibleTo: big.said.count)
    #expect(full.moved == [.top(1000)], "up to the 1,000 jawd keeps, not 1,100")
    var held = Update<RosterRow>(seq: 2, total: 5000)
    held.moved = Moved(place: .top(1000), newer: 0)
    big.apply(held)
    big.grow(visibleTo: big.said.count)
    #expect(full.moved == [.top(1000)], "nothing more once it's there")
}
```

Add to `mac/Tests/JawUITests/SidebarTests.swift`:

```swift
@MainActor
@Test func `the roster sits under the tree once jawd serves folders, and not before`() {
    let s = sidebar()
    #expect(s.roster.view.isHidden)
    s.apply(Samples.folderUpdate)
    #expect(!s.roster.view.isHidden && s.roster.tree == Samples.tree)
}
```

In `mac/Tests/JawHarnessTests/ScenesTests.swift`, put `"roster"` after `"empty-folder"` in the expected list.

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `cannot find 'RosterLines' in scope` and `cannot find type 'RosterHost' in scope`.

- [ ] **Step 3: Write the lines**

Create `mac/JawUI/Sources/Sidebar/RosterLines.swift`:

```swift
import JawKit

/// A line of the roster: its title, a group's heading under All, someone, or the people folded under
/// "N offline", open or not.
public enum RosterLine: Hashable, Sendable {
    case title(String)
    case heading(String, around: Int, of: Int)
    case person(RosterRow, group: String)
    case offline(group: String, count: Int, open: Bool)
}

/// The roster as the sidebar draws it, and how someone is, in words.
public enum RosterLines {
    /// Under All, grouped by your top folders in their order, then everyone else under Others, each
    /// group's people who are around listed and the rest folded, as the terminal groups them; under
    /// any other folder, its people as jawd sorts them, those jawd has no last seen for folded.
    /// Someone is around while online, or while their chat says they're typing or recording, which
    /// `typing` holds by person id, since jawd keeps no typing on people's rows; someone typing is
    /// never folded. A fold in `open` lists its people after it.
    public static func lines(_ people: [RosterRow], folder: String, tree: FolderTree, open: Set<String>,
                             typing: Set<String> = []) -> [RosterLine]
    {
        var out: [RosterLine] = [.title(folder == "all" ? "People" : "People in \(tree.label(folder))")]
        let around = { (p: RosterRow) in p.presence.around || typing.contains(p.id) }
        func group(_ key: String, _ members: [RosterRow], listed: (RosterRow) -> Bool) {
            out += members.filter(listed).map { .person($0, group: key) }
            let folded = members.filter { !listed($0) }
            guard !folded.isEmpty else { return }
            out.append(.offline(group: key, count: folded.count, open: open.contains(key)))
            if open.contains(key) { out += folded.map { .person($0, group: key) } }
        }
        guard folder == "all" else {
            group(folder, people) { $0.presence.state != .hidden || typing.contains($0.id) }
            return out
        }
        var placed: Set<String> = []
        for top in tree.children(of: nil) {
            let under = tree.subtree(top.id)
            let members = people.filter { !under.isDisjoint(with: $0.folders) }
            guard !members.isEmpty else { continue }
            placed.formUnion(members.map(\.id))
            out.append(.heading(top.name, around: members.count(where: around), of: members.count))
            group(top.id, members, listed: around)
        }
        let rest = people.filter { !placed.contains($0.id) }
        if !rest.isEmpty {
            out.append(.heading("Others", around: rest.count(where: around), of: rest.count))
            group("others", rest, listed: around)
        }
        return out
    }

    /// How someone is, in words, and whether they're drawn in the online colour.
    public struct Said: Equatable, Sendable {
        public var text: String
        public var online: Bool
    }

    /// Typing…, in the online colour, while their chat says they're typing or recording: a chat's row
    /// says who, not which, so recording reads typing… too, as a pane's header says it. Otherwise
    /// online; when last seen, as a time ago; nothing when they hide it.
    public static func said(_ presence: Presence, typing: Bool = false, now: Int64) -> Said {
        if typing { return Said(text: "typing…", online: true) }
        return switch presence.state {
        case .online: Said(text: "online", online: false)
        case .seen: Said(text: presence.at.map { Format.ago($0, now: now) } ?? "", online: false)
        case .hidden: Said(text: "", online: false)
        }
    }
}
```

- [ ] **Step 4: Write the roster**

Create `mac/JawUI/Sources/Sidebar/RosterController.swift`:

```swift
import AppKit
import JawKit
import JawThemes

@MainActor
public protocol RosterHost: AnyObject {
    /// The roster scrolled near its end: its window should hold more people.
    func roster(_ roster: RosterController, move place: Place)
}

/// The picked folder's people, under the folder tree, as `RosterLines` lays them out: each with their
/// avatar, their name and how they are, faint where jawd isn't following them now; 200 at a time from
/// the top, more as it scrolls. Watching them is what tells jawd whose presence to follow.
@MainActor
public final class RosterController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    public weak var host: RosterHost?
    public private(set) var people: People<RosterRow>
    /// The tree, which groups the people under All and names the folder.
    public var tree = FolderTree() {
        didSet { rebuild() }
    }

    /// Who's typing or recording in their one-to-one chat now, by person id, as the app reads it from
    /// the chats' rows.
    public var typing: Set<String> = [] {
        didSet { if typing != oldValue { rebuild() } }
    }

    public var roles: Roles? {
        didSet { if isViewLoaded { table.reloadData() } }
    }

    /// The root jawd's paths are relative to, for avatars.
    public var root: String?
    /// What time it is, for how long ago someone was seen; the harness and the tests set their own.
    public var now: @MainActor () -> Int64 = { Format.now() }
    var open: Set<String> = []
    var lines: [RosterLine] = []
    let table = NSTableView()
    let scroll = NSScrollView()
    var avatars = AvatarCache()
    /// Once a minute while it's on screen, how long ago people were seen is said again.
    var minute: Timer?

    /// How many people the window keeps from the top, grown as the roster scrolls.
    public static let rows: UInt32 = 200
    /// The most people jawd keeps in a roster's window after the top; it refuses more with -32602.
    public static let most: UInt32 = 1000

    public init(folder: String = "all") {
        people = People(folder: folder, place: .top(Self.rows))
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        let column = NSTableColumn(identifier: .init("person"))
        table.addTableColumn(column)
        table.headerView = nil
        table.style = .sourceList
        table.rowHeight = 26
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(clicked)
        table.setAccessibilityLabel("People")
        Scrolling.style(scroll)
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification,
                                               object: scroll.contentView)
        view = scroll
        rebuild()
    }

    override public func viewDidAppear() {
        super.viewDidAppear()
        minute?.invalidate()
        minute = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.table.reloadData() }
        }
    }

    override public func viewDidDisappear() {
        super.viewDidDisappear()
        minute?.invalidate()
        minute = nil
    }

    /// A new folder's people: its window starts again from the top, its folds closed.
    public func show(folder: String) {
        people = People(folder: folder, place: .top(Self.rows))
        open = []
        rebuild()
    }

    public func apply(_ update: Update<RosterRow>) {
        people.apply(update)
        rebuild()
    }

    /// Opens a fold to list the people in it, or closes it, as a click on it does.
    public func toggle(_ group: String) {
        if !open.insert(group).inserted { open.remove(group) }
        rebuild()
    }

    func rebuild() {
        lines = RosterLines.lines(people.list, folder: people.folder, tree: tree, open: open, typing: typing)
        if isViewLoaded { table.reloadData() }
    }

    /// How someone is, typing… from their chat while it says so.
    func how(_ p: RosterRow) -> RosterLines.Said {
        RosterLines.said(p.presence, typing: typing.contains(p.id), now: now())
    }

    /// The roster as it's drawn, a line each, for the tests and the snapshot: its title with a colon, a
    /// group's heading with who's around of how many, someone with how they are and "(faint)" where
    /// jawd isn't following them and they aren't typing, and a fold with ▸ or ▾.
    public var said: [String] {
        lines.map { line in
            switch line {
            case let .title(text): "\(text):"
            case let .heading(name, around, of): "\(name) \(around) of \(of)"
            case let .person(p, _):
                [p.name, how(p).text, p.presence.known || typing.contains(p.id) ? "" : "(faint)"].filter { !$0.isEmpty }
                    .joined(separator: " ")
            case let .offline(_, count, open): "\(open ? "▾" : "▸") \(count) offline"
            }
        }
    }

    @objc func clicked() {
        let i = table.clickedRow
        guard i >= 0, i < lines.count, case let .offline(group, _, _) = lines[i] else { return }
        toggle(group)
    }

    @objc func scrolled() {
        let visible = table.rows(in: scroll.contentView.bounds)
        grow(visibleTo: visible.location + visible.length)
    }

    /// Asks for 200 more people, up to the 1,000 jawd keeps, once the last person held is on a line in
    /// sight, while the folder has more: with its folds closed the roster's lines are few, so the lines
    /// in sight say nothing of how near the end of its people it is.
    func grow(visibleTo end: Int) {
        guard UInt64(people.count) < people.total, people.place.after < Self.most, let last = people.list.last?.id,
              let at = lines.firstIndex(where: { if case let .person(p, _) = $0 { p.id == last } else { false } }), at < end
        else { return }
        let after = min(UInt32(people.count) + Self.rows, Self.most)
        if after > people.place.after { host?.roster(self, move: .top(after)) }
    }

    // MARK: The table

    public func numberOfRows(in _: NSTableView) -> Int {
        lines.count
    }

    public func tableView(_: NSTableView, isGroupRow row: Int) -> Bool {
        if case .title = lines[row] { true } else { false }
    }

    public func tableView(_: NSTableView, shouldSelectRow row: Int) -> Bool {
        if case .person = lines[row] { true } else { false }
    }

    public func tableView(_ tableView: NSTableView, viewFor _: NSTableColumn?, row: Int) -> NSView? {
        switch lines[row] {
        case let .title(text):
            let cell = (tableView.makeView(withIdentifier: HeadingCell.id, owner: self) as? HeadingCell) ?? HeadingCell()
            cell.show(text, roles: roles)
            return cell
        case let .heading(name, around, of):
            let cell = (tableView.makeView(withIdentifier: GroupCell.id, owner: self) as? GroupCell) ?? GroupCell()
            cell.show(name, "\(around) of \(of)", roles: roles)
            return cell
        case let .person(p, _):
            let cell = (tableView.makeView(withIdentifier: PersonCell.id, owner: self) as? PersonCell) ?? PersonCell()
            cell.show(p, said: how(p), avatar: avatars.image(p.avatar, root: root), roles: roles)
            return cell
        case let .offline(_, count, open):
            let cell = (tableView.makeView(withIdentifier: GroupCell.id, owner: self) as? GroupCell) ?? GroupCell()
            cell.show("\(open ? "▾" : "▸") \(count) offline", "", roles: roles)
            return cell
        }
    }
}

/// Someone's row: a dot in the online colour while they're around, their avatar, their name and how
/// they are, all faint where jawd isn't following them now, unless they're typing, which shows only
/// while jawd hears it; their about text, when they have one, as its tooltip.
final class PersonCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("person")
    let dot = NSView()
    let avatar = AvatarView()
    let name = Labels.make("", .callout)
    let state = Labels.make("", .caption)

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        dot.wantsLayer = true
        dot.layer?.cornerRadius = Radius.small
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let stack = NSStackView(views: [dot, avatar, name, state])
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            dot.widthAnchor.constraint(equalToConstant: 2 * Radius.small),
            dot.heightAnchor.constraint(equalToConstant: 2 * Radius.small),
            avatar.widthAnchor.constraint(equalToConstant: 18),
            avatar.heightAnchor.constraint(equalToConstant: 18),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ p: RosterRow, said: RosterLines.Said, avatar image: NSImage?, roles: Roles?) {
        name.stringValue = p.name
        state.stringValue = said.text
        toolTip = p.about
        setAccessibilityLabel([p.name, said.text].filter { !$0.isEmpty }.joined(separator: ", "))
        guard let roles else { return }
        // `said.online` is typing, from their chat.
        let faint = !p.presence.known && !said.online
        avatar.show(p.name, id: p.id, image: image, roles: roles)
        dot.layer?.backgroundColor = ((p.presence.around || said.online) && !faint ? roles.online : roles.online.opacity(0)).cgColor
        Labels.style(name, faint ? .tertiary : .primary, roles: roles)
        state.textColor = (faint ? roles.tertiary : said.online ? roles.online : roles.secondary).nsColor
    }
}

/// A group's heading under All, with who's around of how many, or a fold of people.
final class GroupCell: NSTableCellView {
    static let id = NSUserInterfaceItemIdentifier("group")
    let title = Labels.make("", .caption)
    let count = Labels.make("", .caption)

    init() {
        super.init(frame: .zero)
        identifier = Self.id
        title.font = TextStyle.caption.emphasised
        let stack = NSStackView(views: [title, count])
        stack.spacing = Space.s
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -Space.s),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ text: String, _ counted: String, roles: Roles?) {
        title.stringValue = text
        count.stringValue = counted
        count.isHidden = counted.isEmpty
        guard let roles else { return }
        Labels.style(title, .secondary, roles: roles)
        Labels.style(count, .tertiary, roles: roles)
    }
}
```

In `mac/JawUI/Sources/Sidebar/SidebarController.swift`: add `NSSplitViewDelegate` to the class's conformances; add after `var collapsing = false`

```swift
    /// The picked folder's people, under the tree.
    public let roster = RosterController()
    let split = NSSplitView()
    /// The defaults key the divider is kept under from launch to launch, as the share of the height the
    /// tree has: the app gives one, the harness and the tests none.
    public var keepsDivider: String?
    var placed = false
```

in `loadView()`, replace `view = scroll` with

```swift
        split.isVertical = false
        split.dividerStyle = .thin
        split.delegate = self
        split.addArrangedSubview(scroll)
        addChild(roster)
        split.addArrangedSubview(roster.view)
        roster.view.isHidden = !tree.served
        view = split
```

in `roles`' `didSet`, add `roster.roles = roles` first; in `apply(_:)`, in the branch that lays the tree out again, add `roster.tree = tree`, `roster.view.isHidden = !tree.served` and `view.needsLayout = true` before `return`, so a flood's counts don't redraw the roster; and add

```swift
    override public func viewDidLayout() {
        super.viewDidLayout()
        guard !placed, split.bounds.height > 0, !roster.view.isHidden else { return }
        placed = true
        let kept = keepsDivider.map { UserDefaults.standard.double(forKey: $0) } ?? 0
        split.setPosition(split.bounds.height * (kept > 0 ? kept : 0.6), ofDividerAt: 0)
    }

    public func splitViewDidResizeSubviews(_: Notification) {
        guard placed, let keepsDivider, split.bounds.height > 0, !roster.view.isHidden else { return }
        UserDefaults.standard.set(scroll.frame.height / split.bounds.height, forKey: keepsDivider)
    }
```

- [ ] **Step 5: Run the JawUI tests to see them pass**

Run: `mac/scripts/build-mac test JawUITests`
Expected: `** TEST SUCCEEDED **`, with the eight new tests among those passed.

- [ ] **Step 6: Wire the app**

Add to `mac/JawApp/Sources/Folders.swift`:

```swift
extension AppModel: RosterHost {
    /// The picked folder's people, under the tree; watching them is what tells jawd whose presence to
    /// follow, within its limits.
    func watchRoster() {
        if let w = rosterWindow { jawd.unwatch(w) }
        window?.sidebar.roster.show(folder: folder)
        rosterWindow = jawd.watchRoster(folder, .top(RosterController.rows))
    }

    func roster(_: RosterController, move place: Place) {
        if let w = rosterWindow { jawd.move(w, place) }
    }

    /// Who in the roster is typing or recording in their one-to-one chat, from that chat's row where
    /// the chat list or a pane holds it, as a pane reads its typing; nobody whose chat neither holds,
    /// as jawd sends typing only for the chats an interface's windows show.
    func rosterTyping() -> Set<String> {
        guard let roster = window?.sidebar.roster else { return [] }
        return Set(roster.people.list.compactMap { p in
            guard let chat = p.chat, let row = window?.chatList.row(chat) ?? panes[chat]?.chat, row.typing.contains(p.id) else { return nil }
            return p.id
        })
    }
}
```

In `mac/JawApp/Sources/AppModel.swift`: add `var rosterWindow: WindowID?` after `var lookingUp: Lookup?`; in the `.hello` case, after `window?.chatList.root = w.root`, add `window?.sidebar.roster.root = w.root`; in the tree's `.folders` case, at its end, add `if rosterWindow == nil, tree.served { watchRoster() }`; at the end of the list's `.chats` case, after `glance.chatsChanged()` and its `if`, add `window?.sidebar.roster.typing = rosterTyping()`, as the chat rows it reads came with that update; and add, before `case let .windowFailed(_, failure):`, which would otherwise take a roster's failure first (the case that ignores `.roster` keeps ignoring a roster that's no longer watched),

```swift
        case let .roster(w, u) where w == rosterWindow:
            window?.sidebar.roster.apply(u)
            window?.sidebar.roster.typing = rosterTyping()
        case let .windowFailed(w, failure) where w == rosterWindow:
            say("the people couldn't be watched: \(failure)", warn: true)
```

and in `pick(folder:saying:)`, after `listWindow = jawd.watchChats(…)`, add `if tree.served { watchRoster() }`.

In `mac/JawApp/Sources/MainWindowController.swift`, after `sidebar.keepsOpen = …`, add

```swift
        sidebar.keepsDivider = "jaw.sidebar.divider"
        sidebar.roster.host = model
```

In `mac/JawApp/Sources/Bench.swift`: add `|| key == "roster"` to `step`, and to the log, beside `sidebar`,

```swift
                } else if key == "roster", let roster = model.window?.sidebar.roster {
                    FileHandle.standardError.write(Data("roster: \(roster.said.joined(separator: " | "))\n".utf8))
```

In `mac/README.md`, add to the steps: "`roster`, which prints the roster as it's drawn".

Add to `mac/JawHarness/Sources/Scenes/SidebarScenes.swift`:

```swift
extension HarnessScene {
    static var roster: HarnessScene {
        HarnessScene(
            id: "roster", title: "The roster", symbol: "person.2",
            synopsis: "The picked folder's people under the tree: grouped under All, the rest folded under N offline, faint where jawd isn't following someone",
            section: .chatList, covers: [RosterController.self], width: 760,
        ) { RosterScene(roles: $0.roles) }
    }
}

/// The roster under All, with Family's fold open, and under Family.
private struct RosterScene: View {
    let roles: Roles

    func roster(_ folder: String, _ people: [RosterRow], open: String? = nil) -> some View {
        ControllerSpecimen {
            let r = RosterController(folder: folder)
            r.tree = Samples.tree
            r.now = { Samples.now }
            r.roles = roles
            _ = r.view
            r.apply(Update(seq: 1, upsert: people, total: UInt64(people.count)))
            r.typing = ["person:1"]
            if let open { r.toggle(open) }
            return r
        }
        .frame(width: 220, height: 340)
    }

    var body: some View {
        SpecimenGroup(
            title: "Under All, with Family's fold open, and under Family",
            note: "Ana's chat says she's typing in each. Under All: Family 1 of 5 with Ana typing…, its 4 folded; Climbing 2 of 3 with Marta and Rui online; Work and Others with theirs folded. Opened, Family lists Mãe, Inês, Avó Lurdes and Sofia. Under Family: Ana typing…, Mãe 12 min ago, faint, as jawd isn't following her, and Inês 2 h ago, then 2 offline. typing… is in the online colour, a green dot marks who's around, and no name is cut off.",
            roles: roles,
        ) {
            HStack(alignment: .top, spacing: Space.l) {
                roster("all", Samples.people)
                roster("all", Samples.people, open: "folder:1")
                roster("folder:1", Samples.people.filter { !Samples.tree.subtree("folder:1").isDisjoint(with: $0.folders) })
            }
        }
    }
}
```

and register it in `BuiltInScenes.swift` after `.emptyFolder`: `catalog.register(.roster)`.

The roster now takes 40% of the sidebar's height, under the tree, which would cut the tree off in `SidebarScene`'s bands. The roster has a scene of its own, so hide it there: in the bands "The tree, Family picked" and "Naming in place", add `s.roster.view.isHidden = true` after `s.apply(Samples.folderUpdate)`, and end each band's note with "The roster, drawn in a scene of its own, is hidden here, so the tree has the whole height."

- [ ] **Step 7: Run every suite, build the app and see the fallback on the demo**

Run: `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests`, `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, `** TEST SUCCEEDED **`, `** BUILD SUCCEEDED **`.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac shot roster /tmp/jaw-mac-folders/roster.png`, then `mac/scripts/build-mac shot sidebar /tmp/jaw-mac-folders/sidebar.png`, and look at each picture against its notes: the roster's three, and the sidebar's bands with the tree whole and no roster.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/roster-demo.png --keys "roster,folder:1,roster"`
Expected: `roster: People:` both times, the roster unwatched and hidden, as jawd from main serves no folders, and no line saying the people couldn't be watched.

- [ ] **Step 8: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/README.md mac/JawHarness/Sources mac/Tests/JawUITests mac/Tests/JawHarnessTests/ScenesTests.swift
git commit -m "The roster sits under the folder tree, under a divider kept at the height it's left: the picked folder's people as jawd sorts them, online first, then by when last seen, those jawd has no last seen for folded under N offline; under All, grouped by your top folders and then the others, each with who's around listed and the rest folded, as the terminal groups them; someone whose one-to-one chat says they're typing or recording is around and listed, read from the chat's row where the chat list or a pane holds it, as a pane reads its typing, since jawd's presence is online, last seen or hidden and typing shows from the chat; each row with its avatar, its name and how they are, typing… in the online colour, never faint, online, or when last seen, faint where jawd isn't following them now, their about text as its tooltip, and how long ago they were seen said again once a minute while it's on screen; 200 at a time from the top, 200 more once the last person held comes into sight, never past the 1,000 jawd keeps; watched again for each folder picked, and neither shown nor watched before jawd serves folders; the harness's sidebar bands hide it, as it has a scene of its own"
```

---

### Task 14: The roster's people

**Waits on jawd:** nothing to build. Opening someone's chat from the roster, starting one with someone who has none (`chat.start` with `person`) and filing them on a real jawd are task 15's.

**Files:**
- Modify: `mac/JawUI/Sources/Sidebar/RosterController.swift` (opening, the menu, dragging, `RosterTable`)
- Modify: `mac/JawUI/Sources/Sidebar/RosterLines.swift` (`RosterMenu`)
- Modify: `mac/JawApp/Sources/Folders.swift` (the roster's host, `openPerson`), `AppModel.swift` (`startInNewPane`, `.chatStarted`), `Bench.swift` (`menu:roster:<row>`), `mac/README.md`
- Test: `mac/Tests/JawUITests/RosterTests.swift`, `mac/Tests/JawUITests/SidebarPictureTests.swift`; references `mac/Tests/JawUITests/References/sidebar-dark.png`, `sidebar-light.png`

**Interfaces:**
- Consumes: `RosterController`, `RosterHost` (task 13), `FolderMenu.entries`, `favourites`, `Filed(_: RosterRow)` (tasks 6 and 10), `folderCommands(for:)`, `openChat(_:name:inNewPane:)` (tasks 6 and 10), `Jawd.startChat(person:)`, `compare(_:_:)` (LayoutTests).
- Produces: `RosterHost.roster(_:open:)` and `roster(_:menuItemsFor:) -> [NSMenuItem]`; `RosterController.menuItems(forRow:)`, `openSelected()`, `person(at:)`; `RosterMenu.open` (`person.open`), `RosterMenu.inNewPane` (`person.pane`), `RosterMenu.entries(_:tree:list:)`; `AppModel.openPerson(_:inNewPane:)`; the snapshot's `menu:roster:<row>`; the sidebar's reference pictures.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawUITests/RosterTests.swift`, add to `RosterStub`

```swift
    var opened: [String] = []
    var asked: [String] = []

    func roster(_: RosterController, open person: RosterRow) {
        opened.append(person.id)
    }

    func roster(_: RosterController, menuItemsFor person: RosterRow) -> [NSMenuItem] {
        asked.append(person.id)
        return [NSMenuItem(title: person.chat == nil ? "Start Chat" : "Open Chat", action: nil, keyEquivalent: "")]
    }
```

and the tests:

```swift
@Test func `someone's menu opens or starts their chat, opens it in a new pane, and files them, with their chat's slot when they have one`() {
    let ana = Samples.people[0], marta = Samples.people[1]
    #expect(RosterMenu.entries(ana, tree: Samples.tree, list: "all").map(head)
        == ["Open Chat", "person.pane", "—", "Add to Folder", "—", "Favourite"])
    #expect(RosterMenu.entries(marta, tree: Samples.tree, list: "all").map(head) == ["Start Chat", "person.pane", "—", "Add to Folder"])
    #expect(RosterMenu.entries(ana, tree: Samples.tree, list: "folder:1").map(head)
        == ["Open Chat", "person.pane", "—", "Add to Folder", "Move to", "unfile", "—", "Favourite"])
    #expect(RosterMenu.entries(ana, tree: FolderTree(), list: "all").map(head) == ["Open Chat", "person.pane"])
}

@MainActor
@Test func `Return or a double click opens someone's chat, a right click asks for their menu, and they drag as themselves`() throws {
    let host = RosterStub()
    let r = roster(host)
    r.table.selectRowIndexes([2], byExtendingSelection: false)
    r.openSelected()
    #expect(host.opened == ["person:1"], "Ana Ribeiro, on the roster's third line")
    #expect(r.menuItems(forRow: 2).map(\.title) == ["Open Chat"] && r.menuItems(forRow: 1).isEmpty, "a heading has no menu")
    #expect(host.asked == ["person:1"])
    let item = try #require(r.tableView(r.table, pasteboardWriterForRow: 2) as? NSPasteboardItem)
    #expect(item.string(forType: .jawPerson) == "person:1" && item.string(forType: .string) == "Ana Ribeiro")
    #expect(r.tableView(r.table, pasteboardWriterForRow: 1) == nil)
}
```

Create `mac/Tests/JawUITests/SidebarPictureTests.swift`:

```swift
import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The sidebar whole, the tree over the roster, Family picked and open with its people under it, drawn
/// at 2× by its own views and compared with its reference, dark and light, as LayoutTests compares the
/// conversation's. The sidebar is the same in either look, so it has a picture for each appearance. It's
/// 760 points high, so the tree's 60% holds its fifteen rows whole.

@MainActor
@Test(arguments: [true, false])
func `the sidebar draws as its reference, the tree over Family's people`(dark: Bool) async throws {
    let s = SidebarController()
    s.roles = Catalog.jaw.roles(dark: dark)
    s.roster.now = { Samples.now }
    let window = NSWindow(contentRect: NSRect(x: -30000, y: -30000, width: 220, height: 760), styleMask: [.borderless],
                          backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
    window.contentView = s.view
    defer { window.close() }
    s.apply(Samples.folderUpdate)
    s.expand("folder:1")
    s.select("folder:1")
    s.roster.show(folder: "folder:1")
    s.roster.apply(Update(seq: 1, upsert: familyPeople, total: UInt64(familyPeople.count)))
    s.roster.typing = ["person:1"]
    s.view.layoutSubtreeIfNeeded()
    try await Task.sleep(for: .milliseconds(100))
    let bounds = s.view.bounds
    let rep = try #require(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(bounds.width * 2), pixelsHigh: Int(bounds.height * 2),
                                            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                            bytesPerRow: Int(bounds.width * 2) * 4, bitsPerPixel: 32))
    rep.size = bounds.size
    s.view.cacheDisplay(in: bounds, to: rep)
    try compare(try #require(rep.cgImage), "sidebar-\(dark ? "dark" : "light")")
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the build fails with `cannot find 'RosterMenu' in scope` and `value of type 'RosterController' has no member 'openSelected'`.

- [ ] **Step 3: Write the code**

Add to `mac/JawUI/Sources/Sidebar/RosterLines.swift`:

```swift
/// Someone's menu in the roster.
public enum RosterMenu {
    public static let open = "person.open"
    public static let inNewPane = "person.pane"

    /// Open Chat, or Start Chat when they have none yet, and Open in New Pane; then the folder items,
    /// and Favourite for their chat when they have one.
    public static func entries(_ person: RosterRow, tree: FolderTree, list: String) -> [MenuEntry] {
        let opening: [MenuEntry] = [.command(open, title: person.chat == nil ? "Start Chat" : "Open Chat"), .command(inNewPane)]
        let folders = ChatMenu.with(FolderMenu.entries(Filed(person), tree: tree, list: list), FolderMenu.favourites(person.chat, tree: tree))
        return ChatMenu.with(opening, folders)
    }
}
```

In `mac/JawUI/Sources/Sidebar/RosterController.swift`: add to `RosterHost`

```swift
    /// Someone double-clicked, or Return on them: their chat opened, or one started with them.
    func roster(_ roster: RosterController, open person: RosterRow)
    /// Someone's menu, made as it opens.
    func roster(_ roster: RosterController, menuItemsFor person: RosterRow) -> [NSMenuItem]
```

add `NSMenuDelegate` to the class's conformances; make the table `let table = RosterTable()`; in `loadView()`, after `table.action = #selector(clicked)`, add

```swift
        table.roster = self
        table.doubleAction = #selector(doubled)
        table.setDraggingSourceOperationMask(.move, forLocal: true)
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        table.menu = menu
```

in `tableView(_:viewFor:row:)`'s person case, after the cell is shown, add `cell.menuFor = { [weak self] cell in self?.menu(for: cell) }`; and add after `grow(visibleTo:)`

```swift
    /// Someone on row `i`, when it's someone's row.
    func person(at i: Int) -> RosterRow? {
        guard i >= 0, i < lines.count, case let .person(p, _) = lines[i] else { return nil }
        return p
    }

    @objc func doubled() {
        if let p = person(at: table.clickedRow) { host?.roster(self, open: p) }
    }

    /// Return on someone selected opens their chat, as a double click does.
    func openSelected() {
        if let p = person(at: table.selectedRow) { host?.roster(self, open: p) }
    }

    public func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        for item in menuItems(forRow: table.clickedRow) {
            menu.addItem(item)
        }
    }

    /// The menu of the person on row `i`; none on a title, a heading or a fold.
    public func menuItems(forRow i: Int) -> [NSMenuItem] {
        person(at: i).map { host?.roster(self, menuItemsFor: $0) ?? [] } ?? []
    }

    /// Someone's menu for VoiceOver's Show Menu, which makes no click for the table's menu to be filled for.
    func menu(for cell: NSView) -> NSMenu? {
        let items = menuItems(forRow: table.row(for: cell))
        guard !items.isEmpty else { return nil }
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in items {
            menu.addItem(item)
        }
        return menu
    }

    /// Someone dragged onto a folder of yours, which files them there.
    public func tableView(_: NSTableView, pasteboardWriterForRow row: Int) -> (any NSPasteboardWriting)? {
        guard let p = person(at: row) else { return nil }
        let item = NSPasteboardItem()
        item.setString(p.id, forType: .jawPerson)
        item.setString(p.name, forType: .string)
        return item
    }
```

add to `PersonCell`

```swift
    /// Their menu, from the roster.
    var menuFor: ((PersonCell) -> NSMenu?)?
    var popUp = MenuPopUp()

    /// VoiceOver's Show Menu: their menu under their row.
    override func accessibilityPerformShowMenu() -> Bool {
        popUp.show(menuFor?(self), at: NSPoint(x: bounds.minX, y: bounds.minY), in: self)
    }
```

and at the end of the file

```swift
/// The roster's table: Return opens the selected person's chat.
final class RosterTable: NSTableView {
    weak var roster: RosterController?

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 36 || event.keyCode == 76, event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty,
           let roster
        {
            roster.openSelected()
            return
        }
        super.keyDown(with: event)
    }
}
```

- [ ] **Step 4: Run the tests, and record the references**

Run: `mac/scripts/build-mac test JawUITests`
Expected: the roster's two new tests pass, and the picture test fails twice with `sidebar-dark: no reference yet; wrote sidebar-dark.png to look at and commit` and the same for `sidebar-light`.

Run: `mac/scripts/build-mac references`, then look at `mac/Tests/JawUITests/References/sidebar-dark.png` and `sidebar-light.png`: the tree as the harness's sidebar scene's second band has it, over Family's people as the roster scene's third has them, nothing cut off, the divider between them at 60% of the height. Then run `mac/scripts/build-mac test JawUITests` again.
Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 5: Wire the app**

Add to the `RosterHost` extension in `mac/JawApp/Sources/Folders.swift`:

```swift
    func roster(_: RosterController, open person: RosterRow) {
        openPerson(person)
    }

    func roster(_: RosterController, menuItemsFor person: RosterRow) -> [NSMenuItem] {
        let bound = [
            Command(RosterMenu.open, "Open Chat", place: nil, palette: false) { [weak self] in self?.openPerson(person) },
            Command(RosterMenu.inNewPane, "Open in New Pane", place: nil, palette: false) { [weak self] in
                self?.openPerson(person, inNewPane: true)
            },
        ] + folderCommands(for: Filed(person))
        return Menus.items(RosterMenu.entries(person, tree: tree, list: folder)) { id in bound.first { $0.id == id } }
    }

    /// Someone's chat, opened, or started with them when they have none yet, as `chat.start` with the
    /// person does.
    func openPerson(_ person: RosterRow, inNewPane: Bool = false) {
        if let chat = person.chat { return openChat(chat, name: person.name, inNewPane: inNewPane) }
        startInNewPane = inNewPane
        jawd.startChat(person: person.id)
    }
```

In `mac/JawApp/Sources/AppModel.swift`: add `var startInNewPane = false` after `var rosterWindow: WindowID?`; make the `.chatStarted` case

```swift
        case let .chatStarted(row):
            if startInNewPane {
                startInNewPane = false
                openInNewPane(row)
            } else {
                open(row, opened: true)
            }
```

and add `startInNewPane = false` to the `.notStarted` case.

In `mac/JawApp/Sources/Bench.swift`, in `menuSaid(_:model:)`, add before `default:`

```swift
        case let ("roster", i?):
            return words(model.window?.sidebar.roster.menuItems(forRow: i) ?? [])
```

and add `menu:roster:<row>` to the unknown step's list. In `mac/README.md`, add to the `menu:` steps: "`menu:roster:<row>`, the menu of the roster's row".

- [ ] **Step 6: Run every suite, build the app, and sweep**

Run: `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests`, `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **`, `** TEST SUCCEEDED **`, `** BUILD SUCCEEDED **`.

Run: `mac/scripts/build-mac sweep`, which needs no jawd, and tell the owner the sidebar, empty-folder and roster scenes are on the review canvas (`.atelier/mac-harness.canvas.tsx`) to mark; the marks are read back with `mac/scripts/review-marks`, and a scene marked needs work is fixed in JawUI before the branch lands.

- [ ] **Step 7: Commit**

```bash
git add mac/JawUI/Sources mac/JawApp/Sources mac/README.md mac/Tests/JawUITests
git commit -m "Someone in the roster opens: a double click or Return opens their chat, found by its row or by a one-row window when the list doesn't hold it, or starts one with them when they have none, as chat.start with the person does; their menu has Open Chat, or Start Chat, and Open in New Pane, then the folder items with the folders they're filed in ticked, and Favourite for their chat when they have one; they drag onto a folder of yours as themselves, which files them there; VoiceOver's Show Menu opens their menu under their row; and the sidebar, the tree over Family's people, has reference pictures dark and light"
```

---

### Task 15: Tried end to end, once folders lands

Do this task only once `folders` is on main and `mac-folders` has been rebased onto it: `git show HEAD:protocol/fixtures/folders.watch.result.json`, in the worktree, prints the tree's fixture. Tasks 1 to 14 can land before then, as the parity plan's rule 2 has a step land before jawd's part, each folder action saying what it waits on; this task follows them, as the parity plan's "tried end to end on `jawd demo`", on `mac-folders` rebased onto a main that has `folders`. It doesn't touch `mac/parity.toml`: step 2.1's rows come out in the branch's last commit before it lands ("Where it's built" and "After the plan").

**Files:**
- Delete: `mac/Tests/JawKitTests/Ahead/`
- Modify: `mac/Tests/JawKitTests/FixtureTests.swift` (protocol/fixtures alone), `mac/Tests/JawKitTests/LiveTests.swift` (folders on a real jawd)
- Modify: `mac/JawHarness/Sources/Fixtures/Fixtures.swift` (`tree`), `mac/JawHarness/Sources/Scenes/SidebarScenes.swift` (a band from jawd's fixture), `mac/Tests/JawHarnessTests/ScenesTests.swift`
- Modify: `mac/JawUI/Sources/ChatList/ChatListController.swift` (`drawn`), `mac/JawApp/Sources/Bench.swift` (`--bench pick`)
- Modify: `mac/README.md`

**Interfaces:**
- Consumes: everything above; `DemoJawd`, `drain(_:seconds:until:)`, `eventually(seconds:_:)`, `helloed(_:)`, `notices(_:)`; `Fixtures.result(_:as:)`.
- Produces: `ChatListController.drawn: (() -> Void)?`; `Fixtures.tree: FoldersWatched`; `--bench pick`.

- [ ] **Step 1: Hold the copies against what landed**

Run: `mac/scripts/build-mac test JawKit`
Expected: `** TEST SUCCEEDED **`, `a fixture copied ahead of jawd is byte for byte the one that lands` among those passed. If it fails for a fixture, `folders` changed it on its way to main: read the landed text (`git show HEAD:protocol/README.md`, "Folders and the roster") and bring JawKit's types and their tests to it until the suite passes against the landed files, and say what changed in this task's commit.

- [ ] **Step 2: Read protocol/fixtures alone**

Run: `git rm -r -q mac/Tests/JawKitTests/Ahead`

In `mac/Tests/JawKitTests/FixtureTests.swift`, delete `ahead` and the test `a fixture copied ahead of jawd is byte for byte the one that lands`, make `fixture(_:)` `try Data(contentsOf: fixtures.appending(path: name))` again, and make `fixtureNames()` `try FileManager.default.contentsOfDirectory(atPath: fixtures.path).filter { $0.hasSuffix(".json") }.sorted()`.

In `mac/JawHarness/Sources/Fixtures/Fixtures.swift`, add after `found`:

```swift
    /// `folders.watch`'s answer in `protocol/fixtures/folders.watch.result.json`: the built-in folders
    /// All and Unread, Family with Cousins in it, Mentions me, and three slots.
    static let tree: FoldersWatched = result("folders.watch.result")
```

add to `the protocol's fixtures are in the harness and decode as jawd sends them`, in `mac/Tests/JawHarnessTests/ScenesTests.swift`,

```swift
    #expect(Fixtures.tree.rows.map(\.id) == ["all", "unread", "folder:9", "folder:10", "smart:7"] && Fixtures.tree.favourites.count == 3)
```

and add a band to `SidebarScene`, after the tree's:

```swift
            SpecimenGroup(
                title: "From jawd's fixture",
                note: "protocol/fixtures/folders.watch.result.json as jawd sends it: All with an @ and 12, Unread with ⌘2, an @ and 12, Family with ⌘1, an @ and 3, open to Cousins with 1, and Mentions me with an @ and 1. The roster, drawn in a scene of its own, is hidden here, so the tree has the whole height.",
                roles: roles,
            ) {
                ControllerSpecimen {
                    let s = SidebarController()
                    s.roles = roles
                    _ = s.view
                    s.apply(Update(seq: Fixtures.tree.seq, upsert: Fixtures.tree.rows, favourites: Fixtures.tree.favourites))
                    s.roster.view.isHidden = true
                    s.expand("folder:9")
                    return s
                }
                .frame(width: 220, height: 260)
            }
```

- [ ] **Step 3: Write the live test**

In `mac/Tests/JawKitTests/LiveTests.swift`, add to the live suite (the one enabled when `builtJawd` is there):

```swift
    @Test func `folders on a real jawd: made, a name refused, filed, moved, on a slot, deleted from another interface, and after jawd is killed its list failing once and the tree a fresh watch's`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let jawd = Jawd(socket: demo.socket, client: "jaw-mac-live", version: "0", focus: false) { _ in }
        jawd.start()
        defer { jawd.close() }
        let tree = jawd.watchFolders()
        let list = jawd.watchChats("all", .top(10))
        var rows: [String: FolderRow] = [:]
        var slots: [Favourite] = []
        var chat: String?
        func take(_ all: [JawUpdate]) {
            for update in all {
                switch update {
                case let .folders(w, u) where w == tree:
                    for id in u.remove { rows[id] = nil }
                    for row in u.upsert { rows[row.id] = row }
                    if let f = u.favourites { slots = f }
                case let .chats(w, u) where w == list:
                    chat = chat ?? u.upsert.first { $0.kind == .direct }?.id
                default:
                    break
                }
            }
        }
        func named(_ all: [JawUpdate]) -> String? {
            all.compactMap { if case let .named(row) = $0 { row?.id } else { nil } }.first
        }
        take(await drain(jawd, seconds: 20) { all in
            all.contains { if case .folders = $0 { true } else { false } } && all.contains { if case .chats = $0 { true } else { false } }
        })
        #expect(Set(["all", "unread", "groups", "communities", "archived"]).isSubset(of: Set(rows.keys)))
        #expect(rows.values.count(where: \.isSmart) == 3, "the three smart folders jaw starts with")
        let id = try #require(chat)

        jawd.folderCreate(name: "Family", parent: nil)
        let made = await drain(jawd) { named($0) != nil }
        take(made)
        let family = try #require(named(made))
        jawd.folderCreate(name: "Family", parent: nil)
        let refused = await drain(jawd) { $0.contains { if case .notNamed = $0 { true } else { false } } }
        #expect(refused.contains { if case let .notNamed(why) = $0 { why.contains("Family") } else { false } }, "a sibling's name, in jawd's words")
        jawd.folderCreate(name: "Work", parent: nil)
        let work = try #require(named(await drain(jawd) { named($0) != nil }))

        jawd.folderAdd(family, item: id, said: "filed")
        take(await drain(jawd) { notices($0).contains("filed") })
        #expect(await eventually { take(jawd.drain()); return rows[family]?.count == 1 }, "Family counts the chat once the tree's push comes")
        jawd.moveItem(id, from: family, to: work, said: "moved", both: "in both")
        take(await drain(jawd) { notices($0).contains("moved") })
        #expect(await eventually { take(jawd.drain()); return rows[family]?.count == 0 && rows[work]?.count == 1 },
                "in the other folder and out of the first")
        jawd.favouriteSet(slot: 1, target: work, said: "on")
        take(await drain(jawd) { notices($0).contains("on") })
        #expect(await eventually { take(jawd.drain()); return slots == [Favourite(slot: 1, folder: work)] })

        let inFamily = jawd.watchChats(family, .top(10))
        take(await drain(jawd) { all in all.contains { if case let .chats(w, _) = $0 { w == inFamily } else { false } } })
        let other = Jawd(socket: demo.socket, client: "jaw-mac-live-other", version: "0", focus: false) { _ in }
        other.start()
        defer { other.close() }
        _ = await drain(other) { helloed($0) == 1 }
        other.folderDelete(family, said: "deleted")
        _ = await drain(other) { notices($0).contains("deleted") }
        #expect(await eventually { take(jawd.drain()); return rows[family] == nil },
                "deleted from another interface, it leaves the tree, which FolderTree.showing reads to take the app's list back to All")

        demo.kill()
        try demo.start()
        let back = await drain(jawd, seconds: 20) { all in
            helloed(all) == 1 && all.contains { if case .folders = $0 { true } else { false } }
                && all.contains { if case let .windowFailed(w, _) = $0 { w == inFamily } else { false } }
        }
        take(back)
        #expect(back.contains { if case let .windowFailed(w, f) = $0 { w == inFamily && f.code == ErrorCode.notFound } else { false } },
                "its list, watched again, fails once rather than being asked for ever, and the app takes the list back to All")
        let fresh = Jawd(socket: demo.socket, client: "jaw-mac-live-fresh", version: "0", focus: false) { _ in }
        fresh.start()
        defer { fresh.close() }
        let watched = fresh.watchFolders()
        var freshRows: [String: FolderRow] = [:]
        var freshSlots: [Favourite] = []
        for case let .folders(w, u) in await drain(fresh, seconds: 20, until: { all in all.contains { if case .folders = $0 { true } else { false } } })
            where w == watched
        {
            for row in u.upsert { freshRows[row.id] = row }
            freshSlots = u.favourites ?? []
        }
        #expect(rows == freshRows && slots == freshSlots, "the tree after jawd was killed is a fresh watch's")
    }
```

- [ ] **Step 4: Measure picking a folder**

In `mac/JawUI/Sources/ChatList/ChatListController.swift`, add after `emptyLine`

```swift
    /// Called each time an update has been applied, for the bench.
    public var drawn: (() -> Void)?
```

and at the end of `apply(_:)`, `drawn?()`.

In `mac/JawApp/Sources/Bench.swift`, add to the doc comment "`pick`: picking each folder of the tree, from the pick to its chats drawn, p50 and p99", to `run(_:model:)`'s switch

```swift
        case "pick":
            model.firstRows = {
                DispatchQueue.main.asyncAfter(deadline: .now() + 2) { picks(model, model.tree.order, []) }
            }
```

and after `opens(_:_:_:)`

```swift
    static func picks(_ model: AppModel, _ folders: [String], _ times: [Double]) {
        guard let folder = folders.first else {
            let sorted = times.sorted()
            func p(_ q: Double) -> Double {
                sorted.isEmpty ? 0 : sorted[min(Int(Double(sorted.count) * q), sorted.count - 1)]
            }
            finish(["picks": Double(times.count), "pick_p50_ms": p(0.5), "pick_p99_ms": p(0.99)])
            return
        }
        let start = Date()
        model.window?.chatList.drawn = {
            model.window?.chatList.drawn = nil
            nextFrame {
                let took = ms(Date().timeIntervalSince(start))
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { picks(model, Array(folders.dropFirst()), times + [took]) }
            }
        }
        model.pick(folder: folder)
    }
```

Measure it as the README's "What it costs" was measured: on the build Mac, a Release build against a release `jawd demo` of a million messages in a folder of its own, five runs, the load average beside them. The design's budget is that picking a folder draws its chats within the 30 ms of opening a chat. Add a row to the table, "Picking a folder | 30 ms p99 | …", saying why if it's over. The design's last budget, that the sidebar adds nothing to the keypress budget in a flood, waits for the keypress bench the README says isn't measured yet; task 4 draws only a push's changed rows so that it holds.

- [ ] **Step 5: Run the suites, and the app on the demo with folders**

Run: `df -h .`, then `CARGO_INCREMENTAL=0 cargo build -p jawd --bin jawd`, so `target/debug/jawd` is main's, which serves folders now.

Run: `mac/scripts/build-mac test JawKit`, `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests`, `mac/scripts/build-mac build jaw`
Expected: `** TEST SUCCEEDED **` three times, the live folders test among JawKit's passed, and `** BUILD SUCCEEDED **`.

`build-mac snapshot` prints only the last 12 lines of the app's log, a line for each step and one for the result, so the steps go in two runs, each on a demo folder of its own.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/e2e-folders.png --keys "sidebar,cmd+shift+n,type:Family,return,sidebar,folder:0,menu:list:0,menu:folder:6,menu:sidebar"`
Expected, among the log's lines:
- the first `sidebar:` with the demo's built-in folders, their counts, then `Smart Folders: | Waiting on me … | Mentions me … | Muted, unread …`, and `· picked all`;
- the second with `Folders: | Family` after `Archived`, and `· picked folder:` with Family's number, as a folder made from the sidebar is picked (the Questions' 5);
- for `folder:0`, the panes' line with `· folder all`;
- `menu:list:0:` ending `— | Add to Folder ▸ (Family | — | New Folder…) | — | Favourite ▸ (⌘1  empty | ⌘2  empty | … | ⌘9  empty)`;
- `menu:folder:6: New Folder Inside… | Rename | [Move to] ▸ ([The Top]) | Delete… | — | Favourite ▸ (…)`, Move to greyed because its only item is, as a submenu whose items are all greyed is greyed itself, and `menu:sidebar: New Folder…`;
- and no line saying a window or the folders couldn't be watched.

Run: `mkdir -p /tmp/jaw-mac-folders && mac/scripts/build-mac snapshot /tmp/jaw-mac-folders/e2e-keys.png --keys "cmd+1,roster,opt+shift+down,sidebar"`
Expected, among the log's lines:
- for `cmd+1`, `· said "nothing is on ⌘1; a chat's or a folder's Favourite menu puts one there"`;
- `roster: People: | Others …`, the demo's people, none filed, with how each is;
- for `opt+shift+down`, `· folder unread`, and the `sidebar:` line ending `· picked unread`;
- and no line saying a window, the folders or the people couldn't be watched.

A folder deleted from another interface while the app shows it is tried by the live test above, as the snapshot drives one interface alone: the tree's push takes the folder out, which `FolderTree.showing` reads to put the list back on All (task 5's test), and the folder's list, watched again, fails once with error 3, which the app answers the same way (task 5).

- [ ] **Step 6: Say what the sidebar is, and sweep**

In `mac/README.md`, add to "How the interface is put together":

```
- **The sidebar** (`JawUI/Sources/Sidebar`): `FolderTree` is jawd's folder tree with its favourite slots, laid out by parent and position; `FolderMenu` makes every folder item, a chat's, someone's and a folder's, from it as a menu opens, each asking for a `FolderAct` the app turns into a request; `SidebarController` draws the tree, names folders in place and takes drops, with `RosterController`, the picked folder's people, under it.
```

Run: `mac/scripts/build-mac sweep`, and tell the owner the sidebar, empty-folder and roster scenes are on the review canvas (`.atelier/mac-harness.canvas.tsx`) to mark.

- [ ] **Step 7: Commit**

```bash
git add -A mac/Tests/JawKitTests mac/JawHarness/Sources mac/Tests/JawHarnessTests mac/JawUI/Sources/ChatList/ChatListController.swift mac/JawApp/Sources/Bench.swift mac/README.md
git commit -m "The Mac's folders and roster tried end to end now jawd's folders are on main: the fixtures copied ahead of it were the files that landed, so JawKit reads protocol/fixtures alone again and the harness draws a tree from jawd's own fixture; a live test makes folders on jawd demo, has a sibling's name turned down in jawd's words, files a chat, moves it, puts a folder on a slot, has another interface delete a folder the first is watching, which leaves its tree, and after jawd is killed has that folder's list fail once and the tree a fresh watch's, each waiting on what it checks rather than for a time; the bench times picking each folder; and two snapshots on the demo show the tree, a folder made in place and picked, the menus with their folder items, the roster and the folder keys"
```

---

## After the plan

The branch lands on main as "Where it's built" says, from `~/src/jaw-mac-folders`, once its tasks are done and reviewed: tasks 1 to 14 alone if `folders` isn't on main yet, and task 15 then lands the same way.

- The branch's last commit before it lands takes step 2.1's rows out of `mac/parity.toml`, if they're still in: delete from its `[requests]` the lines for `folders.watch`, `roster.watch`, `folder.create`, `folder.rename`, `folder.move`, `folder.delete`, `folder.add`, `folder.remove` and `favourite.set`, and from its `[pushes]` those for `folders` and `roster`; run `make check-parity`, which passes, listing no step 2.1; and commit `mac/parity.toml` alone: `git commit -m "The parity list's step 2.1 rows are out, as the Mac sends jawd's folder requests and reads its folders and roster now"`.
- Rebase onto main's tip; run `df -h .`, then `CARGO_INCREMENTAL=0 make mac-test`, which builds jawd from the worktree and runs the four suites, then `mac/scripts/build-mac build jaw` and `make check-parity`; and push to main, again from the rebase if main moved first.
- Each time it has landed, delete the build Mac's folder, `~/src/build-mac/jaw/jaw-mac-folders` with its `build/DerivedData` (`ssh build-mac rm -rf src/build-mac/jaw/jaw-mac-folders`), `/tmp/jaw-mac-folders` (the snapshots and shots) and `target` in the worktree (jawd's debug builds, several gigabytes), which task 15 builds again from a main with folders; once task 15 has landed, delete the worktree too (`git -C ~/src/jaw-mac-menus worktree remove ~/src/jaw-mac-folders`).
- The roster reads typing from each person's chat row (task 13), which today's jawd always sends empty; when the parity plan's step 3.1 moves the panes onto jawd's typing push, it has to feed `RosterController.typing` from that push too, or the roster never shows anyone typing. Add that line to step 3.1 in `docs/plans/2026-10-27-jaw-mac-parity-plan.md` in the branch's last commit.
- Tell the owner `mac-folders` has landed on main, with what each commit does, the leans of this plan's Questions that it builds and the three decided after its review, and, until task 15 is done, that the folder items, the roster and ⌘1 to ⌘9 wait on `folders` and say so.

