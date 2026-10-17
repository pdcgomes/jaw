# Audit: sync's follow-ups (P1, H1, H2, H3 of the third sync audit)

# Third audit: c05a7e6, the second audit's I2 and T1

Range: `c221c28..c05a7e6` in `sync-followups`, and what it changes in the commits under it, 17 October 2026. Auditor: database-expert. The second and first audits are kept below as written, and this audit's verdict is the note's last line. Line numbers are at `c05a7e6`.

## Summary

I2 and T1 are fixed, and each change has a test that fails without it.
- **Every change to the kept chat reaches the chats merging into it.** That covers a read, a message of yours (live or from history, by either JID), a mark set or cleared through app state, and the start of a merge.
- **How:** `settle` now runs a second pass over the chats merging into any chat it settled. That pass runs after the kept chats' new positions and marks are written.
- **The step's order:** a merge step takes the oldest messages first, by an `ORDER BY` on the same covering index, without a sort. So the merged-away chat keeps its newest message until the last step.

Nothing marked data loss or inconsistency remains. The hygiene follow-ups below may stand.

## The second audit's items

| Item | Now | Checked |
| --- | --- | --- |
| I2, a reply or app-state mark on the kept chat | Fixed | `settle` (`write.rs:579-590`) settles what the transaction touched, then, in `settle_each`, the chats merging into each of them, read through `chats_merging_into`. Its statement is pinned, a covering search of `chat_merges_by_keep`. The second pass only recounts. It moves no read position: its `Unsettled` is empty, and the merged-away chat's own `read_at` isn't written. `upsert_chat` unsettles a chat whose row changed with `marked_unread` given (`write.rs:935-938`). |
| T1, the recount when a merge starts | Fixed, tested | `start_merge` is as on main again. The kept chat it unsettles brings the merged-away chat into the second pass. `when_a_merge_starts…` fails without that pass. |
| T1, the merged-away chat's newest after a step | Resolved | `SELECT n FROM messages WHERE chat = ?1 ORDER BY at, n LIMIT ?2` plans as `SEARCH messages USING COVERING INDEX messages_by_time (chat=?)`, without a temporary b-tree, pinned. Steps move oldest first, so the merged-away chat's newest leaves only in the last step, which removes the chat. An early move still refreshes both chats, and the half-moved test covers it. Reversing the order makes the plan test and two merge tests fail. |

The sending branch's hook points are unchanged.
- `start_merge` is identical to main's.
- `fold` is untouched.
- The last step still deletes the quarantine row, then the `chat_merges` row, then the merged-away chat.

## Findings

### Data loss

None.

### Inconsistency

None.

### Unpredictable performance

None new.
- **The second pass costs:** one covering-index search of `chat_merges` per chat a transaction settles. That table is empty except while a merge runs.
- **During a merge:** one more capped count for each chat merging into a settled one.
- The live and background budgets aren't at risk. I didn't run the store bench for this, since these are search-only statements on a table of a row or two. The third sync audit's P2 is unchanged: live writes during history over budget on a loaded Mac, and `jaw.db`'s growth after migration 6.

### Hygiene (stand as follow-ups)

These are the first audit's items that are still open.
- **H3:** a merge's quarantine row would outlive its merge if the `chat_merges` row ever went by cascade. Handle it when `deleteChat` lands.
- **H5:** `MergeStep(None)` is used only by tests.
- **H6:** the stuck count starts at 0 after a restart.
- **H7:** the third sync audit's open items.
- **Chained merges:** the second pass goes one level, from a kept chat to the chats merging into it. That's enough, since a chat being merged is never chosen as a kept chat (`merge_direct_chats`). If chains ever become possible, recount until nothing changes.

## Measurements

**Method:**
- Scratch clone `/tmp/jaw-sync-audit3` at `c05a7e6`. Every database was a tempdir.
- Debug test build with optimised dependencies.
- Scratch tests were added to the clone only, and removed afterwards.
- No `session.db` was copied or opened, and no Go engine was run.

**Test suite:** `cargo test -p jawd`, 186 passed, 1 ignored. The stopping tests passed 2 of 2.

**Mutations**, each undone afterwards:

| Change undone | Result |
| --- | --- |
| The second pass of `settle` | 4 tests fail: the half-moved test, the reply test, the unmark test, and the merge-start test |
| `upsert_chat` unsettling on `marked_unread` | The unmark test fails |
| The step's order reversed (`at DESC, n DESC`) | The plan test, the half-moved test and the reply test fail |

