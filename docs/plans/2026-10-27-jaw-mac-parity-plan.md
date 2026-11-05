# Jaw for the Mac: parity with the terminal

27 October 2026. The plan that brings the Mac app level with the terminal: first every gap where the terminal already has something the Mac lacks, then what the terminal and jawd are building now, as each lands, with the terminal tracked so that nothing new slips past (Owner, 27 Oct). It reorders what's left of [the Mac plan](2026-10-26-jaw-mac-plan.md) around those gaps rather than replacing it, and every step does things the Mac's way, as [the Mac design](2026-10-26-jaw-mac-design.md) and its later designs have them. The owner left the choices to this plan ("just dive in", 27 Oct) and approved it that afternoon, answering its four questions as their leans had it (Owner, 27 Oct); the decisions not marked as his are the plan's.

## Where it starts

- **The Mac app is on main** since 27 October, as `mac-m1` landed, its typing bubble with it. It does everyday text end to end, the whole palette, panes on a grid with named layouts, focus mode, the status line, the glance and linking, and draws a card for every kind of message.
- **The terminal is further along in 26 places** of the 149 features the parity canvas compares (`jaw-feature-parity.canvas.tsx` in the project's canvases), and the Mac in 14. 19 of the terminal's 26 are screens built against its made-up account that wait on jawd: pictures, opening and saving files, voice messages, the library, and folders with the roster and the new folder keys.
- **By the requests each sends,** the terminal asks jawd for 19 things the Mac never does: `layout.get` and `layout.set`; `folders.watch`, `roster.watch` and the six `folder.*` requests; `library.watch`; and the eight `voice.*` requests; and it reads 5 pushes the Mac doesn't: `layout`, `folders`, `roster`, `playback` and `recording`. The Mac sends two the terminal doesn't: `history.more` and `link.cancel`.

## How the terminal is tracked

- **`make check-parity`** (`scripts/check-parity.py`) reads which of jawd's requests each interface sends and which of its pushes it reads, from their source (the terminal's in `crates/jaw` and `crates/jaw-client`, the Mac's in `mac/`), and which requests jawd serves. Every one the terminal uses and the Mac doesn't is listed in [`mac/parity.toml`](../../mac/parity.toml) with the step below that brings it. The check fails on a gap the list doesn't have, and on one the Mac has closed that it still holds, so the list always says what's left and why.
- **Ahead of main:** `make check-parity ARGS="--terminal <branch>"` reads a terminal branch against the Mac on main, so what's being built is queued before it lands. Rows from a branch whose names main's protocol doesn't have yet say which branch (`from`); `library.shown` and the `library` push were queued that way until the library landed.
- **What a request can't show:** a feature that needs no new request, such as the terminal's folder keys or the IRC layout. Those come from each terminal branch's commits and the parity canvas, which is brought up to date whenever main moves; the canvas is the whole picture, and the check is the guard.
- **When it runs:** whenever main moves, with each terminal or jawd branch that lands, and at the start and end of every Mac step. Something new from the terminal becomes a row in the list and a line in its step below, in the same commit.
- **In CI** (Owner, 27 Oct): the check runs on every push, beside the data-audit check, once CI runs again; not in the pre-push hook, so the Mac never holds a terminal branch up.
- **The board** (`ws-mac-parity.canvas.tsx`) shows each step's state and what it waits on.

## The order

1. **The terminal says what, the Mac design says how.** A gap is closed when the Mac does what the terminal does, in the Mac's way: its keys, its menus, its panes and its looks. Where the Mac design draws something differently, the design wins; where it says nothing, the terminal's behaviour and words are the reference.
2. **Build ahead of jawd, against its branch.** The terminal built its screens against its made-up account, and the menus design builds folders against the `folders` branch's fixtures. Each Mac step starts as soon as jawd's branch has its protocol text and fixtures, and tests against them and `FakeJawd`. It lands on main once its tests pass, even before jawd's part does, saying what it waits on when chosen, as the terminal's do ("comes with M3"). It's tried end to end on `jawd demo` and by the owner once jawd's part is on main.
3. **What jawd serves already, first.** It works on the owner's account the day it lands.
4. **Then the terminal's screens, in the order jawd's parts land,** so each is tried end to end as soon as it can be: folders are through their second audit, media in is in its third, and the library and media out are stacked on media in.
5. **Then what's being built now, as each lands,** each step written when its branch's protocol is.
6. **The Mac's own work waits for the gaps,** except what's in flight: the right-click menus (`mac-menus`), which carry folders, and the harness's first phase (`mac-harness`), which every step's screens are checked in. The viewer, Redlamp's themes, the animations, notifications with Reply, the Dock badge, accessibility and a notarized build come after (Phase 4).
7. **Nothing the Mac leads on goes backwards.** Its 14 leads, the grid layouts, the emoji picker, the cards and the rest, stay as they are; they belong to a plan for the terminal, not this one.

## Phase 1 · What jawd serves already

### 1.1 The panes kept by jawd

The terminal's panes come back after a restart because jawd keeps them (P1, on main since 3d60818). The Mac keeps its layouts in its preferences. jawd's layout is one tree of splits (`format` 1, 16 panes, 16 KiB), and the Mac's are named grids: up to 12 slots on a grid's lines, a pinwheel that no tree can hold, and a shape that can be edited beside the saved one. So the Mac can open a tree, as the grid layouts design reads one, but can't keep its own layouts in format 1.

- **Now, done** ([its plan](2026-10-28-jaw-mac-panes-kept-by-jawd-plan.md)): Open the Terminal's Panes, in the Layouts menu and the palette: `layout.get` with `interface` `jaw`, the tree read as a grid into the unsaved panes, its focused pane taking the keys. This is the other interface's layout, which the terminal doesn't open yet (Owner, 26 Oct, "each able to open the other's"). A pane whose chat the chat list doesn't hold, archived or past its first 120, gets its row from a window of one row around the chat, in `all` and then `archived`, so no pane waits for good, the app's own layouts' included: it opens, or its slot says why, that the chat is gone or jawd's words for refusing the lookup.
- **With P10** (below): the Mac keeps its whole book of layouts with `layout.set` once each change settles, reads it with `layout.get` at launch, and takes the `layout` push, which jawd sends when another of the app's connections keeps a newer one, as the terminal does. Its preferences stay as they are until P10 lands, and are read once into jawd after it does. P10 is on main since 817bc02, its protocol text and fixtures with it; this half has a plan of its own, [the layout book plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md), built after the first half.
- **Done when** a relaunch opens the layouts the app was left with, kept by jawd, and the terminal's panes open in the Mac.

### 1.2 Starting jawd when nothing answers

The terminal starts jawd when nothing answers on `jaw.sock`, says so, and keeps drawing. Since [its plan](2026-10-30-jaw-mac-starting-jawd-plan.md), so does a Mac release, by the owner's decisions of 29 October in the jawd lifetime design (`docs/plans/2026-10-29-jaw-jawd-lifetime-design.md` on `jawd-lifetime`, its Defaults awaiting his review), as he chose on 30 October: "Now, by your 29 Oct decisions: start the bundled jawd when nothing answers, say Starting jawd, and remove jawd at Login… and the takeover; --for-apps and the app flag follow once jawd has them."

- **Done:** a release starts the jawd inside it (`Contents/Helpers/jawd`) when nothing answers, in a process group of its own so it goes on when the app quits, its output appended to `jawd.log` beside the socket, and draws at once, saying "Starting jawd" until jawd answers, for 15 s, as the terminal does; one that stopped because another jawd held the folder is started again a second later while nothing answers, so two interfaces starting jawd at once end with one. On an account's folder, jaw's own or one holding a `session.db`, it starts one only when `make mac` built it from a commit on origin/main or a release's tag on one, as the bundle records, so a build in progress never starts its jawd on the owner's folder; on jaw's own folder it then asks `SMAppService` to unregister the agent an earlier Jaw registered, never waiting on the answer, or, while the terminal's agent is installed, names it with `jaw agent uninstall` instead. Jaw › jawd at Login…, its prompts and the takeover are gone, and so is the bundle's LaunchAgent. Debug builds keep `--demo` and `JAW_HOME`, and `--demo` never runs on an account's folder.
- **Tried** on the build Mac's build user, which is clean for jaw, with a release Jaw built without the hardened runtime around main's jawd of 4 November and no engine, so nothing reached WhatsApp: it started jawd on a folder of its own and on jaw's own folder, the window saying so in every frame until jawd answered, never waiting on `SMAppService`; jawd went on after the app quit and after the app's process group was killed; quitting jawd and opening the app brought it back while another process held the folder's lock; and a Jaw built from a branch started none on jaw's own folder. Not tried end to end: an earlier Jaw's agent actually registered and then unregistered, which no Mac we know has, and the hardened runtime, the Developer ID's signature and the engine under a jawd the app started, which the owner's trial of a `make mac` bundle from origin/main brings.
- **Next, once jawd's branch has their protocol text:** `--for-apps` and the hello's `app`, as the last section of the plan says.
- **Done when** on a clean user account, a release build starts jawd when nothing answers, quitting it stops jawd within 30 s (the lifetime design's Default, awaiting the owner's review), and opening it again brings jawd back, with the window drawn the whole time. All but the stop after quitting holds now.

### 1.3 The budgets measured

The terminal's every budget is measured by `make perf` into the README. The Mac's bench (`--bench launch|open|memory|idle`) has its first numbers in `mac/README.md`, with opening a chat over budget at p99 (54.8 ms against 30) and 14 idle wake-ups not yet accounted for.

- `make perf-mac`, which the Mac plan has every milestone close with: launch, opening a chat, keys in a flood, scrolling, applying a frame of updates, memory and idle wake-ups, against `jawd demo`'s million messages, in both looks, its table in the README beside the terminal's with the load average beside each number. The open's p99 and the idle wake-ups are looked into, on a quiet machine first.
- **Done when** the README has the Mac's table and every row over budget says why.

## Phase 2 · The terminal's screens, as jawd lands them

### 2.1 Folders, favourites and the roster

The terminal's sidebar has the tree with its counts, smart folders, folders made, renamed, nested, moved and deleted from the palette, chats and people filed with m, and the roster with presence, on the made-up account. On `folders` it runs on jawd's, with favourites on Alt-1 to Alt-9. Since 27 October, Alt-Shift-Up and Alt-Shift-Down go to the folder before or after from anywhere.

- **The Mac's** is [the menus and folders design](2026-10-27-jaw-mac-menus-and-folders-design.md)'s build-order steps 4 and 5, Mac-M4's step 4 brought forward: the tree in the sidebar, picking a folder, the folder items in the chat menus, a folder's menu, naming in place, dragging, favourites on ⌘1 to ⌘9, the palette's folder actions, and the roster under the tree.
- **Folder keys,** which the design doesn't have yet: ⌥⇧↑ and ⌥⇧↓ for the folder before and after, beside ⌥↑ and ⌥↓, which already mirror Alt-Up and Alt-Down.
- **Done** ([the folders plan](2026-10-28-jaw-mac-folders-plan.md)): all of the above, on jawd's tree, roster and folder requests. The sidebar draws the tree with its counts, the built-in folders, yours nested by parent and position, and the smart folders, and a folder picked shows its chats, an empty one saying so in the terminal's words. Every chat menu files the chat with Add to Folder, Move to, Remove from and Favourite; a folder has a menu of its own; folders are named in place; chats and folders are dragged onto the tree; ⌘1 to ⌘9 and the Find menu's Favourites pick the slots; the palette has the folder actions; and ⌥⇧↑ and ⌥⇧↓ go to the folder before or after. The roster under the tree lists the picked folder's people with how they are: typing… or recording… from their own chat, online, or when last seen, faint where jawd isn't following them. It opens or starts their chat, and has their menu. It's tried end to end on `jawd demo`.
- **Left:** the owner's look at the selected rows, the tree's and the roster's, on a real screen, with and without the keys, in both looks; and VoiceOver's Show Menu on someone's row at the same time, which the shared build Mac can't try.
- **Needs from jawd:** M6, on main since a44a2fe2, and the typing in a roster's people's own chats that came with presence's half of the protocol.
- **The check:** `folders.watch`, `roster.watch`, the six `folder.*` requests and `favourite.set`; the `folders` and `roster` pushes.

### 2.2 Pictures

The terminal draws photos, albums two across, stickers, and the posters of GIFs, videos and video messages from jawd's previews, with kitty, iTerm2, sixel or half-blocks, on the made-up account until media in lands.

- **The Mac's** are Mac-M2's steps 1 and 2: the image queue (four at a time, cancelled when scrolled past), the 64 MB cache of decoded pictures, the preview chosen by the pixels drawn, the blurred thumbnail fading into it; photos, albums, stickers (still, animated with P4 in Phase 4), GIFs looping while on screen, videos and video messages, their posters first and playing in place. The frames on main already keep each picture's shape, so the pictures drop into them and nothing moves.
- **Done** ([the pictures and files plan](2026-10-30-jaw-mac-pictures-and-files-plan.md)): everything above but motion. Photos, albums, stickers and the posters of GIFs, videos and video messages are drawn in their places from jawd's previews, each placed from its row before any pixel, the posters with ▶ looking at them with Quick Look, as the terminal draws them; an album jawd sends as its counts, as it does today, is drawn as a line of them with its photos below as their own messages, until jawd gathers them into one row (P12, in the Mac design); links' thumbnails go through the same queue and cache, and `--bench pictures` measures a fling through them for the README's table.
- **Left for Phase 4,** with the rest of Mac-M2: motion (GIFs looping while on screen, videos and video messages playing in place, stickers animated with P4) and a document's first page on its card, where 2.3's card has its icon.
- **Follow-ups** from the step's third review: the older of the cache's tests still empties it with `PictureCache.empty()`, "as the app does with no window open", where the app now closes it with the window, so the test should close and show the window through `windowClosed` and `windowShown`, and `empty()` can go; and the README's 59 to 61 MB with no window open was measured on 31 October, before the cache closed with the window, which its sentence should say, unless `--bench pictures` measures it again.
- **Needs from jawd:** media in, on main since 30 October: the `media` rows' `previews`, `thumbnail`, `state` and `progress`.
- **The check:** nothing new; media rows come with `messages.watch`.

### 2.3 Opening and saving files

The terminal's Enter opens a photo, a video or a file in its app, and s saves it; both say they come with M3 until media in lands.

- **The Mac's:** Space or a click looks at a photo, a video or a document with Quick Look, ⌘↩ opens it in its app, Save puts it in Downloads (`media.save`), and it drags out to the Finder. Each download state (remote, downloading with its ring, expired, waiting for the phone, gone) says what it is and offers what can be done, asking again with `media.fetch`. Quick Look gives what the terminal's opening gives; the Mac design's own viewer comes after parity (Phase 4).
- **Done** ([the pictures and files plan](2026-10-30-jaw-mac-pictures-and-files-plan.md)): Space, Enter, a click or a double-click looks at a photo, a video or a document with Quick Look, ⌘↩ opens it in its app and Open With in another, and s or Save to Downloads puts the copy jawd writes there, "saved to Downloads as …", with Save HD Copy to Downloads, for a photo with one, and Show in Finder beside it in the message's menu and the Message menu. It drags out as a clone once it's here, which any app takes, and before that as a promise kept through `media.save`, which the Finder and Mail take. Each download state says what it is and offers what can be done (Download, Stop Downloading, Try Again, Ask the Phone Again), and a look or an open of a file that isn't here asks jawd for it and opens it once it is. What's looked at, opened or dragged out is a clone of jawd's file, so no app changes jawd's, and every copy the app hands over is quarantined as a message's attachment.
- **Left:** the owner's hand test, which the plan's task 5 is done when: a look, an open, a save and a drag of the lake on `--demo`, and Save into the real Downloads once, checking that the copy is quarantined under Jaw's name (`xattr -p com.apple.quarantine ~/Downloads/<file>`) and whether macOS asks for Downloads under Jaw's name, and in a signed release under jawd's too.
- **Follow-up** from the third review: a quarantine that can't be set goes unsaid, the status line saying "saved to Downloads as …" all the same (`savedAnswer` in `AppModel`, and the promise's in `Dragged`), where the clones refuse to hand a file over without it; it should be said as a warning.
- **Needs from jawd:** media in (`media.fetch`, `media.save`, `media.cancel`).
- **The check:** the Mac sends `media.fetch`, `media.save` and `media.cancel`. The terminal sends `media.save` too since `terminal-actions` (4 November), s on a message's file asking where from `~/Downloads/`, and `media.cancel` to stop a file of its own on its way, but not `media.fetch` yet, which `make check-parity` lists as the Mac's; `media.cancel`'s row, held for 2.5, came out of the list with it, and the terminal sending any of the three later queues nothing, since the Mac sends them first.

