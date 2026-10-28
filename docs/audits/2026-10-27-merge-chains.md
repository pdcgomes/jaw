# Audit: merge chains

# Round three: `8591529..a2e9040`, C1, T2, H7 and the rebase onto main

Range: `8591529..a2e9040` in `merge-chains`, thirteen commits, 28 October 2026. Auditor: database-expert. Line numbers are at `a2e9040`, and paths are under `crates/jawd/src/` unless they say otherwise.

The worktree stayed at `a2e9040`, with nothing uncommitted but this note. `origin/main` is `8591529`, which holds `feed-names` and main's kinds backfill. Round two's nine commits are `2b119f4` to `8878fac` here, rebased onto it, and four follow: `745eae8` (C1), `054ea2f` (T2), `0e35a6e` (H7) and `a2e9040` (two tests of `carry`). The range adds no migration.

## In short

Approved. Round two's C1, T2 and H7 are closed, each by a test that fails without its fix. The rebase is built as round two asked, and nothing I did to the range lost or broke anything the range is answerable for.

- **C1 is closed.** The second settings merge now runs only for a person with three JIDs or more (`could_chain`, `store/write.rs:1609`), one covering search pinned in `plans.txt`.
  - From `two-chats`, nothing undone comes back as the merge ends, the 24-hour timer included, and `x1-chain`'s and `ended-chain`'s draft and pin still arrive.
  - In my 240 random histories begun by `4126db2`'s store and ended at the head, a person with two JIDs always ended with the settings the head's own run of the same history gave him. With the gate taken out, 8 didn't.
- **With three JIDs or more, what was undone on the kept chat while an older jawd's merge ran still comes back once** (C1b, other). That can be a pin, a mute, an archive undone, a mark, a sent draft or the disappearing timer, and my histories hit it in 7 of 240. It's what round two accepted, and `merge_started`'s comment says so (all but the mark, H8). It can stand as a follow-up.
- **T2 and H7 are closed.**
  - With the flatten's `settle` taken out, `read-chain`'s test fails, and so do 2 of my 18 histories that opened with a chain standing.
  - With the circle guard taken out, the circle test fails after 30 s ("opening jaw.db never ended"), and the run goes on.
- **The rebase is right.** `carry` moves the tombstones with `RETURNING`, calls main's `follow` (the `pending` rows and the votes set aside, done where the kept chat has their message), then deletes the messages the tombstones name. `a2e9040`'s two tests pin both halves:
  - with main's order, the deletions first, the reaction test fails;
  - with the branch's own move of `pending` rows in `follow`'s place, ten tests fail, the new vote test among them.
  Every statement is an index search. The plan test passes at the head and at each commit that conflicted, and each builds alone.
- **The random histories pass.** I ran 120 at the head from the start and 240 begun by `4126db2`'s store.
  - Each was checked after every commit: no merge into itself, chain or circle; no person with two chats not merging; no merge record without its merge; counts and unread as their rows give them.
  - Each was killed after a random op, and every copy a kill left (117 of the 120 at the head) played on to the same rows.
  - Each ended as one chat per person with every message once.
- **What stays from before is main's, and a little more than round two said.** In the 240 begun at `4126db2`, every difference from the head's own run is something `4126db2` had already done by the cut:
  - X2's leftovers (H5) left a message deleted for you showing in 11, and a revoke, edit, reaction or receipt waiting in `pending` in 35. Those were applied only because my histories send everything again at the end.
  - Main's X4 had cancelled owed receipts in 15.
  At the head, everything sent again cancels owed receipts in 51 of the 120, each time by a phone read naming the kept chat by its own JID: that's X4b.
- **Main's newer code doesn't trouble the range.** `feed-names` changes only how messages are read. Main's kinds backfill and the merges, from a jaw.db at version 15, end the same in all six runs I made.
- **The fixtures** are what `4126db2`'s store makes from `made-at-4126db2.rs`, all five, row for row, and hold no real data.
- **Cost.** Against main, the range adds about 0.2 ms to the join that starts a merge, and 0.1 ms to a step and to the last step.

## What was asked, checked

### C1

- **The gate.** `merge_step` deletes the merge's `merge.started` row (`store/write.rs:1595`). If there was none, it merges the chat's settings again only when `could_chain(keep)` holds (`:1596`): the kept chat's person has three rows in `person_jids` or more. They're counted by `person_jids_by_person`, `SEARCH person_jids USING COVERING INDEX person_jids_by_person (person=?)`, with the chat's person found by its key, as `plans.txt` pins it.
- **It never misses a chain.**
  - A chain takes three of a person's chats. Each is a one-to-one chat named by one of their JIDs, and each JID is in `person_jids` from the moment its chat is made.
  - Nothing deletes a `person_jids` row, and a join moves them all to the person kept.
  - So by the last step the count is at least the number of chats the person ever had, and every person who could have had a chain passes.
- **With two JIDs, nothing comes back.**
  - The builder's test of it, `what_was_undone_in_the_chat_that_stays_while_an_earlier_jawd_merged_a_persons_two_chats_stays_undone` (`sync/merge/tests.rs:1003`), starts from `two-chats`. It fails with the gate taken out, and with the gate at two JIDs.
  - With the gate at four JIDs, the D1 tests fail instead (`:807`, `:865`).
  - Nothing else fails under any of the three.
  - In my histories begun at `4126db2`, Rui is a second person with two JIDs, whose settings change at random while his merge runs. In all 240, his settings ended as the head's own run of the same history left them. With the gate taken out or at two JIDs, 8 differed: in 7 an archive was undone, and in 1 a pin came back.
- **With three JIDs or more:** C1b, below.

### T2

- `a_chain_an_earlier_jawd_left_after_reading_the_chat_that_stays_is_counted_read_as_jaw_db_opens` (`:758`) reads the LID chat's 10 unread from `read-chain` before opening it, so it can't pass on a fixture whose count was right already. It then wants 0 right after `Store::open`, before any change.
- With the flatten's `settle` (`store/mod.rs:498`) taken out, it fails, and nothing else of the builder's does.
- Of my 18 histories that opened with a chain standing, 2 then fail as jaw.db opens: the hosted LID's chat shows 1 and 10 unread where its rows give 0.

### H7

- The circle test (`:874`) opens jaw.db on a thread of its own and waits 30 s for it at most (`:885-891`).
- With the circle guard (`store/write.rs:1685`) made unreachable, the test fails, "opening jaw.db never ended: Timeout". Every merge test still ends, in 51 s, rather than the run hanging.
- The thread walking the circle spins on until the test binary exits, on its own copy of jaw.db. That's harmless.

### `a2e9040`, and the rebase

- **The resolutions.** I compared the range with round two's `3d60818..bfdea65`, commit by commit (`git range-diff`). The five commits that didn't conflict are unchanged. The four that did are resolved as round two said:
  - `2b119f4` keeps main's tombstone handling and `follow`, with `repoint` before them, and all four functions;
  - `b0708b6` keeps both lines of `check-data-audit.py`;
  - `55fabf8` builds `carry` on `follow` (`store/write.rs:1446-1459`), and `start_merge` and the last step both call it;
  - `60139ea` keeps `merge_started` beside `follow`.
- **Each conflicting commit stands alone.** `2b119f4`, `b0708b6`, `55fabf8`, `60139ea` and `745eae8` each build with no warnings and pass the plan test.
- **`carry`'s order,** what waited first and then the deletions, is pinned by `a_reaction_waiting_for_a_message_a_chat_that_starts_merging_deleted_for_you_waits_no_more` (`sync/merge/tests.rs:955`). With main's order it fails, and nothing else does; my histories don't catch it (none of 60).
- **`follow` inside `carry`** is pinned by `a_vote_set_aside_under_the_middle_chat_of_a_chain_comes_back_to_the_poll_the_chat_that_stays_has_as_it_goes` (`store/write/polls/tests.rs:558`). With `follow` replaced by the branch's own move of `pending` rows from before the rebase, it fails, along with nine more:
  - four other poll tests, of votes waiting or set aside;
  - two of `store::rules`';
  - three of the builder's merge tests.
  So do 32 of my first 60 histories at the head, with rows left in `pending` for a message their chat has.
- **Plans.** Every statement the range adds is an index search pinned in `plans.txt`, and the plan test passes. There's no scan, no temporary b-tree and no `OFFSET`.

### The random histories

Round two's generator left with its scratch, so these are mine; the source is Appendix A, at the end of this note.

- **What a history is.**
  - Ana has four JIDs (two numbers, a LID and a hosted LID), and Rui two. Their six chats are made in a random order, with 1 to 410 messages each, so a merge takes one to three steps.
  - Ana's JIDs are joined in a random tree and order. A quarter of the histories join three or four at once, and some join twice.
  - Between and around the joins come random merge steps, and random groups of changes sent to any of a person's JIDs:
    - messages from them, and mine;
    - reactions, edits, deletions for everyone and for you, receipts and votes, for messages that came or are still to come;
    - reads on the phone, with the phone's counts or without, and reads here;
    - pins, mutes, archives, timers and marks, and drafts kept as `draft.set` keeps them;
    - earlier groups sent again.
  - A history ends with the messages still to come and every merge stepped to its end. Then everything is sent again but votes and settings, and the merges are stepped again.
  - There are no opens, since `4126db2` quarantines one by a JID merged away (main's X3, which this range fixed).
- **After every commit, it checks:**
  - no merge into itself, and no chain or circle;
  - no merge of a chat that isn't there, or across people or kinds;
  - no person with two one-to-one chats that aren't merging;
  - no merge record without its merge, and, at the head from the start, no merge without its record;
  - each chat's count, newest message, unread and mentions as its rows give them, a chat being merged counting from the later of its position and its kept chat's.
- **At the end, it checks:**
  - no merge left, and one chat for each person;
  - every message that came there once, but those deleted for you;
  - nothing quarantined;
  - nothing in `pending` under a chat that's gone, and nothing but votes waiting for a message its chat has.
- **At the head, from the start:** 120 histories, of 40 ops and 39 commits each on average. 0 failed.
  - Each had a copy of jaw.db taken as a kill would leave it after a random op, opened and played on. In the 117 where the kill didn't fall on the last op, a rest with nothing left to step, the copy ended with the same rows as the run without a kill: every table, by JID and ID, with settings, positions and how far WhatsApp was told.
- **Begun by `4126db2`:**
  - At `4126db2`, its own store played each of 240 histories to a cut and wrote jaw.db with `VACUUM INTO`. The cut came in the second half of what comes before the end, or a few ops after a chain formed. It always came before a step that would end a chain's middle, the one thing `4126db2` does that no later jawd can undo (X1's cascade, which leaves X5).
  - 22 formed a chain, and 18 still stood at the cut, one with two middles. 134 had merges under way, up to four.
  - The head opened each (migrations 12 to 16, and the flatten) and played the rest with the checks above. Each was killed after a random op from the open on and played on from the copy to the same rows.
  - 0 failed for anything this range does.
- **Against the head's own run of the same history**, the 240 begun at `4126db2` ended:
  - with the same messages, reactions, receipts, tombstones, quarantine and people in 229. The other 11 differ only by a message deleted for you that `4126db2` had left beside its tombstone (H5);
  - with each chat's counts, newest message, unread and read position the same in 237. In the other 3, they differ only by that leftover message's own count and unread;
  - with Rui's settings the same in all 240, and Ana's different in 7 (C1b);
  - with more in `pending` in 35, which in each is exactly the rows `4126db2` had left at the cut waiting for a message their chat already had (H5);
  - with what's owed different in 16. In 15, `4126db2` had already cancelled those receipts by the cut, as main's X4 did before this range. The other owes one receipt more, which is harmless.
- **What everything sent again does.**
  - In 51 of the 120 at the head, it cancels receipts owed for messages still read. Each such op holds a phone read naming the chat that's left by its own JID: that's X4b.
  - It also moves some read positions back, which leaves those messages unread again and owes nothing for them, rightly. Main takes a position as said until WhatsApp or you have said where it is (`store/write.rs:940`). A kept chat's position may be only where a message of yours from another device put it, which neither did.

### Main's newer code

- **`feed-names`** (`a1b5f01`) changes only `store/read.rs`: a message row's `v` is read as the newest of its own and its people's. A merge step writes each message it moves with a new `v`, a join writes the person it keeps with one, and the flatten and the second settings merge write only chats. Nothing in this range meets it.
- **Main's kinds backfill.** I ran round two's scene again on the real rebase (Appendix B).
  - A jaw.db at version 15 holds one person's three chats, with templates jawd couldn't show in each, one of them in two of the chats. Under the LID are encrypted edits of a message in the number's chat and of one still in the LID's. I made it once with a chain and once with two plain merges.
  - Opened at the head, migration 16 leaves 24 rows for the backfill. The backfill and the merges were taken in three orders: the backfill first, the merges first, and a step of each in turn.
  - All six runs end the same: one chat with its 21 messages, every template shown, both encrypted edits waiting in `pending` under the number's chat, and nothing quarantined. After every commit there was no chain, and the counts were as their rows give them.

### The fixtures

- **Made by old code.** I added `made-at-4126db2.rs`, as it is at `a2e9040`, to `sync/merge/tests.rs` at `4126db2`, ran it, and compared the five jaw.dbs with the checked-in ones row by row, with the columns the wall clock writes masked. `x1-chain`, `ended-chain`, `standing-chain`, `two-chats` and `read-chain` are each the same, schema, `user_version` and `application_id` included.
- The search index alone differs (`fts_backlog`): its indexer had gone further in one copy than in the other when each was written, as round two found.
- **Free of real data.** `two-chats` and `read-chain` hold the same made-up JIDs as the others, texts `n`, `o` and `l` with a number, and the draft "typed under the LID". They have no raw protobufs, no `sync_state` rows and no free pages.

### The builder's gate

- jawd's whole suite at `a2e9040` passed 548 of 549 unit tests at a load of about 60, and `starting`, `stopping` and `upgrading` passed.
- The one that failed, `the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end`, is a timing test of main's that earlier audits saw fail under load (`2026-10-26-wal-bound.md`, `2026-10-27-unknown-kinds.md`). It passes alone, 3 runs of 3.
- `check-audit` fails until this note is committed with its approval. I didn't run lint or `check-generated`.

## Findings

### Data loss

None.

### Inconsistency

None in this range; C1 is closed.

### Tests (missing)

None. T2 and H7 are closed, and every part of this round's change fails a test when it's switched off (Measurements).

### Unpredictable performance

None.
- `could_chain` is one covering search.
- The range adds about 0.2 ms to the join, and 0.1 ms to a step and to the last step.
- A step of 200 messages takes about 3.8 ms on this machine at main and at the head alike, past the 3 ms background budget in a debug build. That's main's `MERGE_STEP`, as round two said.

### Other (follow-ups; none blocks)

- **C1b. For a person with three JIDs or more, what was undone on the kept chat while an older jawd's merge ran comes back once, as the merge ends.**
  - **Where:** `merge_settings_again` (`store/write.rs:1620`), for a merge with no `merge.started` row whose kept chat passes `could_chain`.
  - **What comes back:** whatever the merge rules prefer of the chat going over the kept chat's.
    - A pin or a mute (`max`), an archive undone (`min`), a mark (`max`), a sent or cleared draft (`coalesce`), and a cleared name or picture.
    - The disappearing timer (`max`). jaw learns a one-to-one chat's timer only from a change of it or from history (`convert/mod.rs:695`, `convert/history.rs:278`). So a timer that comes back stays, and every message sent to the chat carries it.
  - **Measured:** in 7 of my 240 histories begun at `4126db2`, Ana's chat (four JIDs) ended otherwise than the head's own run left it. An archive was undone in 4, a sent draft came back in 1, and a mute in 1. In 1, a 7-day timer came back where the chat had been set to 1 day.
  - **When it can happen:**
    - a merge an older jawd started is still under way when this jawd first opens jaw.db (`jawd status` counts such merges as `merging`);
    - its person has three JIDs or more: a changed number, or a business's hosted JIDs. The count is taken at the last step, so a third JID learned after the upgrade counts too;
    - a setting of the kept chat changed meanwhile toward what the merge rules undo.
  - **Why it can stand:**
    - It's what round two accepted, and it can only happen where D1 could have struck. Nothing an older jawd leaves tells the two apart (round two).
    - Not merging again there would lose what landed on a chain's middle, an answered draft among it, and the design says such a draft is never lost.
    - It's written down, in `merge_started`'s comment.
  - **If it's narrowed again:** merge only the draft again, jaw's own answered write, and leave pins, mutes, archives, marks and the timer to WhatsApp. Or take the timer from the newest line among the kept chat's messages saying it changed, since that line moves with the merge.
