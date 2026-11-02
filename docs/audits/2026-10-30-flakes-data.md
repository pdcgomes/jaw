# Audit: the flaky-test fixes, data half

# Round one: `e80d3855`

Range: `7262a15..e80d3855` in `test-load-flakes-data` (worktree `~/src/jaw-flakes-data`), 30 October 2026. Auditor: database-expert. `7262a15` is `origin/main`. Paths are under `crates/jawd/src/`, and line numbers are at `e80d3855`.

The range is six commits, and nothing is uncommitted but this note:
- `d2f3e169`, two sync tests: the blob test waits for the blob to leave the spool, and the stop's bound is judged on the quickest of three stops;
- `db560e14`, the restart test, with `CONNECTING` and 2 s of its wait left;
- `11530177`, two feed tests: the lone change after quiet judged on its median, and no window starving counted in the others' batches;
- `1ab7be02`, the WAL test with history pouring in, judging live writes on their median;
- `cde183e7`, the outbox's step tests, in release, judged on the median of each step's quickest of three;
- `e80d3855`, the WAL test of a reader on the frames, judged on the quickest of three runs.

What I did, in one clone at `e80d3855` with one build folder, built with `CARGO_INCREMENTAL=0`, everything under `nice`:
- read the range whole against `7262a15`, and the code each test guards;
- ran each changed test 5 times alone, in debug, and the outbox's two in release too;
- ran jawd's whole suite twice, from the built binaries;
- built my own switches, each turning off one rule a changed test guards (some only in part, or in one run of three), read at run time from an environment variable in one debug and one release build. I ran the changed tests against them, alone and four at a time, with prints of what each test measures, so runs could also be judged as main judged them;
- ran the switches a changed test misses against the rest of `sync::` or `store::`, to see whether another test holds the rule.

Everything ran in the tests' own folders, with `JAW_HOME` and `TMPDIR` in `/tmp/jaw-audits/flakes-data`, deleted once this note was written. No Go engine was built, so jawd's one test that starts it skipped itself. I started no jawd outside the tests. The only processes I stopped were three of my own: a module script that, after saying it had finished, started a module run again (the doubled run agents have seen), stopped by PID along with its test binary.

My switches, scripts and each run's results are kept in this auditor's store, `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/reports/flakes-data-audit-r1-files/`. `switches.diff` applies to `e80d3855`, and `results.tsv` has a line for every run.

## In short

Changes needed, for one test gap. This range changes test code only, so nothing in it can lose data or leave jaw.db inconsistent. Six of the nine changed tests still catch what they guard and judge it more steadily. Of the other three:

- **T2, which must change. The outbox's two step tests hold only half the steps to the 3 ms a background transaction may take.**
  - A 4 ms stall in the 32 of 288 steps that finish a message's letting go passed both tests, 3 of 3 each. One in a third of the 1,031 regrouping steps passed 3 of 3.
  - Nothing else measures these steps' 3 ms.
  - Each step's quickest of three stayed under 1.74 ms in all 22 runs without a switch, alone and four at a time. So judging every step at its quickest, rather than the median, is steady on today's main.
- **T1, a follow-up, best fixed in the same round. The WAL test with history pouring in passes with its rule switched off.**
  - With live writes taken only once the history is in, it passed 3 of 10 runs alone at a load of 54 to 56. The history now takes about as long as the test's 1 s bound (0.98 to 1.14 s). Main's check has the same margin, and the builder recorded it.
  - The rule itself is still held, without a clock: `live_changes_go_ahead_of_a_background_batch` failed 5 of 5 with the same switch.

The rest:
- **Nothing outside tests changed.** Every hunk is inside a `#[cfg(test)]` module.
- **Every other switch fails its test 3 of 3:**
  - the stop;
  - the blob kept in the spool;
  - a restart giving no time, or a fresh wait;
  - a lone change waiting to coalesce;
  - windows starving;
  - the writer waiting for readers its whole busy timeout;
  - a stall after the reader lets go;
  - the regroup plan's bound;
  - a 4 ms stall in every step.
- **The quickest-of-three judgements** (the stop, the reader on the frames) miss a slowdown in one run of three, which I showed (O1). Neither bound is one of the design's budgets, and the checks that matter for the data run on every run.
- **Outside this range:** nothing in `sync::` checks that a blob stays in the spool until its drain's synced commit (O2).
- **Steadiness:**
  - none of the nine failed in 45 runs alone, or in two whole suites at a load of 64 to 81;
  - suite 2 failed five other tests, each a timing bound in code this range doesn't touch, and each passed 3 of 3 alone.

## What was asked, checked

### Nothing outside tests changed

The range changes five files:
- `feed/tests.rs`, `send/tests.rs` and `sync/tests.rs` are modules declared `#[cfg(test)] mod tests;` (`feed/mod.rs:13`, `send/mod.rs:30`, `sync/mod.rs:782`).
- In `store/mod.rs`, every hunk is past line 3,050. That's inside the `#[cfg(test)] mod tests` that begins at line 1,852 and runs to the file's end (4,247).
- In `store/write/outbox.rs`, every hunk is past line 1,974, inside the one that begins at 1,316 and runs to its end (3,034).

No SQL, schema, migration, PRAGMA, transaction boundary, ack or file the product writes changed, so no plan, durability or consistency rule can have moved.

### Each changed test: does it still catch what it guards, and does it hold the design's number?

**The stop** (`sync/tests.rs:1084`, `d2f3e169`).
- **Rule:** a deaf engine is ended after `DISCONNECTING` (2 s, `sync/mod.rs:625`), and the clean marker stays "0".
- **Now:** three stops, each on a store of its own. The engine's end and the marker are checked on every one, and the clock on the quickest.
- **Switched off** (the deaf engine given twice `DISCONNECTING`): fails 3 of 3, every stop 4.00 to 4.10 s against 2.5 s.
- **With the rule:** 9 stops of 2.005 to 2.021 s.
- **Weaker:** a stop slow in one run of three passes (O1). The bound isn't a design budget. jawd gives itself 3 s to stop (`STOPPING`, `main.rs:438`), and past it gives up, so the marker stays "0" and the next start checks the file. Nothing is lost either way, and the marker is now checked three times.