### 2.4 The library

The terminal's Ctrl-L has every photo, video, voice message, file and link, across chats or in one: the strip of kinds, `from:`, `in:`, `since:` and `until:`, Enter to open, Ctrl-G to the message. It runs on the made-up account, and on jawd's `library.watch` on `library`.

- **The Mac's** is Mac-M2's step 7: in a pane, with ⌘L for every chat or from a chat's header for one; the kinds with their counts, the filters as tokens, the grid of previews, rows for voice, files and links, Enter to open and ⌘↩ to the message.
- **Done** ([the library plan](2026-11-01-jaw-mac-library-plan.md)): all of it, on jawd's `library.watch`. Enter looks at a photo, a video or a file with Quick Look, as 2.3's Enter does, the photos and videos near it paged through, the viewer coming after parity (Phase 4); a voice message plays once jawd plays voice (2.6), and whether it was played, which P14 carries, is drawn then, as the conversation draws it from 2.6. What jawd's library rows don't carry, a voice message's waveform, a file's download state and a link's thumbnail, is the Mac design's P14; until it's served they're drawn as even bars, "not downloaded" and the link's mark. `from:` offers you, the senders of the items held and the members of the chats open in panes until the Mac watches the roster (2.1). VoiceOver says each item as it's drawn, a button with its menu's actions, and the selection as it moves; the strip's kinds are radio buttons; and while the pane is focused, the Message menu's Save to Downloads and Show in Finder act on the selected item.
- **Left, for `mac-perf`:** two of the design's budgets, as the README's What it costs has them, by `--bench library` over 3,000 photos. Scrolling the grid misses the frame's, as the conversation's does: 16.1 to 18.6 ms at p99 and 98 to 114 ms of hitches a second, against 8.3 ms and 1 ms. A move's landing misses the 2 ms p99 for applying a frame of updates, taking 3.8 to 4.7 ms at the median of each fling, because each landing reloads the grid's data and makes every cell on screen again. The footprint after the fling, measured with the pane still shown, is 65.3 to 70.3 MB, within its 150 MB.
- **Left:** the owner's hand test of the app's own library code, which no suite runs: on `--demo`, ⌘L and ↓ select a photo in every chat's library, and the Message menu's Save to Downloads and Show in Finder then act on it, the rest of its file items greyed, and on the selected message again once a chat's pane is focused (`AppModel.focusedLibrary` and the Message menu's `itemFileCommand`).
- **Follow-ups** from the step's second review: two of its fixes have no test that fails without them, the light look's ring (`LibraryDrawing.state`'s disc, nearly opaque in the light look: as clear as the dark one's, the light grid's reference pictures still pass, within `compare`'s allowance) and prefetching in the window's colours (a tile prefetched in a window whose colours aren't sRGB should be shown from that decode, with no second decode in the queue's counts); nor has a cell asking again when only its window's colours change (`LibraryCell.viewDidChangeBackingProperties`), which a test can check by setting its window's `colorSpace` and seeing the cell ask in the new colours.
- **Needs from jawd:** the library, on main since 77d44257, links with it.
- **The check:** `library.watch`, `library.shown` and the `library` push, out of `mac/parity.toml` as the Mac sends and reads them.