- **H8. `merge_started`'s comment leaves the mark out of what comes back** (`store/write.rs:1432-1435`). It names "a pin, a mute or an archive, a draft sent, the disappearing timer", but `max(marked_unread)` brings a mark back too. It needs one more phrase.
- **H5. What older jawds left of X2 is more than rows in `pending`** (outside this range; round two's H5, now measured). In my 240 histories begun at `4126db2`:
  - 11 end with a message deleted for you still showing, beside its tombstone;
  - 35 end with revokes, edits, reactions or receipts waiting in `pending` for a message their chat has. They took effect only because my histories send everything again. Without that, a message deleted for everyone would still show its text, and an edit, a reaction or a read would be missing;
  - each was already in the jaw.db `4126db2` wrote, one for one, and nothing on main or in this range applies it.

  The owner's jaw.db can hold them from any merge made before main's `783036e`, and round two's read-only checks count them. A repair at open needs a change and an audit of its own: do what waits for a message its chat has, and delete the messages beside their tombstones.
- **X4b** stands, with more of it than round two saw (51 of 120 at the head, against 24), since my histories read on the phone more often.
- **H3, H4, H6, X5 and H1** stand as round two left them.

## Measurements

Method:
- **One scratch clone** of the worktree in `/tmp/jaw-audits/merge-chains-3`, with one build folder and `CARGO_INCREMENTAL=0`. I switched it with `git checkout` between `a2e9040`, `8591529`, `4126db2` and the commits that conflicted. Builds were debug, with dependencies at the workspace's `opt-level = 2`, run under `nice` on an M1 Ultra shared with other agents, at loads of 30 to 61.
- **The probes** were test modules of their own in the clone: the random histories (Appendix A) in `sync/merge/`, built at the head, at main and at `4126db2` alike, and the kinds scene (Appendix B) in `store/write/kinds/`.
- **The mutants** were each a one-line `perl` edit of `store/write.rs` or `store/mod.rs`. Each was built and run against every test with `merg` or `polls` in its name, and against the histories it bears on.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, and no Go engine was run. The only processes I stopped were my own: a run of mutants and its build, by PID, after an edit of my probe broke their builds.

Results:
- **Each part switched off:**

  | | Switched off | What fails |
  | --- | --- | --- |
  | M1 | The gate: the settings merged again for every merge an older jawd started (`store/write.rs:1596`) | `two-chats`' test. 8 of the 240 histories from `4126db2`, on Rui's settings |
  | M2 | The gate from two JIDs (`:1613`) | The same |
  | M3 | The gate from four JIDs | The D1 tests of `x1-chain` and `ended-chain` |
  | M4 | The flatten's `settle` (`store/mod.rs:498`) | `read-chain`'s test. 2 of the 18 histories opening with a chain standing, as jaw.db opens |
  | M5 | The circle guard (`store/write.rs:1685`) | The circle test, after 30 s; the run goes on |
  | M6 | `carry` doing the deletions first, as main's `start_merge` did | `a2e9040`'s reaction test. None of 60 histories |
  | M7 | `follow` in `carry`, replaced by the branch's move of `pending` rows from before the rebase | 10 tests: `a2e9040`'s vote test, four more of the polls', two of `store::rules`' and three of the builder's merge tests. 32 of 60 histories at the head |

- **The random histories:** as above. The 120 at the head took 78 to 96 s, six at a time; the 240 from `4126db2`, each with the head's own run beside it, took 316 s.
- **What a big merge costs the writer.** This is round two's scene: the number's chat with 200 messages, the LID's with 20,000 from others, half read on the phone, and 200 reactions to the number's messages sent to the LID, waiting.
  - Each figure is the writer's own `Applied::took`, the transaction and its commit, over 9 runs (900 steps for a step). For a step, `took` also counts any wait for the checkpointer, which a 2 s pause before the join lets catch up.
  - The median comes first, then the 90th percentile and the most.

  | Transaction | `8591529` (main), load 30 to 33 | `a2e9040`, load 34 to 36 | `a2e9040` again, load 42 to 57 |
  | --- | --- | --- | --- |
  | The join, doing the 200 reactions | 4.34 ms (4.65, 5.24) | 4.58 ms (4.83, 7.03) | 4.52 ms (6.23, 8.96) |
  | A step moving 200 messages | 3.78 ms (4.27, 12.72) | 3.87 ms (6.59, 18.80) | 3.91 ms (5.59, 247.94) |
  | The last step | 0.66 ms (0.69, 0.69) | 0.76 ms (0.87, 3.56) | 0.80 ms (1.06, 4.21) |

  - At main, the join already does what waited, through `follow`; the range adds the record and `repoint` to it.
  - The step's 0.1 ms is round two's read merge at each step's start. The last step's 0.1 ms is `repoint`, `carry`, the record and `could_chain`.
  - The 248 ms was one step at a load of 57.
- **The flatten** is unchanged since round two, and its measurements stand: one search of about 2 µs with no chain, and about 45 µs a merge with one.
- **Plans.** The plan test passes at `a2e9040` and at each commit that conflicted. `could_chain` plans as `SEARCH person_jids USING COVERING INDEX person_jids_by_person (person=?)`, with a `SCALAR SUBQUERY` searching `chats` by its key.

# Round two: `3d60818..bfdea65`, round one's findings and main's X2 to X4

Range: `3d60818..bfdea65` in `merge-chains`, nine commits, 28 October 2026. Auditor: database-expert. Line numbers are at `bfdea65`, and paths are under `crates/jawd/src/` unless they say otherwise.

The worktree stayed at `bfdea65`, with nothing uncommitted but this note. Round one audited `396d500`, which is `0daadc4` here after the builder's rebase onto `3d60818` (the saved layout). `origin/main` is at `99d125f` now, one commit past `e2af6a1` that touches only `mac/`. The range adds no migration.

## In short

Changes needed, for two small things:
- **C1, inconsistency.** D1's second settings merge, at the last step of a merge an older jawd started, brings back everything undone on the kept chat since that merge began. That's not only a pin: a mute, an archive, a draft already sent, and the disappearing timer, which every message sent afterwards then carries. It does this for every merge an older jawd left under way, though nothing can have landed on a going chat unless it was a chain's middle, and that needs three of the person's JIDs. Narrow it to those.
- **T2, test gap.** Nothing tests that the flatten recounts unread in its own transaction: with its `settle` taken out, every builder test passes.

Everything else holds:
- **Round one's findings are closed.** D1, I1, I2 and I3, and main's X2, X3 and X4 as round one reported them, each by a test that fails without its fix. T1 by fixtures genuinely made by `4126db2`'s store. H2 as read: its loops now give up after 50 steps. X4 has a second half the fix doesn't cover (X4b, other).
- **Round one's 120 random histories pass on the head,** 30 of them with a kill, with no chain, counters as their rows give them and the merge records matching after every commit. So do the same 120 begun by `4126db2`'s own store and played on at the head: 21 of them open with a chain standing.
- **The flatten** is one transaction that a fault or a real `SIGKILL` at any point leaves undone. It refuses circles of every shape with nothing written. With no chain, opening jaw.db costs one search of about 2 µs and writes nothing.
- **The fixtures** are what `4126db2`'s store makes from the builder's scenes, row for row, hold no real data, and open through the real upgrade path (migration 12 here, 12 to 16 after the rebase).
- **The trial rebase onto `e2af6a1`** conflicts in four of the nine commits. Each has one right resolution, the important one being to build `carry` on main's `follow`. On the result, the store, sync, merge, layout and send tests pass but for three of main's timing tests under load, which pass alone. A merge in flight while main's kinds backfill runs ends the same in every order.
- **The cycle refusal is right;** the pin caveat isn't acceptable as it stands, and is once narrowed (both under "The builder's notes").

## What was asked, checked

### Round one's findings, each switched off

Each fix in `store/write.rs` and `store/mod.rs` switched off in turn, built, and run against every test whose name has `merg` in it (the builder's, `store::rules`', the outbox's, the layout's and sending's), the plan test, and my two random-history probes (24 seeds from scratch, and the 120 prefixes `4126db2` made).

| | Switched off | What fails |
| --- | --- | --- |
| M1 | The flatten at open (`store/mod.rs:410`) | 6 of the builder's: the flatten's, the pane's, the parked rows', the draft and pin's, the standing chain's receipts and the circle's. And 21 of the 120 histories from `4126db2`, each as jaw.db opens, on the chain |
| M2 | The flatten's `settle` (`store/mod.rs:417`) | None of the builder's (T2). 7 of the 120 from `4126db2`, as jaw.db opens, on the first chat's unread |
| M3 | `carry` applying what waited (`store/write.rs:1380`) | The parked rows from `x1-chain`, and what waited under a chat that starts merging. 8 of the 120 from `4126db2`, a late reaction left waiting |
| M4 | `carry` deleting for you (`:1385`) | The same two |
| M5 | `start_merge` moving what waited without applying it (X2) | What waited under a chat that starts merging |
| M6 | The last step moving it without applying it (I1) | The parked rows from `x1-chain`. 8 of the 120 from `4126db2` |
| M7 | The read merge at each step's start (`:1451`, I2) | The receipts from `ended-chain` |
| M8 | The second settings merge (`:1478`, D1) | The draft and pin from `x1-chain`, and the draft from `ended-chain` |
| M9 | The second merge adding frecency | The draft and pin from `x1-chain`, its frecency 2.0 |
| M10 | The second merge for every merge | What was undone during a merge this jawd started, and my restart probe |
| M11 | No record as a merge starts (`:1331`) | The same two, and 24 of 24 histories from scratch, by the record check |
| M12 | An open by its JID as it came (`:964`, X3) | The open by a JID being merged or merged away |
| M13 | A phone read by any JID clearing "told" (`:895`, X4) | The phone read naming a merged JID |
| M14 | A phone read by any JID moving "told" on (`:898`, X4) | The same |
| M15 | The circle guard (`:1556`) | The builder's circle test and my circle probe hang, until my run's timeout (H7) |
| M16 | The chain check's key range (`m.gone > 0`, `:1531`) | The plan test: the check plans as `SCAN m` |
| M17 | `repoint`'s read merge (`:1414`) | The standing chain's receipts. 7 of the 120 from `4126db2` |

The plan test fails under every mutant in my clone only because it also reads my probes' SQL, so it's left out of the table but for M16, where it names the branch's own statement.

X3 has a remainder from before that no test needs: frecency an older jawd counted in a going chat after its merge began, which X3 was, still goes with that chat, since the second merge leaves frecency out so as not to add the start's twice.

### Round one's 120 random histories

- **On the head.** Round one's generator, unchanged (one person's four chats, joined in random orders, with steps, arrivals, reads, replays and kills between), with two checks added after every commit: the merge records (below), and no chain. 0 of 120 failed; 30 had a kill.
- **Begun by `4126db2`.** At `4126db2`, its own store played each history up to a cut and wrote the jaw.db with `VACUUM INTO`. The cut came a few ops after the first chain formed, or at a random op if none did, and always before a step that would end a chain's middle, the one thing `4126db2` does that no later jawd can undo (X1's cascade, which leaves X5). 22 of the 120 formed a chain: 21 still stood at the cut, 17 a few ops after it formed, 2 just before a middle's last step and 2 at the history's end; in one, the first merge had ended and the middle was still going, I2's shape. The head then opened each, flattened it, played the rest, sent everything again and ended the merges. 0 of 120 failed; 30 had a kill after the cut.
- **What still moves.** Sending everything again moved how far WhatsApp was told in 24 of the 120 at the head (41 at `396d500`) and in 22 of those begun by `4126db2`. In each of the six I traced, the read that moved it is the phone's read of the chat that stays, by its own JID, sent again: X4b below.

### The flatten

- **One transaction, undone by anything before its commit.** I added a fault and a pause to my clone's flatten, after each chat it points on and just before its commit.
  - A fault at each point, from `x1-chain` and `standing-chain` (one middle each) and from a chain of six that `4126db2` made (four middles): jaw.db isn't opened, holds what the migrations left (only `user_version` 11 to 12, migration 12 being a schema step of its own), and the next open flattens it and ends every merge with the same rows, settings and "told" as a run with no fault.
  - A real `SIGKILL` of a jawd paused at each of those points (my own child process, by PID): the WAL holds migration 12's commit and nothing of the flatten's (12,392 bytes each time), and the next open ends the same.
  - Its commit is `NORMAL`. Nothing is acknowledged on it, so losing it to a power cut is a kill before it, and a later synced commit makes it durable.
- **Replayed.** A second open finds no chain, writes nothing (every row of every table compared) and doesn't enter the flatten.
- **No chain.** One search of `chat_merges` by its key, `SEARCH m USING INTEGER PRIMARY KEY (rowid>?)` with a covering probe of `chat_merges_by_keep` for each merge under way, as `plans.txt` pins it. It took 1.7 to 1.9 µs at the median and 2.5 to 3.0 µs at p99 over 2,000 runs each, with 0, 1 and 8 merges under way and none a chain.
- **Long chains.** Chains of 2 to 66 merges, made by editing `chat_merges`, flatten and end as one chat holding every message, the counters as their rows give them. The flatten's body takes about 45 µs a merge, 0.21 ms for 2, 0.84 ms for 20 and 3.07 ms for 66, once per open. At 67 merges and past, the walk passes `CHAIN_MOST` and jaw.db is refused as a circle, with nothing written (H3).
- **Circles.** Every shape is refused with nothing written: a merge into itself, two into each other, three round, a tail into a loop, and a loop beside another person's chain, before or after it in `chat_merges`, whose flatten the refusal undoes too.

### The `sync_state` record

- **Written and cleared with its merge.** `merge.started.<gone>` is inserted in the join's transaction (`store/write.rs:1331`) and deleted in the last step's (`:1476`). So a kill or power cut leaves both or neither, and never a merge this jawd started without its row.
- **Checked after every commit** of the 120 histories from scratch: every merge has its row and no row outlives its merge, before and after each kill. In those begun by `4126db2`, no row is without its merge, and none is left once the merges end.
- **Across a restart.** A merge this jawd started, with the kept chat's pin taken off and draft sent partway, copied as a kill would leave it and opened again: its row is still there, and neither the pin nor the draft comes back at its end.
- **What can leave one behind.** An older jawd ending a merge this jawd started, after a downgrade, doesn't know to delete its row. `chats.n` is `AUTOINCREMENT`, so the row can never name a new chat. It's jaw's own class, copied by any rebuild, and bounded by the merges under way. Harmless.

### The fixtures

- **Made by old code.** I added the builder's `made-at-4126db2.rs` to `sync/merge/tests.rs` at `4126db2`, as its comment says, ran it, and compared the three jaw.dbs with the checked-in ones row by row, with the columns the wall clock writes masked (`events.at`, `quarantine.at`, `tombstones.at`). They match, schema and all, at `user_version` 11 and jaw's `application_id`. The search index alone differs (`fts_backlog`, `messages_fts_docsize`), since its indexer was further on when the builder copied them.
- **Free of real data.** Three made-up JIDs (`351910000001@s.whatsapp.net`, `351910000002@s.whatsapp.net`, `100000000009@lid`), texts `l`, `o` and `n` with a number, `edited`, a draft "typed under the LID". No names, no raw protobufs, no inbox, history, app state, media or outbox rows, no `sync_state` rows, and no free pages.
- **The real upgrade path.** `from_before` writes the file into a home and calls `Store::open`, which runs migration 12 here and 12 to 16 after the rebase, then the flatten.

### Unknown kinds on main, after the trial rebase

- **By construction it stays correct.** Main's kinds backfill rewrites a row it can show now in place, by its n, wherever it is (`store/write/kinds.rs`, `show`). A row that turns out to change another message goes through `amend_message`, which finds its chat by `chat_jid` and its target in the chat that stays or a chat merging into it. And the flatten has run before the writer takes the backfill's first step.
- **Tested on the trial rebase.** I built a jaw.db at version 15 as main's kinds tests build theirs, holding:
  - one person's three chats;
  - templates jawd couldn't show in each, one of them in two of the chats;
  - under the LID, encrypted edits of a message in the number's chat and of one still in the LID's.

  I made it once with a chain and once with two plain merges. Opened at the rebase, migration 16 leaves 105 rows for the backfill to look at. The backfill and the merges were then taken in three orders: the backfill first, the merges first, and a step of each in turn.
- **All six runs end the same:**
  - one chat with its 21 messages, every template shown as text;
  - both encrypted edits gone from the chats and waiting in `pending` under the number's chat;
  - nothing quarantined, no chain after any step, and the counts as their rows give them.