**Scratch tests:** these are the variants of my second-audit reproductions that the builder's tests don't cover, run in the half-moved state.

| Scratch test | Result |
| --- | --- |
| A reply of mine from history to Ana's number | Both chats 0 unread and 0 mentions |
| Only the LID chat marked before the join, then the number unmarked through app state | Both unmarked |

In the second case, after the unmark, each chat's unread, mentions, newest message, `last_at` and count equalled what its rows give: 201/20 for the number, 50/5 for the LID.



# Second audit: c221c28, the first audit's I1 and hygiene items (kept as written)

Range: `da428d5..c221c28` in `sync-followups`, and what it changes in the commit under it, 17 October 2026. Auditor: database-expert. The first audit is kept below as written, and this audit's verdict is the note's last line. Line numbers are at `c221c28`.

## Summary

I1 is fixed for the paths the builder named. Each step and each early move recounts both chats and refreshes each one's newest message. A chat being merged counts from the later of its own and its kept chat's read position. A read set on the kept chat recounts the chats merging into it. The new test fails when any of those parts is removed.

The hygiene items are done and tested: the socket's cleanup, the hourly retry of a left merge, and the backoff on the waiting-merges read.

Two other ways the kept chat changes still leave the merged-away chat wrong, so I1 isn't closed yet:
- **Replying.** Replying moves the kept chat's read position inside `settle`, without recounting the chats merging into it.
- **Clearing "marked unread".** Clearing the kept chat's "marked unread" through app state doesn't reach the merged-away chat.

I reproduced both. In a merge left halfway, each stays wrong until the merge finishes. Each attempt of a left merge fails and rolls back, so the hourly retries recount nothing. Two parts of the fix also have no test.

## The first audit's items

| Item | Now | Checked |
| --- | --- | --- |
| I1, both chats as their rows are | Fixed for steps, early moves and reads; not for replies or app-state marks (I2) | `move_merged` unsettles both chats (`write.rs:1158-1159`). `show_newest` (`write.rs:1122-1131`) runs after every step and early move. `settle` reads the kept chat's position and mark for a merging chat (`write.rs:605-621`). `set_read` calls `unsettle_merging` (`write.rs:680`). `start_merge` unsettles the merged-away chat (`write.rs:1074`). Mutations below. |
| H1, socket on a failed start | Fixed, tested | `start_on` (`main.rs:265-277`). The test fails without the cleanup. |
| H2, a left merge retried | Done | `entry.next = now + left` (1 h) at the 8th failure and after each later one. The count isn't reset, so a left merge stays counted. Each hourly failure logs one line, at most 24 a day. A step that succeeds clears the count, and the merge goes on at once. The test fails without the hourly try. |
| H3, the quarantine row after a cascade | Stands (no chat deletion yet) | — |
| H4, the waiting read's log | Fixed | 1 s doubling to 60 s, reset after a good read (`merge.rs:59-70`). |
| H5, `MergeStep(None)` | Stands | Tests only. |
| H6, the stuck count after a restart | Stands, with the builder's reason | — |

The sending branch's hook points are as the builder says.
- `start_merge` only gained `stamps.unsettle(*gone)` at its end.
- `fold` is untouched.
- The last step still deletes the quarantine row, then the `chat_merges` row, then the merged-away chat. The kept chat's newest is now refreshed through `show_newest` before them.

## Findings

### Data loss

None.

### Inconsistency

**I2. Replying in the person's chat, or unmarking it through app state, leaves the merged-away chat wrong.**

*Replying:*
- `settle` moves a chat's own read position when a message of yours arrives (`moved.mine`, `write.rs:599-601`), live or from history. A history message also moves it before the chat's read is known (`moved.history`).
- When that chat is the kept chat of a merge, nothing unsettles the chats merging into it. Only `set_read` calls `unsettle_merging`.
- So after you reply, the merged-away chat keeps counting from the kept chat's old position.
- Reproduced, in the half-moved state of the builder's own setup: 201 messages in the number chat, 50 left in the LID chat, a merge stopped by a trigger on `l200`. Then a reply of mine to Ana at `AT + 500`:

  | Reply | Number chat | LID chat |
  | --- | --- | --- |
  | Live | 0 unread, read at `AT + 500` | **50 unread**, 0 by its rows |
  | From history | 0 unread | **50 unread**, 0 by its rows |

- On the phone, replying reads the conversation. Here the merged-away half keeps its badge.

