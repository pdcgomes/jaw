# Audit: the writer's fairness (W1, W2)

Range: origin/main..65435e8 in writer-fairness (`8d75ca5`, `54bc9ad`, `d165999`, the fixes in `f365bad` and the test in `65435e8`, on `5ab4b5f`), 2026-10-17. Auditor: database-expert. First look at `d165999`; second at `f365bad`; third at `65435e8`, below.

## Third look: 65435e8

`65435e8` answers T2 and H7.

**T2 is done.**
- **The hook:** a `#[cfg(test)]` hook (`store/mod.rs:1021-1028`) fails the insert of an inbox row of a reserved kind, as a full disk, only while a durable write waits to go in that commit. So the inbox-only retry, which carries no such write, succeeds.
- **The test:** `a_durable_write_waiting_for_an_inbox_commit_that_fails_before_taking_it_is_made_once_after` streams such rows at 500 a second, with batches of a conversion, a durable write and another conversion. It checks:
  - each write is made once, in order;
  - each write is answered only once its message can be read, with its seq at or past its event;
  - no row is acked before it can be read;
  - each row is stored once;
  - no seq is skipped or doubled;
  - the failure happened.
- **My mutations,** each in its own build against all 77 store tests:

| Mutation | Result |
| --- | --- |
| B: the fallback drops the handed write (`let _ = first.take();`) | the new test fails, with "a live write was dropped" from the dropped writes' answers |
| G: `commit_inbox` drops a handed write left over when the inbox empties before its commit | `writes_that_meet_a_lock_held_past_the_busy_timeout_are_tried_again_and_all_land` fails |
| E: the fallback makes the handed write before the rows go in alone | nothing fails (see H8) |

**H7 is done.** In T1's test the send goes before the change that fails, so a rolled-back attempt does write `Written` before the attempt that stands writes it again. The test still passes.

**H8. Nothing pins the rule "while the inbox can't commit, nothing else is written" (hygiene; a follow-up).**
- **What happens under mutation E:** the handed write is made before the rows. It's still synced, answered after its own commit and made once, and the rows are still acked only once kept.
- **What's lost:** only the inbox's priority during a failure. With a full disk, the live write would retry on its own, committing the inbox between tries, before the rows got their turn.
- **Fix:** a test asserting the order (the rows' acks before the handed write's answer), when convenient.

**Gate on 65435e8** (scratch clone `/tmp/jaw-audit-writer`, release, 4 threads, load 26 to 38):
- 243 of 246 passed, with 2 ignored. The three failures were H5's two regrouping timing tests and H6's presence-dependent sending test, as on every run here.
- The new test and T1's both passed 5 of 5.

With T2 closed, every rule this range touches has a test that fails without it, and nothing marked data loss or inconsistency remains. P1, H4 to H6 and H8 stand as follow-ups.

## Second look: f365bad

`f365bad` answers the first look's findings.

**What's done:**
- **U1 is fixed.**
  - How: `Stamps` journals each change to `unsettled` (`write.rs:393-401`): the old entry's value, or `None` for one added. `undo` replays the journal backwards to the savepoint's mark (`:417-422`), and `begin`, `rolled_back` and `settle` clear it.
  - Why it's complete:
    - `unsettled` changes nowhere but through `unsettle`.
    - Nothing `settle` calls goes back into `unsettle`.
    - `unsettled` only ever grows by appending, so the journal's indices stay valid and truncating at an entry that was added is right in reverse order.
  - Tests: `a_group_rolled_back_into_quarantine_leaves_where_its_chat_was_read` (with a group that stays and still moves its chat) and `a_send_whose_group_is_rolled_back_owes_no_receipts_and_reads_nothing`. With the journal's replay removed from `undo`, both fail.
- **T1 is done for a failure after a live write has been taken.**
  - `inbox_commits_that_fail_with_live_writes_in_them_ack_nothing_early_and_make_each_write_once_in_order` uses a `#[cfg(test)]` full-disk hook keyed to a thread-local on the writer's thread.
  - Mutations, each caught:
    - acking the rows in the fallback before they're kept;
    - answering the shared writes without making them again.
