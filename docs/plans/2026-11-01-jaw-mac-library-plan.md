# Jaw for the Mac: the library, implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** [The parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 2.4 in the Mac app, as [the Mac plan](2026-10-26-jaw-mac-plan.md)'s Mac-M2 step 7 has it and [the Mac design](2026-10-26-jaw-mac-design.md)'s "The library" draws it, behaving as the terminal's Ctrl-L does: the library in a pane, ⌘L for every chat and a chat's header for that chat; the strip of kinds with jawd's counts; the filters as tokens; photos and videos as a grid of 256-pixel previews; voice messages, files and links as rows; Enter to open, as 2.3 opens, and ⌘↩ to the message; and `library.shown` named as the terminal names it. Built on pictures and files (`mac-media` at 60014a35), whose picture slots, cache, Quick Look and `Opened` it draws and opens through, and landing after it.

**Not in this step:** the viewer, which comes after parity (Phase 4), so a photo is looked at with Quick Look, as 2.3 looks at one; playing a voice message, which waits for jawd's voice (step 2.6); dragging an item out; and a library bench, the grid's scrolling measured once `make perf-mac` measures the conversation's.

**Architecture:** JawKit gains a third kind of window beside the chat list and the conversation: `library.watch` with its kind and filters, moved with `watch.move` and watched again whole after a drop or a `resync`, its `library` pushes merged between frames as the others' are, and `library.shown`. JawUI does everything drawn and decided, from values: `LibraryLayout` lays a window's items out at a width (months or shelves as headings, tiles for photos and videos, rows for the rest), each tile a `PictureSlot` the image queue fills from the cache every pane shares; `LibrarySearch` is the search line's words and tokens and what a filter being typed could become; `LibraryController` is the pane, its header, strip, search line and grid; `LibraryPane` says where a library opens among the panes. The app watches each library pane's window, keeps library panes in the layouts beside chats, and opens what's chosen through 2.3's Quick Look and `Opened`.

**Tech Stack:** Swift 6 (language mode 6, warnings as errors), AppKit (`NSCollectionView` with prefetching, on a layout of its own), ImageIO off the main thread through `mac-media`'s image queue, Swift Testing, Tuist through mise; builds and tests on the build Mac through `mac/scripts/build-mac`.

## Questions

What the designs leave open, each with its options and the lean the plan builds; none blocks a task, and each is a small change in the one place named.