*Unmarking through app state:*
- The builder's account says a chat being merged "stays marked unread only while the kept chat does". That holds only when the merged-away chat is recounted.
- `upsert_chat` (`write.rs:859`) changes `marked_unread` without unsettling anything, so clearing the kept chat's mark through app state (`markChatAsUnread` false) never reaches the merged-away chat.
- Reproduced: the LID chat marked unread, joined, left halfway. Then `Chat { marked_unread: false }` was applied to Ana's number, and again to her LID (which resolves to the number). The number chat was unmarked. The LID chat stayed **marked unread** both times.

*Why it lasts:* in a merge that's left, the next step to recount either chat is the one that finishes the merge. Each hourly try fails and rolls back. A merge going normally fixes it within a step, but a left merge carries it for up to a whole run. This is the first audit's I1 for paths the fix didn't reach.

*Fix:*
- Have `settle` recount the chats merging into any chat it settles, after that chat.
  - Either append each settled chat's merging chats (`SELECT gone FROM chat_merges WHERE keep = ?1`, already pinned, a covering-index search) to the list being settled;
  - or only when its `read_at` or `marked_unread` changed.
  - Either way, a merging chat has to be counted after its kept chat's new position is written. The list keeps the kept chat first today; keep it that way.
- Have `upsert_chat` unsettle the chat it changes when its `marked_unread` changes, so `settle` (and the merging chats) follow.

*Tests,* all in the half-moved state:
- a reply of mine to the number, live and from history, leaves the LID chat at 0 unread;
- the number unmarked through app state, by either JID, leaves the LID chat unmarked;
- see T1 for starting a merge.

### Tests

**T1. Two parts of the fix have no test.**
- **The recount when a merge starts.** Removing `start_merge`'s `stamps.unsettle(*gone)` leaves every test passing.
  - What it does: a merged-away chat whose own read position is older than the kept chat's starts counting from the kept chat's at once.
  - Without it, the count stays old until the first step, and for a merge whose first step fails, until it finishes.
  - *Test:* the number chat read later than the LID chat's unread messages, every move failing, then the join. The LID chat's unread is counted from the number chat's position before any step.
- **The merged-away chat's newest after a step.** Removing `show_newest(gone)` in `merge_step` leaves every test passing. Steps move the oldest messages first, so the merged-away chat's newest only leaves in the last step, which deletes the chat. The line is a guard for an order SQL doesn't promise. Optional: add `ORDER BY at, n` to the step's select, if its plan stays a covering index search without a sort, and keep the guard.

### Unpredictable performance

None new. The added statements are primary-key or covering-index searches, all pinned.
- **Per step:** two `show_newest` updates, one row each by index, and one extra capped count in `settle`.
- **Per early move:** the same.
- **Per read:** one search of `chat_merges` by `keep`.

The background-step budget isn't at risk: the step's 200 moves dominate it.

### Hygiene (stand as follow-ups)

- The first audit's H3, H5, H6 and H7 stand.
- The step's `SELECT n FROM messages WHERE chat = ?1 LIMIT ?2` has no `ORDER BY` (T1).

## Measurements

**Method:**
- Scratch clone `/tmp/jaw-sync-audit3` at `c221c28`. Every database was a tempdir.
- Debug test build with optimised dependencies.
- Scratch tests were added to the clone only, and removed afterwards.
- No `session.db` was copied or opened, and no Go engine was run.

**Test suite:** `cargo test -p jawd`, 183 passed, 1 ignored. The stopping tests passed 2 of 2.

**Mutations**, each undone afterwards:

| Part undone | Result |
| --- | --- |
| `move_merged`'s `unsettle(gone)` | `a_merge_left_halfway…` fails |
| `show_newest(keep)` after a step | Same test fails |
| `show_newest` after an early move | Same test fails |
| `settle` counting a merging chat from the kept chat's position | Same test fails |
| `set_read`'s `unsettle_merging` | Same test fails |
| `start_merge`'s `unsettle(gone)` | All tests pass (T1) |
| `show_newest(gone)` after a step that isn't the last | All tests pass (T1) |
| `start_on`'s socket removal | `a_jawd_that_fails_to_start_leaves_no_socket_behind` fails |
| The hourly try (an hour instead of `left`) | `a_merge_step_that_keeps_failing…` fails |