### 2.5 Attaching photos, videos and files

The terminal's Ctrl-O has a picker and `send.media`, SD or HD, albums, progress and cancelling, on `media-out`.

- **The Mac's** is Mac-M3's step 5, the box design's B2: pasting, dropping, ⌘O and ⇧⌘O, the strip of up to 30 above the field with captions, SD or HD (⌥ held as Send is pressed), `send.media`, progress from the `job` pushes, and cancelling. The camera (B3) comes after, and "no ffmpeg" is said here, as jawd's `status` reports it.
- **Needs from jawd:** media out, on `media-out`, built and landing after media in.
- **The check:** `send.media` and the `job` push; the Mac sends `media.cancel` already, for downloads (2.3).

### 2.6 Voice messages

The terminal draws the waveform; Enter plays and pauses, the speeds and seeking go by key, and the next unplayed message follows. All of it goes through jawd's `voice.play`, `voice.pause`, `voice.seek`, `voice.speed` and the `playback` push, which jawd has served since 2 November; played receipts are still to come.

- **The Mac's** is Mac-M2's step 3: the waveform's played part, play and pause, 1×, 1.5× and 2×, seeking by a click and by ← and →, the dot until it's played, the next unplayed after one ends, the playhead moving between jawd's pushes only while playing, and the small player in the status line.
- **Needs from jawd:** voice, M5: jawd has served `voice.play`, `voice.pause`, `voice.seek`, `voice.speed` and the `playback` push since 2 November. Built against the protocol and `FakeJawd`'s `playback` pushes, as the terminal was against its made-up account.
- **The check:** `voice.play`, `voice.pause`, `voice.seek`, `voice.speed` and the `playback` push.

