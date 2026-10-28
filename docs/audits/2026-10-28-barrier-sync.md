# Audit: barrier-sync

Range: `fa9265c..4a7cbb9` in `barrier-sync` (worktree `~/src/jaw-barrier`), 28 October 2026. Auditor: database-expert. One commit, `4a7cbb9`, on main's `fa9265c`, which is still `origin/main`. Paths are under `crates/jawd/src/` unless they say otherwise; line numbers are at `4a7cbb9`.

The commit puts jaw.db behind a VFS of jawd's own (`store/vfs.rs`). SQLite asks a commit's sync at `SQLITE_SYNC_NORMAL`, which the VFS flushes in full (`F_FULLFSYNC`). It asks a checkpoint's two syncs and a new WAL header's at `SQLITE_SYNC_FULL`, which the VFS sends to the drive with `fsync` and fences with one `F_BARRIERFSYNC` on a descriptor of the file's folder, where that folder is on the home folder's volume.

I:
- read the whole diff, the store code around it, and the bundled SQLite 3.53.2 wherever it syncs;
- checked the scheme against `fcntl(2)` and `fsync(2)` on this Mac, and measured what the drive does with each kind of sync;
- recorded beneath the VFS every write, send, barrier, full flush, truncate and delete of jaw.db, its WAL and its journal, with every ack and durable answer. I checked the record against the order a power cut needs under those semantics, with three broken setups as controls;
- killed a store at 30 points, several of them inside the VFS;
- ran nine mutants of my own against the branch's sync tests;
- ran pairs of `jawd bench store`, `jawd demo` and `jawd bench inbox` against main, and against the branch's own binary flushing in full;
- ran the gate.

## Summary

Approved, with one test to add before pushing (T1). Nothing here loses data or leaves it inconsistent.

- **Fencing is sound under the semantics macOS documents.**
  - `fcntl(2)` on this Mac describes the exact recipe the VFS follows: `fsync` each descriptor, then issue a single barrier, which orders everything `fsync`'d on the same device before it.
  - `F_FULLFSYNC` persists everything `fsync`'d on the device before it, so every ack's full flush also covers what was fenced before it.
  - A barrier through the folder's descriptor costs what one through the file does, 0.77 against 0.74 ms at p50. On a clean folder it still costs 0.44 ms, against 0.003 ms for that folder's `fsync`, so it reaches the drive, and it isn't a full flush (4.67 ms).
- **No commit before an ack can be fenced.**
  - With `fullfsync` off, SQLite 3.53.2 asks `FULL` in three places only: the WAL before a checkpoint copies, jaw.db once a copy reaches the WAL's end, and a new WAL header. Commits and every rollback-mode sync ask `NORMAL`, which the VFS flushes in full.
  - `connect` sets `fullfsync` off and opens every connection through the VFS, and a test pins both for every role.
- **The order a power cut needs held** in 11 recorded runs: 157,211 acks and 8,634 durable answers. They covered restarts, cuts to nothing and to the size limit, the WAL's deletion at close, and 437 of the checkpointer's own syncs of jaw.db, and included runs with every barrier refused and with jaw.db off the home volume. The recorder flags each of three broken setups.
- **30 kills lost nothing,** including kills between a fenced sync's `fsync` and its barrier, and just before a full flush.
- **The tests catch what they say.**
  - The builder's seven mutants each fail one to three tests.
  - Seven of my nine are caught too, each by the test meant for it. The other two are T1.
  - `roomy_store`'s 25 ms slowdown doesn't weaken what its tests guard: they still catch no flush after an outrun pass, a flush after every pass, and a flush each time the checkpointer looks again.
- **T1, fix before pushing:** neither of `commits_synced`'s two new guards has a test that fails without it. It's a test alone.
- **Performance (P1), a follow-up:**
  - The barrier shortens a restart's pause: 30 ms at p50 against 40 to 45 flushed in full, with a reader on the WAL.
  - A sixth to a third of what a fenced checkpoint saves comes back on the next ack's full flush, 0.9 to 1.6 ms.
  - End to end at a load of 27 to 80, no budgeted figure is consistently better or worse. The one consistent gain is shorter waits for the checkpointer.
  - At this load both modes miss the budgets: acks in every run, a live message while history goes in in every run, and opening a chat in five of eight runs, flushed in full too.
- **Hygiene (H1 to H6):**
  - status says "fenced" while the drive refuses every barrier;
  - no test shows that barriers are taken;
  - the home volume stands in for an Apple SSD;
  - the order test checks less than it could;
  - two pieces of wording;
  - an older jawd shown as "fenced".
- **The gate:** lint and check-generated pass. `cargo test --workspace --no-fail-fast` passes every crate but the terminal's, which failed two drawing benches that run without jawd or SQLite. jawd's 520 passed, and the four tests the builder's runs failed all passed.

## Findings

