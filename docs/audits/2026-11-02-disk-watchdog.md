# Audit: the disk watchdog (M8)

Range: ea0f4134..56b7c848 in disk-watchdog (c0f14e92 the plan, 56b7c848 the change), 2 November 2026. Auditor: database-expert; a second one took the audit over at 14:22, in the first's scratch, when the first was stopped by a reload of Cursor's window. Round one. origin/main was 0dd286c9 when the audit began and is 1669937a now; onto 1669937a the branch conflicts in one line of protocol/README.md (see Landing).

## What the change is

P11 of the data review, with what was left of D7. jawd reads the free space of the disk jaw's folder is on with `statvfs` before each background transaction, indexing step, media scheduler pass and history blob. Below a floor of 2 GB, or 5% of the disk where that's less, background work, indexing and downloads (one an interface asked for included) wait until the disk has the floor and 256 MB more, while inbox rows and live writes go. Interfaces hear a `warning` notice as the disk goes below the floor and an `info` one once it has room. A 64 MB ballast in jaw's folder, reserved with `F_PREALLOCATE` (macOS) or `posix_fallocate` (Linux), is given up when a full disk refuses a commit (the writer's `failing`) or a checkpointer pass, and made again every 10 s once the disk has room for it beside the floor. `jawd status` and `jaw status` say it all. No schema change, no new SQL, no migration.

## D7 and P11, as the review meant them