### 2.7 Recording a voice message

The terminal's Ctrl-R records with a live level; Enter sends, and Esc cancels.

- **The Mac's** is Mac-M3's step 6, with the box design's microphone in Send's place when the box is empty: ⌘R, jawd's levels and time in the box, Enter to send, ⌘R to stop and listen, Esc to throw it away.
- **Needs from jawd:** nothing more: jawd has recorded voice messages since 2 November. A jawd the app starts asks for the microphone in Jaw's name, and Jaw.app carries the usage string and the entitlement for it beside jawd's; one the terminal starts asks in the terminal app's (the Mac design's "The microphone, in Jaw's name", and its Risks). What's left is the Mac's own recording.
- **The check:** `voice.record`, `voice.stop`, `voice.send`, `voice.cancel` and the `recording` push.

## Phase 3 · What's being built now, as it lands

### 3.1 Typing and presence in chats

The Mac draws someone typing as WhatsApp does (0de10c7): a bubble after the newest message, and the header's counting dots. jawd's typing and the terminal's landed on 30 October (9b3fbdf4..7e047803), moving the socket to protocol 2, whose chat rows carry no typing; presence is being built on `presence`.

- **Typing, done** ([its plan](2026-10-30-jaw-mac-typing-plan.md)): JawKit keeps who's typing as each watch's answer and the `typing` push say it, as jaw-client keeps it, and the bubble, the header's who's typing, the status line's left half and the chat list's row follow it, in the terminal's words, the names first in a group.
- **The roster too:** since step 2.1 the roster reads who's typing or recording in each person's one-to-one chat from the app's typing map, which the panes and the chat list read (`AppModel.rosterTyping()` into `RosterController.typing`), and JawKit keeps typing for a roster's people's own chats as jawd sends it, in the roster's answer and the `typing` pushes, since the protocol has a roster show them (Presence and typing in chats), so it shows someone typing whatever the chat list shows.
- **Presence, next:** once jawd serves it, the header and the status line's left half show online or last seen, faint where jawd isn't following someone, from the answer's `presence` and the `presence` push, as the typing plan's last section says.
- **The check:** the `presence` push; the `typing` push's row is out.

