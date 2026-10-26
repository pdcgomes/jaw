# jaw for the Mac: the plan

26 October 2026. The build order for [the Mac design](2026-10-26-jaw-mac-design.md): the shortest path to an app the owner uses every day (reading and sending text in a fast chat list and conversation), then every inline kind, the palette, split panels, and polish. Each milestone says what it needs from jawd, its steps in order, when it's done, and its budgets. The owner answered the design's questions on 26 October; the decisions the plan rests on come first.

- **Every milestone closes with `make perf-mac`** against `jawd demo`'s million messages, its table in the README beside the terminal's, with the load average beside each number, as `make perf` does.
- **Nothing here touches jawd's data layer.** The proposals the app needs (P1 to P6 in the design) are jawd changes, built on jawd's branches by jawd's builders and audited by the database expert before they land; a milestone that needs one says what it does until it lands.
- **The owner's account is never used for tests.** Tests and benches run on `jawd demo` in a folder of their own; the owner tries each milestone's build on his own account once it's done.

## Decided (Owner, 26 Oct)

Each with its reason; the design has the detail.

- **jawd and jaw-engine ship inside the app, as the one LaunchAgent, and the terminal uses it:** one build of the three speaks one protocol, jawd runs at login for notifications, and `jaw.lock` with the shared label makes two jawds impossible. Mac-M1 step 3.
- **The client is Swift:** each row decoded once, no foreign boundary or Rust build inside Xcode's, and the interface's half is Swift either way. Mac-M1 step 2.
- **One box per conversation, naming its chat, with the status line across the window:** with panes, no doubt where a message goes. Mac-M1 steps 4 and 6.
- **Two looks, bubbles and lines, picked in Settings:** bubbles the default, lines the terminal's layout, either compact. Both ship in Mac-M1, so the layout and every kind of message are built for both from the start rather than one being added once media is in. Mac-M1 step 5, and every milestone after.
- **The code lives in jaw's repository, under `mac/`:** a protocol change lands with both its clients. Mac-M1 step 1.
- **macOS 26 and later, the owner's Developer ID, notarized:** the newest APIs without fallbacks, and a stable identity so macOS asks for the microphone once. Mac-M1 step 1, Mac-M5 step 8.
- **Notifications are the app's while it runs, jawd's own once it's quit:** Reply, Mark as Read and a click that opens the chat. Mac-M5 step 5.
- **One split layout per interface, each able to open the other's:** a terminal and a Mac window want different panes. Mac-M4 step 2.
- **The library opens in a pane, like a chat:** beside the chat it came from. Mac-M2 step 7.
- **Mac keys, with the terminal's where they don't clash:** ⌥ and a digit type characters on a Mac. Every milestone.
- **Redlamp's themes are copied, with a check that keeps them in step:** jaw needs the families, not Redlamp's controls. Mac-M5 step 1.
- **jaw's Tokyo Night by default, following the system's appearance:** jaw from the first launch, and a light Mac respected. Mac-M1 step 11.
- **MapKit's maps, with a setting to turn them off:** maps read at a glance, but their tiles come from Apple. Mac-M2 step 4.
- **Homebrew's ffmpeg:** on the owner's Mac already; the choice can change before a build goes to anyone else. Mac-M3 step 5.
- **P1, an interface's saved state, is built now,** for the terminal's panes and the Mac's alike, on `interface-state`.

## Mac-M1 · Text, every day

The owner reads and sends text in jaw for the Mac, all day, on his own account.

**Needs from jawd:** nothing new. Everything here is served on main today: hello, focus, `chats.watch` (All and Archived), `messages.watch`, `watch.move`, `find`, `chat.opened`, `send.text` with `reply` and `mentions`, `send.retry`, `chat.start`, `draft.set`, `read`, `typing`, `react`, `edit`, `delete`, `message.info`, `chat.info`, `history.more`, `link.*`, `status`.