### The trial rebase onto `e2af6a1`

Done in my clone (branch `trial`), commit by commit. Four of the nine conflict; the rest apply cleanly.

- **`0daadc4` (X1's fix), in `store/write.rs` and `store/plans.txt`.** Main's second m2-content look changed `start_merge` in the same lines: it now moves tombstones with `RETURNING`, deletes the kept chat's copy of each message they name, and calls `follow`. Main also adds `follow` and `rehome` where the branch adds `repoint` and `merge_named`.
  - Resolution: keep main's tombstone handling and its `follow` call, with the branch's `repoint` before them; keep all four functions; join the two doc comments.
  - The last step's plain moves merge cleanly, and `8ee82d9` replaces them.
- **`bbcb62f`:** clean.
- **`9330784` (the flatten), in `scripts/check-data-audit.py`.** Main adds `crates/jawd/src/settings` to the list where the branch adds `crates/jawd/tests/fixtures/`. Keep both.
- **`8ee82d9` (I1 and X2), in `store/write.rs` and `store/plans.txt`.** This is the one that matters. Main's `start_merge` and its `follow` meet the branch's `carry`.
  - Build `carry` on `follow`:
    - move the tombstones with `RETURNING`;
    - call `follow(gone's JID, keep)`, which moves the `pending` rows and the votes set aside in quarantine and applies those whose message the kept chat has, with `apply_pending`'s new `poll` flag;
    - then `delete_row` each message the moved tombstones name.
  - `start_merge` and the last step both call it, and `rehome` keeps using `follow`.
  - The branch's own `pending` move doesn't compile against main's `apply_pending`. Patched to, it would leave votes set aside under a middle chat's JID without their chat at its last step.
  - Keep the branch's order, deletions for you after what waited: with main's order, a reaction waiting for a message deleted for you would wait for good.
- **`dd509ac`:** clean.
- **`514151a` (D1), in `store/write.rs` and `store/plans.txt`.** The branch adds `merge_started` where `follow` now is. Keep both.
- **`47e70e6`, `ea1db1d` and `bfdea65`:** clean.
- **`plans.txt`:** take both sides throughout, then write it again with `JAW_PLANS=write`. That only drops the duplicates the merge left: nothing plans differently, and nothing scans.

On the result, which compiles with no warnings:
- `store::` and `sync::`: 227 of 230 pass, at a load of 62. The three that fail are main's timing tests (a reader holding acks for 4.1 s, the checkpointer waking twice, a live write waiting 359 ms), and they pass alone, 3 runs of 3.
- Every test whose name has `merg` in it, `layout::` and `send::`: 152 of 152.
- My probes on the rebased code:
  - the 120 histories from scratch and the 120 from `4126db2`, whose v11 prefixes now take migrations 12 to 16 before the flatten: 0 failures;
  - the flatten's faults, kills, circles and long chains: as at the head;
  - the record across a restart;
  - the caveat, which is the same;
  - and the kinds backfill above.

### The builder's notes

- **The saved layout.** `chat_named` (`store/read.rs:672`) follows one merge, as `chat_jid` does. A chain can only stand from before, since `start_merge` now takes the merges going into a chat with it, and the flatten removes it before the readers open. So one hop always reaches the chat that stays. Tested by `a_pane_naming_the_first_chat_of_a_chain…`, which fails without the flatten (M1). After the rebase, main's new one-hop lookups (`CHAT_KEPT` in `send/actions.rs`, the polls' and links' paths) have the same guarantee.
- **Refusing circles is right.**
  - No jawd makes one. `merge_direct_chats` merges only chats that aren't going; `repoint` only moves a keep onto a chat that isn't going; and the flatten makes every keep such a chat.
  - Stepping one would destroy messages. A merge into itself folds each message into itself, and `fold` (`store/write.rs:1656`) then deletes the row: every message of the chat goes. And with this branch's last-step `repoint`, two chats merging into each other become one merging into itself. Main's cascade would have saved them; this code wouldn't, so it must never step a circle.
  - Quarantining and carrying on would mean choosing a chat to stay for a person without knowing which is right, while writes for those JIDs kept landing on chats that are going.
  - Refusing, with nothing written, is what jawd does with any jaw.db it can't trust at open (a fingerprint it doesn't know, a failed `foreign_key_check`). Nothing is acknowledged meanwhile, so WhatsApp keeps what's new.
  - Its message could say more (H4).
- **D1's caveat isn't acceptable as it stands (C1), and is once narrowed.**
  - From what an older jawd leaves, the two cases can't be told apart. Nothing records a going chat's settings at its merge's start or when each setting changed. Its `v` moves with every unread recount and move, and the feed keeps only 7 days.
  - But only a chain's middle can have had anything land on it, and a chain needs three of one person's chats, so three JIDs. With two, a number and a LID, the common case, nothing can have landed, and the second merge only undoes.
  - Narrowed to persons with three JIDs or more, the caveat remains only where D1 could have struck too. There, losing an answered draft is worse than a setting coming back once, and it can stand as a written follow-up.
- **The owner's jaw.db.** Not opened by me. With no chains and no merges under way in it, neither the flatten nor C1 has anything to act on at the owner's upgrade, unless a merge main starts is still under way at that moment: `jawd status` counts them (`merging`). X5 and H1 stay follow-ups.
- **The builder's gate.**
  - jawd's whole unit suite at `bfdea65`: 414 of 416 pass at a load of 65. The two that fail are main's live-write timing tests (a live write waited 1.18 s), and they pass alone, 3 runs of 3, as the builder found.
  - On the trial rebase, three more of main's timing tests behave the same way.
  - `check-audit` fails until this note is approved and committed. I didn't run lint or `check-generated`.

## Findings

### Data loss

None. D1 is closed: a draft and a pin that `4126db2` put on a chain's middle end in the chat that stays (`a_draft_and_a_pin_an_earlier_jawd_put…`, `a_draft_an_earlier_jawd_put_on_a_middle_chat_whose_first_merge_had_ended…`), failing without the flatten (M1) or without the second merge (M8).

### Inconsistency

- **C1. The second settings merge undoes, once, what was done to the kept chat while an older jawd's merge ran, for every such merge, and brings the disappearing timer back with it.**
  - **Where:** `merge_step` calls `merge_settings_again` for every merge with no `merge.started` row (`store/write.rs:1476-1479`). The merge takes `max` of the pins, mutes and timers, `min` of the archives and `coalesce` of the drafts (`:1491`), as `start_merge` does. But by now the gone chat holds what it held when the merge began, and the kept chat what was done to it since.
  - **What happens,** from `caveat.db`, which `4126db2`'s store made:
    - the LID's chat, pinned, muted, with a draft and a 24-hour timer, starts merging into the number's, which takes all four;
    - a step is taken, then the number's chat is unpinned, unmuted, its draft sent, and its timer turned off;
    - opened at `bfdea65` and ended, the number's chat is pinned, muted, has the sent draft in its box, and its timer is back at 86,400 s.
    - `CHAT_TO_SEND` (`send/outbox.rs:36`) gives every message sent to that chat from then on a 24-hour expiration, and `EXPIRES` (`store/write/outbox.rs:178`) stamps it on jaw's copy. So the owner's messages disappear for the recipient a day later, in a chat whose timer was turned off.
    - The person has two JIDs, so no chain was possible and nothing can have landed on the LID's chat. The second merge brought nothing; it only undid.
  - **Fix:**
    - Merge settings again only for a merge whose chat could have been a chain's middle: its person has three JIDs or more in `person_jids`, one search by `person_jids_by_person`. As a prototype in my clone: `caveat.db` then ends with nothing undone coming back, and every merge test still passes, the builder's D1 tests among them, as do the 120 histories from `4126db2`. The statement plans as `SEARCH person_jids USING COVERING INDEX person_jids_by_person (person=?)`.
    - Say in `merge_started`'s comment, and in the commit, that for such a merge what was undone on the kept chat meanwhile comes back once, the timer included.
    - Test it from `caveat.db`'s scene: nothing undone comes back, and `x1-chain`'s and `ended-chain`'s draft and pin still arrive.

### Tests (missing)

These block approval along with C1.

- **T2. Nothing tests that the flatten recounts unread in its own transaction.**
  - **Where:** `store/mod.rs:417`. The flatten's `repoint` merges each moved chat's read position into the chat that stays and unsettles it, and `settle` recounts both before the commit.
  - **What happens:**
    - With that `settle` taken out (M2), every builder test passes. `a_chain_of_merges_an_earlier_jawd_left_is_flattened…` checks the counts as jaw.db opens, but nothing read the chat that stays at `4126db2`, so the count from before is already right; it then reads that chat on the phone, which recounts everything anyway.
    - 7 of my 120 histories from `4126db2` then fail as jaw.db opens: the first chat of a chain shows the unread it had before, not what its rows and the kept chat's position give.
    - Derived counts would then be stale until the kept chat is next touched, against "recomputed in the transaction that changes their source".
  - **Fix:** a fixture of round one's I3 scene, with the new number's chat read on the phone at `4126db2` while the chain stood: right after `Store::open`, before any change, the LID's chat shows 0 unread.

### Unpredictable performance

None; measurements below. The per-step `merge_reads` adds about 0.1 ms (3 %) to a step moving 200 messages. The flatten costs 2 µs with no chain, and a third of a millisecond for a chain of six, once per open. `carry`'s work is bounded by the rows it moves, as `RETURNING` gives them. Every new statement is an index search, pinned in `plans.txt`.

### Other (follow-ups; none blocks)

- **X4b. A phone read of the chat that stays, by its own JID, sent again after a merge, still cancels the receipts the merge left owed for the merged chats' messages.**
  - **Where:** `set_read`, `store/write.rs:895` and `:898`. X4's fix moves "told" only for a read naming the chat by its own JID, but that read says nothing of the messages that came from the person's other JIDs.
  - **What happens:** in 24 of the 120 histories at the head and 22 of those from `4126db2`, sending everything again cleared "told". In the six I traced, it was the phone's read of the kept number's chat, from before the merge, whose position lay past messages from the LID's chat that the merge left owed.
  - Same class as X4: whether the phone read them can't be known, and the merge's rule is to owe them.
  - **Fix:** move "told" on a phone read only up to the first message after it from another of the person's JIDs, or leave such reads to the receipts, which are harmless twice.
- **H3. A chain of 67 merges or more is refused as a circle.** No person has 68 chats, so it can't happen, but the message would mislead. The walk could stop at the number of merges instead.
- **H4. The circle's refusal could say what to do.** It names a chat's n. It could name the merges by JID and say that deleting the `chat_merges` row that points back lets jaw.db open; or `jawd check-db`, once it's on main, could report it.
- **H5. What older jawds left of X2 stays.** `pending` rows waiting for a message their chat already has, and tombstones beside their message, from merges that ended before main's fix (`783036e`), are applied by nothing now. Read-only checks for the owner:
  - `SELECT count(*) FROM pending p JOIN chats c ON c.id = p.chat JOIN messages m ON m.chat = c.n AND m.id = p.target WHERE p.effect ->> '$.do' <> 'keep'`
  - `SELECT count(*) FROM tombstones t JOIN messages m ON m.chat = t.chat AND m.id = t.id`
- **H6. The last step merges the going chat's settings again, but not its folder places.**
  - `start_merge` moves `folder_items` (`store/write.rs:1335-1336`), but the last step doesn't, and deleting the chat cascades them away.
  - Nothing on main or in this range writes folder places yet. But the `folders` branch does, through `chat_jid`. If it lands first, a jawd with folders and without the flatten could put one on a chain's middle, and this range's last step would drop it.
  - Two statements, the start's, would cover it at the last step whichever lands first.
- **H7. Without its guard, the circle test hangs rather than fails** (M15). `Store::open` could run on a thread of its own with a deadline, as H2's loops now give up.
- **X5 and H1** stand as round one left them.

## Measurements

Method:
- **One scratch clone** of the worktree in `/tmp/jaw-audits/merge-chains-2r`, with one build folder and `CARGO_INCREMENTAL=0`. I switched it with `git checkout` between `bfdea65`, `4126db2`, `3d60818` and the trial rebase. Builds were debug, with dependencies, SQLite among them, at the workspace's `opt-level = 2`, run under `nice` on an M1 Ultra shared with other agents, at loads of 50 to 73.
- **The probes** were test modules of their own in the clone:
  - round one's (its generator and checks unchanged) and mine;
  - hooks added to the clone's flatten (a fault and a pause at each point) and to the writer (the last transaction body's time, from its start to just before commit).
  - The fixtures and prefixes from before were made at `4126db2` by its own store.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, and no Go engine was run. The only processes I stopped were my own: the probe's child jawds, killed by PID inside the flatten, and the test binaries the M15 mutant hung, stopped by my run's timeout.

Results:
- **Tests.** At `bfdea65`, jawd's merge and plan tests pass, 39 of 39, and its whole unit suite passes but for the two timing tests above, 414 of 416, in 305 s. The trial rebase's results are above.
- **The random histories:**
  - from scratch, at `bfdea65` and at the trial rebase: 0 of 120 failed, 30 with a kill;
  - from `4126db2`'s prefixes, at both: 0 of 120 failed, 21 opening with a chain, 30 with a kill;
  - in each, after every commit: no chain, the counts as their rows give them, and the merge records with their merges.
- **The flatten,** its body from its transaction's start to just before commit:
  - with no chain, opening jaw.db doesn't enter it, and its check takes 1.7 to 1.9 µs at the median (2.5 to 3.0 µs at p99) with 0, 1 and 8 merges under way;
  - with a chain, about 45 µs a merge in it: 0.21 ms for 2, 0.34 ms for 6, 0.84 ms for 20, 3.07 ms for 66.
