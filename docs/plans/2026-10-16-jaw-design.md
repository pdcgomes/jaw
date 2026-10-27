# jaw: design

16 October 2026. Settled with the owner over the day; nothing here is built yet. The plan this follows, with its build order, is the workstream's plan; this is the design it builds.

## What it is

jaw is a WhatsApp client for macOS and Linux that never makes you wait: your chats, photos, videos, voice messages and polls, from the keyboard, with the mouse too. It is three programs:

- `jawd`, a Rust daemon that does all the work: it keeps every message, prepares everything an interface shows, and sends and receives.
- `jaw-engine`, a small Go process that speaks WhatsApp for jawd through [whatsmeow](https://github.com/tulir/whatsmeow).
- Interfaces that only draw, starting with `jaw`, a terminal interface in Rust on ratatui, in ooze's style and Tokyo Night. A desktop app can come later and get everything for free.

Its creature is badcoffee.dev's Mimic, a treasure chest that's all jaw: to jaw is to chat, and a linked device mimics your phone.

jaw isn't affiliated with WhatsApp or Meta, and WhatsApp's terms don't allow unofficial clients, so it's developed against a spare number.

## Decisions

- **Name and creature:** jaw and the Mimic. The name is free on crates.io, in Homebrew and on GitHub.
- **The daemon keeps everything; interfaces are views.** jawd stays linked, keeps every chat and message in its own database, and serves interfaces over a Unix socket. Opening jaw never waits on WhatsApp, notifications arrive with no window open, and any later interface gets all of it.
- **Rust everywhere but the engine.** whatsmeow is the proven engine, and it's Go, so Go is confined to `jaw-engine` behind an `Engine` trait in jawd. whatsapp-rust is wired in behind the same trait in M10 and compared before a release picks the default; the first release, M8, runs on whatsmeow.
- **SQLite through rusqlite, with SQLite compiled in** (FTS5 on), so nothing needs installing. Unencrypted for now, in a folder only you can read; encryption later is rusqlite's `bundled-sqlcipher` feature and a key, in one module.
- **First version:** all of messaging, link previews on what you send, communities gathered under their community, nested folders, an IM-style roster, messages scheduled to send later, once or repeating, and reminders on messages. After it: AI assistants reaching jaw over MCP. Later: sending stickers, group admin, several numbers, video inside the terminal, transcription. Out: calls (shown as lines in the chat), status, channels, community admin, business tools and WhatsApp's settings.
- **Modeless keys:** typing goes to the message box; single keys act on a selected message.
- **One media pipeline** for every format, sending SD or HD as WhatsApp does, rather than cases per format.
- **One tree of folders** holding chats and people, each able to sit in several; the roster shows the same folders with presence.
- **Everything findable at once:** a command palette that fuzzy-searches chats, people, folders, actions and messages, and a library of every photo, video, voice message, file and link ever shared, across chats or in one, so nothing needs digging for through chat screens and menus.

## Principles

1. No interface ever waits. It sends a request and keeps drawing; the answer arrives as data.
2. jawd keeps realtime, interactive and background work apart, so history, media and maintenance can't delay a message by more than one short transaction.
3. An interface receives only what it shows (its chat-list window and its open chat's window), coalesced to the newest state of each row.
4. The daemon does everything an interface would otherwise repeat: names, formatting, quotes, reaction and receipt totals, poll tallies, display-sized previews, search, drafts, folders, presence, voice playback and recording, notifications, scheduled messages and reminders.
5. Memory is bounded everywhere: windows instead of histories, bounded queues, caches sized in bytes.
6. The engine stays thin. It speaks WhatsApp and hands jawd WhatsApp's own protobufs; everything that understands them is in jawd, so engines can be swapped and compared.
7. Nothing is ever lost, and the data is never inconsistent. Nothing is acknowledged, to WhatsApp, the phone or an interface, before it's on disk in a way that survives a power cut. A change and its event commit together or not at all. Every write is idempotent, so a replay, a duplicate or a crash halfway changes nothing twice. Every migration is transactional, checked and tested from every earlier version's real database, and can't leave a database half changed. Anything that makes data access slow at times or opens an edge case (a setting, a missing index, a query plan, a lock, a file that grows) is a defect, not a trade-off. Every change to the data layer follows "Data integrity" below and is audited by the database expert before it lands.

## Budgets

| What | Budget |
| --- | --- |
| A keypress drawn | 16 ms (p99), while 2,000 messages a second arrive |
| jaw's first frame | 50 ms after launch, with jawd running |
| Opening a chat | 30 ms |
| jaw's memory | under 40 MB with a screen of images |
| An incoming message durable and acknowledged | 10 ms (p99); a backlog drains at 200 a second on macOS while each ack waits for its own flush, and at 5,000 a second or more once acks are released in groups |
| An incoming message on its way to interfaces, once committed | 5 ms (p99) |
| jawd's own work in the commit-acked round trip, apart from the flush | under 200 µs (p50) |
| A 100-message page | 5 ms at any depth |
| Search | 50 ms over a million messages |
| A scheduled message queued to send, or a reminder due, while jawd runs | within 1 s of its time; a message also waits for WhatsApp to be reachable |
| jawd's memory | under 100 MB steady, 250 MB during a full history sync |
| jaw-engine's memory | under 80 MB steady, 200 MB during a full history sync |
| A 12-megapixel photo ready to upload | 300 ms |
| Idle | no drawing, no wake-ups, no polling |

The load harness measures them on an M-series Mac and a small Linux machine, and the README's "What it costs" reports them, as ooze's does.

## The screen

```text
╭ jaw · online · catching up 1,204 of 5,000 ────────────────────────────────────────────────────────────── 14:02 ╮
╭ folders ───────────────╮╭ chats · Family ────────────────╮╭ Ana Ribeiro · typing… ─────────────────────────────╮
│ ▾ All               12 ││ ● Ana Ribeiro         14:01  2 ││                     Wednesday                      │
│   Unread             5 ││   did you get the photos f…    ││ Ana  did you get the photos from Sunday?     13:58 │
│ ▾ Family             3 ││   Mum                    13:40 ││      ┌────────────────────┐                        │
│     Parents            ││   ✓✓ ok, call you later        ││      │      (photo)       │                        │
│     Cousins            ││ ▸ Lisbon Climbing     12:15 14 ││      └────────────────────┘                        │
│ ▸ Work               2 ││   Rui: Saturday at 9?          ││      ❤ 2                                           │
│ ▸ Communities          ││   Pedro & Ana        yesterday ││                  yes, the one at the lake ✓✓ 13:59 │
│                        ││   ▣ tickets.pdf                ││ Ana  ▶ ▁▂▅▇▆▃▂▁▂▃▅▆▅▃▂  0:14                 14:01 │
│ roster                 ││                                ││                                                    │
│ ● Ana Ribeiro   typing ││                                ││ ─ 2 new ────────────────────────────────────────── │
│ ● Rui           online ││                                ││                                                    │
│ ○ Mum        5 min ago ││                                ││ ╭────────────────────────────────────────────────╮ │
│ ○ Sofia      yesterday ││                                ││ │ a message                                      │ │
│                        ││                                ││ ╰────────────────────────────────────────────────╯ │
│                        ││                                ││                                                    │
╰────────────────────────╯╰────────────────────────────────╯╰────────────────────────────────────────────────────╯
  ⏎ send  ↑ messages  ctrl-k find  alt-↑↓ chats  ctrl-b sidebar  ctrl-o attach  ctrl-r record  ? keys
```

- **The header:** connected or not, catching up and how far, the time.
- **The sidebar:** the folder tree with unread counts, and the roster: online first, then by last seen. It folds away below 120 columns.
- **The chat list:** the folder's chats, newest first, pinned at the top; a community is one row with its groups' unread total, opening to its groups. It folds away below 90 columns.
- **The conversation:** messages anchored where you're reading, photos and stickers drawn with the terminal's graphics where it has them and half-blocks where it doesn't, voice messages with their waveform, and the message box, which always has the cursor.
- **The Mimic** sits in an empty conversation and on the linking screen beside the QR code, chomps when a message arrives, and sleeps when jawd can't be reached.

## How it fits together

```text
WhatsApp ⇄ jaw-engine (Go, whatsmeow, session.db)
              ⇅ length-prefixed protobuf frames on the engine's stdin and stdout
           jawd (Rust: lanes, jaw.db, media, voice, folders, presence, notifications)
              ⇅ JSON lines on a Unix socket
           jaw (Rust, ratatui) · a desktop app, later · jaw-mcp, for AI assistants (M9)
```

All three keep their files in one folder, `~/Library/Application Support/jaw` on macOS or `$XDG_DATA_HOME/jaw` on Linux, mode 0700 and left out of Time Machine. Everything that belongs to an account is under `accounts/<n>/`, so a second account later is a folder, not a migration: `session.db` (whatsmeow's keys), `jaw.db` (everything jaw shows), `vectors.db` later, `spool/` (history on its way in), `media/` (downloaded files and display previews) and `scheduled/` (the files of messages scheduled to send later). Beside them are `jaw.sock` and, when jaw needs one, a small `jaw.db` of its own for what spans accounts. What can't come again from the phone (folders, favourites, drafts, frecency, scheduled messages and reminders, and assistants' grants) is backed up daily with `VACUUM INTO` of just those tables to a folder Time Machine does reach, `scheduled/`'s files copied beside them.

## The engine

- **The `Engine` trait** is a stream of events and a set of commands, both shaped like WhatsApp. An incoming message arrives as whatsmeow's `MessageInfo` and WhatsApp's own `Message` protobuf, and jawd parses it with prost types generated from the `.proto` files whatsmeow ships. `scripts/sync-protos` copies them into `proto/whatsapp/` at the whatsmeow commit the engine pins, and CI checks the two agree.
- **Implementations:** `GoEngine`, which runs jaw-engine; `DemoEngine`, a made-up account for the demo, tests and the load harness; and, in M10, `RustEngine` on whatsapp-rust.
- **The link** is `proto/engine.proto`: length-prefixed frames on the child's stdin and stdout, its logs on stderr into jawd's log. Code is generated for Go (protoc-gen-go, committed) and Rust (prost with protox, so no protoc is needed). The first frame carries a version, and jawd refuses an engine it doesn't match.
- **Durability.** whatsmeow runs with `SynchronousAck` and `EnableDecryptedEventBuffer`. The engine's handler sends each message and waits for jawd's ack, which comes once the raw message is committed to `inbox`; only then does WhatsApp hear "received". A crash anywhere before that means WhatsApp sends it again, and whatsmeow's buffer means it isn't decrypted twice.
- **That round trip caps a backlog.** whatsmeow handles incoming messages one at a time from a single queue, so a backlog drains at one round trip per message. The spike measures it; the target is under 200 µs.
- **Big payloads travel as files.** History blobs are written to the spool and passed by path, one at a time, the next only once jawd has taken the last. Downloads go straight to the path jawd names (`DownloadToFile`), and uploads read from one (`UploadReader`). Frames stay small, so a 50 MB history chunk never queues ahead of a live message.
- **Commands:** link by QR or code, log out, send (a raw message with jawd's ID), upload, download, media retry, mark read, presence, chat presence, subscribe to presence, app-state patches (pin, archive, mute, star, mark read, delete), history request, poll vote (it needs the poll's secret, which whatsmeow keeps), group info and subgroups, user info, profile pictures, `IsOnWhatsApp`, privacy settings.
- **whatsmeow's settings:** `ManualHistorySyncDownload`, `EmitAppStateEventsOnFullSync`, `AutomaticMessageRerequestFromPhone`, `store.SetOSInfo` so the phone lists "jaw", `RequireFullSync`, and `SetProxyAddress` from `HTTPS_PROXY`.
- **`session.db`** is whatsmeow's `sqlstore` on mattn's driver (WAL, `synchronous=NORMAL`), unless the spike shows a pure-Go driver keeps up with a backlog. jawd sets the engine's `GOMEMLIMIT` and `GOGC`.
- **Supervision:** jawd starts the engine, restarts it with backoff and tells interfaces it's reconnecting meanwhile; the engine exits when its stdin closes.

## jawd

### Work lanes

- **Realtime:** incoming messages, receipts, typing, sends, socket I/O. **Interactive:** what an interface shows or sends right now. **Background:** history, auto-downloads, avatars, off-screen previews, media retry, checkpoints, eviction, rebuilds.
- tokio, with at most 4 worker threads, for I/O: the engine link, sockets, HTTP, timers and child processes. Dedicated threads for the database writer and readers, a rayon pool of half the cores for parsing, images and waveforms, and one thread for audio.
- A pool per resource, each a priority queue with interactive ahead of background: downloads 6, uploads 3, video transcodes 1 to 2 (ffmpeg at a lower OS priority), image work half the cores, database readers one per core up to 4, sends one queue per chat with 16 chats at once.
- Jobs with the same key share one run. A background job is promoted when a window shows its message. Bounded queues push back on background producers; realtime work never waits on them.
- Each pool's depth, wait and run times (p50 and p99) show in `jawd status`, and anything over budget is logged with what it was.

### Storage

- `jaw.db`, in WAL mode, through rusqlite with SQLite bundled.
- **Tables:** `inbox` (raw engine events waiting for conversion); `chats` (sort key, last message, unread and mention counts, draft, flags, community, frecency); `messages` (kind, sender, time, status, reply target, edit, delete and expiry, render-ready spans, and the compressed `raw` protobuf); `media` (one row per SD or HD copy); `reactions`; `receipts`; `poll_options`; `poll_votes`; `people` (one row per person, joining their LID and phone JIDs, with names, about text, avatar and last seen); `group_members`; `folders` and `folder_items`; `pending` (edits, reactions and votes for messages not seen yet); `outbox`; `scheduled` (messages waiting for their time, once or repeating) and `reminders` (a time on a message), from M7; `grants`, their tokens and the assistants' log, from M9; `interface_state` (what each interface keeps of how it was laid out: its split panes, by the name it gives in hello); `events` (seq, type, key); and `messages_fts` (FTS5, `unicode61 remove_diacritics 2`, so "nao" finds "não").
- **The raw protobufs are the source of truth.** A migration can rebuild the tables when jaw learns to read something new, without asking the phone again, and nothing WhatsApp sends is lost before jaw understands it.
- **One writer thread** with prioritised queues: `inbox` commits first, since the engine waits on them; then live conversions and actions: after each inbox group, those waiting go in transactions of their own for 1 ms or at least one, and the first that must reach the disk goes, with those behind it, into the next group's synced commit and is answered with its acks, so a catch-up never holds them back and a durable write costs no extra flush; then background batches capped at 1,000 rows or 3 ms per transaction, commit included, each ending early, once its work has had a millisecond, when an inbox row or a live write is waiting, so a first pass of history's small chats doesn't go one to a transaction while a catch-up's rows keep coming; every message's text, live as well as history, waits in `fts_backlog` and is indexed for search afterwards in steps as short, since FTS5 does most of its work at commit: a lone message is searchable about 10 ms after it's stored, and in a catch-up or flood text becomes searchable as the burst goes (a step every 100 ms at least) and all of it once the burst ends. Queries go to a pool of read-only connections, so paging never waits behind a sync. Statements are prepared once.
- **Commits:** `NORMAL` for derived work, synced (`FULL`, and `fullfsync` on macOS, where `fsync` doesn't reach the disk) for every commit that precedes an acknowledgement; `checkpoint_fullfsync` on every connection; `wal_autocheckpoint=0` on the writer, a checkpointer thread running PASSIVE while frames wait and flushing `jaw.db` after each pass the writer outruns, background work held back, and past 60 MiB, or past 48 MiB while background work waits for it, the writer starting the WAL again itself between inbox groups and live writes, so the WAL stays under 64 MiB.
- **Keyset paging** on `(chat, time, rowid)`, never OFFSET, so the 50,000th page comes as quickly as the first; the chat list reads `chats` alone.
- **The feed can't disagree with the tables:** each change writes its `events` row in the same transaction. The feed keeps 7 days.
- **No polling:** a timer for the next disappearing message; one on the wall clock for the next scheduled message or reminder, so neither the Mac's sleep nor a change of the clock makes it late; and eviction when the cache passes its size.
- **Migrations** follow the strategy in "Migrations" below.

#### Data integrity

These are rules; every builder follows them, the tests listed check them, and the database expert audits every change to the data layer against them before it lands (`.cursor/rules/data-integrity.mdc`, `.cursor/agents/database-expert.md`, `docs/audits/`). The review behind them is `docs/reviews/2026-10-16-data-review.md`.

- **Durable before acknowledged.** A commit that precedes an acknowledgement (the inbox commit before the engine's ack, enqueuing a send before `send.*` answers, any action before its reply, the commit that drains a history blob) is synced before the acknowledgement goes: `synchronous=FULL`, and on macOS `fullfsync`. Acks either wait for their own synced commit or are released in groups after one. Everything else commits at `NORMAL`, since a later synced commit makes it durable too. Every connection to `jaw.db` and `session.db` sets `checkpoint_fullfsync`, or a power cut on macOS can corrupt the file. A spool or media file is synced and renamed into place before a row points to it, and deleted only after a synced commit stops pointing to it.
- **Exactly once from at least once.** Everything that arrives can arrive again: live and history, the same message twice, an edit or reaction before its message, a JID as a phone number and as a LID. Every write is an idempotent upsert with merge rules: status only advances, an edit wins by its time, a reaction by its time, a deletion and a for-me tombstone are sticky, and a chat or person is found through its alias table before anything is written. An inbox row is deleted in the transaction its conversion commits in; a change that fails is quarantined, never dropped, and never blocks the changes after it.
- **Derived values are derived.** Counts and summaries (message counts, unread, mentions, reaction and poll totals) are kept by triggers or recomputed in the transaction that changes their source, never adjusted by hand in one code path, and a background check compares them with their source. Unread is derived from jaw's messages and the chat's read position, which is exact for what jaw shows; the phone's count stands in only until history has arrived.
- **Erased when asked.** A disappearing or view-once message, once it's gone, is gone from the files too: its `raw` cleared, `secure_delete=FAST`, and its FTS segments merged so its words go. A deletion's tombstone (about 40 bytes) is kept for as long as the account is linked, so history can't bring the message back.
- **The schema is known.** Tables are `STRICT`; booleans and invariants have `CHECK`s; JSON columns are `json_valid`; foreign keys are on; `jaw.db` carries jaw's `application_id`; every PRAGMA is read back after it's set; jawd checks the schema's fingerprint at every start and refuses one it doesn't expect. Row keys that others hold (`inbox.seq`, `messages.n`, `people.n`) are never reused.
- **Bounded files.** The WAL is kept under 64 MiB by making background work wait for the checkpointer, and inbox rows and live writes wait, a moment once every 60 MiB, for the writer to start it again; it passes 64 MiB only while a reader holds its frames for longer than the writer waits for one (20 ms), when the writer's wait at the edge for the checkpointer's pass runs out (1 s), as a busy drive can make it, or for a transaction larger than the room the writer leaves, and is cut back at the next start; disk space is watched, and background work pauses before the disk fills; nothing grows without a limit and a number in `jawd status`.
- **Predictable plans.** No `ANALYZE` or `PRAGMA optimize`; every statement's plan is pinned by a test; no statement scans a large table, sorts in a temporary b-tree, or uses `OFFSET`.
- **One process.** jawd holds `jaw.lock` and the engine `session.lock` for as long as they run; jaw's folder is local, left out of Time Machine and Spotlight, and refused on a network or synced file system.
- **Checked, and recoverable.** After an unclean shutdown, and weekly when idle, a reader runs `PRAGMA quick_check` and FTS5's `integrity-check` in the background; monthly, `integrity_check`. On corruption jawd stops writing, stops acknowledging (so WhatsApp keeps what's new), tells interfaces, and rebuilds `jaw.db` on its own: jaw's own tables copied, everything else rebuilt from raw and fetched again from WhatsApp. It keeps the damaged file and asks before discarding it.
- **Tested.** Every rule above has a test: crash and power-cut simulation around each commit that precedes an ack, replays and duplicates of every kind, upgrades from real fixture databases of every version, plan snapshots, the WAL bound under a full history sync and in long bursts of live writes, and disk full.

#### Migrations

- **Forward only.** Migrations are numbered SQL built into jawd, run against `PRAGMA user_version`. There are no down migrations: going back is restoring the backup taken before the upgrade, or running an older jawd within its compatibility window.
- **Frozen once merged.** A migration that has reached `main` is never edited. A test hashes each migration's text against a checked-in list, so editing one fails CI. Branches don't claim numbers: the next number is taken when a branch merges, and CI fails if two migrations share one.
- **Three kinds, declared with each migration.** *Schema steps* cost the same however big the database is (new tables, triggers, indexes on small tables, a nullable column or one with a constant default); all pending ones run at start in one `BEGIN IMMEDIATE` transaction with `user_version`. *Data steps* cost time in proportion to the rows (a table rebuild, an index on `messages`, an `UPDATE` of every row); each runs in its own transaction after the socket listens and before the engine connects, within 10 s per million messages on the reference Mac, its measured cost written beside it. *Backfills* run while jaw does, in the background lane, in transactions of at most 3 ms, resumable from the `backfills` table and idempotent; code works with the data in either state until one is done. Anything that can be a backfill is one.
- **Opening a database:** take `jaw.lock` or refuse; check `application_id`; refuse a `user_version` newer than this jawd unless `meta.compat` says the newer migrations were additive; compare the schema's fingerprint with the one expected for that version and refuse to write on a mismatch; before any data step, check free space is at least 3 times the database (the backup, the step's WAL and jaw.db's growth while a table is rebuilt; migration 6 took 2.6 times) and `VACUUM INTO 'jaw.db.before-vN'`; run the migrations; check again (fingerprint, `foreign_key_check`, FTS5's `integrity-check` if `messages` changed), rolling back and refusing to start on a failure.
- **Rebuilding a table** follows SQLite's twelve steps as a declared kind of step: `foreign_keys=OFF` before `BEGIN`, a new `STRICT` table, copy, drop, rename, every index and trigger recreated (dropping a table drops its triggers, and FTS5's own check doesn't notice), `foreign_key_check`, commit, `foreign_keys=ON`.
- **FTS changes** are backfills into a new index, written to by both sets of triggers, then swapped in one schema step.
- **What may be rebuilt.** Every table declares its class beside its `CREATE`: *derived from raw* (message rows, spans, FTS, reactions, tallies, links, vectors), rebuilt from raw as a backfill and a swap; *fetched from WhatsApp* (app-state effects, members, contacts, avatars, the LID map), rebuilt only by asking again with the engine connected; *jaw's own* (folders, favourites, frecency, drafts, the outbox, scheduled messages and reminders, assistants' grants, their tokens and log, read positions jaw sets, quarantine, backfills, meta), never dropped or rebuilt, and copied as they are by every rebuild. A test checks no jaw-own table is ever dropped.
- **session.db.** Before `sqlstore.New`, the engine takes `session.lock`, and if whatsmeow has an upgrade pending, writes `session.db.before-whatsmeow-vN` with `VACUUM INTO`. A refusal reaches jawd as "update needed", not a crash loop.
- **Tests:** frozen migrations and unique numbers; checked-in fixture databases made by every released version's jawd (`jawd fixture`, with edge cases: accents, NULLs, a 64 KB message, LID and phone JIDs for one person, pending amendments, folders, quarantine); an upgrade from every fixture checked by fingerprint, rows, foreign keys, FTS and triggers; a failure injected at every statement of every migration leaving the database as it was; the refusals; the compatibility window; each data step's cost on the million-message bench in `make perf`; a rebuild from raw compared with the original; plan snapshots; drain to empty, then one more.

### Sync

- **Linking** by QR or the phone-number code, through the engine; jawd asks for the full history.
- **The converter** drains `inbox` as soon as rows land, in batches during a burst, into the tables and the feed.
- **History** blobs from the spool are parsed in the background lane a conversation at a time and stored in batches. A chat's sort key is its newest message's time, so older history never moves a chat.
- **Catching up after time offline:** `OfflineSyncPreview` says how many messages, receipts and changes are coming. Until `OfflineSyncCompleted`, rows update in place, the chat list reorders at most every 2 seconds, interfaces show progress, and notifications wait for one summary at the end.
- **One converter per kind:** text, extended text, image, video, video note, audio, voice, document, sticker, location, live location, contact cards, events and their responses, polls (v1 to v6) and their votes, reactions, edits, deletes, ephemeral, view-once and device-sent wrappers, albums and HD children, events, pins, group invites, the call log. Their output is render-ready: names resolved, formatting parsed into spans, mentions and links marked, quotes summarised, reactions and receipts totalled, polls tallied. A kind jaw doesn't know keeps its protobuf and shows as "jaw can't show this yet".
- **App state** (pin, archive, mute, read and unread, star, delete for me, clear and delete chat); contacts and push names joined into people by LID and phone number (`SenderAlt` and whatsmeow's LID map); group info and members; communities from `IsParent`, `LinkedParentJID`, `IsDefaultSubGroup` and `GetSubGroups`; avatars for visible chats in the background lane, resized once.
- **Undecryptable messages** show as waiting until whatsmeow's retries resolve them. Scrolling past the oldest message asks the phone for 50 more.

### Sending

- jawd makes each message's ID up front, in whatsmeow's format, and the engine sends with it (`SendRequestExtra.ID`), so the row shown at once is the real message: pending, sent, delivered, read, played.
- One queue per chat keeps order; chats send in parallel; failures retry with backoff, then stay marked for a manual retry.
- Text, replies, mentions, edits, deletes for me or for everyone, reactions, forwards, polls and votes, read receipts (unless they're off on the phone), typing (throttled), drafts kept in jawd for every interface.
- Online only while you're looking: jawd tells WhatsApp you're online while an interface has focus, which terminals report. Otherwise you're away, and your phone keeps its notifications.
- A new poll carries a fresh secret, which whatsmeow keeps when it sends it; votes go through the engine, which has every poll's secret.
- Link previews are fetched in the interactive lane within 3 seconds; the text goes without one if the page doesn't answer in time.
- Any send can be given a time instead, once or repeating, and goes this same way when it comes (M7, "Scheduled messages and reminders").

### Media

- **Incoming:** every message brings its thumbnail, so it shows at once. Photos, stickers, GIFs and voice download as they arrive, straight into the cache through the engine; videos and documents when you open them, or on arrival below a size you set. An HD copy attaches to its message instead of showing as a second one. Files gone from WhatsApp's servers, as history from before jaw was linked often is, are asked of the phone again (`SendMediaRetryReceipt`), which needs the phone online.
- **For interfaces,** jawd writes display-ready previews at 256 and 768 pixels (the `image` crate and `fast_image_resize`; video posters and an animated sticker's first frame with ffmpeg), so no interface decodes full-size media to show a list.
- **Outgoing:** ffprobe reads what a file is; then a plan per kind: JPEG; H.264 and AAC in MP4, through VideoToolbox on macOS or libx264 on Linux; GIFs as looping MP4; audio as AAC; documents as they are. A file that already fits passes untouched. Metadata is stripped and orientation kept. SD and HD sizes are measured from what the phone app sends before they're fixed, and HD goes as WhatsApp's dual upload: a standard copy, then the HD copy linked to it. Thumbnails, posters and waveforms are made alongside.
- **Streaming** both ways through the engine, with progress events and cancel, so a 2 GB video never sits in memory.
- **The cache** keeps files by content hash, 5 GB by default, dropping what was opened longest ago and never starred or unsent files.

### Voice

- In jawd, so every interface gets it by asking. Ogg Opus is decoded with the `ogg` and `opus` crates (libopus compiled in) and played through cpal; speed and seek re-decode from the position; the playhead is reported ten times a second.
- Recording: cpal captures, jawd encodes mono Ogg Opus as you speak, reports levels, computes the 64-point waveform and sends a voice message. macOS asks once for jawd to use the microphone.

### Folders and the roster

- **One tree of nested folders.** An item is a chat or a person, and can sit in several folders. A person stands for their one-to-one chat in chat lists and for themselves in the roster.
- **Smart folders** are saved filters jawd runs in SQL: unread, mentions me, groups, a community, muted, waiting on me (their message unanswered for a day), from favourites. Built in: All, Unread, Groups, Communities (each community with its groups, the announcement group first) and Archived.
- A folder's unread count is its chats' unread total, each chat counted once. Any folder can be a window, as the chat list is.
- **Favourites:** up to nine folders or chats on Alt-1 to Alt-9. The finder ranks by frecency: opens per chat, decaying over time.
- **The roster:** people from the phone's contacts who are on WhatsApp, and anyone with a one-to-one chat, grouped by folder, online first, then by last seen, then by name, with offline people shown or hidden. It shows online, typing or recording, last seen, or nothing when someone hides it; their about text (`GetUserInfo`, 50 at a time, refreshed daily); and their avatar. Enter opens or starts the chat.
- **Presence is bounded,** because subscribing to everyone is what presence trackers do, and WhatsApp polices it. jawd subscribes to the people in a roster window or an open chat, only while an interface has focus (which is also the only time WhatsApp sends presence), at most 20 new subscriptions a minute and 200 per connection, open one-to-one chats' people first, then favourites.
- Folders, favourites and frecency are jaw's own, kept in `jaw.db` and shared by every interface; WhatsApp never sees them.

### Presence and typing in chats

A step on a branch of its own with its own audit, in two halves, as the owner decided on 27 October: typing now, and presence once M6 is on main, since it comes from the rows M6 writes. M1 tells WhatsApp your own presence and typing, and M6 shows others' in the roster; this step shows them where you chat, in every interface.

- **What shows.** Every chat, groups included, shows who's typing or recording in it now, in its chat-list row and its header. A one-to-one chat's header, and the status line while it's the chat you're in, show the person's presence: online, typing or recording, last seen, or nothing when they hide it, faint when jawd isn't following them, as the roster draws it. The terminal draws them in its chat list, its conversation's title and its status line, and the Mac app in its chat list, header and status line (its proposal P7).
- **Where it comes from.** Typing is whatsmeow's `ChatPresence`, which WhatsApp sends while you're online, groups included, and which the engine forwards as M6 has it, so until M6 is on main only the demo engine types; online and last seen come only for people jawd subscribes to (`Presence`), within the limits above, open one-to-one chats' people first, as M6 follows them.
- **When typing ends.** It fades 25 seconds after it was last said, as the phones let it fade; a pause ends it at once, and so does a message from them in that chat; and all of it ends when jawd tells WhatsApp you're away or WhatsApp can't be reached, since nothing more would come.
- **Pushes, not rows.** They come as the protocol's `typing` and `presence` pushes, each watch's answer saying how things are now, so an interface that watches again or reconnects shows them as they are, never a "typing…" or "online" left over. Who's typing in a chat is kept in jawd's memory only, never in `jaw.db`, and moves no `v`, so it never reaches the writer; chat rows' `typing` leaves the protocol, which lists nothing jawd doesn't send. Carrying them on chat rows, which a re-watch from `since` would bring up to date, was turned down: every start, pause and fade of typing would have been a write on the writer the inbox's commits share.
- **Bounded.** Typing in busy groups can come several times a second. Each chat's is coalesced to one change each 50 ms, adds nothing measurable to a message's ack in a flood with typing in many chats, and what's dropped for coming too fast is counted in `jawd status`.
- **Tested end to end.** The demo engine types in groups and one-to-one chats and has the people followed come online and go, so `jawd demo` shows all of it through the real socket, where the interfaces' end-to-end tests check it. Feed tests watch again from `since` and must hold what a fresh watch gives, as M6's do.

### Chat actions from jaw

A step of its own, now, on a branch with its own audit, as the owner decided on 27 October. jawd reads the pins, archives, mutes, unread marks and stars the phone sets (Sync's app state); this step sets them from any interface, so the terminal's palette and the Mac's menus, which already send `chat.pin`, `chat.archive`, `chat.mute` and `chat.mark_unread`, do what they say.

- **What it sets.** A chat pinned, three at most as WhatsApp allows, or unpinned; archived, which unpins it, or not; muted until a time or for ever, or unmuted; marked unread, or read up to its newest message with read receipts as reading sends them; and a message starred or not.
- **The chat first, then WhatsApp, through the outbox.** Each change and the app-state patch that tells your other devices are written in one synced commit before jawd answers, and the patch goes through the outbox to the engine's `AppStatePatch`, as deleting a message for you does. Asking again changes nothing, and WhatsApp's echo of the patch finds the row as jawd wrote it.
- **Refused for good.** A patch WhatsApp won't take is quarantined, never dropped, and jawd fetches that app-state collection again, so the chat ends as the phone has it, and every interface is told why.
- **Its requests and tests** are in [the Mac's menus and folders design](2026-10-27-jaw-mac-menus-and-folders-design.md); the protocol's text and fixtures come with the step. Deleting and clearing a chat, which lose messages, are a later step of their own.

### Scheduled messages and reminders

M7, on a branch of its own with its own audit, as the owner decided on 27 October. Its requests, its screens in both interfaces and its tests are in [the scheduled messages and reminders design](2026-10-27-jaw-scheduled-messages-and-reminders-design.md), and the screens are drawn in its wireframes canvas.

- **In jawd, not WhatsApp's way** (Owner, 27 Oct). jawd keeps a scheduled message and sends an ordinary one at its time, so whoever gets it needs nothing new, and nor does whatsmeow. WhatsApp's own scheduled messages (`ConditionalRevealMessage`) go ahead of time, hidden until their key is revealed, and need the recipients' apps to support them. The cost is that jawd must be running, and WhatsApp reachable, at the time.
- **Everything jaw sends, once or repeating** (Owner, 27 Oct): text with replies and mentions; photos, videos, GIFs and files with their captions; a voice message recorded now; a poll, an event, a contact card or a place; a GIF found by search; a forward. Every send request takes `at`, and text, media, voice and polls take `repeat` too: every day, weekday, week, month or year at a local time, until a date or for a number of times, at most daily and to one chat. jawd reads typed times ("fri 18:30", "every weekday 9am") for every interface, so they read alike in each.
- **At its time, an ordinary send.** One timer, on the wall clock, waits for the next scheduled message or reminder. At a message's time, one synced commit writes its message row and outbox entry as the send would have, and moves it to its next time or removes it; from there it's sent, ticked, retried and failed like any message. It reads nothing in its chat, since you weren't there. Its files wait in `scheduled/`, each synced and renamed into place before its row points to it; media is planned and transcoded when it's scheduled, so a file that can't go says so then.
- **Late** (Owner, 27 Oct). A message goes once its time has come and WhatsApp can be reached. If that's within an hour of its time, it goes; past the hour it's held, jawd notifies, and every interface offers to send it now, give it a new time or cancel it. A repeating message that missed several times holds only the last, saying how many it missed. In the half hour before a send, jawd keeps the Mac from idling to sleep (a power assertion on macOS, an inhibitor on Linux); it can't wake a Mac that's asleep, which is what the hour is for.
- **Changes race nothing.** Editing, rescheduling, skipping, sending now and cancelling go through the one writer, so a change that arrives after the send is refused, saying which message it became. A message whose quote was deleted, or whose forwarded message is gone, by its time is held as a late one is. A chat merged into another takes its scheduled messages and reminders with it.
- **Reminders** are a time on a message, anyone's; one set on a chat goes on its newest message. When one is due, one synced commit makes it due and marks its chat unread, the app-state patch going through the outbox as "Chat actions from jaw" sends it, so the phone shows the chat as unread too (Owner, 27 Oct); jawd notifies, in a muted chat too, since you asked. It stays due until it's done or snoozed, and reminders that came due while jawd wasn't running notify once it's back, several as one summary. A reminder keeps no copy of its message: one deleted for everyone, or disappeared, shows as gone, and deleting a message for you takes its reminder with it. WhatsApp's own reminders aren't in a form jaw can read or write yet; a later step syncs them once whatsmeow's protobufs carry them.
- **Bounded, and jaw's own.** At most 1,000 scheduled messages holding at most 2 GiB of files, and 1,000 reminders, each limit refused with error 5 and counted in `jawd status` beside the next time and how many are held. Both tables are jaw's own, never dropped or rebuilt, and backed up daily; each change writes its feed event in its transaction, and chat rows' counts of scheduled messages and due reminders are derived.
- **In every interface.** A chat's scheduled messages show under its newest message, faint, with their times, and a message with a reminder shows a flag and its time. The terminal schedules what's typed with Ctrl-T and sets a reminder with t on a selected message; the Mac adds Send Later to the box and Remind Me to its menus (its proposal P9). Both have a Scheduled list and a Reminders list, opened from the palette and the sidebar, and say what's held or due in the status line.

### Finding anything

- **One search behind the palette.** `find` takes what's typed and answers, best first, with chats, people, folders, messages, links, media and files, each result saying what it is. Interfaces add their own actions to it, since those are theirs.
- **Names in memory.** Every chat's and person's name, phone number and folder, with their frecency, are kept in memory and matched fuzzily, so a keystroke's answer comes back within a few milliseconds and ranks what you open often first; initials and parts of words match, and accents are ignored.
- **Text from the store.** Messages, captions, file names, link titles and poll questions come from FTS5, accent-blind, newest first, after the names, with the match marked. A new keystroke cancels the last query, so typing never queues up searches. Text is searched from the second letter (names from the first), and there's no prefix index, which would triple the index and tax every write; to revisit with real text.
- **Links kept as messages are converted:** each URL in a message, with its host, its preview's title and description, who sent it, where and when, in a `links` table that `find` and the library read.
- **Media by kind:** the `media` table already holds each file's kind, size, dimensions, length and name, so the library pages through photos, videos, voice messages and documents by keyset like any window, with their 256-pixel previews.
- **The library window** is `library.watch`: a kind (or all of them), and optionally a chat, a person, a time range and some text; rows are the message's media, link or file with what's needed to show it and jump to its message.

### The socket and windows

- One JSON object per line; requests and replies shaped like JSON-RPC 2.0; `hello` first, with the protocol version. [protocol/README.md](../../protocol/README.md) specifies it.
- **Live windows instead of a firehose:** `chats.watch` (a folder and a range), `messages.watch` (a chat, an anchor, how many before and after), `roster.watch`, and from M7 `scheduled.watch` and `reminders.watch` return rows, then push upserts and removals for that window. Rows carry a sort key and a version, so updates converge in any order, and there are no positions to get wrong.
- **A fan-out task per client** reads the feed, keeps what its windows show and only the newest of each row, and sends the first change after a quiet spell at once, so a lone message is on screen within the budget, then coalesces a burst and flushes when the feed has been quiet for 5 ms, or every 50 ms while it goes on. A rename raises the version of the rows it changes, so a reconnect from a seq is always exact. A slow client never holds jawd up: its pending updates are bounded by its windows, and one too far behind is told to watch again. On reconnect it watches again from its last seq and gets only what changed.
- `jaw-proto` is the protocol's one definition, used by jawd and every Rust interface; `protocol/fixtures/` pins the wire format, and interfaces in other languages get a JSON Schema generated from it.

### Lifecycle and notifications

- `jaw agent install` starts jawd at login, as a LaunchAgent on macOS or a systemd user unit on Linux, from ooze's `agent.rs`; jawd starts the engine. Without it, jaw starts jawd itself and keeps drawing while it starts.
- The listener follows haunt's: refuse if another jawd answers, clear a stale socket, mode 0600, and check that whoever connects is the same user. It listens before the engine connects, so interfaces get local data at once.
- Notifications come from jawd, as ooze's do: Notification Center on macOS, `notify-send` on Linux, or the terminal's own. Muted chats and the chat you're reading stay quiet. A reminder notifies when it's due, in a muted chat too, and a scheduled message held past its hour says so.

### Settings

- **A domain of settings for each frontend,** named by the client its hello gives, kept by jawd as jaw's own in the `settings` table, so a terminal and a Mac app on one machine keep separate preferences. The terminal's are the theme, one of Omarchy's, and where your messages sit. Hello's answer carries a connection's own, `settings.set` changes them with a synced commit before it answers, and each of the domain's connections is pushed them when they change. [docs/plans/2026-10-26-settings-design.md](2026-10-26-settings-design.md) has the whole design.
- **Omarchy's themes, as data:** `protocol/themes/` holds each theme's colors.toml as Omarchy wrote it, served with `themes`; each interface maps the palette onto what it draws, keeping every role readable.

## The terminal interface

- **Threads:** the client's (from `jaw-client`), input (crossterm: keys, mouse, paste, focus), two image workers, and the main thread, which folds and draws. One channel into the main thread; input is handled first.
- **The loop** sleeps until something arrives or the next deadline, applies what's waiting within 8 ms, and draws once if anything changed: at most 60 frames a second in a burst, animations at 10 a second and only while visible.
- **Nothing jumps.** The chat list keeps the selected chat by ID and holds its order while you move through it. The conversation anchors on a message, so history arriving above doesn't move it; new messages below scroll only when you're at the bottom, and otherwise a "2 new" marker shows.
- Wrapping is cached per message, version and width; only visible messages are laid out; long ones fold after 40 lines.
- **Images** with ratatui-image: the protocol is detected once at start with a short timeout (kitty, iTerm2, sixel or half-blocks), images are encoded on the workers from jawd's previews, and kitty's are sent once and then drawn as Unicode placeholders, so scrolling is only text. The image cache is capped in bytes.
- **Media:** photos and stickers inline; videos, GIFs and video notes as posters with their length (Enter opens a player); voice with its waveform and playhead; documents, locations, contact cards and link previews as cards; polls with bars and counts, number keys to vote.
- **The palette:** Ctrl-K opens it over the screen. One line of fuzzy search finds chats to switch to, people to start a chat with, folders, every action (start a chat, new group message, mute, archive, pin, go to unread, a new poll, record, attach, send later, remind me, scheduled messages, reminders, link a device, the library), and messages, links and files; results are grouped by kind with the best match first, Enter does the obvious thing, Tab narrows to one kind, and the actions show their keys, so the palette also teaches them.
- **The library:** every photo, video, voice message, file and link shared, across all chats or in the open one: photos and videos as a grid of previews, files and links as lists with who sent them, where and when, filters by kind, chat, person and date, and text search over titles, file names and captions. Enter opens the item, and another key jumps to its message in its chat. It opens from the palette, from a chat's info, and with a key of its own.
- **Scheduled messages and reminders** (M7): Ctrl-T in the box asks when to send what's typed, with a few times to pick and a line to type one, jawd's reading of it shown as you type, and repeats; a chat's scheduled messages sit faint under its newest message with their times, a clock before each, where Up reaches them first to edit, reschedule, send now or cancel. t on a message asks when to remind you, and the message shows a flag and the time until it's done. The Scheduled and Reminders lists open from the palette and the sidebar and take the conversation's place, as the library does; what's held or due shows in the status line.
- **Settings:** Ctrl-, opens a panel in the chat list's place, the open conversation showing each change: the theme, each previewed on the whole screen as you move through the list, and your messages on the right or, as IRC clients put them, on the left under "You". The palette opens it too, or its list of themes, and turns the IRC layout on or off; `jaw settings` reads and changes them from the command line.
- **Keys:** Ctrl-K opens the palette; Alt-Up and Alt-Down move between chats; Alt-1 to Alt-9 go to favourites; Ctrl-B shows the sidebar, and Tab moves between folders and roster; Alt-Shift-Up and Alt-Shift-Down move between folders without opening it. Ctrl-, opens settings with the kitty keyboard protocol, and the palette's Settings everywhere. Up from an empty box steps into the messages, where r replies, e edits, d deletes, + reacts, f forwards, y copies, Enter opens or plays, s saves, t sets a reminder and i shows its info; m files the selected chat or person into folders; Esc goes back to typing. Enter sends; Alt-Enter, or Shift-Enter with the kitty keyboard protocol, adds a line. @ and : complete names and emoji. Ctrl-O attaches files, dropped paths and clipboard images; Ctrl-R records a voice message; Ctrl-T sends later. Slash commands (/poll, /search) cover the rest, and ? shows the keys. The mouse clicks, scrolls and opens; in the sidebar a click shows a folder's chats, opens or closes a folder on its mark, or opens or begins someone's chat, as Enter does there, without giving the sidebar the keys.
- **States:** linking (the QR code in half-blocks beside the Mimic, or the phone code), starting jawd, catching up, phone offline, logged out, update needed.
- `jaw demo` draws a made-up account, and `jaw snapshot --demo --width --height --keys` prints one frame as text after some keys, as ooze's do.

## When something fails

- **jawd isn't running:** jaw starts it and says so while it starts; if it can't, it says why.
- **The engine crashes:** jawd restarts it with backoff; interfaces show "reconnecting" and keep everything they have.
- **WhatsApp unreachable:** the header says so, everything local keeps working, sends wait in the outbox with their clocks, and the Mimic sleeps when jawd itself can't be reached.
- **The phone is offline:** messages still arrive and send; media retry and older history wait, and say they're waiting for the phone.
- **The Mac asleep, off or offline at a scheduled time:** within the hour the message goes once jawd runs and WhatsApp can be reached; past it, it's held, and every interface asks whether to send it now, give it a new time or cancel it. Reminders due meanwhile notify once jawd is back.
- **Logged out from the phone:** jawd deletes the session, keeps the messages until you choose, and jaw shows the linking screen.
- **WhatsApp wants a newer version:** "update needed", until jaw is rebuilt on a newer whatsmeow.
- **No ffmpeg:** text and receiving never need it; sending media other than photos says what's missing, and `jawd status` lists it.

## Code

```text
engine/                 Go module github.com/pdcgomes/jaw/engine
  cmd/jaw-engine/
  internal/             link (framing, generated enginepb), wa (whatsmeow: settings, events, commands)
crates/                 Cargo workspace
  wa-proto/             prost types for WhatsApp's protobufs and the engine link
  jaw-proto/            the interface protocol
  jaw-client/           the client side, for every Rust interface
  jawd/                 engine work store convert sync send schedule media voice preview organise presence feed rpc notify agent demo
  jaw/                  the terminal interface
  jaw-mcp/              the MCP server for AI assistants (M9)
proto/                  engine.proto; whatsapp/, copied from whatsmeow by scripts/sync-protos
protocol/               README.md, fixtures/, themes/ (Omarchy's 23, as Omarchy wrote them)
docs/plans/             this design
creatures/  scripts/
Makefile                build, test, install, demo, perf, proto
```

## Testing

- **The engine (Go):** framing; every whatsmeow event it forwards turned into the right frame; commands against a fake client; the ack wait and its timeout.
- **jawd:** a converter test per message kind from hand-made protobufs, nothing from a real account; spans; paging at depth, accent-blind search, migrations from every version; writer priority under load; windows and coalescing checked against fresh queries (property tests); resume, resync and slow clients; the send state machine; small media fixtures (jpg, png, heic, webp, gif, mov, mp4, wav) through probe, plan and transcode; catch-up; a voice round trip; folders, smart folders and the presence limits; scheduled messages and reminders on a clock the tests set, through a sleep across their time, the clock set back and forward, a change of hour under a repeat, a crash either side of the commit at their time, an edit racing it, the hour's grace and a full outbox.
- **An engine contract suite** that every `Engine` runs: the demo engine in CI, the Go engine live on the spare number, whatsapp-rust in M10.
- **Assistants (M9):** an MCP client in the tests drives `jaw-mcp` against `jawd demo`: every tool, each grant's limits refused, a send confirmed, declined, and refused where the assistant's app can't ask, the caps, nothing read or shown online for an assistant, and the tokens and OAuth of the later stages; each stage has a security review before it lands.
- **Interfaces:** fixtures, window models against a reference, anchoring, frames as text at several sizes, keys, and a headless flood test that fails when a frame goes over budget.
- **End to end:** `jawd demo` serving a made-up account through the real socket, driven by `jaw snapshot --keys`.
- **Load:** `jawd demo --flood` and `--catch-up` against a million-message database, and the engine's round trip on its own; `make perf` before each milestone closes.
- **CI:** Linux on every push; macOS on main and by hand, since macOS minutes count ten times on private repositories.

## Milestones

- **M0 Foundations:** this design and the protocol draft; scaffold and CI; the engine link and trait with the demo engine; the spike on a spare number; work lanes with metrics.
- **M1 Text, fast:** store, history, catch-up, windows and fan-out, people, sending text with ticks, typing, presence, read receipts, drafts, search, notifications, the agent; the client crate and the terminal interface; the command palette over chats, people, folders, actions and messages; budgets measured.
- **M2 Everything in a message:** replies, mentions, reactions, edits, deletes, forwards, link previews both ways, polls, events and places sent and received (`send.event`, `send.location`, proposed in the protocol's Composing), locations, contact cards, disappearing and view-once, system and call lines, group senders and members, communities, app state; links kept as they're converted, and findable in the palette.
- **M3 Media in**, with the library of photos, videos, voice messages, files and links, across chats and in one; **M4 Media out**, with contact cards sent (`send.contact`) and, after it, GIF search from Giphy through jawd (`gif.search`, `send.gif`), jaw's only request to a service besides WhatsApp, made only while a GIF picker is open, **M5 Voice**, **M6 Folders and the roster**.
- **Presence and typing in chats,** on its own branch with its own audit, typing now and presence once M6 is on main: who's typing in every chat, groups included, and a one-to-one chat's online and last seen, in every interface (see "Presence and typing in chats").
- **Chat actions from jaw,** now, on its own branch with its own audit: pinning, archiving, muting, marking read or unread and starring from any interface, as the phone does (see "Chat actions from jaw").
- **M7 Scheduled messages and reminders,** on its own branch with its own audit: everything jaw sends, scheduled to send later, once or repeating, sent by jawd at its time and held for you past an hour late; reminders on messages, which notify and mark their chat unread so the phone shows it; and both interfaces' screens to see, edit and cancel them (see "Scheduled messages and reminders").
- **M8 Ready to live with, the first release:** full load runs, every failure state, a README with pictures, `make install` and a Homebrew tap, on the Go engine; before going public, a look at WhatsApp's terms and Meta's rules on side projects.
- **M9 AI assistants over MCP,** after the first release, in three stages, each on its own branch with a security review: assistants on this Mac, then your own devices, then cloud assistants (see "After the first release: AI assistants over MCP").
- **M10 A second engine:** whatsapp-rust wired in as `RustEngine`, run beside the Go engine on the spare number, and compared on the contract suite, the live checklist, the load harness, reconnects and logouts, and build size and time; the default is picked for the release after it.
- **M11 Catching up and asking** (see "After the release: catching up and asking").

## After the first release: AI assistants over MCP

M9, after the first release, as the owner decided on 27 October: Claude, Cursor, ChatGPT and other AI assistants reach jaw through MCP, the Model Context Protocol, to read and find and, where you allow it, to send, so you can ask one about your chats and act on them from wherever you are. Its tools, stages and tests are in [the MCP design](2026-10-27-jaw-mcp-design.md). jaw calls them assistants, since `jaw agent` already names the LaunchAgent.

- **`jaw-mcp`, another interface.** A small Rust binary beside jawd, as jaw-engine is, speaks MCP to an assistant and jaw's protocol to jawd on `jaw.sock`, saying hello as `jaw-mcp` with the grant it serves. jawd itself gains no network listener and no sign-in.
- **In three stages** (Owner, 27 Oct), each on its own branch with a security review: assistants on this Mac, which start `jaw-mcp` and speak to it over stdio; then your own devices, over MCP's Streamable HTTP to `jaw-mcp` listening on 127.0.0.1 behind a private network you run (Tailscale Serve), each device with its own token; then cloud assistants, such as Claude on your phone, over public HTTPS with OAuth as MCP's authorization asks, each connection approved with a one-time code jaw shows.
- **A grant for each assistant** (Owner, 27 Oct), made in jaw and never by an assistant: the folders it sees, none until you pick them, and what it may do beyond reading: leave a draft, react, mark read, schedule, set reminders, send. Read-only by default. jawd enforces it on every request of the connection, so its windows, finds, library and sends reach only the grant's chats.
- **Sends** (Owner, 27 Oct): each confirmed in the assistant's app where it can ask (MCP's elicitation) and otherwise refused, saying why, unless the grant says to send without asking; at most 6 a minute and 60 an hour for each assistant; only to chats the grant reaches, never a new one; each marked in jaw as the assistant's, from the assistants' log, while WhatsApp sees an ordinary message.
- **Never you.** An assistant's connection never has focus: what it reads isn't marked read, it sends no receipts, it doesn't make you online, count as opening a chat or quiet a notification. It marks a chat read only when asked to and its grant allows it.
- **Other people's words.** A message can try to steer an assistant that reads it. The server tells assistants that messages are what people wrote, never instructions, but what bounds the harm is the grant's folders, confirming each send, the caps and no new chats. A grant bounds what an assistant can do through `jaw-mcp`; it isn't a wall against programs running as you on the Mac, which can already read jaw's folder.
- **The data layer.** Grants, their tokens (kept hashed) and the assistants' log are jaw's own, bounded and counted in `jawd status`, so each stage that changes them is audited by the database expert too.

## After the release: catching up and asking

When many unread messages pile up, a catch-up feed says what's going on in each chat and what's worth your attention; and you can ask in plain words, such as "my wife asked me to do something later today, what was it?", and get the answer with the messages it rests on. It's M11, after the release, the assistants and the second engine, and nothing earlier depends on it, but its shape is set now so earlier steps keep the door open.

- **Private by default.** Everything runs on the machine: a local model, local embeddings, nothing sent anywhere. Bringing your own key for a hosted model is a choice made per chat or folder, sends only the snippets retrieved for that question, and shows them before they go. The vectors and notes jaw derives are kept as the messages are, in jaw's folder.
- **Its own process.** Models need gigabytes, so they run in a helper jawd starts when they're needed, as it does ffmpeg, and jawd's own budgets don't change: generation through llama.cpp or MLX on the Mac, and a small multilingual embedding model, since chats mix languages.
- **Vectors in `vectors.db`,** beside `jaw.db` and attached for queries, through sqlite-vec compiled in like FTS5, stored as int8. They're derived and rebuildable, and about as big as the messages, so keeping them apart keeps `jaw.db`'s backups, checks and migrations small. Each vector carries its embedding model's ID, so a new model is a backfill. Exact search should do for a million messages; an in-process HNSW index is the step up if it doesn't.
- **Conversation windows, not single messages:** a chat is cut at gaps in time and changes of speaker, so "ok" isn't embedded alone; long messages are embedded on their own too.
- **Retrieval that mixes kinds:** the question's people are resolved through contacts, folders and relationships you can tell jaw ("my wife" is a person); its times become filters ("later today"); FTS5 and the vectors both search within those filters and their results are merged; the model answers from what they found and cites each message.
- **Work done ahead,** in the background lane, never ahead of a live message: media turned into text (voice transcribed, images described and their text read, each optional), what's asked of you, dates, plans and decisions pulled out as notes that point to their messages, and summaries kept per chat until new messages arrive, so the catch-up feed opens at once.
- **To measure first:** which local models summarise busy group chats well enough on an M-series Mac, how fast and in how much memory, and how fast history embeds; the feature only promises what the measurements show.
- **In the protocol:** `catch_up.watch`, a window of chats with their summaries and what needs you, and `ask`, whose answer streams in with its citations; `jaw-mcp` offers both to assistants, within their grants.

## Later

- A spike on the message box and a focused mode, tried against today's layout to see whether it confuses which chat you're sending to:
  - **The box full width** along the bottom of the screen, under the sidebar and chat list too, as Claude's is, since sending is the main job.
  - **A status line beside it**, as BitchX and the Claude command line keep one: the chat you're in, its state (typing, online, muted, catching up), and what needs you elsewhere (unread and mentions in other chats, named when they matter), always on screen.
  - **A focused mode** for one chat, the chat list and sidebar folded away, with the status line as the only glimpse of the rest.
  - **A quick look:** a palette action (and a key on the status line) that shows a short catch-up of what's going on in the chats that need you, without leaving the one you're in; a small version of the catch-up in "After the release".
- **Split panes**, as tmux has them, for several chats at once: split any pane across or down, choose the chat each pane shows (one chat per pane), swap it, close the pane; a key straight to each pane and to each chat, so hopping between them is instant; each chat's message box keeps what you were typing, since drafts already live in jawd per chat. The layout belongs to the interface, but jawd keeps it as an interface's saved state, so a desktop app can offer the same panes and open where you left off. Built behind the `panes` switch: jawd keeps each interface's layout in `interface_state` (`layout.get`, `layout.set` and the `layout` push, in the protocol's "Layouts"), each interface's own and any interface able to open another's, and the terminal opens with the panes it was left with.
- **One message to several chats:** a palette action with a fuzzy multi-select of chats, then the message, sent as an ordinary message to each (one outbox entry per chat, each with its own ticks); "good morning!" to every family group. WhatsApp watches for bulk sending, so it's paced, capped per minute, and shows how many chats it will reach before sending.
- Sending stickers; group admin; several numbers; video inside the terminal (mpv's kitty output); voice messages transcribed on the machine.
- A desktop app on the same socket.
- Encrypting `jaw.db` with SQLCipher.
- Clicking a notification to open its chat.
- WhatsApp's own scheduled messages and reminders: showing the scheduled messages others send you, which arrive hidden until their time, once whatsmeow can reveal them (until then they're kept raw, as every kind jaw can't show yet is); syncing reminders with the phone once WhatsApp's protobufs carry them; and waking a sleeping Mac for a scheduled message, which only an administrator's scheduled wake (`pmset`) can do.
- Assistants sending photos, files and voice messages, and waiting for a reply in a chat.

## Risks

- Unofficial clients can get numbers banned: a spare number for development, and nothing automated. A scheduled or repeating message is one you wrote, sent at its time, at most daily and to one chat; an assistant sends only when asked, to chats its grant reaches, paced and capped.
- A scheduled message needs jawd running and WhatsApp reachable at its time: jawd keeps the Mac from idling to sleep before it, and holds one that's more than an hour late for you rather than sending it out of its moment.
- An assistant reads other people's words, which can try to steer it, and from M9's second stage jaw can be reached from beyond the Mac: the grant's folders, confirming each send, the caps and no new chats; a token for each device and OAuth for cloud assistants; and a security review of each stage.
- whatsmeow tracks WhatsApp's changes: pin it, update often, keep `proto/whatsapp/` at the same commit, and show "update needed" when WhatsApp says so.
- The engine link is a hop on every message: measured in M0, and gone if whatsapp-rust wins in M10.
- whatsapp-rust is at 0.7, with breaking changes landing often: pinned to a commit, and the default only if M10 says so.
- Sixel and iTerm2 resend image data when an image moves: list images stay small there.
- macOS ties microphone permission to jawd's binary: `make install` signs it with a stable identity, so it isn't asked again after each build.
