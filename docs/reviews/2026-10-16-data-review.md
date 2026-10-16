# jaw's data layer: a review

16 October 2026. A review of `jaw.db`, `session.db`, the spool and the paths between them, against the design's seventh principle: nothing is lost, the data is never inconsistent, and anything that makes data access slow at times or opens an edge case is a defect.

What was reviewed: `main` at `8eceeea` (the store, the feed and find, which merged while this review ran; the store was first read at `7ea1809`), the sync builder's worktree `~/src/jaw-sync` at `133c93f` plus its uncommitted changes, `engine/internal/wa/engine.go`, the design and `protocol/README.md`. Measurements ran in scratch copies under `/tmp`, on a Mac Studio (M1 Ultra, internal SSD, APFS, macOS 26.6.2) with the SQLite that rusqlite 0.40.2 bundles, 3.53.2. Nothing in the repository or the owner's folder was changed, apart from adding this file.

## 1. Summary

The data layer is well shaped where it matters most for speed: one writer thread with prioritised queues, keyset paging that costs the same at any depth, each change writing its event in the same transaction, read-only readers that never hold a transaction across an await, FTS5 pinned as the outer loop of search, foreign keys on, and a migration runner that refuses a newer database. Reads are far inside their budgets. But durability and consistency don't yet meet principle 7. The ack WhatsApp receives is not backed by a flush to disk; on macOS, the default settings can corrupt a database on power loss, not only lose its last commits; the WAL grows to ten times the database during a history sync and never shrinks; and several paths can show a message twice, resurrect one that was deleted, regress its state, or leave an acked message invisible until a restart. None of these is hard to fix, but the first one forces a decision from the owner, because durable acks cost about 5 ms each on this Mac, and whatsmeow takes messages one at a time.

The findings that put data at risk, most serious first:

1. **An acked message can be lost.** At `synchronous=NORMAL` the inbox commit is not flushed before jawd acks, so a power cut or kernel panic after the ack loses a message WhatsApp considers delivered to this device; it isn't sent again. On macOS even `synchronous=FULL` only hands the data to the drive's cache; only `F_FULLFSYNC` makes it durable, and it costs 4.0 ms p50, 7.1 ms p99. Measured end to end through the Go engine, a durable ack takes the round trip from 88 µs to 5.0 ms and a backlog from 10,300 to 204 messages a second. (D1)
2. **A power cut on macOS can corrupt `jaw.db` and `session.db`.** Checkpoints sync with plain `fsync`, so the WAL can be reset and overwritten while the pages copied into the database are still in the drive's cache. `PRAGMA checkpoint_fullfsync=ON` fixes it at no measurable cost to commits. (D2)
3. **The WAL grows without bound while history pours in.** It reached 5.8 GB for a 550 MB database on main's bench and 7.1 GB on the sync worktree's, because PASSIVE checkpoints never let the writer restart the WAL while it keeps appending. Nothing ever truncates it, a crash makes the next start replay it (3.5 s for a 10.6 GB WAL), and it can fill the disk. (D3)
4. **`session.db` can fall behind what WhatsApp thinks.** It runs at `NORMAL` without `checkpoint_fullfsync` or immediate transactions, whatsmeow advances app-state versions before jawd stores their events, and receipts aren't kept before whatsmeow acks them. (D4, D5)
5. **The sync worktree's converter loses track of the inbox.** Once the converter drains the inbox, the next inbox row reuses seq 1, below the converter's cursor, so an acked, committed message isn't converted or shown until jawd restarts. (D6)
6. **Two branches each define migration 3.** Main's adds `chats.message_count`; the sync worktree's adds `fts_backlog`, new triggers and four other changes. A database opened by one and then the other is at `user_version` 3 with the wrong schema, and nothing detects it. A table rebuild in a later migration would also silently drop the FTS triggers; I reproduced that, and FTS5's own integrity check still says "ok". (D9, D10)
7. **Replays make state inconsistent.** One conversation can become two chats (phone-number and LID JIDs), a history copy overwrites newer live state, "delete for me" is undone by the next replay, and counters kept by hand drift between code paths. (I1 to I5)

Section 2 lists every finding; section 3 gives the configuration to use, section 4 the plans, section 5 a migration strategy that can go into the design as it is, section 6 the rules for builders, section 7 the measurements, and section 8 the decisions only the owner can make.

## 2. Findings

Severity is one of: **data loss**, **inconsistency**, **unpredictable performance**, **hygiene**. Locations are on `main` at `8eceeea` unless the sync worktree (`jaw-sync`) is named.

### Data loss

#### D1. Acks come before durability

- **Where:** `crates/jawd/src/store/mod.rs:103` (`synchronous=NORMAL`), `:396-410` (insert, then ack); `engine/internal/wa/engine.go:83, 96`; the design's Storage bullet "a power cut can lose the last moments, which the phone still has".
- **What happens:** with `SynchronousAck`, whatsmeow sends the ack and the delivery receipt only after the handler returns, and the handler returns when jawd says the inbox row is committed. In WAL mode at `NORMAL`, a commit writes the WAL with `write()` and no sync; the data is in the OS page cache. A jawd or engine crash loses nothing, as the design says. A power cut or kernel panic loses every commit since the last checkpoint's sync, up to a second of them with the current checkpointer. WhatsApp won't send those messages again, since they were acked. The phone has them, but jaw has no reliable way to ask for exactly the lost ones: on-demand history fetches messages older than an anchor, so it can fill a gap before a later message but not the tail of a chat. The design's sentence is therefore wrong.
- **macOS specifics:** `fsync()` on macOS moves data to the drive but doesn't make the drive write it out; Apple's man page says so, and the measurement agrees (a 1 KiB append plus `fsync` takes 49 µs p50). `PRAGMA fullfsync=ON` makes SQLite use `fcntl(F_FULLFSYNC)` instead, which flushes the drive's whole cache: 4.0 ms p50, 7.1 ms p99 on this Mac. `F_BARRIERFSYNC` (0.3 ms) only orders writes; it doesn't make an ack safe. `PRAGMA checkpoint_fullfsync` governs only the syncs a checkpoint does (see D2). On Linux, `synchronous=FULL` issues an `fsync` that flushes the device cache; its cost depends on the drive and wasn't measured here.
- **Measured, through the real Go engine (`jawd bench link --commit`, 600-byte messages):**

  | jaw.db inbox commit | Round trip p50 | p99 | Messages a second |
  | --- | --- | --- | --- |
  | `NORMAL` (today) | 88 µs | 225 µs | 10,304 |
  | `FULL` (survives a kernel panic, not a power cut, on macOS) | 144 µs | 379 µs | 6,161 |
  | `FULL` + `fullfsync` (survives both) | 5,003 µs | 8,167 µs | 204 |

- **Why batching doesn't help as things stand:** whatsmeow handles incoming nodes on one goroutine, and with `SynchronousAck` it acks a message only after its handler returns. Only one message is ever waiting for jawd, so there is nothing to group: one flush per message, and the backlog drains at one flush per message.
- **The options:**
  - **A. Durable acks now.** Set `synchronous=FULL` and, on macOS, `fullfsync=ON` for the transactions that precede an acknowledgement (inbox commits, outbox enqueues, user actions an interface is told are done, the commit that drains a history blob), and keep `NORMAL` for conversions and background work. `synchronous` can be changed between transactions on the same connection; syncing the WAL at an inbox commit also makes every earlier unsynced commit durable, since they're earlier in the same file. Measured: an inbox commit at `FULL` after a 200-row background transaction at `NORMAL` takes 0.32 ms p50 and 2.0 ms p99 with plain `fsync`, and 6.2 ms p50 and 9.3 ms p99 with `F_FULLFSYNC`. Cost: catch-up after time offline drops to about 200 messages a second on this Mac, so 5,000 waiting messages take about 25 s instead of half a second, and the "5,000 messages a second" and "200 µs" budgets can't hold.
  - **B. Durable acks without the slowdown: deferred acks and group commit.** Patch whatsmeow (a small fork, or an upstream "manual ack" option) so the handler gets an ack function instead of whatsmeow acking when it returns, and so the decrypted-event buffer's plaintext is cleared only when that function runs. The engine then forwards each message at once; jawd commits it at `NORMAL`, pushes it to interfaces, and every 2 ms or so commits once at `FULL`+`fullfsync` and releases the acks of everything committed before it. Throughput becomes the decrypt rate; an ack waits about one group interval plus one flush (6 to 10 ms); the message is on screen in under a millisecond. A crash before the ack means WhatsApp sends the message again and whatsmeow finds the plaintext still buffered, so it isn't decrypted twice. One device flush also covers `session.db`, as long as `session.db` runs at `FULL` (its commits reach the drive before jaw.db's `F_FULLFSYNC` flushes the drive's cache).
  - **C. Keep acks fast and accept the window.** `FULL` with plain `fsync` on macOS (+56 µs p50) survives crashes of jawd, the engine and the OS, but not a power cut. That doesn't meet principle 7 on a desktop Mac.
- **Recommendation:** A now, since it is two PRAGMAs and makes the principle true today, and B before release if the catch-up rate under A is unacceptable on the spare number. Either way, restate the budgets: under A the commit-acked round trip is 5 ms p50, not 200 µs. This is open question 1.

#### D2. On macOS, a power cut can corrupt the database, not only lose its last commits

