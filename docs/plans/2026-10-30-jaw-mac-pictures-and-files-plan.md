# Jaw for the Mac: pictures, and opening and saving files, implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** [The parity plan](2026-10-27-jaw-mac-parity-plan.md)'s steps 2.2 and 2.3 in the Mac app, as [the Mac plan](2026-10-26-jaw-mac-plan.md)'s Mac-M2 steps 1, 2, 4 and 5 have them for the kinds those steps name and [the Mac design](2026-10-26-jaw-mac-design.md) draws them: photos, albums, stickers and the posters of GIFs, videos and video messages drawn in their places from jawd's previews, through an image queue of four at a time that drops what's scrolled past, a cache of decoded pictures of 64 MB at most, the preview the pixels drawn need, and the blurred thumbnail fading into it in place; every download state drawn as the protocol says it, with what can be done about it; and a photo, a video or a file looked at with Quick Look, opened in its app, saved, and dragged out to the Finder. Built on media in (`m3-media-in` at 42a31eaf), before it lands, and landing after it.

**Not in this step:** GIFs looping while on screen, videos and video messages playing in place, and a document's first page, which the Mac plan's Mac-M2 steps 2 and 4 name. This step draws GIFs, videos and video messages as their posters, as the terminal does, ▶ looking at them with Quick Look, and a document's card with its icon; motion and the first page are left for [the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s Phase 4, with the rest of Mac-M2, as its step 2.2 says.

**Architecture:** JawKit gains the media row's fields (`thumbnail`, `path`, `hd_path`, `state`, `progress`, `error`, `hd`), an album item's `media`, and `media.fetch`, `media.save` and `media.cancel`, checked against media in's fixtures and a real jawd's photo. JawUI does everything drawn and decided, from values: the layout lays each picture out as a slot (where it is, how it's clipped, which previews and thumbnail it has, the pixels it's drawn at), with the state's chrome drawn over it; `Pictures` picks a slot's preview, decodes it with ImageIO at the pixels drawn on a queue of four, keeps the result in a cache of 64 MB drawn least recently first out, blurs the thumbnail meanwhile, and drops what's asked for once its cell leaves the screen; `MessageView` draws the ready bitmaps and fades a preview in on Core Animation; `MediaOffer` says what each state offers and in which words; `Opened` makes the copy a file is looked at, opened and dragged from. The app asks jawd, keeps the Quick Look panel, and measures a fling through a chat of jawd demo's pictures.