1. **Where ⌘L and a chat's header open it.** A: beside the focused pane, a new pane across, as ⌘↩ opens a chat from the palette; in the focused pane when that's empty, and in its place when there's no room to split; ⌘L, or the header's button, again goes to the pane that shows that library (lean); B: in the focused pane, in its chat's place, as picking a chat does, which leaves it nothing to sit beside. Task 6 builds A, in `LibraryPane`.
2. **A library pane in the layouts.** A: a slot whose chat is `library` (every chat) or `library:` and a chat's ID, kept with the layouts as a chat's slot is, so a relaunch or a layout opens it again, never looked up as a chat, and named "Library" or "Library · Lisbon Climbing" in the layout editor (lean); B: left out of what's kept. Task 6.
3. **The scope and `in:`.** A: a pane's scope is the one it was opened for, said in its header; an `in:` token narrows its window to a chat, as the terminal's `in:` takes over its library's chat (lean); B: the scope is an `in:` token that can be taken away. Task 5.
4. **The keys.** A: the search field keeps them, as the terminal's search line does: ↑ and ↓ move the selection by the grid's lines, ← and → by items while the field is empty, Page Up and Page Down by a screen, Tab and ⇧Tab change the kind, Enter opens, ⌘↩ goes to the message, ⌫ in an empty field takes the last token back and Esc closes the pane; while offers show, ↑ and ↓ move through them, Tab or Enter takes one and Esc puts them away (lean); B: the grid takes the keys after ↓. Tasks 2, 4, 5 and 7.
5. **Enter on each kind.** A: a photo, a video or a file is looked at with Quick Look from its clone, as 2.3's Enter looks, the photos and videos here paged through in the library's order from the one selected; one not here is asked for and looked at once it is, as 2.3's are; a link opens in the browser; a voice message says it plays once jawd plays voice (step 2.6); a double-click is Enter (lean); B: a file opens in its app, as ⌘↩ opens a message's. Task 7.
6. **⌘↩.** A: to the message in the pane that shows its chat, selected there, the library staying beside it; otherwise its chat opens in a new pane beside the library, or in the library's place when there's no room, as the terminal's Ctrl-G closes its library (lean); B: always in the library's place. Task 7.
7. **The grid.** A: `NSCollectionView` with prefetching, as the design has it, on a layout of its own placing what `LibraryLayout` laid out, each heading, tile and row an item drawing its value, so everything's place is known before any pixel and the tests' pictures are drawn from the same values; tiles as wide as fits from 96 to 128 points, so at 2x they're 256 pixels at most and take jawd's 256 preview (lean); B: one view drawing it all. Task 2.
8. **Paging a kind.** A: from the top, the kind's newest 240; as the grid comes within two screens of either end of what the window holds, `watch.move` centres it on the item at the top of the screen with 300 each side, that item kept where it was on screen, as the conversation anchors on a message (lean); B: the terminal's, the next 120 after the last, kept up to 2,000. Task 2.
9. **The looks.** A: bubbles' tiles take the messages' pictures' corners and their rows sit on cards; lines' tiles have small corners and their rows are plain lines with hairlines between them, as the terminal's library; compact closes the gaps (lean); B: one look for both. Tasks 2 and 3.
10. **`from:` without the roster.** A: you, the senders of the items held and the members of the chats open in panes (from `chat.info`), since the Mac watches the roster only from step 2.1; the terminal's also offers its roster (lean); B: wait for 2.1. Task 5.
11. **`until:` a day or a month.** A: the whole of it, to its last millisecond, as `until` is inclusive (lean; the terminal sends the day's first millisecond, which leaves the day it names out, a follow-up for the terminal); B: the terminal's. Task 5.

## Global Constraints

- macOS 26 and later, Swift 6 language mode, warnings as errors (`mac/Project.swift`).
- No font, radius or line width set by hand outside `JawUI/Sources/Components` and the layout's own: `mac/scripts/check-components` fails the build on one.
- Everything drawn lives in JawUI and is made from values; the app target defines no view or view controller of its own; `AppModel` is the only code that talks to jawd; the harness names neither `Jawd` nor `Client` (`scripts/check-harness`). Every new component has a harness scene whose `covers` name it.
- The budgets: the grid's decoded pictures share the one 64 MB cache every pane draws from (`AppModel.pictures`), and nothing but drawing runs on the main thread: rows are decoded on the client's queue, pictures on the image queue, four at a time and dropped when scrolled past; the main thread lays out by arithmetic and draws.
- Nothing jumps: a tile's and a row's place comes from the window's items and the width, never from a picture; a move keeps the item on screen where it was.
- jawd's files are read, never written: what's looked at is 2.3's clone (`Opened`). The Mac never touches jawd's data layer; what it needs of jawd is a proposal (P14).
- Words are the design's, the protocol's and the terminal's: "Nothing shared here yet.", "Nothing matches.", jawd's `note` and refusals as it says them, the terminal's offers ("a week ago", "October 2026").
- Fixtures are the protocol's (`protocol/fixtures/library.*.json`); a row in a state no fixture shows is written from the protocol's text.
- Tests run on the build Mac (`mac/scripts/build-mac test …`, `build-mac build jaw`), each run under a lock so it can't start twice, with a jawd built from main's code in the scratch folder; references written there are fetched with `scp`, since `build-mac references` deletes new pictures with its first rsync.
- Never open `~/Library/Application Support/jaw`; the app runs `--demo` or `--home` on a folder of its own with a jawd demo it started; never stop a process by name. Scratch goes in `/tmp/jaw-mac-library/`, deleted before the step reports.
- Commits are on `mac-library`, one a task, in the repository's style: plain sentences saying what changed and why, no prefix, British spelling.

## Where it's built, and what it waits on

- **The branch.** `mac-library`, in `~/src/jaw-mac-library`, made from `mac-media`'s head 60014a35 (31 commits on main at 77d44257). Once `mac-media` is on `origin/main`, it's rebased onto `origin/main`, keeping both sides.
- **jawd.** The library is on main since 77d44257: `library.watch` over photos, videos, voice, files and links, `library.shown`, and the `library` push. `jawd --engine demo` shares media as a phone's history has it, about one message in seven. What it doesn't send that the design draws is P14, below; the Mac builds around it.
- **Landing.** After `mac-media`, once a fresh reviewer has read it, from this worktree; the last task takes the rows it closes out of `mac/parity.toml`.

## Files

- `mac/JawKit/Sources/Protocol/Library.swift` (create, task 1): `LibraryKind`, `LibraryRow`, `LibraryCounts`, `LibraryFilter`, `LibraryWatched`, `LibraryPush`, `LibraryShown`.
- `mac/JawKit/Sources/Windows/Window.swift`, `Client/Client.swift`, `Client/Incoming.swift`, `Client/Jawd.swift` (modify, tasks 1 and 8): `Library<R>`, `counts` and `note` in `Update` and `Merged`, the library window kind, `JawUpdate.library`, `Jawd.watchLibrary` and `Jawd.shown`.
- `mac/JawUI/Sources/Library/` (create, tasks 2 to 8): `LibraryLayout.swift` (headings, tiles and rows at a width), `LibraryDrawing.swift` (each drawn from its value), `LibraryCell.swift` (an item of the grid, asking for its picture and letting it go), `LibraryController.swift` (the pane), `LibraryStrip.swift`, `LibrarySearch.swift`, `LibraryPane.swift`.
- `mac/JawUI/Sources/Conversation/ConversationController.swift` (modify, task 6): the header's Library button. `Files/MediaOffer.swift`, `Files/PendingOpen.swift` (modify, task 7): an item's offer and its pending look.
- `mac/JawApp/Sources/AppModel.swift`, `MainWindowController.swift`, `Commands.swift`, `ChatMenus.swift` (modify, tasks 6 to 8).
- `mac/JawHarness/Sources/Scenes/LibraryScenes.swift` (create), `BuiltInScenes.swift`; `mac/JawUI/Sources/Samples.swift` (the library's made-up items).
- `mac/README.md`, `mac/parity.toml`, the parity plan's step 2.4, and the Mac design's Protocol additions (P14) (task 9).
- Tests: `mac/Tests/JawKitTests/LibraryTests.swift` (create), `FixtureTests.swift`, `LiveTests.swift`; `mac/Tests/JawUITests/LibraryLayoutTests.swift`, `LibrarySearchTests.swift`, `LibraryPaneTests.swift` (create) and their references; `mac/Tests/JawHarnessTests/ScenesTests.swift`.

## The budgets

| What | Budget |
| --- | --- |
| Scrolling the grid | no frame missed at 120 Hz: nothing on the main thread but laying out by arithmetic and drawing ready bitmaps |
| Memory | under 150 MB with a screen of the library; its decoded pictures within the 64 MB cache every pane shares |
| Idle | nothing woken: `library.shown` only as what's on screen changes, no timer or display link |

---

### Task 1: JawKit's library window

**Files:** `Protocol/Library.swift` (create), `Windows/Window.swift`, `Client/Client.swift`, `Client/Incoming.swift`, `Client/Jawd.swift`; tests `JawKitTests/LibraryTests.swift` (create), `FixtureTests.swift`, `LiveTests.swift`.

**Interfaces:** `LibraryKind` (`photo`, `video`, `voice`, `file`, `link`; one jawd adds later reads as `other`, which the library leaves out); `LibraryRow: WindowRow` (`id`, `v`, `sort`, `kind`, `message`, `chat`, `chatName`, `from`, `mine`, `at`, `caption`, `media`, `voice` with `seconds` and, once P14 sends them, `waveform` and `played`, `file` with `name`, `type`, `bytes`, `pages` and `path`, `link` with `url`, `host`, `title` and `description`); `LibraryCounts` (each kind's, `link` absent from a jawd before links, `shared`); `LibraryFilter` (`kind`, nil for every kind, `chat`, `person`, `since`, `until`, `text`), written with `around`, `before` and `after` as `library.watch`'s params; `Library<R>` (newest first: `list`, `total`, `older`, `counts`, `note`, `place`, `loaded`); `Update` and `Merged` gain `counts` and `note`; `Client.watchLibrary(_:_:)`, moved with `watch.move` and watched again after a drop or a `resync` with its filters and place and no `have`, since jawd watches a library whole; `JawUpdate.library(WindowID, Update<LibraryRow>)`; `Jawd.watchLibrary(_:_:)`.

**Tests:** `library.watch.request.json` equals what JawKit writes for its filter; the result and the push decode: the photo here with its path, both previews and its thumbnail; the file yours, with no sender, its name, type, size and pages; the voice message's 14 seconds; the link's `~0` id, URL, host, title and description; the item removed; every fixture still decodes, `library.watch` as a library window and `library` as its push; the window model keeps the newest first, applies a push's upserts and removes, keeps `total`, `older` and `counts` until they change and a `note` until the next answer, and a merge of frames ends as applying each; `FakeJawd`: watched, pushed, moved with the same filters, watched again with them after a drop and a `resync`, nothing applied once unwatched; live on `jawd --engine demo`: `all` holds each kind's newest with photos counted, a photo window's previews are files under jawd's root, and a move around its last item brings older items, newest first, none twice.

**Done when** JawKit passes on the build Mac.

### Task 2: The grid of previews for photos and videos

**Files:** `JawUI/Sources/Library/LibraryLayout.swift`, `LibraryDrawing.swift`, `LibraryCell.swift`, `LibraryController.swift` (create); `Samples.swift`; `JawHarness/Sources/Scenes/LibraryScenes.swift` (create), `BuiltInScenes.swift`; tests `JawUITests/LibraryLayoutTests.swift` (create) with references `library-photos-{bubbles,lines}-{dark,light}.png`, `ScenesTests.swift`.

**What:** a kind of photos or videos newest first under its months ("October 2026"), its tiles as wide as fits from 96 to 128 points, 2 points apart (question 7), each a picture slot filled from the centre from jawd's 256 preview, its blurred thumbnail first, fading in as a conversation's pictures do, a video's length on its tile, the selected one ringed in the accent; the cells ask for their tiles' pictures in the cache every pane shares, prefetched as the grid nears them, and let them go when they're reused, scrolled past or the window closes; the window pages as question 8 has it; and the empty states: "Nothing shared here yet.", "Nothing matches.", jawd's `note`, or its words for refusing the window. In both looks (question 9).

**Tests:** the tiles' frames from the width, 96 to 128 points and 2 apart, at every width a resize passes; each tile's slot takes the 256 preview at 2x and at 1x; a video's tile has its length; months head their items; a picture landing changes no frame; a cell asks for its tile once and lets it go when reused, and a cancelled prefetch lets go; a move that drops items above keeps the item on screen where it was; the references in both looks, dark and light; the harness's Library grid scene.

**Done when** JawUITests and JawHarnessTests pass on the build Mac and the scene is shot in both looks.

### Task 3: Rows for voice, files and links

**Files:** `LibraryLayout.swift`, `LibraryDrawing.swift`; `LibraryScenes.swift`; tests `LibraryLayoutTests.swift` with references `library-rows-*` and `library-all-*`.

**What:** voice messages as rows with ▶, their waveform (even bars until P14 sends one), their length, and who, where and when; files with their type's symbol, name, and type, size and pages, then who, where and when; links with the link's well, title (or URL) and host, then who, where and when; each row's height fixed by its kind. All shows a shelf of each, as the terminal's: photos and videos in two lines of tiles, three files, three links and two voice messages, each headed by its kind and count, a click on a heading going to its kind; a shelf with nothing isn't drawn.

**Tests:** each kind's row: its parts, its height, a long name cut short with what follows it whole; All's shelves take what they say from each kind's items; a heading's region is its kind; references in both looks; the Library rows scene.

**Done when** the suites pass and the scene is shot.

### Task 4: The kinds with their counts

**Files:** `LibraryStrip.swift` (create), `LibraryController.swift`; `LibraryScenes.swift`; tests `LibraryLayoutTests.swift` with references `library-strip-*`.

**What:** the strip: All, Photos 214, Videos 31, Voice 58, Files 12, Links 40, from jawd's counts, `1,000+` where jawd counts only so far (with words, a person or a time); a click, or Tab and ⇧Tab, changes the kind and watches it again from the top; the header says Library, its scope and how many are shared; the search line's end says what's shown ("finding…", "newest first", "214 of 1,204").

**Tests:** the labels from counts, capped and not; Tab's order, and ⇧Tab's; a kind chosen is watched with that kind; the header's and the count's words; references in both looks; the scene with each kind chosen.

**Done when** the suites pass and the scene is shot.

### Task 5: The filters as tokens

**Files:** `LibrarySearch.swift` (create), `LibraryController.swift`; `LibraryScenes.swift`; tests `LibrarySearchTests.swift` (create) with references `library-search-*`.

**What:** the search line's words and tokens, as the terminal's: `from:`, `in:`, `since:` and `until:` typed offer what they could become (question 10's people; the chat list's chats; today, yesterday, a week, a month and a year ago, the last twelve months, or a date typed as 2026-08-01), best first, six at most; Tab or Enter takes one as a token, ⌫ in an empty field takes the last back, a token's × takes it away, Esc puts the offers away until the words change; the words are captions, file names and link titles, asked again only when they change, a filter still being typed left out of them; what's asked is the kind, the scope's chat or `in:`'s, the person, `since`, `until` (question 11) and the words.

**Tests:** `from:a` offers Ana and you; Tab takes it and the filter names `person:4`; a name of several words; `in:` offers chats; `since:` and `until:` offer the terminal's dates and a typed one, `until:` reaching the day's end; ⌫ takes the last token back; the words leave out the filter being typed; Esc puts the offers away until the text changes; references in both looks with tokens and offers; the Library search scene.

**Done when** the suites pass and the scene is shot.

### Task 6: The library in a pane

**Files:** `LibraryPane.swift` (create), `LibraryController.swift`, `ConversationController.swift`; `AppModel.swift`, `MainWindowController.swift`, `Commands.swift`, `ChatMenus.swift`; `LibraryScenes.swift`; tests `LibraryPaneTests.swift` (create).

**What:** ⌘L opens every chat's library, and a chat's header's Library button, the Chat menu's and the pane menu's This Chat's Library (⌥⌘L) and the palette open that chat's, where question 1 says; the pane is a slot of the layout as question 2 has it, its window watched while it shows and let go when it closes, its pictures let go while the window is closed; Esc or ⌘W closes it, the keys going to the pane beside it; ⌘L's placeholder ("comes with jawd's M3") goes.

**Tests:** where a library opens: beside the focused pane, in an empty one, in the focused pane's place when there's no room, and to the pane already showing it; the pane IDs and their scopes, and the editor's names for them; a library slot is never looked up as a chat; Esc's and ⌘W's pane afterwards; the conversation's header has the button with its key; the app's snapshot on `--demo` with `cmd+l` shows the library beside Ana's chat.

**Done when** the suites pass, `build-mac build jaw` builds, and the snapshot shows the library in its pane on `--demo`.

### Task 7: Enter to open, and ⌘↩ to the message

**Files:** `LibraryController.swift`, `Files/MediaOffer.swift`, `Files/PendingOpen.swift`; `AppModel.swift`; tests `LibraryPaneTests.swift`, `MediaOfferTests.swift`.

**What:** Enter, or a double-click, opens the selected item as question 5 says, and ⌘↩ goes to its message as question 6 says; an item's menu has Open, Go to Message, Save to Downloads and Show in Finder for a file's, and Copy Link for a link.

**Tests:** an item's offer: a photo's, a video's and a file's, here or not, named as `media.save` names them, an audio file named with its extension; what each kind's Enter does; a pending look opens once the item's row says it's here and stops once it says it can't come; ⌘↩'s pane, by question 6; the snapshot's keys on `--demo`: `cmd+l`, then Enter looks at a photo with Quick Look and ⌘↩ goes to its message.

**Done when** the suites pass and the snapshot's keys do both on `--demo`.

### Task 8: `library.shown`, as the terminal sends it

**Files:** `Client/Jawd.swift`, `LibraryController.swift`, `AppModel.swift`; tests `LibraryTests.swift`, `LibraryLayoutTests.swift`.

**What:** the items on screen, by `id`, in their order, 200 at most, named to jawd as the terminal names them: again whenever they change, as the grid scrolls (at most once in 150 ms, the last always sent), as the window's items change and as the kind or the filters do, and never again while they're the same; nothing named while the window is closed.

**Tests:** `library.shown.request.json` equals what JawKit writes for its items; the items named are those whose frames are on screen, in order, 200 at most; a scroll that leaves them the same names nothing; the window closed names nothing; live on `jawd --engine demo`, `library.shown` is answered.

**Done when** the suites pass.

### Task 9: Parity and the README

**Files:** `mac/parity.toml`, `mac/README.md`, the parity plan's step 2.4, the Mac design's Protocol additions.

**What:** the Mac sends `library.watch` and `library.shown` and reads the `library` push, so their rows come out of the list; `make check-parity` passes. The README's library paragraph; the parity plan's 2.4 says what's built and what's left; P14 in the design.

**Done when** `make check-parity` and `make mac-test` pass, `build-mac build jaw` builds, and every new scene is swept.

## P14, the library's items whole

A proposal for jawd's builders, in [the Mac design](2026-10-26-jaw-mac-design.md)'s "Protocol additions". A library row carries less of its message than a message row does, so the design's library can't be drawn whole: a voice message's `voice` has its `seconds` but not its `waveform` or `played`; a file's `file` has its `path` once it's here but not the copy's `state`, `progress` and `error`, so a file not here can't say it's coming, expired, gone or waiting for the phone, nor offer what 2.3's card offers; and a link's `link` has no `preview`, the thumbnail its message's row has. The rows would carry them as message rows have them, read in the window's one read transaction as the rest of the row is. It changes `crates/jawd/src/feed/library.rs` and `store/library.rs`, so it follows "Data integrity" in jaw's design and is audited by the database expert before it lands. Until then the Mac draws a voice message's bars evenly, a file not here with ↓ and its size, and a link's well with its symbol.

## As built

What changed from the tasks above as they were built:

- **The rows came with the grid.** Task 2's layout and drawing hold the rows and the shelves; task 3 is their tests, references and scenes.
- **The references** are `library-grid-*` (task 2, photos and videos), `library-rows-*` and `library-all-*` (task 3), `library-strip-{dark,light}` (task 4, the strip being the same in either look) and `library-search-*` (task 5, whole panes with tokens and an offer), each set in bubbles and lines, dark and light, but the strip's. The harness's scenes are Library grid, Library rows, Library, every kind, Library kinds and Library search, under Panes, and A chat beside its library, under Screens.
- **The pane** (task 6): while a library has the keys, the box under the panes says them, as the terminal's footer does; the chat header's Library button shows while the pointer is over the header, as the pane's close button does, and the header keeps room for both.
- **The keys** (task 7): a panel that took the keys, as Quick Look's does, leaves the window with none of its own, so a library pane with the focus takes them back as the window becomes key; ⌘↩ is the pane's key equivalent, and the field's when the app isn't active; the snapshot's keys offer a key with ⌘ to the window's views first, as AppKit offers it to a key window's, since the build Mac runs the app inactive.
- **Parity** (tasks 1 and 8): each row came out of `mac/parity.toml` with the task that closed it, so `make check-parity` passes at every commit; task 9 is the README, the parity plan's step and P14.
- **The rebase:** `mac-media` landed during task 7 (07ce129a to 7b1ad56e), and the branch was rebased onto it with `git rebase --onto origin/main 60014a35`, without a conflict; jawd was built again from main's code for the suites after.

## After the plan

The branch is rebased onto main once `mac-media` is there, every suite run again on the build Mac with a jawd built from main's code, and reviewed; then it lands from this worktree, the build Mac's copy (`~/src/build-mac/jaw/jaw-mac-library`) deleted after.