**Scratch tests (I2):**
- Setup: the builder's half-moved state.
- Each chat was read back with its unread counted from the later of its own and its kept chat's position.
- The results are in I2's table. In every case the number chat was right and the LID chat wasn't.


# First audit: da428d5 (kept as written)

Range: `caa0fe1..da428d5` in `sync-followups`, 17 October 2026. Auditor: database-expert. One commit, `da428d5`, on `origin/main` at `caa0fe1`, taking the follow-ups of `docs/audits/2026-10-16-sync.md`. Line numbers are at `da428d5`.

Standard: the design's principle 7, "jawd › Storage", "Data integrity" and "Migrations" in `docs/plans/2026-10-16-jaw-design.md`, the review `docs/reviews/2026-10-16-data-review.md`, the earlier sync audits, and `.cursor/rules/data-integrity.mdc`.

## Summary

The four follow-ups are done as the builder describes them, and each has a test that fails without it, with one exception: removing the socket when starting fails isn't tested.
- **P1:** a merge step that fails is retried 1 s doubling to 60 s, and left until the next start after 8 failures in a row. The wait is in the merging task, outside `store.apply`, so it never holds the writer. Other merges carry on, and `jawd status` counts both kinds of merge.
- **H1:** the state during a merge, before any step, is now tested.
- **H2:** a quarantine row goes in the same savepoint as the inbox or history row it names.
- **H3:** `jaw.lock` is taken before the socket is bound.

A merge left after its failures loses nothing, and the next start finishes it correctly. But making "left until the next start" a state jawd can stay in for days brings out an older problem: while a merge is unfinished, both chats' derived values disagree with their rows. The kept chat's newest message and sort place stay as they were before the merge. The merged-away chat keeps its old unread count, and reading that chat can't clear it. This dates from the merge itself (`957b9a7`) and lasted seconds before. Now it lasts as long as a merge is left, and the rule says counters are recomputed in the transaction that changes them. Fix it before this lands (I1).

## Findings

### Data loss

None.
- A failing step is one savepoint, rolled back whole. A message is in the merged-away chat or the kept one, never in both and never in neither.
- The merged-away chat is deleted only by the step that moved its last message, in the same transaction (`write.rs:1074-1091`).
- Live copies and amendments that name a message the step can't move fail as their own group. If they came from the inbox, the inbox row stays and is converted again at the next start (scratch test below).

### Inconsistency

**I1. While a merge is unfinished, both chats' derived values disagree with their rows, and a merge that's left stays that way until jawd restarts.**
- **The kept chat.** `merge_step` updates its `last_message` and `last_at` only in the last step (`write.rs:1077-1084`). In my scratch test, after one step of 200 moved, the kept chat held messages up to `at` 1,199 but showed 500. It sorted and previewed as though nothing had moved, and stays that way until a new message arrives.
- **The merged-away chat.** `move_merged` unsettles only the kept chat (`write.rs:1122`), so the merged-away chat's `unread` is never recounted as its messages leave. In the scratch test it showed 250 unread while it held 50.
- **Reading the merged-away chat.** A read naming the merged-away chat resolves through `chat_jid` to the kept one and moves only the kept chat's position. The merged-away chat still showed 250, and nothing the owner does clears that badge until the merge finishes at the next start.
- Its `last_message` can point at a message that has moved to the kept chat, after an on-demand move of its newest.
- Readers don't consult `chat_merges` (`read.rs:299`), so the merged-away chat stays in the chat list throughout. That part is fine: it's what the list showed before the join, and no message is hidden. The stale numbers are the problem.
- This dates from `957b9a7`, and the old loop that never stopped had the same exposure. But this commit makes "left" a designed state that lasts up to a whole run, and the brief asks that a left merge show nothing wrong to a reader. The rule is "Counters are derived (triggers or recomputed in the same transaction)". `message_count` is right, kept by the triggers. `unread`, `mentions`, `last_message` and `last_at` aren't.

*Fix:*
- In `move_merged`, unsettle the merged-away chat as well as the kept one, so `settle` recounts both in the step's transaction.
- At the end of every step, and after an on-demand move, refresh both chats' `last_message` and `last_at` with their chat events. Use the statement the last step already runs, whose plan is pinned.
- Let a read that lands on the kept chat count for a chat being merged into it. For example:
  - `settle` counts a merging chat against the later of its own position and its kept chat's;
  - setting the kept chat's position unsettles the chats merging into it.

  Any equivalent that the owner can clear by reading the chat works too.