### 3.2 Chat actions and stars

Both interfaces already send `chat.pin`, `chat.archive`, `chat.mute` and `chat.mark_unread`; jawd's side is being built on `chat-actions` (P8). There's nothing for the Mac to build for the four: the Chat menu sends them, and so do `mac-menus`' right-click menus. Once jawd's step lands, each is tried end to end on `jawd demo` (three pins at most, archiving unpinning, muting for ever as -1, marking read sending receipts), and Star comes into a message's menu (`mac-menus` task 7). The terminal stars with * since `terminal-actions` (4 November), "starred" with ★ after the message, so `star` is this step's row in the list until task 7 lands.

### 3.3 Links and polls

Since `m2-content` landed on 28 October (6c1c81f), jawd sends link previews both ways, keeps every link for the palette's answers and the library, counts polls' tallies, and serves `send.poll` and `poll.vote`.

- **Done** ([its plan](2026-10-30-jaw-mac-links-and-polls-plan.md)): a link's preview under its text, with the thumbnail jawd keeps, the title, two lines of the description and the host, for what you send as for what others do; a poll's card with each option's tick, count and bar against how many voted, and how many voted, drawn again as jawd upserts the row; voting by clicking an option, from the message's Vote menu, or with Enter on a selected poll, which opens that menu under it for the arrows or an option's first letters and Return, the digits staying the quick reactions, as in the terminal; a poll made from the box's + menu, the box design's Poll form brought forward from its B3, sent with `send.poll`; and links in the palette's answers, Enter opening one and ⌘G going to its message. Tried on jawd's demo engine: a vote in Rui's poll counted and was taken back, and a poll sent came back as jawd's row.
- **Waiting:** the box's preview of a link while it's typed, which needs jawd's `link.preview` (P11, proposed in the Mac design and the protocol's Composing); the tallies' growth, with Mac-M5's animations; who voted, by name, which jawd doesn't send, if the owner wants WhatsApp's View Votes; the library's links, with 2.4.
- **The check:** no rows to take out. The terminal votes with `poll.vote` since `terminal-actions` (4 November), by the Mac's rules, so the two don't drift apart: Enter on a selected poll, or the palette's Vote in this poll, opens a vote picker of its own, where 1 to 9 or Enter on an option votes, and 1 to 6 stay the quick reactions on a selected message, as on the Mac. It doesn't make polls yet, so `make check-parity` says the Mac uses `send.poll` where the terminal doesn't, and once the terminal does, that's no gap either.