### Data loss

None.

### Inconsistency

None.

### Fix before pushing

**T1. Neither of `commits_synced`'s two new guards has a test that fails without it.**
- **Severity:** hygiene. Nothing today can make a commit before an ack run fenced, but these guards are what would refuse one on a path added later.
- **Where:** `store/mod.rs:137-142`, `Ok(synchronous >= 2 && fullfsync == 0 && vfs::opened_through(db))`.
- **What happens:** with either guard removed, all 15 of the branch's sync tests pass (my mutants, under Measurements).
- **Why it's only hygiene:** the configuration itself is tested. `connect` sets `fullfsync` off and reads it back on every role (`:401`), and `every_connection_to_jaw_db_is_as_asked_and_its_checkpoints_reach_the_disk` pins that and the VFS for the writer, a reader and the checkpointer. While every connection comes from `connect`, the guards change nothing.
- **What they guard against:** a writer opened some other way, for a new role or a tool, with the ack sent while the commit is still in the drive's cache.
  - Through SQLite's own VFS with `fullfsync` off, a commit at `FULL` is a plain `fsync`.
  - Through jaw's VFS with `fullfsync` on, a commit's sync is asked at `FULL` and fenced.
  - `commits_synced` refuses both cases, and nothing checks that it does. As far as I can tell, main's `fullfsync == 1` guard was untested the same way.
- **Fix:** a unit test beside `switching_the_writers_commits_between_synced_and_not_keeps_its_statements_prepared`:
  - a connection opened with SQLite's own VFS, at `synchronous = 2`, isn't `commits_synced`;
  - nor is `connect(.., Role::Writer)` given `PRAGMA fullfsync = 1` and `synchronous = 2`.

  It's in tests alone, so it needs no further audit.

### Unpredictable performance (follow-ups)

**P1. A fenced sync leaves its pages in the drive's cache for the next full flush, which is an ack's; and at loads of 27 to 80 no budgeted figure is consistently better or worse.**
- **The drive** (`drive2.py`, under Measurements), at a load of 73 to 76:
  - a commit's `F_FULLFSYNC` right after a checkpoint's copy of 64 to 1,024 pages took 6.4 to 6.6 ms at p50 when the copy was fenced, against 5.0 to 5.2 ms when it was flushed in full;
  - 20 ms later it still took 5.8 to 6.2 ms, against 5.0 to 5.1;
  - the fenced copy itself was 4.6 to 7.3 ms quicker at p50.

  So a sixth to a third of what a fenced checkpoint saves comes back on whichever full flush follows. That's the builder's suspected cause. It costs the first ack after each pass 0.9 to 1.6 ms at p50, and it's a cost moved from the restart's pause and the checkpointer's thread, not a new one.
- **What it buys, the restart's pause** (my recorder's store at the small bound, with a reader holding the WAL 30 ms of every 40, the same binary fenced and flushed in full, three interleaved rounds at a load of 44 to 51): a restart held the writer 29.5 to 30.3 ms at p50 fenced, against 40.3 to 44.9 flushed in full. At p90 it was 48 to 59 ms against 63 to 155.
- **End to end:**
  - **`bench store` against main:** a live message while history went in took 35.7, 51.2 and 40.0 ms at p99 on the branch, against 22.3, 33.0 and 45.5 on main. While the history was indexed, the branch took 32.0, 18.1 and 16.8 ms, against 4.7, 13.5 and 9.4. The runs' loads differed: the branch's first went up to 73 while main's stayed under 49.
  - **The same binary, fenced and flushed in full:** this isolates the barrier, since flushing in full is what main does. In four rounds, the fenced mode was ahead in three both while history went in (22.1 to 53.0 ms against 23.4 to 47.6) and while it was indexed (3.7 to 12.0 ms against 5.6 to 13.8). Background work waited less for the checkpointer in all four, 5.4 to 9.3 s against 8.9 to 14.7, as the builder found.
  - **`bench inbox`**, ten pairs against main and six fenced against full:
    - against main, the median ack p99 was 110 ms on the branch against 129 at 300 rows a second, and 98 against 124 at 500;
    - fenced against full, it was 67 against 66 ms and 67 against 85;
    - in one pair the drive was saturated by other work, down to 11 synced commits in 4 s.
  - **`jawd demo`'s million-message history:**
    - against main, two pairs: acks were 62.4 and 28.4 ms at p99 on the branch, against 17.8 and 44.1; a chat opened in 99.5 and 18.1 ms, against 16.1 and 27.5. In each round, the run at the higher load was worse, whichever build it was;
    - fenced against full, two pairs: a chat opened in 31.1 and 78.9 ms at p99 fenced, against 50.5 and 45.7 flushed in full.