- **What a big merge costs the writer.** The LID's chat had 20,000 messages from others, half read on the phone, and 200 reactions waiting under the LID for messages the number's chat has. Each figure is the median of 9 runs (900 steps for a step), with the 90th percentile and the most after it:

  | Transaction | `3d60818` | `bfdea65` |
  | --- | --- | --- |
  | The join that starts the merge | 1.11 ms (1.66, 2.16) | 3.56 ms (3.96, 4.38) |
  | A step moving 200 messages | 3.22 ms (3.61, 8.90) | 3.33 ms (4.58, 27.23) |
  | The last step | 0.49 ms (0.64, 1.18) | 0.52 ms (0.56, 0.80) |

  - The join now does what waited (X2's fix), about 12 µs a reaction. Main's own `follow` does the same since m2-content, so after the rebase the range adds nothing to it.
  - The step's 0.1 ms is the read merge at its start. The 27 ms was one step at a load of 70.
  - A step of 200 messages runs at about the 3 ms background budget on this machine at both commits: that's main's `MERGE_STEP`, outside this range.
- **Plans.** Every statement the range adds is an index search, pinned in `plans.txt`, and the plan test passes at `bfdea65`. After the rebase it still passes, and `plans.txt` written again only loses duplicates.

# Round one: `396d500`, kept as written

# Audit: merge chains

Range: `4126db2..396d500` in `merge-chains` (one commit, `396d500`), 27 October 2026. Auditor: database-expert. Line numbers are at `396d500`, and paths are under `crates/jawd/src/` unless they say otherwise.

The branch stayed at `396d500` throughout, with nothing uncommitted. `origin/main` has moved to `120f5bf`, a docs-only commit, so the builder rebases before landing; this audit is of `396d500` itself. The commit adds no migration: `store/schema.rs` and `store/migrations.lock` are as on `4126db2`.

## In short

Changes needed. For every merge started from now on, X1 is fixed and nothing I could do to it lost or broke anything. But a chain already in jaw.db, which the commit says its last step handles, still ends with things lost or left undone.

- **X1 is fixed for new merges.** `start_merge` moves the merges going into the chat that starts going onto the chat it goes into (`repoint`, `store/write.rs:1344`). So no chat is both kept and going, and every one-hop lookup reaches the chat that stays. I ran 120 random histories of one person's four chats, which joined them in random orders and interleaved steps, arrivals, reads, replays and kills. After every commit there was no self-merge, no chain and no cycle, and the counters matched their rows. Every history ended as one chat with everything applied. Before the fix, 24 of the 120 made chains.
- **A chain already in jaw.db ends as one chat, of any length, but not with everything it was sent.** I made jaw.dbs with the code before the fix (`4126db2`) and opened them with `396d500`. While a chain stands, writes for its first chat's JID still resolve one hop, to the chat in the middle. That middle chat is removed later, and what landed on it is lost or left undone:
  - a draft and a pin are lost (D1);
  - a reaction, an edit and a deletion for everyone, for messages that had already gone through, wait in `pending` for good, and a deletion for you leaves its tombstone while the message still shows (I1);
  - receipts owed for the first chat's messages are dropped (I2);
  - the first chat's unread count isn't recounted when the chat that stays is read (I3).
  The commit's test of this case makes its chain by editing `chat_merges` after the fix has already run, with nothing yet through the middle chat, so it can't see these (T1).
- **The rest holds.**
  - A failed step's quarantine row follows its merge, and so does one asked for before the merge moved on.
  - Every new statement is an index search, pinned in `plans.txt`.
  - `repoint` adds about 0.2 ms to the transactions it's in.
  - Each part of the fix but one fails a test when switched off: the builder's tests catch seven of the nine, and an older test in `store::rules` catches one by hanging (H2). The exception is the rename's `OR REPLACE`, whose collision can't happen (H1).
- **Outside this range, in main's code:**
  - what waited under the chat going for a message the kept chat already has is never applied (X2);
  - an open by a JID being merged, or merged away, is lost (X3);
  - a phone read sent again after a merge cancels receipts the merge left owed (X4);
  - jaw.dbs that X1 has already split stay split (X5).

## What was asked, checked

- **Cycles and self-merges.**
  - By construction there are none:
    - `merge_direct_chats` picks only chats that aren't going (`store/write.rs:1288`) and never keeps the chat it merges away.
    - `repoint` moves only the merges going into the chat that starts going. Their new keep is a chat that isn't going, so none can point at itself, and A into B with B into A can't arise.
    - `INSERT OR REPLACE INTO chat_merges` never replaces, since the chat going is never already going.
  - Checked after every commit of the 120 random histories, which covered:
    - every creation order of two numbers, a LID and a hosted LID;
    - random join trees in random orders, a third of them joining three or four JIDs at once;
    - steps of random merges between them, as the task takes them after failures.
  - The same merge asked for twice, and a merge asked for again after it finished: each history sends everything again once its merges are done, joins included. That changed nothing but X4.
- **Chains of any length.**
  - None form now.
  - One from before, three long (L3 into L2 into L1 into a number), made by `4126db2` with a step of each merge taken, ends as one chat with all 1,000 messages. Its counters are as its rows give them and nothing is quarantined. Each middle chat's last step moves on the merges going into it (`store/write.rs:1410`).
  - Repointing is bounded by the merges going into one chat, a person's other chats, with no recursion.
- **A kill between any two commits.**
  - A live `apply` is one transaction (`store/mod.rs:1303`), and a merge step is a group of its own in one background transaction. So a copy after each apply or step is a copy after each commit.
  - Covered in two ways:
    - the builder's 13 kill points, in both orders;
    - one kill at a random op in 30 of my 120 histories, copied with `VACUUM INTO`, opened again and played on. Each ended with the same rows as the run without a kill, how far WhatsApp was told included.
  - The merge commits are `NORMAL` and nothing is acknowledged on them. So losing a suffix of them to a power cut is a kill earlier.
- **Read positions and receipts.**
  - `repoint` merges each moved merge's position into the kept chat's (`merge_reads`) in the same transaction. That moves "told" back to before its oldest unread message from others.
  - Receipts for the kept chat's own messages after that point go twice. That's by design ("a receipt sent twice is harmless", `store/write.rs:905`), not new.
  - With a fresh chain, the LID read here up to `l9` leaves the chat that stays owing `l0` to `l9`. That's my control, and the builder's test does the same.
  - From before, the same scene owes nothing (I2).
- **Counters stay derived.**
  - Nothing is adjusted by hand. `merge_reads` unsettles the kept chat, and `settle`'s second pass (`store/write.rs:766`) recounts it and every chat going into it, the repointed ones included.
  - Checked after every commit of the random histories: every chat's `message_count`, and the unread of every chat not going. For a chat going, unread is checked from the later of its own and its kept chat's positions, as `settle` counts it.
- **Quarantine rows.**
  - A rename (`store/write.rs:1352`) can meet a row of the same name only if an earlier merge of the same chat into the same chat had left one. That can't happen:
    - `chat_merges.gone` is unique, and the name holds its JID;
    - a merge's keep only moves forward;
    - its row goes with its last step, which now moves on the merges going into it first.
  - If it ever did, `OR REPLACE` keeps the renamed row with the latest error, one row per merge, and drops nothing that's retried, since a step's row holds no payload.
  - A step asked for before its merge moved on is named by `merge_named` (`store/write.rs:521`), so a second failure updates the one row.
- **Concurrency.**
  - One writer, `BEGIN IMMEDIATE` (`store/mod.rs:1318`).
  - `repoint` is inside the join's transaction and inside the last step's. No transaction or acknowledgement is added.
  - The merging task reads `waiting` from a snapshot. A step it asks for after a repoint finds its merge's current keep itself (`store/write.rs:1379`).
  - In the random histories, messages, reactions, edits, deletions for you and for everyone, and reads come to any of the person's JIDs, between joins and steps.
- **A jaw.db from before.** It doesn't hold everything, as above: D1, I1, I2 and I3.

### Each part switched off

Each switch is in `store/write.rs` at `396d500`, run with every test whose name has `merg` in it: the builder's, `store::rules`', the outbox's and my probes. That's 32 tests, and at `396d500` the only failure is my from-before probe (I1).

| Switched off | What fails |
| --- | --- |
| `start_merge`'s `repoint` | 3 of the builder's: the chain goes on into the chat that stays, the quarantine row, and the three chats. The X1 scene itself passes, since the last step's `repoint` saves it |
| The last step's `repoint` | `a_chain_of_merges_jaw_db_already_holds…` |
| The last step's tombstone move | the same |
| The last step's `pending` move | the same |
| `repoint`'s quarantine rename | `a_merge_that_failed_before_it_went_on…` |
| `repoint`'s `merge_reads` | `the_receipts_a_merged_chat_owes…` |
| `merge_named` in `quarantine` | `a_merge_that_failed_before_it_went_on…` |
| The rename as a plain `UPDATE` | none of the builder's; only my probe with a stale row (H1) |
| The last step's tombstone move as a plain `UPDATE` | `store::rules::a_merge_step_applies_what_waited…` hangs (H2). `move_merged` leaves a tombstone in the chat going for each message it deletes, so the move meets a duplicate in every such merge |
| `repoint` at the end of `start_merge` instead | nothing: it's equivalent, since positions merge the same in either order |

## Findings

D1 and I1 to I3 have one cause. A chain already in jaw.db is moved on only when its middle chat's merge ends (`store/write.rs:1410`). Until then, every lookup that follows one merge stops at the middle chat:
- `chat_jid` (`store/write.rs:1055`);
- `send/reads.rs:25`;
- `send/actions.rs:47`;
- `sent_where` (`store/write/outbox.rs:196`);
- `settle`'s second pass.

The middle chat is then removed with whatever landed on it.

Such chains are in scope here. The commit adds code for them (`store/write.rs:1405-1412`). And main, which the owner's jawd runs today, makes them whenever X1's shape comes up: the chain stands until its middle merge ends, which X1 then turns into two chats. If jawd is upgraded while one stands, this is what's left.

### Data loss

- **D1. A draft set in the first chat's pane while a chain from before stands is lost when the middle chat goes.**
  - **Where:**
    - `draft.set` (`send/mod.rs:200`) becomes `Change::Chat`, which `upsert_chat` resolves one hop (`store/write.rs:1113`), onto the middle chat;
    - that chat is deleted at its last step (`store/write.rs:1414`), and its settings were merged into the kept chat only at its start (`store/write.rs:1325`).
  - **What happens,** measured on a jaw.db made by `4126db2` (X1's scene, a step of each merge taken) and opened by `396d500`:
    - a draft and a pin for the LID's chat land on the old number's chat;
    - after the merges the new number's chat has neither.
    - `draft.set` had answered once its synced commit was made, so a draft acknowledged as kept is gone.
  - **Fix:**
    - When the writer opens jaw.db, before it takes any change, flatten every chain in one transaction. While a merge's keep is itself going, `repoint` the merges going into that chat onto where it goes, and `merge_reads` it into the chat that stays.
    - That's bounded by the merges, a handful, and `chat_merges` may be read whole, as `waiting` does.
    - After it, every merge goes into a chat that stays, and each one-hop lookup is right from the first change on. That closes D1, I3, I2 for a chain standing at the upgrade, and I1 for what comes after it.
    - For drafts and settings the old jawd put on the middle chat before the upgrade, the last step could merge the going chat's settings into the kept chat's again, as `start_merge` does. `frecency`, a sum, would be left out.

### Inconsistency

- **I1. What's sent to the first chat's JID, for a message that has already gone through the middle chat, is never applied.**
  - **Where:**
    - `amend_message` resolves the LID to the middle chat and finds the message in neither it nor the chats going into it, so it waits in `pending` (`store/write.rs:1844`), or a deletion for you leaves a tombstone there (`store/write.rs:1826`);
    - the new last step moves both into the kept chat (`store/write.rs:1411-1412`), whose copy of the message never arrives or moves again, so `apply_pending` never runs for it.
  - **What happens,** on the same jaw.db: a reaction, an edit and a deletion for everyone, sent to the LID for `l10`, `l30` and `l25`, which had gone LID to old number to new number:
    - after the merges, the three wait in `pending` under the new number's chat for good;
    - `l25` shows its text, deleted for everyone;
    - `l20`, deleted for you, still shows, with its tombstone in the same chat.
  - The jawd before the fix parks such changes the same way, so a jaw.db from before can already hold some.
  - **Fix:**
    - D1's flattening, for what comes after the upgrade.
    - Where the last step moves `pending` rows and tombstones into the kept chat, apply those whose message the kept chat already has: `apply_pending` for each target moved, `delete_row` for each tombstone moved. Take the targets from the rows moved, so the work is bounded by them.
    - The same in `start_merge` closes X2.
- **I2. Receipts owed for the first chat's messages are lost when its merge ends before the middle chat's.**
  - **Where:**
    - the merging task takes merges by `gone` (`sync/merge.rs:124`), so a LID's chat older than the old number's merges into it first;
    - its messages reach the middle chat after the middle chat's own start moved the kept chat's "told" (`store/write.rs:1336`), and nothing moves it back for them;
    - the last step's `merge_reads` finds the middle chat empty.
  - **What happens,** on a jaw.db made by `4126db2`:
    - the LID was read here up to `l9`, and the numbers' chats on the phone;
    - then the LID started merging into the old number's chat, and that into the new number's;
    - after the merges, in the task's order, the new number's chat owes nothing, and `l0` to `l9` are never told;
    - the same scene made by `396d500` owes all ten.
  - **Fix:**
    - D1's flattening covers a chain standing at the upgrade: its `merge_reads` of the first chat still finds `l0` to `l9` there.
    - For one whose first merge had ended before the upgrade, so that its messages sit in the middle chat, `merge_reads` at the start of every step, not only the first and last. It's idempotent and costs a few index probes.
- **I3. While a chain from before stands, reading the chat that stays doesn't read its first chat.**
  - **Where:** `settle`'s second pass goes one level (`store/write.rs:766`), and a chat going counts from its kept chat's position, one hop (`store/write.rs:801`).
  - **What happens,** on the same jaw.db, after the new number's chat is read on the phone: it and the old number's go to 0 unread, and the LID's chat, still listed, stays at 210 until the old number's merge ends.
  - **Fix:** D1's flattening.

### Tests (missing)

This blocks approval along with D1 and I1 to I3.

- **T1. The test of a chain already in jaw.db isn't one.**
  - **Where:** `sync/merge/tests.rs:626`.
  - **What happens:**
    - it makes its chain by setting the LID's keep back to the old number's chat (`:643`) after the fix's own `start_merge` has already run, its `merge_reads` of the LID included;
    - it does so before any step, so nothing has gone through the middle chat;
    - it sends only what finds its message, and no draft.
    - It passes with D1, I1 and I2 all present.
  - **Fix:**
    - Start from jaw.db as the code before the fix leaves it, with a step of each merge taken: either a fixture written by `4126db2`, or rows written to match it, with the kept chat's "told" left as that code left it.
    - Then send the first chat's JID a draft, a reaction, an edit, a deletion for you and one for everyone, for messages that went through, and read it here before the joins.
    - Check that all of it is applied in the chat that stays, its receipts still owed, and its unread recounted when that chat is read.
    - How I made mine is under Measurements.

### Unpredictable performance

None. `repoint` adds about 0.2 ms to the join that makes a chain and to a middle chat's last step (Measurements). Every statement it adds or repeats is a search by primary key or index, pinned:
- `chat_merges_by_keep`;
- `quarantine_by_ref`;
- the rowids of `chat_merges` and `chats`;
- the tombstones' key and `pending_by_target_at`;
- `merge_reads`' search of `messages_by_time`, a covering range with `LIMIT 1`.

There's no scan of a large table, no temporary b-tree and no `OFFSET`.

### Other (follow-ups; none blocks)

- **H1. The rename's `OR REPLACE` has no test.**
  - **Where:** `store/write.rs:1352`.
  - **What happens:** as a plain `UPDATE`, every test of the builder's passes. A collision can't happen (What was asked, checked). If one did, the join's whole transaction would fail and the join be quarantined, so `OR REPLACE` is right.
  - **Fix:** optional, a test with a row already under the new name. Mine puts one in with a side connection and checks there's one row and the join is made.
- **H2. `store::rules`' merge loops have no bound.**
  - **Where:** `merged` (`store/rules.rs:233`) and the loop at `:207`.
  - **What happens:** a merge step that keeps failing loops them for ever. Under the switch that makes the last step's tombstone move a plain `UPDATE`, `a_merge_step_applies_what_waited_for_a_message_and_drops_one_deleted_for_me` hung for 8 minutes until I stopped it, rather than failing.
  - **Fix:** give up after 50 steps with a panic, as `merges_run` does in `sync/merge/tests.rs:189`.

### Outside this range (main's code)

- **X2, inconsistency. What waited under the chat going for a message the kept chat already has is never applied.**
  - **Where:** `start_merge` moves the going chat's `pending` rows and tombstones into the kept chat (`store/write.rs:1319-1321`) without applying them. They're applied only when their message arrives or moves.
  - **What happens:**
    - a reaction and a deletion for everyone, sent to the LID for `n0`, a message only the number's chat had, wait in `pending` under the LID's chat;
    - after the join and the merge, both wait under the number's chat for good, and `n0` shows its text;
    - the same happens at `4126db2`.
    - It needs only a LID's chat to exist before its join, with a change for a message the number's chat has.
  - **Fix:** the second half of I1's, in `start_merge`. Done in this branch, it would be audited with I1's.
- **X3, other. An open is lost when made by a JID being merged, and quarantined when made by one merged away.**
  - **Where:** `open_chat` passes the JID as it came to `ensure_chat` (`store/write.rs:951`), not through `chat_jid`.
  - **What happens:**
    - an open of the LID's chat during its merge counts in its own frecency, which goes with it: 0.0 after the merge, against 1.0 opened by the number;
    - after the merge, an open by the LID fails and is kept in quarantine ("opened 100000000009@lid: Query returned no rows").
    - Frecency is jaw's own data.
  - **Fix:** resolve the JID through `chat_jid` first, as `upsert_chat` does.
- **X4, other. A phone read sent again after a merge cancels receipts the merge left owed.**
  - **Where:** `set_read`, a read not taken (`store/write.rs:886`).
  - **What happens:**
    - the number's chat was never read, and the LID's was read on the phone past every message;
    - once merged, the chat owes `n0` to `n4` and `l0` to `l4`;
    - the same read again, now resolved to the number's chat, moves "told" up to it, and nothing is owed.
    - In the random histories, sending everything again moved "told" in 41 of 120 at `396d500`, and in 32 of the 96 without a chain at `4126db2`.
    - Whether the phone read the number's messages can't be known, and the merge's own rule says to owe them.
  - **Fix:** move "told" forward on a read only when it names the chat by its own JID. Receipts that then go twice are harmless.
- **X5, other. jaw.dbs that X1 has already split stay split.**
  - **What happens:** a person left with two one-to-one chats, neither going, is merged only at their next join. The owner can check theirs with a read-only query, which I didn't run, since I don't open the owner's folder: `SELECT person, count(*) FROM chats WHERE kind = 'direct' AND person IS NOT NULL AND n NOT IN (SELECT gone FROM chat_merges) GROUP BY person HAVING count(*) > 1`. Chains standing now: `SELECT count(*) FROM chat_merges m JOIN chat_merges k ON k.gone = m.keep`.
  - **Fix:** if any are found, merge such a person's chats once at start, or let the owner decide.

## Measurements

Method:
- **One scratch clone** of the worktree in `/tmp/jaw-audits/merge-chains`, with one build folder and `CARGO_INCREMENTAL=0`. I switched it between `396d500` and `4126db2` with `git checkout`. Builds were debug, with dependencies, SQLite among them, at the workspace's `opt-level = 2`, run under `nice` on an M1 Ultra shared with other agents, at loads of 47 to 72.
- **My probes** were a test module of their own in the clone. The fixtures were made at `4126db2` by the same module and opened at `396d500`. For I1, in X1's scene:
  - `NEW` got 2 messages, `OLD` 5 and the LID 410;
  - `join(OLD, LID)`, then a step of the LID's merge;
  - `join(LID, NEW)`, then a step of the old number's;
  - `VACUUM INTO`.
  - For I2: `NEW`, the LID and `OLD` were made in that order; `NEW` and `OLD` read on the phone and the LID here up to `l9`; then the two joins, with no step.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, and no Go engine was run. The only process I stopped was my own test binary (PID 66889, hung under the switch in H2), by PID.

Results:
- **jawd's suite at `396d500`,** `cargo test -p jawd`: 376 passed, 3 ignored, `starting` 1 and `stopping` 2 passed. It took 167 s at load 50 to 66. The merge and plan tests alone pass.
- **The random histories:**
  - at `396d500`, 0 of 120 failed, 30 of them with a kill;
  - at `4126db2`, 24 of 120 failed. The probe printed 12 of them, each on a chain after 1 to 7 ops; in an earlier run of 40, all 7 that failed did so on a chain.
- **The from-before fixtures, at `396d500`:**
  - X1's scene, as I1 and D1 say;
  - the receipts scene owes nothing, against `l0` to `l9` from the fix's own scene (I2);
  - the 210 unread of I3;
  - the chain of three ends as one chat with 1,000 messages.
- **What `repoint` costs.** These are the writer's transaction bodies (groups and `settle`, to just before commit), logged by a switch in my clone's `store/mod.rs`. Each is the median of 15 runs, with the 90th percentile and the most after it. X1's scene had the LID's chat with 20,000 messages from others, half read on the phone.

| Transaction | `4126db2` | `396d500` |
| --- | --- | --- |
| The join that starts the old number's merge (at `396d500`, moving the LID's on) | 0.29 ms (0.35, 0.39) | 0.48 ms (0.50, 0.54) |
| The old number's last step, a chain standing (at `4126db2`, cascading the LID's merge away) | 0.74 ms (0.80, 1.92) | 0.93 ms (1.03, 1.20) |
| A last step with nothing going into its chat | 0.34 ms (0.39, 0.45) | 0.34 ms (0.41, 0.59) |

  These are within the live (5 ms) and background (3 ms) budgets. `Applied::took` for a background change also counts its wait for the checkpointer (`wait_for_room`). Right after 20,000 inserts, that put the same last step at 8 to 11 ms at both commits: the WAL's bound at work, not the step.