**The blob in the spool** (`sync/tests.rs:1476`, `d2f3e169`).
- **Rule:** a blob left by a run that stopped before its drain is used again without storing anything twice, and leaves the spool once its drain commits.
- **Now:** the test waits for the blob to go. Before, it stopped the fetch as soon as the inbox emptied, which could cancel the removal that follows the drain's synced commit (`sync/fetch.rs:214-222`).
- **Switched off** (the blob kept after its drain): fails 3 of 3, at the new wait.
- **What it never checked:** with the blob removed before the drain's synced commit, against the design's "deleted only after a synced commit stops pointing to it", it passes 3 of 3, as main's did (O2, outside this range).

**The restart** (`send/tests.rs:697`, `db560e14`).
- **Rule:** after a restart, a message waiting for a receipt waits only the time it had left (`send/outbox.rs:347-358`).
- **Now:** it has `CONNECTING` and 2 s left (12 s), where it had 1.5 s. A loaded start, waiting up to 10 s for the demo engine, could use up 1.5 s.
- **Switched off:** with no time left it fails 3 of 3, failing 11.96 s early. With a fresh wait it fails 3 of 3, never failing within 30 s.
- **With the rule:** it failed 5 to 15 ms after its time. The checks are main's (not early, and under 10 s late), so nothing is weaker.

**The lone change** (`feed/tests.rs:711`, `11530177`).
- **Rule:** a lone change after quiet goes at once, rather than waiting to coalesce (`feed/client.rs:672-680`).
- **Now:** judged on the median of ten pushes, under the same 50 ms, against waits of 100 ms.
- **Switched off** (a lone change waits to coalesce): fails 3 of 3, every push 101 to 107 ms.
- **With the rule:** pushes of 0.7 to 6.4 ms.
- **Weaker, but measured elsewhere:** a lone change that waited in four pushes of ten would pass. The design's number, 5 ms at p99 from commit to interface, is measured by `make perf` on release builds (`scripts/perf.py:473-477`: the steady step's lone messages, and `jawd bench feed`'s lone messages after a burst). So this is acceptable.

**No window starving** (`feed/tests.rs:3402`, `11530177`).
- **Rule:** changes are heard only once no flush is due, so the windows take turns (`feed/client.rs:969-985`).
- **Now:** counted in the batches the other six windows had between two of a window's own, at most 24, where it allowed 500 ms between batches. A stall of the machine holds every window alike, so it can't stretch the count.
- **Switched off** (changes heard before a due flush): fails 3 of 3. Three windows got no batch, or one, while the others had 181 to 198.
- **With the rule:** 7 to 13, the chat list's 12 to 13.
- There's no design number for it, and the count measures the rule more directly than the clock did.

**The WAL with history pouring in** (`store/mod.rs:3041`, `1ab7be02`).
- **Rule:** live writes go in between background transactions and while background work waits for room (`store/mod.rs:1711`, `:1460`), so a history never holds one back.
- **Now:** judged on the median of the live writes, under the same 1 s, where each one was.
- **Switched off,** it doesn't reliably fail (T1). Its WAL-bound checks are main's, unchanged.

**The reader on the WAL's frames** (`store/mod.rs:3243`, `e80d3855`).
- **Rule:** a reader holding the WAL's frames holds acks up only a moment, and the WAL starts again once it lets go.
- **Now:** three runs, each on a store of its own and each checked whole. The slowest ack is judged on the quickest run, against the same 2 s and `RESTART_MOST`.
- **Switched off:**
  - with the writer waiting for readers its whole busy timeout, it fails 3 of 3 on the run's own check ("the reader never held the WAL past its bound", 1,631 to 1,759 frames);
  - with a 3.1 s stall in the first restart after the reader lets go, it fails 3 of 3 on the clock, the slowest acks 3.13 to 3.14 s.
- **With the rule:** the slowest ack of a run was 46 to 326 ms.
- **Weaker:** a stall in one run of three passes (O1). The design's 10 ms p99 for acks is measured by `make perf` in its steady, history, catch-up and flood steps (`scripts/perf.py:448-462`), not behind a long reader. There the design lets acks wait "a moment", so this test's 3 s is a bound of its own.