- **Whether a budget gets worse:** I can't show that one does. The figures with budgets miss them in both modes at this load:
  - a live message's 5 ms at p99 while history goes in: 22 to 53 ms in every run, fenced, in full and on main;
  - an ack's 10 ms at p99: 18 to 2,410 ms in every run;
  - opening a chat's 30 ms: missed in five of the eight demo runs, two of them flushed in full.

  The chat-open figure was the one to check: the builder's two pairs had the branch at 27 and 31 ms against 12 and 21. The fenced-against-full pairs put it on the load, not the barrier.
- **Follow-up:** before the README or the design quotes the gains, take pairs on a quiet machine, as the WAL-bound audit asked for its P1. Comparing fenced against full in one binary (`HOME` set to another device, as `ab.sh` does) is the cleanest way.

### Hygiene (follow-ups)

**H1. `jawd status` says "checkpoints fenced" while the drive refuses every barrier.**
- **Where:** `full_flush: !self.fenced` (`store/mod.rs:793`) is decided once, at open, from whether the writer's file has a folder to fence with (`:553`, `vfs.rs:89-92`). `make` falls back to a full flush, silently, each time a barrier is refused (`vfs.rs:168-175`).
- **Seen:** with every barrier refused, my recorder saw 151 refusals, each followed by a full flush, while `storage().full_flush` stayed false.
- **The design says** a refused barrier is flushed in full and that `jawd status` says which.
- **Fix:** count refusals in the VFS and report them, or report "flushed in full" once one has been refused.

**H2. No test shows that barriers are taken where they should be.**
- **Where:**
  - `a_barrier_is_asked_of_the_drive_and_one_it_cant_take_says_so` (`vfs.rs:568-574`) compares `barrier` with `noted::takes_barriers`, which calls `barrier` itself;
  - `checkpoints_made` (`store/mod.rs:2729-2735`) expects whatever `takes_barriers` says.