- **Where:** every connection to `jaw.db` (`mod.rs:101-114`, `read.rs:52-61`) and `session.db` (`engine.go:80-84`); none sets `checkpoint_fullfsync`.
- **What happens:** a checkpoint syncs the WAL, copies its pages into the database file, syncs the database, and later the writer restarts the WAL from its start. On macOS those syncs are plain `fsync`, which doesn't order or persist anything in the drive's cache. After a power cut the drive may hold the new WAL frames but not the database pages the checkpoint wrote, so commits are lost and b-tree pages from different moments can sit side by side: "database disk image is malformed". SQLite's "How To Corrupt An SQLite Database File" lists drives that don't honour sync as a cause, and this is why `checkpoint_fullfsync` exists.
- **Measured:** with a checkpointer every 250 ms (four times today's rate) and `checkpoint_fullfsync=ON`, inbox commits at `NORMAL` took 0.049 ms p50, 0.20 ms p99 and 0.62 ms max, against 0.043, 0.17 and 4.4 ms without it. The flush happens on the checkpointer's thread.
- **Recommendation:** `PRAGMA checkpoint_fullfsync=ON` on every connection to `jaw.db` and `session.db`, since whichever connection closes last runs a checkpoint. For `session.db` that needs a `ConnectHook` on a registered driver, since mattn's DSN has no parameter for it (see D4). It's needed whatever is decided for D1.

#### D3. The WAL grows without bound during a history sync, and never shrinks

- **Where:** `mod.rs:381-394` (a PASSIVE checkpoint once a second), `mod.rs:211` (`wal_autocheckpoint=0`), no `journal_size_limit` anywhere.
- **What happens:** a WAL is restarted from its beginning only when a writer begins a transaction at a moment when every frame has been checkpointed and no reader needs the old ones. While the writer commits back to back, each PASSIVE pass copies the frames that existed when it started, the writer appends more meanwhile, and that moment never comes, so the WAL keeps growing. Since the WAL file is reused rather than shrunk, and `journal_size_limit` is unset, the file stays at its peak size forever. Checkpointing on APFS also costs about 30 µs a frame here, so the checkpointer can't keep up with history.
- **Measured:**
  - `jawd bench store`, 1,000,000 messages on main: the WAL peaked at 5,765 MB for a 550 MB database. On the sync worktree: 7,115 MB for 561 MB. The bench's final `TRUNCATE` hides this; jawd never does one.
  - A synthetic writer committing 200-row transactions back to back with a PASSIVE checkpoint every second: 3.8 GB of WAL in 20 s. With `journal_size_limit=64 MiB` alone: 4.35 GB, since the limit applies only at a restart, and none happens.
  - A checkpoint's cost by frames waiting: 257 frames 8.7 ms, 1,999 frames 60 ms, 4,974 frames 126 ms, 10,039 frames 187 ms.
  - Recovery after a crash with a 10.6 GB WAL: 3.5 s to open and read the first row (the WAL index is rebuilt by reading the whole WAL), then 4.8 s for a `TRUNCATE` checkpoint.
  - Two ways of forcing a restart that don't work: a `RESTART` checkpoint from the checkpointer, which waits on the writer through the busy handler's back-off sleeps (61 ms p50 per checkpoint, with inbox commits stalled to 38 ms p99 and 149 ms max), and the writer running a PASSIVE checkpoint itself once the checkpointer is close (WAL bounded at 67 MB, but each checkpoint took 71 ms p50 on the writer's thread).
- **Recommendation:** bound the WAL by making history wait for the checkpointer, never the inbox.
  1. `PRAGMA journal_size_limit=67108864` on the writer, so each restart truncates the file to 64 MiB.
  2. The checkpointer runs PASSIVE continuously while frames are waiting (not once a second) and publishes `(log, checkpointed)` in an atomic after each pass.
  3. When the WAL passes a soft limit (16,384 frames, 64 MiB), the writer stops starting background transactions; inbox and live writes go on. When the checkpointer reports fewer than about 64 frames left, the writer runs one PASSIVE checkpoint itself between transactions; with so few frames it takes 1 to 2 ms. The writer's next transaction then restarts the WAL.
  4. A hard limit (256 MiB) at which background work waits until the WAL has restarted, and `jawd status` reporting WAL size, frames waiting and how long background waited.
  5. A `TRUNCATE` checkpoint once jawd has been idle for a while, and at clean shutdown.

  History ingestion then runs at the speed of the checkpointer (about 130 MB of page writes a second here) instead of 15,000 to 25,000 messages a second, and disk use stays bounded. The writer's own short checkpoint must be counted in the live budget; the test is the live-write p99 in `jawd bench store` while history pours in.

#### D4. `session.db` can fall behind what WhatsApp thinks

- **Where:** `engine.go:80-84` (`_synchronous=NORMAL`, no `_txlock`), `engine.go:96-107`.
- **What happens:**
  - Losing the tail of `session.db` in a power cut rolls back Signal state. On the receiving side, whatsmeow's retry receipts usually recover. On the sending side, a rolled-back sender chain reuses message counters, and recipients reject those messages as duplicates; they ask for a retry, which whatsmeow can serve only if it still has the message. It keeps the last 256 sent messages in memory unless `UseRetryMessageStore` is on or `GetMessageForRetry` is set, and neither is.
  - D2's corruption risk applies to `session.db` too.
  - whatsmeow stores a new app-state version and its mutation MACs in `session.db` before it dispatches the events (`appstate.go`: `fetchAppState`, then `dispatchEvent` with the result ignored). If jawd or the engine dies in between, those pins, archives, mutes, read marks and stars are never sent again. The sync worktree's `keep()` (`jaw-sync engine.go:169`) waits for jawd's ack, but the version has already moved.
  - mattn's driver opens `BEGIN DEFERRED` transactions by default. whatsmeow writes from several goroutines through a connection pool, and a deferred transaction that reads and then writes can fail at once with `SQLITE_BUSY_SNAPSHOT`, which the busy timeout doesn't cover.
- **Recommendation:**
  - DSN: `_journal_mode=WAL&_synchronous=FULL&_foreign_keys=on&_busy_timeout=5000&_txlock=immediate`.
  - A registered driver with a `ConnectHook` that runs `PRAGMA checkpoint_fullfsync=ON; PRAGMA journal_size_limit=16777216;` (and `fullfsync=ON` only if session.db must be durable on its own; under D1's option A or B, jaw.db's flush covers it).
  - `client.UseRetryMessageStore = true`.
  - After any unclean shutdown of jawd or the engine (a "clean" marker in jaw.db, cleared at start and set at shutdown), ask for a full app-state resync (`FetchAppState(name, fullSync=true)` for each collection, which the sync worktree's `appStateResync` command already does) and apply it idempotently.
  - The engine takes a lock file for `session.db` (see P9) and backs it up before whatsmeow upgrades it (section 5).

#### D5. Receipts are acked before jawd keeps them

- **Where:** `engine.go:176-177` (main) and `jaw-sync engine.go:200`: `e.send`, not `e.keep`.
- **What happens:** with `SynchronousAck`, whatsmeow acks a receipt node when the handler returns, and the handler returns as soon as the frame is written to the pipe. A crash before jawd commits it loses the receipt, so a message stays at "sent" or "delivered" forever.
- **Recommendation:** `keep()` receipts as messages are kept. In large groups this adds a round trip per receipt node, which is acceptable at today's 88 µs and is batched for free under D1's option B.

#### D6. The sync worktree's converter skips inbox rows once the inbox has drained (sync worktree)

- **Where:** `jaw-sync crates/jawd/src/sync/drain.rs:159-230` (reads `seq > after` and advances `after`), `jaw-sync store/write.rs:323-324` (`DELETE FROM inbox WHERE seq = ?`), and the inbox's `seq INTEGER PRIMARY KEY` from migration 1 (`schema.rs:10-15`).
- **What happens:** without `AUTOINCREMENT`, SQLite gives a new row the largest rowid plus one, or 1 if the table is empty. Once the converter has converted and deleted every row, the next message's inbox row gets seq 1, while the converter waits for `seq > after`, where `after` is the last seq it converted. The message is committed and acked but isn't converted, shown or notified until jawd restarts and `after` starts again from 0.
- **Reproduce:** `CREATE TABLE inbox (seq INTEGER PRIMARY KEY, kind TEXT); INSERT INTO inbox (kind) VALUES ('a'), ('b'), ('c'); DELETE FROM inbox WHERE seq <= 3; INSERT INTO inbox (kind) VALUES ('d'); SELECT seq FROM inbox;` returns 1. In jawd: start with an empty inbox, let three live messages convert, send a fourth, and it doesn't appear.
- **Recommendation:** make `inbox.seq` `INTEGER PRIMARY KEY AUTOINCREMENT` (a rebuild of a small table: create, copy, drop, rename, then set `sqlite_sequence` to the old maximum), and test "drain to empty, then one more". The same reuse applies to `messages.n` and `people.n` (I12).

#### D7. A failed inbox commit drops the message's payload and stalls the engine silently

- **Where:** `mod.rs:396-410`.
- **What happens:** if the insert fails (`SQLITE_FULL`, `SQLITE_IOERR`, a locked database), the payload is dropped with a log line, and the ack is dropped unsent. Nothing is lost (WhatsApp keeps the message), but the engine's handler waits for that ack until jawd restarts the engine (whatsmeow only moves on in the background after five minutes), nothing tells interfaces, and history and media keep competing for the disk that just filled.
- **Recommendation:** keep the payload and retry the commit with back-off (10 ms doubling to 1 s), surface a `connection` or `notice` state ("jaw's disk is full"), pause background lanes and downloads while the inbox can't commit, and add a disk watchdog (section 2, P11).

#### D8. History blobs: the spool file can go before its rows are durable

- **Where:** `engine.go:448-457` (`writeWhole`: write, rename, no sync); `sync.rs:242` on main and `jaw-sync sync/fetch.rs:132-137` (the spool file is removed right after the commit that drains its notification).
- **What happens:** the sync worktree already keeps the notification's inbox row until the blob's rows are in and reuses a blob already in the spool, which is right. But the drain commit is at `NORMAL`, so a power cut can roll it back after the file is gone; the notification comes back, the blob has to be downloaded again, and history from before jaw was linked is often gone from WhatsApp's servers. After a power cut, the renamed file can also exist with missing contents, since APFS may persist the rename before the data; the sync worktree handles a blob that doesn't decode by fetching it again, which is good.
- **Recommendation:** the engine syncs the `.part` file before renaming it and syncs the spool directory after. jawd deletes a spool file only after a synced commit (D1's durable class), or leaves deletion to a startup sweep that removes spool files whose notification is no longer in the inbox.

#### D9. Migration 3 exists twice

- **Where:** `schema.rs:252-257` on main (`chats.message_count`); `jaw-sync crates/jawd/src/store/schema.rs:252-295` (`fts_backlog`, new FTS triggers, the `sort` column rebuilt, `avatar_id`, `reactions.mine`, three `pending` columns, `sync_state`, `app_state`).
- **What happens:** whichever merges second must become migration 4. Any database a `sync-v1` build has opened (benches, tests, or the owner's own `jaw.db` if a sync-v1 jawd ever ran on it) is at `user_version` 3 with the sync schema. Main's jawd then skips its migration 3 and fails on the first query that names `message_count`; after renumbering, a jawd with both would skip main's migration 3 (the version says it ran), then run the sync migration again and refuse to start on `CREATE TABLE fts_backlog`. The opposite order, a main database opened by a sync build, skips the sync migration and fails on the first query that names `fts_backlog`. Nothing checks that the schema matches the version.
- **Recommendation:** renumber at merge; add the frozen-migration and schema-fingerprint checks in section 5, which turn this class of mistake into a refusal with a clear message; and check whether the owner's `jaw.db` was ever opened by a sync-v1 build (open question 2).

#### D10. A table rebuild silently drops the FTS triggers, and FTS5's integrity check doesn't notice

- **Where:** the migration runner (`schema.rs:260-276`) has no post-migration checks; the triggers are at `schema.rs:115-124`.
- **What happens:** SQLite's documented way to change a column's type or constraints is to create a new table, copy, drop the old one and rename. Dropping `messages` drops its triggers. I ran that rebuild on a copy of the 1,000,000-message database: it took 5.8 s, the file grew from 558 to 941 MB, the three `messages_fts_*` triggers were gone, and `INSERT INTO messages_fts (messages_fts, rank) VALUES ('integrity-check', 1)` still passed, because the rows hadn't changed yet. From then on, new messages wouldn't be searchable and deleted ones would stay in the index.
- **Recommendation:** the schema fingerprint check after every migration (section 5) catches this; a migration test asserts the trigger list; rebuilds follow the procedure in section 5, which recreates triggers.

### Inconsistency

#### I1. One conversation can become two chats

- **Where:** `write.rs:287` (`ensure_chat` makes a chat for any JID); `messages_by_id` is unique on `(chat, id)` (`schema.rs:103`); `pending.chat` and `outbox.chat` are JID strings (`schema.rs:217-239`).
- **What happens:** WhatsApp is moving one-to-one chats to LID addressing, so the same conversation arrives as `351…@s.whatsapp.net` from history and `1000…@lid` live, or the other way round. Each JID gets its own chat row; the same message can then be stored once in each, since uniqueness is per chat; and `person_jids` joining the two people doesn't merge their chats (`join`, `write.rs:447`). The sync worktree's `web_message` (`jaw-sync convert/mod.rs:70-78`) maps a LID chat to its phone number when `RecipientAlt` or `SenderAlt` carries one, which helps live messages, but history and messages without an alternate JID still split.
- **Recommendation:** a `chat_jids (jid TEXT PRIMARY KEY, chat INTEGER NOT NULL REFERENCES chats (n)) STRICT, WITHOUT ROWID` alias table, as `person_jids` is for people; resolve every incoming JID through it (and whatsmeow's LID map) to one canonical chat; key `pending`, `outbox` and anything else that names a chat by its `n`; and a merge procedure for when a LID and a phone number are found to be one person: move messages from the gone chat to the kept one in background batches, and where both chats hold the same message ID, keep one by the merge rules in I2.

#### I2. A history copy overwrites newer live state

- **Where:** `write.rs:494-575` on main replaces the whole row ("The whole row is replaced each time", `write.rs` `Message` doc); the sync worktree (`jaw-sync write.rs:638-763`) keeps an edit, a deletion and a star, but still overwrites `status`, `at`, `raw`, `detail` and `reply`.
- **What happens:** history and live overlap for minutes after linking and after every catch-up. A history copy of your own message says `DELIVERY_ACK` when receipts have already made it `read`, so its ticks go backwards; its `at` has second resolution, so it moves within its chat; and its `raw` replaces the live envelope.
- **Recommendation:** per-field merge rules, applied in SQL so they hold on every path: `status` only advances (pending < sent < delivered < read < played; `failed` only replaces `pending`); `edited_at` and the edited text win by `edited_at`; `deleted` and a for-me tombstone are sticky (I3); `at` prefers the server's millisecond timestamp over history's seconds; `raw` keeps the live envelope over history's `WebMessageInfo`. For example: `status = CASE WHEN rank(?5) > rank(status) THEN ?5 ELSE status END`, with `rank` as a `CASE` expression.

#### I3. "Delete for me" is undone by the next replay

- **Where:** `write.rs:235-259` on main (`delete_message`) and `jaw-sync write.rs:879-896` (`DeleteForMe`) delete the row.
- **What happens:** once the row is gone, a history chunk or a redelivered message carrying it finds nothing to update and inserts it again.
- **Recommendation:** keep a tombstone: either a `hidden` flag on the row (text, spans, raw and FTS entry cleared) or a `tombstones (chat INTEGER, id TEXT, at INTEGER, PRIMARY KEY (chat, id)) STRICT, WITHOUT ROWID` table that `upsert_message` checks first. Tombstones can be pruned once no history can bring the message back (owner question 8).

#### I4. Counters kept by hand drift

- **Where:** `message_count` is changed in `upsert_message` (`write.rs:560`) and `delete_message` (`write.rs:244`) on main; the sync worktree's `DeleteForMe` (`jaw-sync write.rs:880`) deletes without decrementing it, and once both merge, the count drifts. `unread` and `mentions` go up by one per new live message from others and to zero on one of yours (`jaw-sync write.rs:743-755`); deleting an unread message, an edit that adds a mention, or a message arriving live after its history copy aren't counted.
- **Recommendation:** maintain `message_count` with triggers, so every path is covered:

  ```sql
  CREATE TRIGGER messages_count_insert AFTER INSERT ON messages BEGIN
      UPDATE chats SET message_count = message_count + 1 WHERE n = new.chat;
  END;
  CREATE TRIGGER messages_count_delete AFTER DELETE ON messages BEGIN
      UPDATE chats SET message_count = message_count - 1 WHERE n = old.chat;
  END;
  ```

  For unread, keep a read position per chat (`read_through_at`, `read_through_n`, set from your own messages, read receipts you send and the phone's mark-read) and derive `unread` as the count of others' messages after it, capped at 1,000 (0.05 ms measured for a capped count), recomputed in the transaction that changes either side. Mentions the same way, from a `mentions_me` column. A background check compares stored and derived values and logs any drift.

#### I5. One bad change drops the rest of its batch, and the converter moves past it

- **Where:** `mod.rs` `background_changes` and `transaction` (an error rolls back the transaction and returns, with earlier transactions committed); `jaw-sync sync/drain.rs:221-229` (an error is logged and `after` has already advanced).
- **What happens:** one change that fails (the "a million messages in one millisecond" bail in `allocate`, a constraint, `SQLITE_FULL`) rolls back its transaction, and the rest of the batch is never attempted. The inbox rows stay but the converter's cursor has moved on, so they wait until a restart, where the same change fails again and takes its batch-mates with it each time.
- **Recommendation:** run each change in a `SAVEPOINT`; on error, roll back to it, record the change in a `quarantine (n INTEGER PRIMARY KEY, at INTEGER NOT NULL, source TEXT NOT NULL, ref TEXT NOT NULL, error TEXT NOT NULL) STRICT` table and go on; the converter doesn't advance its cursor past a row it didn't drain; `jawd status` shows the quarantine's size.

#### I6. The feed can develop holes, and a replaced database can look like an old one

- **Where:** `write.rs:580-588` (prune by wall-clock `at`); `feed/client.rs` (the check that `since` lies between the oldest and the last seq).
- **What happens:** events are pruned where `at` is older than 7 days. If the clock was set back, a later event can have an earlier `at` than an earlier one, so pruning removes a seq in the middle while older seqs stay; a client resuming from before that hole is told it can resume and misses a change. Separately, if `jaw.db` is replaced (restored, or recreated after a logout and re-link), seqs start again; once they pass a client's old `since`, its stale rows look current.
- **Recommendation:** prune a prefix by seq (`DELETE FROM events WHERE seq < ?floor LIMIT …`, with the floor found as the first seq whose `at` is recent, never moving backwards); and a random `database_id` written at creation into a `meta` table, sent in `hello`, so a client drops `since` when it changes.

#### I7. One malformed JSON column breaks a whole window

- **Where:** `read.rs:336-342` and `:372-373`: `RawValue::from_string` fails on invalid JSON, and the page's `collect::<Result<_>>()` fails with it.
- **What happens:** one row with bad `spans`, `reply` or `detail` (a converter bug, a migration) makes every page and search that includes it fail.
- **Recommendation:** `CHECK (spans IS NULL OR json_valid(spans))` and the same for `reply` and `detail`, so bad JSON can't be written; and a reader that logs a bad value and shows the row without it rather than failing the page.

#### I8. Column types aren't enforced

- **Where:** every table in `schema.rs`; none is `STRICT`.
- **What happens:** SQLite stores what it's given. A bug binding a string to `at` or `t` stores text, and text sorts after every integer, so that message pages, sorts and searches in the wrong place with no error.
- **Recommendation:** `STRICT` on every new table and on every table a migration rebuilds; `CHECK` for invariants that never change (booleans in `(0, 1)`, `at >= 0`, `seq > 0`), but not for enumerations that will grow (kinds, states), since changing a `CHECK` means a rebuild.

#### I9. Folders can form a cycle, and favourites can't be chats

- **Where:** `schema.rs:194-215`.
- **What happens:** `folders.parent` can be set to a descendant, making a loop that recursive queries never leave. `parent … ON DELETE CASCADE` deletes a whole subtree and its items, jaw-only data that can't be rebuilt. `favourite` is a column on `folders`, but the design puts chats on Alt-1 to Alt-9 too.
- **Recommendation:**

  ```sql
  CREATE TRIGGER folders_no_cycle BEFORE UPDATE OF parent ON folders
  WHEN new.parent IS NOT NULL AND EXISTS (
      WITH RECURSIVE up (n) AS (
          SELECT new.parent
          UNION SELECT f.parent FROM folders f JOIN up ON f.n = up.n WHERE f.parent IS NOT NULL)
      SELECT 1 FROM up WHERE n = new.n)
  BEGIN SELECT RAISE(ABORT, 'a folder can''t go inside itself'); END;

  CREATE TABLE favourites (
      slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 9),
      folder INTEGER REFERENCES folders (n) ON DELETE CASCADE,
      chat INTEGER REFERENCES chats (n) ON DELETE CASCADE,
      CHECK ((folder IS NULL) <> (chat IS NULL))
  ) STRICT;
  ```

  and `ON DELETE RESTRICT` on `parent`, so deleting a folder with children is a deliberate step in code (open question 6 covers backing up jaw-only data).

#### I10. Some JIDs keep their device

- **Where:** `write.rs:262` (`user_jid` strips the device only for `s.whatsapp.net` and `lid`).
- **What happens:** whatsmeow also knows `hosted` and `hosted.lid` users; their device suffix stays, so each device becomes a person of its own.
- **Recommendation:** strip the device for every user server (`s.whatsapp.net`, `lid`, `hosted`, `hosted.lid`), with a test.

#### I11. "The raw protobufs are the source of truth" isn't true yet

- **Where:** the design's Storage bullet; `jaw-sync convert/mod.rs:70-95` turns a live message into a `WebMessageInfo`.
- **What happens:** the stored raw keeps the message and its key but loses `MessageInfo` fields (`SenderAlt`, `RecipientAlt`, addressing mode, server ID, edit attribute, retry count) and the engine's `Decrypted` payloads for poll votes and encrypted reactions, which can't always be decrypted again. History kept raw is "first copy wins" (`INSERT OR IGNORE`, `mod.rs` `history`). And much of what jaw shows was never in a message: app state (pins, archive, mute, read marks, stars), receipts, group metadata, push names, the LID map, avatars, and everything that is jaw's own (folders, favourites, frecency, drafts, the outbox, media state).
- **Recommendation:** store the engine's whole envelope (`MessageInfo`, the message, and `Decrypted`) as the raw of a live message, versioned, and write down per table which data is rebuildable from raw, which can be fetched again from WhatsApp, and which is jaw's own and must never be dropped or rebuilt (section 5 gives the classes).

#### I12. Row numbers are reused

- **Where:** `messages.n`, `people.n` and `inbox.seq` are `INTEGER PRIMARY KEY` without `AUTOINCREMENT` (`events.seq` is safe, since jawd assigns it and pruning always keeps the newest); the schema's comment says "n only goes up" (`schema.rs:75`); the sync worktree chooses `max(n) + 1` ahead of an insert (`jaw-sync write.rs:682`).
- **What happens:** deleting the newest row lets its number be used again. D6 is where that loses something; elsewhere a feed event names an `n` that now means another row.
- **Recommendation:** `AUTOINCREMENT` on `inbox`, `messages` and `people` when they're next rebuilt (its cost is one `sqlite_sequence` row update per insert), or correct the comment and never key anything durable by them.

### Unpredictable performance

#### P1. Live writes wait behind FTS5 work during history

- **Where:** `mod.rs` writer; the sync worktree's `fts_backlog` and `index()` (`jaw-sync store/mod.rs:359`, `write.rs:364`).
- **Measured (live message, hand-over to commit, one every 5 ms during the 1,000,000-message bench):** main: 3.70 ms p50, 23.7 ms p99, 175 ms max. Sync worktree: 1.03 ms p50, 6.3 ms p99, 105 ms max while history went in, and 21.2 ms p99, 45 ms max during the 28 s it then spent indexing. Much of the remaining tail is FTS5 merging segments inside whichever commit crosses its `automerge` threshold, and checkpoint I/O on a WAL of several GB (D3).
- **Recommendation:** turn off automatic merging and merge in small background steps:

  ```sql
  INSERT INTO messages_fts (messages_fts, rank) VALUES ('automerge', 0);
  INSERT INTO messages_fts (messages_fts, rank) VALUES ('crisismerge', 64);
  -- in the background lane, only while nothing else waits, each in its own short transaction:
  INSERT INTO messages_fts (messages_fts, rank) VALUES ('merge', 50);
  ```

  Measured on 300,000 messages in 20-row commits: p99 2.74 ms and max 30.2 ms with the default, 0.97 ms and 7.5 ms with `automerge=0`; merge steps of 200 pages took 0.5 ms p50, 9.0 ms p99, so steps of about 50 pages fit the 3 ms background slice. Don't run `('optimize')` while jawd serves (1.5 s for a million messages, in one transaction). Fix D3 first, since its I/O is in the same tail.

#### P2. Searching one chat walks every match in every chat

- **Where:** `read.rs:471-492` (`search` with a chat) and `read.rs:498-512` (`matching` with a chat): FTS5 yields matches newest first and the chat is checked on each.
- **Measured:** "cafe amanha" in a 71-message chat: 92 ms p50, 102 ms p99 (budget 50 ms). `matching` in the biggest chat: 55 ms p50, 77 ms p99.
- **Recommendation:** index the chat as an FTS5 column, so FTS5 intersects the doclists itself: `fts5(text, chat, content='messages', content_rowid='t', …)` and `MATCH 'text : ("cafe"* "amanha"*) AND chat : "553"'`. Measured on a copy, side by side in one run: 16 ms against today's 84 ms in the small chat, 6.6 ms against 8.6 ms in the biggest, for 11 MB more index (14%). The `UPDATE` trigger must then fire on `UPDATE OF text, chat`, since a chat merge (I1) changes `chat`.

#### P3. One- and two-letter prefixes are slow

- **Measured:** `"a"*` 62 ms and `"ca"*` 28 ms on a million messages; a search's first keystroke is one letter.
- **Recommendation:** either send message text to FTS5 only from the second character (the palette matches names in memory anyway), or add prefix indexes. With `prefix='1 2 3'`, `"a"*` took 0.07 ms and `"caf"* "ama"*` 0.11 ms (from 11.8 ms), but the index grew from 79 to 274 MB per million messages and each insert does more FTS5 work. Open question 4.

#### P4. The first unread message is found with OFFSET

- **Where:** `read.rs:618-629`.
- **Measured:** 1.9 ms p50, 3.3 ms p99 at 5,000 unread, growing with the count; it also picks the wrong message when the phone's unread count includes messages jaw doesn't have.
- **Recommendation:** seek from the read position in I4: `… WHERE chat = ?1 AND (at, n) > (?2, ?3) AND mine = 0 ORDER BY at, n LIMIT 1` (2 µs measured).

#### P5. Deleting or clearing a chat is one huge transaction

- **Where:** `messages.chat … ON DELETE CASCADE` (`schema.rs:80`) and the cascades to media, reactions, receipts and poll tables.
- **What happens:** `chat.delete` on a 100,000-message chat runs the FTS delete trigger and four cascades per message in one transaction, holding the writer, and so every inbox commit and ack, for seconds.
- **Recommendation:** mark the chat cleared (a `cleared_at` or `deleted_at` column that reads respect) in one small transaction, then delete its messages in background batches.

#### P6. Query plans and `ANALYZE`

- **What happens today:** no `sqlite_stat1` exists, so plans come from SQLite's fixed heuristics and are the same everywhere; every statement reviewed uses the intended index (section 4). Running `ANALYZE` on the bench database (3.2 s) changed one plan: the lookup of a chat's folders switched from `folder_items_by_chat` to scanning `folder_items_chats`, because `folder_items` was empty when it was analyzed. That is the risk: statistics taken early become wrong later. The bundled build also has `SQLITE_ENABLE_STAT4`, which makes plans depend on parameter values once `ANALYZE` has run. `PRAGMA optimize` can run `ANALYZE` on its own (it did, taking 2.5 s on its first run).
- **Recommendation:** never run `ANALYZE` or `PRAGMA optimize`; if a plan ever needs statistics, write fixed rows into `sqlite_stat1` in a migration so they're the same on every machine; and pin every statement's plan with a test (section 5).

#### P7. PRAGMAs fail silently

- **Measured:** `PRAGMA synchronous='FULL ff'` sets `NORMAL` (1), `PRAGMA journal_mode=WALL` leaves the mode as it was, and a misspelled PRAGMA does nothing; none returns an error. (This bit my own first measurement.)
- **Recommendation:** read every PRAGMA back after setting it and refuse to start if it isn't what was asked.

#### P8. Unused indexes tax every write

- **Where:** `messages_by_sender` (`schema.rs:105`), `reactions_by_sender`, `receipts_by_recipient`, `poll_votes_by_voter`, `group_members_by_member`, and FTS5's `docsize` table.
- **What happens:** no query reads these indexes; they exist so that deleting a `person_jids` row can check its references, and no code deletes one. `messages_by_sender` is 6.9 MB per million messages and one more b-tree insert per message; `receipts_by_recipient` will be the most written index in large groups. FTS5's `docsize` (17.8 MB per million) is used only by `bm25()`, which jaw doesn't call, since it ranks by time.
- **Recommendation:** drop the five indexes, forbid deleting `person_jids` rows with a trigger (`BEFORE DELETE ON person_jids BEGIN SELECT RAISE(ABORT, 'a JID is never removed'); END`), add `(sender, t)` back when a "messages from this person" query exists, and create the FTS table with `columnsize=0`.

#### P9. Two jawds can run on one folder

- **Where:** `main.rs:179-180` and `sock.rs:19-23`: connect, and if nothing answers, remove the socket and bind; then open the store.
- **What happens:** two jawds starting together (the LaunchAgent and `jaw` starting one) both find nothing listening, both bind (the second removes the first's socket file), and both open `jaw.db` and start an engine on the same `session.db`. Two whatsmeow clients for one device replace each other's stream and interleave Signal writes; two writers contend on `jaw.db` with 5 s busy waits on the ack path.
- **Recommendation:** first thing at start, open `jaw.lock` in jaw's folder and take `flock(LOCK_EX | LOCK_NB)`, writing the pid into it for diagnostics; refuse to start if it's held. The kernel releases it when the process dies, so there is no stale-lock problem. The engine takes `session.lock` the same way.

#### P10. One feed event per history message

- **What happens:** a full sync of a million messages writes a million events (30 MB) that the feed reads and keeps for 7 days, though windows only need to know which chats changed.
- **Recommendation:** for background conversion, write one event per chat per transaction and give its messages that seq as `v`; keep per-row events for live changes.

#### P11. Disk space isn't watched

- **What happens:** with D3, a history sync can need several times the database's size in WAL; media downloads and the spool share the disk. `SQLITE_FULL` in the middle of a transaction rolls it back cleanly, but then nothing can commit, including the inbox.
- **Recommendation:** check free space with `statvfs` before each background batch and download; below a floor (2 GB or 5%), pause background lanes and downloads and tell interfaces; keep a 64 MB ballast file in jaw's folder that can be deleted to let a checkpoint or a rollback finish when the disk is full.

### Hygiene

- **H1.** The inbox insert, the hottest statement, is re-prepared each time (`mod.rs:400`, `db.execute`); use `prepare_cached`.
- **H2.** A statement cache of 64 per connection (`mod.rs:107`, `read.rs:60`) is close to the number of distinct statements the feed, find and store prepare; raise it to 128 so none is evicted and prepared again.
- **H3.** Identify the file: `PRAGMA application_id = 0x6A617764` ("jawd") at creation, checked at every open, so jawd never migrates some other SQLite file. Set `page_size=4096` and `auto_vacuum=NONE` explicitly before the first table, so a different compile default can't change them.
- **H4.** `PRAGMA trusted_schema=OFF` (tested: FTS5 triggers still work), `PRAGMA cell_size_check=ON`, and build with `LIBSQLITE3_FLAGS="-DSQLITE_DQS=0"` in `.cargo/config.toml` so a string in double quotes is an error, not a column name that happens not to exist.
- **H5.** `Store::count` (`mod.rs:179-183`) opens a new connection each call and runs `count(*)` over whole tables for `status`; read counts kept by triggers, or an estimate, instead.
- **H6.** `history_raw` is `WITHOUT ROWID` with blobs of several KB (`schema.rs:16-22`); `WITHOUT ROWID` suits small rows. Once the sync worktree converts history straight from blobs, drop the table.
- **H7.** Not built yet, and needed: Time Machine exclusion of jaw's folder (`tmutil addexclusion` or `CSBackupSetItemExcluded`), which also avoids backups that copy a database and its WAL at different moments; a `.metadata_never_index` file so Spotlight leaves it alone; and refusing to run when jaw's folder is on a network or synced file system (`statfs` `f_fstypename` of `nfs`, `smbfs`, `afpfs`, `webdav`, or a path under `~/Library/Mobile Documents`), since WAL needs shared memory and working locks.
- **H8.** In-process timers (send retries, the next disappearing message) should use monotonic time; anything stored as wall-clock time is clamped on load (a retry's `next_at` no later than now plus the longest back-off), so a clock change can't delay a send for a day or fire every retry at once.
- **H9.** `chats.sort` uses `printf('%013d', …)`, which breaks for a negative time; clamp to 0 where it's computed. (The sync worktree sorts pinned chats by pin time, which matches WhatsApp.)

## 3. Recommended configuration

### jaw.db

| Setting | Writer | Readers (×4) | Checkpointer | Why |
| --- | --- | --- | --- | --- |
| open flags | read-write, create | `READ_ONLY`, `NO_MUTEX` | read-write | Readers can't write by construction. |
| `application_id` | set at creation, checked | checked | checked | Never open or migrate another file (H3). |
| `page_size` | 4096, before the first table | | | APFS's block size; small inbox commits write 1 to 3 pages. Explicit, so a compile default can't change it. |
| `auto_vacuum` | `NONE`, before the first table | | | The database mostly grows; incremental vacuum adds a pointer-map write per page. Compact with `VACUUM INTO` in maintenance if `freelist_count` passes 25%. |
| `journal_mode` | `WAL`, read back | | | Readers never wait for the writer. |
| `synchronous` | `NORMAL`; `FULL` for the durable class (D1) | (no writes) | `FULL` (default) | `NORMAL` for derived work; durable commits before any ack. |
| `fullfsync` | `ON` on macOS if the owner chooses power-loss durability (D1) | | | Makes a `FULL` commit survive a power cut on macOS. |
| `checkpoint_fullfsync` | `ON` | `ON` | `ON` | Prevents corruption on power loss (D2); any connection that closes last checkpoints. |
| `wal_autocheckpoint` | `0` | `0` | | No commit pays for a checkpoint. |
| `journal_size_limit` | `67108864` | | `67108864` | Truncates the WAL to 64 MiB at each restart (D3). |
| WAL bound | soft 16,384 frames, hard 65,536 | | continuous PASSIVE while frames wait | Section D3. |
| `busy_timeout` | 5000 ms, expected never to be used | 5000 ms | 5000 ms | Only WAL recovery or the writer's short checkpoint can make anyone wait; a lock file (P9) removes the rest. |
| `cache_size` | `-32768` (32 MB) | `-4096` (4 MB) | default | The writer touches indexes all over; readers mostly hit the OS cache. About 48 MB at most of jawd's 100 MB. |
| `mmap_size` | `0` | `0` | `0` | An I/O error under mmap is a SIGBUS crash rather than an error; reads are already well inside budget. |
| `temp_store` | `MEMORY` | `MEMORY` | | No plan sorts; small temp structures only. |
| `foreign_keys` | `ON`, read back | (n/a) | | Integrity; parent lookups are rowid seeks. |
| `recursive_triggers` | `ON` (the bundled default; leave it) | | | `REPLACE` then fires delete triggers, which keeps FTS right if anyone uses it. |
| `trusted_schema` | `OFF` | `OFF` | `OFF` | Recommended hardening; tested with the FTS triggers. |
| `cell_size_check` | `ON` | `ON` | | Detects some corruption before it causes a crash. |
| `secure_delete` | `FAST` if erasure is wanted (question 5), else `OFF` | | | Zeroes deleted content in pages at little cost. |
| statement cache | 128 | 128 | | H2. |
| `ANALYZE`, `PRAGMA optimize` | never | never | never | P6. |
| FTS5 | `automerge=0`, `crisismerge=64`, merge steps of 50 pages in the background | | | P1. |

### session.db (the engine)

| Setting | Value | Why |
| --- | --- | --- |
| DSN | `_journal_mode=WAL&_synchronous=FULL&_foreign_keys=on&_busy_timeout=5000&_txlock=immediate` | `FULL` so its commits reach the drive before jaw.db's flush; immediate transactions so a read-then-write never fails with `SQLITE_BUSY_SNAPSHOT`. |
| `ConnectHook` | `PRAGMA checkpoint_fullfsync=ON; PRAGMA journal_size_limit=16777216;` | D2, D3. mattn's DSN has no parameter for these. |
| whatsmeow | `UseRetryMessageStore = true` | Retry receipts for sent messages work after a restart (D4). |
| lock | `session.lock`, `flock` | One engine per session (P9). |

## 4. Indices and plans

Every statement the store, the feed and find prepare, with `EXPLAIN QUERY PLAN` from the bundled SQLite 3.53.2 on the 1,000,000-message bench database (2,001 chats, 1,022,171 events), and its time over 50 warm runs where it's a read. "After" is the plan once this review's changes are made; "same" means no change.

### Writes

| Statement | Plan today | After |
| --- | --- | --- |
| `INSERT INTO inbox …` | append to the rowid b-tree | same, `AUTOINCREMENT` (D6), `prepare_cached` (H1) |
| `INSERT INTO events …` | append | same |
| `SELECT n FROM chats WHERE id = ?` | `SEARCH chats USING COVERING INDEX sqlite_autoindex_chats_1 (id=?)`, 2 µs | via `chat_jids` primary key (I1), 2 µs |
| `SELECT n FROM person_jids WHERE jid = ?` | covering autoindex on `jid`, 2 µs | same |
| `INSERT INTO people / person_jids … RETURNING n` | append, plus foreign-key child checks that only run when needed | same |
| chat upsert (`INSERT … ON CONFLICT (id) DO UPDATE … RETURNING n`) | autoindex on `id` | same |
| `UPDATE people … WHERE n = ?` | rowid | same |
| join: `UPDATE person_jids SET person = ? WHERE person = ?` | `person_jids_by_person` | same |
| join: `UPDATE chats SET person = ? WHERE person = ?` | `chats_by_person` (partial) | same |
| join: `UPDATE OR IGNORE folder_items …`, `DELETE FROM folder_items WHERE person = ?` | `folder_items_by_person` | same |
| `SELECT t FROM messages WHERE t BETWEEN ? AND ? ORDER BY t DESC LIMIT 1` (allocate) | `SEARCH messages USING COVERING INDEX sqlite_autoindex_messages_1 (t>? AND t<?)`, 2 µs | same |
| `SELECT … FROM messages WHERE chat = ? AND id = ?` | `messages_by_id (chat=? AND id=?)`, 2 µs | same |
| message `UPDATE … WHERE n = ?` | rowid | same, with merge rules (I2) |
| message `INSERT … RETURNING n` | appends to 5 b-trees (rowid, `messages_by_id`, `messages_by_time`, `t`, `messages_by_sender`) plus FTS data and docsize | 4 b-trees plus FTS data (P8) |
| chat newest (`UPDATE chats SET last_at … WHERE n = ? AND (… ?5 > (SELECT t FROM messages WHERE n = last_message))`) | rowid, correlated subquery by rowid | same |
| `UPDATE chats SET message_count = message_count ± 1 WHERE n = ?` | rowid | replaced by triggers (I4) |
| delete: `UPDATE chats SET last_message = (SELECT n FROM messages WHERE chat = ? ORDER BY at DESC, n DESC LIMIT 1) …` | `messages_by_time (chat=?)`, covering | same |
| prune: `DELETE FROM events WHERE seq IN (SELECT seq FROM events WHERE at < ? AND seq < (SELECT max(seq) …) ORDER BY seq LIMIT ?)` | rowid range from the start | `DELETE FROM events WHERE seq < ?floor` in batches, rowid range (I6) |
| `INSERT OR IGNORE INTO history_raw …` | `WITHOUT ROWID` b-tree on `(chat, id)` | table goes (H6) |
| sync: `SELECT coalesce(max(n), 0) + 1 FROM messages` | rowid max, 2 µs | not needed with `AUTOINCREMENT` and `last_insert_rowid()` |
| sync: `index()` reading `fts_backlog` newest first, then `INSERT INTO messages_fts … SELECT t, text FROM messages WHERE n = ?` | rowid ranges and seeks | same |

### Reads

| Statement | Plan today | Time | After |
| --- | --- | --- | --- |
| chat list page (`… WHERE c.archived = ? AND (c.sort, c.n) < (?, ?) ORDER BY c.sort DESC, c.n DESC LIMIT 100`, four left joins by rowid) | `SEARCH c USING INDEX chats_by_sort (archived=? AND sort<?)`, no sort step | 0.10 ms | same |
| chats above an anchor, chats from an anchor | `chats_by_sort (archived=? AND sort>?)` | 0.008 ms | same |
| chats by n (`… WHERE c.n IN (SELECT value FROM json_each(?))`) | rowid per value | 0.004 ms | same |
| chat count (`SELECT count(*) FROM chats WHERE archived = ?`) | covering `chats_by_sort (archived=?)` | 0.035 ms | same |
| messages, newest 100 (`… WHERE m.chat = (SELECT n FROM chats WHERE id = ?) ORDER BY m.at DESC, m.n DESC LIMIT 100`) | `SEARCH m USING INDEX messages_by_time (chat=?)`, no sort step | 0.048 ms | same |
| messages before / after a cursor | `messages_by_time (chat=? AND at<?)` / `(at>?)` | 0.039 / 0.027 ms | same |
| message by chat and ID; message place | `messages_by_id (chat=? AND id=?)` | 0.002 ms | same |
| raw by n | rowid | 0.002 ms | same |
| message by t (find) | `sqlite_autoindex_messages_1 (t=?)` | 0.002 ms | same |
| search, all chats, `"nao"*` (`messages_fts CROSS JOIN messages …`) | `SCAN messages_fts VIRTUAL TABLE INDEX 192:M1<`, then `t=?` seek | 5.5 ms | same |
| search, all chats, `"cafe"* "amanha"*` | same | 11.6 ms (19 ms p99 in the bench) | same |
| search in one chat | FTS scan with a per-row chat check | 8.8 ms in the biggest chat, **92 ms** in a small one | `MATCH 'text : (…) AND chat : "n"'`, 6.6 / 16 ms (P2) |
| find: matching, all chats (`SELECT rowid FROM messages_fts WHERE … ORDER BY rowid DESC LIMIT 1000`) | FTS only | 11.1 ms | same |
| find: matching in a chat | FTS scan with per-row chat check | **55 ms p50, 77 ms p99** | chat column (P2) |
| one-letter prefix `"a"*` | FTS prefix expansion | **62 ms** | minimum length or `prefix=` (P3) |
| events since a seq | rowid range | 0.07 ms for 1,000 | same |
| feed bounds | was `SCAN events`, 35.6 ms; fixed on main (`8eceeea`) to two rowid seeks | 0.002 ms | same |
| message count | was a covering-index count, 1.75 ms for 100,000; now read from `chats.message_count` | 0.002 ms | via triggers (I4) |
| newer than a cursor | was an uncapped count, 1.64 ms; now capped at 1,000 | 0.046 ms | same |
| older than a cursor (`EXISTS …`) | covering `messages_by_time` | 0.002 ms | same |
| first unread (`… ORDER BY at DESC, n DESC LIMIT 1 OFFSET ?`) | `messages_by_time (chat=?)`, walking `unread` rows | **1.9 ms at 5,000** | seek from the read position, 0.002 ms (P4) |
| find: phone JIDs (`… WHERE jid LIKE '%@s.whatsapp.net'`) | `SCAN person_jids` | 0.25 ms for 3,001 | same; it runs once at start |
| find: events since, chats and people only | rowid range | 0.09 ms | same |
| find: a chat's folders; a person's JIDs | `folder_items_by_chat`, `person_jids_by_person` | 0.002 ms | same |

No plan uses a temporary b-tree for `ORDER BY`. The only scans are of `person_jids` at start (small, once) and of FTS5's own index. The row-value comparisons `(c.sort, c.n) < (?, ?)` and `(m.at, m.n) < (?, ?)` use the index for both the range and the order.

### Pinning plans

Keep every SQL statement in a `const` (most already are), and add a test that runs `EXPLAIN QUERY PLAN` for each on a seeded database (built through the writer, with no `sqlite_stat1`, as in production) and compares it with a checked-in expected plan. It fails on any change, and in particular on `SCAN` of a table not on an allow-list, `USE TEMP B-TREE`, or an index other than the expected one. It runs in CI on every push, and a rusqlite upgrade (which brings a new SQLite) can't change a plan unnoticed.

## 5. Migration strategy

This section is written to go into the design under "### Storage" as it stands.

### Migrations

- **Forward only.** Migrations are numbered SQL built into jawd, run against `PRAGMA user_version`. There are no down migrations: going back is restoring the backup taken before the upgrade, or running an older jawd within its compatibility window.
- **Frozen once merged.** A migration that has reached `main` is never edited. A test hashes each migration's text and compares it with a checked-in list, so editing one fails CI. Branches don't claim numbers: the next number is taken when a branch merges, and CI fails if two migrations share one.
- **Three kinds, declared with each migration:**
  - **Schema steps** cost the same however big the database is: creating tables, triggers and indexes on small tables, adding a nullable column or one with a constant default, dropping an unused index. All pending schema steps run at start in one `BEGIN IMMEDIATE` transaction, with `user_version` set in the same transaction.
  - **Data steps** cost time in proportion to the rows: rebuilding a table, creating an index on `messages`, `UPDATE`s over every row. (FTS changes are backfills; see below.) Each runs at start in its own transaction, after the socket is listening (so interfaces show progress) and before the engine connects (so no ack waits on it), with a measured cost per million rows written beside it. A data step must take no more than 10 s per million messages on the reference Mac (a `CREATE INDEX` on `messages` takes 2.3 s per million, rebuilding `messages` 5.8 s, and rebuilding FTS in one go 10.4 s, which is why FTS changes are backfills).
  - **Backfills** fill or transform data while jaw runs: in the background lane, in transactions of at most 3 ms, resumable from a position kept in a `backfills (name TEXT PRIMARY KEY, position TEXT, done INTEGER NOT NULL DEFAULT 0) STRICT` table, and idempotent. Code works with the data in either state until the backfill is done; a later schema step may then add a constraint that relies on it. Anything that can be a backfill is one.
- **Opening a database**, in this order:
  1. Take `jaw.lock` (`flock`), or refuse to start because another jawd is running.
  2. Check `application_id`; refuse a file that isn't jaw's.
  3. Read `user_version` and `meta.compat`. If the version is newer than this jawd knows: if `compat` is no newer than this jawd's version, open it (the newer migrations were additive); otherwise refuse with "jaw.db is from a newer jawd (schema N); this jawd knows up to M; run `make install` again".
  4. Compute the schema fingerprint (SHA-256 of `SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name`, with whitespace normalised) and compare it with the fingerprint this jawd expects for that version. On a mismatch, refuse to write and say so: the schema was hand-edited, a migration was changed, or two branches used one number.
  5. If migrations are pending and any is a data step: check that free space is at least 1.2 times the database (a backup plus a rebuild's free pages), or refuse with how much is needed; then `VACUUM INTO 'jaw.db.before-vN'` (2.0 s for 558 MB here, about 18 s for 5 GB). Schema-step-only upgrades don't need a backup, since they are one transaction and touch no data.
  6. Run the migrations.
  7. Check again: the fingerprint must equal the new version's; `PRAGMA foreign_key_check` must return nothing; if a migration touched `messages` or `messages_fts`, `INSERT INTO messages_fts (messages_fts, rank) VALUES ('integrity-check', 1)` (2.0 s per million) must pass. A failure rolls back the last migration and refuses to start, keeping the backup.
  8. Start; delete the backup after the first clean shutdown that follows, or keep the last one if the owner prefers (question 6).
- **Rebuilding a table** (SQLite's twelve steps, and the only way to change a column's type or constraints): with `PRAGMA foreign_keys=OFF` set before `BEGIN` (it does nothing inside a transaction) and `legacy_alter_table` left `OFF`: create `new_X` (STRICT), `INSERT INTO new_X SELECT …`, drop `X`, `ALTER TABLE new_X RENAME TO X`, recreate every index, trigger and view on `X` (dropping `X` dropped them; D10), run `PRAGMA foreign_key_check`, commit, set `foreign_keys=ON`. The runner supports this as a declared kind of step; a migration never toggles foreign keys itself.
- **FTS changes** (tokenizer, options, columns) can't be made with `ALTER`: create `messages_fts_new`, add triggers that write to both, fill it in background batches (`INSERT INTO messages_fts_new (rowid, text, chat) SELECT t, text, chat FROM messages WHERE n BETWEEN ? AND ?`), then in one schema step rename the tables and drop the old one and its triggers. Search keeps working on the old index until the swap.
- **What may be rebuilt.** Every table is in one class, written beside its `CREATE`:
  - *Derived from messages' raw* (message rows, spans, FTS, reactions, poll tallies, links, vectors later): may be dropped and rebuilt from raw, as a backfill into new tables followed by a swap.
  - *Fetched from WhatsApp* (app-state effects, group members, contacts and push names, avatars, the LID map): may be rebuilt by asking WhatsApp again (app-state full sync, group info), and only with the engine connected.
  - *jaw's own* (folders, folder items, favourites, frecency, drafts, the outbox, media cache state, read positions jaw sets, quarantine, backfills, meta): never dropped, never rebuilt, and copied as they are by every rebuild of the database.
  A migration that drops or rewrites a table must name its class, and a test checks that no jaw-own table is ever dropped.
- **session.db.** whatsmeow's own upgrades run in transactions and refuse a newer database. Before calling `sqlstore.New`, the engine takes `session.lock`, reads `whatsmeow_version`, and if an upgrade is pending, writes `session.db.before-whatsmeow-vN` with `VACUUM INTO`. A refusal is reported to jawd as "update needed", not as a crash loop.

### The tests the strategy needs

1. **Frozen migrations:** each migration's SHA-256 against a checked-in list; duplicate numbers fail.
2. **Fixture databases:** for every released schema version, a small database made by that version's jawd (a `jawd fixture` command that fills every table, with edge cases: Unicode and accents, NULLs, a 64 KB message, LID and phone JIDs for one person, pending amendments, folders, a quarantine row), checked in under `crates/jawd/tests/fixtures/`. Generating them from the current `MIGRATIONS` array, as `a_database_at_every_earlier_version_keeps_its_rows` does today, can't catch an edited migration, since the old database changes with it.
3. **Upgrade from every fixture:** opens each with the current jawd and checks that the fingerprint equals a freshly created database's, that the rows are all there and right, that `foreign_key_check` is empty, that the FTS integrity check with `rank=1` passes, and that the expected triggers exist.
4. **A failure at every statement:** a test hook that fails a migration at statement k, for each k, and checks that `user_version` and the fingerprint are what they were before.
5. **Refusals:** a newer `user_version` outside the compatibility window, a wrong `application_id`, a fingerprint mismatch, too little free space.
6. **Compatibility window:** an older jawd opens a database whose newer migrations were additive.
7. **Cost:** each data step on the 1,000,000-message bench database within its budget per million, in `make perf`.
8. **Rebuild from raw:** drop the derived tables of a fixture, rebuild them from raw, and compare with the original.
9. **Plans:** the snapshot test in section 4.
10. **Drain to empty:** convert the whole inbox, then commit one more row and see it converted (D6).

## 6. Proposed wording for the design

### For the principles

Replace principle 7 with:

> 7. Nothing is ever lost, and the data is never inconsistent. Nothing is acknowledged, to WhatsApp, the phone or an interface, before it's on disk in a way that survives a power cut. A change and its event commit together or not at all. Every write is idempotent, so a replay, a duplicate or a crash halfway changes nothing twice. Every migration is transactional, checked and tested from every earlier version's real database, and can't leave a database half changed. Anything that makes data access slow at times or opens an edge case (a setting, a missing index, a query plan, a lock, a file that grows) is a defect, not a trade-off.

### For "### Storage": a "Data integrity" subsection

> **Data integrity.** These are rules; every builder follows them, and the tests listed check them.
>
> - **Durable before acknowledged.** A commit that precedes an acknowledgement (the inbox commit before the engine's ack, enqueuing a send before `send.*` answers, any action before its reply, the commit that drains a history blob) is synced: `synchronous=FULL`, and on macOS `fullfsync`, since `fsync` there doesn't reach the disk. [Or, under option B: acks are released in groups after a synced commit.] Everything else commits at `NORMAL`, since a later synced commit makes it durable too. Every connection to `jaw.db` and `session.db` sets `checkpoint_fullfsync`, or a power cut on macOS can corrupt the file. A spool or media file is synced and renamed into place before a row points to it, and deleted only after a synced commit stops pointing to it.
> - **Exactly once from at least once.** Everything that arrives can arrive again: live and history, the same message twice, an edit or reaction before its message, a JID as a phone number and as a LID. Every write is an idempotent upsert with merge rules: status only advances, an edit wins by its time, a reaction by its time, a deletion and a for-me tombstone are sticky, and a chat or person is found through its alias table before anything is written. An inbox row is deleted in the transaction its conversion commits in; a change that fails is quarantined, never dropped, and never blocks the changes after it.
> - **Derived values are derived.** Counts and summaries (message counts, unread, mentions, reaction and poll totals) are kept by triggers or recomputed in the transaction that changes their source, never adjusted by hand in one code path, and a background check compares them with their source.
> - **The schema is known.** Tables are `STRICT`; booleans and invariants have `CHECK`s; JSON columns are `json_valid`; foreign keys are on; `jaw.db` carries jaw's `application_id`; every PRAGMA is read back after it's set; jawd checks the schema's fingerprint at every start and refuses one it doesn't expect.
> - **Migrations** follow the strategy below: forward only, frozen once merged, numbered at merge, schema steps in one transaction at start, data steps measured and before the engine connects, everything else a resumable backfill; backed up before any data step; checked after (fingerprint, foreign keys, FTS). A migration never drops or rebuilds jaw's own data (folders, favourites, drafts, the outbox), which no phone can give back.
> - **Bounded files.** The WAL is kept under 64 MiB by making background work wait for the checkpointer; disk space is watched, and background work pauses before the disk fills; nothing grows without a limit and a number in `jawd status`.
> - **Predictable plans.** No `ANALYZE` or `PRAGMA optimize`; every statement's plan is pinned by a test; no statement scans a large table, sorts in a temporary b-tree, or uses `OFFSET`.
> - **One process.** jawd holds `jaw.lock` and the engine `session.lock` for as long as they run; jaw's folder is local, left out of Time Machine and Spotlight, and refused on a network or synced file system.
> - **Checked, and recoverable.** After an unclean shutdown, and weekly when idle, a reader runs `PRAGMA quick_check` and FTS5's `integrity-check` in the background; monthly, `integrity_check`. On corruption jawd stops writing, stops acknowledging (so WhatsApp keeps what's new), tells interfaces, and rebuilds `jaw.db`: jaw's own tables copied, everything else rebuilt from raw and fetched again from WhatsApp.
> - **Tested.** Every rule above has a test: crash and power-cut simulation around each commit that precedes an ack, replays and duplicates of every kind, upgrades from real fixture databases of every version, plan snapshots, the WAL bound under a full history sync, and disk full.

And replace the `synchronous=NORMAL` bullet with:

> - **Commits:** `NORMAL` for derived work, synced (`FULL`, and `fullfsync` on macOS) for every commit that precedes an acknowledgement; `checkpoint_fullfsync` everywhere; `wal_autocheckpoint=0` on the writer, a checkpointer thread running PASSIVE while frames wait, and background work held back so the WAL stays under 64 MiB.

## 7. Measurements

All on a Mac Studio, Apple M1 Ultra (20 cores, 128 GB), internal SSD, APFS, macOS 26.6.2, with nothing else heavy running. SQLite 3.53.2 from `libsqlite3-sys` 0.38.2 (rusqlite 0.40.2, `bundled`), compiled with `ENABLE_FTS5`, `ENABLE_STAT4`, `ENABLE_DBSTAT_VTAB`, `DEFAULT_FOREIGN_KEYS`, `DEFAULT_RECURSIVE_TRIGGERS`, `DEFAULT_WAL_SYNCHRONOUS=2`, `MAX_MMAP_SIZE=0x7fff0000`, `TEMP_STORE=1`, without `DQS=0`. Scratch builds of main (`/tmp/jaw-dbreview`) and of the sync worktree (`/tmp/jaw-sync-review`), and a throwaway program (`/tmp/dbexp`) on the same rusqlite. Timings are p50 / p99 / max unless said otherwise.

### Device and commit costs (1,000 runs each)

| What | p50 | p99 | max |
| --- | --- | --- | --- |
| 1 KiB append + `fsync` | 0.049 ms | 0.140 ms | 0.257 ms |
| 1 KiB append + `F_BARRIERFSYNC` | 0.307 ms | 1.468 ms | 4.411 ms |
| 1 KiB append + `F_FULLFSYNC` | 4.041 ms | 7.058 ms | 8.972 ms |
| SQLite WAL commit, 1 KiB row, `synchronous=OFF` | 0.026 ms | 0.155 ms | 0.296 ms |
| … `NORMAL` | 0.025 ms | 0.115 ms | 0.306 ms |
| … `FULL` | 0.146 ms | 0.449 ms | 4.524 ms |
| … `FULL` + `fullfsync` | 4.129 ms | 7.135 ms | 14.994 ms |
| 64 KiB row, `NORMAL` | 0.306 ms | 1.002 ms | 8.143 ms |
| 64 KiB row, `FULL` + `fullfsync` | 5.027 ms | 8.731 ms | 12.239 ms |
| inbox commit at `FULL` after a 200-row `NORMAL` transaction (500 runs) | 0.318 ms | 1.981 ms | 2.116 ms |
| same with `fullfsync` | 6.207 ms | 9.338 ms | 11.671 ms |
| inbox commit at `NORMAL`, checkpointer every 250 ms (3,000 runs) | 0.043 ms | 0.170 ms | 4.445 ms |
| same, checkpointer with `checkpoint_fullfsync` | 0.049 ms | 0.202 ms | 0.619 ms |
| inbox commit at `FULL`, checkpointer with `checkpoint_fullfsync` | 0.150 ms | 0.483 ms | 8.195 ms |

### The engine's commit-acked round trip (`jawd bench link --commit`, Go engine, 600 B)

| jaw.db writer | Count | p50 | p99 | max | Rate |
| --- | --- | --- | --- | --- | --- |
| `NORMAL` | 20,000 | 88 µs | 225 µs | 2,720 µs | 10,304/s |
| `FULL` | 20,000 | 144 µs | 379 µs | 6,983 µs | 6,161/s |
| `FULL` + `fullfsync` | 2,000 | 5,003 µs | 8,167 µs | 18,641 µs | 204/s |

### `jawd bench store`, 1,000,000 messages in 2,000 chats

| | main (`7ea1809`) | sync worktree (`133c93f` + changes) |
| --- | --- | --- |
| Build | 65.5 s, 15,268 messages/s | 40.4 s, 24,746 messages/s, then 28.3 s indexing |
| Live message during history (hand-over to commit) | 3.70 / 23.67 / 174.6 ms | 1.03 / 6.27 / 105.4 ms |
| Live message during indexing | n/a | 1.02 / 21.17 / 45.4 ms |
| Writer waits (status) | n/a | 0.52 ms p50, 30.7 ms p99 |
| WAL peak (sampled every 2 s) | 5,765 MB | 7,115 MB |
| jaw.db at the end | 550 MB (558 bytes a message) | 561 MB |
| jawd's peak resident memory | 128 MB | not measured |
| 100-message page, every page of the biggest chat | 0.42 / 1.20 / 7.50 ms | 0.25 / 0.38 / 1.40 ms |
| Search "nao", 50 hits | 5.87 / 6.73 ms | 5.38 / 6.10 ms |
| Search "cafe amanha" | 11.55 / 19.09 ms | 10.56 / 11.24 ms |

Sizes in the 1,000,000-message database (`dbstat`): `messages` 349 MB, `messages_fts_data` 78 MB, `messages_by_id` 36 MB, `events` 30 MB, `messages_by_time` 20 MB, the `t` index 19 MB, `messages_fts_docsize` 18 MB, `messages_by_sender` 7 MB.

### The WAL

- Synthetic writer (200-row transactions back to back), PASSIVE checkpoint every second, 20 s: no reader, 3,859 MB of WAL; a reader holding a snapshot, 4,746 MB; `journal_size_limit=64 MiB`, 4,353 MB.
- Checkpoint cost on a 300 MB database by frames waiting: 257 frames 8.7 ms; 1,999 frames 59.6 ms; 4,974 frames 125.7 ms; 10,039 frames 186.7 ms.
- Forcing restarts: `RESTART` from the checkpointer when nearly caught up: 61 ms p50, 172 ms p99 per checkpoint; inbox commits meanwhile 0.058 ms p50, 38.4 ms p99, 149 ms max; WAL bounded at 67 MB. The writer running PASSIVE when the checkpointer is close: WAL bounded at 67 MB; checkpoints 71 ms p50, 123 ms p99 on the writer.
- After an abort with a 10,654 MB WAL: open and first read 3,455 ms; then `TRUNCATE` 4,802 ms.

### FTS5, on the 1,000,000-message database

| What | Result |
| --- | --- |
| `rebuild` | 10.4 s |
| `integrity-check` | 0.67 s; with `rank=1` (against the content table) 2.0 s |
| `optimize` | 1.5 s; `merge` of 500 pages 1.4 to 5.5 ms |
| Index today (`detail=full`) | 78.8 MB data + 17.8 MB docsize |
| `columnsize=0` | 78.8 MB, no docsize; "nao" 5.9 ms (same) |
| `detail=column` | 76.7 MB |
| `detail=none` | 57.5 MB; "nao" with highlight 47 ms (8× slower); "cafe amanha" 94 ms; `"e-mail"*` fails ("phrase queries are not supported") |
| `prefix='1 2 3'` | 273.8 MB; `"a"*` 0.068 ms (from 62 ms); `"ca"*` 0.056 ms (from 28 ms); `"caf"* "ama"*` 0.11 ms (from 11.8 ms); build 16.8 s |
| a `chat` column | +11 MB; small-chat search 16 ms (from 84 ms); biggest chat 6.6 ms (from 8.6 ms) |
| 300,000 messages in 20-row commits | default: 0.48 / 2.74 / 30.2 ms per commit; `automerge=0`, `crisismerge=64`: 0.37 / 0.97 / 7.5 ms, with merge steps of 200 pages at 0.51 / 8.95 / 17.3 ms |
| `trusted_schema=OFF` | FTS5 triggers work |

### Integrity, backup and migration costs, on the 558 MB database

| What | Time |
| --- | --- |
| `PRAGMA quick_check` (first, cold) | 4.06 s |
| `PRAGMA integrity_check` (warm) | 2.57 s |
| `PRAGMA foreign_key_check` | 0.20 s |
| `VACUUM INTO` | 2.03 s, 547 MB |
| `CREATE INDEX … ON messages (sender, at)` | 2.33 s |
| Rebuild of `messages` (twelve steps) | 5.82 s; file 558 → 941 MB; FTS triggers lost |
| The sync worktree's migration 3 DDL (`DROP COLUMN sort`, generated `VIRTUAL` column, index) | 0.10 s; `quick_check` ok |
| `ANALYZE` | 3.23 s; one plan changed |
| `PRAGMA optimize=0x10002` (first run) | 2.46 s; created 14 `sqlite_stat1` rows |

### Silent PRAGMAs

`PRAGMA synchronous='FULL ff'` then `PRAGMA synchronous` returns 1 (`NORMAL`); `PRAGMA journal_mode=WALL` returns the current mode; `PRAGMA cache_sizee=5` does nothing; none errs.

## 8. Open questions for the owner

1. **Power-loss durability against catch-up speed.** With durable acks, catch-up after time offline runs at about 200 messages a second on this Mac (5,000 waiting messages take about 25 s), and the round-trip budget becomes 5 ms. *Recommendation:* durable acks now (option A in D1), then deferred acks with group commit (option B, a small whatsmeow patch) if catch-up on the spare number feels slow. Option C (fast acks, a power cut can lose the last second) doesn't meet principle 7 on a desktop Mac.
2. **Was your `jaw.db` ever opened by a jawd built from `sync-v1`?** Both branches have a migration 3 (D9). You can check without risk, with jawd stopped: `sqlite3 "file:$HOME/Library/Application Support/jaw/jaw.db?mode=ro" "PRAGMA user_version; SELECT name FROM sqlite_schema WHERE name IN ('fts_backlog', 'sync_state'); SELECT count(*) FROM pragma_table_info('chats') WHERE name = 'message_count';"`. *Recommendation:* if it shows the sync tables at version 3, keep using the sync build until the renumbered migrations and the fingerprint check land, or re-link into a fresh database; the account can be synced again, and nothing jaw-only exists yet.
3. **What does "unread" mean?** The phone's count includes messages jaw may not have; a count derived from jaw's messages and a read position is exact for what jaw shows and can't drift. *Recommendation:* derive it from a read position (I4), and show the phone's count only until history has arrived.
4. **Short prefixes in message search.** A one-letter message search takes 62 ms; prefix indexes make it 0.07 ms but triple the index (from 79 to 274 MB per million messages) and add work to every insert. *Recommendation:* search message text from the second character, with names matched in memory from the first, and no prefix index; revisit with real text.
5. **Erasure.** Should a disappearing, view-once or revoked message be unrecoverable from jaw's files, or only hidden? Erasure means clearing `raw`, `secure_delete=FAST`, and merging FTS segments so deleted tokens go. *Recommendation:* yes for disappearing and view-once messages, since that's what the sender asked for; it costs little.
6. **Backups of jaw's own data.** Time Machine leaves jaw's folder out, so folders, favourites, drafts and frecency exist nowhere else. *Recommendation:* a daily `VACUUM INTO` of just those tables (kilobytes) into a folder Time Machine does back up, plus the backups before data migrations.
7. **Several accounts later.** *Recommendation:* decide the layout now: everything that belongs to an account (`jaw.db`, `session.db`, `spool/`, `media/`) under `accounts/<n>/`, with jaw's own cross-account data in a small `jaw.db` beside them. Adding an account is then a folder, not a migration, and SQLite never needs a transaction across two databases.
8. **How long tombstones are kept.** A deleted-for-me message must not come back from history (I3). *Recommendation:* keep tombstones for as long as the account is linked; they're about 40 bytes each.
9. **Vectors (M9).** int8 vectors for a million conversation windows are about 400 MB, as big as the messages. *Recommendation:* keep them in a separate `vectors.db` attached for queries: they're derived and rebuildable, and keeping them apart keeps `jaw.db`'s backups, checks and migrations small. Store the embedding model's ID with each vector so a new model is a backfill. The schema needed now is only a `windows` table (chat, first and last `t`, model, version).
10. **When corruption is found.** *Recommendation:* stop writing and acknowledging, tell interfaces, and rebuild automatically from raw and from WhatsApp while keeping jaw's own tables, asking before discarding the damaged file.
