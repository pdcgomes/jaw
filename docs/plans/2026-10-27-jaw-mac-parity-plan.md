# Jaw for the Mac: parity with the terminal

27 October 2026. The plan that brings the Mac app level with the terminal: first every gap where the terminal already has something the Mac lacks, then what the terminal and jawd are building now, as each lands, with the terminal tracked so that nothing new slips past (Owner, 27 Oct). It reorders what's left of [the Mac plan](2026-10-26-jaw-mac-plan.md) around those gaps rather than replacing it, and every step does things the Mac's way, as [the Mac design](2026-10-26-jaw-mac-design.md) and its later designs have them. The owner left the choices to this plan ("just dive in", 27 Oct), so the decisions below are its own; the questions at the end are the ones that are his.

## Where it starts

- **The Mac app is on main** since 27 October, as `mac-m1` landed, its typing bubble with it. It does everyday text end to end, the whole palette, panes on a grid with named layouts, focus mode, the status line, the glance and linking, and draws a card for every kind of message.
- **The terminal is further along in 26 places** of the 149 features the parity canvas compares (`jaw-feature-parity.canvas.tsx` in the project's canvases), and the Mac in 14. 19 of the terminal's 26 are screens built against its made-up account that wait on jawd: pictures, opening and saving files, voice messages, the library, and folders with the roster and the new folder keys.
- **By the requests each sends,** the terminal asks jawd for 19 things the Mac never does: `layout.get` and `layout.set`; `folders.watch`, `roster.watch` and the six `folder.*` requests; `library.watch`; and the eight `voice.*` requests; and it reads 5 pushes the Mac doesn't: `layout`, `folders`, `roster`, `playback` and `recording`. The Mac sends two the terminal doesn't: `history.more` and `link.cancel`.

## How the terminal is tracked

- **`make check-parity`** (`scripts/check-parity.py`) reads which of jawd's requests each interface sends and which of its pushes it reads, from their source (the terminal's in `crates/jaw` and `crates/jaw-client`, the Mac's in `mac/`), and which requests jawd serves. Every one the terminal uses and the Mac doesn't is listed in [`mac/parity.toml`](../../mac/parity.toml) with the step below that brings it. The check fails on a gap the list doesn't have, and on one the Mac has closed that it still holds, so the list always says what's left and why.
- **Ahead of main:** `make check-parity ARGS="--terminal <branch>"` reads a terminal branch against the Mac on main, so what's being built is queued before it lands. Rows from a branch whose names main's protocol doesn't have yet say which branch (`from`); `library.shown` and `settings.set` are queued that way now.
- **What a request can't show:** a feature that needs no new request, such as the terminal's folder keys or the IRC layout. Those come from each terminal branch's commits and the parity canvas, which is brought up to date whenever main moves; the canvas is the whole picture, and the check is the guard.
- **When it runs:** whenever main moves, with each terminal or jawd branch that lands, and at the start and end of every Mac step. Something new from the terminal becomes a row in the list and a line in its step below, in the same commit.
- **The board** (`ws-mac-parity.canvas.tsx`) shows each step's state and what it waits on.

## The order

1. **The terminal says what, the Mac design says how.** A gap is closed when the Mac does what the terminal does, in the Mac's way: its keys, its menus, its panes and its looks. Where the Mac design draws something differently, the design wins; where it says nothing, the terminal's behaviour and words are the reference.
2. **Build ahead of jawd, against its branch.** The terminal built its screens against its made-up account, and the menus design builds folders against the `folders` branch's fixtures. Each Mac step starts as soon as jawd's branch has its protocol text and fixtures, and tests against them and `FakeJawd`. It lands on main once its tests pass, even before jawd's part does, saying what it waits on when chosen, as the terminal's do ("comes with M3"). It's tried end to end on `jawd demo` and by the owner once jawd's part is on main.
3. **What jawd serves already, first.** It works on the owner's account the day it lands.
4. **Then the terminal's screens, in the order jawd's parts land,** so each is tried end to end as soon as it can be: folders are through their second audit, media in is in its third, the library and media out are stacked on media in, and voice hasn't started in jawd.
5. **Then what's being built now, as each lands,** each step written when its branch's protocol is.
6. **The Mac's own work waits for the gaps,** except what's in flight: the right-click menus (`mac-menus`), which carry folders, and the harness's first phase (`mac-harness`), which every step's screens are checked in. The viewer, Redlamp's themes, the animations, notifications with Reply, the Dock badge, accessibility and a notarized build come after (Phase 4).
7. **Nothing the Mac leads on goes backwards.** Its 14 leads, the grid layouts, the emoji picker, the cards and the rest, stay as they are; they belong to a plan for the terminal, not this one.

## Phase 1 · What jawd serves already

### 1.1 The panes kept by jawd

The terminal's panes come back after a restart because jawd keeps them (P1, on main since 3d60818). The Mac keeps its layouts in its preferences. jawd's layout is one tree of splits (`format` 1, 16 panes, 16 KiB), and the Mac's are named grids: up to 12 slots on a grid's lines, a pinwheel that no tree can hold, and a shape that can be edited beside the saved one. So the Mac can open a tree, as the grid layouts design reads one, but can't keep its own layouts in format 1.

- **Now:** Open the Terminal's Panes, in the Layouts menu and the palette: `layout.get` with `interface` `jaw`, the tree read as a grid into the unsaved panes. This is the other interface's layout, which neither interface opens yet (Owner, 26 Oct, "each able to open the other's").
- **With P10** (below): the Mac keeps its whole book of layouts with `layout.set` once each change settles, reads it with `layout.get` at launch, and takes the `layout` push, which jawd sends when another of the app's connections keeps a newer one, as the terminal does. Its preferences stay as they are until P10 lands, and are read once into jawd after it does.
- **Done when** a relaunch opens the layouts the app was left with, kept by jawd, and the terminal's panes open in the Mac.

### 1.2 Starting jawd when nothing answers

The terminal starts jawd when nothing answers on `jaw.sock`, says so, and keeps drawing. A Mac release build says how to start jawd instead, by the Mac plan's rule for the owner's Mac, and only `--demo` starts one of its own.

- A release build starts the jawd inside it (`Contents/Helpers/jawd`) as a process of its own that outlives the app, as the terminal does, when nothing answers and no agent is installed. With jawd at login registered, it kicks the agent instead. `jaw.lock` keeps it to one jawd either way, and the status line says "Starting jawd" meanwhile.
- Debug builds keep `--demo` and `JAW_HOME`. The release path is tried on a clean user account, never on the owner's folder by a build in progress.
- **Done when** quitting jawd and opening the app brings jawd back, in a release build on a clean account, with the window drawn the whole time.

### 1.3 The budgets measured

The terminal's every budget is measured by `make perf` into the README. The Mac's bench (`--bench launch|open|memory|idle`) has its first numbers in `mac/README.md`, with opening a chat over budget at p99 (54.8 ms against 30) and 14 idle wake-ups not yet accounted for.

- `make perf-mac`, which the Mac plan has every milestone close with: launch, opening a chat, keys in a flood, scrolling, applying a frame of updates, memory and idle wake-ups, against `jawd demo`'s million messages, in both looks, its table in the README beside the terminal's with the load average beside each number. The open's p99 and the idle wake-ups are looked into, on a quiet machine first.
- **Done when** the README has the Mac's table and every row over budget says why.

## Phase 2 · The terminal's screens, as jawd lands them

### 2.1 Folders, favourites and the roster

The terminal's sidebar has the tree with its counts, smart folders, folders made, renamed, nested, moved and deleted from the palette, chats and people filed with m, and the roster with presence, on the made-up account. On `folders` it runs on jawd's, with favourites on Alt-1 to Alt-9. Since 27 October, Alt-Shift-Up and Alt-Shift-Down go to the folder before or after from anywhere.

- **The Mac's** is [the menus and folders design](2026-10-27-jaw-mac-menus-and-folders-design.md)'s build-order steps 4 and 5, Mac-M4's step 4 brought forward: the tree in the sidebar, picking a folder, the folder items in the chat menus, a folder's menu, naming in place, dragging, favourites on ⌘1 to ⌘9, the palette's folder actions, and the roster under the tree.
- **Folder keys,** which the design doesn't have yet: ⌥⇧↑ and ⌥⇧↓ for the folder before and after, beside ⌥↑ and ⌥↓, which already mirror Alt-Up and Alt-Down.
- **Needs from jawd:** M6, on `folders`, through its second audit. Built against its fixtures (copied into JawKit's tests until they're on main) and `FakeJawd`, as the design says, after `mac-menus`' current tasks.
- **The check:** `folders.watch`, `roster.watch`, the six `folder.*` requests and `favourite.set`; the `folders` and `roster` pushes.

### 2.2 Pictures

The terminal draws photos, albums two across, stickers, and the posters of GIFs, videos and video messages from jawd's previews, with kitty, iTerm2, sixel or half-blocks, on the made-up account until media in lands.

- **The Mac's** are Mac-M2's steps 1 and 2: the image queue (four at a time, cancelled when scrolled past), the 64 MB cache of decoded pictures, the preview chosen by the pixels drawn, the blurred thumbnail fading into it; photos, albums, stickers (still, animated with P4 in Phase 4), GIFs looping while on screen, videos and video messages, their posters first and playing in place. The frames on main already keep each picture's shape, so the pictures drop into them and nothing moves.
- **Needs from jawd:** media in, on `m3-media-in`, in its third audit: the `media` rows' `previews`, `thumbnail`, `state` and `progress`.
- **The check:** nothing new; media rows come with `messages.watch`.

### 2.3 Opening and saving files

The terminal's Enter opens a photo, a video or a file in its app, and s saves it; both say they come with M3 until media in lands.

- **The Mac's:** Space or a click looks at a photo, a video or a document with Quick Look, ⌘↩ opens it in its app, Save puts it in Downloads (`media.save`), and it drags out to the Finder. Each download state (remote, downloading with its ring, expired, waiting for the phone, gone) says what it is and offers what can be done, asking again with `media.fetch`. Quick Look gives what the terminal's opening gives; the Mac design's own viewer comes after parity (Phase 4).
- **Needs from jawd:** media in (`media.fetch`, `media.save`, `media.cancel`).
- **The check:** nothing yet; when the terminal sends `media.save` or `media.fetch`, they're queued here.

### 2.4 The library

The terminal's Ctrl-L has every photo, video, voice message, file and link, across chats or in one: the strip of kinds, `from:`, `in:`, `since:` and `until:`, Enter to open, Ctrl-G to the message. It runs on the made-up account, and on jawd's `library.watch` on `library`.

- **The Mac's** is Mac-M2's step 7: in a pane, with ⌘L for every chat or from a chat's header for one; the kinds with their counts, the filters as tokens, the grid of previews, rows for voice, files and links, Enter to open and ⌘↩ to the message.
- **Needs from jawd:** the library, on `library`, in audit and stacked on media in; links come with `m2-content`.
- **The check:** `library.watch`, `library.shown` and the `library` push (both from `library`).

### 2.5 Attaching photos, videos and files

The terminal's Ctrl-O has a picker and `send.media`, SD or HD, albums, progress and cancelling, on `media-out`.

- **The Mac's** is Mac-M3's step 5, the box design's B2: pasting, dropping, ⌘O and ⇧⌘O, the strip of up to 30 above the field with captions, SD or HD (⌥ held as Send is pressed), `send.media`, progress from the `job` pushes, and cancelling. The camera (B3) comes after, and "no ffmpeg" is said here, as jawd's `status` reports it.
- **Needs from jawd:** media out, on `media-out`, built and landing after media in.
- **The check:** `send.media`, `media.cancel` and the `job` push.

### 2.6 Voice messages

The terminal draws the waveform; Enter plays and pauses, the speeds and seeking go by key, and the next unplayed message follows. All of it goes through jawd's `voice.play`, `voice.pause`, `voice.seek`, `voice.speed` and the `playback` push, on the made-up account until jawd serves voice.

- **The Mac's** is Mac-M2's step 3: the waveform's played part, play and pause, 1×, 1.5× and 2×, seeking by a click and by ← and →, the dot until it's played, the next unplayed after one ends, the playhead moving between jawd's pushes only while playing, and the small player in the status line.
- **Needs from jawd:** voice, M5. `jaw-voice` is built and audited; wiring it into jawd hasn't started. Built against the protocol and `FakeJawd`'s `playback` pushes, as the terminal was against its made-up account.
- **The check:** `voice.play`, `voice.pause`, `voice.seek`, `voice.speed` and the `playback` push.

### 2.7 Recording a voice message

The terminal's Ctrl-R records with a live level; Enter sends, and Esc cancels.

- **The Mac's** is Mac-M3's step 6, with the box design's microphone in Send's place when the box is empty: ⌘R, jawd's levels and time in the box, Enter to send, ⌘R to stop and listen, Esc to throw it away.
- **Needs from jawd:** voice (M5), and jawd's identity for the microphone: macOS asks about jawd by its signature, which the Mac design lists as a risk to check here.
- **The check:** `voice.record`, `voice.stop`, `voice.send`, `voice.cancel` and the `recording` push.

## Phase 3 · What's being built now, as it lands

### 3.1 Typing and presence in chats

The Mac draws someone typing as WhatsApp does (0de10c7): a bubble after the newest message, and the header's counting dots. For now it reads typing from the chat's row. The terminal's half and jawd's are being built on `presence-typing`, as P7's two halves: typing now, presence once folders are on main.

- When jawd sends the `typing` push, the Mac reads it, and each watch's answer saying how things are now, and the bubble follows it. With presence, the header and the status line's left half show online or last seen, faint where jawd isn't following someone.
- **The check:** the `typing` and `presence` pushes.

### 3.2 Chat actions and stars

Both interfaces already send `chat.pin`, `chat.archive`, `chat.mute` and `chat.mark_unread`; jawd's side is being built on `chat-actions` (P8). There's nothing for the Mac to build for the four: the Chat menu sends them, and so do `mac-menus`' right-click menus. Once jawd's step lands, each is tried end to end on `jawd demo` (three pins at most, archiving unpinning, muting for ever as -1, marking read sending receipts), and Star comes into a message's menu (`mac-menus` task 7). Starring isn't the terminal's yet; that's for its builders to decide.

### 3.3 Links and polls

The Mac's cards for link previews and polls already draw what jawd will send. Once `m2-content` lands, they're checked against real rows; voting by clicking an option (`poll.vote`) comes in this step, beside the terminal's number keys; and links come into the palette's answers and the library (2.4). When the terminal sends `poll.vote`, the check queues it here.

### 3.4 Failure states

The Mac draws every state already. Once `failure-states` lands (the engine restarting said, the phone offline reported, update needed, linking again after the phone logs jaw out), JawKit's live suite runs each through `jawd --engine demo --fail`, as `crates/jaw/tests/failures.rs` does for the terminal, so both interfaces are held to the same states.

### 3.5 Settings kept by jawd

On `settings`, the terminal's settings move into jawd, a domain for each frontend (`settings.set`, the `settings` push and `themes`), with Omarchy's 23 themes and the IRC layout. The Mac keeps its own preferences, as its design has them, and already has two looks. Whether its settings join jawd as a domain of their own, and whether it offers jawd's themes beside Redlamp's, are the owner's (Questions); until he says, this step is the Mac reading nothing from jawd's settings, which the list records.

- **The check:** `settings.set` and the `settings` push (both from `settings`).

### 3.6 Scheduled messages and reminders

[Their design](2026-10-27-jaw-scheduled-messages-and-reminders-design.md) builds the terminal's screens on jawd's branch and the Mac's on a branch of its own beside it (P9; the Mac plan's Mac-M3 steps 7 to 9). This plan starts the Mac's as soon as jawd's branch has the protocol's text and fixtures, so both land together: the first feature the two interfaces build at once, not one after the other. When the protocol has the `scheduled.*` and reminders' requests, the check queues them here.