*Test:* a merge left after one step of a chat with more than 200 messages, then:
- each chat's `unread`, `mentions`, `last_message` and `last_at` equal what its rows give;
- a read naming the LID chat at its newest leaves both chats at 0 unread.

It fails today, as the scratch test shows.

### Unpredictable performance

None new. The step's added statements are primary-key and index searches at the merge's end, all pinned. I1's fix adds one capped `settle` count per step, measured at 0.35 to 0.45 ms p50 in the second audit, within the 3 ms background budget. The third audit's P2 is unchanged: live writes during history over budget on a loaded Mac, and `jaw.db`'s growth after migration 6.

### Hygiene (stand as follow-ups)

- **H1. Removing the socket when starting fails isn't tested.** If the `remove_file` in `run`'s `Err` arm (`main.rs:192-196`) is deleted, every test still passes. The builder's account says "tested". The refusal at `open` is tested, and fails if the socket is bound before the lock. Add a test that makes `start` fail (an unreadable `jaw.db`, say) and checks the socket is gone.
- **H2. A left merge waits for the next start however long jawd runs.** That's the builder's and the owner's choice, and nothing is lost. But once I1 is fixed, a slow retry would let a failure that has since cleared finish without a restart: once an hour, say, without resetting the count. Optional.
- **H3. A quarantine row for a merge outlives its merge if the `chat_merges` row ever goes by cascade.** Nothing deletes a chat today except the merge's last step. When `deleteChat` lands, delete the merge's quarantine row with it, as the last step does.
- **H4. A persistent failure to read the waiting merges logs once a second without a bound** (`merge.rs:55-58`). Back that off as well.
- **H5. `MergeStep(None)`, and its pinned `ORDER BY gone LIMIT 1` statement, are now used only by tests.**
- **H6. After a restart, `merges_stuck` starts at 0** while the left merge's quarantine row is still there, until 8 more failures (about two minutes). That's acceptable, since `status` shows the merge as being merged and the quarantine row meanwhile.
- **H7. Still open from the third audit:** P2, and H4 and H5 (history conversion's speed; kept inbox rows, fixtures, `session.db` without `fullfsync`, `STRICT` for migration 4's tables, `pending`'s growth, `clearChat` and `deleteChat`, the app-state refetch's cost).

### The rest of the commit, checked

- **The backoff.**
  - Failures 1 to 7 wait 1, 2, 4, 8, 16, 32 and 60 s; the eighth leaves the merge, about 2 minutes after the first.
  - The wait is a `timeout_at` on the task's `Notify` (`merge.rs:68-75`), outside `store.apply`, so the background lane is free while it waits.
  - A poke during the wait only reruns the cheap read. The `next <= now` filter keeps a waiting merge from running early.
  - Transient errors never count: `Writer::retrying` retries them inside the writer (`store/mod.rs:762-779`). Only failures that persist reach the 8.
  - The log is bounded at 8 lines a merge per run, plus the quarantine line each try writes.
- **Resuming.** The failure counts live only in memory, so the next start tries the merge from scratch. In my scratch test, the poison removed and a new `Store` opened, the merge finished with:
  - all 252 messages in the kept chat, counts equal to rows, unread derived;
  - the revoke made while it was left still applied, `foreign_key_check` empty;
  - the merge's quarantine row gone and `merges_stuck` at 0.
- **The quarantine row's lifetime.**
  - **A merge:** the reference is `<gone JID> into <kept JID>`, from the waiting read. It's upserted on `(source, ref)`, so there's one row however many tries. It's deleted in the transaction that deletes the `chat_merges` row, by the same JIDs recomputed there (`write.rs:1085-1089`). Chat JIDs never change, so the two always match.
  - **An inbox row:** the reference is its `seq`, which is `AUTOINCREMENT` since migration 5 and never reused. The history notification quarantined by `fetch.rs:117` uses the same source and reference, so the third audit's case is covered.
  - **A `history_raw` row:** the reference is `<chat> <id>`, and nothing writes `history_raw` any more.
  - In each case the deletion is in the savepoint that drains the row, so a group that fails keeps both the row and its quarantine.
- **Other merges carry on.** In a scratch test with Ana's merge poisoned and Rui's not, Rui's finished and Ana's was left, counted once.
- **`jaw.lock`** is still taken before `jaw.db` is opened: `open` takes it, then `start` opens the store.
- **Plans:** the new statements plan as primary-key or index searches. `waiting`'s `SCAN m` of `chat_merges` is allowed with a reason, and the plan test passes.