### 3.4 Failure states

The Mac draws every state already. Once `failure-states` lands (the engine restarting said, the phone offline reported, update needed, linking again after the phone logs jaw out), JawKit's live suite runs each through `jawd --engine demo --fail`, as `crates/jaw/tests/failures.rs` does for the terminal, so both interfaces are held to the same states.

### 3.5 Settings kept by jawd

Since `settings` landed on 27 October (ebbd905), the terminal's settings are jawd's, a domain for each frontend (`settings.set`, the `settings` push and `themes`), with Omarchy's 23 themes and the IRC layout, so this step can start whenever the Mac builder reaches it. The Mac keeps its own preferences, as its design has them, and already has two looks.

- **What jawd can keep, jawd keeps** (Owner, 27 Oct): the settings that are jawd's (what downloads by itself, the cache's size, notifications, P5) are set from the Mac's Settings through jawd and shared with the terminal; the Mac's look (bubbles or lines, density, text size, the panes' outline) stays in its own preferences.
- **Both sets of themes** (Owner, 27 Oct): once `settings` lands, the Appearance pane lists Omarchy's 23, read from jawd's `themes` and mapped onto the Mac's roles as the terminal maps them onto its own, beside jaw's Tokyo Night; Redlamp's families join them in Mac-M5.
- **The check:** `settings.set` and the `settings` push.

### 3.6 Scheduled messages and reminders

[Their design](2026-10-27-jaw-scheduled-messages-and-reminders-design.md) builds the terminal's screens on jawd's branch and the Mac's on a branch of its own beside it (P9; the Mac plan's Mac-M3 steps 7 to 9). This plan starts the Mac's as soon as jawd's branch has the protocol's text and fixtures, so both land together: the first feature the two interfaces build at once, not one after the other. When the protocol has the `scheduled.*` and reminders' requests, the check queues them here.