**The outbox's steps** (`store/write/outbox.rs:1976-2060`, `cde183e7`).
- **Rule:** what each step of regrouping and letting go may do is bounded, and checked exactly on every run. In a release build, each step also stays within the 3 ms a background transaction may take, commit included (the design's "Storage", `BATCH_TIME` at `store/mod.rs:287`).
- **Now:** the release check judges the median of each step's quickest of three. Main allowed one step in thirty past 3 ms and none past 15 ms.
- **Switched off:**
  - with the regroup plan's bound off, both fail 3 of 3 on the exact check, in debug;
  - with a 4 ms stall in every step, both fail 3 of 3 in release, the median 4.8 to 6.1 ms.
- **With part of the rule off,** both pass (T2).

## Findings

### Data loss

None. The range changes test code only.

### Inconsistency

None.

### Test gaps

**T2. The outbox's step tests hold only half the steps to 3 ms.** A test gap, hiding unpredictable performance; it must change before this lands.
- **Where:** `store/write/outbox.rs:1976-2002`, `in_a_thousand`, which both tests use.
- **What happens:** judged on the median, up to half of a test's steps may pass 3 ms at their quickest, in every run, and it still passes.
  - With a 4 ms stall only in the 32 steps of 288 that finish a message's letting go (where `LET_GO_DONE` runs, `store/write/outbox.rs:411`), both tests passed in release, 3 of 3 each. Those 32 steps took 4.2 to 5.3 ms at their quickest, and the median was 0.40 ms.
  - With a 4 ms stall in a third of the regrouping steps (by member), the second test passed 3 of 3. 333 of its 1,031 steps were over 3 ms at their quickest, and the median was 0.14 to 0.16 ms.
  - Main's own rule (one step in thirty past 3 ms, none past 15 ms), applied to the same quickest of three, fails all nine of those runs.
  - So a regression that reaches only some steps would pass: a statement, trigger or plan that only the last step of each message runs, or only some members' steps.
- **Against:** the design's "background batches capped at 1,000 rows or 3 ms per transaction, commit included", principle 2's "one short transaction", and the expert's 3 ms for background transactions. That's a cap on every transaction, not a median. Nothing else measures it for these steps:
  - `make perf` has no row for background transactions;
  - the WAL-bound audit's P3 (commits taking background transactions past 3 ms under load) was measured once, by hand;
  - this check runs only in release, which neither `make test` nor CI builds (O3).
- **What the median was for:** the test's comment and the commit say a commit that meets the checkpointer's pass waits for its syncs, at the same steps each run, so no quickest of three escapes them. That was measured on 27 October, when the checkpointer's syncs were full flushes. On today's main, which fences them with a barrier on the Mac's own drive, no step stayed slow:
  - in 22 runs without a switch, 6 alone at a load of 52 to 54 and 16 four at a time at 59 to 64, every step's quickest of three was under 1.74 ms;
  - in the switched runs at a load of 65 to 79, every step of a kind the switch didn't touch stayed under 1.9 ms at its quickest;
  - main's rule applied to each run as it came would have failed 1 of 18 runs alone (7 regrouping and 14 letting-go steps over 3 ms, the slowest 30 and 49 ms), and none of 48 four at a time.

  So the stalls are other processes', which the quickest of three removes, as the commit intends.
- **Fix:** keep the three runs and each step's quickest, and judge every step rather than the median:
  - main's allowance on the quickest of three (at most one step in thirty past 3 ms, none past 15 ms). It catches both partial stalls above, and passed all 22 runs here. Every step under 3 ms passed them all too, if you'd rather hold the design's number exactly.
  - Where a drive brings the checkpointer's slow syncs back (they're full flushes off the Mac's own drive), mark the steps whose commit met its pass or started the WAL again, and allow only those up to 15 ms. The test can tell those steps: `store.wal`'s frames fall, or `storage().background_waits` rises, across one.
  - Say so in the comment, which now describes the checkpointer as it was before the barriers.
  - Then show the 32-step stall fails.

**T1. The WAL test with history pouring in passes with its rule switched off.** A test gap; a follow-up, since the rule is held elsewhere, best fixed in the same round.
- **Where:** `store/mod.rs:3041-3091`, `the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it`.
- **What happens:** I took live writes only once the history is in (no `between()` in `background_changes`, `store/mod.rs:1741`, and no live arm while background work waits for room, `:1470`). The test passed 3 of 10 runs alone, at a load of 54 to 56.
  - Each such run made two live writes. The first went before the history began (1 to 4 ms), and the second waited for all of it (963 to 1,127 ms), while the history took 0.98 to 1.14 s.
  - With two writes the median is the slower one, so the run fails only when the history takes longer than the 1 s bound.
  - Four at a time, the history took 1.31 to 1.42 s, and it failed 8 of 8. On a quieter or quicker Mac it would pass more often.
  - Main's per-write check has the same margin (it would have passed those three runs too), and the builder's report records it as a follow-up. But the commit says the test "fails 3 of 3" with this switch, and at today's load it doesn't.
  - With `between()` alone removed, live writes waited 17 to 19 ms at the median, against 0.8 to 1.1 ms with the rule, and the test passed 10 of 10, as main's would.
  - With the rule, 111 to 118 live writes went in alone, and 157 to 182 four at a time. Their median was 0.8 to 1.6 ms, and the slowest 2.1 to 34 ms.
  - The median also dropped any bound on the slowest live write. A single one could now wait seconds behind quick ones.
- **Held elsewhere:**
  - `store::tests::live_changes_go_ahead_of_a_background_batch` (`store/mod.rs:2370`) checks that a live write returns before a 20,000-message batch has finished, with no clock. It failed 5 of 5 with the switch above.
  - `background_work_waiting_for_the_wal_has_it_started_again_at_the_next_live_write_past_wal_soft` failed 3 of 3 with the live arm removed.
  - `between()` alone is held only loosely: the first failed 2 of 3 alone with it removed, and passed in a run of all 179 `store::` tests. Nothing else in `store::` failed.
  - The design's 10 ms p99 for a live message while history goes in is measured by `make perf`'s history step, in release, at the production WAL bound.
- **Fix:** judge what the history's speed can't hide:
  - live writes kept going in while the history did, for example at least 20 of them. With the rule there were 111 to 182; with live writes waiting for the history there are 2, however quickly it goes.
  - the slowest under what a legitimate wait allows: a try at the edge waits up to `RESTART_MOST` and then two syncs, so 3 s with room for load.
  - optionally, the median under a bound nearer what live writes take, say 10 ms, if that holds through whole suites at a load of 80. That would catch `between()` removed, which nothing holds reliably now.

### Unpredictable performance

None in the product: the range doesn't change it.

### Other (follow-ups; none blocks)

**O1. The quickest-of-three judgements miss a slowdown in one run of three.**
- **The stop:** with the deaf engine given twice `DISCONNECTING` in the first stop only, the stops took 4.0, 2.0 and 2.0 s, and the test passed 3 of 3.
- **The reader on the frames:** with the 3.1 s stall in the first run only, the runs' slowest acks were 3.13 s and about 50 to 360 ms, and it passed 3 of 3.
- **Why it's acceptable:** a regression that slows every run is caught (above), and the checks that matter for the data run on every run. For the stop, that's the engine ended and the marker at "0". For the reader, it's the WAL held past its bound, the tries spaced, the WAL started again, and two flushes at most. Neither clock is one of the design's budgets.
- **Follow-up:** say in each comment that a slowdown in one run of three passes.

**O2. Nothing checks that a blob stays in the spool until its drain's synced commit (outside this range).**
- With the blob removed before `apply_durable` (`sync/fetch.rs:215`), the blob test passes 3 of 3, and so do all 80 of `sync::`'s tests.
- The code's order is right today (the removal is at `:222`, after the synced commit), and the 16 October sync audit checked it by hand.
- **Follow-up:** a test that fails the drain's commit, or stops before it, and finds the blob still in the spool.

**O3. The outbox's clock check runs in release only, which neither `make test` nor CI builds.** The builder's report notes it. So even once T2 is fixed, the design's 3 ms for these steps is measured only when someone runs these tests in release.
- **Follow-up:** run the two tests in release from `make perf`, or give the check a bound a debug build can keep.

## Steadiness

- **The nine changed tests** passed 5 of 5 alone each, and the outbox's two 5 of 5 in release too. None failed in either whole suite, at a load of 45 to 81 (tables below).
- **What they steady, of today's usual flakes:**
  - the stop (the budgets audit: once in 8 sync runs);
  - the restart test (the failure-states audit's load flakes);
  - the WAL test with history pouring in (the WAL-bound audit's H8: a live write past 1 s);
  - the outbox's steps in release;
  - the lone change and the starving windows;
  - the reader on the frames;
  - the blob test's race with the fetch's abort.

  Each now judges what other processes' load can't fake, or waits for what it checks.
- **How much this sample shows:** at today's loads, main's judgements mostly passed too, computed on the same measurements. The outbox's, as each step came, failed 1 of 18 runs alone. The stop's, the lone change's, the reader's and the history's never did. So two suites and 45 runs show none of the nine failing, but they can't show a rate. The evidence that the changes steady these tests is each one's mechanism, plus the builder's 20-of-20 runs.
- **What still fails, untouched by this range:** suite 2's five, each a timing bound, each passing 3 of 3 alone:
  - the long-request lane (`feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`);
  - the catch-up's one summary (`notify::tests::a_catch_up_waits_and_ends_in_one_summary`);
  - a page too slow for its preview (`send::link::tests::sending::a_page_too_slow_leaves_its_text_to_go_without_a_preview_and_the_next_message_after_it`);
  - the writer-fairness test (`store::tests::live_writes_go_in_with_a_stream_of_inbox_rows_and_are_never_held_back_by_it`, the budgets audit's too);
  - a full connection's answer (`sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`).

  The builder's other untouched flakes passed both my suites: settings over the socket, the feed's other pacing tests, the checkpointer's flush tests, the WAL-bound tests with inbox rows or live writes coming, and jawd stopping while messages pour in.

## Measurements

**Method.**
- **The clone:** one clone of the worktree at `e80d3855` in `/tmp/jaw-audits/flakes-data`, its `target` the one build folder, built with `CARGO_INCREMENTAL=0` under `nice -n 10`. First the branch's tests in debug and release, then my switches and prints in debug and release, each binary copied out before the next build.
- **Each run:** the test binary itself, under `nice -n 10`, from `crates/jawd` as cargo runs it, with `JAW_HOME` and `TMPDIR` in the scratch folder, bounded by a timeout that kills the binary. One test process at a time, unless it says four at a time.
- **Load:** the 1-minute load average at each run's start, on 20 cores, beside other agents' work.
- **The switches** were read from `JAW_AUDIT_MUTANTS`. With none set, a switch build runs the branch's code.

**Each changed test alone, on the branch's own binaries.**

| Test | Runs | Took | Load |
| --- | --- | --- | --- |
| `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean` | 5 of 5 | 11.5 to 16.1 s | 68 to 77 |
| `sync::tests::a_restart_before_a_blobs_drain_uses_the_blob_in_the_spool_again_without_storing_anything_twice` | 5 of 5 | 0.7 to 3.3 s | 75 to 77 |
| `send::tests::a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left` | 5 of 5 | 12.9 to 13.6 s | 69 to 75 |
| `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce` | 5 of 5 | 2.5 to 2.8 s | 65 to 68 |
| `feed::tests::no_window_starves_while_the_others_keep_falling_due` | 5 of 5 | 2.2 to 2.5 s | 63 to 65 |
| `store::tests::the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it` | 5 of 5 | 1.7 to 1.8 s | 58 to 63 |
| `store::tests::a_reader_on_the_wals_frames_holds_acks_up_only_a_moment_and_the_wal_starts_again_once_it_lets_go` | 5 of 5 | 21.5 to 23.6 s | 50 to 58 |
| `store::write::outbox::tests::in_a_group_of_a_thousand_a_step_of_regrouping_looks_at_one_message_so_it_stays_short` | debug 5 of 5, release 5 of 5 | 1.6 to 2.2 s; release 4.3 to 6.7 s | 47 to 52 |
| `store::write::outbox::tests::when_most_of_a_thousand_leave_steps_are_sized_by_who_was_sent_to_and_skip_what_is_read` | debug 5 of 5, release 5 of 5 | 2.3 to 7.2 s; release 5.8 to 76.8 s | 45 to 64 |

Three of the release runs of the second outbox test took 37 to 77 s, against 6 s, as the load rose to 70, and passed. Each step's quickest of three removes the stalls that make such a run long.

**jawd's whole suite, twice,** from the built binaries as `cargo test -p jawd` runs them (the unit tests, then the four integration tests), one after the other, with nothing else of mine running:

| Suite | Load | Unit tests | Integration tests |
| --- | --- | --- | --- |
| 1 | 69 before (64 over 5 minutes), 80 after | 627 passed, 12 ignored, 421 s | 6 of 6 |
| 2 | 81 before (74 over 5 minutes), 57 after | 622 passed, 5 failed (above), 12 ignored, 477 s | 6 of 6 |

What suite 2's failures said:
- the lane and the watch waited for the long requests;
- a notice came before the catch-up's summary;
- answered in 1.96 s;
- a live write waited 316 ms for the stream of rows;
- `Elapsed`, waiting for an answer.

Each passed 3 of 3 alone at a load of 53 to 55.

**The switches.** Debug unless it says release; each run alone unless it says four at a time.

| Switch | Test | Result | What it measured |
| --- | --- | --- | --- |
| deaf engine given twice `DISCONNECTING` | the stop | fails 3 of 3 | stops of 4.00 to 4.10 s |
| the same, in the first stop only | the stop | passes 3 of 3 (O1) | 4.0, 2.0, 2.0 s |
| blob kept after its drain | the blob | fails 3 of 3 | at the wait for it to go |
| blob removed before the drain's commit | the blob; all of `sync::` | passes 3 of 3; 80 of 80 pass (O2) | |
| no time left after a restart | the restart | fails 3 of 3 | 11.96 s early |
| a fresh wait after a restart | the restart | fails 3 of 3 | not failed within 30 s |
| a lone change waits to coalesce | the lone change | fails 3 of 3 | pushes of 101 to 107 ms |
| changes heard before a due flush | no window starving | fails 3 of 3 | three windows with 0 or 1 batch, 181 to 198 for the others |
| live writes only once the history is in | the WAL with history | fails 7 of 10, **passes 3** (T1); four at a time fails 8 of 8 | 2 live writes, the second 963 to 1,127 ms alone, 1.30 to 1.40 s four at a time |
| the same | `live_changes_go_ahead_of_a_background_batch` | fails 5 of 5 | |
| no `between()` | the WAL with history | passes 10 of 10 (T1) | 41 to 46 live writes, 17 to 19 ms at the median |
| the same | `live_changes_go_ahead_of_a_background_batch`; all of `store::` | fails 2 of 3; 179 of 179 pass | |
| no live arm while waiting for room | the WAL with history | passes 3 of 3 | 109 to 121 live writes, 1.0 to 1.3 ms at the median, 25 ms at most |
| the same | `background_work_waiting_for_the_wal_has_it_started_again_at_the_next_live_write_past_wal_soft` | fails 3 of 3 | |
| readers waited for the whole busy timeout | the reader on the frames | fails 3 of 3 | "never held the WAL past its bound", 1,631 to 1,759 frames |
| 3.1 s stall after the reader lets go | the reader on the frames | fails 3 of 3 | slowest acks 3.13 to 3.14 s |
| the same, in the first run only | the reader on the frames | passes 3 of 3 (O1) | 3.13 s, then about 50 to 360 ms |
| the regroup plan's bound off | both outbox tests | fails 3 of 3 each | the exact check |
| 4 ms in every step, release | both outbox tests | fails 3 of 3 each | medians of 4.8 to 6.1 ms |
| 4 ms in the 32 steps that run `LET_GO_DONE`, release | both outbox tests | **passes 3 of 3 each** (T2) | those steps 4.2 to 5.3 ms at their quickest, the median 0.40 ms |
| 4 ms in a third of the regrouping steps, release | the leaving test | **passes 3 of 3** (T2) | 333 of 1,031 over 3 ms at their quickest, the median 0.14 to 0.16 ms |

The whole-module runs used 4 test threads. With live writes taken only once the history is in, `store::` failed `live_changes_go_ahead_of_a_background_batch`, the WAL test and the WAL-restart test, and then `background_work_behind_a_reader_that_held_restarts_up_gets_room_once_it_lets_go_with_nothing_committing` waited until the 15-minute bound ended the run.

**What the tests measure with every rule on,** from the switch builds with none set:

| Test | Runs, load | Measured | Its bound |
| --- | --- | --- | --- |
| the stop | 3, 53 to 57 | each of 9 stops 2.005 to 2.021 s | the quickest under 2.5 s |
| the restart | 3, 49 to 53 | failed 5 to 15 ms after its time | not early, under 10 s late |
| the lone change | 5, 53 to 54 | pushes 0.7 to 6.4 ms, the medians about 1 ms | the median under 50 ms |
| no window starving | 5, 51 | at most 7 to 13 of the others' batches between a window's own | 24 |
| the WAL with history | 10, 55 to 56 | 111 to 118 live writes, the median 0.80 to 1.10 ms, the slowest 2.1 to 3.7 ms; the history 0.87 to 0.98 s | the median under 1 s |
| the same, four at a time | 8, 59 | 157 to 182 live writes, the median 1.36 to 1.62 ms, the slowest 4.1 to 34 ms; the history 1.31 to 1.72 s | |
| the reader on the frames | 3, 48 to 51 | the slowest ack of each run 46 to 326 ms | the quickest run's under 3 s |
| the outbox's steps, release | 6 alone at 52 to 54, 16 four at a time at 59 to 64 | every step's quickest of three under 1.74 ms, the medians 0.08 to 0.79 ms. As each step came, main's rule failed 1 of 18 runs alone (14 of 288 over 3 ms, one at 49 ms) and 0 of 48 four at a time | the median of the quickest under 3 ms |

## What must change

1. **T2:** the outbox's step tests judge every step at its quickest of three, against main's allowance or 3 ms each, not the median, and show the 32-step stall failing.
2. **T1, in the same round, though it doesn't block on its own:** the WAL test with history pouring in judges what the history's speed can't hide (live writes kept going in, and the slowest under what a wait at the edge allows), and shows the switch failing alone on a quiet machine.

Then a fresh round of this audit, on the two tests and their switches.

## Verdict

Verdict: changes needed

# Round two: `8a854fa6`

Range: `0dd286c9..8a854fa6` in `test-load-flakes-data` (worktree `~/src/jaw-flakes-data`), 2 November 2026. Auditor: database-expert. `0dd286c9` is `origin/main`. Paths are under `crates/jawd/src/`, and line numbers are at `8a854fa6`.

The range is ten commits, and nothing is uncommitted but this note:
- the six of round one, rebased onto `0dd286c9`. `git range-diff` against round one's `7262a15..e80d3855` shows five identical, and the restart test's differing only in a context line, where main added `typing` to the send tests' `Running`;
- `d5964145`, T2: the outbox's step tests judge every step at its quickest of three against main's allowance;
- `6fc21f2e`, T1: the WAL test with history pouring in counts the live writes that went in while it did, and bounds the slowest;
- `87f387ad`, O2: a new test that a blob stays in the spool while its drain can't commit;
- `8a854fa6`, O3: `make perf` runs the outbox's step tests in release and makes a row of them.

What I did, in one clone at `8a854fa6` with one build folder, built with `CARGO_INCREMENTAL=0`, everything under `nice`:
- read the range whole against `0dd286c9`, and the code each test guards, on today's main;
- ran jawd's whole workspace suite once, on the head's own binaries, and its three failures alone;
- built my own switches again, on today's code: 17 of them, each turning off one rule a test guards (some only in part), read at run time from an environment variable in one debug and one release build, with prints of what each test measures. I ran each test against the switches for its rules, alone;
- ran the changed and new tests again alone on the head's binaries, and all ten together four at a time;
- ran `make perf`'s new step as the Makefile and `scripts/perf.py` run it, and drew its row with `perf.py`'s own `render()`.

Everything ran in the tests' own folders, with `JAW_HOME` and `TMPDIR` in `/tmp/jaw-audits/flakes-data-r2`, deleted once this note was written. No Go engine was built (the range has no Go), so the one test that starts it skipped itself, and Go's tests weren't run. I started no jawd outside the tests, and stopped no process. My switches, scripts and each run's results are kept in this auditor's store, `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/reports/flakes-data-audit-r2-files/`.

## In short

Approved. The range changes test code and `make perf` only, so nothing in it can lose data or leave jaw.db inconsistent. Round one's four items are done as it meant them:
- **T2:** every step is now judged at its quickest of three, against one in thirty at 3 ms and none at 15 ms. Both partial stalls round one found pass no longer: a 4 ms stall in the 32 steps that finish letting go, and one in a third of the regrouping steps, each fail 3 of 3.
- **T1:** live writes taken only once the history is in fail 3 of 3 alone on a quiet machine, on the new count, however quickly the history goes.
- **O2:** the new test fails 3 of 3 with the blob removed before its drain's synced commit, which nothing caught before, and 3 of 3 with it kept after.
- **O3:** `make perf` runs the two tests in release. Its row holds every step to the design's 3 ms, stricter than the tests' allowance, and passed three runs on the head.

Every steadied test still fails with its rule off, 3 of 3. The full suite at the head failed only three tests, all on the landers' list and outside this range, each passing alone 3 of 3. Two follow-ups, neither blocking:
- **T3:** the WAL test with history passes a live write that waits for the last second of the history once 20 have gone in. Main's 1 s bound caught that only by the margin that made it flaky. `make perf` measures it.
- **O4:** `make perf`'s report prints no load for its new row and the rows after it.

## What was asked, checked

### T2, T1, O2 and O3, as round one meant them

**T2** (`store/write/outbox.rs:2325-2365`, `d5964145`).
- **Now:** in a release build, every step is judged at its quickest of three runs. At most 1 + n/30 steps may take 3 ms or more, and none 15 ms. That's 2 of the first test's 32 regrouping steps, 10 of each test's 288 letting-go steps, and 35 of the second test's 1,031 regrouping steps. Each run is still checked exactly in `steps_in_a_thousand`, and a debug build runs once and doesn't time the steps. The comment now describes the checkpointer as it is, with barriers fencing its syncs on the Mac's own drive.
- **Switched off, in release:**
  - a 4 ms stall in the 32 steps that run `LET_GO_DONE` fails both tests 3 of 3: 32 steps at 3 ms or more where 10 are allowed. Those steps took 4.2 to 6.2 ms at their quickest, and the median was 0.48 to 0.53 ms;
  - a 4 ms stall in a third of the regrouping steps fails the leaving test 3 of 3: 333 of 1,031 where 35 are allowed, the median 0.14 to 0.17 ms;
  - 4 ms in every step fails both 3 of 3, the medians 4.7 to 5.8 ms;
  - with the regroup plan's bound off, both fail 3 of 3 in debug, on the exact check.
- **With the rule:** every step's quickest stayed at or under 1.74 ms, and none reached 3 ms. That held in the switch build with no switch set, and in three runs of `make perf`'s step on the head.
- **As round one meant:** yes. Round one's other option, allowing up to 15 ms only to the steps whose commit met the checkpointer's pass, for drives without barriers, wasn't taken. The comment says the allowance holds on the Mac's own drive. The tests and `make perf` put their stores in `TMPDIR`, on that drive.

**T1** (`store/mod.rs:3181-3222`, `6fc21f2e`).
- **Now:** at least 20 live writes must go in while the history does (`:3208`). The median must stay under 1 s, and the slowest under `RESTART_MOST` plus 2 s, which is 3 s (`:3210`).
- **Switched off:**
  - with live writes taken only once the history is in, it fails 3 of 3 alone at a load of 35, on the count. There were 2 live writes, the second waiting 1.31 to 1.42 s, and the history took 1.33 to 1.44 s;
  - a 3.1 s stall in one live write fails 3 of 3 on the slowest, with 143 to 145 live writes, the slowest 3.10 to 3.11 s.
- **With the rule:** 142 to 148 live writes, 1.27 to 1.29 ms at the median and 3.2 to 5.2 ms at most, in debug. In release, 90 to 95, with the history taking 0.68 to 0.72 s, so the count of 20 has room on a quicker build too.
- **As round one meant:** yes, both of its required parts. Its optional median under 10 ms was left out, as the fix report says, because load could push past it. What the test still passes is T3.

**O2** (`sync/tests.rs:1710-1743`, `87f387ad`).
- **Now:** another connection holds jaw.db's write lock, so the drain's synced commit is refused as busy.
  - The writer tries it again until it can (`retrying`, `store/mod.rs:1302`), as it would with a full disk: both errors are transient (`store/write.rs:585`).
  - The test waits for `trouble`, then finds the blob and its notification still there. It lets the lock go, waits for the drain and then for the blob to leave the spool, and checks that `history_kept` holds what the blob carried and that the engine was asked for it once.
- **What the test waits on:**
  - Only that retry loop raises `trouble` (`failing`, `store/mod.rs:1267`). The search indexer met the lock first in 10 of my 11 runs, and it retries on its own without raising it (`:1220`). So the test wakes only once the drain's commit has been refused.
  - That's also why it takes 11 s here: two busy timeouts of 5 s, or one when the indexer doesn't meet the lock (the drain's took 5.3 s in the other run), as the fix round's 5.6 to 11.1 s shows.
- **Switched off:**
  - with the blob removed before the drain's commit, it fails 3 of 3, at "the blob stays while its drain can't commit". The old blob test still passes this switch, 2 of 2, as round one found;
  - with the blob kept after its drain, it fails 3 of 3, at the wait for the blob to go.
- **What it relies on:** it reaches the drain's commit with the lock held only because a blob of settings hands nothing to the background lane first, as its comment says. The first switch failing at its check shows that holds today.
- **As round one meant:** yes.

**O3** (`Makefile:62`, `scripts/perf.py`, and the outbox test's printed lines; `8a854fa6`).
- **Now:** `make perf` builds jawd's tests in release. A `background` step, which `--quick` leaves out, runs the two tests and reads each kind of step's line. From those it makes the row "A background transaction: 3 ms, commit included", met only when no step at its quickest takes 3 ms. That's the design's cap exactly, where the tests allow one step in thirty.
- **Run:** I ran the Makefile's build line, then the step's exact command three times on the head, and parsed and drew each with `perf.py`'s own regular expression and `render()`. Each gave "0 of 1,639 at 3 ms or more", 1.56 to 1.74 ms at most, met.
- **One step at 10 ms in every run,** which the tests allow, gave "11.7 ms at most, 1 of 1,639 at 3 ms or more", missed. So `make perf` holds what the tests' allowance lets through.
- **As round one meant:** yes. O4 is a slip in its report.

### Each steadied test still catches its rule

Each switch below turns off the rule on today's code. Each ran 3 times, alone, in debug, at a load of 28 to 43. Each test failed 3 of 3:

| Test | Switch | What it measured |
| --- | --- | --- |
| the stop (`sync/tests.rs:1234`) | the deaf engine given twice `DISCONNECTING` | every stop 4.003 to 4.007 s |
| the blob in the spool (`sync/tests.rs:1685`) | the blob kept after its drain | at the wait for it to go |
| the restart (`send/tests.rs:710`) | no time left after a restart | failing 11.83 to 11.91 s early |
| the same | a fresh wait | "failed never happened" |
| the lone change (`feed/tests.rs:831`) | a lone change waits to coalesce | every push 101 to 105 ms |
| no window starving (`feed/tests.rs:3522`) | changes heard before a due flush | three windows got no batch while the others had 233 to 234 |
| the reader on the frames (`store/mod.rs:3383`) | the writer waits for readers its whole busy timeout | "never held the WAL past its bound", 1,752 to 2,040 frames |
| the same | a 3.1 s stall in the first restart after the reader lets go | the slowest acks 3.12 to 3.14 s, in every run |

The changed and new checks, above, fail too.

### No test was made toothless

- **One step in thirty at 3 ms:**
  - Every kind of step the two outbox tests time comes 32 times or more in a test, against allowances of 2, 10 and 35.
  - The second test's 32 regrouping steps that each look at one message sent to 1,024 fit within its 35. But they're the kind the first test has 32 of, where 2 are allowed.
  - So a regression in any kind of step fails one of the two tests.
  - What passes is up to 10 letting-go steps, 35 of the second test's regrouping steps or 2 of the first's, each between 3 and 15 ms. `make perf`'s row marks even one missed, as shown above.
- **None at 15 ms:** a plain assert on the slowest step at its quickest.
- **The 3 s bound:** it fails a live write held 3.1 s, but not one held for the last second of the history (T3). What fails the regression the test exists for, live writes waiting for the history, is the count, however quickly the history goes. Main's bound passed that once the history took under 1 s (round one's T1).
- **Round one's judgements, unchanged:** the median of ten pushes, the count of the others' batches, and the quickest of three stops and runs. Round one judged them, and O1 stands.

### Tests only

- `feed/tests.rs`, `send/tests.rs` and `sync/tests.rs` are modules declared `#[cfg(test)] mod tests;` (`feed/mod.rs:16`, `send/mod.rs:37`, `sync/mod.rs:869`).
- `store/mod.rs`'s hunks are past line 3,190, inside the `#[cfg(test)] mod tests` that runs from line 1,992 to the file's end (4,439).
- `store/write/outbox.rs`'s are past line 2,320, inside the one from 1,612 to its end (3,978).
- The rest is the Makefile's perf target, one line, and `scripts/perf.py`, which runs with `JAW_HOME` and `TMPDIR` in a folder of its own.

No SQL, schema, migration, PRAGMA, transaction boundary, ack or file the product writes changed.

### The full suite at the head

`cargo test --workspace --no-fail-fast`, from 14:42 to 15:43, at a load of 31 at the start, up to 83 while it ran, and 37 at the end:
- every target passed but jawd's unit tests, which had 1,016 passed, 3 failed and 19 ignored, in 2,546 s;
- jawd's `installed`, `starting`, `stopping` and `upgrading` targets passed, and no fake engine was left running;
- all ten of this range's tests passed in it.

The three failures are on the landers' list, in code this range doesn't touch, and each passed alone 3 of 3 at a load of 35 to 39:
- the feed's half-closed connection (`a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`);
- the feed's lane (`a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`, "the lane and the watch waited for the long requests");
- the socket's connection-full test ("nothing went missing: Elapsed").

So the suite passes under the flakes rule.

## Findings

### Data loss

None. The range changes test code and `make perf` only.

### Inconsistency

None.

### Test gaps

**T3. The WAL test with history pouring in passes a live write that waits for the last second of the history once 20 have gone in.** A test gap, and a follow-up: the rule failing whole is caught, and `make perf` measures the budget.
- **Where:** `store/mod.rs:3181-3222`, the count at `:3208` and the slowest at `:3210`.
- **What happens:** live writes went in until the 30th, and from then the writer took none between background transactions or while waiting for room, so the 31st waited for the rest of the history. The test passed 3 of 3, alone at a load of 34 to 35:
  - 31 live writes, 1.1 to 1.5 ms at the median, the slowest 1.03 to 1.14 s;
  - the history took 1.29 to 1.40 s;
  - main's bound of 1 s on every write would have failed all three, by 30 to 140 ms. That's the same margin that made main's flaky, when load stalled one live write to 1.04 s.
- **What is caught:** the count catches live writes waiting for all of the history (2 writes, 3 of 3), and the slowest bound a write held past 3 s. A wait that begins after the 20th live write and lasts under 3 s passes.
- **Also passes:** with no `between()`, it passes 2 of 2, with 48 to 50 live writes at 21.5 ms at the median, as round one found.
- **Held elsewhere:**
  - `live_changes_go_ahead_of_a_background_batch` catches the rule failing whole, with no clock, but not in part: its one live write goes early.
  - `make perf`'s history step holds a live message to 10 ms at p99 through a whole history sync, in release (`scripts/perf.py:505`). A live message that waited for the rest of a chunk would miss that.
- **Fix, as a follow-up:** judge the interleaving on the history's progress rather than the clock, which a stall of the whole machine can't fake. After each live write, read how much of the history is in (a count of `messages`, less the live writes), and require a live write in every tenth of the 20,000. With the rule, there are about 15 a tenth. With the switch above, the last seven tenths have none.

### Unpredictable performance

None in the product: the range doesn't change it.

### Other (follow-ups; none blocks)

**O4. `make perf`'s report prints no load for the background row and the rows after it.**
- **Where:** `scripts/perf.py:603` assigns `steps = r["background"]["steps"]` inside `render()`. That shadows `render()`'s `steps` parameter, which `row()` reads through its closure. So the background row, and the three photo rows that follow it, print "–" for their load average.
- **Shown:** I called `render()` with a load for each step. The background row and the photo row after it printed "–"; without the background step, the photo row printed its load.
- **Verdicts:** right either way.
- **Fix:** name the local something else, such as `outbox`.

**O1 (round one) stands.** The quickest-of-three judgements of the stop and the reader on the frames say "the quickest of three", but not that a slowdown in one run of three passes. I didn't switch it again.

Round one's T1, T2, O2 and O3 are closed.

## Steadiness

On the head's own binaries, each run alone unless it says otherwise:

| Test | Runs | Took | Load |
| --- | --- | --- | --- |
| the WAL with history (T1) | 5 of 5 | 2.0 to 2.1 s | 31 to 32 |
| the new spool test (O2) | 5 of 5 | 11.0 s | 27 to 31 |
| the blob in the spool | 3 of 3 | 0.4 s | 27 |
| the stop | 2 of 2 | 7.0 to 7.1 s | 26 to 28 |
| the restart | 2 of 2 | 12.4 s | 27 to 28 |
| the lone change | 3 of 3 | 2.4 to 2.5 s | 28 to 29 |
| no window starving | 3 of 3 | 2.4 s | 29 to 31 |
| the reader on the frames | 2 of 2 | 22.1 to 22.2 s | 31 to 32 |
| the outbox's two, release, as `make perf` runs them (T2, O3) | 3 of 3 | 20.8 to 70.5 s | 34 to 36 |
| all ten, four at a time | 3 rounds of 3 | 26 to 39 s | 38 to 51 |
| all ten, in the full suite | 1 of 1 | | 31 to 83 |

Add the fix round's 15 of 15 alone, at a load of 34 to 81, and its `make test`, and none of the ten has failed since round one. Round one's six tests are the same code in round one's two whole suites, the fix round's and mine; T1's new judgement and the new spool test ran in the last two.

## Measurements

**Method.**
- **The clone:** one clone of the worktree at `8a854fa6` in `/tmp/jaw-audits/flakes-data-r2`, its `target` the one build folder, built with `CARGO_INCREMENTAL=0` under `nice -n 10`. In order, I built:
  - the head's 21 test binaries in debug, and jawd's tests in release;
  - my switches in debug and release, each binary copied out;
  - the head again, with the switches removed, through the Makefile's `cargo test --release -p jawd --bin jawd --no-run`, before running `make perf`'s step.
- **Each run:** the test binary itself, under `nice -n 10`, from `crates/jawd` as cargo runs it, with `JAW_HOME` and `TMPDIR` in the scratch folder, in a process group of its own that a timeout kills. One test process at a time, unless it says four at a time.
- **Load:** the 1-minute load average at each run's start, on 20 cores, beside other agents' work.
- **The switches** were read from `JAW_AUDIT_SWITCH`. With none set, a switch build runs the branch's code.

**The switches against the changed and new checks.** Each ran alone, at a load of 34 to 55:

| Switch | Test | Result | What it measured |
| --- | --- | --- | --- |
| live writes only once the history is in | T1 | fails 3 of 3 | 2 live writes, the second 1.31 to 1.42 s |
| a 3.1 s stall in the 30th live write | T1 | fails 3 of 3 | 143 to 145 live writes, the slowest 3.10 to 3.11 s |
| live writes waiting for the rest of the history after the 30th | T1 | **passes 3 of 3** (T3) | 31 live writes, the slowest 1.03 to 1.14 s, the history 1.29 to 1.40 s |
| no `between()` | T1 | passes 2 of 2 (T3) | 48 to 50 live writes, 21.5 to 21.6 ms at the median |
| none | T1 | passes 2 of 2 | 142 to 148 live writes, 1.27 to 1.29 ms at the median, 3.2 to 5.2 ms at most |
| the blob removed before the drain's commit | the new spool test; the old blob test | fails 3 of 3; passes 2 of 2 | "the blob stays while its drain can't commit" |
| the blob kept after its drain | the new spool test; the old blob test | fails 3 of 3 each | the wait for the blob to go |
| 4 ms in the 32 steps that run `LET_GO_DONE`, release | both outbox tests | fails 3 of 3 each | 32 at 3 ms or more, 10 allowed |
| 4 ms in a third of the regrouping steps, release | the leaving test | fails 3 of 3 | 333 of 1,031, 35 allowed |
| 4 ms in every step, release | both outbox tests | fails 3 of 3 each | all at 3 ms or more, medians 4.7 to 5.8 ms |
| 10 ms in one letting-go step, every run, release | the leaving test | passes 2 of 2, as allowed | "1 of 3 ms or more", 10.3 to 11.7 ms; `make perf`'s row missed |
| the regroup plan's bound off, debug | both outbox tests | fails 3 of 3 each | the exact check |
| none, release | both outbox tests | passes | every step's quickest 1.51 ms at most, none at 3 ms |

**`make perf`'s step,** the exact command, three times on the head:
- 0 of 1,639 steps at 3 ms or more each time, 1.56 to 1.74 ms at most;
- the medians 0.79 to 0.83 ms and 0.12 to 0.14 ms regrouping, and 0.49 to 0.60 ms letting go;
- met.

## Landing

- **Trailers:** all ten commits touch `scripts/check-data-audit.py`'s paths (`store/`, `sync`, `send` and `feed/`), so all ten take `Data-Audit: docs/audits/2026-10-30-flakes-data.md`, with this note committed in the branch. The script reads the note's last `Verdict:` line, which is round two's.
- **The flakes list:** once this lands, these tests can come off the landers' list:
  - sync's stop bound (`an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean`);
  - the restart test (`a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left`);
  - the feed's starvation and coalescing tests (`no_window_starves_while_the_others_keep_falling_due`, `a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce`);
  - the two WAL-bound tests here (`the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it`, `a_reader_on_the_wals_frames_holds_acks_up_only_a_moment_and_the_wal_starts_again_once_it_lets_go`);
  - the blob test, if it's listed under the restart tests.
- **Why:** each now judges what other processes' load can't fake, or waits for what it checks. None has failed since round one, in two whole suites at the head and two on round one's version, and in every run alone or four at a time.
- **Watched, not listed:** I'd watch them for a week, as `a_flood_of_photos_never_holds_up_a_message` is, so that a failure is looked into rather than run again alone.
- **What stays on the list:** the rest of those entries: sync's engine-disconnect tests, the feed's pacing, lane and chat-list reorder tests, and the other WAL-bound tests.
- **The outbox's step tests** were never on it: in `make test`, a debug build, they check only what each step may do.

## Verdict

Verdict: approved