1. **The project.** `mac/` with Tuist (Swift 6 language mode, macOS 26, warnings as errors), `make mac` building jawd, jaw-engine and jaw for release into `Contents/Helpers`, each signed; a first app icon (the Mimic on Tokyo Night's ground).
2. **JawKit, the client.** The connection, the rows (chats, messages, `find`, `link`, `connection`, `notice`), the window models ported from jaw-client's `window.rs` with their tests, the merge per window between frames, reconnecting with `have`, waking at once for answers and once a frame for pushes, and the interface's half from `wire.rs`: keys, asking again after a lost answer within 20 hours, owed drafts and read positions, the latest `find`. Every fixture in `protocol/fixtures` decodes; a live suite on `jawd demo` kills jawd mid-stream and checks the windows.
3. **jawd at login.** The bundled jawd registered as the LaunchAgent `dev.badcoffee.jaw.jawd` with `SMAppService`, offering to take over the terminal's agent on the owner's click and never downgrading a newer jawd; `JAW_HOME` honoured; `--demo` running `jawd demo` in a folder of its own.
4. **The window.** The sidebar with All and Archived; the chat list (56-point rows, the hold, ticks, typing, drafts, unread and mentions, pinned and muted, communities opening to their groups); the conversation (spans, quotes, reactions, ticks, edited, deleted, forwarded, starred, system and call lines, day separators, "2 new", anchoring on a message, `history.more` at the top), each media message drawn as its one line and caption until Mac-M2; the status line's left half (jawd's state, and how the focused chat is without its name, which its box and header say, as the owner asked of the terminal's in 3d1da48); folding below 960 and 740 points.
5. **Two looks** (Owner, 26 Oct): bubbles, the default, and lines, the terminal's layout (names on the left, yours from a third of the way across, the time at the right edge when the minute or the sender changes), each comfortable or compact, picked in Settings › Messages and in View › Message Style. The layout cache is keyed by message, version, width and look; switching lays out the visible messages first and keeps the anchored message where it was.
6. **The box,** one per conversation, naming its chat. Enter sends with a key, ⇧↩ and ⌥↩ add a line, drafts after 1.5 s and on leaving, typing said, replying, `@` names from `chat.info`, `:` emoji, Retry on a failed message, refused text back in the box with jawd's reason.
7. **Actions.** Reply, react, edit, delete (for me or for everyone), copy and the info sheet, from keys, hover and the context menu, in both looks. Starring waits for jawd to serve `star`, and forwarding for the palette's picker in Mac-M3.
8. **Focus, reading and opening:** `focus` as the app is active and its window visible; `read` once a second at most while the window is key; `chat.opened` on your own opens.
9. **The palette, first form:** ⌘K over chats, people, folders and messages, the counts strip, Enter opens, Esc closes; start a chat by number.
10. **States:** starting jawd, jawd unreachable (the Mimic asleep), connecting, catching up, phone offline, offline, engine restarting, logged out (link again with `jaw` until Mac-M5), update needed, banned; empty states with the Mimic, still.
11. **jaw's theme and Settings, first form:** ooze's Tokyo Night, dark and light, following the system's appearance by default (Owner, 26 Oct), every colour asked for by its role, so Redlamp's families drop in at Mac-M5; a Settings window with Appearance (dark, light or the system's) and Messages (the look, comfortable or compact, the text size, the system font or SF Mono).
12. **The harness:** every state above drawn from fixtures in both looks, light and dark, and snapshot tests of them.
13. **The bench:** `jaw-mac bench` for launch, opens, keys in a flood, scrolling, memory and idle wake-ups, in each look.

**Done when** the owner has used it for a whole day for text on his own account, in the look he picks, and switching looks in Settings keeps his place; the conformance, window-model and live suites pass; and every budget below is measured in both looks and in the README.

| Budget | |
| --- | --- |
| Launch to a usable chat list | 250 ms p99 with jawd running, 500 ms after a reboot |
| Opening a chat | 30 ms p99; 50 ms p99 during 2,000 messages a second |
| Keypress to glyph | handled in under 4 ms p99, its glyph in the next frame, during the flood |
| A pushed message | drawn in the frame after it's read |
| Applying a frame of updates | 2 ms p99 on the main thread |
| Scrolling a chat of text | no frame missed: main-thread turns under 8.3 ms p99 |
| Memory | under 80 MB with a screen of text; under 25 MB with no window |
| Idle | no frames, timers or wake-ups but the clock's minute |
| App size | under 60 MB with the helpers |

## Mac-M2 · Every inline kind

Every kind of message shows inline as the design draws it, in both looks, and the viewer and the library are there.

**Needs from jawd:** M3's media in (`media` rows with `previews`, `thumbnail`, `state` and `progress`; `media.fetch`, `media.save`, `media.cancel`), on `m3-media-in` now; `library.watch`, M3; link previews, on `m2-content` now; voice playback, `voice.*`, M5. Polls are shown from their rows; voting waits for `poll.vote`. Until a part lands, its messages keep Mac-M1's line.

1. **Pictures:** the image queue (up to four, cancelled when scrolled past), the 64 MB cache of decoded bitmaps, the preview chosen by the pixels drawn, the blurred thumbnail fading into the preview in place.
2. **Photos and albums** (two across, grown up: 2, 3, 4, and 5 or more with "+N"), **stickers** (still, and animated WebP while on screen, three times through; P4 or the file says which), **GIFs** (looping, muted, while on screen), **videos** (poster, length and size, fetched with a ring, played in place), **video messages** (a circle, played in place).
3. **Voice** on jawd's playback: the waveform, speed, seek, the dot until played, the next unplayed one after, the playhead between pushes only while playing, the small player in the status line's place; **audio files** with AVFoundation.
4. **Documents** (icon, name, type, size, pages, first page; Quick Look, open, drag out), **link previews**, **locations** (MapKit's map, with its setting), **contact cards**, **polls** (live tallies), **events**, **view-once**.
5. **Every download state:** remote, downloading, local, expired, waiting for the phone, gone, each saying what it is and offering what can be done.
6. **The viewer:** growing from the bubble, the photo decoded at the window's size, zoom and pan, the album's paging and strip, video with the system's controls, Save, Share, Forward, Copy, Open in Preview, Go to Message.
7. **The library in a pane:** the kinds with their counts, search with `in:`, `from:`, `since:` and `until:` as tokens, the grid of previews, rows for voice, files and links, Enter to open and ⌘↩ to go to the message.

**Done when** every kind draws, plays and opens as designed in both looks, in the harness and on `jawd demo`'s account and the owner's, and the scrolling budget holds on a chat of photos and albums, in each look, and in the library's grid.

| Budget | |
| --- | --- |
| Scrolling with photos | no frame missed at 120 Hz: main-thread turns under 8.3 ms p99, hitches under 1 ms a second |
| Memory | under 150 MB with a screen of photos or the library; decoded pictures at most 64 MB |
| The viewer | its first frame within 50 ms of the click, from the 768-pixel preview; a 12-megapixel photo at full size within 300 ms |
| A video that's here | playing within 150 ms of the click |
| Idle with GIFs and stickers off screen | nothing playing, nothing woken |

## Mac-M3 · The palette, whole

Everything the terminal's palette finds and does, the app's does, with its keys; and sending media and voice.

**Needs from jawd:** `find` (served; links, media and files as jawd's find returns them); `forward` (served); `chat.pin`, `chat.archive`, `chat.mute` and `chat.mark_unread` for those actions, as jawd serves app state; `send.media`, M4; `voice.record` and the rest of recording, M5. An action jawd doesn't serve yet says so when chosen, as the terminal's does.

1. **Every kind in the results,** the counts strip, Tab narrowing and stepping through kinds, Enter doing the obvious thing for each kind.
2. **The app's actions, each with its key:** start a chat, go to unread, search messages, search here, mute for how long, archive, pin, mark unread, record, attach, react, delete, forward, the sidebar, the library, the theme, settings, the keys.
3. **Modes:** a reaction (six quick ones or any emoji by name), delete (saying why not when it can't be), forward (up to five chats picked with Tab).
4. **Search in a chat** (⌘F): `find` narrowed to the chat and to messages, each match gone to with `watch.move` and its words marked.
5. **Sending media:** dropped, pasted and attached files gathered above the box with captions and SD or HD, sent with `send.media`, their progress from `job` pushes and their rows.
6. **Recording:** ⌘R, jawd's levels and time in the box, Enter to send, ⌘R to stop and listen, Esc to throw it away.

**Done when** each of the terminal's palette actions and modes works in the app with its key, and the owner sends photos and voice messages from it on his own account.

| Budget | |
| --- | --- |
| Opening the palette | its field taking keys in the frame after ⌘K; nothing typed straight after ⌘K lost |
| A keystroke's results | names drawn within 16 ms of jawd's answer, 30 ms p99 from the key; messages within 60 ms p99 |
| Sending a photo | nothing the app adds beyond jawd's own 300 ms for a 12-megapixel photo |

## Mac-M4 · Split panels

Several chats at once, the spike's status line, focus mode and glance, and the sidebar's folders and roster.

**Needs from jawd:** P1 (an interface's saved state) to keep the layout, being built now on `interface-state` for the terminal and the Mac alike, as the owner asked on 26 October; until it lands, the layout lives in the app's preferences. M6's `folders.watch`, `roster.watch`, `folder.*` and `favourite.set`, with `chats.watch` taking any folder; until then the sidebar keeps All and Archived.

1. **Panes:** the layout as a tree of splits with ratios; split across (⌘D) and down (⌘⇧D); move (⌘⌥ arrows), go to (⌥⌘1 to ⌥⌘9), swap (⌘⇧ arrows), close (⌘W); a chat dragged onto a pane's edge or middle; each pane's header, box and draft; the focused pane's outline; twelve panes at most.
2. **The layout kept** with P1, or in preferences, and restored at launch.
3. **The status line's right half (what needs you elsewhere), focus mode (⌘⇧↩) and the glance (⌘J),** as the spike drew them, the glance's windows watched only while it's open.
4. **Folders and the roster:** the tree with its counts, smart folders, filing by drag and with m, the palette's filing and folder modes, favourites on ⌘1 to ⌘9, the roster with presence, faint where jawd isn't following someone now.

**Done when** the owner has worked a day in three panes, relaunching brings them back, and the sidebar's counts agree with the phone's.

| Budget | |
| --- | --- |
| Keypress to glyph, four panes of photos open | handled in under 4 ms p99, in the next frame |
| Opening a chat in a pane | 30 ms p99 |
| Launch with four panes restored | 350 ms p99, the panes' windows asked for together |
| Memory, four panes of photos | under 180 MB, decoded pictures still at most 64 MB |

## Mac-M5 · Polish, and it's jaw

Redlamp's themes, the animations, the Mimic, linking, the app's own notifications, accessibility, and a build anyone can install.

**Needs from jawd:** P2 (notifications claimed) for the app's notifications, P3 (the chat list's totals) for the Dock badge, P5 (jawd's settings) for the Media pane; linking is served. Without P2 the app leaves notifications to jawd, without P3 it shows no badge, and without P5 the Media pane shows `status`'s values.

1. **Themes:** Redlamp's families copied with `scripts/sync-themes`, jaw's roles mapped from their ten colours, eight senders' colours derived and checked at 4.5:1, tint, dark, light or the system's, the sidebar's translucency, native controls in the theme's accent, the palette in a theme of its own, and the Appearance pane drawing each family as a small chat.
2. **Animations,** each in the design's table, with its Reduce Motion form; the Mimic's chomp and sleep.
3. **Branding:** the icon in Icon Composer's layers with its dark, clear and tinted forms and its 16-pixel redraw; About with the poster's rainbow rule.
4. **Linking:** the QR code beside the Mimic, the phone-number code, logged out leading back to it, catching up after it.
5. **Notifications (P2):** posted by the app, grouped by chat, Reply and Mark as Read, a click opening the chat, withdrawn when the chat is read anywhere; the app kept running with its window closed; the Dock badge (P3).
6. **Settings, whole:** General (jawd at login, the `jaw` command on the PATH), Appearance (now with Redlamp's families), Messages (as in Mac-M1: bubbles or lines, comfortable or compact, text size, the system font or SF Mono), Notifications, Media (P5, and maps on or off), Account (linked as, log out), Keys, Advanced (jaw's folder, jawd's status).
7. **Accessibility:** VoiceOver over every list and message, every action on a key, Increase Contrast.
8. **Shipping:** the app and its three helpers signed with the Developer ID and the hardened runtime (jawd with the microphone's entitlement and its embedded Info.plist), notarized, a disk image, updates as Redlamp's, `make mac-install`; the budgets measured on a ProMotion MacBook too.

**Done when** a notarized build installs on a clean Mac, links by QR and catches up, meets every budget in the design on the Mac Studio and a ProMotion MacBook, and the owner has used it for a week as his WhatsApp.

| Budget | |
| --- | --- |
| Every budget in the design | as its table says |
| Energy | nothing beyond jawd's pushes when idle; App Nap allowed when hidden; animations only while visible |
| With no window, holding the notifications | under 25 MB, woken only by a notification |
| App size | under 60 MB with jawd, jaw-engine and the terminal |

## Alongside

- **jawd's proposals,** each on a jawd branch with its audit: P1 (an interface's saved state) now, on `interface-state`, for the terminal's panes and the Mac's alike, as the owner asked; P6 (what an interface owes, and the schema) before Mac-M1 ends, since JawKit checks against it; P3 by Mac-M1 for the badge, or with M6's folders; P4 by Mac-M2; P2 and P5 by Mac-M5.
- **The terminal's `jaw agent`** learns of the app's agent in Mac-M1, as step 3 needs.
- **`scripts/sync-themes`** comes with Mac-M5's themes and runs in CI after it.