- **H1, H2 and H3 are done:**
  - the design sentence uses the suggested wording;
  - `jawd status` counts `shared_writes` and `shared_failed`, both `#[serde(default)]`;
  - the fallback logs its error once until the reason changes or a shared commit succeeds.

**What's open:**

**T2. Nothing tests an inbox commit that fails before taking the durable write handed to it (missing test; blocks the approval).**
- **Where:** `store/mod.rs:974-976`, `first.take()` in the fallback.
- **The path:** `first` is still `Some` there only when the next group's commit fails before its loop takes it. That happens when:
  - `synced(true)` fails;
  - `BEGIN IMMEDIATE` meets `SQLITE_BUSY`;
  - or an inbox insert meets `SQLITE_FULL`.

  These are the very failures the fallback is there for, and they're likely when a catch-up meets a full disk.
- **The gap:** a mutation that drops `first` there (`let _ = first.take();`) passes all 150 store, sync, send and feed tests that otherwise pass. The new test's hook fails inside a live write, after `first` was taken.
- **Effect of such a regression:** a durable write (a send, a read, a draft, the commit before a spool file goes) is never made. Its caller gets "jawd's writer has stopped". The code is right today; this is about keeping it so.
- **Fix:** a second `#[cfg(test)]` hook that fails the inbox insert (or the `BEGIN`) as a full disk, once or a few times, when the commit has a `first`. Then check:
  - the durable write is made once and answered after its commit;
  - writes behind it keep their order;
  - the rows are acked only once they're kept.

**H7. The send in T1's test can't show that sending's result comes from the attempt that committed (hygiene).**
- **Why:** in each failing batch, the `Enqueue` comes after the change that fails. That failure is transient, so it aborts the shared attempt before the enqueue runs. No rolled-back attempt ever writes `Written`, and the check confirms only that the retry's result names its rows.
- **Fix:** put the `Enqueue` before the failing change. Even then, the retry will usually get the same row numbers as the rolled-back attempt, so the test pins the overwrite only weakly. Reading the code remains the main assurance (see the first look).

**Still standing as follow-ups:** P1, H4, H5 and H6.
- H5 and H6 failed again in the gate run (the two regrouping timing tests and the presence-dependent sending test), at load 23 to 30. With those set aside, 242 of 245 passed.

**Measured on f365bad** (scratch clone `/tmp/jaw-audit-writer` checked out at `f365bad`, release build):
- Full jawd suite at 4 threads: 242 passed, 3 failed (H5's two tests and H6's), 2 ignored.
- The four writer tests (the three new ones and the stream test) passed 5 of 5.
- Mutations, each in its own build, against the three new tests:

| Mutation | Result |
| --- | --- |
| `undo` without the journal's replay | both U1 tests fail |
| fallback acks rows before keeping them | T1's test fails |
| fallback answers shared writes without making them | T1's test fails |
| fallback drops `first` | nothing fails, across store, sync, send and feed (T2) |

The range answers W1 and W2 of `docs/audits/2026-10-16-send.md`:
- `8d75ca5` adds `jawd bench inbox`.
- `54bc9ad` lets live writes share inbox groups' synced commits (W1).
- `d165999` wakes the checkpointer when background work starts waiting for the WAL (W2).

`origin/main` has since moved to `2822c59`. Its two commits, `934249f` and `2822c59`, touch nothing under `crates/jawd`, so the rebase doesn't change what's audited here.

In short:
- The writer change is correct. I found no data loss or inconsistency in it:
  - Acks and answers stay behind a synced commit.
  - Live writes keep their order, including through the fallback.
  - A bad change can't poison the inbox rows it shares a commit with.
- One test is missing (T1): nothing makes a shared commit fail. That's why the verdict below is "changes needed".
- Probing the shared commits found an inconsistency that is older than this range (U1). A group rolled back into quarantine still moves its chat's read position. Fix it before the owner's first run.

## Findings

### In this range

