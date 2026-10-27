# jaw for the Mac: design

26 October 2026. A plan; nothing here is built. It follows [jaw's design](2026-10-16-jaw-design.md), whose principles, budgets and rules hold here too, and [the protocol](../../protocol/README.md). The build order is [the Mac plan](2026-10-26-jaw-mac-plan.md); the screens are drawn in the Mac wireframes canvas (`jaw-mac-wireframes.canvas.tsx` in the project's canvases). The owner answered its fourteen questions on 26 October, and each answer is a decision below, marked "Owner, 26 Oct".

## What it is

jaw for the Mac (`jaw.app`) is a native macOS client for jaw: another interface on jawd's socket, on exactly the terminal's architecture. jawd keeps every message and prepares everything an interface shows: names, formatting, quotes, reaction and receipt totals, poll tallies, previews at 256 and 768 pixels, search, drafts, folders, presence, voice playback and recording, notifications. The app draws what jawd's windows hold and asks jawd for everything else, as `jaw` does.

It shows every kind of message inline, albums included, and has the command palette, split panels, the library and everything else the terminal does, in Redlamp's themes, with the Mimic where the brand lives. It never makes you wait: your chats 250 ms after launch, a chat open 30 ms after the key, the glyph you typed in the next frame, photos scrolling at 120 Hz, and nothing running when nothing changes.

jaw's design lists "a desktop app on the same socket" under Later. This is it.

## Decisions

The owner's answers are marked "Owner, 26 Oct"; the others are this design's, with their evidence further down.

- **Another interface, on the terminal's architecture.** The app connects to `jaw.sock`, says hello as `jaw-mac`, and watches windows: the chat list, each open chat, the folders, the roster, the library. It keeps nothing of jawd's on disk, only its own preferences and window frames. Its rows are jawd's rows: it never draws a guess, as the terminal's actions don't.
- **jawd and jaw-engine inside the app, as the one LaunchAgent, and the terminal uses it** (Owner, 26 Oct). They ship in `Contents/Helpers` with the terminal beside them, signed with the app's identity, and the app registers jawd with `SMAppService` under the label `jaw agent` already uses. One build of jawd, jaw-engine and jaw speaks one protocol; jawd runs at login, so notifications come with no window open; and `jaw.lock` with the shared label makes two jawds impossible.
- **A Swift client of the protocol** (Owner, 26 Oct), not jaw-client through UniFFI: each row is decoded once, into the value the view draws; there's no foreign boundary in the app and no Rust build inside Xcode's; and the interface's half (keys, asking again, owed drafts, all in the terminal's `wire.rs`) is Swift either way. "The socket client" has the evidence.
- **AppKit where lists and text need it, SwiftUI for the rest.** The chat list, the conversation, the library's grid and the message box are AppKit; settings, sheets, the palette, the linking screen, empty states and the viewer's chrome are SwiftUI. The evidence, measured, is under "The UI stack".
- **macOS 26 and later, signed with the owner's Developer ID and notarized** (Owner, 26 Oct), as Redlamp: the newest APIs (the view display link, Observation, Liquid Glass) without fallbacks, a stable identity so macOS asks for the microphone once, and an app Gatekeeper opens. Swift 6 language mode, warnings as errors.
- **Not sandboxed, so not in the App Store.** The app reads jaw's folder (previews and files, by the paths jawd sends) and runs jawd and jaw-engine as helpers, which the App Sandbox forbids. It's hardened as well as signed and notarized.
- **In jaw's repository, under `mac/`** (Owner, 26 Oct), so a protocol change and both its clients land in one branch, and the fixtures, `jawd demo` and `make perf` are at hand.
- **One message box per conversation, naming its chat, with the status line across the window** (Owner, 26 Oct). With panes, a box under each chat leaves no doubt where a message goes, and the status line keeps the spike's view of the rest. The spike's full-width box isn't taken.
- **Two looks, picked in Settings: bubbles and lines** (Owner, 26 Oct). Bubbles, yours on the right, are the default; lines are the terminal's layout, names on the left and yours from a third of the way across; either can be compact. Both ship in Mac-M1: the layout cache and every kind of message are then drawn for both from the start, which costs less than adding a look once media is in. "Two looks" has the detail.
- **Notifications from the app while it runs, jawd's own once it's quit** (Owner, 26 Oct). With P2 they have Reply, Mark as Read and a click that opens the chat; the app stays open with its window closed, in under 25 MB; nothing is lost when it's quit.
- **One split layout per interface, each able to open the other's** (Owner, 26 Oct): a 160-column terminal and a 27-inch window want different panes, and opening the other's covers moving between them.
- **The library opens in a pane, like a chat** (Owner, 26 Oct), so it sits beside the chat it came from, with the keys and the layout panes already have.
- **Mac keys, with the terminal's where they don't clash** (Owner, 26 Oct): ⌘K opens the palette and Ctrl-K does too; ⌥↑ and ⌥↓ move between chats, as in the terminal; ⌘1 to ⌘9 go to favourites, since ⌥ and a digit type a character on a Mac.
- **Redlamp's themes copied into jaw, with a check that keeps them in step** (Owner, 26 Oct). `RedlampDesign` is a Tuist framework that depends on Redlamp's engine API and is mostly a photo editor's controls; jaw needs only its ten-colour theme families, and `scripts/sync-themes` keeps the copy diffable, as `scripts/sync-protos` keeps whatsmeow's protobufs.
- **jaw's Tokyo Night by default, following the system's light and dark** (Owner, 26 Oct): it looks like jaw from the first launch and still respects a light Mac.
- **MapKit's maps for locations, with a setting to turn them off** (Owner, 26 Oct): a map reads at a glance, but MapKit asks Apple for its tiles.
- **Homebrew's ffmpeg for sending video, GIFs and audio** (Owner, 26 Oct), as jawd uses today, the app saying when it's missing: it's on the owner's Mac already, and the choice can change before a build goes to anyone else.

## Principles

jaw's seven principles hold. Six more bind the app:

1. **Input first.** A key or a click is handled before any update waiting to be applied, and the box always takes keys. Nothing a key does waits for jawd, for a picture or for an animation.
2. **Off the main thread.** Decoding jawd's lines, measuring and laying out text, and decoding pictures all happen on other threads. The main thread applies what's ready and draws, and each of its turns stays under 8.3 ms, one frame at 120 Hz.
3. **Nothing jumps.** As in the terminal: the chat list keeps the chat under the pointer or the keys in place while you move through it, and the conversation anchors on a message, so history arriving above moves nothing. A picture's place is known from the `w` and `h` jawd sends before its preview arrives, so a preview landing never moves the text around it.
4. **Idle is idle.** When nothing changes, nothing is drawn: no display link, no timers, no polling, no wake-ups, but one a minute for relative times while a window shows them.
5. **Animation is decoration.** Every animation runs on Core Animation's render server after the model has already changed, so it costs the main thread nothing per frame, and a key pressed during one finishes it at once. Reduce Motion turns movement into a fade or into nothing.
6. **Draw what jawd says.** A send or an action shows when jawd's row comes back, which is once it's on disk, about 10 ms (the durable-ack budget), one or two frames. The box clears on Enter and gets its text back if jawd turns the message down, as the terminal's does.

## Budgets

| What | Budget | How it's measured |
| --- | --- | --- |
| Launch to a usable chat list | 250 ms p99 with jawd running; 500 ms after a reboot | `jaw-mac bench launch`: from the process's start, as the kernel records it, to the first frame with the chat list's rows and the box taking keys |
| Opening a chat | 30 ms p99 from the key or the click to its messages drawn; 50 ms p99 while 2,000 messages a second arrive | the bench, as `jaw bench` times opens |
| Keypress to glyph | the key handled in under 4 ms p99 and its glyph in the next frame (8.3 ms at 120 Hz), while 2,000 messages a second arrive | the key event's timestamp to the display link's timestamp of the frame that shows it |
| Scrolling with photos | no frame missed at 120 Hz: every main-thread turn under 8.3 ms p99, and hitches under 1 ms per second of scrolling, flinging through a chat of photos and albums and through the library's grid | Redlamp's `MainThreadMonitor` and the display link's frame times, on a ProMotion display; on a 60 Hz display, the same work budget |
| A pushed message drawn | in the frame after it's read from the socket | the bench, as jaw-client's bench times read to applied |
| Applying a frame of updates | 2 ms p99 on the main thread in a flood | the bench |
| Memory | under 80 MB with a screen of text, under 150 MB with a screen of photos or the library, under 25 MB with no window open; decoded pictures capped at 64 MB | `phys_footprint` through `task_info`, as jaw-client's `usage` reads it |
| Idle | no frames, timers or wake-ups when nothing changes, but one a minute while relative times show | the bench's wake count over a quiet minute, as `jaw bench` counts the terminal's |
| Energy | nothing beyond reading jawd's pushes when idle; App Nap allowed when hidden; animations only while visible | Activity Monitor's energy impact and `powermetrics` over ten idle minutes and ten minutes of a message a minute |
| App size | under 60 MB with jawd, jaw-engine and the terminal inside; the app's own code under 10 MB | the bundle on disk |