### 3.7 Assistants and the catch-up

After the first release: Settings › Assistants with [M9](2026-10-27-jaw-mcp-design.md)'s second stage, beside the terminal's `jaw mcp grant` and its palette's Assistants; and the catch-up's panel with M11.

## Phase 4 · The Mac's own, after the gaps

The rest of Mac-M2 (the viewer, MapKit's maps, contact cards' Message and Add to Contacts, animated stickers with P4) and Mac-M5 (Redlamp's themes, the animations, the Mimic's chomp everywhere, notifications with Reply and Mark as Read with P2, the Dock badge with P3, Settings whole with P5, accessibility, a signed and notarized build with updates). Each starts once the gaps of its area are closed, so the viewer can follow 2.3 while voice is still waiting on jawd.

## P10, a layout book

A proposal for jawd's builders, for 1.1, added to the Mac design's proposals. `layout.set` and `layout.get` take `format` 2: a book of named layouts, each slots on a grid's lines (its columns and rows as fractions, each slot its lines and its chat), its edited shape beside its saved one, which is focused, and the unnamed layout, as the grid layouts design keeps them in `LayoutBook`. Format 1 is still taken and read, a tree being read as a grid. jawd keeps following merged and aliased JIDs in every slot of every layout, as it does in a tree's panes, and the limits are sized for a book: 12 slots a layout, the ten layouts ⌃⌘0 to ⌃⌘9 reach, and its bytes measured against a bound `status` reports. It changes `crates/jawd/src/layout.rs` and the store's `interface_state`, so it follows "Data integrity" in jaw's design and is audited by the database expert before it lands.