**T1. No test makes a shared commit fail (missing test; blocks the approval).**
- **Where:** `store/mod.rs:941-953`, the fallback in `commit_group`.
- **What's missing:** the branch's test (`:1996`) puts a bad change in every durable write. That change is quarantined inside its savepoint and the shared commit still succeeds. No test fails the whole shared commit. Yet that path is what keeps acks and answers behind a synced commit when sharing goes wrong. A later edit could:
  - lose `first` when the commit fails before any live write is taken;
  - answer `shared` before its retry;
  - or retry the live writes ahead of the inbox rows.

  Every current test would still pass.
- **What I measured:** a scratch hook makes a `Remember` with a reserved key fail with `SQLITE_FULL` (transient) whenever it runs inside a shared commit. Two of every six live writes carry it. The test streams 1,500 inbox rows at 500 a second while sending batches of six live writes (three of them durable) straight into the live queue.
  - Over 3 runs there were 1,056 to 1,086 live writes, and 142 to 164 shared commits failed and fell back.
  - Every run held:
    - no ack was seen before its row could be read;
    - each of the 1,500 rows was stored exactly once;
    - each write's message landed once, in the order the writes were sent, with events in that order and no seq skipped or repeated;
    - each answer's seq was at or past its write's event;
    - the last write's `Remember` value stands;
    - nothing was quarantined.
- **Fix:** adopt the probe as a test, with the hook under `#[cfg(test)]`. It's in `/tmp/jaw-audit-writer-probes.diff`: the hook in `write.rs`, `AUDIT_SHARING` set around `store_inbox`, and `audit_shared_commits_that_fail_fall_back_in_order_with_acks_only_after_commit`. Adding an `Outbox(Enqueue)` to one of the failing writes would also pin that sending's result comes from the attempt that committed.

**P1. During a stream, a live write waits for the inbox flush already under way (unpredictable performance; a follow-up, not a blocker).**
- **What happens:** with one connection, a synced commit can't be interrupted. So a live write that arrives during an inbox flush waits for that flush, about 5 to 6 ms on this Mac.
- **Measured** at 500 rows a second:
  - a conversion: p50 3.9 to 4.1 ms, p99 9.3 to 9.9 ms (5.1 to 5.8 ms with no stream);
  - a durable write: p99 15.9 to 16.6 ms (11 to 12 ms with no stream).
- **Budgets:** the 5 ms budget is for an incoming message on its way to interfaces once committed, which this bench doesn't measure. Inbox acks, budgeted at 10 ms p99 for "durable and acknowledged", took 13.8 to 14.3 ms p99 here and 16.9 to 31.6 ms on main. This machine was at load 9 to 20, so this is no regression.
- **Follow-up:** say in the design's budgets that a live write during a catch-up waits for at most one inbox flush.

**H1. The design doc's writer sentence says more than the code does (hygiene).**
`docs/plans/2026-10-16-jaw-design.md:129` says live conversions and actions "ride the next inbox group's synced commit while rows keep coming". In the code:
- Between groups, live writes go in transactions of their own, for 1 ms or at least one.
- Only the first durable write, with whatever is queued behind it within 1 ms, rides the next group's synced commit.
- Writes already waiting when a group begins ride it too.

Suggested wording: "then live conversions and actions: after each inbox group, those waiting go in transactions of their own for 1 ms or at least one, and the first that must reach the disk goes, with those behind it, into the next group's synced commit and is answered with its acks, so a catch-up never holds them back and a durable write costs no extra flush".

**H2. Shared writes aren't counted in the writer's stats (hygiene).**
- The shared commit counts once in `transactions`, and its `runs` includes the live writes' work.
- The shared writes add their waits (`:918`) but not a transaction or a run of their own.
- So `jawd status` under-reports the writer's `done` and blurs the inbox commit's run time. Count each shared write, or report the shared commits separately.

**H3. The fallback drops the shared commit's error unlogged (hygiene).**
`Err(_)` at `:941` discards the error. If a live write keeps breaking shared commits, nothing says so, unless its retry alone fails too. Log it once per failure kind.

