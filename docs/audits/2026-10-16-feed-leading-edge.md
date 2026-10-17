# Audit: the feed's leading edge

Range: 027c560..16c152b in feed-leading-edge (c42bc3a, bb3b264, dbfaefe, 16c152b), 2026-10-16. Auditor: database-expert. This is a re-audit. The first pass (at f04f1aa, then based on cd29b0e) found two things that had to change. Both are fixed, and its findings are summarised under "First pass" below. `origin/main` is now at 004481d. As agreed, the branch lands only after `BEGIN IMMEDIATE` for the store writer (sync audit I5, built in the `durability` branch) is on main, rebased onto it.

What the branch does: coalescing timing is kept per window (`first`, `last`, `read`, `failing`). `Window::due` sends a window's first change at once if it hasn't been read for `burst` (50 ms), and otherwise waits for `quiet` (5 ms) or `burst`. A window is never read again sooner than `burst` after its last read, whether or not that read sent anything. A window whose read fails stays marked, is read again at that pace, and is resynced once it has failed for `behind` (5 s). Conversations are read before chat lists in a flush. `Client::ready` returns `Flush` at once when a flush is already due, and `Feed::reads()` counts window reads. There's no SQL, schema, migration or store change; the window query (`window::query`) is unchanged.

## Findings

### First pass, now closed

- **Unpaced reads (unpredictable performance, breaking the 5 ms push budget at four interfaces): fixed in bb3b264.** `read` is set after every read, sent or not (`client.rs:625`), and `due` paces on it (`:329-337`). Test: `changes_a_window_doesnt_show_cost_it_at_most_one_read_each_burst`, which allows at most one read each 50 ms plus two, and gets about 1,950 a second at c42bc3a. Measured below: about 20 reads a second per window per interface, at every interface count.
- **A failed read dropped what the window owed (inconsistency): fixed in bb3b264.** On an error, `first` and `last` are kept, so the retry waits for `read + burst`. The error is logged once, and after `behind` the window goes to `resync` (`client.rs:626-641`); the README says so. Test: `a_window_whose_read_fails_is_read_again_at_its_pace_then_resynced`. Through a side connection it makes a row the window shows unreadable, then checks four things: the window is retried 2 to 17 times in 150 ms, nothing is sent meanwhile, it catches up once mended, and it's resynced, with an `INTERNAL` error on watching again, once broken for longer than `behind`.
- **Test gap (H1): closed.** The racing property test is adopted as `windows_match_fresh_queries_while_writes_race_their_reads`: 8 seeds × 25 pauses, each checking six windows against fresh queries.
- **Flaky tests on main: both fixed as the sync audit described.** The slow-client test's bound is now relative (`3 × alone + 250 ms`, with `alone` the same writes unwatched), and its loop selects on the writer's handle, so it can't wait forever. The burst test allows 21 batches in a second, with a comment. The feed spaces its reads at least 50 ms apart, since `read` is set once each read has returned, so 21 can only come from the test timing batches as its own task receives them.
- **A correction to the first pass.** I wrote that a reader holding the WAL write lock while it rebuilds a torn wal-index header "is absorbed by the writer's 5 s busy timeout". It isn't, for the store's `BEGIN DEFERRED` transactions: SQLite calls no busy handler when a read transaction upgrades to a write, so the write fails at once with "database is locked". That is the sync audit's I5, the cause of the burst test's error. It's still not the feed's code, but the feed's reads are among the readers that can trigger it. That's one more reason reads had to be paced: they are now, at about main's rate per window.

### Unpredictable performance (stands as follow-ups; re-measure quietly)

- **A lone message's delay grows with the number of interfaces.** Each connection reads its own windows for the same commit: N interfaces each reading the open chat and the chat list make 2N reads, on 4 readers shared with the feed's own event reads. Conversations go first only within a connection, so the timed interface's conversation can wait behind other connections' chat lists. Measured with `jawd bench feed --far 250` at load 37-45 on 20 cores; "busy" means while 2,000 unread counts and names a second change in chats no window shows. One interface: p99 4.09 ms quiet, 2.21 ms busy. Four: 4.09-4.74 ms quiet, 4.61-5.27 ms busy. Eight: 8.25 ms quiet, 10.84 ms busy. The p50 rises with the count (0.84, 3.4, 4.7 ms) though reads a second stay low (10, 39, 77 quiet). So this is fan-out queueing, not read volume. One interface, the usual case, meets the 5 ms budget with margin even on this loaded machine. Four are at the budget and eight over it, on a machine with 37-45 runnable threads competing for 20 cores, which can't settle whether that's jawd or the machine. **Must be re-measured on a quiet machine** (load under 2) before landing: `jawd bench feed --far 250 --interfaces N` for N = 1, 2, 4, 8, three runs each, quiet and busy p99. That run decides it:
  - If 1 and 2 interfaces are within 5 ms p99, this stands as a follow-up.
  - If 1 or 2 interfaces exceed 5 ms p99, it's changes needed.
  - If 4 interfaces exceed 5 ms p99, file the follow-up, since the design doesn't say how many interfaces the budget assumes. Its fix is to read a spec once for every connection watching it (identical windows share a snapshot), or to give conversation reads a lane of their own ahead of chat lists in the read pool. The owner should also say what the budget assumes.