- The terminal's first frame is 50 ms; an app's launch costs more before its first line runs (dyld, AppKit, the window server, and on the owner's Mac the endpoint security that looks at each launch, 7 to 37 ms for jaw). The rest is the terminal's path: hello, `chats.watch`, a 60-row window and its first frame.
- The memory budgets are estimates to confirm in Mac-M1 and Mac-M2. The floor is measured: 17.3 MB for an AppKit window with a 10,000-row table and a SwiftUI view drawn off screen ("Measurements").
- Every budget holds in both looks, bubbles and lines, and the bench measures each.
- The owner's Mac Studio likely drives a 60 Hz display. The 120 Hz budget is measured as work per frame on any display, and on a ProMotion MacBook before Mac-M5 closes.
- `make perf-mac` measures every row against `jawd demo`'s million messages, beside the terminal's in the README's "What it costs", with the load average beside each number.

## How it fits together

```text
WhatsApp ⇄ jaw-engine (Go, whatsmeow, session.db)
              ⇅ length-prefixed protobuf frames on the engine's stdin and stdout
           jawd (Rust: jaw.db, media, voice, folders, presence, notifications)
              ⇅ JSON lines on jaw.sock
     jaw (the terminal)  ·  jaw.app (Swift: AppKit lists and text, SwiftUI around them)
```

```text
jaw.app/Contents/
  MacOS/jaw                     the app
  Helpers/jawd                  the daemon, signed with the app's identity, its Info.plist embedded
  Helpers/jaw-engine            the engine
  Helpers/jaw                   the terminal, linked onto the PATH when you ask
  Library/LaunchAgents/dev.badcoffee.jaw.jawd.plist   registered with SMAppService
```

Inside the app:

```text
socket reader ─ lines ─► decode queue (rows as Swift values) ─► merged per window, bounded
                                                                   │ at the next display frame,
                                                                   │ or at once for an answer awaited
main thread ◄──────────────────────────────────────────────────────┘ input first; apply up to 2 ms; draw
layout queue   heights and text layout per message, version and width (Core Text)
image queue    previews decoded with ImageIO, up to 4 at a time, cancelled when scrolled past
```

## The app as a jawd interface

### Who starts jawd

- **jawd inside the app, as the one LaunchAgent.** The app registers `Contents/Library/LaunchAgents/dev.badcoffee.jaw.jawd.plist` with `SMAppService.agent`, its program `Contents/Helpers/jawd`, restarted 10 s after it stops and given 10 s to stop, as `jaw agent install` does today. It appears in System Settings' Login Items as jaw, with the app's icon. jawd starts the engine, as always.
- **One jawd, by construction.** The label is the terminal agent's own, `dev.badcoffee.jaw.jawd`, so there can't be two agents, and jawd's `jaw.lock` refuses a second jawd whoever starts it. When `~/Library/LaunchAgents/dev.badcoffee.jaw.jawd.plist` is already installed, the app says so and offers to take it over (stop it, remove it, register its own) on the owner's click; it never does that by itself.
- **Never a downgrade.** Before taking over, the app asks the running jawd for `status`. If that jawd is newer than its own, the app keeps it and says so, since an older jawd refuses a newer `jaw.db`.
- **The app starts without waiting.** It connects at launch. If nothing answers, it registers or kicks the agent and draws its window at once, saying "Starting jawd" in the status line, as the terminal says it while jawd starts. With jawd at login, that's only the first launch.
- **The terminal finds jawd beside it,** as `wire.rs`'s `jawd_binary()` already does, and the app's Settings offer to link `Contents/Helpers/jaw` onto the PATH, so `jaw` and `jawd` come from one build and always speak one protocol.
- **Its own folder for development.** The app honours `JAW_HOME`, as the terminal does, and `--demo` runs `jawd demo` in a folder of its own, as `jaw demo` does. A debug build never opens the owner's folder unless it's told to.
- **Deleting the app takes its agent with it.** The terminal's `jaw agent install` puts one back.

### Beside the terminal

Both are interfaces on the same socket, and everything they share is jawd's:

- **Drafts are jawd's,** per chat, for every interface. The app takes a draft from elsewhere into its box only if the box hasn't been edited since it last sent its own; otherwise its next `draft.set` wins, as jawd keeps the last.
- **Frecency, read positions, folders and favourites** are jawd's, so the palette ranks alike in both and a chat read in one is read in the other.
- **Voice plays in jawd,** one message at a time on the Mac's speakers, so a voice message started in the terminal shows as playing in the app and can be paused from either.
- **Focus.** You count as looking while either has focus, so presence and notifications follow whichever you're in.
- **The protocol version guards the pair.** jawd serves one version; an interface that speaks another is refused with error 6, which the app shows with the two versions and how to match them.

### Notifications

- **jawd decides; the app delivers while it runs** (Owner, 26 Oct). jawd already decides what notifies and when: bursts gathered, one summary after a catch-up, muted chats and the chat you're reading quiet. With proposal P2, the app claims notifications and jawd sends it each one instead of handing it to `osascript`; the app posts it through `UNUserNotificationCenter`, grouped by chat, with Reply and Mark as Read, and a click opens the chat (jaw's Later, "clicking a notification to open its chat"). When the app quits, jawd notifies by itself again, as today.
- **Cleared when read anywhere:** with P2, jawd says when a chat it notified for has been read, in the terminal, the app or the phone, and the app withdraws its notifications for that chat.
- **The app keeps running when its window closes,** as Mail and Messages do. With no window it watches nothing and drops its caches, so it holds the claim in under 25 MB and wakes only for a notification.
- **The Dock badge** is the unread total of chats that aren't muted, from proposal P3 or from `folders.watch`'s All row once jawd serves folders (M6). Until one of them lands, the app shows no badge rather than a count of the chats it happens to hold.

### Focus, reading and presence

- **Focus** is true while the app is active and one of its windows is visible and not minimised, and false when it resigns, the screen locks or the last window closes. jawd says you're online half a second after focus and away 30 s after it goes, so switching apps doesn't flicker.
- **Reading.** A chat counts as read up to the newest message on screen while its window is key and the app active, sent with `read` at most once a second while messages keep coming, as the terminal does. With panes, only the focused pane reads its chat, as only the terminal's pane with the keys does (f4fed0a). Nothing is read in a window that isn't key, so the phone's notifications and unread counts stay right.
- **`chat.opened`** goes when you open a chat yourself (the list, the palette, a favourite), never when a window is watched again after reconnecting, so frecency counts opens.
- **Others' presence.** The roster shows it from Mac-M4, as jawd's M6 serves it. Who's typing in any chat, and a one-to-one chat's online or last seen in its header and the status line, come with P7, in jawd's two halves: typing now, and presence once jawd's M6 is on main; until each comes, the app shows nobody typing and no online or last seen, rather than guessing.

## The socket client

### What a client of jawd does

jaw-client does the connection's half in Rust: threads to read and write; replies and pushes drained a frame at a time with each window's updates merged (`Merged`); reconnecting from 100 ms doubling to 2 s; watching every window again with `have` after a reconnect or a `resync`; the chat list's hold; the latest `find`; and the window models (`Chats`, `Messages`). Beside it, the terminal's `wire.rs` does the interface's half: a key for each message and action, asked again with its key once jawd is back if the answer was lost and less than 20 hours have passed; drafts and read positions owed until jawd answers; the library, folder and roster windows by hand. The Mac app needs all of both.

### Weighed