- **Effect:** if every barrier were refused on the Mac's own drive (a wrong descriptor or command, or a macOS that stops taking them on folders), every sync would fall back to a full flush, status would still say "fenced" (H1), and every test would pass. Durability doesn't rest on it; the branch's whole benefit does.
- **Fix:** on macOS, with the test's folder on the home folder's volume, assert that `takes_barriers` holds (skipped where a CI disk doesn't take barriers), or assert no refusals once H1 counts them.

**H3. "The home folder's volume" stands in for "an Apple SSD".**
- **Where:** `fence` (`vfs.rs:124-149`), and the design's "the Mac's own drive".
- **Why it matters:** `fcntl(2)` says barriers "require hardware support, which Apple SSDs are guaranteed to provide". A home folder can be on another drive: an external boot disk, a Fusion Drive, or a third-party SSD in a Mac Pro.
- **What it rests on there:** macOS turning the barrier into a full flush where the hardware can't take one.
  - XNU's `fcntl` promotes `F_BARRIERFSYNC` to `F_FULLFSYNC` when the file system says it can't fence, and HFS+ falls back to a full flush for a device without the barrier feature. Both are from their open source, not tested here.
  - APFS's handling is closed source, and nothing here can test it.
  - A drive that takes barriers and ignores them is in the same class as one that ignores `F_FULLFSYNC`, which the man page says some FireWire drives do.
- **The owner's Mac** is the case the design names.
- **Follow-up:** say in the design that the rule assumes the home volume is on the Mac's internal SSD, or fence only where IOKit says the drive is Apple's and internal.

**H4. The branch's order test checks less than the order a power cut needs, and records it serialized.**
- **Where:** `assert_ordered` (`store/mod.rs:2842-2877`), over the events `vfs::noted::around` records while holding one lock across each operation (`vfs.rs:501-510`).
- **What it covers:** restarts, cuts to nothing and the new header's sync.
- **What it doesn't:**
  - that a page is copied into jaw.db only once its frame is fenced;
  - that an ack follows a full flush covering what the WAL depends on;
  - the WAL's cut back to WAL_LIMIT and its deletion at close.
- **How it counts:** any sync as ordering, whatever it was made of. Holding the lock across each operation forces the watched files' I/O into one sequence.
- **Why it doesn't block:** these orders are SQLite's, the flags test covers what jawd chooses, and my recorder checked them all with the writer and the checkpointer running concurrently.
- **Follow-up:** extend the checker, or adopt mine (Measurements).

**H5. Two pieces of wording leave out where the cost went.** The design's "Commits" says the WAL's restarts and the checkpointer's passes "cost the drive no flush of their own", and the comment at `store/mod.rs:311-318` counts a restart's "three syncs". Both are true, but the next full flush pays for their pages (P1). Say so, as the WAL-bound audit's P2 asked of the restart's three flushes.

**H6. `jaw status` shows "checkpoints fenced" for a jawd too old to say.**
- **Where:** `Storage::full_flush` is `#[serde(default)]` false (`crates/jaw-proto/src/lib.rs:371-374`), and `describe` prints "fenced" for false (`crates/jaw-client/src/lib.rs:281`).
- **Also:** no test covers the line's text.
- **Fix:** `Option<bool>`, printing only what jawd said.

## What I checked, and how it came out

**The semantics the scheme rests on.**
- `fcntl(2)` on this Mac (macOS 26.6.2), for `F_BARRIERFSYNC`:
  - "The barrier applies to I/O that have been flushed with fsync(2) on the same device before. These operations are guaranteed to be persisted before any other I/O that would follow the barrier."
  - "To do so, execute operations of phase one, then fsync(2) each FD and issue a single barrier. Finally execute operations of phase two."
- For `F_FULLFSYNC`: "data that had been fsync'd on the same device before is guaranteed to be persisted when this call returns."
- **The fenced sync is that recipe.** First `fsync` of the file, through SQLite's own `xSync` at NORMAL, which is `fsync` on macOS (`sqlite3.c:43965-44010`). Then one barrier on a descriptor of the same device. And every full flush persists everything fenced before it began.
- **The folder is on the file's own device.** SQLite resolves symlinks before `xOpen` and names the WAL after the resolved path, so the folder is the real file's.
- **The barrier reaches the drive** (part 1 of the drive probe, under Measurements). The folder's own `fsync`, which comes with it, is harmless.

**Where SQLite asks for which sync** (`sqlite3.c`, 3.53.2):
- **The flags.** With `fullfsync` off, `checkpoint_fullfsync` on and `synchronous` at FULL or NORMAL, `sqlite3PagerSetFlags` (`:63245-63257`) sets:
  - `syncFlags` to NORMAL;
  - `WAL_SYNC_FLAGS` to NORMAL, or 0 at NORMAL;
  - `CKPT_SYNC_FLAGS` to FULL.
- **Only three sites ask `FULL`,** all with `CKPT_SYNC_FLAGS`:
  - the WAL before a checkpoint copies (`:69713`);
  - jaw.db once a copy reaches the WAL's end (`:69769`);
  - a new WAL header (`:71558`).
- **Every commit's sync** is `WAL_SYNC_FLAGS`, so NORMAL (`:71389`, `:71648`).
- **Every rollback-mode sync is NORMAL,** either through `syncFlags` (`:61008`, `:61691`, `:63963`, `:63975`, `:66012`) or as a literal (`:63656`, `:90859`), and the VFS flushes those in full. That covers a new jaw.db before it's made a WAL.
- **`VACUUM INTO`** gives its output the main database's flags (`:161539-161556`), so the pre-upgrade backup's syncs are full flushes. jawd also `sync_all`s the backup and its folder itself (`store/schema.rs:1201-1204`), which is `F_FULLFSYNC`.
- **A failed sync can't let the WAL start again.** SQLite advances `nBackfill` only after the sync of jaw.db at the end of a pass succeeds (`:69764-69774`). A sync the VFS can't send returns its error with no barrier (`vfs.rs:170-172`).
- **The checkpointer's `flush`** asks `FULL` of jaw.db's own file (`store/mod.rs:496-516`), so it's fenced. It only moves the sending of pages off the writer, since SQLite starts the WAL again only after a pass that reached the end and synced jaw.db.

**Nothing can make a commit before an ack fenced.**
- **A connection with `fullfsync` on:** `connect` sets it off and reads it back on every role (`store/mod.rs:401`), pinned by `every_connection_…`. `commits_synced` refuses one too, untested (T1).
- **A connection not through the VFS:** no production code opens jaw.db any other way. The only other opens are a read-only one in `store/bench.rs:593` and tests'. `commits_synced` refuses one, untested (T1).
- **The flag composition changing:** pinned at the bundled SQLite by `every_commit_before_an_ack_is_flushed_in_full_and_every_checkpoint_and_new_wal_header_fenced`, which any upgrade of SQLite would have to pass. Nothing checks it at run time.
- **SQLite asking a commit's sync at `FULL`:** no site does with `fullfsync` off (above).
- **Every ack and durable answer** goes through `commits_synced` inside its transaction: `store/mod.rs:1356-1365` for inbox groups and the live writes in them, `:1633-1636` for durable live writes.
- **The engine's `session.db`** commits with a plain `fsync` and relies on jaw.db's `F_FULLFSYNC` (`engine/internal/wa/session.go:22`). It still gets one before each ack, since `keep` waits for jawd's ack (`engine/internal/wa/engine.go:146-170`).
- **A clean stop** notes itself with a durable commit before its truncate (`sync/mod.rs:381-386`, `main.rs:509-519`), and nothing after the truncate leaves the device.

**The order a power cut needs, recorded** (`audit_power_cut_order_with_barriers`, tables under Measurements).
- **The recorder:** a shim installed as SQLite's default VFS before jaw's registered, so jaw's VFS wraps it. Every send (`fsync`), full flush, write, read of a WAL frame, truncate and delete passes through it, with hooks for barriers, acks and durable answers. Each operation's start and end are noted separately, so the writer and the checkpointer run concurrently.
- **The model:**
  - a write is sent by a sync of its file begun after the write ended;
  - a barrier fences what was sent before it began;
  - a full flush persists, and so fences, what was sent before it began and its own file's writes.
- **The checks:**
  - every page copied into jaw.db only once the frame it came from was fenced;
  - every new header over frames, every cut of the WAL and its deletion only once every write to jaw.db was fenced;
  - every frame after a new header, and every cut to a size, only once that header was fenced;
  - every ack and durable answer only once every WAL write, and every page of jaw.db written before the WAL last started again, was persisted;
  - every fenced send followed on its thread by its barrier or by a full flush;
  - every sync in rollback mode a full flush.
- **Results:** nothing broke the order in 11 runs. Each of the three broken setups is flagged, by thousands of violations.
- **My checker's own errors.** As first written it skipped every write. Then it credited each pending write to only one barrier or flush, and fenced writes sent while a flush ran, which `F_FULLFSYNC` doesn't promise. The second error could only miss violations, so I fixed both and ran everything again. The results here are the fixed checker's.

**Kills.** 31 runs reported, each a store in a process of its own, at a load of 63 to 74; 30 of them were killed with SIGKILL, at:
- inside the VFS: between a fenced sync's `fsync` and its barrier, after the barrier, and before a full flush;
- at a new header, and at the first frame after it;
- mid-copy, by the writer and by the checkpointer;
- at the checkpointer's syncs of jaw.db, and before and after its flush;
- before a pass, before and after a restart, and after a busy one;
- at the idle truncate;
- from outside.

The 31st point wasn't reached in its run, which ended on its own. After recovery, every acked row (up to 13,156 a run) and every durable answer (up to 310) was in jaw.db, with no duplicates, `quick_check` ok and no foreign-key violations. A kill doesn't drop the OS's cache, so this tests SQLite's recovery and jawd's order of commit and ack through the VFS. The recorder is what tests a power cut.

**The tests.**
- **The builder's seven mutants** each failed one to three tests, as the commit says.
- **Mine:** seven of the nine were caught, each by the test meant for it; the other two are T1. Table under Measurements.
- **`roomy_store`'s 25 ms** (`vfs.rs:451-466`, `store/mod.rs:3368-3369`) sleeps after each sync on the checkpointer's thread, and only in tests. It doesn't weaken what the three tests that use it guard. With it in place:
  - no flush after an outrun pass fails both flush tests, at their 60 s deadline;
  - a flush after every pass that copied fails the first;
  - a flush each time the checkpointer looks again behind a reader fails the reader test, with 11 flushes.

  It makes outrun passes reliable where fenced ones were over too soon, at the cost of slower passes. Those tests assert only flush counts and locks, so slower passes change nothing they check.

**Plans.** No new statement; the VFS adds no SQL.

## Measurements

**Setup**
- **Machine:** Apple M1 Ultra (20 cores, 128 GB), macOS 26.6.2, shared with other agents. Load averages ran from 27 to 80, given with each figure.
- **Scratch:** everything in `/tmp/jaw-audits/barrier-sync`, with one clone of the worktree and its `target` the only build folder, built with `CARGO_INCREMENTAL=0`:
  - release binaries of `fa9265c` (main) and `4a7cbb9` (the branch), each copied out and told apart by checksum and strings;
  - test binaries of the branch, of each mutant on it, and of a local branch over it with my probe and test-only hooks.
- **Kept:** the probe (`audit.rs`, with its hooks in `probe-and-hooks.patch`), the scripts and every run's output are in `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/748c56ed-4629-4060-809d-b03979d9677c/files/barrier-sync-audit/`.
- **Not touched:** the Go engine never ran, and nothing under `~/Library/Application Support/jaw` was opened. I stopped only processes I'd started. In `~/src/jaw` and `~/src/jaw-barrier` I only read, apart from this note.

**The drive** (`drive2.py`, 10:44 to 10:45, load 73 to 76, on the Data volume)

Part 1: a 4 KiB append and its sync, each kind in turn, 200 rounds.

| Sync | p50 | p99 |
| --- | --- | --- |
| `fsync` alone | 0.215 ms | 0.846 ms |
| `fsync`, then `F_BARRIERFSYNC` on the folder (the VFS's fenced sync) | 0.772 ms | 6.487 ms |
| `F_BARRIERFSYNC` on the file | 0.740 ms | 6.722 ms |
| `F_FULLFSYNC` (a commit's) | 4.670 ms | 10.300 ms |
| `F_BARRIERFSYNC` on the folder, nothing written since the last flush | 0.437 ms | 4.907 ms |
| `fsync` of the folder, nothing written since | 0.003 ms | 0.008 ms |

Part 2: a checkpoint's copy of random pages into a 64 MiB file, fenced or flushed in full; then, at once or 20 ms later, a commit's 4 KiB and `F_FULLFSYNC`, timed. 50 rounds each, at p50:

| Pages | Copy fenced / in full | Commit after, at once: fenced / in full | 20 ms later: fenced / in full |
| --- | --- | --- | --- |
| 64 | 4.6 / 10.3 ms | 6.37 / 4.98 ms | 6.18 / 5.11 ms |
| 400 | 20.9 / 25.7 ms | 6.59 / 4.99 ms | 5.85 / 4.98 ms |
| 1,024 | 42.8 / 48.3 ms | 6.42 / 5.17 ms | 6.14 / 5.08 ms |

**The order a power cut needs** (`audit_power_cut_order_with_barriers`): a store with inbox rows about every millisecond, live writes of ten messages, durable writes every 7 ms, history in batches of 2,000 and a reader holding a snapshot 30 ms of every 40. Then a quiet second, a truncate, and the store closed.

| Run | Seconds | Acks / durable answers | Restarts over frames | Cuts to nothing / to WAL_LIMIT | WAL deleted at close | Pages copied | Barriers (refused) | Checkpointer's syncs of jaw.db | Violations | Load |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 256 / 512 frames | 20 | 12,811 / 741 | 80 | 2 / 0 | 1 | 22,699 | 365 (0) | 0 | none | 54–60 |
| 8,192 / 12,288 | 30 | 18,865 / 1,421 | 10 | 2 / 0 | 1 | 52,970 | 359 (0) | 161 | none | 63–69 |
| jawd's own, 12,288 / 15,360 | 30 | 19,408 / 1,104 | 5 | 2 / 1 | 1 | 31,222 | 264 (0) | 117 | none | 63–69 |
| 8,192 / 12,288, every barrier refused | 20 | 13,172 / 698 | 4 | 2 / 0 | 1 | 15,277 | 0 (151), each flushed in full | 67 | none | 66–72 |
| 8,192 / 12,288, `HOME` on another device | 20 | 12,930 / 885 | 5 | 2 / 0 | 1 | 23,819 | 0: every sync flushed in full | 92 | none | 64–66 |
| 256 / 512, three fenced and three in full | 6 × 20 | 80,025 / 3,785 | 375 by the writer | | | | | | none | 44–51 |

- **Negative controls,** 10 s each at 256 / 512 frames, at a load of 58 to 63. Each is flagged:
  - a barrier that does nothing: 1,858 pages copied before their frames were fenced, 57 restarts over pages not fenced, 57 headers not fenced before their frames, and 207 sends with no barrier after them;
  - a barrier with nothing sent first: 2,161 copies, 66 restarts, 66 headers, and 6,652 acks and answers before what they depended on was persisted;
  - a commit's sync fenced: every one of 6,848 acks and answers, 3 rollback-mode syncs short of a full flush, and the journal deleted before jaw.db was persisted.

**The restart's pause:** the last row's six runs, the same binary fenced and flushed in full, interleaved.

| Round | Mode | Restarts | Held the writer, p50 / p90 / max | Load |
| --- | --- | --- | --- | --- |
| 1 | fenced | 56 | 30.3 / 59.2 / 118.6 ms | 50–51 |
| 1 | in full | 69 | 40.3 / 63.4 / 88.4 ms | 50 |
| 2 | in full | 64 | 43.1 / 62.6 / 149.4 ms | 47–50 |
| 2 | fenced | 73 | 29.5 / 48.6 / 203.6 ms | 46–47 |
| 3 | fenced | 55 | 30.0 / 48.4 / 171.1 ms | 46 |
| 3 | in full | 58 | 44.9 / 154.8 / 345.0 ms | 44–46 |

**Kills** (`audit_kill_harness`):

| WAL bound | Kill points | Load |
| --- | --- | --- |
| 256 / 512 | `fence-mid:7,40`, `fence-after:3,25`, `full-before:5,300`, `walhdr:3`, `walframe:3`, `dbwrite-w:300`, `restart-before:2`, `restart-after:3`, `restart-after-busy:1`, from outside at 3.7 and 6.1 s; `dbsync-c:2` not reached | 63–68 |
| 8,192 / 12,288 | `fence-mid:1,30`, `fence-after:12`, `flush-before:1`, `flush-after:2`, `pass-before:10`, `dbwrite-c:100`, `dbsync-c:3`, `full-before:400`, from outside at 2.0 and 4.4 s | 70–74 |
| 256 / 512, with quiet spells | `idle-truncate:1,2`, `restart-before:1`, `fence-mid:90`, from outside at 1.5 s | 70–74 |

All 31 runs were clean: 30 killed, and the one whose point wasn't reached.

**Mutants** (`mutate.py`): each built on the branch and run against its 15 sync tests in one process, at a load of 59 to 75.

| Mutant | Result |
| --- | --- |
| None | 15 pass |
| `commits_synced` without "through jaw's VFS" | 15 pass (T1) |
| `commits_synced` without "fullfsync off" | 15 pass (T1) |
| The barrier before the send | caught by `a_sync_that_cant_be_sent_fails_with_no_barrier` |
| A refused barrier not flushed in full | caught by `a_commits_sync_is_a_full_flush_and_a_checkpoints_is_fenced_only_where_barriers_hold` and the test above |
| A folder fenced on any volume | caught by `a_file_on_the_home_folders_volume_has_its_folder_to_fence_with_and_one_flushed_in_full_doesnt` |
| No flush after a pass the writer outran | caught by both flush tests, at their 60 s deadline |
| A flush after every pass that copied | caught by `the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end` |
| A flush each time the checkpointer looks again behind a reader | caught by `a_pass_a_reader_holds_back_flushes_jaw_db_once_not_each_time_it_looks_again` (11 flushes) |
| Status always "fenced" | caught by `off_the_macs_own_drive_every_sync_of_jaw_db_is_a_full_flush` |

**`jawd bench store --runs 20`**, release builds. Each run is in a folder of its own, deleted after it. Pairs are interleaved, the order alternating each round. "In full" is the branch's binary with `HOME` set to `/dev`, so every sync is `F_FULLFSYNC`, as on main.

| Round | Build | A live message while history went in, p50 / p99 / max | While indexed, p99 (indexing) | Background waited | Load |
| --- | --- | --- | --- | --- | --- |
| 1 | main | 1.19 / 22.33 / 85.0 ms | 4.73 ms (83.7 s) | 7.1 s | 29–49 |
| 1 | branch | 1.26 / 35.70 / 593.3 ms | 32.04 ms (145.1 s) | 12.6 s | 27–73 |
| 2 | branch | 1.55 / 51.18 / 223.9 ms | 18.06 ms (115.9 s) | 10.6 s | 59–75 |
| 2 | main | 1.26 / 33.04 / 220.0 ms | 13.48 ms (98.4 s) | 11.0 s | 48–63 |
| 3 | main | 1.45 / 45.49 / 250.2 ms | 9.36 ms (100.8 s) | 11.6 s | 49–63 |
| 3 | branch | 1.42 / 39.98 / 196.2 ms | 16.78 ms (106.4 s) | 5.8 s | 45–68 |
| A1 | fenced | 1.57 / 46.81 / 491.1 ms | 9.04 ms (96.0 s) | 9.3 s | 63–80 |
| A1 | in full | 1.43 / 47.55 / 248.5 ms | 13.17 ms (98.7 s) | 14.7 s | 60–65 |
| A2 | in full | 1.31 / 39.49 / 151.6 ms | 5.63 ms (74.1 s) | 8.9 s | 33–62 |
| A2 | fenced | 1.31 / 22.12 / 138.1 ms | 12.02 ms (98.3 s) | 7.5 s | 44–63 |
| A3 | fenced | 1.34 / 52.99 / 222.2 ms | 4.35 ms (77.7 s) | 6.1 s | 33–64 |
| A3 | in full | 1.18 / 32.88 / 478.3 ms | 13.76 ms (100.6 s) | 10.4 s | 30–63 |
| A4 | fenced | 1.37 / 22.12 / 165.5 ms | 3.69 ms (78.1 s) | 5.4 s | 33–51 |
| A4 | in full | 1.24 / 23.44 / 114.2 ms | 6.17 ms (94.1 s) | 9.0 s | 33–39 |

**`jawd demo`'s million-message history** on a fresh folder, a live message every 100 ms and a chat opened every 300 ms.

| Round | Build | Stored | Indexed | Acks, p50 / p99 / max | Chat opened, p50 / p99 / max | Load |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | branch | 162.5 s (6,153 a second) | 78.6 s | 16.06 / 62.39 / 590.3 ms | 2.58 / 99.49 / 178.0 ms | 46–67 |
| 1 | main | 67.4 s (14,843) | 86.7 s | 8.93 / 17.81 / 47.8 ms | 2.49 / 16.06 / 94.0 ms | 32–57 |
| 2 | main | 146.6 s (6,822) | 78.6 s | 12.15 / 44.10 / 73.5 ms | 2.04 / 27.54 / 60.3 ms | 57–73 |
| 2 | branch | 95.3 s (10,493) | 95.4 s | 9.54 / 28.42 / 36.9 ms | 2.69 / 18.08 / 37.6 ms | 45–62 |
| A1 | fenced | 69.8 s (14,329) | 76.9 s | 8.58 / 27.54 / 70.4 ms | 2.19 / 31.07 / 49.5 ms | 39–47 |
| A1 | in full | 92.8 s (10,782) | 78.5 s | 10.23 / 26.04 / 60.1 ms | 2.26 / 50.47 / 102.6 ms | 48–56 |
| A2 | in full | 139.0 s (7,192) | 94.0 s | 16.32 / 124.60 / 413.9 ms | 2.86 / 45.73 / 109.4 ms | 43–62 |
| A2 | fenced | 156.2 s (6,401) | 113.2 s | 13.79 / 62.90 / 505.1 ms | 2.57 / 78.93 / 158.7 ms | 44–66 |

**`jawd bench inbox --rates 300,500`:** an inbox row's ack p99 at 300 / 500 rows a second.

| Round | First | Second | Load |
| --- | --- | --- | --- |
| 1 | main 86.3 / 111.0 ms | branch 80.3 / 49.5 ms | 59–62 |
| 2 | branch 121.8 / 433.6 ms | main 129.3 / 385.2 ms | 61–68 |
| 3 | main 135.4 / 124.2 ms | branch 55.6 / 95.5 ms | 70–74 |
| 4 | branch 110.1 / 97.9 ms | main 75.2 / 69.7 ms | 76–78 |
| 5 | main 216.5 / 408.7 ms | branch 418.5 / 2,410 ms, the drive saturated: 11 synced commits in 4 s | 77–78 |
| A1 | fenced 25.8 / 29.0 ms | in full 50.4 / 72.9 ms | 63–66 |
| A2 | in full 40.7 / 139.0 ms | fenced 101.6 / 369.5 ms | 66–70 |
| A3 | fenced 69.3 / 137.5 ms | in full 76.3 / 76.2 ms | 70–76 |
| A4 | in full 56.2 / 108.2 ms | fenced 64.5 / 65.0 ms | 72–74 |
| A5 | fenced 59.6 / 68.4 ms | in full 224.2 / 66.2 ms | 69–74 |
| A6 | in full 113.9 / 93.5 ms | fenced 95.1 / 38.7 ms | 63–69 |

An ack's p50 was 8 to 27 ms throughout, one full flush with others' writes in the drive's cache, apart from the saturated round.

**The gate,** on `4a7cbb9`:
- `make lint`: exit 0, 12:26 to 12:27, load 45 to 40.
- `make check-generated`: exit 0, 12:27.
- `cargo test --workspace --no-fail-fast`: exit 101, 12:27 to 12:38, load 40 to 58.
  - **The terminal's crate (`jaw`):** 318 passed and 2 failed, both drawing benches that run on a made-up feed in-process (`jawd: false`), with no jawd and no SQLite:
    - `bench::tests::a_flood_alone_is_drawn_at_most_60_times_a_second` drew 573 messages a second in 12 frames a second;
    - `bench::tests::with_the_librarys_photos_showing_a_keypress_is_drawn_within_16_ms_at_p99_while_2000_messages_a_second_arrive` drew a keypress in 17.49 ms at p99, against 16.
  - **Every other crate passed:**
    - jawd: 520 passed, 10 ignored;
    - `starting` 1, `stopping` 2, `upgrading` 1;
    - `jaw`'s `settings` 3;
    - `jaw-client` 19, plus 3 in `tests/jawd.rs`;
    - `jaw-media` 22 and 17;
    - `jaw-proto` 12 and 3;
    - `jaw-voice` 36;
    - `wa-proto` 3.
  - **The four tests the builder's whole runs failed all passed:** notify's burst test, sock's full-connection test, the terminal's first-frame test, and the store's idle test.
- `make check-audit` waits for this note and the trailers.

## Verdict

Nothing here loses data or leaves it inconsistent. Under the semantics `fcntl(2)` documents, fencing a checkpoint's syncs and a new WAL header's is sound, in every order a power cut needs.
- Every commit before an ack is still flushed in full, and that flush persists everything fenced before it.
- In 11 recorded runs, no page reached jaw.db before its frame was fenced, and the WAL never started again, was cut or was deleted before every page copied into jaw.db was fenced. No ack went out before what it depended on was persisted.
- 30 kills lost nothing.
- The tests catch what they're for, and `roomy_store`'s slowdown doesn't weaken them.

Fix before pushing, in tests alone, needing no further audit:
1. **T1:** a test that `commits_synced` refuses a connection outside jaw's VFS, and one with `fullfsync` on.

These may stand as follow-ups:
- **P1:** the cost a fenced checkpoint leaves to the next ack's full flush, and quiet-machine pairs before the gains are quoted;
- **H1 to H6:** status and refused barriers, a test that barriers are taken, the home volume standing in for an Apple SSD, the order test's reach, the two wordings, and an older jawd shown as fenced.

To land:
1. Add T1's test and commit this note in the branch.
2. Give every data-layer commit the trailer `Data-Audit: docs/audits/2026-10-28-barrier-sync.md`.
3. Run `make check-audit`.

Verdict: approved