## Measurements

**Method:**
- Scratch clone `/tmp/jaw-sync-audit3` at `da428d5`. Every database was a tempdir.
- Debug test build with optimised dependencies, on a Mac Studio M1 Ultra.
- Scratch tests were added to the clone only, and removed afterwards.
- No `session.db` was copied or opened, and the Go engine wasn't run.

**Test suite:** `cargo test -p jawd`, 181 passed, 1 ignored. The stopping tests passed 2 of 2.

**Mutations**, each undone afterwards:

| Fix undone | Result |
| --- | --- |
| Backoff off: a quarantined step counted as success (the old loop) | `a_merge_step_that_keeps_failing…` fails ("the merge was never left") |
| Backoff with no wait (`next` = now) | Same test fails, on the waits |
| Merge's quarantine row not deleted at finish | `a_merge_that_succeeds_after_failing…` fails |
| `set_read`'s move-first removed | `during_a_merge_what_names_a_message…` fails |
| `located`'s move-first removed | Same test fails |
| `upsert_message`'s move-first removed | Same test fails |
| Inbox drain leaves its quarantine row | `a_row_kept_in_quarantine_leaves_it_once_it_converts` fails |
| `history_raw` drain leaves its quarantine row | Same test fails |
| Socket bound before `jaw.lock` | `a_jawd_refused_while_another_stops…` fails |
| Socket not removed when starting fails | All tests pass (H1) |

**Scratch test, a merge left half-moved.**

Setup:
- Ana's number chat had 1 message, and her LID chat 250 unread.
- A trigger made moving `l249`, the LID chat's newest, fail.
- The two were joined, and `merge::run` ran with 10 ms doubling to 20 ms and 3 tries.

| Moment | Kept chat (number) | Merged-away chat (LID) |
| --- | --- | --- |
| Before the join | count 1 = rows, unread 1 = derived, `last_at` 500 | count 250 = rows, unread 250 = derived |
| Left after one step and 3 failures | count 201 = rows, unread 201 = derived, **`last_at` 500** (holds up to 1,199) | count 50 = rows, **unread 250, derived 50** |
| A new LID message, a revoke of `l210`, a copy of `l249`, a read of the LID chat to 5,000 | count 203, unread 0, read at 5,000 | count 49, **unread 250, derived 49**, read position unchanged |
| Next start, poison gone | count 252 = rows, unread 0 = derived | removed |

- At the left state, one quarantine row: `merge 100000000002@lid into 351910000002@s.whatsapp.net`.
- The copy of the poison message was quarantined as its own group (`message … l249`). That row stays after the merge, since that change never applied.
- After the next start, the merge's quarantine row was gone, `l210` was revoked, `foreign_key_check` was empty, and nothing was pending.

**Scratch test, two merges:** Rui's finished while Ana's was left; `merges_stuck` was 1.

## Verdict

Changes needed. One thing before this lands:

1. **I1:** while a merge is unfinished, both chats' `unread`, `mentions`, `last_message` and `last_at` are recomputed in each step's transaction (and after an on-demand move), and a read naming the merged-away chat clears its unread. Test it with a merge left after one step of a chat with more than 200 messages.

H1 to H7 may stand as follow-ups. The next audit only needs to check I1, its test, and that nothing else moved.

Verdict: changes needed

# Verdict of the second audit (c221c28)

Changes needed. Before this lands:

1. **I2:** `settle` recounts the chats merging into each chat it settles, after that chat, and `upsert_chat` unsettles a chat whose `marked_unread` changes. Test it in the half-moved state: a reply of mine (live and from history) leaves the LID chat at 0 unread, and the number unmarked through app state (by either JID) leaves the LID chat unmarked.
2. **T1:** a test of the recount when a merge starts, which fails without `start_merge`'s `unsettle(gone)`.

The hygiene items may stand as follow-ups. The next audit only needs to check these two items, their tests, and that nothing else moved.

Verdict: changes needed

# Verdict of the third audit (c05a7e6)

Approved. The first audit's I1 and the second audit's I2 and T1 are fixed. Each change has a test that fails without it, and no data loss or inconsistency remains. These follow-ups are written down above: H3, H5, H6 and H7 of the first audit, the one-level second pass, and the third sync audit's P2.

Rebase onto `origin/main` before landing, and give every data-layer commit the trailer `Data-Audit: docs/audits/2026-10-17-sync-followups.md`.

Verdict: approved