### 3.7 Assistants and the catch-up

After the first release: Settings › Assistants with [M9](2026-10-27-jaw-mcp-design.md)'s second stage, beside the terminal's `jaw mcp grant` and its palette's Assistants; and the catch-up's panel with M11.

### 3.8 Invitations, and what jaw couldn't show

Since `unknown-kinds` landed on 28 October (7f8fc11), jawd shows most of what it called "jaw can't show this yet": history's community and group events come as system lines in jawd's words, which the Mac draws already, and invitations to join a group, follow a channel or be one of its admins come as a kind of their own, `invite`, with the group's or channel's name and when it runs out, under the sentence WhatsApp writes for it as the caption ("Invitation to follow the channel, 'Jaw news'"). A message row carries no summary, so the terminal draws an invitation as "✉ invitation" over its caption. Since 28 October the Mac draws it in both looks as a line over its caption, saying what it's to, the name and until when ("Group invitation · Lisbon · until 2 November, 14:00"), where it said it couldn't show it.

- **The Mac's:** JawKit's rows take `invite`, done; what's left is the card in both looks, in place of the line: what it invites to, the group's or channel's name, and when it expires.
- **The check:** nothing, since it's a kind and not a request; the parity canvas carries it.

### 3.9 Contact cards

Since `send-contact` landed on 1 November (fb19ae45), jawd serves `send.contact` (one card or several, each a name and a vCard 3.0) and shows every card, received or yours, as the protocol's `contact` row: its name and its vCard, holding each card one after another when a message shares several. The terminal sends one from the palette's Send a contact, made from one of jawd's people's name and number, draws each card as its name and numbers, and starts a chat with whoever a card shares on Enter.

- **The Mac's, done** ([its plan](2026-10-31-jaw-mac-contact-cards-plan.md)): every card drawn as jawd's row has it, received or yours, one or several, in both looks, from what JawKit read of its vCard once as the row arrived, on the client's reading thread, so nothing on the main thread reads one: its photo or its initials, its name and its numbers, several as their pictures, names and how many. Message on a card starts a chat with its number with `chat.start`, offering each number when it has several, as the terminal's Enter does, and several cards open a sheet of them. The box's Contact… is the box design's B3 contact form, brought forward for contacts alone: the system's contact picker, each card a vCard 3.0 the Contacts framework writes (CNContactVCardSerialization) of a contact's names, organisation, numbers and emails alone, counted as jawd counts before Send, `send.contact`, and jawd's refusals in its words, the form opening again as it was.
- **What waits:** Add to Contacts on a card, the Mac's own, in Phase 4; a card's WhatsApp picture, a follow-up now that 2.2's image queue is on main; and sending jawd's people as cards, as the terminal's palette does, if the owner wants it.
- **Needs from jawd:** `send-contact`, on main since fb19ae45.
- **The check:** `send.contact`, which the Mac sends, so the ledger holds nothing for 3.9.

### 3.10 Communities in the chat list

Since `communities` lands, jawd shows a community as one row wherever its groups would show, as the protocol's "Communities in a chat list" has it: in Unread, Groups, Archived, your folders and the smart folders too, not only in All and Communities, its row summing up the groups of it the list holds (`children_unread`, and `children_mentions`, new), its `last` their newest message and its place just above the newest of them. The groups now keep their own `sort` rather than sorting right after their community, and a window brings the groups only of the communities named in its `open`, every community's when it sends none, as the Mac does now. The terminal sends `open` and shows the Communities folder with each community open.