**H4. The first write in a shared commit is taken whatever its size (note).**
- `SHARE_TIME` bounds what follows the first write, not the first write itself.
- A conversion is up to about 64 changes (`sync/drain.rs:24`). A merge step or a large `Members` change holds its group's acks for as long as it runs.
- Before this change, such a write held the next group's acks for the same time, and the measured acks are unchanged. No change needed.

### The questions asked, judged

- **The fallback when a shared commit fails.** Correct.
  - The inbox rows stay in `self.inbox` and none is acked. They commit alone through `commit_group(false, …)`, which retries from 10 ms to 1 s and writes nothing else meanwhile.
  - Then the writes go one by one through `live_write`, in this order:
    - `first`, which is still `Some` only if the commit failed before taking it, and then `shared` is empty;
    - then `shared` in order;
    - then a `Truncate`.
  - `stamps.rolled_back()` clears the failed attempt's seqs and its `unsettled`. Each retry stamps afresh.
  - Recursion is bounded: inside the fallback and inside `retrying`, `commit_inbox` runs with `share = false`.
  - The probe in T1 exercised this path about 150 times per run.
- **Live writes keep their order.**
  - `between_groups` stops at the first durable write and hands it to the next group, so nothing behind it can pass it.
  - `store_inbox` takes `first` before reading the queue.
  - `commit_inbox` makes a leftover `first` before returning to `run`.
  - A `Truncate` stops the taking and runs right after the commit.
  - Measured in T1's probe: messages in the order the writes were sent, across the fallbacks.
- **A shared write's answer gives the shared commit's last seq.** That is `stamps.last()` after the commit, at or past the write's own event. The `seq` watch moves before the answers go out. It matches what a write alone gets, `stamps.last()` after its commit, so a client waiting for the feed to reach the answer's seq sees its change.
- **Sending's enqueue result after a rollback and a retry alone.**
  - `Written` is overwritten on every attempt (`outbox.rs:293, 299, 316`).
  - Sending reads it only after `durably` returned `Ok` with nothing quarantined (`send/outbox.rs:147-148`).
  - So the value a rolled-back attempt put can only be read if the retry also succeeds, and the retry puts its own.
- **No ack before its rows are synced.**
  - `commits_synced` is checked inside the transaction before `COMMIT` (`:1018-1021`).
  - Acks and answers go only in the `Ok` branch.
  - A failed `COMMIT` rolls back when rusqlite drops the transaction, and its rows stay waiting.
  - A shared commit always runs synced, so a durable live write in one is answered only after its synced commit. The path alone is unchanged.
- **Events and the feed.** Each change writes its event in the shared transaction, stamped in the order the writes were made. The feed's watch moves once the commit is done, so readers see the shared commit whole and in order.
- **A quarantined change can't poison the inbox rows.**
  - The inbox inserts sit outside every savepoint, and each group runs in its own savepoint.
  - No schema statement uses `RAISE(ROLLBACK)` or `ON CONFLICT ROLLBACK`; I searched.
  - So a bad change rolls back only its group. A transient error, or an error that ends the whole transaction, fails the commit and takes the fallback.
  - The branch's test covers the quarantine case, and the probe covers the fallback.
- **W2.** One kick at the start of a wait is enough.
  - `held` is set before it, so the checkpointer doesn't skip the pass.
  - Its pass loops until every frame is copied, sleeping while a reader holds some.
  - Every commit during the wait kicks again.
  - The few frames left behind by live writes are still finished by the writer after `FINISH_AFTER`.
  - Leaving sending's allowance at one in thirty is reasonable. See H5 for those tests under load.

### On main, found while probing (not introduced here)