- **Plans:** four statements are new in `plans.txt` and one has moved. All plan as primary-key or index searches, and the plan test passes.

## Verdict

X1 is fixed for every merge from now on. That part is well tested, and nothing in it loses data or leaves jaw.db inconsistent. Before landing:
1. Make a chain already in jaw.db end consistent:
   - flatten chains when the writer opens jaw.db (D1, I2, I3, and I1 from then on);
   - apply the `pending` rows and tombstones the last step moves whose message the kept chat already has (I1);
   - for receipts of a first merge that ended before the upgrade, merge reads at every step (I2).
2. Test it from jaw.db as the code before the fix leaves it (T1).
3. Have a fresh audit look at the change.

H1 and H2 can stand as follow-ups. X2 to X5 are main's: X2 shares I1's fix and could come with it; the rest need their own changes. Once approved, commit this note and give `396d500`, and what follows it, the trailer `Data-Audit: docs/audits/2026-10-27-merge-chains.md`.

Verdict: changes needed

# Verdict of round two (`bfdea65`)

Round one's findings are closed, and so are main's X2, X3 and, as reported, X4. Nothing in `3d60818..bfdea65` loses data. But C1 leaves jaw.db inconsistent with what was done to a kept chat, and T2 leaves a rule the flatten touches untested. Before landing:
1. **C1.** Merge a going chat's settings again only where it could have been a chain's middle (its person has three JIDs or more). Say in `merge_started`'s comment what still comes back there. Test it from `caveat.db`'s scene: nothing undone comes back, and `x1-chain`'s and `ended-chain`'s draft and pin still arrive.
2. **T2.** Test the flatten's recount: from round one's I3 scene made at `4126db2`, the first chat's unread right after `Store::open`.
3. **The rebase.** Rebase onto main with the resolutions under "The trial rebase onto `e2af6a1`", `carry` built on main's `follow`.
4. **A fresh audit** of the change.

X4b and H3 to H7 can stand as follow-ups, with X5 and H1. Once approved, commit this note, and give every commit of the range that touches the data layer the trailer `Data-Audit: docs/audits/2026-10-27-merge-chains.md`.

Verdict: changes needed

# Round three's appendices

## Appendix A: the random histories (`probe.rs`)

To run them again:
- Put this file at `crates/jawd/src/sync/merge/probe.rs`, and add to the end of `crates/jawd/src/sync/merge/tests.rs`:

  ```rust
  #[path = "probe.rs"]
  mod probe;
  ```

- At the head: `PROBE_SEEDS=0..120 PROBE_AT_ONCE=6 cargo test -p jawd probe_head -- --ignored --nocapture`.
- Checked out at `4126db2`, with the same two steps: `PROBE_DIR=<folder> PROBE_SEEDS=0..240 cargo test -p jawd probe_prefixes -- --ignored --nocapture` writes each prefix and its cut.
- Back at the head: `PROBE_DIR=<folder> PROBE_SEEDS=0..240 cargo test -p jawd probe_from_prefixes -- --ignored --nocapture`.
- `probe_trace` (`PROBE_TRACE=<seed>`) prints each op of a history and every chat's read position after it. `probe_at_cut` compares what's owed at the cut, in a prefix and at the head. `probe_measure_merge` (`PROBE_RUNS`) gives the merge's costs above. `PROBE_SEEDS` also takes a list (`30,37,55`), and `PROBE_NO_PHONE_COUNTS=1` leaves the phone's counts out of the reads.
- Set `TMPDIR` to the audit's folder, since every history makes homes of its own. Take the module out again before running the plan test, which reads its SQL too.