**Tech Stack:** Swift 6 (language mode 6, warnings as errors), AppKit (`NSTableView`'s cells drawing ready bitmaps, `QLPreviewPanel`, `NSFilePromiseProvider`), ImageIO and Accelerate (vImage) off the main thread, Swift Testing, Tuist through mise; builds and tests on the build Mac through `mac/scripts/build-mac`.

## Questions

What the designs leave open, each with its options and the lean the plan builds; none blocks a task, and each is a small change in the one place named.

1. **What's looked at and opened: jawd's file, or a copy.** jawd keeps a file once for every message that has it (`media/files/<sha256>.<ext>`, mode 0600, the owner's to write), and an app that edits what it opens, Preview among them, saves into it: a rotated photo would change the file every message with it shows, under a hash it no longer has. Quick Look's "Open with Preview" opens what it shows, so the same goes for a look. A: a clone of jawd's file in the app's own temporary folder, named as `media.save` names a file (the message's name, or its ID and the file's extension), made with `clonefile`, which costs no space and no time on APFS and leaves jawd's file as it was whatever is done to the clone; a copy where a clone can't be made (another volume), off the main thread; the folder emptied at launch (lean; amended after the review: each clone quarantined as a message's attachment and, once looked at or opened, read-only, in a folder for each run, emptied as it quits, the folders of runs that have gone emptied as one starts); B: jawd's own file, as the terminal's `open` would have it; C: `media.save` into the app's folder each time, a whole copy jawd writes and syncs. Task 5 builds A, in `Opened`.
2. **Dragging out.** The design has a file dragged "by the file once it's here, by `media.save` into the drop's folder before". A: once it's here, the clone of question 1, as a file URL any app takes, under its name; before, a file promise the Finder and Mail take, kept by `media.save` into the folder dropped on, so jawd downloads it, names it and never writes over a file there (lean; amended after the review: kept in the app, `media.save` writing into the app's own folder and the app moving the file to the promised place, under the promised name or `name (1).ext` when that's taken, quarantined, so what's written there is the app's, and macOS asks for the drop's folder under the app's name, not jawd's); B: a promise always, kept by `media.save`. Task 5 builds A, in `Dragged`.
3. **A click on a picture or file that isn't here.** A: asks jawd for it (`media.fetch`), the ring shows, and once it's here it's looked at as a click on it would, unless something else has been asked to open since or it couldn't be had, which its card then says (lean, as WhatsApp opens what you tapped once it comes); B: asks for it, and a second click looks once it's here. A click on the ring stops it (`media.cancel`). Task 5 builds A, in `AppModel.pending`.
4. **Albums from jawd today.** The protocol's album row has `items`, each with its `media`; media in's jawd sends an album as `{"photos": 3, "videos": 1}` and each photo or video as a message of its own after it (`convert/kinds.rs`'s `album`). A: an album with items is drawn as the design has it, and one with counts alone as a line of them, "Album · 3 photos and 1 video", as the Mac writes a kind's line (jawd's summary is "Album: 3 photos and 1 video"), its photos drawn below as jawd sends them, with a proposal (P12, below) that jawd gather them into the album's row (lean); B: the app gathers the photos after an album row itself, guessing which are its. Task 3 builds A.
5. **The sizes.** The frames on main keep each picture's shape inside 280 by 320 points, stickers at 140 and video messages at 160; the design has photos inside 320 by 400 (120 at least), stickers at 160 with no bubble, video messages a 192-point circle and albums 320 wide, 2 points apart. A: the design's (lean); B: main's. Task 3 builds A, in `Cards.swift`; the place is still known from `w` and `h` before any pixel, so nothing moves when a preview lands.
6. **The HD copy.** A: an "HD" mark on a photo with one; Quick Look shows `hd_path` when that's here; Save saves the copy shown, and Save HD Copy, in the menu, the HD one, which jawd downloads first (lean); B: Save always saves the HD copy. Task 5 builds A.
7. **The fade under Reduce Motion.** The design's animation table: "the same fade, shorter". A: 120 ms, and 60 ms under Reduce Motion (lean); B: none under Reduce Motion. Task 3 builds A, in `MessageView`.

## Global Constraints

- macOS 26 and later, Swift 6 language mode, warnings as errors (`mac/Project.swift`).
- No font, radius or line width set by hand outside `JawUI/Sources/Components` and the layout's own: `mac/scripts/check-components` fails the build on one.
- Everything drawn lives in JawUI and is made from values; the app target defines no view or view controller of its own; `AppModel` is the only code that talks to jawd; the harness names neither `Jawd` nor `Client` (`scripts/check-harness`). Every new component has a harness scene whose `covers` name it.
- Off the main thread: decoding, scaling and blurring pictures, and cloning a file. The main thread draws bitmaps that are ready, at the size they're drawn, and never waits for a picture (the design's principles 1 and 2).
- Nothing jumps: a picture's place comes from the row (`w`, `h`, the album's count, the kind's fixed size), never from its pixels.
- jawd's files are read, never written: nothing the app does, or lets another app do, changes a file under jaw's folder (question 1). The Mac never touches jawd's data layer; what it needs of jawd is a proposal.
- The words are the design's and the protocol's: "Ask the phone again", "Waiting for your phone", jawd's `error` as it says it; elsewhere the terminal's, in the Mac's lower-case status line: "saved to Downloads as …", "downloading …".
- Fixtures: media in's are on its branch (`protocol/fixtures/media.*.json`), so they're read where they are; a row in each state that no fixture shows is written from the protocol's text, as the folders plan wrote a chat row's `folders`.
- Tests run on the build Mac (`mac/scripts/build-mac test …`, `build-mac build jaw`), checked to answer first (`ssh -o BatchMode=yes -o ConnectTimeout=5 build-mac true`); jawd is built here (`CARGO_INCREMENTAL=0 cargo build -p jawd --bin jawd`).
- Never open `~/Library/Application Support/jaw`; the app runs `--demo` or `--home` on a folder of its own with a jawd demo it started; never stop a process by name.
- Commits are on `mac-media`, one a task, in the repository's style: plain sentences saying what changed and why, no prefix, British spelling. Scratch goes in `/tmp/jaw-mac-media/`, deleted before the step reports.

## Where it's built, and what it waits on

- **The branch.** `mac-media`, in `~/src/jaw-mac-media`, made from media in's head 42a31eaf (main at abfa15a with folders and the keepalive fix, then media in's commits). Once media in is on `origin/main` (its note commit, "The database expert's audit of M3's media in"), it's rebased with `git rebase --onto origin/main 42a31eaf mac-media`, and again as main moves, keeping both sides: main has since moved the window's parts into JawUI (`WindowContent`), opened the terminal's panes and taken the harness's pictures once conversations have drawn, which touch `ConversationController`, `Jawd`, `Fixtures` and `Specimen`. Media in landed during task 3 (fb1f98ff, then its landing's audit at 9b3fbdf4), and the branch was rebased onto 9b3fbdf4 then, keeping both sides: `layout.get`'s reply beside `media.save`'s, main's harness scenes beside Pictures, and the scenes waiting for their conversations to draw as main's do.
- **jawd.** Media in serves everything here: `media` rows with `previews`, `thumbnail`, `state`, `progress` and `error`, `media.fetch`, `media.save` and `media.cancel`. `jawd --engine demo` has one photo, the lake in Ana's chat (`3EB0DEMO00000008`), downloaded and previewed by jawd as a real photo is; `jawd demo`'s million messages are text. Nothing here waits on jawd but P12 for albums gathered into one row.
- **Landing.** After media in, once a fresh reviewer has read it, from this worktree; the last task takes the rows it closes out of `mac/parity.toml`.

## Files

- `mac/JawKit/Sources/Protocol/Rows.swift` (modify, task 1): `Media`'s new fields, `Album.Item.media`, `Album`'s counts.
- `mac/JawKit/Sources/Protocol/Media.swift` (create, task 1): `MediaState`, `Variant`, `MediaFetch`, `MediaSave`, `MediaCancel`, `Saved`.
- `mac/JawKit/Sources/Client/Jawd.swift`, `Incoming.swift` (modify, task 1): `fetch`, `save`, `cancel`, `JawUpdate.saved`.
- `mac/JawUI/Sources/Pictures/` (create, task 2): `PictureSource.swift` (a slot's sources and the preview its pixels pick), `Decode.swift` (ImageIO at the pixels drawn, filled or fitted, and the blur), `PictureCache.swift` (64 MB, least recently drawn first out), `PictureQueue.swift` (four at a time, cancelled when scrolled past, or at once for pictures that must come out whole).
- `mac/JawUI/Sources/Layout/Cards.swift`, `MessageLayout.swift` (modify, tasks 3 and 4): the design's sizes, `PictureSlot`s and the chrome over them in `Laid`, the album's tiles, a sticker without a bubble, each state's chrome and regions.
- `mac/JawUI/Sources/Conversation/MessageView.swift`, `ConversationController.swift` (modify, tasks 3 to 5): drawing the slots, asking for them and letting them go, the fade; the keys, the menu's items and the drag.
- `mac/JawUI/Sources/Files/` (create, task 5): `MediaOffer.swift` (what a message's file offers in each state, in words), `Opened.swift` (the clone), `Dragged.swift` (the drag's pasteboard writers), `QuickLook.swift` (the panel's items).
- `mac/JawApp/Sources/AppModel.swift`, `MainWindowController.swift`, `Commands.swift` (modify, tasks 5 and 6): the host's media acts asked of jawd, the pictures' root, saved answers, pending opens, the Quick Look panel, the Message menu's items.
- `mac/JawApp/Sources/Bench.swift` (modify, task 6): `--bench pictures`.
- `mac/JawHarness/Sources/Scenes/PictureScenes.swift` (create, tasks 3 and 4), `Fixtures/HarnessPictures.swift` (create, task 3), `BuiltInScenes.swift` (modify): the pictures and the download states, whose documents are the files.
- `mac/README.md` (modify, tasks 5 and 6), `mac/parity.toml` (modify, task 7).
- Tests: `mac/Tests/JawKitTests/MediaTests.swift` (create), `FixtureTests.swift`, `LiveTests.swift` (modify); `mac/Tests/JawUITests/PictureTests.swift`, `PictureLayoutTests.swift`, `MediaOfferTests.swift` (create) and their references; `mac/Tests/JawHarnessTests/ScenesTests.swift` (modify).

## The budgets

From the README's table and the Mac plan's Mac-M2, measured by task 6 on the build Mac, in both looks, with the load beside each:

| What | Budget |
| --- | --- |
| Scrolling with photos | no frame missed at 120 Hz: main-thread turns under 8.3 ms p99, hitches under 1 ms a second |
| Memory | under 150 MB with a screen of photos; decoded pictures at most 64 MB |
| Opening a chat | 30 ms p99, as before: no picture is decoded on the way |
| Idle | nothing woken by pictures: no timer, no display link, the fade on Core Animation |

---

### Task 1: JawKit's media rows, requests and answers

**Files:** `Protocol/Rows.swift`, `Protocol/Media.swift` (create), `Client/Jawd.swift`, `Client/Incoming.swift`; tests `JawKitTests/MediaTests.swift` (create), `FixtureTests.swift`, `ClientTests.swift`' `FakeJawd`, `LiveTests.swift`.

**Interfaces:** `MediaState` (`remote`, `downloading`, `local`, `expired`, `waitingForPhone`, `gone`; one jawd adds later reads as `remote` with its `error`, as nothing's known to be here); `Media` gains `hd: Bool`, `thumbnail`, `path`, `hdPath: String?`, `state: MediaState`, `progress: Double?`, `error: String?`, with `isHere` and `preview(_:)`; `Album.Item` gains `media: Media?`, `Album` `photos` and `videos` (jawd's counts today) and `count`; `MediaFetch(message:chat:variant:)`, `MediaSave(message:to:chat:variant:)`, `MediaCancel(message:chat:)`, `Saved(path:)`; `Jawd.fetch(chat:message:variant:)`, `Jawd.save(chat:message:to:variant:)` answered by `JawUpdate.saved(message:, path:, why:)`, `Jawd.cancel(chat:message:)`.

**Tests:**
- media in's six fixtures: each request JawKit writes equals its fixture, and `media.save`'s answer reads as `Saved`; every result and push fixture still decodes.
- a media row in each state, written from the protocol's text: the photo of the protocol's example (`downloading`, `progress` 0.4, `thumbnail`, `hd`), a local one with both previews and `path`, `expired` and `gone` with `error`, `waiting_for_phone`, a `remote` one with an `error` (a download that failed, as jawd's `LATER` leaves it), and a state jawd may add later; an album with items and their media, and jawd's album of counts.
- `FakeJawd`: `fetch` and `cancel` ask once with the chat; `save` is answered with its path, or refused in jawd's words (error 4's "jawd couldn't download the file: …"), each said as the message it was for.
- the live suite on `jawd --engine demo`: the lake in Ana's chat comes `local`, with `path`, both previews and `thumbnail`, each a file under jawd's `root`; `media.fetch` is answered; `media.save` into a folder of the test's own answers with a path whose bytes are the cache's; `media.cancel` is answered.

**Done when** the four suites pass on the build Mac, with the new tests among them.

### Task 2: The pictures: the preview the pixels need, the queue and the cache

**Files:** `JawUI/Sources/Pictures/` (create); tests `JawUITests/PictureTests.swift` (create).

**Interfaces:** `PictureSource` (`previews`, `thumbnail`, and `pick(pixels:)` giving the paths to try, best first: the 256-pixel preview for 256 pixels or fewer, the 768 above, the other, then nothing; the thumbnail apart); `PictureKey` (path, pixel size, fill or fit, blurred); `PictureCache` (64 MB by bytes, least recently drawn first out, `bytes`, `count`, `drawn(_:)`); `PictureQueue` (four decodes at a time; `ask(_:for:)` with a token, `letGo(_:)` dropping what a token asked that hasn't started; `.here` decoding on the asking thread, for pictures that must come out whole); `Pictures` (the queue, the cache and the files: jawd's `root`, or bytes in memory for the harness and tests).

**Tests:** the 256 preview for an album tile at 1x and the 768 for a photo at 2x, and the other or nothing when one is missing; a decode is at the pixels drawn, filled from the centre or fitted, and never larger than the file; at most four decode at once with forty asked; what's let go before it starts is never decoded, and what's let go while decoding is cached but not handed over; the cache stays under its bytes, drops the least recently drawn first, and keeps what's drawn; a missing or broken file is tried once and its slot falls back; the blur is deterministic and as large as asked.

**Done when** JawUITests passes, with the new tests, and a 768-pixel preview's decode is off the main thread in every path the app takes.

### Task 3: Photos, albums, stickers and posters in their places

**Files:** `Layout/Cards.swift`, `Layout/MessageLayout.swift`, `Conversation/MessageView.swift`, `Conversation/ConversationController.swift`; `JawHarness/Sources/Fixtures/HarnessPictures.swift` and `Scenes/PictureScenes.swift` (create), `BuiltInScenes.swift`; tests `JawUITests/PictureLayoutTests.swift` (create) with references `pictures-{bubbles,lines}-{dark,light}.png`, `ScenesTests.swift`.

**What:** a photo in its shape inside 320 by 400 points, 120 at least; albums of 2 (side by side), 3 (two across, one wide below), 4 (two by two) and 5 or more (two by two, "+N" on the fourth), filled and cropped from the centre, 2 points apart, 320 wide; jawd's album of counts as its line; a sticker at 160 points, no bubble, from its transparent preview; a video's and a GIF's poster with ▶, the length and the size, a GIF marked GIF; a video message a 192-point circle; an "HD" mark. Each is a slot in `Laid`, drawn by `MessageView` from the cache, asked of the queue when it isn't there, let go when the cell is reused or leaves the table; the blurred thumbnail until then, fading into the preview in 120 ms (60 under Reduce Motion) as a Core Animation transition, and no fade for a preview already decoded. In both looks: inside the bubble, or under the text at its indent.

**Tests:** each kind's slot is where its size says before any pixel, and a preview arriving changes no height; albums of 2 to 8 lay out as the design draws them; the slots' pixel sizes pick the previews task 2 says; a sticker has no bubble and keeps its shape; a cell asks for its slots once and lets them go when reused; the references in both looks, dark and light, with pictures drawn for the purpose (shapes and colours, plainly not photos); the harness's Pictures scene draws, the same picture twice.

**Done when** the suites pass, the harness's Pictures scene is swept in both looks, and the app on `--demo` draws the lake in Ana's chat from jawd's 768 preview.

### Task 4: Every download state

**Files:** `Layout/Cards.swift`, `Layout/MessageLayout.swift`; `Scenes/PictureScenes.swift`; tests `PictureLayoutTests.swift` and references `download-states-{bubbles,lines}-{dark,light}.png`.

**What:** over a picture, and on a document's card: `remote`, the size to fetch with ↓; `remote` with `error`, jawd's words and "Try again"; `downloading`, a ring filled to `progress` with ×; `local`, nothing over a picture and the file's details on a document; `waiting_for_phone`, "Waiting for your phone"; `expired` and `gone`, jawd's `error` and "Ask the phone again". Each is a region a click acts on (`Region.media`): fetch, cancel, ask again, look.

**Tests:** each state's chrome and region for a photo, a video, a sticker, an album's tile and a document; a progress push lays out again (its `v` is new) and the ring follows it; references in both looks; the harness's Download states scene.

**Done when** the suites pass and the scene is swept.

### Task 5: Looking, opening, saving and dragging out

**Files:** `JawUI/Sources/Files/` (create); `ConversationController.swift`, `MessageView.swift`; `JawApp/Sources/AppModel.swift`, `MainWindowController.swift`, `Commands.swift`; tests `JawUITests/MediaOfferTests.swift` (create); `mac/README.md`.

**What:** Space, Enter, a click or a double-click looks at a selected photo, video or document with Quick Look; ⌘↩ opens it in its app; s saves it into Downloads with `media.save`, said as "saved to Downloads as …"; the message's menu has Quick Look, Open, Open With ▸, Save, Save HD Copy, Show in Finder, and the state's Download, Stop Downloading, Try Again or Ask the Phone Again; the Message menu has the same with their keys; a file drags out (question 2). What isn't here is asked for and opened once it is (question 3). Quick Look and the apps see the clone (question 1).

**Tests:** what each state and kind offers, and in which words (`MediaOffer`), as the menu and the keys read it; a clone's name is the message's name or its ID with the extension, unique in its folder, and a change to it leaves jawd's file as it was; the drag's item is the clone's URL once here and a promise before; the keys reach the right act; a refused save says jawd's words.

**Done when** the suites pass, the app's snapshot on `--demo` with `menu:message:<row>` lists the items for the lake, and on the demo a look, an open, a save and a drag of the lake each work by hand.

### Task 6: A fling through a chat of pictures, measured

**Files:** `JawApp/Sources/Bench.swift`; `mac/README.md`.

**What:** `--bench pictures` runs on `--demo`, waits for jawd to have the lake's previews, makes a chat of 240 photos, albums, stickers and posters whose previews are copies of jawd demo's (each a file of its own, so each is decoded), and flings through it at 120 Hz down and up in each look, timing every main-thread turn, as Redlamp's `MainThreadMonitor` does, the decoded cache's bytes, and the footprint with a screen of photos; it removes its copies before it quits.

**Done when** the README's table has the rows, each within its budget or saying why.

### Task 7: Parity

**Files:** `mac/parity.toml`.

**What:** the Mac sends `media.cancel` now (task 5), which the list holds for step 2.5; it comes out. `media.fetch` and `media.save` the terminal doesn't send, so they're the Mac's alone, as `history.more` is. `make check-parity` passes.

## P12, an album in one row

A proposal for jawd's builders, for question 4, in [the Mac design](2026-10-26-jaw-mac-design.md)'s "Protocol additions", where they look: the album's row carries its `items` (each an `id`, a `kind` and its `media`, in the order sent), as the protocol has it, gathered from the messages WhatsApp associates with it (`MessageAssociation` of type `MEDIA_ALBUM`), and those messages don't come as rows of their own. Filled out after the review: each item stays its own message, its own ID taken by `media.*`; the expected counts stay beside `items`, so the album doesn't grow as they land; items come before their album, after it, or never; what becomes of the items' messages' reactions, quotes, edits and deletes, stars, receipts and `message.info`, read positions and anchors, `find`'s and the library's hits, unread counts and notifications, and the windows' totals and paging; and a backfill for the albums already in jaw.db. It changes `convert/`, `messages` and `store/read.rs`, so it follows "Data integrity" in jaw's design and is audited by the database expert before it lands. It was first written here as P11, which `mac-links-polls`' `link.preview` keeps, in the design, the protocol's Composing and the parity plan's step 3.3.

## After the plan

The branch is rebased onto main once media in is there, every suite run again on the build Mac, and reviewed; then it lands from this worktree, the build Mac's copy (`~/src/build-mac/jaw/jaw-mac-media`) and `target` deleted after.