| | A Swift client | jaw-client through UniFFI |
| --- | --- | --- |
| What's written | the connection and window models (jaw-client's `client.rs` and `window.rs`, 1,649 lines) and their tests (1,004 lines), in Swift; the interface's half either way | wrappers: jaw-client's `Client<C: Row, M: Row>` is generic, which UniFFI can't export, so concrete types, a callback for `wake`, and every row as a UniFFI record; the interface's half either way, since `wire.rs` isn't in jaw-client |
| A row's way to the screen | decoded once, from JSON into the Swift value the view draws | decoded by serde_json into Rust, lowered into UniFFI's buffer, lifted again into Swift: three copies of every row, with nested spans, reactions, media and album items |
| Measured | Foundation's `JSONDecoder` decodes a 100-row window (67 KB) in 1.43 ms p50 and 2.46 ms p99, 68,000 rows a second on one core, so 2,000 messages a second cost 2.9% of a core, off the main thread (load 19 to 28) | not measured: it adds lowering and lifting to serde_json's decode |
| Build | Xcode and Swift alone; cargo and go build the helpers, outside the app's process | a Rust static library for each architecture built inside the Xcode build, generated bindings to keep clean under Swift 6's strict concurrency |
| Debugging and profiling | one language in Instruments | a foreign boundary in every stack |
| Keeping in step with jawd | Swift types checked against every file in `protocol/fixtures` and against `jawd demo`; a JSON Schema from jaw-proto (P6) to check them by | compiled from jaw-proto, so in step by construction |

The protocol was made for this: jaw's design says interfaces in other languages get a JSON Schema generated from jaw-proto, and the wire is JSON lines. UniFFI's one gain, staying in step by construction, the fixtures and a live suite give too. Its costs stay: two more copies of every row, a foreign boundary in every stack, and a Rust build inside Xcode's.

### Decision