```rust
//! The merge-chains audit's random histories, round three. Ana has four JIDs (two numbers, a LID
//! and a hosted LID) and Rui two. Each history creates their chats in a random order, joins Ana's
//! JIDs in random trees and orders (sometimes three or four at once, sometimes a join again), and
//! between the joins takes random merge steps and sends messages, reactions, edits, deletions for
//! everyone and for you, receipts, votes, phone and here reads, settings, drafts and earlier
//! groups again, to any of a person's JIDs, for messages that came or are still to come (no
//! opens: 4126db2 quarantines one by a JID merged away).
//! It ends with the messages still to come, every merge stepped to its end, everything sent again
//! and the merges stepped again.
//!
//! - `probe_head`: plays each history at the head with a check after every commit, keeps a copy
//!   of jaw.db as a kill would leave it after a random commit, plays the rest from it, and wants
//!   the same rows.
//! - `probe_prefixes` (run at 4126db2): plays each history up to a cut (a few ops after the first
//!   chain forms, or at a random op if none does, and always before a step that would end a
//!   chain's middle, which 4126db2 turns into two chats) and writes jaw.db and the cut.
//! - `probe_from_prefixes` (run at the head): opens each prefix, which flattens it, plays the rest
//!   with a check after every commit and a kill after a random commit, and compares the end with
//!   the same history played at the head from the start.
//!
//! `PROBE_SEEDS=a..b` picks the histories, `PROBE_DIR` the prefixes' folder, `PROBE_AT_ONCE` how
//! many run at once.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, params};

use crate::store::{Amend, Amendment, Change, Chat, Lane, Message, Person, Read, Store};

use super::super::waiting;

const NEW: &str = "351910000002@s.whatsapp.net";
const OLD: &str = "351910000001@s.whatsapp.net";
const LID: &str = "100000000009@lid";
const HOSTED: &str = "100000000010@hosted.lid";
const RUI: &str = "351920000003@s.whatsapp.net";
const RUI_LID: &str = "100000000011@lid";
const ANA: [&str; 4] = [NEW, OLD, LID, HOSTED];
const RUIS: [&str; 2] = [RUI, RUI_LID];
const AT: i64 = 1_792_000_000_000;
const MERGE_STEP: i64 = 200;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }
    fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i + 1);
            xs.swap(i, j);
        }
    }
}

#[derive(Clone)]
enum Op {
    Live(Vec<Change>),
    Durable(Vec<Change>),
    /// A step of the k-th merge waiting, by the chat going, or of the first.
    Step(Option<usize>),
    /// Steps until no merge waits.
    Rest,
}

fn from_them(chat: &str, id: &str, at: i64) -> Change {
    Change::Message(Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: Some(chat.into()),
        kind: "text",
        text: Some(id.into()),
        summary: id.into(),
        live: true,
        ..Message::default()
    })
}

fn mine(chat: &str, id: &str, at: i64) -> Change {
    let Change::Message(message) = from_them(chat, id, at) else { unreachable!() };
    Change::Message(Message { sender: None, mine: true, status: Some("sent"), ..message })
}

fn amend(chat: &str, target: &str, at: i64, what: Amendment) -> Change {
    Change::Amend(Amend { chat: chat.into(), target: target.into(), sender: Some(chat.into()), mine: false, at, what, raw: None })
}

fn join(jids: &[&str]) -> Change {
    Change::Person(Person { jids: jids.iter().map(|j| j.to_string()).collect(), ..Person::default() })
}

/// What a person's messages are, so far and still to come, and which of them are mine.
#[derive(Default)]
struct Said {
    came: Vec<String>,
    mine: Vec<String>,
    coming: Vec<String>,
}

/// A history, and how many of its ops come before its end (the messages still to come, the rest of
/// the merges, everything again).
pub(super) struct History {
    ops: Vec<Op>,
    core: usize,
    /// The IDs each person should end with, sorted: every message that came, but those deleted for you.
    want: [Vec<String>; 2],
}

/// Whether a change may be sent again as a resync sends it: votes are kept once for each copy,
/// settings would go back to what they were, and opens count each time.
fn again_able(change: &Change) -> bool {
    match change {
        Change::Amend(Amend { what: Amendment::Keep { .. }, .. }) => false,
        Change::Chat(_) | Change::Opened(_) => false,
        _ => true,
    }
}

pub(super) fn history(seed: u64) -> History {
    let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0x5EED);
    let mut t = AT;
    let mut tick = |by: i64| {
        t += by;
        t
    };
    let mut said = [Said::default(), Said::default()];
    let mut deleted: Vec<String> = Vec::new();
    let mut ops: Vec<Op> = Vec::new();
    let mut next = 0usize;
    let mut fresh = |prefix: &str| {
        next += 1;
        format!("{prefix}{next}")
    };
    // The chats, created in a random order, Rui's among Ana's.
    let mut order: Vec<&str> = ANA.iter().chain(RUIS.iter()).copied().collect();
    rng.shuffle(&mut order);
    let mut first: Vec<Change> = Vec::new();
    for jid in &order {
        let p = usize::from(RUIS.contains(jid));
        let size = rng.pick(&[1usize, 2, 6, 25, 140, 199, 200, 230, 410]);
        for _ in 0..size {
            let id = fresh(if p == 0 { "a" } else { "r" });
            let at = tick(1);
            if rng.chance(0.1) {
                first.push(mine(jid, &id, at));
                said[p].mine.push(id.clone());
            } else {
                first.push(from_them(jid, &id, at));
            }
            said[p].came.push(id);
        }
        if rng.chance(0.3) {
            ops.push(Op::Live(std::mem::take(&mut first)));
        }
    }
    if !first.is_empty() {
        ops.push(Op::Live(first));
    }
    // A message of Ana's both of two of her chats have, as one sent from another device.
    {
        let (a, b) = (rng.pick(&ANA), rng.pick(&ANA));
        let id = fresh("both");
        let at = tick(1);
        let mut group = vec![mine(a, &id, at)];
        if b != a {
            group.push(mine(b, &id, at));
        }
        said[0].came.push(id.clone());
        said[0].mine.push(id);
        ops.push(Op::Live(group));
    }
    // Ana's joins: a random tree over her four JIDs, sometimes three or four at once; Rui's one.
    let mut joins: Vec<Change> = Vec::new();
    if rng.chance(0.25) {
        let mut jids = ANA.to_vec();
        rng.shuffle(&mut jids);
        let k = if rng.chance(0.5) { 4 } else { 3 };
        joins.push(join(&jids[..k]));
        if k == 3 {
            joins.push(join(&[jids[3], jids[rng.below(3)]]));
        }
    } else {
        let mut group: Vec<usize> = (0..4).collect();
        let find = |group: &Vec<usize>, mut i: usize| {
            while group[i] != i {
                i = group[i];
            }
            i
        };
        while (0..4).map(|i| find(&group, i)).collect::<std::collections::BTreeSet<_>>().len() > 1 {
            let (a, b) = (rng.below(4), rng.below(4));
            if a == b {
                continue;
            }
            let (ra, rb) = (find(&group, a), find(&group, b));
            if ra != rb || rng.chance(0.15) {
                group[ra] = rb;
                joins.push(join(&[ANA[a], ANA[b]]));
            }
        }
    }
    rng.shuffle(&mut joins[..]);
    let rui_at = rng.below(joins.len() + 1);
    let mut rui = RUIS.to_vec();
    rng.shuffle(&mut rui);
    joins.insert(rui_at, join(&rui));
    // Between and around the joins, random steps and arrivals.
    let mut lives: Vec<Vec<Change>> = Vec::new();
    for joined in joins.into_iter().map(Some).chain(std::iter::once(None)) {
        for _ in 0..rng.below(9) {
            let roll = rng.below(100);
            if roll < 30 {
                ops.push(Op::Step(if rng.chance(0.5) { None } else { Some(rng.below(4)) }));
                continue;
            }
            if roll < 37 {
                let p = usize::from(rng.chance(0.3));
                let jid = if p == 0 { rng.pick(&ANA) } else { rng.pick(&RUIS) };
                let text = if rng.chance(0.4) { String::new() } else { format!("draft {}", tick(1)) };
                ops.push(Op::Durable(vec![Change::Chat(Chat { id: jid.into(), draft: Some(text), ..Chat::default() })]));
                continue;
            }
            if roll < 43 && !lives.is_empty() {
                let again: Vec<Change> = lives[rng.below(lives.len())].iter().filter(|c| again_able(c)).cloned().collect();
                if !again.is_empty() {
                    ops.push(Op::Live(again));
                }
                continue;
            }
            let mut group = Vec::new();
            for _ in 0..1 + rng.below(4) {
                let p = usize::from(rng.chance(0.3));
                let jids: &[&str] = if p == 0 { &ANA } else { &RUIS };
                let jid = rng.pick(jids);
                let at = tick(1);
                // A target: a message that came, or one still to come.
                let target = |rng: &mut Rng, said: &mut [Said; 2], fresh: &mut dyn FnMut(&str) -> String| {
                    if said[p].came.is_empty() || rng.chance(0.2) {
                        let id = fresh(if p == 0 { "fa" } else { "fr" });
                        said[p].coming.push(id.clone());
                        id
                    } else {
                        let came = &said[p].came;
                        came[rng.below(came.len())].clone()
                    }
                };
                match rng.below(14) {
                    0 | 1 => {
                        let id = fresh(if p == 0 { "a" } else { "r" });
                        if rng.chance(0.2) {
                            group.push(mine(jid, &id, at));
                            said[p].mine.push(id.clone());
                        } else {
                            group.push(from_them(jid, &id, at));
                        }
                        said[p].came.push(id);
                    }
                    2 => {
                        let id = target(&mut rng, &mut said, &mut fresh);
                        let emoji = rng.pick(&["👍", "❤️", "😮"]);
                        group.push(amend(jid, &id, at, Amendment::React { emoji: emoji.into() }));
                    }
                    3 => {
                        let id = target(&mut rng, &mut said, &mut fresh);
                        let edit = Amendment::Edit { text: Some(format!("edited {at}")), spans: None, summary: format!("edited {at}") };
                        group.push(amend(jid, &id, at, edit));
                    }
                    4 => {
                        let id = target(&mut rng, &mut said, &mut fresh);
                        group.push(amend(jid, &id, at, Amendment::Revoke));
                    }
                    5 => {
                        let id = target(&mut rng, &mut said, &mut fresh);
                        deleted.push(id.clone());
                        group.push(amend(jid, &id, at, Amendment::DeleteForMe));
                    }
                    6 => {
                        if let Some(id) = (!said[p].mine.is_empty()).then(|| said[p].mine[rng.below(said[p].mine.len())].clone()) {
                            group.push(amend(jid, &id, at, Amendment::Status { status: "read".into() }));
                        }
                    }
                    7 => {
                        let id = target(&mut rng, &mut said, &mut fresh);
                        group.push(amend(jid, &id, at, Amendment::Keep { kind: "poll_vote".into() }));
                    }
                    8 | 9 => {
                        group.push(Change::Read(Read { chat: jid.into(), at: at - rng.below(400) as i64, ..Read::default() }));
                    }
                    10 => {
                        if !said[p].came.is_empty() {
                            let id = said[p].came[rng.below(said[p].came.len())].clone();
                            group.push(Change::Read(Read { chat: jid.into(), ids: vec![id], here: true, ..Read::default() }));
                        }
                    }
                    11 | 12 => {
                        let mut chat = Chat { id: jid.into(), ..Chat::default() };
                        if rng.chance(0.5) {
                            chat.pinned = Some(if rng.chance(0.5) { at } else { 0 });
                        }
                        if rng.chance(0.4) {
                            chat.muted_until = Some(if rng.chance(0.5) { at + 9_000_000 } else { 0 });
                        }
                        if rng.chance(0.3) {
                            chat.archived = Some(rng.chance(0.5));
                        }
                        if rng.chance(0.3) {
                            chat.ephemeral = Some(rng.pick(&[0u32, 86_400, 604_800]));
                        }
                        if rng.chance(0.2) {
                            chat.marked_unread = Some(rng.chance(0.5));
                        }
                        group.push(Change::Chat(chat));
                    }
                    // The phone's own counts, as history gives them where it can't say where a
                    // chat was read. (No opens: 4126db2 quarantines one by a JID merged away, X3.)
                    _ => {
                        let phone = Some((rng.below(5) as u32, 0)).filter(|_| std::env::var("PROBE_NO_PHONE_COUNTS").is_err());
                        group.push(Change::Read(Read { chat: jid.into(), at: at - rng.below(400) as i64, phone, ..Read::default() }));
                    }
                }
            }
            if !group.is_empty() {
                lives.push(group.clone());
                ops.push(Op::Live(group));
            }
        }
        if let Some(joined) = joined {
            lives.push(vec![joined.clone()]);
            ops.push(Op::Live(vec![joined]));
        }
    }
    let core = ops.len();
    // The messages still to come, each to one of its person's JIDs.
    let mut coming = Vec::new();
    for p in 0..2 {
        let jids: &[&str] = if p == 0 { &ANA } else { &RUIS };
        for id in std::mem::take(&mut said[p].coming) {
            let at = tick(1);
            coming.push(from_them(rng.pick(jids), &id, at));
            said[p].came.push(id);
        }
    }
    if !coming.is_empty() {
        lives.push(coming.clone());
        ops.push(Op::Live(coming));
    }
    ops.push(Op::Rest);
    for group in &lives {
        let again: Vec<Change> = group.iter().filter(|c| again_able(c)).cloned().collect();
        if !again.is_empty() {
            ops.push(Op::Live(again));
        }
    }
    ops.push(Op::Rest);
    let want = [0, 1].map(|p| {
        let mut ids: Vec<String> = said[p].came.iter().filter(|id| !deleted.contains(id)).cloned().collect();
        ids.sort();
        ids.dedup();
        ids
    });
    History { ops, core, want }
}

fn reader(home: &Path) -> Connection {
    let db = Connection::open_with_flags(home.join("jaw.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db
}

fn one(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

/// What's wrong with jaw.db now, if anything: a merge into itself, a chain or circle, a merge of a
/// chat that's gone or across people or kinds, a person with two chats that aren't merging, a
/// merge record without its merge (and, with `records`, a merge without its record), or a count,
/// a newest message, unread or mentions not as their rows give them.
fn wrong(db: &Connection, records: bool) -> Vec<String> {
    let mut wrong = Vec::new();
    db.execute_batch("BEGIN").unwrap();
    let mut checks = vec![
        ("a merge into itself", "SELECT count(*) FROM chat_merges WHERE gone = keep"),
        ("a chain of merges", "SELECT count(*) FROM chat_merges m JOIN chat_merges k ON k.gone = m.keep"),
        (
            "a merge of a chat that isn't there",
            "SELECT count(*) FROM chat_merges m WHERE NOT EXISTS (SELECT 1 FROM chats c WHERE c.n = m.gone)
                OR NOT EXISTS (SELECT 1 FROM chats c WHERE c.n = m.keep)",
        ),
        (
            "a merge across people or kinds",
            "SELECT count(*) FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep
            WHERE g.person IS NOT k.person OR g.kind <> 'direct' OR k.kind <> 'direct'",
        ),
        (
            "a person with two chats that aren't merging",
            "SELECT count(*) FROM (SELECT person FROM chats c WHERE kind = 'direct' AND person IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = c.n) GROUP BY person HAVING count(*) > 1)",
        ),
        (
            "a merge record without its merge",
            "SELECT count(*) FROM sync_state WHERE key LIKE 'merge.started.%'
                AND CAST(substr(key, 15) AS INTEGER) NOT IN (SELECT gone FROM chat_merges)",
        ),
        ("a message count", "SELECT count(*) FROM chats c WHERE c.message_count <> (SELECT count(*) FROM messages m WHERE m.chat = c.n)"),
        (
            "a newest message",
            "SELECT count(*) FROM chats c WHERE NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = c.n)
                AND c.last_message IS NOT (SELECT n FROM messages m WHERE m.chat = c.n ORDER BY at DESC, n DESC LIMIT 1)",
        ),
    ];
    if records {
        checks.push((
            "a merge without its record",
            "SELECT count(*) FROM chat_merges m WHERE NOT EXISTS (SELECT 1 FROM sync_state s WHERE s.key = 'merge.started.' || m.gone)",
        ));
    }
    for (what, sql) in checks {
        let n = one(db, sql);
        if n > 0 {
            wrong.push(format!("{what} ({n})"));
        }
    }
    // Unread and mentions: messages from others after the position, counted from the kept chat's
    // when that's later for a chat being merged, plus the phone's counts while they stand.
    let chats: Vec<(i64, String, i64, i64, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>)> = db
        .prepare(
            "SELECT c.n, c.id, c.unread, c.mentions, c.read_at, c.read_n, c.phone_unread, c.phone_mentions, k.read_at, k.read_n
            FROM chats c LEFT JOIN chat_merges m ON m.gone = c.n LEFT JOIN chats k ON k.n = m.keep",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for (n, id, unread, mentions, read_at, read_n, phone_unread, phone_mentions, kept_at, kept_n) in chats {
        let own = read_at.map(|at| (at, read_n.unwrap_or(i64::MAX)));
        let (mut from, mut phone) = (own, (phone_unread.unwrap_or(0), phone_mentions.unwrap_or(0)));
        if let Some(kept) = kept_at.map(|at| (at, kept_n.unwrap_or(i64::MAX))) {
            if own.is_none_or(|own| kept > own) {
                (from, phone) = (Some(kept), (0, 0));
            }
        }
        let (fa, fb) = from.unwrap_or((i64::MIN, i64::MIN));
        let (count, mentioned): (i64, i64) = db
            .query_row(
                "SELECT count(*), coalesce(sum(mentions_me), 0) FROM (SELECT mentions_me FROM messages
                    WHERE chat = ?1 AND (at, n) > (?2, ?3) AND mine = 0 AND kind NOT IN ('system', 'call') LIMIT 1000)",
                params![n, fa, fb],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let (want, want_mentions) = ((count + phone.0).min(1000), (mentioned + phone.1).min(1000));
        if (unread, mentions) != (want, want_mentions) {
            wrong.push(format!("{id}'s unread {unread} and mentions {mentions}, where its rows give {want} and {want_mentions}"));
        }
    }
    db.execute_batch("COMMIT").unwrap();
    wrong
}

/// Everything jaw.db holds of the people's chats, a line per row, by JID and message ID rather than
/// by key. Each line starts with what it is, so parts can be compared alone.
fn digest(db: &Connection) -> Vec<String> {
    const TABLES: &[(&str, &str)] = &[
        (
            "settings",
            "SELECT c.id, c.pinned, c.archived, c.muted_until, c.marked_unread, coalesce(c.draft, ''), c.ephemeral FROM chats c ORDER BY c.id",
        ),
        ("frecency", "SELECT c.id, round(c.frecency, 6), c.opened_at FROM chats c ORDER BY c.id"),
        (
            "chat",
            "SELECT c.id, c.kind, c.unread, c.mentions, c.message_count, l.id, c.last_at, c.read_at, r.id, c.read_known, c.phone_unread
            FROM chats c LEFT JOIN messages l ON l.n = c.last_message LEFT JOIN messages r ON r.n = c.read_n ORDER BY c.id",
        ),
        ("told", "SELECT c.id, c.told_at, t.id FROM chats c LEFT JOIN messages t ON t.n = c.told_n ORDER BY c.id"),
        (
            "owed",
            "SELECT c.id, m.id FROM chats c JOIN messages m ON m.chat = c.n
            WHERE c.told_at IS NOT NULL AND m.mine = 0 AND m.kind NOT IN ('system', 'call')
                AND (m.at, m.n) > (c.told_at, c.told_n) AND (m.at, m.n) <= (c.read_at, coalesce(c.read_n, 9223372036854775807))
            ORDER BY 1, 2",
        ),
        ("merging", "SELECT g.id, k.id FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep ORDER BY 1"),
        (
            "record",
            "SELECT c.id FROM sync_state s JOIN chats c ON c.n = CAST(substr(s.key, 15) AS INTEGER) WHERE s.key LIKE 'merge.started.%' ORDER BY 1",
        ),
        (
            "message",
            "SELECT c.id, m.id, m.mine, m.at, m.status, m.deleted, m.text, m.summary, m.edited_at, m.starred
            FROM messages m JOIN chats c ON c.n = m.chat ORDER BY 1, 2",
        ),
        (
            "reaction",
            "SELECT c.id, m.id, j.jid, r.emoji, r.at FROM reactions r JOIN messages m ON m.n = r.message
                JOIN chats c ON c.n = m.chat JOIN person_jids j ON j.n = r.sender ORDER BY 1, 2, 3",
        ),
        (
            "receipt",
            "SELECT c.id, m.id, j.jid, r.delivered_at, r.read_at, r.played_at FROM receipts r
                JOIN messages m ON m.n = r.message JOIN chats c ON c.n = m.chat JOIN person_jids j ON j.n = r.recipient ORDER BY 1, 2, 3",
        ),
        ("pending", "SELECT chat, target, kind, at, effect FROM pending ORDER BY 1, 2, 3, 4, 5"),
        ("tombstone", "SELECT c.id, t.id FROM tombstones t JOIN chats c ON c.n = t.chat ORDER BY 1, 2"),
        ("quarantine", "SELECT source, ref FROM quarantine ORDER BY 1, 2"),
        ("jid", "SELECT j.jid, count(*) FROM person_jids j JOIN person_jids k ON k.person = j.person GROUP BY j.jid ORDER BY 1"),
    ];
    let mut lines = Vec::new();
    db.execute_batch("BEGIN").unwrap();
    for (table, sql) in TABLES {
        let mut statement = db.prepare(sql).unwrap();
        let columns = statement.column_count();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let values: Vec<String> = (0..columns).map(|i| format!("{:?}", row.get::<_, Value>(i).unwrap())).collect();
            lines.push(format!("{table} {}", values.join(" ")));
        }
    }
    db.execute_batch("COMMIT").unwrap();
    lines
}

fn part<'a>(digest: &'a [String], kinds: &[&str]) -> Vec<&'a String> {
    digest.iter().filter(|l| kinds.iter().any(|k| l.starts_with(&format!("{k} ")))).collect()
}

fn differ(want: &[&String], got: &[&String]) -> Option<String> {
    if want == got {
        return None;
    }
    let missing: Vec<&&String> = want.iter().filter(|l| !got.contains(l)).take(8).collect();
    let extra: Vec<&&String> = got.iter().filter(|l| !want.contains(l)).take(8).collect();
    Some(format!("missing {missing:?}, extra {extra:?}"))
}

/// A copy of jaw.db as a kill now would leave it, every commit so far and nothing more.
fn killed(home: &Path) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute("VACUUM INTO ?1", [copy.path().join("jaw.db").to_str().unwrap()]).unwrap();
    copy
}

/// Takes a step of the k-th merge waiting, or of the first; says whether one waited.
async fn step(store: &Store, k: Option<usize>) -> Result<bool, String> {
    let waiting = waiting(store).await.map_err(|e| format!("{e:#}"))?;
    if waiting.is_empty() {
        return Ok(false);
    }
    let merging = waiting[k.unwrap_or(0) % waiting.len()].clone();
    let applied = store
        .apply(Lane::Background, vec![Change::MergeStep(Some(merging.clone()))])
        .await
        .map_err(|e| format!("a step of {}: {e:#}", merging.reference))?;
    if applied.quarantined > 0 {
        return Err(format!("a step of {} was quarantined", merging.reference));
    }
    Ok(true)
}

/// How a run went.
#[derive(Default)]
struct Played {
    commits: usize,
    /// Where to go on from, and a copy of jaw.db as a kill after a commit would leave it.
    kill: Option<(usize, tempfile::TempDir)>,
    /// How far WhatsApp was told, before everything was sent again.
    told_before_again: Vec<String>,
    /// Each op sent again that moved how far WhatsApp was told: the reads it held, each with
    /// whether it named the chat that's left by its own JID (X4b's shape).
    told_moves: Vec<String>,
}

/// Plays `ops` from `from` on, checking jaw.db after every commit; with `kill_after`, keeps a copy
/// as a kill would leave it after that op (after the first step of a `Rest`).
async fn play(
    store: &Store,
    home: &Path,
    history: &History,
    from: usize,
    kill_after: Option<usize>,
    records: bool,
) -> Result<Played, String> {
    let db = reader(home);
    let mut played = Played::default();
    let first_rest = history.ops.iter().position(|op| matches!(op, Op::Rest));
    let check = |db: &Connection, i: usize, played: &Played| -> Result<(), String> {
        let wrong = wrong(db, records);
        if wrong.is_empty() { Ok(()) } else { Err(format!("after op {i}, commit {}: {}", played.commits, wrong.join("; "))) }
    };
    for (i, op) in history.ops.iter().enumerate().skip(from) {
        if Some(i) == first_rest.map(|r| r + 1) {
            played.told_before_again = part(&digest(&db), &["told"]).into_iter().cloned().collect();
        }
        match op {
            Op::Live(changes) => {
                let again = first_rest.is_some_and(|r| i > r);
                let owed: Vec<String> =
                    if again { part(&digest(&db), &["owed"]).into_iter().cloned().collect() } else { Vec::new() };
                let applied = store.apply(Lane::Live, changes.clone()).await.map_err(|e| format!("op {i}: {e:#}"))?;
                if applied.quarantined > 0 {
                    return Err(format!("op {i} was quarantined"));
                }
                played.commits += 1;
                check(&db, i, &played)?;
                let now: Vec<String> =
                    if again { part(&digest(&db), &["owed"]).into_iter().cloned().collect() } else { Vec::new() };
                // Owed before and not now, though still read: a receipt that will never go. One
                // that's unread again, the position having moved back, owes nothing rightly.
                let cancelled: Vec<&String> = owed
                    .iter()
                    .filter(|l| !now.contains(l))
                    .filter(|l| {
                        let id = l.rsplit(' ').next().unwrap().trim_matches(|c| c == '"' || c == ')').trim_start_matches("Text(\"");
                        db.query_row(
                            "SELECT (m.at, m.n) <= (c.read_at, coalesce(c.read_n, 9223372036854775807)) FROM messages m
                                JOIN chats c ON c.n = m.chat WHERE m.id = ?1",
                            [id],
                            |r| r.get::<_, Option<bool>>(0),
                        )
                        .unwrap_or(None)
                        .unwrap_or(false)
                    })
                    .collect();
                if !cancelled.is_empty() {
                    let left: Vec<String> = db
                        .prepare("SELECT id FROM chats WHERE kind = 'direct'")
                        .unwrap()
                        .query_map([], |r| r.get(0))
                        .unwrap()
                        .collect::<rusqlite::Result<_>>()
                        .unwrap();
                    let reads: Vec<String> = changes
                        .iter()
                        .filter_map(|c| match c {
                            Change::Read(r) => Some(format!(
                                "{}{}{}{}",
                                r.chat,
                                if r.here { " here" } else { " phone" },
                                if r.phone.is_some() { " with counts" } else { "" },
                                if left.contains(&r.chat) { " (own)" } else { "" }
                            )),
                            _ => None,
                        })
                        .collect();
                    played.told_moves.push(format!("op {i} cancelled {} owed: {}", cancelled.len(), reads.join(", ")));
                }
            }
            Op::Durable(changes) => {
                let applied = store.apply_durable(changes.clone()).await.map_err(|e| format!("op {i}: {e:#}"))?;
                if applied.quarantined > 0 {
                    return Err(format!("op {i} was quarantined"));
                }
                played.commits += 1;
                check(&db, i, &played)?;
            }
            Op::Step(k) => {
                if step(store, *k).await? {
                    played.commits += 1;
                    check(&db, i, &played)?;
                }
            }
            Op::Rest => {
                let mut steps = 0;
                while step(store, None).await? {
                    steps += 1;
                    played.commits += 1;
                    check(&db, i, &played)?;
                    if steps == 1 && kill_after == Some(i) {
                        played.kill = Some((i, killed(home)));
                    }
                    if steps > 100 {
                        return Err(format!("op {i}: the merges never end"));
                    }
                }
                continue;
            }
        }
        if kill_after == Some(i) {
            played.kill = Some((i + 1, killed(home)));
        }
    }
    Ok(played)
}

/// What a history must end with: no merge, one chat for each person, every message that came once
/// but those deleted for you, nothing quarantined, and nothing in pending but votes, all waiting
/// in the chat that's left. With `legacy`, rows an earlier jawd left in pending for a message its
/// chat has (X2, H5) are counted rather than failing.
fn ended(db: &Connection, history: &History, legacy: bool) -> Result<usize, String> {
    let mut wrong = Vec::new();
    if one(db, "SELECT count(*) FROM chat_merges") > 0 {
        wrong.push("a merge is left".to_string());
    }
    let chats = one(db, "SELECT count(*) FROM chats WHERE kind = 'direct'");
    if chats != 2 {
        wrong.push(format!("{chats} one-to-one chats, not one each"));
    }
    for (p, jid) in [(0, NEW), (1, RUI)] {
        let ids: Vec<String> = db
            .prepare(
                "SELECT m.id FROM messages m JOIN chats c ON c.n = m.chat
                WHERE c.person = (SELECT person FROM person_jids WHERE jid = ?1) ORDER BY m.id",
            )
            .unwrap()
            .query_map([jid], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        // With `legacy`, a message deleted for you that an earlier jawd left beside its tombstone
        // (X2) is counted, as below.
        let beside = |id: &String| {
            one(db, &format!("SELECT count(*) FROM tombstones t JOIN messages m ON m.chat = t.chat AND m.id = t.id WHERE t.id = '{id}'")) > 0
        };
        let missing: Vec<&String> = history.want[p].iter().filter(|id| !ids.contains(id)).take(8).collect();
        let extra: Vec<&String> = ids.iter().filter(|id| !history.want[p].contains(id)).filter(|id| !(legacy && beside(id))).take(8).collect();
        if !missing.is_empty() || !extra.is_empty() || ids.len() < history.want[p].len() {
            wrong.push(format!("{jid}'s person has {} messages for {}: missing {missing:?}, extra {extra:?}", ids.len(), history.want[p].len()));
        }
    }
    if one(db, "SELECT count(*) FROM quarantine") > 0 {
        wrong.push("something is quarantined".into());
    }
    let left = one(
        db,
        "SELECT count(*) FROM pending p JOIN chats c ON c.id = p.chat JOIN messages m ON m.chat = c.n AND m.id = p.target
        WHERE p.kind <> 'poll_vote'",
    );
    if left > 0 && !legacy {
        wrong.push(format!("{left} rows wait in pending for a message their chat has"));
    }
    let astray = one(db, "SELECT count(*) FROM pending p WHERE NOT EXISTS (SELECT 1 FROM chats c WHERE c.id = p.chat)");
    if astray > 0 {
        wrong.push(format!("{astray} rows wait in pending under a chat that's gone"));
    }
    let tombstoned = one(db, "SELECT count(*) FROM tombstones t JOIN messages m ON m.chat = t.chat AND m.id = t.id");
    if tombstoned > 0 && !legacy {
        wrong.push(format!("{tombstoned} messages show beside their tombstone"));
    }
    if wrong.is_empty() { Ok(left as usize + tombstoned as usize) } else { Err(wrong.join("; ")) }
}

fn seeds() -> Vec<u64> {
    let seeds = std::env::var("PROBE_SEEDS").unwrap_or_else(|_| "0..120".into());
    match seeds.split_once("..") {
        Some((a, b)) => (a.parse().unwrap()..b.parse().unwrap()).collect(),
        None => seeds.split(',').map(|s| s.trim().parse().unwrap()).collect(),
    }
}

fn at_once() -> usize {
    std::env::var("PROBE_AT_ONCE").ok().and_then(|n| n.parse().ok()).unwrap_or(4)
}

fn folder() -> PathBuf {
    PathBuf::from(std::env::var("PROBE_DIR").expect("PROBE_DIR names the prefixes' folder"))
}

/// Runs `each` for every seed, `at_once` at a time, and fails at the end with every seed that failed.
async fn every_seed<F, Fut>(name: &str, each: F)
where
    F: Fn(u64) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    let each = Arc::new(each);
    let limit = Arc::new(tokio::sync::Semaphore::new(at_once()));
    let mut runs = Vec::new();
    for seed in seeds() {
        let (each, limit) = (each.clone(), limit.clone());
        runs.push((
            seed,
            tokio::spawn(async move {
                let _turn = limit.acquire_owned().await.unwrap();
                each(seed).await
            }),
        ));
    }
    let mut failed = Vec::new();
    for (seed, run) in runs {
        match run.await {
            Ok(Ok(said)) => println!("{name} seed {seed}: ok {said}"),
            Ok(Err(e)) => {
                println!("{name} seed {seed}: FAILED {e}");
                failed.push(seed);
            }
            Err(e) => {
                println!("{name} seed {seed}: PANICKED {e}");
                failed.push(seed);
            }
        }
    }
    println!("{name}: {} of {} failed {failed:?}", failed.len(), seeds().len());
    assert!(failed.is_empty(), "{name}: seeds {failed:?} failed");
}

/// A history played at the head from the start, and from a kill after a random commit.
async fn at_head(seed: u64) -> Result<(Vec<String>, String), String> {
    let history = history(seed);
    let mut rng = Rng(seed ^ 0x0B0E);
    let kill_after = rng.below(history.ops.len());
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).map_err(|e| format!("{e:#}"))?;
    let played = play(&store, home.path(), &history, 0, Some(kill_after), true).await?;
    let db = reader(home.path());
    ended(&db, &history, false)?;
    let straight = digest(&db);
    let told_moved = part(&straight, &["told"]).into_iter().cloned().collect::<Vec<_>>() != played.told_before_again;
    let mut said = format!("{} ops, {} commits, killed after op {kill_after}", history.ops.len(), played.commits);
    if told_moved {
        said.push_str(", told moved by everything sent again");
    }
    if !played.told_moves.is_empty() {
        said.push_str(&format!(", receipts cancelled by everything sent again [{}]", played.told_moves.join("; ")));
    }
    if let Some((next, copy)) = played.kill {
        let started = Store::open(copy.path()).map_err(|e| format!("reopening after the kill: {e:#}"))?;
        play(&started, copy.path(), &history, next, None, true).await.map_err(|e| format!("after the kill: {e}"))?;
        let resumed = digest(&reader(copy.path()));
        let all: Vec<&String> = straight.iter().collect();
        if let Some(d) = differ(&all, &resumed.iter().collect::<Vec<_>>()) {
            return Err(format!("killed after op {kill_after}, it ends otherwise: {d}"));
        }
    }
    Ok((straight, said))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn probe_head() {
    every_seed("head", |seed| async move { at_head(seed).await.map(|(_, said)| said) }).await;
}

/// Whether the step `k` names now would end the merge of a chat in the middle of a chain.
async fn ends_a_middle(store: &Store, home: &Path, k: Option<usize>) -> bool {
    let waiting = waiting(store).await.unwrap();
    if waiting.is_empty() {
        return false;
    }
    let gone = waiting[k.unwrap_or(0) % waiting.len()].gone;
    let db = reader(home);
    let middle: bool = db.query_row("SELECT EXISTS (SELECT 1 FROM chat_merges WHERE keep = ?1)", [gone], |r| r.get(0)).unwrap();
    let left: i64 = db.query_row("SELECT count(*) FROM messages WHERE chat = ?1", [gone], |r| r.get(0)).unwrap();
    middle && left < MERGE_STEP
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn probe_prefixes() {
    every_seed("prefix", |seed| async move {
        let history = history(seed);
        let mut rng = Rng(seed ^ 0xC0FFEE);
        let target = history.core / 2 + rng.below(history.core - history.core / 2);
        let after_chain = 1 + rng.below(6);
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).map_err(|e| format!("{e:#}"))?;
        let mut chain_at: Option<usize> = None;
        let mut cut = history.core;
        let mut why = "the core's end";
        for i in 0..history.core {
            if chain_at.is_none() && i == target {
                (cut, why) = (i, "a random op");
                break;
            }
            if chain_at.is_some_and(|at| i == at + after_chain) {
                (cut, why) = (i, "ops after a chain formed");
                break;
            }
            if let Op::Step(k) = &history.ops[i] {
                if ends_a_middle(&store, home.path(), *k).await {
                    (cut, why) = (i, "a step that would end a middle");
                    break;
                }
            }
            match &history.ops[i] {
                Op::Live(changes) => {
                    let applied = store.apply(Lane::Live, changes.clone()).await.map_err(|e| format!("{e:#}"))?;
                    if applied.quarantined > 0 {
                        return Err(format!("op {i} was quarantined at 4126db2"));
                    }
                }
                Op::Durable(changes) => {
                    store.apply_durable(changes.clone()).await.map_err(|e| format!("{e:#}"))?;
                }
                Op::Step(k) => {
                    step(&store, *k).await?;
                }
                Op::Rest => unreachable!("the core has no rest"),
            }
            if chain_at.is_none() && one(&reader(home.path()), "SELECT count(*) FROM chat_merges m JOIN chat_merges k ON k.gone = m.keep") > 0 {
                chain_at = Some(i);
            }
        }
        let standing = one(&reader(home.path()), "SELECT count(*) FROM chat_merges m JOIN chat_merges k ON k.gone = m.keep");
        let merges = one(&reader(home.path()), "SELECT count(*) FROM chat_merges");
        let dir = folder();
        let path = dir.join(format!("{seed}.db"));
        let _ = std::fs::remove_file(&path);
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        db.execute("VACUUM INTO ?1", [path.to_str().unwrap()]).unwrap();
        std::fs::write(dir.join(format!("{seed}.cut")), format!("{cut}\n")).unwrap();
        Ok(format!(
            "cut at op {cut} of {} ({why}), chain formed at {chain_at:?}, {standing} standing, {merges} merges",
            history.core
        ))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn probe_from_prefixes() {
    every_seed("from 4126db2", |seed| async move {
        let history = history(seed);
        let dir = folder();
        let cut: usize = std::fs::read_to_string(dir.join(format!("{seed}.cut"))).unwrap().trim().parse().unwrap();
        let (head, _) = at_head(seed).await.map_err(|e| format!("at the head from the start: {e}"))?;
        let home = tempfile::tempdir().unwrap();
        std::fs::copy(dir.join(format!("{seed}.db")), home.path().join("jaw.db")).unwrap();
        let chains_before = one(&reader(home.path()), "SELECT count(*) FROM chat_merges m JOIN chat_merges k ON k.gone = m.keep");
        let store = Store::open(home.path()).map_err(|e| format!("opening the prefix: {e:#}"))?;
        let wrong_at_open = wrong(&reader(home.path()), false);
        if !wrong_at_open.is_empty() {
            return Err(format!("as the prefix opens: {}", wrong_at_open.join("; ")));
        }
        let mut rng = Rng(seed ^ 0xD1CE);
        // A kill right after the open, or after a random commit from the cut on.
        let kill_after = cut + rng.below(history.ops.len() - cut + 1);
        let open_copy = (kill_after == history.ops.len()).then(|| killed(home.path()));
        let played = play(&store, home.path(), &history, cut, Some(kill_after), false).await?;
        let db = reader(home.path());
        let legacy = ended(&db, &history, true)?;
        let got = digest(&db);
        let kill = match open_copy {
            Some(copy) => Some((cut, copy)),
            None => played.kill,
        };
        if let Some((next, copy)) = kill {
            let started = Store::open(copy.path()).map_err(|e| format!("reopening after the kill: {e:#}"))?;
            play(&started, copy.path(), &history, next, None, false).await.map_err(|e| format!("after the kill: {e}"))?;
            let resumed = digest(&reader(copy.path()));
            if let Some(d) = differ(&got.iter().collect::<Vec<_>>(), &resumed.iter().collect::<Vec<_>>()) {
                return Err(format!("killed after op {kill_after}, it ends otherwise: {d}"));
            }
        }
        // Against the same history played at the head from the start: the same rows, but what
        // 4126db2 itself did otherwise (X2's rows left in pending, X4's reads, its merges'
        // settings merged again for Ana's four JIDs, which the caveat allows).
        let same = ["message", "reaction", "receipt", "tombstone", "quarantine", "merging", "jid"];
        let mut said = format!(
            "cut at {cut}, {chains_before} chains standing, killed after op {kill_after}, {} commits",
            played.commits
        );
        if let Some(d) = differ(&part(&head, &same), &part(&got, &same)) {
            if legacy == 0 {
                return Err(format!("its rows differ from the head's: {d}"));
            }
            said.push_str(&format!(", rows differ with {legacy} left by X2 ({d})"));
        }
        let rui: Vec<&String> = part(&head, &["settings"]).into_iter().filter(|l| l.contains(RUI) || l.contains(RUI_LID)).collect();
        let rui_got: Vec<&String> = part(&got, &["settings"]).into_iter().filter(|l| l.contains(RUI) || l.contains(RUI_LID)).collect();
        if let Some(d) = differ(&rui, &rui_got) {
            return Err(format!("Rui's settings (two JIDs) differ from the head's: {d}"));
        }
        let report = [
            ("Ana's settings", &["settings"][..]),
            ("frecency", &["frecency"]),
            ("told", &["told"]),
            ("owed", &["owed"]),
            ("pending", &["pending"]),
            ("chat", &["chat"]),
        ];
        for (what, kinds) in report {
            if let Some(d) = differ(&part(&head, kinds), &part(&got, kinds)) {
                said.push_str(&format!(", {what} differ: {d}"));
            }
        }
        Ok(said)
    })
    .await;
}

fn said_of(change: &Change) -> String {
    match change {
        Change::Message(m) => format!("msg {} {}{}", m.chat, m.id, if m.mine { " mine" } else { "" }),
        Change::Amend(a) => format!("amend {} {} {:?}", a.chat, a.target, a.what),
        Change::Read(r) => format!("read {} at {} ids {:?}{}{:?}", r.chat, r.at - AT, r.ids, if r.here { " here" } else { " phone" }, r.phone),
        Change::Chat(c) => format!("chat {} {:?}", c.id, (c.pinned, c.archived, c.muted_until, c.ephemeral, &c.draft, c.marked_unread)),
        Change::Person(p) => format!("join {:?}", p.jids),
        _ => "other".into(),
    }
}

fn reads_now(db: &Connection) -> Vec<String> {
    db.prepare(
        "SELECT c.id, c.read_at - ?1, r.id, c.read_known, c.told_at - ?1, t.id, c.unread, c.phone_unread,
            (SELECT k.id FROM chat_merges m JOIN chats k ON k.n = m.keep WHERE m.gone = c.n), c.message_count
        FROM chats c LEFT JOIN messages r ON r.n = c.read_n LEFT JOIN messages t ON t.n = c.told_n ORDER BY c.n",
    )
    .unwrap()
    .query_map([AT], |r| {
        let v: Vec<String> = (0..10).map(|i| format!("{:?}", r.get::<_, Value>(i).unwrap())).collect();
        Ok(v.join(" "))
    })
    .unwrap()
    .collect::<rusqlite::Result<_>>()
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn probe_trace() {
    let seed: u64 = std::env::var("PROBE_TRACE").unwrap().parse().unwrap();
    let history = history(seed);
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let db = reader(home.path());
    for (i, op) in history.ops.iter().enumerate() {
        match op {
            Op::Live(changes) | Op::Durable(changes) => {
                let shown: Vec<String> = changes.iter().map(said_of).collect();
                let shown = if shown.len() > 6 { format!("{} changes, first {:?}, last {:?}", shown.len(), shown[0], shown[shown.len() - 1]) } else { format!("{shown:?}") };
                println!("op {i}: {shown}");
                if matches!(op, Op::Live(_)) {
                    store.apply(Lane::Live, changes.clone()).await.unwrap();
                } else {
                    store.apply_durable(changes.clone()).await.unwrap();
                }
            }
            Op::Step(k) => {
                println!("op {i}: step {k:?} -> {}", step(&store, *k).await.unwrap());
            }
            Op::Rest => {
                let mut n = 0;
                while step(&store, None).await.unwrap() {
                    n += 1;
                }
                println!("op {i}: rest, {n} steps");
            }
        }
        for line in reads_now(&db) {
            println!("    {line}");
        }
    }
}

/// For each seed, how far WhatsApp was told and what's owed, at the cut: in the prefix 4126db2
/// wrote, and in the same history played at the head up to the cut.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn probe_at_cut() {
    every_seed("at the cut", |seed| async move {
        let history = history(seed);
        let dir = folder();
        let cut: usize = std::fs::read_to_string(dir.join(format!("{seed}.cut"))).unwrap().trim().parse().unwrap();
        let prefix = Connection::open_with_flags(dir.join(format!("{seed}.db")), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let old = digest(&prefix);
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).map_err(|e| format!("{e:#}"))?;
        let short = History { ops: history.ops[..cut].to_vec(), core: cut, want: history.want.clone() };
        play(&store, home.path(), &short, 0, None, true).await?;
        let new = digest(&reader(home.path()));
        let mut said = String::new();
        for kind in ["owed", "told", "pending"] {
            let (a, b) = (part(&new, &[kind]), part(&old, &[kind]));
            let (head_only, old_only) = (a.iter().filter(|l| !b.contains(l)).count(), b.iter().filter(|l| !a.contains(l)).count());
            said.push_str(&format!("{kind}: head only {head_only}, 4126db2 only {old_only}; "));
        }
        Ok(said)
    })
    .await;
}

fn quantiles(mut xs: Vec<Duration>) -> String {
    xs.sort();
    let at = |q: f64| xs[((xs.len() - 1) as f64 * q).round() as usize].as_secs_f64() * 1000.0;
    format!("median {:.2} ms, p90 {:.2} ms, most {:.2} ms, of {}", at(0.5), at(0.9), at(1.0), xs.len())
}

/// What a big merge costs the writer, as `Applied::took` says (the transaction and its commit, and
/// for a step any wait for the checkpointer, which a pause before the join lets it catch up on):
/// the number's chat with 200 messages, the LID's with 20,000 from others, half read on the phone,
/// and 200 reactions to the number's messages sent to the LID, waiting; then the join, every step,
/// and the last. `PROBE_RUNS` runs (9).
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn probe_measure_merge() {
    let runs: usize = std::env::var("PROBE_RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(9);
    let (mut joins, mut steps, mut lasts) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..runs {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let numbers: Vec<Change> = (0..200).map(|i| from_them(NEW, &format!("n{i}"), AT + i)).collect();
        store.apply(Lane::Live, numbers).await.unwrap();
        for chunk in 0..20i64 {
            let lids: Vec<Change> =
                (0..1000i64).map(|i| from_them(LID, &format!("l{}", chunk * 1000 + i), AT + 10_000 + chunk * 1000 + i)).collect();
            store.apply(Lane::Live, lids).await.unwrap();
        }
        let read = Change::Read(Read { chat: LID.into(), at: AT + 20_000, ..Read::default() });
        store.apply(Lane::Live, vec![read]).await.unwrap();
        let reactions: Vec<Change> =
            (0..200).map(|i| amend(LID, &format!("n{i}"), AT + 40_000 + i, Amendment::React { emoji: "👍".into() })).collect();
        store.apply(Lane::Live, reactions).await.unwrap();
        assert_eq!(store.count("pending").unwrap(), 200);
        tokio::time::sleep(Duration::from_millis(2000)).await;
        let joined = store.apply(Lane::Live, vec![join(&[NEW, LID])]).await.unwrap();
        assert_eq!(store.count("pending").unwrap(), 0, "the reactions are done as the merge starts");
        joins.push(joined.took);
        loop {
            let waiting = waiting(&store).await.unwrap();
            let Some(merging) = waiting.first() else { break };
            let applied = store.apply(Lane::Background, vec![Change::MergeStep(Some(merging.clone()))]).await.unwrap();
            assert_eq!(applied.quarantined, 0);
            if store.count("chat_merges").unwrap() == 0 {
                lasts.push(applied.took);
            } else {
                steps.push(applied.took);
            }
        }
        assert_eq!(store.count("messages").unwrap(), 20_200);
    }
    println!("the join: {}", quantiles(joins));
    println!("a step of 200: {}", quantiles(steps));
    println!("the last step: {}", quantiles(lasts));
}
```