- **What main already had of D7.** An inbox commit that fails keeps its rows and is tried again from 10 ms doubling to 1 s, nothing else written meanwhile (`commit_group`, store/mod.rs:1397-1490); live and background writes go through `retrying` (1336-1353), which keeps their changes and commits the inbox between tries. Acks go only after `store_inbox`'s synced commit. A full disk says "jaw's disk is full…" in an `error` notice and `storage.trouble`. Media already kept 512 MB (64 MB for one asked for) beside each download (media/fetch.rs:149). Migrations already refuse to start without room for their backup (schema.rs:2141-2155).
- **What this change adds, checked by reading every path:** the pause of background lanes, indexing and downloads (D7's "while the inbox can't commit": a disk that refuses a commit for want of room reads below the floor, so they wait; for an I/O error or a lock, the one writer thread is in the inbox's retry loop, so background work waits behind it, and only downloads in flight go on, their rows queued behind the writer in bounded lanes); the notices; P11's statvfs check and the ballast; and the status.
- **On a full disk** a refused commit keeps its payload and is tried again; `failing` gives up the ballast before the first retry's 10 ms sleep; nothing is acknowledged before a synced commit; nothing is dropped. The change adds no commit and moves no acknowledgement. The VFS change (store/vfs.rs) is test-only.
- **The ballast.** In WAL mode a rollback needs no room (the failed transaction's frames are never committed, and `temp_store` is memory), so what it buys is room for the retried commit's WAL frames and for a checkpoint's growth of jaw.db. It is made only with the floor and its 64 MB free and while nothing waits (disk.rs:244-247), `F_ALLOCATEALL` is all or nothing, and it's made under `ballast.partial` and renamed. It can't be remade at a full disk (that would need the floor free). A give-up that lands while it's being made finds only the partial and frees nothing that moment (see O3); the next refused try gives it up. It lives in jaw's folder, outside the media folders whose files the cache's limit counts (`media_files`), outside the spool, and outside anything `status` sums, and nothing in jawd lists or copies jaw's folder, so no bound of jaw's own files counts it.
- **Pausing resumes where it was.** The writer holds a background batch in memory and waits before each of its transactions, each a committed unit as before; a stop during the wait loses only the in-memory copy of a batch whose source (inbox row, spool blob, backfill position) is still durable, and it's redone idempotently as after any stop. Indexing resumes from `fts_backlog`; the scheduler takes nothing while short of room and its `wanted` rows stay; the fetcher waits before it asks for a blob, never between a blob's chunks. The background queue stays bounded at 16. At the writer, the wait takes inbox rows first, then live writes (`select_biased!`, 1640-1651), so it starves neither (switch 2 shows the test catches a wait that doesn't); but the converter puts the rows left in the inbox at a start in the background lane, and every live row behind them waits with them (P1). Nothing awaits `Store::indexed` but the demo engine's timed runs (demo/mod.rs:627, 746), so pausing indexing holds up no request. Media's eviction runs on its own task through durable live writes, so the cache still frees room while background work waits.
- **Another volume:** jaw.db, the spool, the media folders and the ballast are all in jaw's folder, and media writes no temporary file outside it, so one reading of that folder's disk is the right one. A network folder is O7.

## Findings

Nothing is lost and nothing is left inconsistent: no finding is data loss or inconsistency. One breaks the live budget (P1), and two rules the change brings have no test that fails without them (T1, T2), so this round asks for changes. The rest (O1 to O10) may stand as follow-ups.

### P1. Unpredictable performance: below the floor, the rows left in the inbox at a start hold up every live message's conversion

- **Where:** sync/drain.rs:376-389 (rows whose seq is at most the inbox's largest when the converter starts go in `Lane::Background`, in the one loop that converts the whole inbox in order); store/mod.rs:1927 (`wait_for_disk` before each background group); drain.rs:390-391 (a row whose conversion fails stays in the inbox, in quarantine).
- **What happens:** the rows in the inbox when jawd starts, those an unclean stop left unconverted and every row kept in quarantine (converted again at each start), go through the background lane, which now waits for the disk. The converter reads the inbox in seq order in one loop, so every row that arrives behind them waits too. A jawd that starts below the floor with any such row commits and acknowledges inbox rows as before, but converts, shows, pushes and indexes no new message until the disk has the floor and 256 MB more, against the change's own rule ("inbox rows and live writes never wait for it"), the notice and protocol/README.md ("messages still arrive and send"), and the live budget. Reproduced (see Measurements): with room both rows convert; below the floor, 5 s after the live row, neither has. Nothing is lost: both convert once there's room.
- **Fix:** below the floor, neither the rows left from before nor the live rows behind them wait for the disk. For example, while `store.disk().room()` says no, the converter takes the leftovers in the live lane a few dozen at a time, as it takes live rows; or the writer lets a background batch that drains inbox rows past `wait_for_disk`; or live rows are converted on a loop of their own. With a test that fails at the head: below the floor, a row left in the inbox from before and a live one after it, and the live one converted (the audit's reproduction is one).

### T1. Test gap: jawd serving's wiring, its status and the real disk reader are never tested

- **Where:** main.rs:597-600 (`Disk::watch`, the `tell` task, `Store::open_watched`, `disk.start()`), state.rs:306 (`disk` in `jawd status`), disk.rs:301-309 (`free_space`, the only code that reads a real disk).
- **What happens:** every test of the rules builds its own `Disk::faked` and calls `tick()` itself; the eight main.rs tests that run `start()` (with the real watchdog on their temp folder) assert nothing of it, and no integration test looks at the disk. Switched off one at a time, each of these passes all 18 tests that could see it (those eight and the change's ten): w1, jawd serving opening an unwatched store (`Store::open`), so nothing ever waits and no ballast is kept; w2, the watchdog never started, so no ballast is made; w3, no notices task, so interfaces hear nothing; w4, `status` without the disk; w5, the real reader taking the disk's size for its free space, so it never pauses.
- **Fix:** test jawd serving as it runs. For example, in main.rs's tests or `crates/jawd/tests/starting.rs`: a jawd started on a temp folder has `status.disk` with the free space and size of that folder's disk (checked against `libc::statfs`, a second reader, within the churn of a busy disk), its floor, and once the first tick has run `ballast` 64 MB with `JAW_HOME/ballast` holding its blocks. That catches w1, w2, w4 and w5. For the notices (w3), let `start` take its `Disk` from its caller, so a test can start jawd on a `Disk::faked`, take the disk below the floor and hear the `warning` notice on a connected interface, then the `info` one once it has room.

### T2. Test gap: the checkpointer's PASSIVE pass giving up the ballast

- **Where:** store/mod.rs:2028-2033; the test store/mod.rs `a_checkpoint_a_full_disk_refuses_gives_up_the_ballast_and_goes_in_its_room`.
- **What happens:** the test leaves 20 messages' frames in the WAL, fewer than `WAL_EASY` (1,024), so the checkpointer passes over them (2009) and only its idle TRUNCATE (200 ms in the test) meets the full disk. Switch 11a (only the PASSIVE pass's give-up off) passes the test; 11b (only the idle TRUNCATE's off) fails it, as the builder's 11 (both off) does.
- **Fix:** a test in which a PASSIVE pass meets the full disk: at least `WAL_EASY` frames waiting, or background work held for the WAL, with `idle` long enough that the TRUNCATE can't come first.

### O1. Other: a full disk met while the WAL index grows isn't taken for a full disk

- **Where:** store/mod.rs:1053-1060 (`disk_full`); SQLite 3.53.2 in libsqlite3-sys 0.38.2, sqlite3.c:45346-45372 (`unixShmMap`).
- **What happens:** SQLite grows `jaw.db-shm` by writing a byte to each new page, and a full disk there is `SQLITE_IOERR_SHMSIZE`, not `SQLITE_FULL` (its ordinary writes say `SQLITE_FULL` for `ENOSPC`, sqlite3.c:43898-43903). `disk_full` knows only `SQLITE_FULL`, so a commit refused this way is tried again as transient (write.rs:585-602) with the ballast still kept, and the `error` notice says "jaw can't write to jaw.db (…)" rather than that the disk is full. The window is narrow (the first time since jawd started that the WAL passes 4,062, 8,158 or 12,254 frames, with between 4 KB and 32 KB free), and it heals: after `IDLE` (10 s) without a commit the checkpointer's TRUNCATE either empties the WAL, freeing its room, or meets `SQLITE_FULL` and gives the ballast up. Meanwhile inbox rows wait on the phone, unacknowledged.
- **Fix:** take `SQLITE_IOERR_SHMSIZE` for a full disk too, or give the ballast up on any transient write failure while the last reading of the disk is under the ballast's size.

### O2. Other: the writer's own WAL restart doesn't give up the ballast, and the plan says why wrongly

- **Where:** store/mod.rs:1792-1837 (`bound_wal`, `restart_wal`), called after each inbox group (1456) and live write (1772); the plan's "The ballast" and the builder's report's limits.
- **What happens:** the plan says the writer's checkpoint "runs only as background work is about to go". That holds for the PASSIVE copy in its WAL check (1578-1606), but `bound_wal`'s `wal_checkpoint(RESTART)` runs past `WAL_HARD` after inbox groups and live writes too; on `SQLITE_FULL` it says so and tries again later without giving up the ballast. Nothing stalls on it: once the WAL file's own room is used, a commit meets `SQLITE_FULL` and gives the ballast up, as does the checkpointer's next pass.
- **Fix:** call `disk.full()` where those two checkpoints fail for a full disk, so the rule holds in every place SQLite checkpoints jaw.db; or put the plan's limit right.

### O3. Other: a refused commit while the ballast is being made frees nothing that moment

- **Where:** disk.rs:194-211, 236-263.
- **What happens:** `full()` removes `ballast` only; while `keep_ballast` is making `ballast.partial`, a refused commit finds nothing to give up, and the partial is then renamed into place on a full disk. It needs the disk to go from the floor and 64 MB free to full within the few milliseconds the making takes, and the next refused try (within 1 s for the writer) gives it up.
- **Fix:** optional: `full()` removes `ballast.partial` too, or `keep_ballast` looks at the disk again before the rename.

### O4. Other: a free space that can't be read counts as room, and leaves `paused` as it was

- **Where:** disk.rs:140-141.
- **What happens:** when `statvfs` fails, `room()` says yes without touching `paused`, so background work and downloads go while `status`'s `disk.paused` and `media.paused` still say they wait.
- **Fix:** on a failed read, answer from the last state (`paused.borrow().is_none()`).

### O5. Other: one download can take the disk far below the floor

- **Where:** media/schedule.rs:279-283; media/fetch.rs:149-164.
- **What happens:** the scheduler looks at the floor once per pass, and fetch.rs keeps only 512 MB (64 MB for one asked for) beside each file. At 2.1 GB free, an asked-for 2 GB video starts and leaves about 100 MB, with the ballast's 64 MB beside it all the room the inbox and checkpoints have; several downloads started in one pass each see the free space before the others land.
- **Fix:** keep the floor beside a download (it goes once the free space less its size is above the floor), or have the owner accept that one asked for may go below it.

### O6. Other: the benches don't watch the disk

- **Where:** store/bench.rs:271, 481; feed/bench.rs:99; find/bench.rs:135.
- **What happens:** they open `Store::open`, which reads no disk, so `make perf` never meets the `statvfs` the writer now makes before each background transaction and index step. Its cost is measured below instead.
- **Fix:** bench through `open_watched` on the bench's folder, so a slower reader shows in `make perf`.

### O7. Other: on a network folder the writer would wait on the server

- **Where:** disk.rs:140-165, called on the writer's thread (store/mod.rs:1240, 1634-1654) and on tokio's workers (media/schedule.rs:279, sync/fetch.rs:162).
- **What happens:** jawd doesn't yet refuse a network or synced folder, as the design's "One process" rule says it will. On one, each `statvfs` is a round trip to the server, made before every background transaction and index step on the writer's own thread, so a server that stops answering would hold the writer, and the inbox behind it. jaw.db in WAL mode isn't supported there anyway.
- **Fix:** the design's refusal; or read the disk on the watchdog's own task every `POLL` and have `room()` read what it last found.

### O8. Other: the feed's prune doesn't wait for the disk

- **Where:** store/mod.rs:1947-1968.
- **What happens:** pruning old events is background work, at the writer's start and hourly after a background batch, and it waits for the WAL but not for the disk; on a full disk its `retrying` gives up the ballast for itself. Its transactions are a few pages, it frees rows, and the inbox is committed between its tries, so it costs nothing that matters.
- **Fix:** `wait_for_disk` before it, for one rule everywhere; or leave it, said.

### O9. Other: `statvfs`'s block counts are 32-bit on macOS

- **Where:** disk.rs:301-309 (and media/fetch.rs:208-213 on main).
- **What happens:** `sizeof(f_bavail)` is 4 on macOS (measured). Exact below 16 TiB at 4 KiB blocks, which a Mac's own drive is; `statfs`'s counts are 64-bit.
- **Fix:** optional: read `statfs` on macOS.

### O10. Other: tests that start jawd now need the floor free

- **Where:** main.rs's tests of `start()`, every jawd an integration test starts, and the demo engine's timed runs.
- **What happens:** each watches its temp folder's disk and makes a 64 MB ballast there; on a machine or runner with under 2 GB free (or 5% of a small disk), their background work, indexing and downloads wait, and tests that need them, and the demo's runs that await indexing, time out or hang.
- **Fix:** give the tests' jawd a zero floor, or say so where the tests are documented.

## Measurements

All in one clone of the branch at 56b7c848 under /tmp/jaw-audits/disk-watchdog, built with `CARGO_INCREMENTAL=0`, the machine at a load of 60 to 75 on 20 cores, test runs under `nice`.

- **The change's tests at the head:** its 10 jawd tests pass (7.7 s), and jaw-client's `describe` test.
- **Each rule's switch, applied one at a time** (`git apply` of the switch to 56b7c848, a build, the switch's test, `git checkout`). The builder's 19, each read first to check it switches off its rule and nothing else, each failing its test with the assertion the builder reported:

  | Switch | Fails at |
  | --- | --- |
  | 1. no wait before a background transaction | background work waits for room |
  | 2. a wait that takes no inbox rows or live writes | a live write never waits: Elapsed |
  | 3. indexing not waiting | indexing … waits for room (0, not 1) |
  | 4. the scheduler starting downloads below the floor | nothing downloads, … asked for included (1, not 0) |
  | 5. the scheduler not looking again while it waits | 3EB0ASKED never came to downloaded once there's room |
  | 6. the fetcher not waiting before it asks | not asked for while the disk is short of room (1, not 0) |
  | 7. no notice either way | a notice: Elapsed |
  | 8. going on at the floor, not past the margin | past the floor, but not by the margin |
  | 9. the floor the greater of the two | 25 GB for 2 GB |
  | 10. the writer not giving up the ballast | the row is committed in the ballast's room: Elapsed |
  | 11. the checkpointer not giving it up, both places | the checkpoint the full disk refused gave up the ballast: Elapsed |
  | 12. made while short of room | not made while the disk is short of room |
  | 12b. made before the margin | nor once it has room for it, while it hasn't the margin |
  | 13. never made again | made again once there's room: NotFound |
  | 14. a partial left | what a stop left as it was made goes, short of room as it is |
  | 15. one of another size kept | made again at its size (10, not 1,000,000) |
  | 16. a sparse one | it holds its room: 0 blocks |
  | 17. `media.paused` without the disk's reason | downloads say why they wait |
  | 18. `describe` without the disk's line | the line missing |

  And seven of the audit's, for rules it found no test of: 11b (only the idle TRUNCATE's give-up off) fails the checkpoint test as 11 does; 11a (only the PASSIVE pass's off) passes it (T2). w1 to w5 (T1) each pass all 18 tests that could see them, the eight main.rs tests that run `start()` and the change's ten (five in disk::tests, three in the store, one in media, one in sync).
- **P1's reproduction.** An audit-only test in the clone's sync/tests.rs, not in the branch: a store opened with `open_watched` on `Disk::faked` (10 GB free of 500 GB, no ballast), one message row put in the inbox and committed, the disk then set to the case's free space and the converter started as jawd starts it (`converting`), and 500 ms later a live message row; counted 5 s after the live row, then the disk given 10 GB and both awaited. With 10 GB free: 2 messages, 0 inbox rows. With 1 GB free: 0 messages, 2 inbox rows, and both converted once there was room. One run of each.
- **What a reading of the disk costs.** A C program making the call `free_space` makes (`statvfs` of a folder on the Data volume, APFS), 500,000 times, each timed with the monotonic clock, under `nice` at a load of 74: p50 2 µs, p99 5 µs, p99.9 10 µs, p99.99 264 µs, the worst 4.5 ms (the scheduler at that load, not the call; 50,000 calls on `~/Library`: 0.97 µs each, worst 25 µs). The paths it guards make one each: the writer before each background transaction (whose budget is 3 ms: 0.07% at p50, 0.3% at p99.9), outside the transaction, so it holds no lock; each indexing step, as long; each media scheduler pass; each history blob; the watchdog every 10 s; and while something waits, one every 250 ms and one after each inbox row or live write the writer takes meanwhile. A download also makes media's own reading, as on main. A live write's 5 ms p99 isn't touched: nothing is added on its path.
- **The ballast on APFS, as disk.rs makes it** (`F_PREALLOCATE` with `F_ALLOCATEALL` from the physical end, then `ftruncate`; three runs, 64,000,000 bytes, on the Data volume while other builds wrote to it): `fst_bytesalloc` 64,000,000, `st_blocks` × 512 = 64,000,000; the free space fell 63,979,520, 61,288,448 and 64,000,000 bytes as it was made, and rose 62,951,424, 62,943,232 and 63,172,608 bytes the moment it was unlinked. So it holds its room, and gives it back at once where no snapshot holds it.
- **A real full disk wasn't made:** attaching a 1 GB APFS disk image is refused in this sandbox (`hdiutil: attach failed - Permission denied`), and the real disk isn't to be filled. The full-disk paths are judged by reading them and SQLite's source, and by the change's tests, whose VFS hook fails jaw.db's writes with `SQLITE_FULL` while the fake disk is empty.
- **The full suite at the head,** each target's binary of `cargo test --workspace` run on its own, under `nice`:
  - jawd's unit tests: 1,003 passed, 2 failed, 17 ignored (2,576 s). The two are on the landers' list and each passed alone 3 of 3: the feed's lane test `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` and the socket's connection-full test `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`.
  - jawd's integration tests: installed 1, starting 1, upgrading 1, stopping 3 of 4. The fourth, `jawd_whose_engine_never_answers_stops_on_sigterm_and_takes_its_engine_with_it` ("the engine outlived jawd", which it gives 1 s after jawd exits), is one of the list's stopping tests and passed alone 3 of 3.
  - jaw-voice: 35 passed, 1 failed, 2 ignored; the failure is the list's recording-end test, `a_recordings_end_is_held_for_a_caller_that_falls_behind_and_never_dropped`, which passed alone 3 of 3. This branch doesn't touch jaw-voice.
  - Everything else passed: jaw's unit tests 345 and its contacts 1, composing 3, settings 3, failures 10 and typing 2; jaw-client 27 and its tests against a real jawd 3; jaw-proto 25 and fixtures 3; jaw-media 28 and pipeline 18; wa-proto's roundtrip 3.
  - The integration tests that start jawd find it beside their own binary, so those ran from `target/debug` at the head, before any switch was built. There are no doc tests to run: the workspace's one fenced block in a doc comment is in jawd (convert/spans.rs), a binary crate, which cargo doesn't doc-test. The Go engine's tests weren't run: the branch changes nothing under `engine/`.
- **Format:** every changed Rust file at 56b7c848 passes `rustfmt --check` under the repo's `rustfmt.toml`. The gate's other steps are the builder's (check-generated 0, lint 0, build 0); this branch changes no Go and no protos.
- **The merge:** `git merge-tree` of 56b7c848 onto main's 1669937a, in the clone: one conflict, the `status` row of protocol/README.md's table, which M2 extended with `quarantined` and `awaiting` and this branch with `disk`.

## Landing, once approved

- **The trailer:** 56b7c848 and every later commit that touches the data layer (store/, sync/, and disk.rs and disk/, which the change adds to check-data-audit.py's list) take `Data-Audit: docs/audits/2026-11-02-disk-watchdog.md`. c0f14e92, the plan, touches no listed path and needs none; the script asks the trailer only of commits that do.
- **No migration:** no schema change and no SQL, so nothing to number or renumber after main's migration 29, and the plan test is unchanged.
- **What its lander re-checks after main's landings:** the README's `status` row, keeping both sides; `make check-generated` for `status.result.json` and jaw-proto's `Status`, which M2 changed beside `disk`; that each `Lane::Background` caller main added since ea0f4134 (chat actions, refresh) is background work, now that the lane waits for the disk, and none is on a live path (P1's kind); that any commit or checkpoint path main added gives up the ballast on `disk_full`; and the change's tests and its switches again on the rebased head.

## Verdict

Changes needed before landing: P1 (live messages converted below the floor whatever the inbox held at a start, with a test that fails without the fix), T1 (jawd serving's wiring, its status and the real reader tested) and T2 (the PASSIVE pass's give-up tested), each new test failing with its rule switched off. O1 to O10 may stand as follow-ups.

Verdict: changes needed

# Round two: 59a09906

Range: 1669937a..59a09906 in disk-watchdog, 2 and 3 November 2026. Auditor: database-expert, a fresh one for this round; a second took it over at 06:43 on 3 November, in the first's scratch, when the first was stopped by a crash of Cursor's extension host. Seven commits: 52df51a8 the plan and 9733c357 the change (round one's c0f14e92 and 56b7c848, rebased from ea0f4134), then one per finding of round one: 0f7999cc P1, 803981fc T1, 8fd44dfa T2, 076fc36c O4, 59a09906 O2. origin/main was 73c986bb when this round began and is 5bd7f6e8 now; the branch merges onto it with no conflict (see Landing).

## In short

Nothing is lost and nothing is left inconsistent, and P1 is met: below the floor, the rows left in the inbox at a start and the live rows behind them are converted at once, whatever the inbox held, and with room a stream of background work holds them up no longer than one of its batches. A leftover and the live rows after it end as they would in order, across a restart, with replays and a row quarantined again at each start. T1, T2, O4 and O2 are tested as round one meant: each new test fails with its rule switched off, and every one of the fix round's 33 switches fails its test. The test-only switch can't be reached outside jawd's unit tests. Since the branch's base, main has changed nothing in jawd but its tests, and the suite at the head fails only where the landers' list says it may.

But two of the rules P1's fix brings, both in the plan, have no test that fails without them, so this round asks for changes: taking the leftovers ahead of other background work (T3) and in short transactions (T4). Switched off, the first lets a stream of background work hold a leftover and the live row behind it for 7.6 s, and the second holds live writes up to 25 ms behind each batch of leftovers, and every test passes either way. O11 and O12 may stand as follow-ups.

## What was asked, checked

- **P1, as round one meant it.** Below the floor, the rows left in the inbox at a start no longer wait for the disk, nor do the live rows behind them.
  - **How it's done:** the converter hands a batch of leftovers to `Store::apply_leftovers` (sync/drain.rs:395), a queue of its own into the writer, bounded at 2 (store/mod.rs:705), counted in the writer's `queued_background` (1015). The writer takes it after inbox rows and live writes and ahead of other background work (1223-1226), and also while other background work waits for the disk (1691-1694). `background_changes(…, true)` skips `wait_for_disk` (1984-1986) and keeps `wait_for_room`, the short transactions (1990) and `between()`. The leftovers' link-preview thumbnails go the same way (drain.rs:314).
  - **Order.** The converter still awaits each batch's commit before it reads the next (drain.rs:394-421), and a batch counts as leftovers only if its last row was in the inbox when the converter started, so a leftover and every live row after it keep their order, as on main. The thumbnails' rows are applied on their own task after their batch, as on main; a thumbnail row is an insert keyed by its file's hash that only bumps its message's version (store/write/links.rs:133-154), so a late one is harmless. What jumps ahead is other background work only: history's blobs, history kept raw, the backfills, refresh's contacts and groups, media's steps, avatars, merges, folders and the outbox's regrouping. Each of those already raced the leftovers on main (separate tasks feeding one lane, and live rows overtaking them), so no merge rule could rely on the leftovers coming after them. The account's JIDs, which conversion needs first, are committed in the live lane before the converter gets them (sync/mod.rs:271-288).
  - **Probe 1, across a restart** (audit-only, in the clone's sync/tests.rs, with a `#[cfg(test)]` hook in store/write.rs failing one message's write). Below the floor, with history's copies of m1 to m3 waiting in the writer: leftovers (three messages, a reaction before its message, a group message from a LID with its phone number, and the poisoned row) and live rows after them (an edit of m1, the reaction's message, a revoke of m2, your reaction to m3, a sender renamed). Then a restart below the floor with replays left in the inbox (m1 as it came, its edit, the revoke, the early reaction), history's blob again waiting, and one live row more. The end state equals, line for line, both controls (with room, history first and history last), in all six runs at the head: m1 edited, m2 deleted, both reactions, the LID joined to the phone number, nothing pending, and one quarantine row after two starts. The live rows were converted 20 ms and 15 ms after arriving, at the first start and the second, at a load of 76 on 2 November; on 3 November, with the suite and other agents' builds running (load about 60), 58 to 937 ms at the first start and 42 ms to 1.8 s at the second. That is the machine's: probe 3, with room and no wait for the disk at all, slowed from 2 to 13 ms to 20 to 239 ms under the same load, every inbox commit and live write being a full flush.
  - **A leftover quarantined again at every start.** It stays in the inbox, is converted again through `apply_leftovers` at each start, below the floor too, and is kept again by an upsert on `(source, ref)` (store/write.rs:682-688): one row in `quarantine`, its error and time updated. The converter moves past it (drain.rs:401-411), and the live rows behind it are converted at once (probe 1).
  - **With room, behind a stream of background work** (probe 3, audit-only): three tasks applying 400-message batches in the background lane keep the writer's queue from emptying while a leftover and a live row after it are converted. At the head the live row is converted 13 ms and 2.4 ms after it arrives at a load of 16, and 20 ms and 239 ms at about 60. With the leftovers taken after other background work (P1e), 7.6 s: until the stream stopped (T3).
  - **Starvation** (probe 2, audit-only): 20,000 leftovers converting while a task makes a live write every 5 ms. At the head, at a load of 16, the leftovers took 3.3 s with room and 3.4 s below the floor, and the live writes made meanwhile took p50 1.6 ms, p90 2.6 to 3.2 ms, p99 6.1 to 7.0 ms; with no leftovers, p50 0.23 ms, p99 2.9 ms. With the leftovers in the background lane, as on main (P1a+), the live writes' tail is the same (p99 6.3 ms with room), so it's the background lane's short transactions, as on main, not this change's. Unpaced (P1i), each batch of 256 leftovers becomes one transaction of about 20 ms and the live writes go to p90 20 ms and p99 25 ms (97 ms at a load of 76), with every test passing (T4).
  - **The rest of the order, switched:** the leftovers taken ahead of inbox rows and live writes (P1h) passes everything and needn't fail: a background transaction yields 1 ms (`YIELD_AFTER`) after an inbox row or a live write starts waiting, and `between()` takes it, so the order at the loop's top changes a live write's wait by at most that (live writes p99 7.2 to 9.9 ms, within the head's noise). The leftovers in the live lane (P1j) pass everything too: that is round one's own first suggested fix, which meets P1, at some cost to live writes' tail (p99 9.7 to 12.7 ms).
- **T1, T2, O4 and O2, as round one meant them.** Each new test passes at the head, and each fails with its rule switched off; the switches were applied in the audit's clone, one at a time, to the head, each test binary run directly.
  - **T1** (803981fc). `start` builds the watch serving uses, `Disk::watch(home, Config::default())`, and hands it to `start_watching` (main.rs:595-600), which a test can start on `Disk::faked`; serving and the demo both start through `start_on`, which calls `start` (main.rs:493, 588; demo/mod.rs:238). `jawd_serving_says_its_folders_free_space_size_and_floor_and_keeps_its_ballast_there` goes through `start` itself, so serving's own watch is under test: `status` over the socket gives the folder's disk's size as `statfs` does, its free space between two `statfs` readings taken around a fresh `room()`, the floor, nothing paused, and, once the first tick has run, the 64 MB ballast holding its blocks. `an_interface_connected_to_jawd_hears_when_its_disk_goes_below_the_floor_and_when_it_has_room_again` hears the `warning` and then the `info` notice on a connected interface. w1 (serving opening an unwatched store), w2 (no `disk.start()`) and w4 (`status` without the disk) fail the first at "the watchdog's first look makes the ballast"; w5 (the real reader taking the disk's size for its free space) fails it at the free space ("994662584320 bytes free, where statfs says 373621796864"); w3 (no notices task) fails the second at "both notices". Each passes the other test.
  - **T2** (8fd44dfa). `a_checkpointers_pass_a_full_disk_refuses_gives_up_the_ballast_and_the_next_copies_the_wal_in_its_room` fills only jaw.db itself (`noted::fill` matches whole path components, so the WAL isn't), so commits go on in the WAL and only a checkpoint meets the full disk; `idle` is 60 s, so the idle TRUNCATE can't come first; live writes go on until the ballast is given up or 6,144 frames wait, past `WAL_EASY` (1,024), where the checkpointer's PASSIVE pass begins, and short of `WAL_HARD` (15,360), where the writer's restart would. The pass gives the ballast up, no commit is refused, and the next pass copies the WAL. 11a (only the pass's give-up off) fails it ("the pass the full disk refused gave up the ballast") and passes the TRUNCATE test; 11b (only the TRUNCATE's off) fails the TRUNCATE test and passes it; 11 (both) fails both.
  - **O4** (076fc36c). While the free space can't be read, `room()` answers `paused.borrow().is_none()` (disk.rs:143): as the disk was when last read, which is room if never read, so `status` says what happens; an unwatched store, never read, always has room. The test covers never read, short of room when last read (work waits, `status` says so with the free space last read, and goes on once read with room) and room when last read. O4's switch (`return true`, 56b7c848's rule) fails it at "short of room when last read, so work still waits".
  - **O2** (59a09906). `bound_wal`'s RESTART and the WAL check's PASSIVE copy call `disk.full()` on `disk_full` (store/mod.rs:1867 and 1642). With the checkpointer's two passes (2063, 2093) and `failing`, which every retried commit goes through, the writer's TRUNCATE at a stop among them, the ballast goes wherever SQLite checkpoints jaw.db while jawd runs: only the writer commits, with `wal_autocheckpoint` off. O2a (the restart's give-up off) fails `a_restart_of_the_wal_a_full_disk_refuses_…` and passes the other; O2b (the copy's) fails `the_writers_copy_of_the_last_frames_a_full_disk_refuses_…` and passes the other.
  - **Round one's and the builder's 19** fail as reported (the table in Measurements).
- **The rebase, checked again.** ea0f4134..1669937a (103 commits) adds `Lane::Background`, BEGIN/COMMIT, `wal_checkpoint` and `transaction_with_behavior` only in tests. Chat actions write through `settle` (the live lane) and `apply_durable` (the live queue); `fetch_again` and refresh's `resynced` hold the app-state turn only across the engine's call. Below the floor, refresh's contacts and groups wait for room, and the full app-state refetch, the LID lookups and avatars behind them on refresh's own task; live app-state patches still convert through the inbox. Since 1669937a, main's 93 commits (to 5bd7f6e8) change, outside `mac/` and `docs/`, only test code in jawd (the flaky tests' round: store/mod.rs's and outbox.rs's changes are inside `mod tests`), the terminal's `until:` bound (b33dcba1), and the Makefile's `perf` target with scripts/perf.py: no commit or checkpoint path, no background-lane caller, nothing in the protocol, jaw-proto, jaw-client, the engine or its protos. Every `Lane::Background` caller at the head predates this round.
- **The test-only switch.** `Wal::parked` (store/mod.rs:262-264, 301-302, 2047-2050) exists only under `#[cfg(test)]`: the field, its initialiser and the checkpointer's check of it; only T2's and O2's tests set it (3500). No build but jawd's own unit-test one has the field: the non-test jawd at the head builds without it, and the integration tests and the demo run that jawd. No flag, variable or setting reaches it.
- **The full suite at the head** fails only where the landers' list says it may (details in Measurements). jawd's unit tests failed four of 1,183, each passing alone 3 of 3: three on the list (the feed's half-closed-connection and lane tests, the socket's connection-full test) and the WAL test with history pouring in, which was on the list at the head's base and which main's flaky tests' round has since made to judge live writes on their median (0156ee3e); since that round landed the list watches it rather than retrying it, and its new version passes on the merge onto today's main (Measurements). The watchdog adds nothing on its path: it runs on an unwatched store, whose `room()` answers yes without reading anything. The feed's `a_watch_is_answered_while_a_request_sent_before_it_waits_on_the_writer` passed. Every other target passed.
- **The terminal's two tests** (`when_whatsapp_wants_a_newer_jaw_the_header_says_update_needed_and_reading_still_works` and `an_engine_that_crashes_shows_reconnecting_with_the_chats_kept_until_jawd_has_started_it_again`) don't fail at the head: failures.rs whole 5 of 5 and each alone 3 of 3, at a load of 54 to 68. The watchdog's start can't be why they failed once in the fix round. The branch doesn't touch the terminal. A jawd that starts with room sends no notice: `room()` changes `paused` only when the free space crosses the floor (disk.rs:149-166), and `tell` speaks only on a change (289-301). The terminal draws nothing of the disk, and the ballast is made once, at the first tick, on a blocking thread, before the tests' `ready()`. Their failures were snapshot timing on paths the change doesn't touch: a frame drawn before the "hi" just sent came back (a send is a durable live write, which never waits for the disk), and a demo presence push ("online" against "seen 2 h ago") between the two frames compared.

## Findings

Nothing is lost and nothing is left inconsistent, and at the head live writes wait behind the leftovers no longer than they did on main: no finding is data loss, inconsistency or unpredictable performance. Two rules P1's fix brings have no test that fails without them (T3, T4), so this round asks for changes. O11 and O12 may stand as follow-ups, with round one's O1, O3 and O5 to O10.

### T3. Test gap: nothing tests the leftovers going ahead of other background work

- **Where:** store/mod.rs:1223-1226 (the writer's loop takes `leftovers` before `background`); the plan's "taken after live writes, ahead of other background work, and while it waits for the disk, so they don't wait behind it either"; the change's one test of it, sync/tests.rs:1738.
- **What happens:** that test runs below the floor, where background work waits inside `wait_for_disk` and the leftovers are taken there (1691-1694), so it never has background work going with room. Switch P1e (the leftovers' arm moved after the background arm in the writer's loop, nothing else) passes it, and the audit's probes 1 and 2. The audit's probe 3: with room, three tasks applying 400-message batches in the background lane keep the writer's queue from emptying while a row left in the inbox and a live row after it are converted. At the head the live row is converted 13 ms and 2.4 ms after it arrives; under P1e it waits 7.6 s, until the batches stop. So a stream of background work with room (history pouring in after a link, a backfill, media's steps) would hold up the leftovers and every live row behind them, round one's P1 with room rather than below the floor, and nothing would catch it.
- **Fix:** a test with room in which background work keeps coming from more than one task while a row left in the inbox and a live row after it are converted, that fails with P1e. Probe 3 (Measurements) is one; judged on progress rather than the clock, as the flaky tests' round asks, it would keep the stream going until the live row is converted (30 s at most) and check the stream was still going when it was.

### T4. Test gap: nothing tests the leftovers' short transactions

- **Where:** store/mod.rs:1990 (`w.transaction(changes, rest, true, false)` in `background_changes`, the leftovers' path since 0f7999cc); `apply_leftovers`' "in short transactions behind inbox rows and live writes" (845) and the plan's "converted as background work, so they never hold up live writes".
- **What happens:** on main the leftovers went in the background lane, whose short transactions main's WAL test (`the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it`) holds, judging that live writes go in while history does. Now they come on a queue of their own, which no test drives with live writes going. Switch P1i (`!leftovers` for `background` in that call, so each batch of up to 256 leftovers is one transaction) passes every test, the audit's three probes among them, while live writes made as 20,000 leftovers convert go from p90 2.6 to 3.2 ms and p99 6.1 to 7.0 ms to p90 20 ms and p99 25 ms, only 216 and 220 of them going in rather than 389 and 404 (at a load of 16; at 76, p99 97 ms). Each of those transactions runs about 20 ms, by the live writes' p90, against the 3 ms background budget.
- **Fix:** a test that fails with P1i. For example, `apply_leftovers` of more than `BATCH_ROWS` changes takes more than one transaction (`Applied::transactions`), and a live write sent meanwhile goes in before they're all in; or, as main's WAL test does for history, live writes keep going in while thousands of leftovers convert, judged on how many go in.

### O11. Other: nothing tests that `status` counts the leftovers' queue

- **Where:** store/mod.rs:1015.
- **What happens:** switch P1f (the leftovers' queue left out of `queued_background`) can fail no test: none reads the store's `queued_background`, as none did on main. The queue is bounded at 2 by construction (the converter's batch and its thumbnails', each awaited), so what goes untested is only that `status` shows it.
- **Fix:** a test of the writer's queues in `status` with work waiting, or leave it as on main.

### O12. Other: P1's test judges the live row on a 2 s clock

- **Where:** sync/tests.rs:1771 (`took < Duration::from_secs(2)`).
- **What happens:** below the floor the live row is converted while history waits for room, which the test also checks (1774), so what it holds is the order, not the time. Under load the time grows: the audit's probe 1, with a few more rows, took up to 1.8 s at a load of about 60. The test passed 3 of 3 at a load of 59, but its 2 s is the kind of bound the landers' flakes list collects.
- **Fix:** let it wait as long as `until` does, or longer, and keep the checks that history still waits; under main's rule the live row never converts while the disk is short of room, so any wait catches it.

## Measurements

All in one clone of the branch under /tmp/jaw-audits/disk-watchdog-r2, built with `CARGO_INCREMENTAL=0`; the probes are scratch commits on the head (a93f6399, ad0bc40e), never in the branch. On 2 November the machine was at a load of 60 to 80 on 20 cores, on 3 November at 8 to 16 for the probes and switches and 50 to 70 for the suite; test runs under `nice`.

- **The change's tests at the head:** its 17 jawd tests pass (10.6 s), and jaw-client's `describe` test.
- **Each rule's switch, applied one at a time** to the head (an exact replacement found once, a build, the test binary run directly, `git checkout`):

  | Switch | Fails at |
  | --- | --- |
  | 1. no wait before a background transaction | background work waits for room |
  | 2. a wait that takes no inbox rows or live writes | a live write never waits: Elapsed |
  | 3. indexing not waiting | indexing the live message's text waits for room (0, not 1) |
  | 4. the scheduler starting downloads below the floor | nothing downloads, what an interface asked for included (1, not 0) |
  | 5. the scheduler not looking again while it waits | 3EB0ASKED never came to downloaded once there's room |
  | 6. the fetcher not waiting before it asks | not asked for while the disk is short of room (1, not 0) |
  | 7. no notice either way | a notice: Elapsed |
  | 8. going on at the floor, not past the margin | past the floor, but not by the margin |
  | 9. the floor the greater of the two | 25 GB for 2 GB |
  | 10. the writer not giving up the ballast | the row is committed in the ballast's room: Elapsed |
  | 11. the checkpointer not giving it up, both places | both checkpointer tests |
  | 12. made while short of room | not made while the disk is short of room |
  | 12b. made before the margin | nor once it has room for it, while it hasn't the margin |
  | 13. never made again | made again once there's room: NotFound |
  | 14. a partial left | what a stop left as it was made goes |
  | 15. one of another size kept | made again at its size (10, not 1,000,000) |
  | 16. a sparse one | it holds its room: 0 blocks |
  | 17. `media.paused` without the disk's reason | downloads say why they wait |
  | 18. `describe` without the disk's line | the line missing |
  | P1a. the leftovers in the background lane (56b7c848's rule) | both messages, the disk short of room |
  | P1b. the leftovers waiting for the disk on their own queue | both messages, the disk short of room |
  | P1c. not taken while background work waits for the disk | both messages, the disk short of room |
  | P1d. their thumbnails in the background lane | the left row's thumbnail |
  | 11a. the checkpointer's pass not giving up the ballast | the pass the full disk refused gave up the ballast (T2's test only) |
  | 11b. its idle TRUNCATE not giving it up | the checkpoint the full disk refused gave up the ballast (that test only) |
  | O4. a free space that can't be read counting as room | short of room when last read, so work still waits |
  | O2a. the writer's restart not giving up the ballast | the restart the full disk refused gave up the ballast (that test only) |
  | O2b. the writer's copy of the last frames not giving it up | the writer's copy … gave up the ballast (that test only) |
  | w1. serving opening an unwatched store | the watchdog's first look makes the ballast |
  | w2. serving's watchdog never started | the watchdog's first look makes the ballast |
  | w3. no notices task | both notices: Elapsed |
  | w4. `status` without the disk | the watchdog's first look makes the ballast |
  | w5. the real reader taking the size for the free space | 994662584320 bytes free, where statfs says 373621796864 |

  And this round's own, alone against the change's P1 test and the audit's three probes (3 November, load 16): P1a+ (56b7c848's rule) fails the change's test, probe 1 and probe 2 below the floor; P1e fails probe 3 only (T3); P1f can fail nothing, as no test reads what it changes (O11; its one run, of the whole binary at a load of 76, reached its 900 s limit); P1h, P1i and P1j fail nothing (P1h and P1j needn't; P1i is T4). The first run of P1e and P1j, beside 140 other tests at a load of 76, failed probe 2's 30 s limit; alone, both pass it, so that was the load.
- **Probe 1** (`audit_leftovers_and_the_live_rows_after_them_end_as_in_order_below_the_floor_across_a_restart`): a store on `Disk::faked` (10 GB free of 500 GB, no ballast), the leftovers put in the inbox and committed, the disk set to 1 GB, history's blob applied in the background lane and seen waiting, the converter started as jawd starts it, the live rows 500 ms later; then a new store on the same folder, the replays left in the inbox, history again, the converter, one live row more. The state (messages with their text, kind and deletion, reactions, people and their JIDs, pending counts, quarantine) compared line for line with two controls with room, history applied before and after. Equal in all six runs at the head; the live rows converted 20 ms and 15 ms after arriving at a load of 76, and 42 ms to 1.8 s at about 60.
- **Probe 2** (`audit_leftovers_never_starve_live_writes_with_room_or_below_the_floor`, `AUDIT_LEFT=20000`; `AUDIT_LEFT=0` for the baseline): 20,000 one-line messages over 300 chats committed to the inbox, the disk set to the case's free space, history's blob applied, the converter started, and a task making a `Remember` live write every 5 ms, each timed from its call to its answer, until the inbox is empty and at least 3 s have passed.

  | Rule (load) | Case | Leftovers took | Live writes | p50 | p90 | p99 | Most |
  | --- | --- | --- | --- | --- | --- | --- | --- |
  | head (76) | with room | 3.25 s | 361 | 1.62 ms | 3.42 ms | 11.29 ms | 15.34 ms |
  | head (76) | below the floor | 3.19 s | 347 | 1.67 ms | 4.15 ms | 9.90 ms | 19.14 ms |
  | head (16) | with room | 3.27 s | 389 | 1.56 ms | 2.59 ms | 6.07 ms | 7.68 ms |
  | head (16) | below the floor | 3.42 s | 404 | 1.58 ms | 3.21 ms | 6.89 ms | 10.62 ms |
  | no leftovers (16) | below the floor | — | 411 | 0.23 ms | 0.44 ms | 2.93 ms | 5.85 ms |
  | main's rule, P1a+ (16) | with room | 3.26 s | 400 | 1.57 ms | 2.87 ms | 6.27 ms | 11.27 ms |
  | unpaced, P1i (16) | with room | 3.06 s | 216 | 0.85 ms | 19.86 ms | 25.67 ms | 29.80 ms |
  | unpaced, P1i (16) | below the floor | 3.03 s | 220 | 0.53 ms | 19.85 ms | 24.91 ms | 33.20 ms |
  | unpaced, P1i (76) | with room | 3.91 s | 157 | 15.11 ms | 39.94 ms | 96.73 ms | 121.98 ms |

  Under main's rule below the floor the leftovers never convert while the disk is short of room (P1).
- **Probe 3** (`audit_with_room_a_leftover_and_the_live_row_after_it_never_wait_behind_a_stream_of_background_work`): a store on `Disk::faked` with room, one message row committed to the inbox; three tasks each applying batches of 400 group messages in the background lane, one after another, for up to 8 s; 300 ms later the converter started, 200 ms after that a live row; timed from the live row's commit to the inbox until the inbox is empty (both converted), polled every 5 ms, and judged under 2 s. Head: 13.3 ms (34 batches meanwhile) and 2.4 ms (35). P1e: 7.58 s (329 batches in 8.08 s). P1a+: 60 ms; P1h: 76 ms; P1i: 80 ms; P1j: 407 ms.
- **The full suite at the head,** each target's binary of `cargo test --workspace` run on its own from its crate, under `nice`:
  - jawd's unit tests (2 November, load 76, 3,031 s): 1,160 passed, 4 failed, 19 ignored. Each failure passed alone 3 of 3: `a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends` (2, 2, 2 s) and `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` (6, 4, 7 s), the socket's connection-full test (11, 11, 10 s), and `the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it` ("a live write waited 1.17 s"; 4, 4, 5 s).
  - The rest (3 November, load 54 to 68, built at 59a09906): jaw's unit tests 350 (2 ignored), composing 3, contacts 1, failures 10 (five runs), settings 3, typing 2; jaw-client 28 and its tests against a real jawd 3; jaw-media 28 and pipeline 18 (1 ignored); jaw-proto 25 (1 ignored) and fixtures 3; jaw-voice 36 (2 ignored); jawd's installed 1, starting 1, stopping 4, upgrading 1; wa-proto's roundtrip 3. All passed.
  - The integration tests that start jawd find it beside their own binary, so a non-test jawd was built at 59a09906 first. The Go engine's tests weren't run: the branch changes nothing under `engine/`.
  - The change's 17 tests also passed 3 of 3 at a load of 59 (9.5, 8.9, 9.0 s).
- **The merge onto today's main:** 59a09906 merged onto 5bd7f6e8 in the clone with no conflict (git merged store/mod.rs and sync/tests.rs itself; disk.rs and sync/drain.rs are as at the head). Its jawd unit tests built, and store's, sync's, disk's, send's and the outbox's tests, the two T1 tests and media's below-the-floor test, 319 in all, ran at a load of 60 to 68: 316 passed, 3 ignored, none failed (314 s). Among them were main's new versions of the two WAL-bound tests and its new test of a blob kept in the spool while its drain can't commit.

## Landing, once approved

- **The trailer:** by check-data-audit.py's list at the head, 9733c357, 0f7999cc, 8fd44dfa, 076fc36c and 59a09906 touch the data layer (store/, sync/, disk.rs and disk/) and each takes `Data-Audit: docs/audits/2026-11-02-disk-watchdog.md`, as does any commit the fixes for T3 and T4 add there; 52df51a8 (the plan) and 803981fc (main.rs and the plan) touch no listed path and need none. The script reads the note's last `Verdict:` line, so round three's must end it.
- **No migration:** no schema change and no SQL, so nothing to number or renumber, and the plan test is unchanged.
- **What its lander re-checks against main as it then is:** that main still changes nothing in jawd outside tests, or, for each `Lane::Background` caller it adds, that it's background work and on no live path (P1's kind), and for each commit or checkpoint path, that it gives up the ballast on `disk_full`; the README's `status` row and `make check-generated` if main touches `status`; the change's tests and the switches above, T3's and T4's among them, on the rebased head; and the gate. The merge onto 5bd7f6e8 was checked here (Measurements).

## Verdict

Changes needed before landing: T3 (a test that fails with the leftovers taken after other background work) and T4 (a test that fails with the leftovers' transactions unpaced). Everything else round one asked for is done and tested. O11 and O12 may stand as follow-ups.

Verdict: changes needed

# Round three: ab92c9be

Range: bab95fc2..ab92c9be in disk-watchdog, 3 November 2026. Auditor: database-expert, a fresh one for this round; when the owner's window reloaded at 14:58 a second was started on the same brief while this one went on, and it checked T5, the resolutions, the trailer, the voices backfill and the landing's failures again on its own, finding the same, and ran store's and sync's tests once more at the landing and the head (O16). Eleven commits: round two's seven, rebased onto bab95fc2 (main with the drain fix), 657fb643 the plan, 74ad432f the change, e7f4fbd9 P1, 0b52b7f2 T1, 8c21ae54 T2, 3e15849a O4 and d615fa4b O2; then one for each item round two left: a7456273 T3, 29b3aa9c T4, 8b34377f O12 and ab92c9be O11. origin/main is b4d9624d; onto it the branch conflicts in two files, each resolved by keeping both sides (see Landing).

## In short

Nothing is lost and nothing is left inconsistent. T3, O11 and O12 are met as round two meant them: each test fails with its rule switched off, and judges the rule on the stream still coming, on `status` and on the order, not on the clock; but T3's and O11's tests still wait in their setup on 5 s clocks that the suite's load can outlast, and each failed so at the landing (O16). Every switch of rounds one and two and of the fix round that should fail still fails at the head, at the assertion it failed at before (38 of them), and the two that needn't still pass; of this round's four, T4a fails and P1k, T4b and T4c pass, which is what T5, O13 and O14 are about. The rebase composes: below the floor, a batch of leftovers that main's back-off holds (its conversion panicking outside every row's catch) converts once it can, with its link preview's thumbnail, and ends exactly as a batch that never panicked; nothing is acknowledged on that path. The watchdog can't be why sync's stop bound failed once in the fix round. The suite at the head fails only where the landers' list says it may; at the landing, T3's test also failed once, in its setup (O16). The branch lands on today's main with two conflicts, each keeping both sides, and main's voices backfill waits for the disk as background work should.

But T4's test can't see what keeps the leftovers' transactions short in jawd. It applies 2,000 changes, twice BATCH_ROWS, so the 1,000-row cap alone splits them; a converter batch is 256 inbox rows, far under 1,000 changes, so in jawd only the 3 ms cap and the early end for a waiting live write split one. With those two switched off for the leftovers (T4b), every test passes, T4's included, while live writes made as 20,000 leftovers convert go from p99 6 and 10 ms (with room and below the floor) to 28 and 30 ms, round two's measured cost of P1i. So this round asks for one change, T4's test with fewer changes than BATCH_ROWS (T5); the stale comment in the drain fix's test (O15) and the 5 s clocks in T3's and O11's tests (O16) are best changed with it. O13 and O14 may stand as follow-ups.

## What was asked, checked

- **T3** (a7456273), `sync::tests::with_room_the_rows_left_in_the_inbox_and_the_live_row_after_them_go_ahead_of_a_stream_of_background_work` (sync/tests.rs:2087).
  - **What it does:** with room, four tasks each send their next batch of 400 history messages in the background lane once their last is in, so background work always waits for the writer, until told to stop or for 30 s; READ (256) rows left in the inbox, so the converter's first batch is all leftovers whenever the live row comes, then a live row. It passes when the inbox empties while the stream hasn't run out, and the live row is its chat's newest.
  - **Judged on the stream:** the 30 s is the stream's own end, there so the test ends under P1e; the head passes on the stream still coming (0.6 s, 5 of 5 alone at a load of 35 to 65), and P1e fails it only once the stream ends ("converted only once background work stopped coming, after 1228 batches of it").
  - **Switched:** P1e (the leftovers' arm after the background arm in the writer's loop) fails it and the audit's probe 3 (7.56 s, until the stream stopped) and nothing else. My own P1k (with room, the leftovers in the background lane behind what's queued there, their own queue only below the floor) passes it: a FIFO of at most 16 batches holds them up a batch or a few, not until the stream ends (probe 3: 12 ms). That is O13.
  - **Under the suite's load:** in the full suite at the landing it failed once, in its setup ("waited too long for background work coming": `until`'s 5 s passed before the four producers had their first four batches of 400 in), not at the rule; it passed alone 3 of 3 and in each of 3 runs of sync::tests beside store::tests. That is O16.
- **T4** (29b3aa9c), `store::tests::the_rows_left_in_the_inbox_go_in_short_transactions_with_a_live_write_going_in_between` (store/mod.rs:3560).
  - **What it does:** an outside BEGIN IMMEDIATE holds the leftovers' first transaction of 2 × BATCH_ROWS changes until a live write waits behind it (`queued_interactive` > 0), the writer's prune at its start done first; once the lock goes, the leftovers take more than one transaction, and the live write, which makes no event, reports a seq after the feed's last before them and before their last. Judged on transactions and seqs, not on the clock: 5 of 5 alone.
  - **Switched:** P1i (each batch of leftovers in one transaction) fails it ("2000 changes in 1 transaction"), and my T4a (no `between()` after the leftovers' transactions) fails it ("the live write went in at seq 2064, not between"). My T4b (the leftovers' transactions ended only at BATCH_ROWS) passes it and every other test: T5. My T4c (no early end for a waiting live write, the 3 ms and 1,000-row caps kept) passes it too, at no cost that breaks a budget: O14.
- **O11** (ab92c9be), `store::tests::jawd_status_counts_the_rows_left_in_the_inbox_waiting_for_the_writer_among_its_background_work`: `State` built with the store as serving's writer pool; an outside lock keeps the writer on the first of three `apply_leftovers` batches; `status`'s writer pool says `queued_background` 2, then 0 once all three are in. It reads `status` as interfaces do, and the queue is bounded at 2, so it reads 2 whether the writer has taken the first or not. P1f (the leftovers' queue left out of `queued_background`) fails it ("Elapsed"). 5 of 5 alone. But under load it fails with the rule in place, on its 5 s timeout: in 4 of 5 runs of store::tests beside sync::tests at the landing (none of 3 at the head), for a reason the audit's traced copy shows (O16).
- **O12** (8b34377f): P1's test (sync/tests.rs:2042) no longer times the live row; it waits as `until` does (5 s) and keeps its checks that history still waits for room as the live row converts and that the left row's thumbnail is kept. Under main's rule the live row never converts below the floor, so any wait catches it: the fix round's O12 switch (the converter waiting for room before it hands the leftovers over) fails it, and the audit's probes 1, 2 (below the floor) and 4; so do P1a to P1d. 5 of 5 alone.
- **The rebase's drain.rs.** The inbox loop at the head (sync/drain.rs:392-497) is main's at bab95fc2, its back-off and all, but for two lines: `apply_leftovers` for `apply(Lane::Background, …)` (457), and `thumbnailing(&mut converted, from_before)` (455) for main's lane, which on main fed only the thumbnails; `keep_thumbnails` applies a leftover batch's thumbnails through `apply_leftovers` (366).
  - **Nothing converted twice:** both back-off paths (448-453, a panic outside every row's catch; 478-483, rows that couldn't be stored) `continue` before `after = last`, before the offers and before the thumbnails are sent, so a batch is applied only once it has converted whole, and read again from the inbox otherwise; an inbox row is deleted in the transaction its conversion commits in, so a row already in isn't read again; and every write is an upsert.
  - **No thumbnail lost:** a batch's thumbnails go to `keep_thumbnails` only after its commit, as on main; one dropped with a failed attempt comes again with the rows read again. The one window where a preview shows without its thumbnail, a batch whose first transactions committed before a later one failed for good (a write failure `retrying` doesn't take for transient), is main's own (the same `background_changes`), and the thumbnail stays in the message's raw.
  - **No acknowledgement moves:** the converter acknowledges nothing; inbox rows were acknowledged after their synced inbox commit as they arrived, and the leftovers' commits are at NORMAL, as main's background lane's were.
  - **Probe 4** (audit-only): below the floor, with history waiting for room, a row left in the inbox with a link preview's thumbnail, beside a row that makes its batch's conversion panic outside every row's catch until it's mended after three tries; then a live row. The batch panicked 3 times (backed off 10, 20 and 40 ms), then converted below the floor with its thumbnail, and the end state (messages, events by type, thumbnails, links, quarantine, the inbox) equals a control's that never panicked: messages h1, left and live; events chat 4, message 4, person 2; one thumbnail, one link, no quarantine, the unknown row left in the inbox. Every leftovers' switch but O12's and P1a+ passes it.
  - **The fetcher:** it waits for the disk only before it asks the engine for a blob not in the spool (sync/fetch.rs:206); main's sweep doesn't wait, so below the floor it still deletes what it knows is done with.
  - **The stale comment** at sync/tests.rs:307 ("From before the converter started, so the background lane takes them") should change before landing: on this branch the leftovers' own queue takes them. The test is right either way (it covers rows from before and live rows, whatever takes them), so it's O15.
- **Round one's and two's rules still hold at the head:** the fix round's 37 switches and round two's P1h, P1j and P1a+, applied as they were defined, one at a time, in the audit's clone (Measurements): the 38 that should fail fail where they did before, the fix round's 37 at its table's assertions and P1a+ at P1's test and probes 1, 2 and 4, and P1h and P1j, which needn't fail, pass everything, as in round two.
- **The full suite at the head** fails only where the landers' list says it may (Measurements): jawd's unit tests failed 4 of 1,175 at a load of 25 to 77, each on the list and each passing alone 3 of 3, every watched test passed, and every other target passed.
- **Sync's stop bound** (`an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean`, sync/tests.rs:1532), which failed once in the fix round's first run: the watchdog can't be why. The test opens `Store::open`, whose disk is unwatched: `room()` reads nothing and answers yes, there is no watchdog task and no ballast, and `wait_for_disk` returns at once. The branch changes nothing in sync's stop (sync/mod.rs), the engine supervisor, the schema or the plans. Its three stops at 3.9, 3.2 and 2.9 s, against DISCONNECTING + 500 ms, came at a load of 50 to 70, beside seven other timing failures in the same run. It passed here.

## Findings

Nothing is lost and nothing is left inconsistent: no finding is data loss, inconsistency or unpredictable performance. One test gap is left in what round two asked for (T5), so this round asks for changes. O13 and O14 may stand as follow-ups, with round one's O1, O3 and O5 to O10; O15 and O16 are best done with T5.

### T5. Test gap: T4's test holds only the row cap, which never ends a leftovers' transaction in jawd

- **Where:** store/mod.rs:3572 (T4's test applies `2 * BATCH_ROWS` changes) and 3595 (`applied.transactions > 1`); the leftovers' pacing is `transaction(…, true, false)` in `background_changes` (1990), whose end is `Pace::ends` (1184-1186, called at 1927): at BATCH_ROWS (1,000) changes, at BATCH_TIME (3 ms) with the commit, or once the work has had YIELD_AFTER (1 ms) while an inbox row or a live write waits.
- **What happens:** with more than BATCH_ROWS changes, the row cap alone splits them, so the test can't tell the bound that matters from the one that doesn't. A converter batch is READ (256) inbox rows, whose changes stay far under 1,000 (round two measured each batch unpaced as one transaction of about 20 ms), so in jawd only BATCH_TIME and YIELD_AFTER end a leftovers' transaction. Switch T4b (the leftovers' transactions ended only at BATCH_ROWS, nothing else changed) passes every test, T4's among them (2 transactions for its 2,000 changes, the live write between), and the audit's probes; while 20,000 leftovers convert, live writes made every 5 ms go from p50 1.5 ms, p90 3.0 and 3.4 ms and p99 5.9 and 10.3 ms at the head (with room and below the floor, 356 and 353 of them going in) to p50 0.25 to 0.49 ms, p90 23 to 24 ms and p99 28 to 30 ms, only 197 and 185 of them going in: P1i's cost (p99 29 to 30 ms, 174 and 180), which is what T4 was raised for. With 500 or 250 changes, T4b and P1i each take one transaction and the live write goes after them all.
- **Fix:** T4's test with fewer changes than BATCH_ROWS, for example BATCH_ROWS / 2, so only BATCH_TIME and YIELD_AFTER can split them, with the 2 × BATCH_ROWS case kept beside it if the row cap is to stay tested. The audit's probe T4x is that test for 500 and 250 changes: at the head they took 7 or 8, and 4 or 6, transactions, the live write always after the first (at seq 3), in 20 runs of 20 at a load of 25 to 65; with P1i, T4a or T4b they fail it (one transaction, or the live write after them all). It should fail with P1i, T4a and T4b, and pass 20 of 20 at the head.

### O13. Other: T3 holds that the leftovers aren't starved, not that they go first

- **Where:** store/mod.rs:1223-1227 (the leftovers' arm ahead of the background arm); the plan's "taken after live writes, ahead of other background work".
- **What happens:** switch P1k (with room, the leftovers in the background lane, behind what's queued there; on their own queue below the floor) passes every test, T3's among them: the lane is a FIFO of at most 16 batches, so a stream of background work holds them up a batch or a few (probe 3: 12 ms) rather than until it ends, and the live rows behind them wait as long. That is main's behaviour with room, and it breaks no budget the leftovers themselves don't already set at a start.
- **Fix:** optional: a test in which background batches already wait in the lane, held by an outside lock as T4's test holds its transaction, as a batch of leftovers comes, and the leftovers go in first.

### O14. Other: nothing tests the leftovers' early end for a waiting live write

- **Where:** store/mod.rs:1185 (`waiting && body >= self.yield_after`), shared with the background lane.
- **What happens:** switch T4c (the leftovers' transactions not ending early for a waiting inbox row or live write, the 3 ms and 1,000-row caps kept) passes every test, T5's suggested test too: the live write goes in after the first transaction's 3 ms rather than its 1 ms (T4x: at seq 41 and 50, not 3), and probe 2's live writes run p99 6.4 to 7.2 ms against the head's 5.9 and 10.3 ms. A live write waits at most 2 ms more, within its 5 ms budget.
- **Fix:** optional; `Pace::ends`'s unit test holds the early end for the background lane, which the leftovers share.

### O15. Other: a comment in the drain fix's test says the background lane takes the leftovers

- **Where:** sync/tests.rs:307, in `an_inbox_row_whose_conversion_panics_is_kept_in_quarantine_once_and_the_rows_after_it_go_on`, which the drain fix added and the rebase kept.
- **What happens:** "From before the converter started, so the background lane takes them": on this branch their own queue takes them (`Store::apply_leftovers`). The test is right either way; the comment misleads its next reader about which path it covers.
- **Fix:** "so they go on the leftovers' own queue", with T5's change (sync/tests.rs is in the data layer's paths, so its commit takes the trailer).

### O16. Other: T3's and O11's tests wait in their setup on 5 s clocks that the suite's load outlasts

- **Where:** sync/tests.rs, T3's `until("background work coming", || batches.load(Ordering::Relaxed) >= 4)` (`until` polls 500 times, 10 ms apart: 5 s at least), with STREAM's 30 s counted from before it; store/mod.rs, O11's `tokio::time::timeout(Duration::from_secs(5), …)` around its loop of `state.status()` calls.
- **What happens:** neither is on the landers' list, and neither is wrong about its rule, but each failed at the landing with the rule in place. T3's, in the full suite (load 34 to 97): its four producers' first four batches of 400 history messages weren't in within 5 s, so it failed before the converter started. O11's, in 4 of 5 runs of store::tests beside sync::tests ("Elapsed": 2 of 3 as two processes at a load of 55 to 64, and 2 of 2 as one at 70 to 82), and in none of 3 at the head (two at 95 to 102, one at 53 to 64). So few runs can't tell the rebase from the moment's disk traffic, and the rebase adds little to the call: `status` gains only the voices backfill's count, one reader connection of about 40, and the test's code and the writer's are the same at both. The audit's traced copy shows why: the first `status()` reads the writer's queues before the three `apply_leftovers` tasks have sent (0, every run, alone too), and one `status()` call, some 40 reader connections for its counts, took 5.35 and 5.98 s under that load, against 54 to 59 ms alone, so the second read came after the timeout. The leftovers' queue held 2 throughout.
- **Fix:** T3's test waits for its stream to start as long as it takes, under a bound of its own (a minute, say, saying the stream never started), and counts STREAM's 30 s from the converter's start, so a slow start can't eat the stream. O11's waits, as `until` does or longer, on what `status` reads without the database (the store's own `report()`, or the leftovers' queue's length) until it says 2, then reads `state.status()` once: with the lock held the queue stays at 2, however slow the call. Each should still fail with its switch (P1e for T3, P1f for O11) and pass every run of its module beside the other.

## Measurements

All in one clone of the worktree under /tmp/jaw-audits/disk-watchdog-r3, built with `CARGO_INCREMENTAL=0`, test runs under `nice`, the machine at a load of 20 to 100 on 20 cores. The probes are one scratch commit on the head (round two's probes 1 to 3, applied cleanly, probe 1's two 2 s clocks loosened to 10 s since it judges the end state; this round's probe 4 and T4x) and two uncommitted on the landing (the voices backfill's and a traced copy of O11's test), never in the branch; they're kept beside the audit's report.

- **The change's leftovers tests at the head,** each alone 5 times (load 35 to 65): P1's, T3's, T4's and O11's, 5 of 5 each; T3 alone 3 more times took 2.66, 0.86 and 0.56 s.
- **Each rule's switch, applied one at a time** to the head with the probes (an exact replacement found once, a build, the test binary run directly, `git checkout`), 44 in all:

  | Switch | Fails at |
  | --- | --- |
  | 1 to 18, 12b, 11a, 11b, O4, O2a, O2b, w1 to w5 | each at the assertion of round two's table |
  | P1a, P1b, P1c | waited too long for both messages, the disk short of room (P1's test, without its clock) |
  | P1d | waited too long for the left row's thumbnail |
  | P1a+ (56b7c848's rule) | P1's test, probes 1, 2 (below the floor) and 4 |
  | P1e. the leftovers after other background work in the writer's loop | T3's test: "converted only once background work stopped coming, after 1228 batches of it"; probe 3, 7.56 s |
  | P1i. the leftovers' transactions unpaced | T4's test: "2000 changes in 1 transaction" |
  | P1f. their queue left out of `queued_background` | O11's test: "status counts the two batches … waiting for the writer: Elapsed" |
  | O12's. the converter waiting for room before it hands them over | P1's test, probes 1, 2 (below the floor) and 4 |
  | T4a (this round's). no `between()` after the leftovers' transactions | T4's test: "the live write went in at seq 2064, not between" |
  | P1h, P1j (round two's, needn't fail) | nothing, as in round two |
  | P1k (this round's). with room, the leftovers in the background lane | nothing (O13) |
  | T4b (this round's). the leftovers' transactions ended only at BATCH_ROWS | nothing (T5) |
  | T4c (this round's). no early end for a waiting live write | nothing (O14) |

  P1k, T4b and T4c also ran against every test in store::tests and sync::tests (153, the probes skipped, at a load of about 60): P1k and T4c passed all; T4b failed one, `a_reader_that_lets_go_while_a_restart_waits_for_it_doesnt_keep_the_wal_from_starting_again` ("a reader that let go 3.9 ms into the try kept the WAL from starting again"), which writes only through the live lane, where T4b's switch can't reach (`background && …`), and which passed alone 3 of 3 at the head: the load.
- **Probe 2** (20,000 leftovers converting, a live write every 5 ms, each timed from its call to its answer):

  | Rule | Case | Live writes | p50 | p90 | p99 | Most |
  | --- | --- | --- | --- | --- | --- | --- |
  | head | with room | 356 | 1.46 ms | 2.99 ms | 5.87 ms | 19.45 ms |
  | head | below the floor | 353 | 1.54 ms | 3.40 ms | 10.31 ms | 28.37 ms |
  | P1i, unpaced | with room | 174 | 0.68 ms | 27.34 ms | 29.43 ms | 30.02 ms |
  | P1i, unpaced | below the floor | 180 | 0.29 ms | 26.19 ms | 29.94 ms | 31.07 ms |
  | T4b, BATCH_ROWS only | with room | 197 | 0.49 ms | 22.65 ms | 27.67 ms | 29.77 ms |
  | T4b, BATCH_ROWS only | below the floor | 185 | 0.25 ms | 24.10 ms | 29.97 ms | 34.94 ms |
  | T4c, no early end | with room | 357 | 1.66 ms | 3.82 ms | 7.18 ms | 9.41 ms |
  | T4c, no early end | below the floor | 339 | 1.91 ms | 4.06 ms | 6.35 ms | 9.42 ms |
  | T4a, no `between()` | with room | 157 | 4.20 ms | 27.74 ms | 34.06 ms | 43.50 ms |
  | P1k, FIFO with room | with room | 369 | 1.52 ms | 2.49 ms | 4.17 ms | 5.72 ms |

  Under the other switches that leave the leftovers' pacing alone (P1e, P1f, P1h, O12's with room) the live writes' p99 was 4.1 to 9.3 ms, the head's noise; P1j's 7.2 to 8.6 ms, as in round two.
- **T4x** (T4's test for 2,000, 500 and 250 changes, saying how many transactions they took and where the live write went): at the head, 20 runs, 24 to 30, 7 or 8, and 4 or 6 transactions, the live write between in all 60 cases and always at seq 3, after the first transaction; under P1i 1, 1 and 1, never between; under T4b 2, 1 and 1, between only for 2,000; under T4a 63, 17 and 9, never between; under T4c 25, 7 and 4, between (at seq 3, 41 and 50).
- **Probe 1** at the head: equal to both controls line for line; the live rows converted 30 and 40 ms after arriving (19 to 40 ms under the switches that pass it, and 187 ms at P1e's first start). **Probe 3:** 74 ms, 37 batches of 400 meanwhile; P1e 7.56 s. **Probe 4:** as in "What was asked, checked".
- **The full suite at the head,** each target's binary of `cargo test --workspace` run on its own from its crate (copies of the head's binaries, so the switches could build meanwhile; jaw's and jaw-client's integration tests, which look for jawd beside them, from target/debug/deps):
  - jawd's unit tests (load 25 to 77, 2,654 s): 1,171 passed, 4 failed, 19 ignored. The four, each on the landers' list and each passing alone 3 of 3: the feed's `a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends` (0, 0, 1 s) and lane test `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` (4, 4, 3 s), media in's starts check `the_starts_check_reads_a_page_again_that_it_couldnt_and_shows_nothing_of_it_meanwhile` (2, 1, 1 s), and the socket's connection-full test (8, 8, 8 s). The watched ones all passed: sync's stop bound, the restart tests, the feed's starvation and coalescing tests, the WAL tests, the blob tests, and jawd's stopping test.
  - Everything else passed: jaw 354 (2 ignored), composing 3, contacts 1, failures 10, settings 3, typing 2; jaw-client 28 and its tests against a real jawd 3; jaw-media 28, pipeline 18 (1 ignored); jaw-proto 25 (1 ignored), fixtures 3; jaw-version 0; jaw-voice 36 (2 ignored), its recording-end and playhead tests among them; jawd's installed 1, starting 1, stopping 4, upgrading 1; wa-proto 0, roundtrip 3. jaw's and jaw-client's integration tests build jawd themselves before they start it (`cargo build -p jawd`), so they ran with the clone's source at the head. The Go engine's tests weren't run: the branch changes nothing under `engine/`.
- **The full suite at the landing** (Landing, below), run the same way from the rebased tree's binaries:
  - jawd's unit tests (load 34 to 97, 3,024 s): 1,205 passed, 6 failed, 20 ignored. Five are on the landers' list, each passing alone 3 of 3: the feed's half-closed test (4, 1, 3 s) and lane test (7, 4, 4 s), media in's starts check (3, 2, 3 s), the socket's connection-full test (8, 9, 8 s) and the checkpointer's flush test `the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it` (1, 1, 2 s). The sixth, T3's, isn't: it failed in its setup (O16) and passed alone 3 of 3 (0, 1, 1 s). Each of the six passed alone 3 of 3 again in the second auditor's runs, from the same binary, at a load of 66 to 70. The watched ones all passed: sync's stop bound, the restart tests, the feed's starvation and coalescing tests, the WAL tests, the blob tests and jawd's stopping test.
  - Everything else passed: jaw 354 (2 ignored), composing 3, contacts 1, failures 10, settings 3, typing 2; jaw-client 28 and its tests against a real jawd 3; jaw-media 28, pipeline 18 (1 ignored); jaw-proto 25 (1 ignored), fixtures 3; jaw-version 0; jaw-voice 36 (2 ignored); jawd's installed 1, starting 1, stopping 4, upgrading 1, and main's microphone test 1; wa-proto 0, roundtrip 3.
  - store::tests beside sync::tests, two processes at once: at the landing 3 runs (load 55 to 64), sync's 79 passing each time, store's failing O11's test in the first two ("Elapsed"); at the head 2 runs (load 95 to 102), all passing. The second auditor's, both modules in one process (154 tests): at the landing 2 runs (load 70 to 82, from the binary the full suite ran), O11's test failing in both ("Elapsed"), beside the checkpointer's flush tests, which are on the landers' list, and T3's passing; at the head 1 run (load 53 to 64), all 152 passing. The traced copy of O11's test (audit-only) read `status` once in each of the two runs it was in, the call taking 5.35 and 5.98 s; alone, in each of 3 runs, it read `status` twice, each call taking 54 to 59 ms, the second reading 2 at 116 to 124 ms.

## Landing on today's main

Checked in the audit's clone, on a branch of its own: `git rebase upstream/main` onto b4d9624d, with rerere off. It stops once, at 74ad432f (the change), in two files; the other ten commits apply on their own. Once T5 is in, the lander's rebase meets the same two conflicts.

- **protocol/README.md, `status`'s row.** Main adds `voices_backfill` and `voice`; the branch adds `disk` and drops "background" before "downloads" in media's part. Resolved as main's row with the branch's two changes made in it: the `disk` part where the branch put it (after "`last_drift` it found; ", before "and `media`: the cache's"), and "background downloads" made "downloads". Checked both ways: the resolved row less main's two additions is the branch's, less the branch's two changes is main's, and each less its own is bab95fc2's.
- **protocol/README.md, the notices.** The branch's `disk` notice paragraph ends the notices where main now starts its "## Voice" section (on main renamed from "Voice (proposed)"): the branch's paragraph, then main's heading and section.
- **scripts/check-data-audit.py, `DATA_PATHS`.** The branch adds `crates/jawd/src/disk.rs` and `crates/jawd/src/disk/`, main its voice paths beside them: both kept.

None is more than keeping both sides, and none is in code: the rebased tree equals `git merge-tree`'s merge of ab92c9be and b4d9624d in every file but these two, `git range-diff` shows only 74ad432f changed, and the branch's diff against its base is the same at the landing line for line but for the status row. main.rs, state.rs, media/mod.rs and jaw-proto merge on their own; main's `Voice::start` comes inside the branch's `start_watching`, after the watched store and `disk.start()` (main.rs:610, 611, 649). `make check-parity` passes at the landing (it fails at bab95fc2 and the head alike, on `send.contact`, to which main has since given the Mac's parity step).

- **The voices backfill** (main's migration 30) is background work that rightly waits for the disk: one more name in `BACKFILLS`, stepped by the unchanged backfill loop through `store.apply(Lane::Background, …)`, so it goes through `background_changes(…, false)` and `wait_for_disk` as every other backfill does, and below the floor it waits while live writes go on. It reads again only what jaw.db keeps (the voice messages' raw protobufs) and goes on from `backfills.after`, so waiting loses and repeats nothing. The audit's probe at the landing (a voices backfill over 200 messages, the backfill loop running, the disk below the floor): after a second, and after a live write that went in meanwhile, all 200 were still to look at; once there was room it was done 98 to 154 ms later, 3 runs of 3 (250 to 278 ms in the second auditor's 3).
- **No migration is needed:** the branch adds none and changes neither the schema, `migrations.lock` nor `plans.txt` (the watchdog keeps its state in memory and in its ballast file, nothing in jaw.db); main's migration 30 stays main's, as it was.
- **The trailer:** `check-data-audit.py upstream/main HEAD` at the landing names nine commits, as at the head: 270c297e (74ad432f), c96431c1 (e7f4fbd9), 33907533 (8c21ae54), 72972e15 (3e15849a), 5e18f08d (d615fa4b), 5d2d94d4 (a7456273), e70ad264 (29b3aa9c), 75d05369 (8b34377f) and 0016cfd8 (ab92c9be); not e1ddbdce (657fb643, the plan) or 2a171f6f (0b52b7f2, main.rs and the plan). The commits for T5, O15 and O16, in store/mod.rs and sync/tests.rs, take it too.
- **What the lander checks again** after T5 and the rebase: T5's test and its three switches (P1i, T4a, T4b), O16's two tests with theirs (P1e, P1f) and beside each other's module, the status row and the notices as resolved here, `make check-parity`, and the gate.

## Verdict

Changes needed before landing: T5 (T4's test with fewer changes than BATCH_ROWS, failing with P1i, T4a and T4b and passing 20 of 20 at the head), and with it O15's comment and O16's two waits, which the gate's suite can otherwise fail on. Everything else rounds one and two asked for is done and tested. O13 and O14 may stand as follow-ups, with round one's O1, O3 and O5 to O10.

Verdict: changes needed

# Round four: ad3402e5

Range: b4d9624d..ad3402e5 in disk-watchdog, 3 and 4 November 2026. Auditor: database-expert, a fresh one for this round, which stopped at 20:39 on 3 November when Cursor's stream stalled; a second, started at 12:55 on 4 November, took over its scratch and report, checked what it took from them, and finished the round. Fourteen commits: round two's eleven, rebased onto b4d9624d (main with jaw-voice's steadied tests), then one for each item round three left: 96040c28 T5, 04e2c0e7 O16 and ad3402e5 O15. Main has since moved to b827db02 (1708094e, the release polish, then the first release's README); onto it the branch rebases with no stop (see Landing).

## In short

Nothing is lost and nothing is left inconsistent. O15 and O16 are done as round three asked, and so is T5 as it was asked: T4's test now runs for BATCH_ROWS / 2 changes, which take about as many transactions as a converter's batch of 256 rows does, and it fails with P1i, T4a and T4b. The rebase onto b4d9624d kept both sides of its two conflicts and nothing more. store's and sync's tests passed at the head in four runs under load, but for three of the checkpointer's tests that the flakes list names, each passing alone; under the heaviest load, at the head and at the landing, main's test of live writes beside a stream of inbox rows failed on its 250 ms, and passed alone. Onto today's main the branch rebases with no conflict, and main's Time Machine attribute and Spotlight marker change nothing the watchdog does.

But T5's test holds one live write, waiting as the leftovers' first transaction starts, so it holds only that one write goes in after their first transaction. Three switches pass every test at a cost the writer's budgets don't allow. Taking inbox rows and live writes only after a batch of leftovers' first transaction (T4e) costs what P1i does. Taking only live writes between their transactions, not inbox rows (T4f), leaves the inbox rows the engine waits on behind each whole batch. Dropping BATCH_TIME for them (T4d) lets one run to 1,000 changes when nothing waits and doubles the live writes' p99. Main holds the first for its own background lane, but nothing holds it for the leftovers' path that P1 added. So this round asks for two more cases in T5's test (T6, T7), test code only; the audit's probes of both are steady at the head under load and fail with their switches. And whichever of this branch and disappearing lands second must keep disappearing's erasures from waiting for the disk (O17).

## What was asked, checked

- **T5** (96040c28), `store::tests::the_rows_left_in_the_inbox_go_in_short_transactions_with_a_live_write_going_in_between` (store/mod.rs:3598), with its helper `leftovers_with_a_live_write_waiting` (3562).
  - **What it does:** for BATCH_ROWS / 2 changes, then 2 × BATCH_ROWS, an outside BEGIN IMMEDIATE holds the leftovers' first transaction until a live write waits behind it; once the lock goes, they take more than one transaction, and the live write, which makes no event, goes in at a seq after the feed's last before them and before their last. It's judged on transactions and seqs, not on the clock.
  - **As a converter's batch makes them:** at the head its 500 changes took 7 or 8 transactions in nearly every run, and a converter's batch of 256 one-to-one text rows, 768 changes, took 6 to 9 in 92% of some 470 batches in the audit's probes (5 to 18 in all). Both are below BATCH_ROWS, so only BATCH_TIME and the early end for a waiting write can split them, as in jawd.
  - **Switched:** P1i fails it ("500 changes in 1 transaction"), T4a fails it ("500 changes: the live write went in at seq 517, not between the leftovers' first and last, after 0 and before 517"), and T4b fails it ("500 changes in 1 transaction"); each passes everything else. T4c, O14's switch, passes it, as round three said it would. This round's T4d, T4e and T4f pass it: T6 and T7.
  - **Steady:** it passed in each of the four runs of store's and sync's tests at the head (at a load of 16 to 82) and in the landing's run. The first auditor ran it alone 20 times of 20 (load 22 to 24, 0.80 to 0.93 s), and the audit's probe of its helper put the first live write between in all 40 of its cases (load 59 to 76).
- **O16** (04e2c0e7).
  - **T3's test** (sync/tests.rs:2088) waits for its stream to start for a minute at most ("the stream of background work never started"), and counts STREAM's 30 s, and its last wait's 30 s and 10 more, from a `OnceLock` set as the converter starts, so a slow start can't eat the stream. Its one other clock is `until`'s, for the 256 rows to reach the inbox, round three's and unchanged, which never failed. It still judges the leftovers on the stream still coming (`ran_out`) and the live row as its chat's newest. P1e fails it ("the rows left from before and the live row after them were converted only once background work stopped coming, after 1673 batches of it").
  - **O11's test** (store/mod.rs:3615) waits 30 s at most on the writer's own `report()`, which reads no database, until it says 2. It then reads `state.status()` once, from the pools as jawd serving gives them, and asserts that the writer pool says 2, and 0 once all three batches are in; the lock keeps them waiting while status reads. So it still reads the count where status reads it. P1f fails it at that wait ("the writer counts the two batches of rows left in the inbox waiting for it: Elapsed(())"), since status and the report share the one count.
  - **Under load:** both passed in all four runs of store's and sync's tests at the head, two of them as two processes at once (load 19 to 40) and two beside switch builds (load 16 to 82), and in the landing's run.
- **O15** (ad3402e5): sync/tests.rs:308 now says the three rows "go on the leftovers' own queue". They're in the inbox before `converting()` starts, so the converter's first read finds them at or below the start's `max(seq)` (drain.rs:405, 438) and hands them to `apply_leftovers` (457): the comment says what the code does.
- **The rebase's two keep-both files** (protocol/README.md and scripts/check-data-audit.py, onto b4d9624d): word by word, each carries the branch's changes as they were on bab95fc2 (3 runs of words in the README, 2 in the script) and main's as main made them (126 and 1). Of the 18 files the branch changes, the other 17 carry exactly its changed lines; the README differs as lines only in status's row, which holds both sides. Nothing of either side is lost, and nothing else changed.
- **The suite at the head** (ad3402e5's test binary, 1,231 tests): store::tests and sync::tests in one process, four times.
  - Two at once, at a load of 19 to 40: 152 passed each.
  - One beside switch builds, at a load of 16 to 61: 151 passed, and the checkpointer's idle test failed ("the checkpointer woke 2 times with nothing to do").
  - One at 74 to 82: 150 passed, and the checkpointer's two flush tests failed ("the checkpointer never flushed jaw.db while the writer appended"; "the checkpointer never flushed jaw.db").
  - All three are on the flakes list, and each passed alone 3 of 3. Of the watched tests, sync's stop bound, the restart tests, the WAL tests and the blob tests passed in all four.
  - A fifth run, the head with the audit's probes as a control at a load of 78 to 102, failed main's `store::tests::live_writes_go_in_with_a_stream_of_inbox_rows_and_are_never_held_back_by_it` ("a live write waited 497.077583ms for the stream of rows", against its 250 ms), which isn't on the list; it failed so at the landing too, and passed alone 3 of 3 at a load of 94 to 96. The branch changes nothing it runs: its store is `Store::open`'s, whose disk is unwatched, and it writes only inbox rows and live writes, half of them durable, so under that load the drive's full flushes decide it.
  - The whole suite at the head is the fix round's gate (`make test`: four listed failures, each passing alone 3 of 3); the whole jawd suite at the landing is in Measurements.

## Findings

Nothing is lost and nothing is left inconsistent: in what the branch does, no finding is data loss, inconsistency or unpredictable performance. Two test gaps are left in what T5 was raised to hold (T6, T7), so this round asks for changes. O17 is for whichever of this branch and disappearing lands second. O13 and O14 stand as follow-ups, with round one's O1, O3 and O5 to O10.

### T6. Test gap: nothing holds inbox rows and live writes going in after each of a batch of leftovers' transactions

- **Where:** store/mod.rs:1999, `self.between()` after each transaction in `background_changes` (1978), which takes the inbox rows and live writes waiting (1964-1974); the loop is shared with the background lane. T5's test (3562-3611) has one live write, waiting from before the leftovers' first transaction.
- **What happens:** T5's test holds that a write waiting as the leftovers start goes in after their first transaction. Two switches of this round keep that and break the rest, and each passes every test in store's and sync's modules and every earlier probe (Measurements):
  - **T4e,** inbox rows and live writes taken only after the first of a batch of leftovers' transactions (`if !leftovers || applied.transactions == 1`), costs what P1i does. In paired runs of round two's probe 2 (20,000 leftovers, a live write every 5 ms), the live writes' p99 was 30.3 to 40.5 ms against the head's 4.4 to 7.3 ms, with 167 to 176 of them going in against 354 to 372. A converter's real batch of 256 rows held a live write 55 to 68 ms in five runs, against the head's 2.3 to 5.1.
  - **T4f,** only live writes taken between them, not inbox rows, leaves the inbox rows the engine waits on behind each whole batch. In the audit's probe of main's own test's shape through `apply_leftovers` (inbox rows one at a time while 1,000 leftover changes go in), T4f's slowest ack was 117 to 156 ms, the whole batch, with 14 to 23 rows acked in the run, against the head's 14.6 to 46.0 ms and 87 to 105 rows at the same load. The design ends a background transaction early for a waiting inbox row because the engine waits on it.
  - **Main holds this for its own background lane:** T4e for every background batch (T4e-shared) fails main's `small_chats_of_history_go_many_to_a_transaction_while_inbox_rows_come_one_at_a_time` ("an ack of 22 waited 5.977293584s"), which sends inbox rows one at a time through a background batch. Nothing does for the leftovers' path, which P1 added and round two's T4 was raised for.
- **Fix:** T5's test holds writes of both kinds that come after the leftovers' first transaction:
  - **A second live write:** the helper sends one as soon as the first is answered, and it must go in before the leftovers' last too. The audit's probe of it, at the head 20 runs of 20 at a load of 59 to 76: the second live write went in at seq 11 to 189 of 508 or 509 for 500 changes, and at 14 to 148 of 2,025 to 2,035 for 2,000, always before the leftovers' last. With T4e it went in at their very last for both counts; with P1i, T4a and T4b it went in after them.
  - **Inbox rows:** a case sends them one at a time, as `one_at_a_time` does for main's test, while a batch of leftovers goes in, and more than a few of them must be acked before the batch is answered. Count them rather than time them: under load the head's slowest ack reached 46 ms. In the audit's probe at a load of 78 to 84, the head acked 87 to 105 rows in the run, against 14 to 23 with T4f and 21 to 24 with T4e; those counts include the rows sent in the 100 ms before the batch starts, so the test should count from its start.

### T7. Test gap: nothing holds BATCH_TIME for the leftovers

- **Where:** store/mod.rs:1184-1186, `Pace::ends`, which ends a background transaction at BATCH_ROWS, at BATCH_TIME with its commit, or once its work has had YIELD_AFTER while an inbox row or a live write waits. It ends the leftovers' transactions too (1927, through `background_changes(…, true)`).
- **What happens:** in T5's test a live write always waits, so the early end alone splits the leftovers, and switch T4d (the leftovers' transactions with no BATCH_TIME, ended at BATCH_ROWS or by the early end) passes every test in store's and sync's modules and every earlier probe. With nothing waiting, a leftovers' transaction then runs to BATCH_ROWS. In the audit's probe, 250 and 500 changes with nothing waiting went in one transaction each, against 4 and 6 to 12 at the head, so a background transaction runs many times its 3 ms. A live write that comes as one runs waits for its commit: in paired runs of probe 2 the live writes' p99 was 10.7 to 13.5 ms, against the head's 4.4 to 7.3 (p90 7.8 to 8.8 against 2.5 to 2.7), and a converter's real batch held one 5.7 to 10.2 ms in five runs, against 2.3 to 5.1. Pace's unit test holds BATCH_TIME where the leftovers share it, as round three said of O14's early end; but unlike O14's switch, this one breaks budgets: a background transaction's 3 ms, and a live message's 5 ms p99 where the head keeps it.
- **Fix:** a case in T5's test with BATCH_ROWS / 2 leftover changes and nothing waiting, which only BATCH_TIME can split, taking more than one transaction. The audit's probe of it, at the head 20 runs of 20: 6 to 12 transactions for 500 changes, and 4 for 250; with T4d, P1i or T4b, one.

### O17. Other, for whichever of this branch and disappearing lands second: disappearing's erasures would wait for the disk

- **Where:** on the disappearing branch (41ae564f), expiry.rs:113, where `erase_due` applies `Change::Expire` through `Lane::Background`; on this one, `background_changes(…, false)` calls `wait_for_disk` first (store/mod.rs:1984-1986).
- **What happens:** once both are on main, below the floor a disappearing message's erasure waits with other background work until the disk has the floor and 256 MB more, so the message, its raw and its words stay in jaw.db past its time, against the design's "Erased when asked". The erasure frees room, too (its raw cleared, its FTS segments merged), so making it wait for room works against the watchdog itself. Neither branch's conflicts show it: expiry.rs is disappearing's own file, and the three hunks the branches meet in, in store/mod.rs, are additions side by side.
- **Fix:** whichever lands second has the erasures not wait for the disk, as the leftovers don't, with a test that below the floor a disappearing message goes at its time, in its own audited change.

## Measurements

All in one clone of the worktree under /tmp/jaw-audits/disk-watchdog-r4 (the first auditor's, checked and reused), built with `CARGO_INCREMENTAL=0`, the test binaries run directly under `nice`, the machine at a load of 16 to 84 on 20 cores. The probes are scratch commits on the head, never in the branch: rounds two's and three's, applied cleanly; the first auditor's (an env-gated report of each leftovers batch's changes and transactions in `apply_leftovers`, and a converter's real batch of 256 rows with a live write every 2 ms); and mine (probe_r4b: T5's helper with a second live write, and the leftovers with nothing waiting; probe_r4c: inbox rows one at a time while 1,000 leftover changes go in). They're kept beside the audit's report.

- **Each switch, applied one at a time** to the head with the probes (an exact replacement found once, a build, the test binary run directly, `git checkout`), against P1's, T3's, T4's (T5) and O11's tests and the probes, the second auditor's runs:

  | Switch | The branch's tests | Probe 2's live p99, with room / below the floor | Second live write | 500 alone |
  | --- | --- | --- | --- | --- |
  | head | pass | 6.20 / 10.16 ms (load 49) | between | 6 to 12 |
  | P1i. unpaced | T5's: "500 changes in 1 transaction" | 28.57 / 76.12 ms | not between | 1 |
  | T4a. no `between()` | T5's: "500 changes: the live write went in at seq 517, not between …" | 30.73 / 33.19 ms | not between | 7 |
  | T4b. BATCH_ROWS only | T5's: "500 changes in 1 transaction" | 30.54 / 27.38 ms | not between | 1 |
  | T4c. no early end (O14) | pass | 7.35 / 15.54 ms | between | 7 |
  | T4d. no BATCH_TIME (T7) | pass | 12.34 / 13.56 ms | between | 1 |
  | T4e. `between()` after the first only (T6) | pass | 31.86 / 32.02 ms | not between | 7 |
  | T4f. live writes only between (T6) | pass | 6.80 / 7.95 ms | between | 6 |
  | P1e. after other background work | T3's: "… after 1673 batches of it" | 5.55 / 9.86 ms | | |
  | P1f. out of `queued_background` | O11's: "… waiting for it: Elapsed(())" | 10.38 / 4.45 ms | | |

  The first auditor's run of the same switches on the same tree (its logs kept) agreed in every exit and message, numbers aside. T4e for every background batch (T4e-shared), against all of store's and sync's tests and the probes (158 run): 155 passed, and 3 failed, main's `small_chats_of_history_…` ("an ack of 22 waited 5.977293584s") and two of round two's probes.
- **T4d, T4e and T4f against all of store's and sync's tests** with the probes, at a load of 80 to 101, and the head with the probes as the control, at 78 to 102. Every test of the branch's and main's passed under each switch, but for ones that failed at such a load with no switch too: the checkpointer's two flush tests under T4f (as in the fourth run at the head), and `live_writes_go_in_with_a_stream_of_inbox_rows_and_are_never_held_back_by_it` under T4d ("a live write waited 308.755791ms for the stream of rows"; in the control 497 ms, against its 250 ms). Of the probes, round two's probe 2 failed under all three and in the control ("with room: every leftover converted", its own bound at that load), and probe 3 failed once under T4e ("the live row behind a leftover waited 2.559609125s"), then passed alone 3 of 3 from T4e's binary (3 to 23 ms): it has one leftover row, which T4e can't touch. The control's other failures, the checkpointer's flush test and sync's `what_a_stopped_engine_sent_comes_before_jawd_says_it_is_reconnecting_and_the_next_engines_after` (an engine-disconnect test), are on the flakes list; the engine test passed alone 3 of 3.
- **Paired cost runs** (probe 2 and the converter's real batch, from the head's, T4d's and T4e's binaries in turn, 3 rounds, load 20 to 40): probe 2's live writes p99 at the head 4.35 to 7.30 ms (p90 2.5 to 2.7, 354 to 372 going in), T4d 10.73 to 13.52 (p90 7.8 to 8.8, 280 to 291), T4e 30.25 to 40.48 (p90 25.5 to 28.2, 167 to 176, the most 113 ms); the real batch's slowest live write at the head 2.32 to 4.53 ms, T4d 5.65 to 6.27, T4e 55.32 to 59.07.
- **The inbox rows probe** (1,000 leftover changes, two to a chat, through `apply_leftovers` with the store's limits; main's `one_at_a_time` sending inbox rows from 100 ms before): at the head 14 runs, 62 to 124 transactions, 84 to 145 rows acked in the run and the slowest ack 8.9 to 16.1 ms at a load of 20 to 39, and 87 to 105 rows and 14.6 to 46.0 ms at 78 to 81; with T4f 14 to 23 rows and 117 to 156 ms, with T4e 21 to 24 and 103 to 140 ms (load 17 to 84); with T4d 89 rows and 10.2 ms. With main's test's long limits (200 and 100 ms) the batch went in one transaction of 72 to 81 ms, so those limits can't tell.
- **The second live write probe** at the head, 20 runs of 20 at a load of 59 to 76: as in T6. The leftovers with nothing waiting: 250 changes in 4 transactions in all 20, 500 in 6 to 12.
- **The suite at the head:** as in "What was asked, checked".
- **The landing** (b827db02 and the 14 commits rebased, built with `cargo test -p jawd --no-run`):
  - jawd's integration tests, at a load of 68 to 80: starting 3, main's two tests of the folder among them, stopping 4, installed 1, microphone 1 and upgrading 1, all passing.
  - jawd's unit tests, the whole suite of 1,231, in one process at a load of 54 to 104: the audit's 30-minute bound ended the run after 1,170, with 1,143 passed, 20 ignored and 7 failed, so libtest printed no messages. Six of the failures are on the flakes list: the feed's half-closed and lane tests, media's save-waiting test and media in's starts check, the socket's connection-full test and the checkpointer's flush test. The seventh is main's live-writes test above. Each passed alone 3 of 3 at the landing (load 37 to 96). The watched ones among them passed: sync's stop bound, the feed's starvation and coalescing tests, the three WAL tests and jawd's stopping test. The 61 it hadn't reached, run on their own, all passed (1,638 s, load 23 to 71), the migrations' fault test and the two merge tests among them.
  - `check-parity.py` passed.

## Landing on today's main

Today's main is b827db02 (origin/main since 12:56 on 4 November): 1708094e, the release polish the brief names (the terminal's view files, mac/, and jawd's home.rs, main.rs and tests/starting.rs), then the first release's README and its pictures (README.md and docs/pictures/ only). The main checkout's own main, 660701c5, is a docs commit that isn't on origin/main. Checked in the audit's clone, on a branch of its own, with rerere off:

- **What conflicts:** nothing. `git rebase` onto b827db02 applies all 14 commits without a stop; the rebased tree is `git merge-tree`'s merge of ad3402e5 and b827db02, and `git range-diff` shows every commit "=". The one file both sides change is crates/jawd/src/main.rs, which merges on its own: main's `mod home`, and `home::keep_out(home)` in `run()` before the runtime starts, beside the branch's `start` and `start_watching` as they were. The branch's changed lines against b827db02 are its lines against b4d9624d, in the same 18 files. Nothing needs more than git's own merge.
- **Time Machine's attribute and Spotlight's marker:** neither the ballast nor the reading of free space minds them.
  - The watchdog reads free space with statvfs on jaw's folder (disk.rs:304-312), which counts the whole volume's blocks, so an attribute on the folder and an empty file in it change nothing it reads.
  - The ballast is a file named `ballast` in the folder, made as `ballast.partial` and renamed, and given up by name (disk.rs:196-214, 239-266); nothing in the watchdog lists the folder, reads its attributes or touches the marker.
  - `keep_out` runs in `run()` before the store opens and before the watchdog's first look makes the ballast. It sets each only when it isn't already there, and when it can't, it says so and carries on, so a full disk can't keep jawd from starting, which is when the ballast is wanted. With the folder left out of Time Machine, the ballast is left out of backups too.
  - One thing the attribute doesn't do: Time Machine's local snapshots are of the whole volume, so a ballast given up while one still holds its blocks frees them only once macOS thins that snapshot, as macOS does as the disk fills. That was so before main's change, and main's change doesn't alter it.
  - At the landing, main's two tests of the folder (tests/starting.rs, `kept_out`) pass with the watchdog in jawd, as do jawd's other integration tests.
- **The trailer:** `check-data-audit.py` from b827db02 to the rebased head names 12 commits: those of the change (bd82ddda), P1 (f4d64d40), T2 (e76ecb43), O4 (61a174ac), O2 (f32a1835), T3 (0aca812d), T4 (f75f5872), O12 (de27dd98), O11 (c5aa6c95), T5 (96040c28), O16 (04e2c0e7) and O15 (ad3402e5). It doesn't name the plan's (3d439d06) or T1's (4adbb7eb: main.rs and the plan). The commits for T6 and T7, in store/mod.rs, will take it too.
- **No migration is needed:** the branch adds none and changes neither schema.rs, migrations.lock nor plans.txt; the watchdog keeps its state in memory and in the ballast's file.
- **If disappearing lands first** (41ae564f, on 1708094e): the rebase meets it in store/mod.rs, three hunks of additions side by side (the expiry import beside the disk one; `Soonest`, `erased` and `closed` beside the `disk` field; their accessors beside `disk()`), and in protocol/README.md, protocol/fixtures/status.result.json and scripts/check-data-audit.py, each keeping both sides; and, with no conflict at all, in expiry.rs: O17.
- **If engine-stop lands first** (39461d70): main.rs conflicts in three places, and resolving them is more than keeping both sides. `start_watching` takes engine-stop's signature (its `supervised` slot where the branch passes the lock); it opens the store watched (`Store::open_watched(home, disk)`, where engine-stop has `Store::open(home)`) and starts the watchdog; and it makes the pushes channel once, before the store, since `disk::tell` needs it, where engine-stop makes it after the supervisor. In the tests' region, engine-stop makes `a_jawd_refused_while_another_stops_leaves_no_socket_behind` a plain test beside the branch's T1 tests. The lander then runs T1's tests and their switches w1 to w5, and engine-stop's stop tests, again.
- **What the lander checks again** after T6 and T7: T5's test with its switches (P1i, T4a and T4b, and now T4d, T4e and T4f), the rebase onto main as it is then, and the gate.

## Verdict

Changes needed before landing: T6 (T5's test holding a second live write and inbox rows that come after the leftovers' first transaction, failing with T4e and T4f) and T7 (T5's test with BATCH_ROWS / 2 leftover changes and nothing waiting, failing with T4d). Both are test code in T5's test, and each case should still fail with P1i, T4a and T4b. Everything else rounds one to three asked for is done and tested. O17 goes to whichever of this branch and disappearing lands second. O13 and O14 may stand as follow-ups, with round one's O1, O3 and O5 to O10.

Verdict: changes needed

# Round five: 8d3473ea

Range: 645a0f7b..8d3473ea in disk-watchdog, 5 November 2026. Auditor: database-expert, a fresh one for this round. Seventeen commits on 645a0f7b, which is origin/main (the engine-stop fix, 24847ccf..800f2598, then disappearing, 800f2598..645a0f7b): round four's fourteen, rebased onto the engine-stop fix and then onto disappearing; T6's 0417772f and T7's 394e08d0, test code only; and O17's 8d3473ea.

## In short

Nothing is lost and nothing is left inconsistent.

- **T6 and T7** are done as round four asked. At the head each of round four's six switches fails at least one of their three cases, and the cases are steady under load.
- **Both moves keep both sides.** The engine-stop fix's four hunks in main.rs are resolved as its note asks. Disappearing meets the branch only where round four said: additions side by side.
- **O17** is what disappearing's round five spells out, and its test fails with each of its six switches. Judged as new code in the writer, nothing it lets through below the floor can fill the disk, erasures keep their own rules, and other background work still waits.
- **But O17 split the idle step's one look at the disk into terms, and two of them aren't held by any test** (T8). Switches that break indexing's hold (M2, M2b) or the wait for the disk's cap at disk::POLL (M3) each pass all 207 tests with a faked disk.
  - With M2 or M2b, below the floor, the whole backlog of text waiting to be indexed is indexed whenever an erasure's WAL emptying comes due, which is O17's own situation.
  - The fix is test code only. The audit's probe of it passes at the head and fails with each of the three; it's kept beside this round's report.
- **The expiry flake is neither this branch's nor a lost count** (O21). Disappearing's test applies its 70 messages only 200 ms before their time. A writer slower than that writes the late ones as tombstones only, so the copy it erases from holds fewer. Applied 205 ms late, the test fails with 62 erased of 70, as at disappearing's landing.
- **The WAL's emptying on a full disk**, giving up no ballast, may land as the plan says (O20).
- **The landing:** the branch is on main's head, 16 commits take the trailer, no migration is needed, and plans.txt is what the head writes.

## What was asked, checked

- **T6 and T7** (0417772f, 394e08d0), each test code with the plan's lines.
  - **The cases:** T5's helper sends a second live write once the first is answered, which must go in before the leftovers' last transaction. One case acks inbox rows one at a time while BATCH_ROWS leftover changes go in, at least ten. Another applies BATCH_ROWS / 2 with nothing waiting, which must take more than one transaction.
  - **Switched at the head:**

    | Switch | T5's test (second live write) | Inbox rows between | Nothing waiting (T7) |
    | --- | --- | --- | --- |
    | T4d, no BATCH_TIME | pass | pass | "500 changes with nothing waiting in 1 transaction" |
    | T4e, between() after the first only | "…sent once the first was answered went in at seq 517, not before the leftovers' last at 517" | "only 3 … (transactions: 56)" | pass |
    | T4f, live writes only between | pass | "only 2 … (transactions: 58)" | pass |
    | P1i, unpaced | "500 changes in 1 transaction" | "only 3 … (transactions: 1)" | "…in 1 transaction" |
    | T4a, no between() | "…went in at seq 518, not between the leftovers' first and last" | "only 2 … (transactions: 54)" | pass |
    | T4b, BATCH_ROWS only | "500 changes in 1 transaction" | "only 3 … (transactions: 1)" | "…in 1 transaction" |

    That is round four's table. T4d's and T4b's last replacement, written before disappearing, no longer matched: disappearing's stop after a secure delete now leads the condition (store/mod.rs:2202). So they're restated on the head's line with that stop kept.
  - **Steady:** 20 of 20 each alone at a load of 49 to 53 (0.37 to 1.41 s), and in each of the three suite runs.
- **The move onto the engine-stop fix** (main.rs). Against the branch as it was on b827db02, its main.rs lines differ only by the lock parameter becoming engine-stop's `supervised` (and the tests' `_lock`), and every engine-stop line is kept. The four things its note asks hold:
  - `start` calls `start_watching(home, engine, options, supervised, Disk::watch(…))`, which takes no lock (main.rs:738-749).
  - There is one `pushes` channel, made before the watched store opens, since `disk::tell` needs it (:750); the second one is gone.
  - `supervised` is set straight after `Supervisor::spawn` (:765-767).
  - `run` takes jaw.lock (:532) before the runtime, so before `Store::open_watched` (:752), and lets it go (:570) only after `shutdown_timeout` (:569), which has dropped the watchdog's look loop and `disk::tell`, both tokio tasks.
  - **What O11 leaves:** a ballast still being made on the blocking pool when jawd stops is waited for 100 ms at most, and may finish in the moment between `drop(lock)` and the exit. It writes only `ballast.partial` and its rename to `ballast`, never jaw.db. A jawd started in that moment would at worst find a partial ballast, or one of another size, and its own next look, every 10 s, removes and remakes it (disk.rs:237-266). That stands as engine-stop's follow-up.
- **The move onto disappearing.** `git range-diff 800f2598..4751fff4 645a0f7b..394e08d0`: 15 "=", and the change's own commit "!".
  - Per file, the branch's added and removed lines (as multisets) are the same on 800f2598 as on 645a0f7b, and so are disappearing's lines onto the branch. That holds in all 18 of the branch's files and 95 of disappearing's, store/mod.rs's three blocks and check-data-audit.py's included, but for two one-line files both sides changed.
  - In those two, the README's `status` row is the base with both sides' word edits, which don't overlap, and the status fixture is the base with both sides' keys (`disappearing`, then `disk`).
  - So nothing of either side is lost, and the two meet only where round four said. What git merged without a conflict and mattered is O17's, below.
- **O17's change** (8d3473ea), against disappearing's round five, part by part:
  - **The timer's erasures** go through `apply_leftovers` (expiry.rs:115). In `run` they're taken ahead of `background`, and in `wait_for_disk` while it waits.
    - They run the same `transaction()` as before: secure_delete ON and FTS5's secure-delete through `erasing`, the transaction ending after the group that made a secure delete (`costliest`), NORMAL with no ack, and a full disk going through `retrying` and `failing`, which gives up the ballast. Only the disk's wait is gone.
    - The queue's three producers (the converter, `keep_thumbnails` and the timer) each await their batch, so `bounded(3)` never blocks one. `apply_leftovers` sends from `spawn_blocking`, so a full queue would only wait.
    - **Taking them ahead of background batches queued before them changes nothing a merge rule doesn't already cover.** No change in a queued batch can make a due message survive: keep in chat is only a system line, and every write brings a time only sooner (`OPEN_UNTIL`, `EXPIRES` only where it's NULL, the fold's `iif`). An amendment that comes after the erasure meets its tombstone and is dropped (`amend_message`, write.rs:2903-2905).
  - **The idle step** (store/mod.rs:1436-1453) works out a whole merge due or under way, and the WAL's emptying due, before it reads the disk.
    - `held` holds only `merge()` for `unmerged` and `index()` for `backlog`, and only while no whole merge is due or under way. With room, the order is disappearing's, but a whole merge under way now goes on even when `unmerged` is false.
    - `wait_for_disk` takes idle steps (:1909, :1923). Without them S2b shows no follow-up goes while any background batch waits for the disk.
  - **`idle_for`'s first arm** waits for `index_due()` too (:1402).
  - **The design:** the plan's bullet "Erasures never wait for the disk", "Bounded files" and disappearing's timer say what the code does, but for O18's and O20's wording.
  - **Its test** (store/write/expiry/tests.rs:2420) fails with each of O17's switches at the head:

    | Switch | O17's test |
    | --- | --- |
    | S1, erasures back in Lane::Background | "link is erased at its time while the disk is short of room" |
    | S2, the disk read first | "the whole merges the erasures leave are done while the disk is short of room" |
    | S2b, no idle steps in wait_for_disk | the same |
    | S2c, the WAL's emptying held | "and the WAL they leave is emptied" |
    | S2d, a whole merge under way held | the whole merges |
    | S3, idle_for ignoring index_after | "while jaw.db is full, the writer took 1354 steps where the watchdog looks 12 times" |

  - **What it lets through below the floor can't fill the disk.**
    - An erasure frees more than it writes: its row and raw protobuf deleted, its pages zeroed in place, a tombstone and an event row added. It waits for the WAL's room as before.
    - A whole merge is bounded by the index it rewrites. It goes in steps of 64 pages, each behind the WAL's room (`room()` comes first in `index_step`), and FTS5 trims what it has merged.
    - The WAL's emptying truncates the WAL.
    - Everything else below the floor still waits: the history message in O17's test, and the backfills (`once` among them) in Lane::Background, which no caller moved to the leftovers' queue.
  - **What O17 doesn't add:** a commit. Its erasures, merges and emptying are the transactions they were, so a kill or a power cut at any of them leaves what disappearing's tests already hold: the erasures before it, the rest still due, and the notes that leave a whole merge and the WAL's emptying to do.
- **The two things the O17 builder flagged.**
  - **`wait_for_disk`'s idle steps** are right and needed: S2b fails O17's test.
    - Each wakes at least every disk::POLL to read the disk, as before.
    - While the WAL lacks room, a step returns at `room()` and the wait comes round in 1 ms to disk::POLL. That's never faster than main's own wait for the WAL's room beside it (HOLD_POLL, 1 ms, :486).
    - What isn't tested is the cap at disk::POLL (M3, T8).
  - **The WAL's emptying, or a whole merge, refused by a full disk gives up no ballast.** It's tried again from 10 ms, doubling to 60 s. That may land, written down as O20, for three reasons:
    - The erased words it leaves are in pages the checkpointer copies too. On a disk that refuses jaw.db's writes, the checkpointer's pass, or its emptying of the WAL once jaw.db has gone 10 s without a commit, fails and gives up the ballast itself (T2's test, and round one's 11b). Its next pass then copies them, and the writer's next try has room.
    - Where nothing else gives it up, the writer's TRUNCATE has already emptied the WAL, and only its note's commit is refused. What's left is a note in sync_state, not words.
    - The ballast is for commits that precede acks, and the inbox gives it up for its own.
- **The suite at the head**, from the head's test binary under nice: store::tests, sync::tests, both expiry modules, convert, store::schema (the migrations' and kept tests) and store::plans, 296 tests, then main.rs's 14. Three runs, the second and third at once, beside switch builds; see Measurements. Every watched test among them passed in each run.
- **Disappearing's expiry test under load**, `expiry::tests::what_came_due_while_the_timer_wasnt_running_goes_once_it_starts_none_of_it_synced` (expiry.rs:362). It's the test's timing, not the count (O21).
  - **What the test does:** a first store gets 70 messages due 200 to 269 ms after the test reads the clock, then waits 400 ms. Its jaw.db is copied with VACUUM INTO, and a second store's timer erases what the copy holds.
  - **Where the count is lost:** it isn't. A message whose time has passed when the first store's writer converts it is never written; only its tombstone is (write.rs:2637), and no store counts it. So when that writer takes the 70 more than 200 ms after the test read the clock, the copy holds fewer, the timer erases each one it holds, and `erased` counts exactly those.
  - **The probe:** the test printing what the copy holds, and with a delay between reading the clock and the apply:
    - no delay: 70 held, passes;
    - 205 ms: 62 held and 8 tombstones, fails with `DisappearingStatus { expiring: 0, next: None, erased: 62, … }`, disappearing's landing failure exactly;
    - 220 ms: 43 held, 43 erased;
    - 260 ms: none held.
  - **Under load without the delay:** it passed 90 of 90 at 30 at once and 400 of 400 at 100 at once. So it takes a whole `make test`'s load to slow a just-opened store's writer that much, as both failures were in one.
  - **Not this branch's:** the first store is `Store::open`'s, whose disk is unwatched and never short, and it converts through the live lane, which O17 doesn't touch. The second store's erasures come on the leftovers' queue, but through the same `transaction()` and `committed()`, so O17 can neither delay nor lose the count. It doesn't bear on landing. The fix is disappearing's follow-up (O21).
- **The landing onto origin/main** is below.

## Findings

Nothing is lost and nothing is left inconsistent. One test gap is left in what O17 rewrote (T8), test code only, so this round asks for changes. O18 to O21 may stand as follow-ups, with O13 and O14 and round one's O1, O3 and O5 to O10.

### T8. Test gap: indexing's hold below the floor, and the wait for the disk's cap, as O17 rewrote them

- **Where:** store/mod.rs:1439, `held = !whole && (self.unmerged || self.backlog) && !self.disk.room()`, and :1447, `index()` only `if self.backlog && !held`; :1909, `wait_for_disk`'s `idle_for().min(disk::POLL)`.
  - Before O17, one `!self.disk.room()` check returned before anything. So the one test of indexing waiting below the floor, `below_the_disks_floor_background_work_and_indexing_wait_while_inbox_rows_and_live_writes_go` (:3624), caught any break of it; and `wait_for_disk` slept a fixed disk::POLL.
- **What happens:** that test runs on a store just opened, whose `unmerged` starts true and is never cleared while it's held. So there O17's `unmerged` term holds indexing alone, and three switches pass all 207 tests with a faked disk and their modules:
  - **M2,** `held` on `unmerged` alone and `index()` whenever `backlog`. It lets a step of indexing through each time the disk goes short with nothing left to merge, a running jawd's usual state.
  - **M2b,** only `&& !held` dropped from the index branch.
  - **M3,** `wait_for_disk`'s wait not capped at disk::POLL.
- **What they'd cost:**
  - **M2 and M2b:** with either, below the floor, `index()` comes before `empty_wal()` once the WAL's emptying is due, and goes on, step after step, until nothing is left to index. So in a jawd whose disappearing messages keep going below the floor, which is O17's own case, the whole backlog of text waiting is indexed while the disk is short of room. A history sync's backlog is among it, and that's the background work the watchdog holds.
  - **M3:** a background batch that waits for the disk with nothing to index, merge or empty sleeps until the next inbox row, live write or leftover wakes the writer, up to `idle_for`'s hour, after the disk has room.
- **Fix:** a case, test code only, like the audit's probe (`audit_probe_below_the_floor_with_nothing_left_to_index_or_merge`, kept beside this round's report as disk-watchdog-audit-r5-probes.rs). On a watched store indexed to the end:
  - below the floor, a background batch waits, and goes on within 5 s once the disk has room;
  - indexed again, below the floor, a live message's text waits in fts_backlog;
  - and the WAL a deletion for you of a disappearing message leaves is emptied while that text still waits.
  - At the head it passed 20 of 20 at a load of 18 to 27. M2 fails it ("with nothing left to merge, indexing waits for room", left 0), M2b fails it ("and the text waiting to be indexed still waits", left 0), and M3 fails it (the 5 s).

### O18. Other: the merges no erasure left go below the floor while a whole merge is under way, and nothing tests that they wait otherwise

- **Where:** store/mod.rs:1976-1993, `merge()`'s ordinary branch, which a whole merge's steps under way go through, as `Whole::due` is None while one is (:1156).
- **What happens:** below the floor, while a contentless index's whole merge is under way, each step merges messages_fts's levels first, and the other contentless index's. The plan says only the merges no erasure left wait for room.
  - Switch M1 passes all 207 tests: `held` on `backlog` alone, so the merges no erasure left go whenever nothing is left to index.
  - Neither fills the disk. A merge rewrites what indexing already wrote, in steps of 64 pages behind the WAL's room, and frees what it merged; and indexing, which adds, still waits. So a test of it would pin a choice rather than a guarantee.
- **Fix:** say in the plan that a whole merge's steps take the levels' merges first, as they always do; or have `merge()`, while the disk is short, step only the whole merge under way, with a case M1 fails.

### O19. Other: a failed idle step's log line says it couldn't index

- **Where:** store/mod.rs:1458, "jawd: can't index messages for search, so it's tried again in N ms".
- **What happens:** since O17, the step that fails below the floor is a whole merge or the WAL's emptying, as O17's test's full jaw.db shows. Each try prints that line, at 60 s at most.
- **Fix:** say which step failed.

### O20. Other: the WAL's emptying and a whole merge on a full disk give up no ballast, and the plan's words for it

- **Where:** store/mod.rs:1455-1464, where a failed idle step only waits to be tried again. The writer's restart and copy of the last frames call `self.disk.full()` (:1868-1870); this doesn't.
- **What happens:** as the O17 builder found, and as "The two things the O17 builder flagged" above explains, it may land. Two of the plan's words are wrong, though:
  - "the ballast goes with the writer's next commit": it goes with the next commit a full disk refuses;
  - "however short of room the disk is": with no room at all and the ballast gone, an erasure's own commit waits in `retrying` too.
- **Fix:** have a failed idle step call `self.disk.full()` on `disk_full(&e)`, as `room()` does, so the WAL's emptying needn't wait for the next refused commit or pass. Or keep it as it is, with the plan's two phrases made exact.

### O21. Other, for disappearing: its expiry test's 200 ms

- **Where:** expiry.rs:365, `let soon = now_ms() + 200`, and the assertion of 70 at :383.
- **What happens:** see "Disappearing's expiry test under load". It fails, rarely and only under a whole suite's load, with a right count.
- **Fix:** count what the copy holds before the timer starts, and assert 70 there, so a slow start says what it is; or give the 70 messages seconds rather than 200 ms. Until then it stays watched.

## Measurements

All in one clone of the worktree under /tmp/jaw-audits/disk-watchdog-r5, built with `CARGO_INCREMENTAL=0`. The test binaries were copied out and run directly under `nice -n 10` with a timeout, the machine at a load of 13 to 63 on 20 cores.

- **Switches:** each applied alone to the head (an exact replacement found once, `cargo test -p jawd --bin jawd --no-run`, the binary copied, the tree put back and checked clean). Round four's six ran against T5's, T6's and T7's three cases, O17's six against its test, and M1, M2, M2b and M3 against the 207 tests with a faked disk and their modules (store::tests, sync::tests, disk::, both expiry modules, media's two disk tests and main.rs's two watchdog tests). The runner and its results are kept beside this round's report.
  - M1, M2, M2b and M3: 207 passed, 3 ignored (the `measure_*`), each in about four minutes at a load of 20 to 63.
  - T8's probe: the head passes; with M2, M2b or M3 it fails, as in T8.
- **Steadiness:** T6's and T7's cases, 20 runs each alone from the head's binary, at a load of 49 to 53: 20 of 20 each. T8's probe at the head, 20 runs at a load of 18 to 27: 20 of 20.
- **The suite at the head**, 296 tests by module, then main.rs's 14 by name:
  - **Run 1** (01:02 to 01:31, load 15 to 58, switch builds beside it): the modules 289 passed and 7 ignored (the `measure_*`), none failed. main.rs: 13 passed; `a_second_jawd_on_the_same_folder_is_refused_until_the_first_has_gone` failed ("another jawd is running"), a listed lock test, which then passed alone 3 of 3.
  - **Run 2** (01:36 to 02:05, load 12 to 63, run 3 beside it): 288 passed; the checkpointer's listed idle test, `an_idle_store_empties_its_wal_and_then_its_checkpointer_sleeps_until_the_next_commit`, failed ("woke 2 times with nothing to do"). main.rs: 13 passed; `jaw_lock_goes_only_once_the_runtime_has_shut_down` failed ("and went once it had"), a listed lock test. Each passed alone 3 of 3.
  - **Run 3** (01:45 to 02:13, load 13 to 63): 289 passed, none failed. main.rs: 13 passed; `a_jawd_refused_while_another_stops_leaves_no_socket_behind` failed ("another jawd is running"), a listed lock test, which then passed alone 3 of 3.
  - **Watched tests:** every one in them passed in all three runs: the live writes beside inbox rows, disappearing's expiry test, sync's stop bound, O17's test, T5's to T7's cases, and the 21 WAL tests and 13 blob tests (but the listed idle test once). The restart test, `send::tests::a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left`, passed alone 3 of 3.
  - **The control for the switches,** the 207 tests with a faked disk at the head: 207 passed.
- **The expiry probe:** as in "What was asked, checked".
- **The moves:** the range-diff and per-file multisets above, with the scripts beside this round's report.
- **Plans:** `JAW_PLANS=write` on store::plans: 3 passed, the clone unchanged.

## Landing on today's main

- **Main:** origin/main is 645a0f7b, the branch's base, so there's nothing to rebase onto.
- **The trailer:** `check-data-audit.py 645a0f7b 8d3473ea` names 16 commits, every one but the plan's (16a6067f):
  - round four's 12: 4def48ac (the change), 6258f728 (P1), 08e6b9aa (T2), 1427ee75 (O4), 42d50e2a (O2), 9aca4514 (T3), 2d50db34 (T4), e6534178 (O12), 0c30825c (O11), 6bc99a36 (T5), 0752aa8e (O16) and 34283633 (O15);
  - T1's b87b15f9, as engine-stop put main.rs on the list;
  - T6's 0417772f, T7's 394e08d0 and O17's 8d3473ea.
  - T8's commit, in store/, will take it too.
- **No migration:** the range changes neither schema nor migrations.lock nor plans.txt.
- **What the lander checks after T8:** T8's case with M2, M2b and M3, the rest of this round being done; the branch on main as it is then; and the gate.

## Verdict

Changes needed before landing: T8, a case that holds indexing waiting below the floor in a writer with nothing left to merge, a background batch going on once there's room when nothing is left to index, and the WAL's emptying not letting indexing through. It is test code, and it should fail with M2, M2b and M3. Everything rounds one to four asked for, T6 and T7, both moves and the rest of O17 are done and tested. O18 to O21 may stand as follow-ups, O21 being disappearing's, with O13, O14 and round one's O1, O3 and O5 to O10.

Verdict: changes needed

# Round six: dc148ca2

Range: 645a0f7b..dc148ca2 in disk-watchdog, 5 November 2026. Auditor: database-expert, a fresh one for this round. Eighteen commits on 645a0f7b, which is origin/main: round five's seventeen, unchanged, and T8's dc148ca2, a test and two sentences of the plan. The gated tree, 1dd6a4b5, is the head's.

## In short

Nothing is lost and nothing is left inconsistent, and T8 is closed.

- **T8's test** holds what round five asked, on a watched store indexed to the end, below the floor:
  - a background batch waits, and goes on within 5 s once there's room;
  - a live message's text waits to be indexed;
  - and the WAL a disappearing message's deletion for you leaves is emptied while that text still waits.
- **Its switches:** each of round five's three fails it in 40 runs of 40, every time at its own assertion, at loads up to 73. At the head it passed 119 runs of 119, at loads of 12 to 72.
- **Its plan sentences** say what it holds and what its switches are.
- **Nothing else changed:** the range-diff against round five's head shows only T8's commit.
- **The gate** meets the flakes rule.
- **The landing:** the branch is on main's head, 17 commits take the trailer, no migration is needed, and plans.txt is what the head writes.
- **One new follow-up,** O22: two of the test's windows for what mustn't happen are sleeps where a count of the writer's steps would do. M2 fails it every time regardless.

## What was asked, checked

- **T8's test,** `below_the_disks_floor_with_nothing_left_to_merge_indexing_and_background_work_wait_while_the_wal_is_emptied` (store/write/expiry/tests.rs:2509), beside O17's. It's round five's probe, with exact counts.
  - **Its setting is the one round five named.**
    - The fake disk has 10 GB free of 100 GB, whose floor is 2 GB, and `Disk::room` reads it on every call (disk.rs:143).
    - The store is indexed to the end: `indexed()` is answered only once `backlog` and `unmerged` are both false (store/mod.rs:1383, :1466).
    - A live write then sets `backlog` alone (:2228); only an indexing step sets `unmerged` (:2054).
    - So each time the disk goes below the floor, nothing is left to merge, and O17's `unmerged` term can't hold indexing on its own, as it did in the watchdog's first indexing test.
  - **What it checks, below the floor at 1 GB:**
    - A history message in Lane::Background hasn't gone two polls later (:2523). With 10 GB again, it goes within 5 s (:2526), though nothing comes to wake the writer. That holds `wait_for_disk`'s cap at disk::POLL: with nothing to index, merge or empty, `idle_for` is an hour (:1403).
    - After indexing again, a live message's text is still in fts_backlog three polls later, the count exactly 1 (:2533).
    - A disappearing message is applied and deleted for you in the live lane. One row changes, and the count is 1 again, as the deletion's trigger takes the deleted text. The WAL's note goes below the floor (`wal_emptied`, which polls it for 10 s at least), and the live message's text still waits: 1 (:2539).
    - With room again, everything is indexed: 0.
  - **Counted rather than timed:**
    - What the switches break is judged by counts: fts_backlog's, the WAL's note, the batch finished or not.
    - The one clock a switch is caught by is M3's 5 s, which round five asked for, against a 250 ms poll.
    - The two windows for what mustn't happen, two polls and three, are sleeps (O22).
  - **Its switches,** each applied alone at the head and run under load:

    | Switch | T8's test, 40 runs |
    | --- | --- |
    | M2, `held` on `unmerged` alone, `index()` whenever `backlog` | 0 of 40, each "with nothing left to merge, indexing waits for room" (:2533, left 0, right 1), at loads of 16 to 63 |
    | M2b, only `&& !held` dropped from the index branch | 0 of 40, each "and the live message's text still waits to be indexed" (:2539, left 0, right 1), at loads of 16 to 70 |
    | M3, `wait_for_disk`'s wait not capped at disk::POLL | 0 of 40, each "and goes on once the disk has room, with nothing to index: Elapsed(())" (:2527), at loads of 15 to 73 |

  - **Its plan sentences,** as T6's and T7's commits added theirs:
    - The `store` bullet's new last sentence says what the test holds, in the test's own terms.
    - "Each rule's switch" gains "From its fifth round" with the three switches, each named for what it does.
    - Nothing else in the plan or the design changes, and nothing needs to, since T8 is test code.
- **Nothing else changed.** `git range-diff 645a0f7b..8d3473ea 645a0f7b..dc148ca2` shows round five's 17 commits "=", with their hashes unchanged, and dc148ca2 the only new one. That commit changes the test file (+43) and the plan (two sentences), and nothing else.
- **The gate,** from the fix round's report and its record (disk-watchdog-fix-r5-gate.txt), in the worktree at dc148ca2:
  - check-generated, lint and build exit 0.
  - `make test` exit 2. Cargo stopped at jawd's unit tests, 1,304 passed and 5 failed: disappearing's expiry test (`erased: 62`, O21's margin), the feed's half-closed and lane tests, media in's starts-check test and the socket's connection-full test. Each is on the fix brief's list and passed alone 3 of 3 there. I ran each alone 3 times again from the head's binary: 15 of 15.
  - The targets `cargo test --workspace` runs after jawd's unit tests are jawd's five integration tests, wa-proto and the doc tests (the Makefile's `test` is Go's tests, then `cargo test --workspace`). Each passed on its own.
  - No watched test failed, and none in the watchdog's or O17's module did.
  - So the gate meets the rule, on the head's tree.
- **The landing onto origin/main** is in the verdict.

## Findings

Nothing is lost and nothing is left inconsistent, and no rule the branch touches is left untested. One new follow-up, O22, which may stand with O18 to O21, O13, O14 and round one's O1, O3 and O5 to O10.

### O22. Other: two of T8's windows for what mustn't happen are sleeps

- **Where:** store/write/expiry/tests.rs:2523 and :2532. `tokio::time::sleep(disk::POLL * 2)` comes before the history batch is checked as still waiting, and `* 3` before the live message's text is counted.
- **What happens:** a switch that lets either through is caught only if the writer takes a step within the sleep. The module already counts the writer's idle steps (`idle_steps_in`, :2402, O17's test), which would make that certain whatever the load.
  - It isn't a gap. Under M2 the text goes at the writer's first idle step after the live write, about a millisecond later. A lone live write sets no live pause: `live_at` is set only by inbox groups and conversions of several inbox rows (store/mod.rs:1678, :2093). So three polls are ample, and M2 failed 40 of 40 at loads up to 63.
  - The history batch's window isn't one of round five's switches. Round one's tests hold background work waiting.
- **Fix (follow-up):** wait for the writer to take a few idle steps after the live write, and after the batch is queued, rather than for polls.

## Measurements

All in one clone of the worktree under /tmp/jaw-audits/disk-watchdog-r6, built with `CARGO_INCREMENTAL=0`. Test binaries were copied out and run directly, each run with its own TMPDIR, under `nice -n 10` and `gtimeout -k 30 300`, on 20 cores.

- **Switches:** each applied alone to the head (exact replacements found once, `cargo test -p jawd --bin jawd --no-run`, the binary copied out, the file put back and the clone checked clean), and built in 56 to 57 s. Each ran against T8's test 10 times one at a time (load 15 to 17); 10 times, two at once, beside two loaders (load 30 to 37); and 20 times, two at once, beside three loaders (load 56 to 73). The results are in the table above.
- **Steadiness at the head:** T8's test passed 119 of 119:
  - the control, and 8 runs before my first launch died with its shell call (load 12 to 18);
  - set 1, 20 in a row beside the switch builds: 2.90 to 2.98 s, load 14 to 16;
  - set 2, 30 runs, three at once, beside two loaders (store::tests and sync::tests from the head's binary, 12 threads each): 2.99 to 3.44 s, load 30 to 38;
  - set 3, 60 runs, four at once, beside three loaders (store::, sync:: and feed::, 16 threads each): 3.01 to 4.13 s, load 56 to 72.
- **The gate's five listed failures,** each alone 3 times from the head's binary: 15 of 15, at a load of 33 to 46.
- **Plans:** `JAW_PLANS=write` on store::plans: 3 passed, and the clone was unchanged.
- **Landing facts:**
  - `git ls-remote` (read-only) shows GitHub's main at 645a0f7b.
  - `check-data-audit.py 645a0f7b dc148ca2` exits 1 and names 17 commits.
  - `git diff 645a0f7b dc148ca2` touches no schema, migration, migrations.lock or plans.txt.

## Verdict

Approved. T8's test holds three things, and it fails with M2, M2b and M3 every time and is steady under load:
- indexing waits below the floor with nothing left to merge;
- a background batch goes on once there's room, with nothing to index;
- and the WAL's emptying doesn't let indexing through.

Everything rounds one to five asked for is done and tested, and the gate passes on the head's tree.

**Landing.** origin/main is 645a0f7b, the branch's base, so there's nothing to rebase.
- **The trailer:** give `Data-Audit: docs/audits/2026-11-02-disk-watchdog.md` to these 17 commits, named by their hashes before the trailers: 4def48ac, 6258f728, b87b15f9, 08e6b9aa, 1427ee75, 42d50e2a, 9aca4514, 2d50db34, e6534178, 0c30825c, 6bc99a36, 0752aa8e, 34283633, 0417772f, 394e08d0, 8d3473ea and dc148ca2. The plan's 16a6067f doesn't need one, though `git rebase -x` giving it one does no harm.
- **The note:** commit this note in the branch.
- **What the lander checks:**
  - The trailers changed no tree: before the note's commit, the head's tree is still 1dd6a4b5, the gated tree.
  - `make check-audit` exits 0.
  - Main is still at 645a0f7b when pushing. If it has moved, rebase onto it. If the rebase conflicts, or main's new commits change a data-layer file this branch changes, the move comes back to the audit before landing, as O17 met disappearing with no conflict. Otherwise run the gate again on the rebased tree.
  - The push is from ~/src/jaw-disk-watchdog, through the pre-push hook, never with --no-verify.
- **Follow-ups:** O18 to O22 stand, O21 being disappearing's, with O13, O14 and round one's O1, O3 and O5 to O10.

Verdict: approved