- **`JawKit`, a Swift package in `mac/`:** the connection (a POSIX socket read on a thread of its own, lines up to 16 MiB as the protocol allows, writes that give up after 10 s stalled, as jaw-client's do), Codable rows mirroring jaw-proto, the window models ported from `window.rs` (the chat list's hold, the conversation's anchor, `newer` and what arrived since a move), the merge per window between frames, reconnecting as jaw-client does, and the interface's half ported from `wire.rs`: keys made from 128 random bits (`jaw-` and 32 hex digits, as `data.rs` makes them), asking again after a lost answer, owed drafts and reads, the latest `find`.
- **Bounded like jaw-client:** at most 256 requests waiting to be written, and at most 4,096 replies and pushes waiting for the main thread, past which the client stops reading and jawd keeps the window updates in their windows, as it does for any slow interface.
- **Wakes like jaw-client after c4af9c1:** the answer to anything awaited (hello, a window's watch or move landing where it was asked to be, a request) goes to the main thread at once; pushes at most once a display frame, merged.
- **Unknown fields are ignored and an unknown `kind` shows as "jaw can't show this yet",** as the protocol's versioning says.
- **Conformance:** every file in `protocol/fixtures` decodes into JawKit's types, and each request JawKit writes equals its fixture; jaw-client's window tests (a reference model under shuffled and repeated updates) are ported; and a live suite runs against `jawd demo` in a temporary folder, as `crates/jaw-client/tests/jawd.rs` does, killing jawd mid-stream and checking the windows end as a fresh watch would leave them.

## The UI stack

### Weighed

Measured off the main thread on the M1 Ultra at a load of 19 to 28 ("Measurements"):

- **Measuring a message's height at a width costs 57 µs with Core Text, 63 µs with `NSAttributedString.boundingRect` and 97 µs with a TextKit 2 layout manager each.** A 100-message window is 5 to 8 ms, most of a 120 Hz frame. So heights must be measured off the main thread and kept per message, version and width, as the terminal keeps its layout.
- **AppKit's `NSTableView` takes heights from its delegate and reuses its cells,** so it draws from a cache built on another thread and never measures on the main thread. SwiftUI's `List` and `LazyVStack` lay out each row's views, text included, on the main thread as the row comes into view, and a lazy stack estimates the heights of rows it hasn't laid out yet. They can be given heights, but they still lay their text out again on the main thread, and an estimate that proves wrong moves the scroll position when history arrives above, which "nothing jumps" forbids.
- **Anchoring on a message** needs rows inserted above and the clip view's origin moved by their height in the same transaction. AppKit does that directly; SwiftUI's `scrollPosition(id:)` re-anchors after its own layout.
- **Text in a message** is drawn from the layout made off the main thread, with selection, links and mentions hit-tested against that same layout. The box needs what `NSTextView` gives: marked text for input methods, Enter told apart from ⇧↩, ↑ from an empty box, completion for `@` and `:`, files and images dropped or pasted, undo and spelling. SwiftUI's `TextEditor` wraps `NSTextView` but exposes little of it.
- **Decoding a 768-pixel preview with ImageIO costs 1.74 ms, a 256-pixel one 0.24 ms.** A screen of photos is 10 to 20 ms of decoding, so it runs on other threads, and the conversation's cells take a bitmap that's ready.
- **Redlamp, a fast native Mac app by the same owner, already splits it this way:** AppKit outline views for its sidebar and a collection view for its filmstrip and import grid; SwiftUI for its panels and its command palette; the main thread's p99 held under 8.3 ms by `MainThreadMonitor`.
- **SwiftUI earns its place elsewhere:** settings, sheets, the palette (at most 40 rows a keystroke), the linking screen, empty states, the viewer's chrome and theme previews, where building fast matters more and its costs don't show.
- **AppKit and SwiftUI together are cheap:** 6.8 MB once both are loaded, 17.3 MB with a window of a 10,000-row table and a SwiftUI view drawn.

The prototype has no window, so as not to put one on the owner's screen at a load of 30. It measures the work each row needs, which decides where that work can run; that's the question.

### Decision

- **AppKit:** the window and its split views; the chat list (`NSTableView`, fixed 56-point rows); the conversation (`NSTableView`, one column, heights from the layout cache, cells that draw pre-laid text and ready bitmaps); the sidebar (`NSOutlineView`, as Redlamp's); the library's grid (`NSCollectionView` with prefetching); the box (an `NSTextView` on TextKit 2).
- **SwiftUI,** hosted in AppKit: the palette (in a panel of its own), settings, sheets (message info, chat info, forwarding), the linking screen, empty states, the viewer's chrome, the status line. SwiftUI never sits inside a list's cell.

### Threads and the frame

- **Main:** AppKit's events, then at each display frame what's merged since the last, applied for at most 2 ms (the rest waits for the next frame, input still first), then drawing. The display link (`NSView.displayLink`) runs only while something waits to be applied or animates, and pauses otherwise.
- **Reading:** a thread reads the socket and splits lines; a serial queue decodes them; window updates merge per window under a lock until the main thread takes them.
- **Layout:** a concurrent queue measures and lays out messages for the widths on screen, newest first, the visible ones before the rest.
- **Pictures:** a queue of up to four decodes previews into bitmaps, each cancelled when its cell leaves the screen before it's done.
- **Opening a chat:** `messages.watch` around the first unread or the newest; when the rows come (jawd pages 100 in 2 ms on the socket), the visible ones are laid out first (about 20 at 57 µs) and drawn, the rest after; 30 ms in all with room to spare.

### The layout cache

- **Per message, version, width and look,** as the terminal's is per message, version and width: the row's height and its text laid out (Core Text frames for each span run, the positions of links and mentions). A message's `v` changes when anything it shows changes, so an edit, a reaction or a sender's new name lays it out again, as does a change of look or density, and nothing else does.
- **A picture's size comes from the row,** before any pixel: `media.w` and `media.h` give a photo's shape inside its limits, an album's layout is fixed by how many items it has, and a sticker, a video note and a voice message have fixed sizes.
- **Resizing a window** lays out the visible messages at the new width as it goes (20 messages, about 1 ms) and the rest when the resize ends.
- **Bounded:** each pane keeps layouts for the rows its window holds, at most 200, so the cache can't outgrow the windows.

### Pictures

- **The right preview for the pixels drawn:** jawd's 256-pixel preview for anything drawn at 256 pixels or fewer (the chat list's avatars, the library's tiles, album tiles in a narrow window), the 768-pixel one up to 768, and the file itself, decoded at the screen's size with ImageIO's thumbnail API, only in the viewer. Nothing decodes a full-size photo to draw a list.
- **Never blank:** the `thumbnail` jawd draws from the tiny copy in the message shows, blurred, until the preview is decoded, then fades into it in 120 ms. The place doesn't change.
- **The decoded pictures are cached by bytes, 64 MB at most,** least recently drawn first out; the files stay in jawd's cache.

## The window

One window by default, of four columns: the sidebar, the chat list, the conversation and, when split, more conversations. The status line runs along the bottom of the whole window.

- **The sidebar** (220 points, ⌘B): the folder tree with unread counts (All, Unread, Groups, Communities and Archived, the smart folders, then yours), favourites marked with their ⌘ digit, and the roster below it, online first, with presence. It sits on the system's sidebar glass, tinted by the theme. Until jawd serves folders (M6), it shows All and Archived, as jawd serves them now.
- **The chat list** (300 points, 260 to 420): a folder's chats, pinned first, then newest. Each 56-point row: the avatar; the name, bold when unread; the time; the newest message's line (who wrote it in a group, `Draft:` and the draft in the accent, "typing…" in green, from P7); the unread count, an @ for a mention, the muted and pinned marks, your last message's ticks. A community is one row that opens to its groups, the announcement group first. The list holds its order while the pointer is over it or for 1.5 s after a key moves through it, so nothing moves under the pointer before a click; moves then animate to their places.
- **The conversation:** a header (avatar, name, who's typing and online or last seen, both from P7, members for a group; find in chat, info, close for a pane), the messages in the look picked in Settings, bubbles or lines (see "Two looks"), a "2 new ↓" pill when new messages come below while you read above, and the box.
- **The box** (one per conversation, Owner, 26 Oct): an `NSTextView` growing to eight lines, then scrolling, its placeholder naming the chat it sends to ("to Lisbon Climbing"), as the spike's border does; above it, the message you're replying to or editing, with Esc to drop it. Enter sends, ⇧↩ or ⌥↩ adds a line, `@` completes the group's members from `chat.info`, `:` completes emoji, ⌘O attaches, files dropped or pasted attach, ⌘R records.
- **The toolbar** (the window's, unified): the sidebar button; jawd's state when it's anything but online ("Catching up 1,204 of 5,000"); the palette as a field that opens it; split, library and new chat.
- **Folding,** as the terminal folds: below 960 points the sidebar folds away, and below 740 the chat list does too, opening over the conversation when asked (⌘0 or the toolbar), as the terminal's narrow picking does.
- **Typography:** the system font at 13 points for messages (11 to 17 with ⌘+ and ⌘−), 12-point semibold names, 11-point times and the status line in SF Mono, a nod to the terminal; a setting sets messages in SF Mono too.

## Messages

### Two looks

- **Bubbles or lines, picked in Settings › Messages and in View › Message Style,** for every chat at once (Owner, 26 Oct). Bubbles are the default. Switching lays out the visible messages in the new look first (about 20, 1 ms), then the rest, and keeps the anchored message where it was on the screen, so nothing jumps.
- **Bubbles:** theirs on the left, yours on the right, their width at most 72% of the conversation and 520 points. In a group, the sender's name tops each run in their colour (jawd's `colour` index), with their avatar beside the run's last message. The time and your ticks sit at the end of the last line.
- **Lines, the terminal's layout grown up:** no bubbles; others' messages under their name, which starts each run at the left in their colour; yours from a third of the way across, as the terminal's `layout.rs` indents them; the time at the right edge when the minute or the sender changes, your ticks before it. Day separators and "2 new" are rules across the conversation, as in the terminal.
- **Compact,** for either look: half the space between messages and between runs, and no avatars in bubbles.
- **A run** is one sender's messages within 5 minutes, closer together, in both looks.
- **Every kind has its place in both:** pictures, video and cards keep their sizes, inside the bubble in one look and under the text at its indent in the other, as the terminal puts its pictures; system and call lines are centred in both.

### Text and what's around it

- **Spans** as jawd parses them: bold, italic, strike, `mono` in SF Mono on the surface colour, quotes with a bar, lists with bullets, links in the link colour, mentions marked, and a mention of you on the mention colour.
- **What's said around the text:** the time and, on yours, the ticks (pending, sent, delivered, read and played; failed in the error colour, with Retry), where the look puts them; Edited; Forwarded, or Forwarded many times; a star; a timer for a disappearing message. A deleted message says "This message was deleted", or "You deleted this message", in italics.
- **Quotes:** the quoted message's sender in their colour, its line and a small picture if it has one, at the top of the bubble, or as an indented line with a bar in lines; a click goes to it and flashes it.
- **Reactions** under the message, each emoji with its count, yours outlined; a click toggles yours.
- **Long messages** fold after 40 lines with "Show more", as the terminal's. One to three emoji alone are drawn large, without a bubble.
- **Day separators** float at the top while their day scrolls past. Opening a chat with unread messages shows "2 new" above the first of them.

### Actions

- **From the keys,** as the terminal's: ↑ from an empty box selects the newest message, ↑ and ↓ move, and r replies, e edits, d deletes, + reacts, f forwards, y copies, Enter opens or plays, s saves, i shows the info, Space looks at a photo, video or document, and Esc goes back to typing.
- **From the pointer:** hovering a message shows react, reply and more beside it; a right click has the full menu (Reply, React, Copy, Forward, Edit, Delete for Me or for Everyone, Star, Info, Save, Show in Finder, Open With). A two-finger swipe right on a message replies, as the phone's swipe does.
- **Each asks jawd and shows what jawd pushes back:** `send.text` with `reply` and `mentions`, `react`, `edit` (your text within 15 minutes; the box fills, its label says Editing, Enter saves), `delete` (for me, or for everyone within 60 hours, from a two-choice sheet), `forward` (the palette's forward mode, up to five chats), `star`, `message.info` (a sheet of each recipient's delivered, read and played times), `chat.info` (members, admins, the disappearing timer). What jawd refuses is said where you acted, in its words, as the terminal says it.

### Every inline kind

All of it from jawd's rows, previews and files, as the terminal draws it. `state` on a media row decides what's drawn while the file isn't here: `remote` (the preview, and the size to fetch), `downloading` (a ring with `progress`), `local`, `expired` and `gone` (the preview, and `error` in jawd's words, with "Ask the phone again"), `waiting_for_phone` ("Waiting for your phone").

- **Photos:** the original's shape inside 320 by 400 points (120 at least), from the 768-pixel preview (the 256 when it's drawn smaller), the blurred thumbnail first; HD on a photo with an HD copy, whose full size the viewer fetches (`media.fetch` with `variant` `hd`). Space or a click opens the viewer.
- **Albums: the terminal's two across, grown up.** Two items side by side; three, two across and one wide below; four, two by two; five or more, two by two with "+3" on the fourth. Tiles are filled and cropped from the centre, 2 points apart, inside a 320-point bubble whose height the item count fixes; the caption is the album's spans. A click opens the viewer at that item, paging through the album.
- **Stickers:** 160 points, no bubble, from the transparent preview. An animated sticker (animated WebP, through ImageIO's frames) plays while it's on screen and the window is key, at most three times through; one whose format the app can't animate shows its still preview. Knowing which are animated before the file is here is proposal P4; without it, the app looks at the file once it's downloaded.
- **GIFs** (WhatsApp sends them as looping MP4): a muted looping `AVPlayerLayer` on the file, playing while on screen and the window key, marked GIF. With Reduce Motion they wait for a click.
- **Videos:** the poster jawd makes, with the length and the size; a click fetches a video that isn't here (videos above 16 MB wait to be asked, as jawd downloads by itself only below that) with a ring on the poster, then plays it in its place, sound on, one at a time, with the system's inline controls; double-click or ⌘↩ opens the viewer. jawd puts a file in place only once it's whole and its hash checked, so nothing plays while it downloads.
- **Video messages:** a 192-point circle from the poster, a click plays it in place with sound and a ring that fills as it plays.
- **Voice messages:** play, the 64-point waveform drawn as bars with the played part in the accent, the length or where it's got, the speed (1×, 1.5×, 2×), and a dot until you've played it. Playing is jawd's (`voice.play`, `voice.pause`, `voice.seek`, `voice.speed`), so the terminal and the app share it; the playhead moves between jawd's ten `playback` pushes a second at the display's rate, and only while playing. A click on the waveform seeks; one ending plays the next one you haven't heard, as the phone does; while one plays, the status line holds a small player, so it can be paused from any chat. Enter plays the selected one, ← and → seek 5 s, x changes the speed, as in the terminal.
- **Audio files** (not voice) play in the app with AVFoundation from the file, as videos do, since jawd plays Ogg Opus voice messages.
- **Sending media** (jawd's M4): files dropped, pasted or attached with ⌘O gather above the box as a strip of previews, each with its caption, and SD or HD for the lot; Enter sends them with `send.media`, and jawd plans, transcodes and uploads each, its progress coming as `job` pushes and the message's rows. Photos need nothing more; video, GIFs and audio need ffmpeg in jawd, Homebrew's (Owner, 26 Oct), and the app says when it's missing.
- **Recording** (⌘R in the box): `voice.record`, the box becomes a strip of jawd's levels with the time; Enter sends (`voice.send`), ⌘R stops it to listen first (`voice.stop`, then `voice.play` with `recording`), Esc throws it away (`voice.cancel`). The microphone is jawd's, which macOS asks about once.
- **Documents:** the file's icon for its type, its name, type, size and pages, and its first page once it's here (Quick Look's thumbnails); Space or a click looks at it with Quick Look, ⌘↩ opens it in its app, and it drags out to the Finder, by the file once it's here, by `media.save` into the drop's folder before.
- **Link previews:** the 140-pixel thumbnail jawd keeps, the title, two lines of the description and the host, under the text; a click opens the link in the browser.
- **Locations:** a 280-by-140-point map with a pin, drawn by MapKit's snapshotter from the coordinates, and the place's name and address; a click opens Maps. A live location says until when and when it last moved, its map drawn again when its row changes. MapKit fetches its map from Apple, so a setting turns maps off, leaving the name, the coordinates and "Open in Maps" (Owner, 26 Oct).
- **Contact cards:** the name, the numbers, Message (`chat.start` with the number) and Add to Contacts; several contacts open a sheet of them.
- **Polls:** the question, each option with its bar and count and your choice ticked, how many voted, single or multiple choice; the bars grow to new tallies as rows come. A click votes with `poll.vote` once jawd serves it; until then the poll is shown and says to vote on the phone.
- **Events:** a card with what jawd says of it: the name, when, where.
- **System and call lines:** one sentence in the middle, small ("Rui added Sofia", "Missed voice call").
- **View-once:** "Photo, view once" and "open it on your phone", since jawd never keeps the file.
- **Anything else:** "jaw can't show this yet", as the protocol has it.

### The media viewer

- **It opens from the picture,** growing from the bubble's frame to the window in 220 ms, and closes back into it (with Reduce Motion it fades). It covers the window, not the screen, unless you make it full screen.
- **A photo:** the 768-pixel preview at once, then the file decoded at the window's size; pinch, double-click and ⌘+ zoom, dragging pans.
- **An album:** "2 of 5", ← and → or a swipe page through it, and a strip of its items along the bottom.
- **A video:** played by `AVPlayerView` with the system's controls, from the file.
- **Around it:** who sent it, when, and the caption; Save (`media.save`, to Downloads by default), Share, Forward, Copy, Open in Preview, and Go to Message. Esc closes it.

## The palette

jawd's `find` behind one line, as the terminal's (Ctrl-K there, ⌘K here):

- **Each keystroke asks `find` again** and keeps only the latest answer; jawd cancels the one before (error 7), so typing never queues searches. Names come from jawd's index in memory within a few milliseconds, and messages, links, media and files after them in the same answer.
- **Results grouped by kind, best first,** with the strip of `counts` above them (Chats 1 · People 2 · Messages 214 · …), the matched letters marked from jawd's `mark` spans. Nothing typed shows the chats you open most.
- **The app's own actions are matched here as jawd matches names and merged in,** each with its key, so the palette teaches them: start a chat, go to unread, search messages, search here, mute and for how long, archive, pin, mark unread, record, attach, react, delete, forward, file into folders, new folder, rename, move, the sidebar, the library, split across, split down, close the pane, focus, the glance, the theme, link a device, settings, the keys.
- **Keys:** ↑ and ↓ move, Tab narrows to one kind (and steps through them), Enter does the obvious thing (opens a chat, starts one with a person, goes to a message and flashes it, opens a file or a link), ⌘↩ opens the chat in a new pane beside the focused one, Esc closes.
- **Modes,** as the terminal's: a reaction (the six quick ones, or any emoji by name), delete (for me or for everyone, saying why not when it can't be), forward (chats picked with Tab, five at most), filing a chat or a person into folders (ticks, and a new folder made on the way), a folder's name, where to move a folder.
- **It's an `NSPanel` hosting SwiftUI,** at the top of the window, 620 points wide, rows of 32 points, as Redlamp's. It opens in the frame after ⌘K with its field already taking keys, so nothing typed straight after ⌘K is lost to the animation. It can take a theme of its own, as Redlamp's can.

## Split panels

The terminal's panes, as tmux has them, which the terminal has had behind its `panes` switch since f4fed0a (Ctrl-P and a key, six panes at most); the Mac's follow the same rules with the Mac's keys:

- **Split the focused pane across (⌘D) or down (⌘⇧D)**; the new pane takes the next chat in the list that no pane shows, as the terminal's does. Each pane shows one chat, with its own header and its own box, whose draft is jawd's for that chat, so moving between panes loses nothing typed.
- **Moving:** ⌘⌥ and an arrow move to the pane that way, ⌥⌘1 to ⌥⌘9 go straight to a pane, ⌘W closes the focused pane (the last one stays). ⌘⇧ and an arrow swap the focused pane with its neighbour.
- **Choosing a pane's chat:** pick it in the list (it opens in the focused pane), ⌘↩ in the palette (a new pane), or drag a chat from the list onto a pane: onto an edge to split there, onto the middle to replace it. The edges show as you drag.
- **A chat shows in one pane at most:** picking one that another pane shows goes to that pane, and ⌥↑ and ⌥↓ pass over chats other panes show, as in the terminal.
- **The focused pane** has the accent's thin outline and takes the keys; its chat is the one selected in the list.
- **The layout is a tree of splits with their ratios,** kept by jawd as this interface's saved state (proposal P1), so the app opens where you left off. P1 is being built now, on `interface-state`, for the terminal and the Mac alike, as the owner asked on 26 October; until it lands, the app keeps the layout in its preferences. Each interface keeps its own layout, and either can open the other's (Owner, 26 Oct).
- **Limits:** each pane is a `messages.watch` window, and jawd allows 32 windows a connection, so the app allows 12 panes, leaving room for the chat list, the folders, the roster, the library and the glance.
- **Only the focused pane reads its chat,** as only the terminal's pane with the keys does; the others show what arrives, with the "new" marker, until you go to them. Every pane's chat stays quiet in notifications while the window is in front, as jawd's attention treats every messages window.

## The status line, focus and the glance

The spike's three ideas, from the terminal's experiments (`JAW_EXPERIMENTS`, on main since f499679):

- **The status line** along the bottom of the window, 24 points, in SF Mono: on the left, jawd's state when it's not online and how the focused chat is (typing and online or last seen, both from P7, and muted), without its name, which the focused pane's box and header already say, as the owner asked of the terminal's once its box named the chat (3d1da48); on the right, what needs you in the other chats, mentions and people by name ("@ Família Gomes", "Ana · 2") and groups counted ("3 more chats · 12 unread"), the count shortening before a name goes and leaving out chats a pane shows, as the terminal's does with panes, then "⌘J glance". A name is a link to its chat; the count opens the glance. It reads the chat list's and the roster's windows the app already watches and asks jawd for nothing, as the spike's does. Its left half comes in Mac-M1, since jawd's states are said there; its right half comes with focus mode and the glance in Mac-M4.
- **Focus mode** (⌘⇧↩, as iTerm2 keeps a pane alone): the sidebar and the chat list fold away for the focused pane, the status line the only glimpse of the rest; ⌥↑ and ⌥↓ still move between chats, and ⌘⇧↩ or ⌘B brings the rest back.
- **The glance** (⌘J, the status line, or the palette): what the spike calls the quick look, renamed so it can't be taken for macOS's Quick Look of files. A popover over the status line with the latest unread messages of the chats that need you, three of each of at most four, from windows watched only while it's open; ↑ and ↓ move, Enter goes to the chat, ⌘↩ opens it in a new pane, Esc closes; nothing is marked read.
- **The box stays in each conversation,** naming its chat, with the status line across the window (Owner, 26 Oct); the spike's full-width box isn't taken.

## Folders and the roster

As jawd serves them in M6 (`folders.watch`, `roster.watch`, `folder.*`), and as the terminal's sidebar draws them. Brought forward from Mac-M4 on 27 October, with right-click menus on the chat list, the panes and the sidebar: [the menus and folders design](2026-10-27-jaw-mac-menus-and-folders-design.md) has them, and settles filing from a chat's menu (Add to Folder, Move to, Remove from), a folder's menu, naming in place and dragging.

- **One tree:** the built-in folders, the smart ones (Waiting on me, Mentions me, Muted and unread), then yours, nested, each with its unread total and mentions, each chat counted once. Picking one makes the chat list that folder's window.
- **Filing:** drag a chat or a person onto a folder (`folder.add`), or m and the palette's filing mode; a chat can sit in several. Folders are made, renamed, nested and deleted from the sidebar's menu and the palette.
- **Favourites:** ⌘1 to ⌘9 go to up to nine folders or chats (`favourite.set`), shown beside their rows.
- **The roster:** the folder's people, online first, then by last seen, then by name, with their presence (online, typing, recording, last seen, or nothing when they hide it, faint when jawd isn't following them now), their about text and avatar; Enter or a double click opens or starts their chat. Watching it is what tells jawd whose presence to follow, within its limits.

## The library

`library.watch` (jawd's M3), as the terminal's library:

- **It opens in a pane, as a chat does** (Owner, 26 Oct): ⌘L for every chat, or from a chat's header for that chat alone, so it can sit beside the chat it came from.
- **A strip of kinds with their counts:** All, Photos, Videos, Voice, Files, Links. All shows a shelf of each; a kind pages through its window by keyset.
- **Search with tokens,** as the terminal's takes words and `from:`, `in:`, `since:` and `until:`: a person, a chat and dates complete as tokens, and words match captions, file names and link titles.
- **Photos and videos as a grid** of 256-pixel previews (videos with their length), voice messages as rows with their waveforms, files and links as lists with who sent them, where and when.
- **Enter opens** (the viewer, Quick Look, the browser) and ⌘↩ goes to the message in its chat.

## Search

- **Everywhere:** the palette; ⌘⇧F opens it narrowed to messages.
- **In a chat:** ⌘F opens a field in the pane's header: `find` with `chat` and `kinds` messages, the matches listed newest first, ↑ and ↓ (or ⌘G and ⌘⇧G) going to each, the window moved around it (`watch.move`) and the words marked in the message.

## Drafts, reading and typing

- **Drafts** go to jawd (`draft.set`) once typing has paused for 1.5 s and when you leave the chat, as the terminal's do, and come back with the chat's row; the chat list shows them.
- **Typing** (`typing` `composing`) is said when typing starts and again every 5 s while it goes on, `paused` when the box empties or you leave; jawd paces what WhatsApp hears.
- **Read receipts** as "Focus, reading and presence" says.

## Linking and the states

- **Linking** (`link.start`): the Mimic beside a QR code drawn crisp from jawd's `link` push, with the three steps on the phone (Settings, Linked devices, Link a device). The code is drawn again, crossfading, when jawd sends a new one; "Use a phone number instead" asks the number and shows the code to type on the phone. Once jawd's `link` push says `done`, the Mimic chomps, then "Catching up 1,204 of 5,000" with its progress, and the chat list fills in behind as jawd stores history, since jawd serves what it has at once.
- **Starting jawd:** the window as it was, the status line saying "Starting jawd", for the moment the agent takes the first time.
- **jawd unreachable:** the conversation area shows the Mimic asleep (grey, eyes shut, as `art.rs` draws it) and why, from jaw-client's words ("jawd isn't running: nothing answers on …"), trying again from 100 ms to every 2 s; everything already drawn stays.
- **Connecting, catching up, phone offline, offline, engine restarting:** said in the status line and the toolbar, everything local still working, sends waiting with their clocks.
- **Logged out:** the linking screen, saying it was logged out and why, as jawd says it.
- **Update needed, banned until:** a bar across the top, in jawd's words.
- **Empty:** no chats yet (the Mimic, and "⌘N to start one"); no chat open (the Mimic, and "⌘K to find one"); an empty chat (the Mimic sits in it and chomps when the first message comes, as the terminal's does); an empty folder (drag chats here); nothing of a kind in the library, or nothing found.
- **Loading:** a window's rows come in a few milliseconds, so nothing shows while they come; only past 150 ms does a pane show its header and a quiet placeholder. Asking the phone for older messages (`history.more`) puts one row at the top saying so.

## Themes

### Redlamp's catalog

- **Redlamp keeps themes as families,** each a matched dark and light pair, so a choice survives an appearance change. A family is either authored directly or ten colours (`background`, `card`, `foreground`, `muted`, `primary`, `accent`, `border`, `success`, `destructive`, `neutral`, and `primaryRoles`, which keeps a brand colour to the places it's allowed), mapped to tokens. Its catalog is Neutral, Redlamp, and twenty ported from omoi-station: Kanagawa, Tokyo Night, Moonlight, Catppuccin, Rosé Pine, Nord, Everforest, Gruvbox, Dracula, Solarized, One Dark, Monokai, Ocean, Sakura, Copper, Vinyl Noir, Neon Dusk, Burnt Tape, Phantom Frequency and Velvet Static. Around them: a tint from 0 (the theme's tones in grey) to 1, native controls in the theme's accent or the system's, the panels' translucency (solid under Reduce Transparency), and the palette in a theme of its own.
- **Its tokens are a photo editor's** (labels, tracks, thumbs, wells, an edited dot), so jaw maps the ten colours to a chat's roles itself, below.
- **Copied, not linked** (Owner, 26 Oct). `RedlampDesign` has no `Package.swift` (it's a Tuist framework target), depends on `RedlampEngineAPI` through its sliders and `TrackGradients.swift`, and is mostly controls jaw doesn't use. Linking it would mean a package carved out of it in Redlamp's repository and jaw's builds reaching a private one. jaw copies what it uses, about 300 lines: `RGBA` with its HSL parser and mixing, `ThemePalette`, `ThemeFamily`, `PrimaryRoles` and the catalog's data, in the same slot order. `scripts/sync-themes` compares jaw's copy with `ThemeCatalog.swift` in `~/src/darkroom` and says what differs, as `scripts/sync-protos` does for whatsmeow. Redlamp's files are under the MPL 2.0; the copied ones keep their notice, or the owner, who holds both, relicenses them for jaw.

### jaw's own theme

- **jaw, first in the list and the default, following the system's light and dark** (Owner, 26 Oct): ooze's Tokyo Night, the terminal's palette, authored directly, since it has more than ten colours: the ground `#1a1b26`, the surface `#24283b`, the selection `#283457`, text `#c0caf5` and `#a9b1d6`, the quieter `#737aa2` and `#565f89`, borders `#3b4261`, and its eight accents, which are also the eight group senders' colours. Its light half is Tokyo Night Day, Tokyo Night's own light palette, taken from its source when the family is added. Redlamp's Tokyo Night stays beside it as it is, a lighter take.

### The roles

Every colour the app draws is asked for by what it's for, as `theme.rs` asks, never by hue:

| Role | jaw's (ooze's Tokyo Night) | A Redlamp family |
| --- | --- | --- |
| The window's ground | bg `#1a1b26` | `background` |
| The sidebar, the list, their bubbles | surface `#24283b` | `card` |
| Text | fg `#c0caf5` | `foreground` |
| Secondary text: previews, times | muted `#737aa2` | `muted` |
| Quieter text, ticks not yet read | comment `#565f89` | `muted` at 62%, Redlamp's tertiary |
| Separators | gutter `#3b4261` | `border` |
| Focus, selection, links, read ticks, unread counts | blue `#7aa2f7`, selection `#283457` | `primary`, or `neutral` where `primaryRoles` keeps it out |
| Your bubbles | blue at 18% over the ground | `primary` at 18% over `background` (12% in light) |
| Mentions, the "new" marker | orange `#ff9e64` | `accent` |
| Online, typing | green `#9ece6a` | `success` |
| Failed, destructive | red `#f7768e` | `destructive` |
| Group senders | red, yellow, blue, purple, cyan, green, orange, teal | eight hues 45° apart from `primary`'s, each lightened or darkened until it reads at 4.5:1 on their bubble |

- **Tint, appearance and translucency** as Redlamp's: the tint pulls every role toward the grey of its own lightness; appearance is dark, light or the system's; the sidebar's glass shows through by the translucency setting, solid under Reduce Transparency. Increase Contrast draws borders and secondary text a step stronger.
- **Switching a theme is live:** the roles are read as views draw, and every window redraws on the change, with no relaunch.
- **The Settings' Appearance pane** shows each family as a small chat drawn in its roles (a list row, their bubble, yours, a mention, a sender's name), as Redlamp's gallery shows each theme as a small panel.

## Animation

Each one says something, takes at most 220 ms, runs on the render server, finishes the moment a key comes, and has its Reduce Motion form:

| Where | What | Why | Reduce Motion |
| --- | --- | --- | --- |
| A message arriving at the bottom | rises 6 points and fades in, 160 ms | says it's new without moving what you read | fades in |
| Your message's ticks | the tick crossfades as it advances, 120 ms | shows the step from sent to read | none |
| The chat list's order | a row moves to its new place, 200 ms, only once the hold ends | the list never moves under the pointer | rows swap without moving |
| A preview arriving | the blurred thumbnail fades into it, 120 ms | no flash of grey, no shift | the same fade, shorter |
| Reactions | the pill pops from 0.8 to 1, 140 ms | marks what changed | none |
| Poll tallies | bars grow to new widths, 200 ms | tallies read as moving | none |
| Typing | three dots breathing, only while on screen and the window key | a live state, not an event | dots shown still |
| The palette | fades in, scaling from 0.98, 120 ms, with its field taking keys before it starts | arrives without hiding keys | fades in |
| Panes and folding | widths ease to their new sizes, 180 ms | shows where a pane came from and went | instant |
| The viewer | grows from the bubble and shrinks back, 220 ms | keeps your place in the conversation | fades |
| Voice | the playhead moves at the display's rate, only while playing | follows the sound | the playhead steps with jawd's pushes |
| The Mimic | chomps when a message comes into an empty chat and when linking is done; sleeps when jawd is out of reach; pixel frames stepped at 10 a second, never smoothed | jaw's creature, as in the terminal | still, open or asleep |

Nothing animates when nothing happens: no idle breathing, no shimmer, no loops off screen.

## Branding

- **The app icon:** the Mimic without its sparkles, 32 by 26 pixels, at the largest whole multiple within four fifths of the icon's rounded square (20× at 1,024 pixels), centred on Tokyo Night's ground, its edges crisp, built with Icon Composer in macOS 26's layers (the ground, then the Mimic), with its dark, clear and tinted forms; the smallest sizes use a 16-pixel redraw, as the poster's header does.
- **Where the brand lives,** as Redlamp keeps its brand out of editing surfaces: the icon, the linking screen, empty states, the asleep state, About and the README. The conversation and the lists are the theme's.
- **The Mimic in the app** is `creatures/mimic.png` drawn with nearest-neighbour scaling at whole multiples (4×, 180 by 104 points, in empty states and linking), with `art.rs`'s two looks: asleep (grey, eyes shut) and mid-chomp (the lid, eyes and upper teeth dropped 3 pixels).
- **The poster's rainbow rule** (green, yellow, orange, red, purple, blue), from `marketing/jaw-in-a-minute/jaw-poster.png` on main, runs under the linking screen's heading and across About, nowhere else.
- **Words:** "jaw" in lower case, as everywhere; "isn't affiliated with WhatsApp or Meta" in About and on the linking screen.

## Keys

| Key | Does | The terminal's |
| --- | --- | --- |
| ⌘K | the palette | Ctrl-K, also taken |
| ⌥↑ ⌥↓ | the previous or next chat | Alt-↑ Alt-↓ |
| ⌘1 to ⌘9 | favourites | Alt-1 to Alt-9 (⌥ and a digit type a character on a Mac) |
| ⌘B | the sidebar | Ctrl-B |
| ⌘L | the library | Ctrl-L |
| ⌘N | start a chat | the palette |
| ⌘F, ⌘⇧F | find in this chat, in every chat | `/search` |
| ↑ from an empty box | select the newest message | the same |
| r e d + f y Enter s i m | reply, edit, delete, react, forward, copy, open or play, save, info, file into folders | the same |
| Space | look at the selected photo, video or document | |
| Esc | back to typing; close what's open | the same |
| Enter, ⇧↩ or ⌥↩ | send, a new line | Enter, Alt-Enter or Shift-Enter |
| `@`, `:` | complete names, emoji | the same |
| ⌘O, ⌘R | attach, record | Ctrl-O, Ctrl-R |
| ⌘D, ⌘⇧D | split across, down | Ctrl-P then % or \|, " or - (the `panes` switch) |
| ⌘⌥ arrows, ⌥⌘1 to ⌥⌘9, ⌘⇧ arrows, ⌘W | move between panes, go to a pane, swap, close | Ctrl-P then an arrow, a digit, { or }, x |
| ⌘⇧↩ | focus mode | Ctrl-F in the spike |
| ⌘J | the glance | Ctrl-G in the spike |
| ⌘+, ⌘− | message text size | |
| ⌘/ | the keys | ? |
| ⌘, | settings | |

## Accessibility

- **VoiceOver:** each message is one element read as its sender, text or kind, time and state ("Ana, did you get the photos from Sunday?, 13:58, 2 hearts"); chat rows read their unread and mentions; actions are offered as the element's actions, so the hover buttons have keyboard and VoiceOver equivalents.
- **Every action has a key,** and the palette lists them.
- **Reduce Motion, Reduce Transparency and Increase Contrast** as "Themes" and "Animation" say; text sizes with ⌘+ and ⌘−; group senders' colours checked at 4.5:1.

## Protocol additions

Proposals for jawd's builders. Each is a jawd change; those on the data layer's paths follow "Data integrity" in jaw's design and are audited by the database expert before they land.

| | Proposal | In jawd | Audited | For |
| --- | --- | --- | --- | --- |
| P1 | **An interface's saved state.** `ui.get` (`key`) answers `value`; `ui.set` (`key`, `value`) keeps a JSON object of at most 64 KB for the connection's `client` name and that key, durably before it answers, as jaw's own class (never dropped or rebuilt, backed up daily with folders, drafts and frecency). | a small table, a schema-step migration, two methods | yes: SQL, a migration | split panes and the window's layout reopening where you left them, in the terminal and the Mac alike. Being built now, on `interface-state`, as the owner asked on 26 October; the app uses it from Mac-M4 |
| P2 | **Notifications claimed by an interface.** `notifications.claim` makes jawd send its `notification` pushes (`chat`, `message`, `title`, `subtitle`, `body`, `unread`, `summary`) to the last connection that claimed them instead of Notification Center, and `notification.clear` (`chat`) when a chat it notified for is read anywhere; with no connection claiming, jawd notifies by itself again. | `notify` and `sock.rs`: a notifier that is a connection | yes, by the paths: `sock.rs` is on them, though it writes nothing | Reply, Mark as Read and a click to open the chat (Mac-M5) |
| P3 | **The chat list's totals.** `chats.watch` results and pushes carry the folder's `unread` and `mentions`, each chat counted once, derived as `folders.watch`'s rows will be; or `folders.watch` served for the built-in folders ahead of M6. | the feed and the store: a derived total, kept by triggers or counted in the transaction that changes it | yes | the Dock badge and the status line's count (Mac-M1, shown only once served) |
| P4 | **Animated stickers known.** `media.animated` on stickers, from the message's own flag. | the converter and the store | yes | animated stickers drawn as such before their file is here (Mac-M2) |
| P5 | **jawd's settings over the socket.** `settings.get` and `settings.set` for what's set by environment today: the size below which media downloads by itself (`JAW_MEDIA_AUTO_MB`), the cache's size (`JAW_MEDIA_CACHE_MB`), notifications on or off. | settings kept in `jaw.db` as jaw's own | yes | Settings' Media and Notifications panes (Mac-M5); until then they show the values `status` reports |
| P6 | **What an interface owes, written down, and the schema.** A section of `protocol/README.md` with what `wire.rs` does (keys, asking again after a lost answer within the day, owed drafts and reads, the latest `find`, watching again with `have`), and the JSON Schema jaw's design promises, generated from jaw-proto. | the protocol's text and jaw-proto | no | the Swift client's conformance (Mac-M1) |
| P7 | **Others' typing and presence in chats.** Who's typing or recording in every chat, groups included, and a one-to-one chat's person online or last seen, as jaw's design's "Presence and typing in chats" brings them to every interface, in two halves as the owner decided on 27 October: typing now, and presence once M6 is on main. Both come as the `typing` and `presence` pushes, each watch's answer saying how things are now, as the protocol's section of that name says; chat rows' `typing` goes when typing comes, so `protocol` is 2 from then and the app moves to it with jawd. | the engine's typing for every chat, who's typing kept in memory, whom presence follows, and the feed | yes, by the paths: the feed and presence, though typing writes nothing | the chat list's "typing…", the header's who's typing and online or last seen, the status line's left half (Mac-M1, shown once served) |
| P8 | **Chat actions from jaw.** `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star` served, each change written with its app-state patch in one synced commit and sent through the outbox, as jaw's design's step "Chat actions from jaw" has it, as the owner decided on 27 October. | `send`, the store and the outbox, through the engine's `AppStatePatch` | yes: the store and the outbox | Pin, Mute, Archive and Mark as Read in the right-click menus and the Chat menu, and a message's Star ([the menus and folders design](2026-10-27-jaw-mac-menus-and-folders-design.md)); Mac-M3's palette actions |

Nothing else is needed: several open chats are several `messages.watch` windows, which jawd already allows (32 a connection) and already counts as being read; voice, media, find, the library, folders and the roster are as the protocol has them, or will have them in jawd's milestones.

## Changes outside the app

- **`jaw agent`** recognises the app's agent (the same label, its program inside `jaw.app`) and says so instead of installing a second one; `uninstall` says to use the app. The terminal's change, not the data layer.
- **`make mac`** builds jawd, jaw-engine and jaw for release, then the app, putting the three in `Contents/Helpers` and signing each with the hardened runtime (jawd with the microphone's entitlement and its embedded Info.plist, as the workstream's open item on jawd's identity asks); `make mac-install` and `make perf-mac` beside them.

## Code

```text
mac/
  Project.swift             Tuist, as Redlamp: Swift 6, macOS 26, warnings as errors
  JawKit/                   the client: connection, rows, window models, keys and asking again, focus
  JawThemes/                Redlamp's families (copied), jaw's own, the roles
  JawUI/                    AppKit: chat list, conversation, cells, layout cache, pictures, the box, library grid, sidebar
  JawApp/                   the app: windows, panes, palette, viewer, settings, linking, states, agent, notifications
  JawHarness/               every screen and state from fixtures, as RedlampHarness's gallery, light and dark, in any theme
  Tests/                    fixtures, window models, live on jawd demo, snapshots, the bench
scripts/sync-themes         jaw's copy of Redlamp's catalog against darkroom's
```

## Testing

- **The protocol:** every fixture decodes, and every request JawKit writes equals its fixture.
- **Window models:** jaw-client's tests, ported: a reference model under shuffled and repeated updates; anchors; the hold; what arrived since a move.
- **Live, on `jawd demo`** in a temporary folder, never the owner's: watching, paging at depth, sending with keys, a flood of 2,000 a second, jawd killed mid-stream and the windows checked against a fresh watch.
- **Screens:** the harness draws every state from fixtures (each message kind, each media state, each empty and failing state, the palette's modes, panes) to images at fixed sizes, in both looks, in jaw's theme light and dark and two of Redlamp's, compared with references, as `jaw snapshot --demo` pins the terminal's frames.
- **The bench:** `jaw-mac bench` times launch, opens, keys in a flood, scrolling with photos, memory and idle wake-ups against `jawd demo`, as `jaw bench` does, and `make perf-mac` writes its table.
- **CI:** macOS runners on main and by hand, as jaw's design keeps them, since macOS minutes count ten times.

## Milestones

The order and each milestone's steps, done-when and budgets are in [the Mac plan](2026-10-26-jaw-mac-plan.md).

| | | Needs from jawd |
| --- | --- | --- |
| Mac-M1 | **Text, every day:** the client, the window, the chat list, the conversation as text in both looks, the box, the message actions jawd serves, chats in the palette | P7 for others' typing and presence, typing now and presence once jawd's M6 is on main, each shown once served |
| Mac-M2 | **Every inline kind:** photos, albums, stickers, GIFs, videos, video messages, voice, documents, links, locations, contacts, polls; the viewer; the library | jawd's M3 media in, M2's link previews, M5 voice |
| Mac-M3 | **The palette, whole:** every kind and action, its modes, search in a chat, sending media and voice | jawd's M4 media out, M5 recording |
| Mac-M4 | **Split panels:** panes, focus mode, the glance, folders and the roster | jawd's M6 folders, P1 (being built) |
| Mac-M5 | **Polish, and it's jaw:** Redlamp's themes, the animations, the Mimic, linking, notifications, accessibility, a signed build | P2, P3, P5 |

## The owner's answers

The fourteen questions this design asked, answered on 26 October; each is a decision above, with its reason, and the numbers are the canvas's.

1. **Who runs jawd:** the app's own jawd and jaw-engine, as the one LaunchAgent; the terminal uses it.
2. **The socket client:** Swift.
3. **The message box:** one per conversation, naming its chat, with the status line across the window.
4. **Notifications:** the app's while it runs (P2), staying open with its window closed; jawd's own once it's quit.
5. **How messages look:** both, bubbles (the default, yours on the right, with a compact setting) and lines like the terminal's, picked in Settings. It differs from the recommendation, which was bubbles alone with a compact setting; both ship in Mac-M1.
6. **The split layout:** one per interface, each able to open the other's.
7. **The library:** in a pane, like a chat.
8. **Keys:** the Mac's, with the terminal's where they don't clash.
9. **Redlamp's themes:** copied into jaw, with a check that keeps them in step.
10. **The default theme:** jaw's Tokyo Night, following the system's light and dark.
11. **The code:** jaw's repository, under `mac/`.
12. **macOS and signing:** macOS 26 and later, the owner's Developer ID, notarized.
13. **Maps:** MapKit's, with a setting to turn them off.
14. **ffmpeg:** Homebrew's, as jawd uses today.

He also asked for P1, an interface's saved state, to be built now, for the terminal's panes and the Mac's alike; a builder is on it, on `interface-state`.

## Later

- A chat in a window of its own; a menu bar item with the unread count.
- One message to several chats (jaw's Later) as a palette action with its multi-select.
- The catch-up and asking (jaw's M9) as a panel: `catch_up.watch` and `ask` are windows and answers like any other.
- Shortcuts actions, widgets, sending stickers, group admin, several numbers, transcription.
- Video that plays while it downloads, if jawd ever puts a file in place before it's whole, which it won't while it checks the hash first.

## Risks

- **macOS asks about the microphone for jawd, inside the bundle,** by its own signature: whether the prompt names jaw or jawd is checked when voice recording lands, and jawd becomes a helper bundle with the app's name and icon if it says jawd.
- **The protocol is a draft;** two clients now follow it. The fixtures, P6's schema and the live suite catch a change before it ships.
- **A terminal built from main beside an app built from a release** can speak different protocols: one bundle avoids it, and error 6 says it plainly.
- **SwiftUI inside AppKit** has costs at the seams; it's kept out of every list's cells.
- **Two looks double the screens to keep right:** the harness draws every state in both, and the bench measures both.
- **120 Hz isn't the owner's display;** the budget is held as work per frame and checked on a ProMotion MacBook.
- **WhatsApp's terms** don't allow unofficial clients, as for jaw.

## Measurements

The prototype is [`jaw-mac-prototype/`](jaw-mac-prototype/): `bench.swift` and `footprint.swift`, built with `swiftc -O -parse-as-library` (Swift 6.3.3) and run on the Mac Studio (Apple M1 Ultra, 20 cores, 128 GB, macOS 26.6.2) on 26 October 2026, while other builders kept the load average at 19 to 28 (34 for the footprint). It opens no window and no socket.

| What | Measured |
| --- | --- |
| A 100-row `messages.watch` result (67 KB), decoded by `JSONDecoder` | 1.43 ms p50, 2.46 ms p99, 4.06 ms at worst, over 1,000 |
| A 20-row push (13 KB) | 0.29 ms p50, 0.42 ms p99, over 4,000 |
| One row | 0.022 ms p50, 0.036 ms p99 |
| Rows a second on one core | 68,201, so 2,000 a second cost 2.9% of a core |
| A message's height at 480 points, 13-point system font, 85 messages with text | Core Text's framesetter 4.84 ms p50 (57 µs each), 6.69 ms p99; `boundingRect` 5.34 ms p50 (63 µs), 7.05 ms p99, one pass at 105 ms; TextKit 2, a layout manager each, 8.21 ms p50 (97 µs), 9.68 ms p99; over 300 |
| A 256-pixel preview (15 KB JPEG) decoded by ImageIO into a ready bitmap | 0.235 ms p50, 0.43 ms p99, over 300 |
| A 768-pixel preview (130 KB JPEG) | 1.74 ms p50, 1.98 ms p99 |
| `phys_footprint` | 1.8 MB at start; 6.8 MB with AppKit and SwiftUI loaded; 17.3 MB with a 1,200-by-800 window of a 10,000-row table scrolled to its middle and a SwiftUI view, drawn off screen |

The rows follow the protocol's message rows, mixed as a group chat's are: text of 4 to 63 words with emoji (which costs Core Text a font fallback), photos with both previews, albums of four, voice messages with 64-point waveforms, stickers, replies and reactions.