## Appendix B: main's kinds backfill and the merges (`kinds_merges.rs`)

Put this file at `crates/jawd/src/store/write/kinds/kinds_merges.rs`, add `#[path = "kinds_merges.rs"] mod kinds_merges;` to the end of `crates/jawd/src/store/write/kinds/tests.rs`, and run `cargo test -p jawd kinds_backfill_and_merges -- --ignored --nocapture`.

```rust
//! The merge-chains audit, round three: main's kinds backfill and merges under way, taken in every
//! order, from a jaw.db at version 15 holding one person's three chats with templates jawd couldn't
//! show in each (one in two of them), and under the LID encrypted edits of a message in the number's
//! chat and of one still in the LID's; with their merges as a chain an earlier jawd left (LID into
//! the old number's, that into the new number's) or as two plain merges.
use std::path::Path;

use super::*;

const NEW: &str = ANA;
const OLD: &str = "351910000004@s.whatsapp.net";
const LID: &str = "100000000009@lid";

/// An encrypted edit, from the chat's own JID, of message `target` in `chat`.
fn secret_edit_of(chat: &str, id: &str, target: &str) -> WebMessageInfo {
    let edit = e2e::SecretEncryptedMessage {
        target_message_key: Some(MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(false),
            id: Some(target.into()),
            participant: None,
        }),
        enc_payload: Some(vec![9, 9, 9]),
        secret_enc_type: Some(e2e::secret_encrypted_message::SecretEncType::MessageEdit as i32),
        ..Default::default()
    };
    web(chat, id, Some(chat), Some(e2e::Message { secret_encrypted_message: Some(edit), ..Default::default() }))
}

fn three_chats(home: &Path, chain: bool) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..15] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 15).unwrap();
    let merges = if chain { "(3, 2), (2, 1)" } else { "(3, 1), (2, 1)" };
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 0);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{NEW}', 1), (2, '{OLD}', 1), (3, '{LID}', 1);
        INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{NEW}', 'direct', 1, 0), (2, '{OLD}', 'direct', 1, 0),
            (3, '{LID}', 'direct', 1, 0);
        INSERT INTO chat_merges (gone, keep) VALUES {merges};
        INSERT INTO sync_state (key, value) VALUES ('me', '{ME}');"
    ))
    .unwrap();
    let mut rows: Vec<(i64, &str, i64, WebMessageInfo)> = Vec::new();
    for i in 0..4 {
        rows.push((1, NEW, 1, template_saying(NEW, &format!("n{i}"), &format!("{MONDAY} n{i}"))));
        rows.push((2, OLD, 2, template_saying(OLD, &format!("o{i}"), &format!("{MONDAY} o{i}"))));
    }
    for i in 0..12 {
        rows.push((3, LID, 3, template_saying(LID, &format!("l{i}"), &format!("{MONDAY} l{i}"))));
    }
    rows.push((1, NEW, 1, template_saying(NEW, "both", MONDAY)));
    rows.push((3, LID, 3, template_saying(LID, "both", MONDAY)));
    rows.push((3, LID, 3, secret_edit_of(LID, "e-n1", "n1")));
    rows.push((3, LID, 3, secret_edit_of(LID, "e-l1", "l1")));
    for (i, (chat, _, sender, message)) in rows.iter().enumerate() {
        let n = i as i64 + 1;
        db.execute(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, summary, raw, v)
            VALUES (?1, ?2, ?3, ?4, ?1, ?5, 'unknown', ?6, ?7, 0)",
            params![n, chat, message.key.id.clone().unwrap(), AT + n, sender, UNKNOWN, packed(message)],
        )
        .unwrap();
    }
    db.execute_batch(
        "UPDATE chats SET last_message = (SELECT n FROM messages WHERE chat = chats.n ORDER BY at DESC, n DESC LIMIT 1),
            last_at = (SELECT at FROM messages WHERE chat = chats.n ORDER BY at DESC, n DESC LIMIT 1),
            unread = (SELECT count(*) FROM messages WHERE chat = chats.n AND mine = 0 AND kind NOT IN ('system', 'call'))",
    )
    .unwrap();
}

/// What's wrong now: a merge into itself, a chain, or counts, newest messages and unread that their
/// rows don't give.
fn wrong(db: &Connection) -> Vec<String> {
    let mut wrong = Vec::new();
    for (what, sql) in [
        ("a merge into itself", "SELECT count(*) FROM chat_merges WHERE gone = keep"),
        ("a chain", "SELECT count(*) FROM chat_merges m JOIN chat_merges k ON k.gone = m.keep"),
        ("a message count", "SELECT count(*) FROM chats c WHERE c.message_count <> (SELECT count(*) FROM messages WHERE chat = c.n)"),
        (
            "a newest message",
            "SELECT count(*) FROM chats c WHERE NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = c.n)
                AND c.last_message IS NOT (SELECT n FROM messages WHERE chat = c.n ORDER BY at DESC, n DESC LIMIT 1)",
        ),
        // Nothing in the scene is read, so every message from others is unread.
        (
            "an unread count",
            "SELECT count(*) FROM chats c WHERE c.read_at IS NULL AND c.unread <> min(1000,
                (SELECT count(*) FROM messages WHERE chat = c.n AND mine = 0 AND kind NOT IN ('system', 'call')))",
        ),
    ] {
        let n: i64 = db.query_row(sql, [], |r| r.get(0)).unwrap();
        if n > 0 {
            wrong.push(format!("{what} ({n})"));
        }
    }
    wrong
}

async fn checked(store: &Store, home: &Path, what: &str) {
    let db = Connection::open_with_flags(home.join("jaw.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let wrong = wrong(&db);
    assert!(wrong.is_empty(), "after {what}: {wrong:?}");
    let _ = store;
}

async fn merge_step(store: &Store) -> bool {
    if store.count("chat_merges").unwrap() == 0 {
        return false;
    }
    let applied = store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    assert_eq!(applied.quarantined, 0, "a merge step failed");
    true
}

async fn backfill_step(store: &Store) -> bool {
    if store.backfill_left(KINDS).unwrap() == 0 {
        return false;
    }
    let applied = backfill(store).await;
    assert_eq!(applied.quarantined, 0, "a backfill step failed");
    true
}

/// How one run ends: each message's chat, ID, kind and text, and what waits in pending.
async fn end(store: &Store) -> Vec<String> {
    query(store, |db| {
        let mut lines: Vec<String> = db
            .prepare("SELECT c.id || ' ' || m.id || ' ' || m.kind || ' ' || coalesce(m.text, '-') FROM messages m JOIN chats c ON c.n = m.chat ORDER BY 1")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        lines.extend(
            db.prepare("SELECT 'pending ' || chat || ' ' || target || ' ' || kind FROM pending ORDER BY 1")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        );
        lines.extend(
            db.prepare("SELECT 'quarantine ' || source || ' ' || ref FROM quarantine ORDER BY 1")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        );
        Ok(lines)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn kinds_backfill_and_merges_in_every_order_end_the_same() {
    let mut ends: Vec<(String, Vec<String>)> = Vec::new();
    for chain in [true, false] {
        for order in ["the backfill first", "the merges first", "a step of each in turn"] {
            let home = tempfile::tempdir().unwrap();
            three_chats(home.path(), chain);
            let store = Store::open(home.path()).unwrap();
            let what = format!("{} merges, {order}", if chain { "chained" } else { "plain" });
            checked(&store, home.path(), &format!("{what}: opening")).await;
            let left = store.backfill_left(KINDS).unwrap();
            let mut steps = 0;
            match order {
                "the backfill first" => {
                    while backfill_step(&store).await {
                        checked(&store, home.path(), &what).await;
                    }
                    while merge_step(&store).await {
                        checked(&store, home.path(), &what).await;
                    }
                }
                "the merges first" => {
                    while merge_step(&store).await {
                        checked(&store, home.path(), &what).await;
                    }
                    while backfill_step(&store).await {
                        checked(&store, home.path(), &what).await;
                    }
                }
                _ => loop {
                    let a = backfill_step(&store).await;
                    checked(&store, home.path(), &what).await;
                    let b = merge_step(&store).await;
                    checked(&store, home.path(), &what).await;
                    steps += 1;
                    if !a && !b {
                        break;
                    }
                    assert!(steps < 200, "{what}: never ends");
                },
            }
            let ended = end(&store).await;
            let chats: Vec<String> = query(&store, |db| {
                Ok(db.prepare("SELECT id FROM chats")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
            })
            .await;
            assert_eq!(chats, [NEW], "{what}: one chat");
            assert_eq!(unshown_count(&store).await, (0, 0), "{what}: every template shown");
            println!("{what}: {left} rows for the backfill, {} lines at the end", ended.len());
            for line in ended.iter().filter(|l| l.starts_with("pending") || l.starts_with("quarantine")) {
                println!("    {line}");
            }
            ends.push((what, ended));
        }
    }
    for (what, ended) in &ends[1..] {
        assert_eq!(ended, &ends[0].1, "{what} ends otherwise than {}", ends[0].0);
    }
}
```