- **A chat list owed a reorder isn't paced by its reads** (`client.rs`, `Window::reorder_at` and the reorder half of `flush`'s filter). While catching up, a chat list whose reorder falls due reads with no `first`. If that read fails, `reorder` stays set and `reorder_at` (`reordered + hold`, already past) ignores `read`. `ready` then returns `Flush` at once, and the window is read back to back, one read at a time, for up to `behind`, until it's resynced. This needs both a catch-up and a failing read, and it holds about one reader for 5 s, so it doesn't break a budget in normal running; it's a follow-up. Fix: `reorder_at` returns at least `read + burst`, with a test like the failing-read one with `catching_up` set.

### Hygiene

- **The corruption test won't survive sync-v1.** `a_window_whose_read_fails_…` writes `spans = '{'`. sync-v1's messages rebuild adds `CHECK (json_valid(spans))` (`jaw-sync schema.rs:402`), so after the rebase the `UPDATE` fails and the test panics. The gate will show it, but plan for it. Either set `PRAGMA ignore_check_constraints = ON` on the side connection, or better, give `Client::snapshot` a `cfg(test)` way to fail. That survives the review's I7 too, whose second half makes the reader tolerate bad JSON, after which this injection wouldn't fail a read at all.
- **`failing` isn't cleared by a successful `watch.move`** (`relocate` calls `keep`, which leaves `failing` set). A window that failed, was moved and read successfully, and fails again within one more read is resynced after that one failure. Clear `failing` in `keep`.
- **The bench takes commit times after `apply` returns** (`bench.rs`, `lone` and `write`), so delays are understated by the task's wake-up, tens of µs; the numbers stand.

### The feed's tests once `BEGIN IMMEDIATE` is on main

- **Keep `Bed::apply`'s and `burst`'s `unwrap()`** (`tests.rs:43`, `:703`). After the fix, an error from `apply` is a store bug, and the feed's tests should fail loudly on one, as they did here. Don't add retries in the tests.
- **Keep the slow-client test's relative bound and its watched writer handle.** Keep the burst test's 21 too, unless it starts timing batches by when they're sent (a timestamp inside the push would do), which would let it go back to 20.
- **Adapt the corruption test as above.** It also opens a second writer on `jaw.db`. With `BEGIN IMMEDIATE`, the store's writer waits for it under its busy timeout rather than failing, and the side connection's `busy_timeout(5 s)` covers the other way round, so the test stays sound.
- Nothing in the feed's tests depends on the deferred behaviour.

## Measurements

These ran in scratch clones on a Mac Studio (M1 Ultra, 20 cores, macOS 26.6.2) at load averages of 31-50 from other builders, with bundled SQLite 3.53.2. Nothing in `~/src/jaw` or this worktree was changed, apart from this file.

- **Re-audit, `/tmp/jaw-feed-audit2` at 16c152b.** The feed tests passed 3 times in release (all 11). The debug full suite passed 6 times (126 tests), with no "database is locked" in these 6, against the 3 of 10 the builder saw, which the sync audit attributes to I5.
- **`jawd bench feed --far 250 --interfaces N`** (release; 49,900 + 250 seeded messages; 2,000 messages a second for 5 s; 50 lone messages 200 ms apart, then 50 more while 2,000 changes a second go to the 250 far chats). Burst batches were 18.8-19.4 a second per window, at most 20 in any second, in every run; commit to push p99 53-57 ms.

  | interfaces | load | window reads a second (burst / quiet / busy) | lone p50 / p99, quiet | lone p50 / p99, busy | jawd CPU, watched |
  | --- | --- | --- | --- | --- | --- |
  | 1 | 45 | 38 / 10 / 25 | 0.84 / 4.09 ms | 1.10 / 2.21 ms | 83.6% |
  | 4 | 44 | 153 / 39 / 98 | 3.43 / 4.74 ms | 3.64 / 4.61 ms | 88.2% |
  | 8 | 40 | 299 / 77 / 195 | 4.65 / 8.25 ms | 5.41 / 10.84 ms | 124.1% |
  | 4 | 37 | 153 / 39 / 98 | 3.40 / 4.09 ms | 3.95 / 5.27 ms | 72.7% |

- **First pass, `/tmp/jaw-feed-audit` at f04f1aa.** Window reads, process CPU and lone-message delay were measured with counters in `Client::flush`, under 2,000 changes a second to chats outside a top-20 chat list, for 1, 4 and 8 connections. At f04f1aa there were 950-1,950 reads a second with one interface and 3,100-3,200 with four, where lone p99 was 6.2-6.7 ms. With reads paced (the fix now in bb3b264): 30 a second with one, 174-180 with four, lone p99 1.8-2.3 ms with one and 3.3 ms with four. The WAL size was unchanged by reads, 126.9 MB on both branches over 10,000 changes; that growth is the review's D3. The racing property test passed 200 checks, with 4,494 leading-edge reads. Debug full-suite runs split the slowest write: in 8 of 10, 97% or more of it was inside the writer's own transaction.

## Verdict

Nothing marked data loss or inconsistency remains. The first pass's performance finding is fixed and measured, and every rule the change touches has a test that fails without it: the leading edge, read pacing, the failed-read retry and resync, conversations before chat lists, and windows matching fresh queries while writes race reads. What stands, as follow-ups:

- Re-measure lone-message latency with 1, 2, 4 and 8 interfaces on a quiet machine before landing. If 1 or 2 interfaces exceed 5 ms p99 there, this approval no longer holds and the branch needs changes.
- Pace reorder reads on `read`.
- Adapt the corruption test to sync-v1's `json_valid` CHECK at the rebase.
- Clear `failing` in `keep`.

Verdict: approved