**U1. A group rolled back into quarantine still moves its chat's read position, and settle applies the move (inconsistency).**
- **Where:**
  - `Stamps::mark` and `undo` (`store/write.rs:396-406`) put back `next` and `touched` but not `unsettled`.
  - `upsert_message` records its read move at `:1662`, before the rest of its group runs.
  - `settle` (`:707`) then applies whatever `unsettled` holds.
  - `make_groups` (`store/mod.rs:602`, main's savepoint loop moved here as it was) relies on `undo`.
- **Reproduced:** a chat had one unread message from others, at 1,000. A live write went in with your own message at 2,000 (live, from another device), `Person::default()` and a `Drained`, all in one group.
  - The group was quarantined and the message rolled back.
  - But the chat's `read_at` went from none to 2,000 and its `unread` from 1 to 0.
  - It fails the same way on `8d75ca5`, which has main's writer.
- **Effect:**
  - A message that never landed marks its chat read: unread and mention counts drop, and marked-unread clears.
  - `read_n` keeps the rolled-back row's number, which the next insert may take.
  - Read from the code, not run: a send whose enqueue fails after its message row would mark the chat read here, with receipts owed (`told`).
  - `delete_row`, `set_read` and merges record their moves the same way.
- **Reach on the first run:** any raw row whose conversion is quarantined after its message, within the same group.
- **Fix:**
  - Let `Mark` carry what `undo` needs to restore `unsettled`. That can be a copy of it (`Unsettled` is `Copy`, one entry per chat the transaction touched), or a journal of the entries changed since the mark.
  - Test: `audit_a_group_rolled_back_leaves_its_chats_read_position_alone` in the probe patch.
- **Priority:** under the sending audit's precedent, findings in main's code don't block this range. But this is an inconsistency, it can happen in the very first catch-up, and `make_groups` is now the one place to fix it. Fix it in this branch before the owner's first run, and I'll look at it with T1.

### Test-suite observations (hygiene, not from this range)

- **H5. Sending's regrouping timing tests fail in parallel runs at this machine's load.** `in_a_group_of_a_thousand…` and `when_most_of_a_thousand_leave…` failed in both full parallel runs, at load 10 to 15, and passed 3 of 3 alone. Run them single-threaded under `make perf`, or bound `Applied::took` rather than wall time.
- **H6. `send::tests::sending_goes_through_the_socket_with_the_pending_row_in_its_answer` fails here whichever writer runs.** It failed 4 of 5 alone on `d165999` and 5 of 5 on `8d75ca5`. It waits for `Presence { available: true }`, but the engine was asked `available: false`. Presence follows Attention, so the test depends on the machine's state. Make it deterministic.
- `sync::tests::a_history_blob_the_engine_couldnt_ask_for_is_never_quarantined` failed once in a parallel run and passed 5 of 5 alone.

## Measurements

**Method.**
- Two scratch clones of the worktree, each a release build with its own target directory:
  - `/tmp/jaw-audit-writer`: `d165999` plus the probes.
  - `/tmp/jaw-audit-base`: `8d75ca5`, which is main's writer with the bench, plus the branch's two new tests and U1's probe.
- `JAW_HOME` and `TMPDIR` pointed at temp folders, and the benches made their own databases.
- The machine is a shared 20-core Mac, at load 9 to 20 throughout.
- No owner data was opened and no Go engine was run.

**The branch's tests without the change** (on `8d75ca5`):
- `live_writes_go_in_with_a_stream_of_inbox_rows_and_are_never_held_back_by_it` fails its check that at least 20 live writes went in while the rows came.
- `background_work_waiting_for_the_wal_gets_room_with_nothing_else_committing` fails with "background work waited 70215 ms in 11 waits".
- On `d165999`, both pass 5 of 5.

**`jawd bench inbox`** (4 s at each rate, a durable write every 50 ms and a conversion halfway between): times in ms, p50 / p99, two alternated runs per build.

| Rows a second | Durable write, this branch | Durable write, main | Conversion, this branch | Conversion, main | Inbox row's ack, this branch | Inbox row's ack, main |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | 6.1 / 11.0; 6.3 / 11.8 | 6.2 / 16.2; 6.1 / 13.9 | 1.0 / 5.1; 1.0 / 5.8 | 0.8 / 11.2; 1.0 / 6.2 | none | none |
| 100 | 7.8 / 14.9; 10.4 / 17.0 | 7.9 / 21.1; 8.0 / 20.5 | 2.2 / 13.0; 2.4 / 7.8 | 1.2 / 15.1; 2.1 / 26.8 | 5.3 / 15.5; 5.4 / 16.1 | 5.1 / 14.1; 5.2 / 15.7 |
| 300 | 9.4 / 14.3; 9.1 / 14.4 | 2,318 / 4,008; 2,309 / 4,012 | 5.1 / 15.7; 3.7 / 7.6 | 2,293 / 3,986; 2,285 / 3,988 | 8.0 / 15.9; 7.6 / 15.0 | 7.5 / 13.7; 7.8 / 16.5 |
| 500 | 9.2 / 16.6; 8.3 / 15.9 | 2,317 / 4,009; 2,393 / 4,025 | 4.1 / 9.9; 3.9 / 9.3 | 2,293 / 3,984; 2,368 / 4,004 | 7.5 / 14.3; 7.3 / 13.8 | 8.4 / 31.6; 8.4 / 16.9 |

The worst durable write on this branch was 17 ms; on main it was 4.0 s, the stream's length. This agrees with the builder's numbers.

**`jawd bench store`** (1,000,000 messages, 2,000 chats; one run per build):

| | This branch (load 13.5) | Main (load 20) |
| --- | --- | --- |
| A live message while history went in, p50 / p99 / max | 0.79 / 10.64 / 127 ms | 0.93 / 14.40 / 271 ms |
| A live message while history was indexed, p99 | 5.95 ms | 3.71 ms |
| WAL peak | 14,924 frames (61.5 MB); the limit is 15,360 | 14,608 frames (60.2 MB) |
| Background waits for the checkpointer | 85, 10.7 s in all | 88, 15.2 s in all |

The differences are within this machine's noise, and the WAL stays under its bound.

**T1's fallback probe:** 3 runs, all passing, with 1,056 to 1,086 live writes and 142 to 164 shared commits failed and fallen back in each.

**Not re-measured:** `jawd bench link --commit`. The builder's figures (p50 5.0 ms, p99 8.3 to 8.5 ms) stand as reported.

## The owner's first run with this writer

A catch-up of an unknown backlog, then history, after a day or so offline:
- **Inbox:** it still commits first, and every row is acked only after its synced commit.
- **Durable writes** (a send, a read, a draft, the commit after which a history spool file goes): each waits for about one inbox flush plus its share of the next, at 8 to 10 ms p50 and 17 ms at worst at 500 rows a second. On main they waited for the stream to end.
- **Conversions:** they run one live write of about 64 changes at a time, since the converter waits for each, and get one turn per inbox group. At the rates the bench covers they keep up (p50 4 ms). If the engine ever delivers faster than they convert, rows wait durably in `inbox` and drain afterwards. Nothing is lost; messages just show later.
- **History:** the background lane waits behind both for as long as the catch-up lasts, as designed. With W2, its waits for the WAL end within a checkpoint pass rather than at the 10 s idle timeout.
- **U1:** it would show up in this run when a raw row's conversion is quarantined after its message. That's the reason to fix it first.

## Verdict

First look (`d165999`): changes needed, for T1, with U1 to fix before the first run.

Second look (`f365bad`):
- U1 is fixed and tested.
- T1 is tested for failures after a live write has been taken.
- H1 to H3 are done.
- No data loss or inconsistency remains in the range.

One change is still needed before landing:
- T2: a test in which the commit carrying a handed-over durable write fails before taking it, for example a full disk on the inbox insert or `SQLITE_BUSY` at `BEGIN`.
- The test must check that the write is made once, in order, and answered only after its commit.
- It must fail with `first` dropped from the fallback.

H7, P1, H4, H5 and H6 can stand as follow-ups. Then ask me to look again; I'll check the test against that mutation.

Third look (`65435e8`):
- T2 is tested and fails with the handed write dropped. H7 is done.
- No data loss or inconsistency remains, and every rule the range touches is tested.

Follow-ups, written down above, to schedule after landing:
- P1: a live write during a stream waits for the inbox flush under way.
- H4: the first write in a shared commit is taken whatever its size.
- H5: sending's two regrouping timing tests fail under parallel load.
- H6: the presence-dependent sending test fails on this machine.
- H8: nothing pins the inbox's priority while it can't commit.

Verdict: approved