## Appendix C: the mutants

Each is one edit at `a2e9040`, made with `perl -0pi -e` on `crates/jawd/src/store/write.rs` (`$W`) or `crates/jawd/src/store/mod.rs` (`$M`):

```sh
mutate() {
  case $1 in
    c1-every) perl -0pi -e 's/if !started && could_chain\(db, keep\)\? \{/if !started {/' $W ;;
    c1-two) perl -0pi -e 's/Ok\(jids >= 3\)/Ok(jids >= 2)/' $W ;;
    c1-four) perl -0pi -e 's/Ok\(jids >= 3\)/Ok(jids >= 4)/' $W ;;
    t2-settle) perl -0pi -e 's/(let chats = write::flatten\(&tx, stamps\)\?;)\n\s*write::settle\(&tx, stamps\)\?;/$1/' $M ;;
    h7-guard) perl -0pi -e 's/if hop == CHAIN_MOST \{/if hop == usize::MAX {/' $W ;;
    carry-order) perl -0pi -e 's/(    follow\(db, stamps, gone\.1, keep\)\?;\n)(    for id in &deleted \{\n.*?\n    \}\n)/$2$1/s' $W ;;
    carry-no-follow) perl -0pi -e 's/    follow\(db, stamps, gone\.1, keep\)\?;/    db.prepare_cached("UPDATE pending SET chat = ?1 WHERE chat = ?2")?.execute([keep.1, gone.1])?;/' $W ;;
  esac
}
```

# Verdict of round three (`a2e9040`)

Round two's C1, T2 and H7 are closed, each by a test that fails without its fix. The rebase builds `carry` on main's `follow`, as round two asked, and `a2e9040`'s tests pin it. Nothing in `8591529..a2e9040` loses data or leaves jaw.db inconsistent, and every rule it touches is tested. My random histories, at the head and begun by `4126db2`, pass every check after every commit and across kills.

These stand as follow-ups: C1b, H8, X4b, and H3, H4, H6, X5 and H1. H5 is main's, and worth a change of its own soon, since an older jawd's merges can have left a message deleted for you showing and a deletion for everyone undone.

Commit this note, and give every commit of the range that touches the data layer the trailer `Data-Audit: docs/audits/2026-10-27-merge-chains.md`.

Verdict: approved