- **The Mac's:** read `children_mentions` for a community's @ badge; place a community's groups under it by `community` and its own `sort`, the announcement group first, as `rebuild()` does already, but hide a group under a row only when that row's `kind` is `community` (a group naming a chat that's only a group to jaw shows on its own); send `open` with `chats.watch` and `watch.move`, moving the window as a community opens or closes; and open every community in the Communities folder.
- **The check:** nothing, since `open` is a parameter of requests the Mac sends already; the parity canvas carries it.

### 3.11 Disappearing and view-once messages

The terminal's half of M2's disappearing and view-once messages is built on `disappearing`, with jawd's, which erases a message at its `expires_at`, keeps a view-once message unseen until it's opened once, and sets a chat's timer through the outbox. The terminal says a chat's timer in its title ("· ⏱ 7 days"), sets it from the palette (Disappearing messages: off, 24 hours, 7 days or 90 days), marks a message that disappears in the gap before its time and says when in its info, marks a view-once message ("① View once photo", "· opened" on yours once it's been), and opens someone else's once over the screen, its caption under it and how long until it goes, until Esc closes it and jawd erases it.

- **The Mac's:** the header's timer from chat rows' `ephemeral`, and Disappearing Messages in the Chat menu and the palette with the four timers (`chat.disappearing`), its refusal said where it was asked; a timer beside a disappearing message's time, as the Mac design has it, and when it goes in the info sheet; and for view-once, in place of "open it on your phone", a click or Enter that opens it once (`view_once.open`) in the viewer with its caption and a countdown to `until`, closed by Esc or the viewer going (`view_once.close`), never forwarded; yours say Opened once its status is `played`. A video or a voice message plays from the answer's `path`.
- **The check:** `chat.disappearing`, `view_once.open` and `view_once.close`.

## Phase 4 · The Mac's own, after the gaps

The rest of Mac-M2 (the viewer, GIFs looping while on screen, videos and video messages playing in place, a document's first page, MapKit's maps, contact cards' Add to Contacts, animated stickers with P4) and Mac-M5 (Redlamp's themes, the animations, the Mimic's chomp everywhere, notifications with Reply and Mark as Read with P2, the Dock badge with P3, Settings whole with P5, accessibility, a signed and notarized build with updates). Each starts once the gaps of its area are closed, so the viewer can follow 2.3 while voice is still waiting on jawd.

## P10, a layout book

A proposal for jawd's builders, for 1.1, added to the Mac design's proposals. `layout.set` and `layout.get` take `format` 2: a book of named layouts, each slots on a grid's lines (its columns and rows as fractions, each slot its lines and its chat), its edited shape beside its saved one, which is focused, and the unnamed layout, as the grid layouts design keeps them in `LayoutBook`. Format 1 is still taken and read, a tree being read as a grid. jawd keeps following merged and aliased JIDs in every slot of every layout, as it does in a tree's panes, and the limits are sized for a book: 12 slots a layout, the ten layouts ⌃⌘0 to ⌃⌘9 reach, and its bytes measured against a bound `status` reports. It changes `crates/jawd/src/layout.rs` and the store's `interface_state`, so it follows "Data integrity" in jaw's design and is audited by the database expert before it lands.

## Done when

- `make check-parity`'s list holds nothing but what's not for the Mac, each with why.
- The parity canvas shows no feature further along in the terminal.
- The owner spends a day on his own account doing in the Mac app everything he does in the terminal.

It stays done: each terminal feature that lands gets its Mac step that day, queued by the check or from its commits, and the board shows it until it's closed.

## The owner's answers

The plan's four questions, answered on 27 October as their leans had them:

1. **The Mac's settings in jawd:** yes, what jawd can keep, with P5; the look stays the Mac's own (3.5).
2. **Themes:** both, Omarchy's from jawd's `themes` first and Redlamp's in Mac-M5 (3.5, Phase 4).
3. **The check in CI:** yes, on every push once CI runs again, but not in the pre-push hook.
4. **The other way:** yes, a plan of its own for the terminal's builders: [the terminal's parity plan](2026-10-27-jaw-terminal-parity-plan.md).

## Working rules

- **As the Mac plan's "Building on the owner's Mac":** his folder and processes are never touched, tests run on `jawd demo` in a folder of their own, no agent opens a release Jaw on his Mac, since one can start its jawd on his folder whenever his isn't answering, and work lands from the worktree it was built in.
- **Each step begins with its own implementation plan** in `docs/plans/`, as the menus and harness plans do, on a branch from main named for it (`mac-folders`, `mac-pictures`, …), and lands with its rows taken out of `mac/parity.toml` and `make check-parity` passing.
- **jawd's parts stay jawd's,** on its branches with their audits; a Mac step that finds the protocol short writes a proposal, as P10 is, rather than changing jawd itself.
- **Who** (Owner, 27 Oct): the Mac builder builds the steps, after `mac-menus`' current tasks and the harness's first phase; the parity workstream keeps the check, the list, the parity canvas and the board current whenever main moves.