## Done when

- `make check-parity`'s list holds nothing but what's not for the Mac, each with why.
- The parity canvas shows no feature further along in the terminal.
- The owner spends a day on his own account doing in the Mac app everything he does in the terminal.

It stays done: each terminal feature that lands gets its Mac step that day, queued by the check or from its commits, and the board shows it until it's closed.

## Questions for the owner

1. **The Mac's settings in jawd.** Should the Mac's settings join jawd as a domain of their own, as the terminal's are moving to? This plan's lean: yes, with P5, so Media and Notifications live in jawd; the look stays the Mac's own.
2. **Themes.** Should the Mac offer Omarchy's 23 from jawd's `themes`, Redlamp's families as its design has them, or both? Lean: both, Omarchy's first since jawd serves them and nothing is copied, Redlamp's in Mac-M5.
3. **The check in CI.** Should `make check-parity` run in CI, so a terminal branch can't land a new request without its Mac row? Lean: yes, once CI runs again, but not in the pre-push hook, so the Mac never holds a terminal branch up.
4. **The other way.** Should the 14 places the Mac leads get a plan of their own for the terminal's builders? Lean: yes, separately; `history.more` is already coming on `failure-states`.

## Working rules

- **As the Mac plan's "Building on the owner's Mac":** his folder and processes are never touched, tests run on `jawd demo` in a folder of their own, the real agent label is never registered on his Mac by a build in progress, and work lands from the worktree it was built in.
- **Each step begins with its own implementation plan** in `docs/plans/`, as the menus and harness plans do, on a branch from main named for it (`mac-folders`, `mac-pictures`, …), and lands with its rows taken out of `mac/parity.toml` and `make check-parity` passing.
- **jawd's parts stay jawd's,** on its branches with their audits; a Mac step that finds the protocol short writes a proposal, as P10 is, rather than changing jawd itself.
- **Who:** the Mac builder builds the steps, after `mac-menus`' current tasks and the harness's first phase; the parity workstream keeps the check, the list, the parity canvas and the board current whenever main moves.
