# Audit: the merge fix's follow-ups

# Round three: `c50a2ba3..4225199c`

Range: `c50a2ba3..4225199c` in `merge-followups`, eleven commits on `origin/main` `c50a2ba3`, which has `folders`, the keepalive fix and the owner's Mac harness, 30 October 2026. Auditor: database-expert. Line numbers are at `4225199c`, and paths are under `crates/jawd/src/` unless they say otherwise.

The worktree stayed at `4225199c`, with nothing uncommitted but this note. GitHub's `main` is still `c50a2ba3`. The range adds no migration, table, index, statement, commit or acknowledgement.

## In short

Approved. T2 and I2 are closed, each by tests that fail without the change. Nothing in the range loses data or leaves jaw.db inconsistent, and main's new code doesn't meet the read and receipt paths.

- **T2 is closed.** `ae8e9e3f` adds my S10 and S11 as tests.
  - With "told" taken as the position in `set_read`, S11's test is the only one of the 187 focused tests that fails. In `settle_each`, it's S10's.
  - My scenes and my 60 histories differ only there.
- **I2 is closed.** `4225199c`'s line is Appendix E's, with its comment reworded: the same call in the same place.
  - With the line out, the builder's window test fails in exactly 8 of its 12 variants: both openers, each with the four ways of moving `l0`, owing `l1` to `l4`. Nothing else of the 187 fails, and with the line in, all pass.
  - The line has three ways in, and each fails a test of the builder's without it: reads naming a message (2 variants), amendments through `located` (2) and copies (4). With `merge_reads` after the move instead of before, the same 8 fail.
  - My own window scenes, 51 variants, add an edit, a star, reads naming `l0` by either JID, two messages moved and a chain of three JIDs. At the head all owe what the rule says; with the line out, 36 differ.
  - A generator of mine made for the window plays 300 histories (44,344 ops). At the head they find nothing, and every kill ends as its run does. With the line out, 9 of them end a merge with 16 messages counted read and nothing owed.
- **My 60 histories at the head** find only the oracle's two (seeds 21 and 24). They send 4,783 receipt IDs in 174 lots in the first pass, as the builder says; with the line out, 4,779, round two's head's.
- **Main's new code,** the keepalive fix, changes only the connection and the catch-up in memory, and when the receipts loop tries again. Nothing in what's owed or in how far WhatsApp was told changes.
- **Plans.** Both plan tests pass on the head's own sources, and I2 adds no statement.
- **Cost.** The line runs only when a message moves out of turn.
  - History bringing copies of 1,000 of the going chat's messages takes 50.3 ms against 47.1 ms with the line out, about 3 µs a message, and a live copy's transaction stays about 1.5 ms.
  - Its walk is longest past a run of yours sent from jaw, which it walks again at each move (U2, a follow-up): copies of 3,000 of them take 0.11 s more, in transactions each still kept to 3 ms.
- **Follow-ups:** U1 and O1 to O5 stand, and U2 is new.

## What was asked, checked

### The rebase and the new commits

- `git range-diff 5230b9f4..01a550c0 c50a2ba3..4225199c` finds round two's nine commits unchanged (`=`), and `ae8e9e3f` and `4225199c` new.
- `store/write.rs` at `ae8e9e3f` is round two's head's blob (`ecdedc11`), and at `4225199c` differs from it only by I2's three lines. `store/write/outbox.rs` and `store/plans.txt` are round two's.
- `ae8e9e3f` adds two tests, my S10 and S11.
- `4225199c` adds `merge_reads(db, stamps, keep, gone)?` before `move_merged` in `moved_from_merging` (`store/write.rs:1966`), and the window test. `read_on_the_phone_and_merged` is split so the scene can start from the join.

### T2 (`ae8e9e3f`)

- **The tests are my S10 and S11,** to the message and the millisecond (`sync/merge/tests.rs:1468`, `:1489`). Each starts from X4b's merge with its receipts sent:
  - S10: her number's `n5` to `n7`, read on the phone by her number with its counts of 2, so 2 unread and nothing owed. Then her LID's `l5`, and one of yours from another device to her LID. It wants `n6`, `n7` and `l5` owed, and 0 unread.
  - S11: her LID's `l5` and `l6`, read on the phone by her LID with its counts of 2. Then her number's `n5`, and a phone read by her number past it. It wants `l5`, `l6` and `n5` owed, and 0 unread.
- **Each switch fails one test** (round two's switches on this head, Appendix G).
  - `set-from-at`, "told" while None taken as the position in `set_read`: of the 187, only S11's test fails, and of my scenes only S11 differs. My 60 histories find what the head finds, sending 3 fewer IDs, each for a message WhatsApp knew of or had had a receipt for.
  - `settle-from-at`, the same in `settle_each`: only S10's test fails, owing `l5` alone, and only my S10 differs. The histories are as the head's, with 2 fewer IDs, each for a message WhatsApp knew of.

### I2 (`4225199c`)

- **The line.** `moved_from_merging` (`store/write.rs:1956`) now runs `merge_reads(keep, gone)` before `move_merged` (`:1966`), as `merge_step` does before each step's moves (`:1736`). Appendix E applied to `ae8e9e3f` gives the head's `write.rs` but for the comment's two lines.
- **What it does.** `merge_reads` (`:1078`) puts "told" back to just before the going chat's oldest message from others that WhatsApp hasn't been told of, while the message about to move is still there to be found, and the later position wins, as at each step.
  - It never moves "told" forward: it takes the lower of where the kept chat was told and that place.
  - Between steps, the going chat's position and "told" don't change (`settle_each`'s comment, `:887-890`), and no receipts go for either chat while the merge waits (`outbox::owed`, `store/write/outbox.rs:1279-1284`). So the call puts "told" where a step at that moment would.
  - Its callers read the chat's position only after it. `set_read` reads `was` after its moves (`:991`); `upsert_message` reads nothing of it before its call (`:2130`); settle counts each chat at the transaction's end.
  - A message moved once is in the kept chat, so a replay finds nothing to move and doesn't call it.
- **Its ways in.** Every move out of turn goes through `moved_from_merging`:
  - `set_read`, for a read naming messages (`:976`);
  - `located` (`:1985`), for amendments, your own acts in the outbox and link previews;
  - `upsert_message`, for copies (`:2130`).
  A message changes chat only in `move_merged` (`:1936`), whose only other caller is the step.
- **The builder's test** (`sync/merge/tests.rs:1266`):
  - her number's chat read on the phone past `n0` to `n4`, her LID's `l0` to `l4` never read, joined;
  - one of two openers, each checked to leave "told" None, then one of six moves, with `l0`'s chat checked after it;
  - then the merge, owing `l0` to `l4`, or `l1` to `l4` after her LID's read naming `l0`.
- **The switches** (Appendix G). The builder's other tests pass with each: the 187 with `i2-out`, the merge tests with the rest.

  | Switch | What it changes | The builder's window test | My window scenes, of 51 | My window histories |
  | --- | --- | --- | --- | --- |
  | `i2-out` | the line out | 8 of 12 fail: both openers, each with a copy from history, a live copy, a reaction or a read here, owing `l1` to `l4` | 36 differ | 9 of 300, 16 messages |
  | `i2-after` | `merge_reads` after the move instead | the same 8 | 36 | 1 of 60 |
  | `i2-skip-read` | the line out for reads naming a message | 2: `l0` read here, after each opener | 9 | |
  | `i2-skip-located` | out for `located` | 2: the reaction | 9 | |
  | `i2-skip-upsert` | out for copies | 4: the two copies | 18 | |

- **My window scenes** (`audit_merge_window3`, Appendix F):
  - The openers: a phone read by her number, one of yours from another device to her number, and a phone read by her number with its counts of 0.
  - The moves: a copy of `l0` from history and a live one, a reaction, an edit, a star, `l0` read here named by either JID, a phone read naming `l0` by her LID and by her number, `l0` deleted for you, a copy of `l1`, copies of `l0` and `l1` in one transaction and in two, a copy of `l0` then a read by her number then a copy of `l1`, and nothing. And with her second LID's `m0` to `m2`, older than `l0`, in a chain of three, a live copy of `m0`.
  - At the head every variant owes what the rule says. After a read with the phone's counts, a later read is taken even if it's earlier (`set_read`'s `taken`, `:993`, main's rule). So one naming `l0` moves the position back to it: `l1` to `l4` count unread again and aren't owed, and `l0` is owed unless her LID's read told WhatsApp of it. The scene expects that.
  - With the line out, the 36 that differ are every move of `l0`, `l1` or `m0` after each opener, but her LID's read naming `l0` and the deletion.
- **My window histories** (`history_window`, Appendix F):
  - Each history reads her number's chat on the phone past its messages, sometimes with its counts; leaves her LID's never read, or read part way; and in a third, adds her second LID's, in one of three orders of joins.
  - It joins them early, then plays 30 to 80 ops while the merges wait. Most are reads by her number and yours to her number, copies of her LIDs' messages, live and from history, reactions to them, and reads here and on the phone naming them; few are steps. It ends as round one's histories do.
  - P1, P2, P4 and the kill are round one's.
  - At the head, seeds 0 to 299 (44,344 ops) find nothing, and every kill ends as its run does.
  - With the line out, 9 of the 300 end a merge with messages from her LID counted read, untold and not owed: 16 messages, at a step or the last rest.
- **My 60 histories** with the line out find what they find at the head, as round two found: they don't reach the window. They send 4 fewer IDs, each for a message WhatsApp knew of.
- **Round two's prototype and the line agree:** the same call in the same place, so round two's `i2-fix` results hold for the head. My scenes S1 to S13 owe at the head what round two's head owed, and round two's window scene owes `l0` to `l4` in the eight variants that lost `l0`.

### Main's new code

- Main's 34 commits since `5230b9f4` change, in the data layer, `sync/mod.rs` (`apply`, `cut_short`, `caught`), the failing demo engine, `engine/internal/wa/` (`keepalive.go`, `engine.go`'s connection words) and `proto/engine/engine.proto` (`KEEPALIVE_RESTORED = 11`).
- **None meets the read and receipt paths.**
  - `apply` changes only the connection and the catch-up in memory (`said_connected`, `cut_short`, the catch-up put back as `catching_up`). A restore writes nothing to jaw.db: only a `Connected` with a JID writes the account and its JIDs.
  - A restore maps to online, so `send/presence.rs` wakes `inner.online` sooner, and the receipts loop tries again sooner (`send/reads.rs:115`).
  - What's owed (`outbox::owed`) and how far WhatsApp was told (the `Told` step, after a lot is sent) are as they were, and nothing in them reads the connection.
  - The engine's changes order its connection words under one lock. None touches `MarkRead`, acks or the spool.
- Main's tests of it pass on the head: the store, sync and send tests, as below.

### Plans and cost

- **Plans.** At the head's own sources, with nothing of mine in the clone, both plan tests pass. `plans.txt` is round two's blob, and I2's line calls `merge_reads`, whose statements are pinned already: I2 adds no statement.
- **The line's cost** (Appendix F's `audit_measure_i2`, `audit_measure_i2_mine` and `audit_measure_i2_here`; numbers under Measurements):
  - It runs only when `moved_from_merging` has found the message in a chat being merged.
  - One live copy of the going chat's oldest message: 1.56 ms at the head against 1.52 ms with the line out, with 5 in the going chat, and 1.60 against 1.48 ms with 1,000. Both are within the 5 ms live budget, and the difference is within the noise.
  - History bringing copies of all the going chat's messages: 200 in 11.3 ms against 10.9 ms, and 1,000 in 50.3 ms in 16 transactions against 47.1 ms in 15, about 3 µs a message. The writer ends each background transaction at 3 ms. With the line out, the merge then owes none of them; at the head, all of them.
- **Its walk (U2).** `merge_reads` finds the going chat's oldest message from others past where WhatsApp was told of it, reading past yours.
  - Yours from another device move "told" past them, so they're not walked: copies of 3,000 of them take 152 ms against 137 ms.
  - Yours sent from jaw leave "told" before them, so each of them moved out of turn walks what's left of the run: 1,000 in 68.7 ms against 54.7 ms, and 3,000 in 233 ms, in 63 to 77 transactions, against 123 ms in 39 or 40.
  - That's about 0.025 µs a message walked, and n²/2 in all. Each transaction is still kept to 3 ms, and a single change walks at most the going chat.

### The tests

- **At the head** (its own binary, copied out before the probe went in):
  - The 187 focused tests (`sync::merge::tests::`, `store::rules`, `store::write::outbox`, `send::tests`: round two's 183, S10, S11, the window test and main's keepalive send test), 6 threads at a time at a load of about 63: 186 passed.
  - The store, sync and send tests, but the plan tests, which ran alone on the clean sources: 415 passed, 9 ignored, in 853 s at a load of about 60.
  - In both, only main's `linked_again_after_a_logout_that_cut_a_catch_up_short_…` failed ("no notification came"), as it did under load in rounds one and two. It passed alone 3 times of 3.
- **The switches' build with every switch off:** the 187 passed, my scenes and both window scenes owe as the head, and both kinds of histories find what the head finds.

## Findings

### Data loss

None.

### Inconsistency

None left: I2 is closed.

### Tests (missing)

None: T2 is closed.

### Unpredictable performance

None that breaks a budget.

- **U1 (follow-up, round one's)** stands as round two left it.
- **U2 (follow-up). I2's walk past yours sent from jaw.**
  - **Where:** `merge_reads`' search for the going chat's oldest untold message from others (`store/write.rs:1081-1088`), which now runs at each move out of turn (`:1966`).
  - **What happens:** a run of yours sent from jaw after where WhatsApp was told of the going chat is walked again at each of its messages moved out of turn. That's about 0.025 µs a message, n²/2 in all: history bringing 3,000 of them during the merge takes 0.11 s more. Each transaction stays within 3 ms, and a change walks the run once, at most the going chat's messages.
  - **Fix, if wanted:** call `merge_reads` there only when the message moving is from others (`mine = 0`, and neither a system message nor a call). Moving one of yours changes neither what the next step finds nor where the kept chat stands, so it owes the same.

### Other (follow-ups; none blocks)

- **O1 to O5** stand as round two left them.
- **The gate's timing test:** main's `linked_again_after_a_logout_…` failed under load in two of my runs, as above, and passed alone 3 times of 3.

## Measurements

Method:
- **One scratch clone** of the worktree in `/tmp/jaw-audits/merge-followups-3/jaw`, with one build folder and `CARGO_INCREMENTAL=0`.
  - It was built at `4225199c`, and that test binary was copied out. Both plan tests ran on those clean sources.
  - Then a scratch branch took round two's switches, mine for I2 (Appendix G) and the probe (Appendix F), built again as the probe grew, each binary copied out before the next build.
- **Builds** were debug, dependencies at the workspace's `opt-level = 2`, run under `nice` on an Apple M1 Ultra shared with other agents, at loads of 46 to 70.
- **Each run** took a lock folder and a timeout that kills the test binary, and its log has one start. Nothing ran twice.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, no Go engine was run, and nothing was sent to WhatsApp: every jaw.db was a test's own, in the scratch folder. I stopped no process. GitHub was asked only for `main`'s head (`git ls-remote`).

Results:
- **Tests.**

  | Build | Tests | Result | Time |
  | --- | --- | --- | --- |
  | The head, `4225199c` | the 2 plan tests, on its own sources | passed | 2 s |
  | The head | the 187 focused | 186 passed; the timing test failed, then passed alone 3 of 3 | 164 s, 6 threads, load 63 |
  | The head | `store::`, `sync::`, `send::` but the plan tests | 415 passed, 9 ignored; the same timing test failed | 853 s, 6 threads, load 60 |
  | Switches, all off | the 187 focused | 187 passed | 147 s |

- **Each switch.** "Mine" is my scenes that differ, and what my 60 histories find beyond the oracle's two.

  | Switch | The builder's tests failing, of the 187 | Mine |
  | --- | --- | --- |
  | none | none | none |
  | `set-from-at` (T2) | S11's | S11 |
  | `settle-from-at` (T2) | S10's | S10 |
  | `i2-out` (I2) | the window test, 8 of its 12 | round two's window scene: the 8; my window scenes: 36 of 51; window histories: 9 of 300 |
  | `i2-after` | the window test, 8 | window scenes: 36; window histories: 1 of 60 |
  | `i2-skip-read` | the window test, 2 (merge tests run) | window scenes: 9 |
  | `i2-skip-located` | the window test, 2 (merge tests run) | window scenes: 9 |
  | `i2-skip-upsert` | the window test, 4 (merge tests run) | window scenes: 18 |

- **The histories.**

  | Build | Histories, ops | With a finding | First pass: IDs, lots, had had one, known, neither | Kills ending otherwise |
  | --- | --- | --- | --- | --- |
  | The head (switches off) | round one's 60, 25,214 | 2, the oracle's | 4,783, 174, 155, 3,769, 859 | 0 |
  | `i2-out` | round one's 60 | 2, the oracle's | 4,779, 174, 155, 3,765, 859 | 0 |
  | `set-from-at` | round one's 60 | 2, the oracle's | 4,780, 171, 153, 3,768, 859 | 0 |
  | `settle-from-at` | round one's 60 | 2, the oracle's | 4,781, 174, 155, 3,767, 859 | 0 |
  | The head (switches off) | window, 300, 44,344 | 0 | 6,801, 532, 449, 4,783, 1,569 | 0 |
  | `i2-out` | window, 300 | 9: 16 messages out of merges | 6,628, 530, 444, 4,629, 1,555 | 0 |

  Every replay of every change at the end sent 6 IDs in 1 lot in round one's histories, and 121 in 12 in the window histories, each for a message that had had one.

- **The line's cost,** at the head and with the line out, the transaction's `Applied::took`: a live copy the median of 7, history the median of 3.

  | What | At the head | The line out |
  | --- | --- | --- |
  | A live copy of the oldest, 5 in the going chat | 1.56 ms (most 3.29) | 1.52 ms (1.92) |
  | A live copy of the oldest, 1,000 in it | 1.60 ms (most 41, one pause) | 1.48 ms (1.61) |
  | History with copies of all 200 | 11.3 ms, 4 transactions | 10.9 ms, 4 |
  | History with copies of all 1,000 | 50.3 ms, 16 | 47.1 ms, 15 |
  | Copies of 1,000 of yours from another device, oldest first / newest first | 49.5 / 48.3 ms, 15 to 17 | 41.1 / 45.2 ms, 13 to 14 |
  | Copies of 3,000 of those | 151.8 / 158.5 ms, 46 to 51 | 136.9 / 142.9 ms, 41 to 51 |
  | Copies of 1,000 of yours sent from jaw, before her `l1` | 68.7 ms, 19 to 23 | 54.7 ms, 17 to 19 |
  | Copies of 3,000 of those | 233 ms, 63 to 77 | 123 ms, 39 to 40 |

  With all of the going chat's copies, the merge ends owing all of them at the head (the first lot of at most 1,000) and none with the line out. The other cases end owing her one message either way.

# Round two: `5230b9f..01a550c0`

Range: `5230b9f..01a550c0` in `merge-followups`, nine commits on `origin/main` `5230b9f`, which has `folders`, 30 October 2026. Auditor: database-expert. Line numbers are at `01a550c0`, and paths are under `crates/jawd/src/` unless they say otherwise.

The worktree stayed at `01a550c0`, with nothing uncommitted but this note. GitHub's `main` is still `5230b9f`. The range adds no migration, table, index, commit or acknowledgement; its new statements are `several_jids`' and `came_other`'s.

## In short

Changes needed, for one untested rule (T2) and one inconsistency in main's code that this branch's class covers (I2). I1 and T1 are closed, and nothing in the range loses data.

- **I1 is closed.** `1a049da8` is Appendix B's patch with the four small changes the builder lists, and the builder's tests of it fail without it.
  - My 60 histories at the head find no message counted read that's neither owed nor told, but the oracle's two (seeds 21 and 24, history arriving below a place WhatsApp was told, which the design counts read). Round one's 182 are gone, the 41 that came out of merges among them.
  - Nothing owed was ever a message counted unread, at any of the 25,214 ops, and every kill ended as its run did.
  - My scenes S1 to S4 and S3b owe what they moved past. S7, a group, and S12, a chat whose person has one JID, owe as main does.
- **T1 is closed.** With `came_elsewhere`'s range made exclusive, the builder's S6 test fails, the only one of the 183 focused tests that does, and so does my S6.
- **T2, test gap: the phone's counts in I1's "told".** While "told" is None, I1 takes it as `told_now(was)`: where the chat was read, but before the phone's own unread messages while its counts stand.
  - Taken as the position instead, in `set_read` or in `settle_each`, every test of the builder's passes, and so do my 60 histories. Only my scenes S11 and S10 fail.
  - Then messages the phone's counts called unread, which came under another JID than the read or the message of yours that moves past them, count read with no receipt owed: I1 again, for those.
  - Fix: S10 and S11 as tests (Appendix C).
- **I2, inconsistency, in main's code: during a merge, a message moved out of turn can count read with nothing owed.**
  - A phone read by the kept chat's own JID, or one of yours from another device to it, clears "told", rightly for the kept chat's own messages.
  - Then the going chat's oldest untold message, moved early because a copy of it, a reaction to it or a read here of it came, lands below the position. The next step looks only at what's still in the going chat, and the merge ends owing the rest but not it.
  - Main's since 16 October, and round one's logic does the same. The fix is one line: `merge_reads` before an out-of-turn move, as each step does before its moves (Appendix E).
  - It's I1's class, in the code this branch exists for, and the branch needs a round for T2 anyway, so fix it here, with its scene as a test.
- **The receipts' volume is acceptable.**
  - In my histories the fix sends 7% more receipt IDs, in 33% more lots: 43 more `MarkRead`s over 25,214 ops. Of the 313 extra IDs, 126 are receipts WhatsApp needed, 146 are for messages it knew of from a read or one of yours under their own JID, and 41 are for messages that had had one. None is for a message counted unread.
  - The builder's replay-heavy histories replay every read with the phone's counts in order, which real traffic doesn't: those reads come only from history sync, a few times per conversation.
- **Mutants.** I switched the new rules off in 19 ways and the older ones in 4. Every switch that can owe less fails a test of the builder's, but the two of T2 and one of O3, `came_other`'s top end, which can only fail to owe the message the read itself names. The rest that survive can only owe more, or change nothing.
- **`folders`' merge changes don't meet the read and receipt paths,** and the rebase is clean: `git range-diff` finds round one's seven commits unchanged.
- **Plans and cost.** Both new statements are pinned, and the plan test passes on the head's sources. The walk I1 adds is about 0.15 to 0.19 µs a message: a read, or one of yours, past 20,000 unread with nothing owed takes 3.6 to 3.9 ms in all, within the 5 ms live budget (U1).

## What was asked, checked

### I1 (`1a049da8`)

- **The code is Appendix B's.**
  - `set_read` (`store/write.rs:994-1016`): in a one-to-one chat whose person has more than one JID (`several_jids`, `:1038`), a phone read speaks only if no message from others between "told" and the read came under another JID than the read's (`came_other`, `:1049`). While "told" is None it's taken as `told_now(was)`, and kept there when the read doesn't speak.
  - `settle_each` (`:910-919`): in such a chat, while "told" is None and the position has moved, "told" becomes `told_now(was)` when one of yours came under another JID (`mine_by_another`) or `came_elsewhere` finds a message from another JID between.
  - The four changes:
    - `set_read`'s comment now says a read by a JID merged away speaks for what came under it;
    - `by`, the read's JID, is bound once;
    - the let-chain asks `several_jids` only once the position is known to have moved somewhere;
    - `several_jids`' SQL is over two lines, which the plan test reads as one.
    None changes what's owed.
- **Groups and single-JID chats are as on main.**
  - `several` is false there, so `speaks` is `own`, main's rule, and `settle_each`'s new block doesn't run.
  - `came_elsewhere`, which `set_read` no longer calls for them, answered false there anyway: a group has no person, and a person with one JID has no other.
  - My S7 and S12 owe as main does, the builder's group test passes, and a phone read in a single-JID chat past 20,000 unread takes 0.37 ms, with no walk.
- **My 60 histories at the head** (Appendix A's, with round two's counts, Appendix C):
  - P2 found two messages, both history arriving counted read:
    - seed 21's is round one's oracle case: Ana's three chats not yet joined, and history under her number older than a read with the phone's counts of 5, which the phone had when it counted;
    - seed 24's is history under her number arriving below where WhatsApp was told, after receipts and a send from here moved "told".
    The design counts history below the position read, whatever moved it, and neither jaw nor the oracle can know what the phone knew of it (O4).
  - With the whole fix switched off (`fix-out`, round one's logic), the same histories find 184, as round one did: 141 read past with nothing owed, 41 out of merges, and the oracle's 2.
  - P1 and P4 held at every op. What `outbox::owed` gave was only messages from others counted read, the phone's own unread left out, exactly as an independent query of the rows gives it; and unread was as its rows give it. With `owed`'s bound off (`bound-off`), P1 fails 219 times, so it has teeth.
  - Each history was killed before a random op, by `VACUUM INTO`, and played on from the copy. All 60 ended with the same rows and receipts as the run without the kill.
- **The scenes** (Appendix C) each check what the head owes:
  - S1 to S4 and S3b owe what they moved past, and each scene's other side, past only the read's own JID's messages, owes nothing;
  - S13, a phone read by her LID past only what came under it after X4b's merge, owes nothing;
  - S5, round one's O1, owes as it did.

### T1 (`01a550c0`)

The builder's test is my S6. With `came_elsewhere`'s top end made `<` (`range-high`), it's the only one of the 183 focused tests that fails, and my S6 differs; the histories don't notice. At the head, both pass.

### What's still untested: the phone's counts in "told" (T2)

- **The rule.** While "told" is None, WhatsApp knows as much as jaw: up to the position, but not the phone's own unread messages while its counts stand. That's `told_now(was)` (`:781`), which I1 takes as "told" in `set_read` (`:995-998`) and in `settle_each` (`:915`).
- **Untested.** Each taken as the position itself, `was.at`:
  - in `set_read` (`set-from-at`), every one of the 183 focused tests passes, the builder's 24 histories among them, and my 60 histories find nothing. Only S11 differs:
    - her LID's `l5` and `l6`, read on the phone by her LID with its counts of 2, so unread there, and nothing owed;
    - then her number's `n5`, and a read on the phone by her number past it;
    - the head owes `l5`, `l6` and `n5`. The mutant owes nothing, though the read by her number says nothing of what came under her LID.
  - in `settle_each` (`settle-from-at`), the same. Only S10 differs:
    - her number's `n5` to `n7`, read on the phone by her number with its counts of 2;
    - then her LID's `l5`, then one of yours from another device to her LID;
    - the head owes `n6`, `n7` and `l5`. The mutant owes `l5` alone, though one of yours to her LID says nothing of her number's messages.
  - The histories don't reach either: each mutant changed 2 or 3 receipts in all, every one for a message WhatsApp knew of.
- **Fix:** S10 and S11 as tests. Appendix C has both, in the scenes.

### During a merge: a message moved out of turn (I2)

- **The scene** (`audit_merge_window`, Appendix C):
  - Ana's number's chat read on the phone past `n0` to `n4`, her LID's `l0` to `l4` never read, then her two JIDs joined, so the LID's chat starts merging into the number's.
  - The merge's start (`merge_reads`, `store/write.rs:1078`) puts "told" just before `l0`, so `l0` to `l4` are owed once they've moved.
  - Before the first step, a phone read by her number, or one of yours from another device to her number, reads the kept chat. It speaks, rightly: nothing from another JID lies in the kept chat between "told" and it, since `l0` to `l4` are still in the chat going. "Told" is None.
  - The chat going counts from the kept chat's position, so `l0` to `l4` now count read. What's owed for them is kept by the going chat's own "told", until they move.
  - Then, before the next step, something names `l0`: a copy of it, live or from history, a reaction to it, or a read here of it. `moved_from_merging` (`:1956`) moves it into the kept chat out of turn, through `move_merged` (`:1917`), which leaves "told" as it is. `l0` lands below the position, with "told" None.
  - The next step's `merge_reads` (`:1736`) puts "told" before the oldest message from others still in the chat going, `l1`. The merge ends owing `l1` to `l4`, and `l0` counts read with nothing owed.
- **How much.**
  - 8 of the scene's 12 variants lose `l0`: two openers, each with four ways to move it.
  - In the other four, `l0` isn't lost: with nothing moved, all five are owed; and her LID's own read naming `l0` tells WhatsApp of it, as the rule says, so owing `l1` to `l4` is right.
  - Only the going chat's oldest untold messages go missing this way: for one moved from above the oldest left, "told" goes back to that oldest one, which covers it.
  - History bringing her LID's conversation again while its chat merges could move all of them.
  - My 60 histories don't reach it: with the fix, they send 4 more receipts, each for a message WhatsApp knew of, and find nothing new.
- **Whose.** Main's. `moved_from_merging` came with sync's second audit on 16 October (`e836e50e`), and with all of I1 switched off the scene loses `l0` in the same 8 variants. The range writes none of `moved_from_merging`, `move_merged`, `merge_reads` or `merge_step`.
- **Why it's asked for here.**
  - It's I1's class: receipts a merge leaves owed that then go missing, in the code this branch exists to fix.
  - The fix is one line, and the branch needs a round for T2 anyway.
  - Merge-chains folded in main's X2 to X4 the same way.
- **Fix** (Appendix E): `moved_from_merging` runs `merge_reads(keep, gone)` before it moves the message, as `merge_step` does before each step's moves (`:1733-1736`).
  - With it, all eight owe `l0` to `l4`, and her LID's read naming `l0` still owes `l1` to `l4`.
  - The 183 focused tests pass, and the 60 histories find what they find at the head.
  - `merge_reads`' statements are pinned already, and it runs only while a merge waits.
  - Test it with the window scene.

### The receipts' volume

- **What the design says.**
  - A receipt sent twice is harmless and one skipped is lost (`merge_reads`' comment, `store/write.rs:1073-1077`).
  - The receipts go at once, then at most every 2 s per chat (`protocol/README.md:222`, `read_every` in `send/mod.rs:79`).
  - A one-to-one chat's lot is one `MarkRead`, with up to 1,000 IDs (`send/reads.rs:160-171`, `RECEIPTS_AT_ONCE`).
- **In my 60 histories,** the fix against round one's logic (`fix-out`):
  - IDs sent went from 4,472 to 4,785 (+7%), in 175 lots rather than 132 (+33%): 43 more `MarkRead`s over 25,214 ops.
  - Of the 313 more IDs, 126 were for messages WhatsApp hadn't been told of (I1's own, which it needed), 146 for messages it knew of from a phone read or one of yours under their own JID, and 41 for messages that had had a receipt.
  - None was for a message counted unread: P1 held at every op.
  - Their replay of every change sent 6 IDs in one lot at the head, and none with the fix out.
- **The builder's 24 histories** sent 3,749 and then 6,305: +26% in the first pass, and 563 to 2,284 in the replay. I didn't count them again, but the cause the builder gives matches the code. A phone read with counts is taken even when it's earlier (`set_read`'s `taken`, `:993`), so replaying every read in order moves the position back and forth, and each move forward that doesn't speak owes everything from where the chat was read before.
- **Is the replay-heavy case realistic? No.**
  - A read with the phone's counts comes only from history sync: a conversation whose unread count is more than the messages its chunk brought (`convert/history.rs:346-354`).
  - App state's `markChatAsRead` carries no counts and can't move the position back (`convert/app_state.rs:50-66`).
  - So a conversation's counts come a few times at most, mostly at link time, and the phone's later reads come without them.
  - Replaying every read with counts, in order and between forward reads, is a stress test, not traffic. The first pass's +7% to +26% more IDs is the realistic figure.
- **WhatsApp's side.**
  - A read receipt for a message already read changes nothing for its sender, whose ticks are blue already.
  - The extra lots go as every lot does: at most one per chat every 2 s, with up to 1,000 IDs each.
  - What would show is a receipt for a message the owner hasn't read, and none goes.
- **My judgement: acceptable.** It's the design's trade, bounded per chat by the 2 s pacing, and the form that keeps X4 as it was sends more (round one: 5,006 against 4,785).

### `folders` on main

- **What it changed in `store/write.rs`** (`7262a15..5230b9f`):
  - `settle` now ends with `communities_follow` (`:844`, `:850-864`), which gives a community a new version when one of its groups changed;
  - `upsert_chat` gives new versions to the community a group leaves and to a person's roster row;
  - `join` refiles folder items, giving the person's chats new versions;
  - `start_merge` moves favourite slots and decays the two chats' frecency, before `merge_reads` as before;
  - `upsert_message` keeps `last_at` the newest.
- **None meets the read and receipt paths.**
  - None of these reads or writes `read_at`, `told_at`, the phone's counts or `marked_unread` beyond what it did, or calls `settle` or `unsettle` anywhere new.
  - None changes `person_jids` or `chat_merges`, which `several_jids` and `came_other` read. `store/write/folders.rs` only reads `person_jids`.
  - `communities_follow` runs after `settle_each`, and a one-to-one chat has no community.
- **The tests.** At the head, the store, sync and send tests pass: 405, with 9 ignored, and the 2 plan tests on the head's own sources.

### Plans and cost

- **Plans.** Both new statements are in `plans.txt`, and the plan test passes on the head's sources.
  - `several_jids`: `SEARCH person_jids USING COVERING INDEX person_jids_by_person (person=?)`, with a scalar subquery on `chats`' primary key.
  - `came_other`: `SEARCH messages USING INDEX messages_by_time (chat=? AND at>? AND at<?)`, with a scalar subquery on `person_jids`' unique index for the read's JID, run once. No temporary b-tree, no `OFFSET`.
- **The walk** (U1).
  - `came_other`, and `came_elsewhere` in `settle_each`, read each message from others between "told" and the read, or the message of yours, until one came under another JID. They read the whole range when none did.
  - That's about 0.15 µs a message alone: 3.08 ms over 20,000.
  - With nothing owed, I1 now walks what the read or the message of yours moves past: past 20,000 unread, 3.62 ms in all for a phone read and 3.91 ms for one of yours, against 0.37 and 0.58 ms in a single-JID chat, which doesn't walk.
  - That's within the 5 ms live budget up to about 25,000 unread from one person under one of her JIDs, read at once. That's far more than a one-to-one chat holds unread in practice, so U1 stays a follow-up.

### The rebase onto `5230b9f`

- `git range-diff dec0fe1..9316ec69 5230b9f..51d6fb9d` finds each of round one's seven commits unchanged (`=`).
- `1a049da8` is Appendix B's patch with the four changes above, and `01a550c0` adds only S6's test.
- `scripts/check-data-audit.py 5230b9f 01a550c0` names eight commits for the trailer, all but `51d6fb9d`, which changes only `protocol/README.md`.

## Findings

### Data loss

None.

### Inconsistency

- **I2. During a merge, a message moved out of turn can count read with nothing owed.** Main's code; asked for in this branch.
  - **Where:** `moved_from_merging` (`store/write.rs:1956-1968`) moves a message from the chat going into the kept chat through `move_merged` (`:1917-1946`), neither of which touches "told". `merge_reads`, which keeps "told" before the going chat's oldest untold message, runs only at the merge's start and at each step (`:1736`, `:1756`).
  - **What happens:** a phone read by the kept chat's own JID, or one of yours from another device to it, clears "told" mid-merge, rightly for the kept chat's own messages. A copy of the going chat's oldest untold message, a reaction to it, or a read here of it then moves it below the position, and the merge ends without owing it: 8 of the window scene's 12 variants.
  - **Why it's an inconsistency:** as I1's, jaw.db says WhatsApp knows of a read that, by the branch's rule, it can't know of. No message is lost, and unread is as its rows give it.
  - **Fix:** Appendix E, with the window scene as its test.

### Tests (missing)

This blocks approval, as T1 did.

- **T2. The phone's counts in I1's "told" are untested.**
  - **Where:** `set_read` (`store/write.rs:995-998`) and `settle_each` (`:915`) take "told" as `told_now(was)` while it's None.
  - **What happens:** with either taken as the position instead, every test of the builder's passes, and so do my 60 histories. My S11 and S10 fail, each leaving the phone's own unread messages that came under another JID counted read with nothing owed.
  - **Fix:** S10 and S11 as tests (Appendix C).

### Unpredictable performance

None that breaks a budget.

- **U1 (follow-up, round one's).** The walk now runs while nothing is owed too, but only in one-to-one chats whose person has more than one JID: about 0.18 µs a message, 3.6 to 3.9 ms past 20,000 unread. A single-JID chat no longer walks at all. `settle_each`'s new block has the same `several_jids` guard; its known block, `ba69c43a`'s, still walks while something is owed, in single-JID chats too, as round one measured.

### Other (follow-ups; none blocks)

- **O1** (round one's): the phone's counts taken as the merged chat's newest. It stands.
- **O2** (round one's, with round two's): mutants that can only owe more, or change nothing.
  - Round one's three still stand.
  - `came_other`'s lower end made `>=` (`other-low`), and without `mine = 0` (`other-mine`) or the kinds filter (`other-kinds`), a read speaks less often and so owes more.
  - `IS NOT` made `<>` (`other-null`) is the same here: every sender, and every read's JID, has a `person_jids` row (`ensure_jid`, `store/write.rs:1268`; the converter sets a one-to-one message's sender to the JID it came under, `convert/mod.rs:125-133`).
  - In `settle_each`'s block, without `now.at != was.at` (`settle-moved-off`), "told" is kept only where nothing comes to be owed. Without `several_jids` (`settle-guard-off`), groups and single-JID chats walk to find nothing.
- **O3. `came_other`'s top end is untested.**
  - With `<` (`other-high`), everything of the builder's passes and only my S8 differs: a read by her number that names `l5`, which came under her LID and is the only such message in the range.
  - The head owes `l5` again. The mutant counts it told.
  - Since the read names it, the phone read it, so nothing WhatsApp wasn't told of is lost either way. A test of S8 pins the inclusive end if wanted.
- **O4. History arriving below where WhatsApp was told counts read with nothing owed,** whatever moved "told" (seed 24). That's the design's rule for history below the position; what the phone knew of it can't be known from what jaw holds.
- **O5. Where a merged chat's receipts go.** Every receipt for a one-to-one chat goes to the kept chat's JID (`send/reads.rs:162-171`), so one for a message that came under her LID is addressed to her number. Whether WhatsApp passes that on to her is for a real account to show. It's outside the range, and nothing in this audit sent anything to WhatsApp.
- **The gate's timing tests.** Three focused runs had one failure each in tests none of their switches touches: `the_step_refuses_a_reaction_in_each_state_…` (`other-low`), `a_restart_gives_a_message_waiting_for_a_receipt_…` (`other-null`) and main's `linked_again_after_a_logout_…` (`settle-elsewhere-off`). The first two passed alone twice each with their switch on; the third is round one's.

## Measurements

Method:
- **One scratch clone** of the worktree in `/tmp/jaw-audits/merge-followups-2/jaw`, with one build folder and `CARGO_INCREMENTAL=0`. It was built at `01a550c0` and then on a scratch branch with my probe and the switches, and each test binary was copied out before the next build.
- **Builds** were debug, dependencies at the workspace's `opt-level = 2`, run under `nice` on an Apple M1 Ultra shared with other agents, at loads of 52 to 70.
- **The probe** is round one's (Appendix A), with round two's changes (Appendix C): the receipts counted by lot and by what WhatsApp knew, the first pass apart from the replay; the scenes checked against what the head owes; the merge window's scene; and the walks while nothing is owed.
- **The switches** were runtime switches in a scratch commit, read from `JAW_AUDIT_MUT` (Appendix D), so one build served every mutant. Each was run against my scenes, the 183 focused tests (`sync::merge::tests::`, `store::rules`, `store::write::outbox`, `send::tests`) six threads at a time, and my 60 histories, four at a time. With every switch off, that build passed all three.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, no Go engine was run, and nothing was sent to WhatsApp: every jaw.db was a test's own, in the scratch folder. I stopped no process. Each runner took a lock folder, so none ran twice.
- **A slip of mine:** the head's first suite run read my probe, which I had added to the clone while it ran, and the plan test, which reads the sources, failed on the probe's SQL. I ran the plan test again with the head's sources checked out, and it passed.

Results:
- **Tests.**

  | Build | Tests | Result | Time |
  | --- | --- | --- | --- |
  | The head, `01a550c0` | `store::`, `sync::`, `send::` | 405 passed, 9 ignored; the statements' plan test failed on my probe's SQL, as above | 798 s, 6 threads, load about 60 |
  | The head's sources | the 2 plan tests | passed | 8 s |
  | Switches, all off | the 183 focused, my scenes, my 60 histories | all passed; the histories as the head | 469 s |

- **Each part switched off.** "Mine" is my scenes that differ, and how many of my 60 histories find something beyond the oracle's 2.

  | Switch | What it changes | The builder's tests failing | Mine |
  | --- | --- | --- | --- |
  | `fix-out` | I1's fix out, both parts | S1, S2, S3, S3b, S4, the LID's read, the 24 histories | S1 to S4, S3b, S8, S10, S11, S13; 28 histories, 182 messages |
  | `set-read-out` | `set_read`'s part out | S1, S2, the LID's read, the 24 | S1, S2, S8, S11, S13; 12 |
  | `set-told-none` | `set_read` doesn't take "told" while it's None | S1, S2, the 24 | S1, S2, S8, S11; 12 |
  | `set-not-kept` | "told" not kept when a read doesn't speak | S1, S2, the 24 | S1, S2, S8, S11; 12 |
  | `set-x4-kept` | a read by a JID merged away never speaks | S2, the LID's read | S2a, S13 |
  | `set-several-all` | `set_read` without the `several_jids` guard | the group's | S7 |
  | `set-from-at` | `set_read`'s "told" while None the position (T2) | none | S11 |
  | `other-own-chat` | `came_other` against the chat's own JID, not the read's | S2, the LID's read, the 24 | S2, S2a, S13 |
  | `other-high` | `came_other`'s top end `<` (O3) | none | S8 |
  | `other-low` | its lower end `>=` (O2) | none | none |
  | `other-mine` | without `mine = 0` (O2) | none | none |
  | `other-kinds` | without the kinds filter (O2) | none | none |
  | `other-null` | `<>` for `IS NOT` (O2) | none | none |
  | `settle-out` | `settle_each`'s part out | S3, S3b, S4, the 24 | S3, S3b, S4, S10; 22 |
  | `settle-flag-off` | its block without `mine_by_another` | S3b | S3b; 1 |
  | `settle-elsewhere-off` | its block without `came_elsewhere` | S3, S4, the 24 | S3, S4; 9 |
  | `settle-from-at` | its block's "told" the position (T2) | none | S10 |
  | `settle-moved-off` | its block without `now.at != was.at` (O2) | none | none |
  | `settle-guard-off` | its block without `several_jids` (O2) | none | none |
  | `range-high` | `came_elsewhere`'s top end `<` (T1) | S6 | S6 |
  | `known-off` | `settle_each`'s known block off (`ba69c43a`) | 8, the 24 among them | S3, S3b, S4, S6, S10; 38 |
  | `flag-off` | `mine_by_another` never set | 3, the 24 among them | S3b; 2 |
  | `bound-off` | `owed`'s bound off (`99f237d7`) | 3, the 24 among them | S5; 5, and 219 P1 failures |
  | `i2-fix` | Appendix E's patch | none | none; the window scene owes `l0` |

- **The receipts** in my 60 histories:

  | Build | IDs sent: first pass, replay | Lots | For a message that had had one | Known to WhatsApp without one | Neither |
  | --- | --- | --- | --- | --- | --- |
  | Round one's logic (`fix-out`) | 4,472, 0 | 132 | 120 | 3,619 | 733 |
  | The head | 4,779, 6 | 175 | 161 | 3,765 | 859 |
  | With Appendix E's patch | 4,783, 6 | 175 | 161 | 3,769 | 859 |

- **The walk,** her number's chat with *n* messages from her number past "told", none from another JID; the statements alone 15 times on a side connection, the transactions 3 times each (`Applied::took`):

  | What | *n* | Her JIDs | Median | Most |
  | --- | --- | --- | --- | --- |
  | `came_elsewhere` alone | 2,000 | 2 | 0.31 ms | 0.76 ms |
  | `came_elsewhere` alone | 20,000 | 2 | 3.21 ms | 4.00 ms |
  | `came_other` alone | 2,000 | 2 | 0.30 ms | 10.9 ms, one pause at load 65 |
  | `came_other` alone | 20,000 | 2 | 3.08 ms | 73 ms, likewise |
  | A phone read, something owed | 20,000 | 2 / 1 | 3.45 / 0.46 ms | |
  | A phone read, nothing owed | 1,000 | 2 / 1 | 0.59 / 0.41 ms | 0.93 / 1.26 ms |
  | One of yours, nothing owed | 1,000 | 2 / 1 | 0.82 / 0.58 ms | 1.76 / 0.63 ms |
  | A phone read, nothing owed | 20,000 | 2 / 1 | 3.62 / 0.37 ms | 3.73 / 1.52 ms |
  | One of yours, nothing owed | 20,000 | 2 / 1 | 3.91 / 0.58 ms | 3.92 / 0.67 ms |

# Round one: `dec0fe1..9316ec69`

Range: `dec0fe1..9316ec69` in `merge-followups`, seven commits, 29 October 2026. Auditor: database-expert. Line numbers are at `9316ec69`, and paths are under `crates/jawd/src/` unless they say otherwise.

The worktree stayed at `9316ec69`, with nothing uncommitted but this note. `origin/main` is `7262a15`, eleven test-only commits past the branch's base. The range adds no migration, table, index, commit or acknowledgement.

## In short

Changes needed, for one inconsistency (I1) and one untested boundary (T1). Everything the range sets out to fix is fixed, and nothing in it loses data.

- **What the range fixes is right, and tested.** X4b, `b2cd82f2`'s bound on what's owed, `c0f2dd3c`'s siblings of X4b, H1, H3, H4, H40 and H43 each do what their commits say. I switched off each part in 18 ways. Every switch fails a test of the builder's but four: three of those can only owe more or change nothing, and the fourth is T1.
- **My 60 random histories** give one person a number and two LIDs: 25,214 ops in all, each checked after every op, and each history killed and played on from a copy.
  - At the head, nothing owed was ever a message counted unread, the phone's own unread included. Unread was always as its rows give it, and every kill ended as the run without it did.
  - At the base, `dec0fe1`, the same histories owe messages the phone's counts call unread in 2 of them. 134 receipts owed were cancelled, or passed, by a read or one of yours under another JID than the message's, and 125 more messages came out of merges counted read and not owed. The range closes the first two kinds, and 84 of the 125.
- **I1 is the builder's noted case, wider than the note says: an inconsistency of X4b's class, not data loss.**
  - While nothing is owed, a phone read, one of yours from another device, or history, under any of the person's JIDs, moves the position past messages from others that came under another JID. They then count read with nothing owed.
  - That's 182 of the 184 such messages my histories find at the head, in 29 of the 60. It's main's, and the range leaves it as it was, but the range's own `came_elsewhere` is what answers "nothing came elsewhere" while nothing is owed.
  - A patch on the branch's head (Appendix B) closes it. With it, the 378 store, sync and send tests pass, the plan test among them, and my histories find none of these.
- **T1.** With `came_elsewhere`'s range stopping short of the read, history from her LID cancels what's owed for her number's messages, when its own message is the only one from her LID in the range, and no test fails. My scene S6 does.
- **Follow-ups:** O1, the phone's counts taken as the merged chat's newest; U1, `came_elsewhere`'s walk, about 0.15 µs a message and within budget; and O2, three mutants that can only owe more.
- **The trial rebase onto `7262a15`** applies all seven commits with no conflict, so there's no resolution to give the lander, and `git range-diff` finds them unchanged. The store, merge, rules, outbox, send, sync and plan tests pass on it, 378 of 378.

## What was asked, checked

### What's owed after a merge (`c090cf7`, `b2cd82f2`, `c0f2dd3c`)

- **The rules.** "Told" (`chats.told_at`, `told_n`) is how far WhatsApp has been told a chat is read; None means it knows as much as jaw. What's owed is the messages from others after "told" and up to where the chat is read.
  - `set_read` (`store/write.rs:930`) moves "told" for a phone read only when the read names the chat by its own JID and no message from others that came under another of the person's JIDs lies between "told" and the read (`came_elsewhere`, `:993`).
  - `settle_each` (`:882-892`) does the same for one of yours from another device and for history read as it comes. It never moves "told" for one of yours that came under another JID (`mine_by_another`, set in `upsert_message`, `:2138`).
  - `owed` (`store/write/outbox.rs:1285-1290`) stops at `told_now` with "told" cleared: the position, but before the phone's own unread messages while its counts stand. `set_read`'s exception for reads with counts is gone.
- **At the head, every rule the range states holds while something is owed.** I checked it by reading, by switching each part off against the builder's tests (Measurements), and by my 60 random histories. In those, no op left `owed` giving a message counted unread. No op cancelled a receipt owed, or moved "told" past one, for a message that came under another JID than the op's.
- **At the base, my histories find what the range fixed.** The same 60 histories, played at `dec0fe1`:
  - in 2 of them, `owed` covered messages the phone's counts call unread, at 11 ops: `b2cd82f2`'s bug;
  - 134 messages counted read had their owed receipt cancelled, or "told" moved past them, by a read on the phone (with counts or without) or one of yours from another device, under another JID than the message's: X4b and `c0f2dd3c`'s siblings;
  - 125 more came out of a merge counted read and not owed, against 41 at the head (I1).
  None of the first two kinds is left at the head.

### While nothing is owed: the builder's case (I1)

The builder's note is right, and the case is wider than it says.

- **The cause.** While "told" is None, `came_elsewhere` answers that nothing came under another JID: `let Some(from) = told else { return Ok(false) }` (`store/write.rs:994`). So the read speaks, and "told" stays None as the position moves. In `settle_each`, `now.told.map(...)` leaves None as it is (`:891`).
- **When.** Once a merged chat's receipts have gone, within 2 s of the merge's end while WhatsApp can be reached, the premise every rule in the range rests on stops being applied: that a read, one of yours or history under one JID says nothing of what came under another.

Five scenes, each after X4b's merge with its receipts sent (my probe's `audit_scenes`, Appendix A):

| Scene | Before | Then | At the head | With the patch |
| --- | --- | --- | --- | --- |
| S1 | `l5`, `l6` from her LID, unread | a phone read by her number past them | nothing owed | `l5`, `l6` |
| S2 | `n5`, `n6` from her number, unread | a phone read by her LID past them | nothing owed | `n5`, `n6` |
| S3 | `l5`, `l6` from her LID, unread | one of yours from another device, to her number | nothing owed | `l5`, `l6` |
| S3b | `n5`, `n6` from her number, unread | one of yours from another device, to her LID | nothing owed | `n5`, `n6` |
| S4 | her LID's `l0` to `l4` never read and her number's `n0` from history, merged | more history by her number, newer | nothing owed | `l0` to `l4`, and `n1` again |

- **What each breaks.**
  - S1 is the builder's case.
  - S2 breaks X4's own rule, that a read by a JID merged away never moves "told".
  - S3b breaks `c0f2dd3c`'s rule, that one of yours under another JID never moves it.
  - S4 is `c0f2dd3c`'s third test with nothing owed when the history comes.
  Each ends with unread 0.
- **During a merge.** A read by the chat going moves the kept chat's position past the kept chat's own unread messages, and the merge's end puts "told" back only to just before the going chat's oldest untold message (`merge_reads`, `:1009`). In my histories at the head, that's the 41 messages that surface at a merge's last step (traced in seed 0).
- **How much.**
  - In my 60 histories at the head, this is 182 of the 184 messages counted read that WhatsApp wasn't told of and nothing owes, in 29 of the 60.
  - The other 2 are my oracle's, not jaw's: a phone read with counts came before older history arrived, which its counts then call read.
  - At the base it was 142 of the same kind, and 125 out of merges: main's, and the range leaves it as it was.
- **My judgement: not data loss, but an inconsistency of X4b's class, and not acceptable as it stands.**
  - Nothing jaw keeps is lost: every message stays, and unread and the positions are as their rules derive them. No receipt goes for a message jaw counts unread.
  - But jaw.db then records that WhatsApp knows of reads that, by the rule the range enforces, it can't know of. So jaw shows those messages read and never makes sure WhatsApp was told, which is what the merge's rule, X4, X4b and this range exist to do.
  - If the phone keeps one thread per person, as WhatsApp's phones now do, it has most likely sent those receipts itself. That's why the merge-chains audits filed X4 and X4b as follow-ups: they were in main's code, outside those ranges.
  - Here it can't wait, for three reasons:
    - the range writes the very lines that decide it, and `came_elsewhere`'s answer for None is new;
    - the state it covers is the one every merged chat is in nearly all the time, where X4b's needs receipts still owed;
    - it's the larger part of the class the range was made to close. My histories find about as many such messages left as the range closed.
- **The fix,** as the patch in Appendix B has it:
  - **`set_read`:** in a one-to-one chat whose person has more than one JID, a phone read speaks only for what came under its own JID (`came_other`: no message from others after "told" and up to the read whose sender isn't the read's JID). While "told" is None, it's taken as `told_now(was)`, and kept there when the read doesn't speak; `told_if_owed` drops it if nothing from others lies between. That's X4's premise made exact, and it owes nothing new where only the read's own JID's messages lie between.
  - **`settle_each`'s `None` arm:** while "told" is None, the position moved and the person has more than one JID, "told" is set to `told_now(was)` when `mine_by_another` holds or `came_elsewhere` finds something between.
  - **Other chats are as now.** `several_jids` keeps groups, which have no person, and chats whose person has one JID on today's rule, with no walk. Without that guard, `came_other` would match every sender in a group, and a phone read there would owe again everything it read (my S7).
  - **`set_read`'s comment** then needs X4's sentence changed to say a read by a JID merged away speaks for what came under it.
- **What it costs.**
  - Receipts sent in my 60 histories went from 4,472 to 4,785, and those for a message that had had one from 120 to 161. All are the duplicates the design calls harmless. Keeping X4's rule as it is instead, so a read by a LID never moves "told", sent 5,006: every phone read by a LID would owe again what it read, for as long as the chats are merged.
  - The walk the patch adds runs over what the read moves past, about 0.15 µs a message. A read past 1,000 unread took 0.47 ms in all, against 0.31 ms at the head, and one past 20,000 took 3.4 ms. `several_jids` is one covering search.
- **Tests to add:**
  - S1 to S4 and S3b, each owing what it moved past;
  - S7, a phone read in a group, owing nothing;
  - and, in the builder's random histories, a check after every change that each message counted read is owed unless a read, one of yours or history under its own JID covered it, or a receipt went for it. Today they check only that nothing owed is cancelled.

### The phone's counts in a merged chat (O1)

`b2cd82f2`'s bound takes the phone's counts, which come with a read by one JID, as the newest messages of the merged chat, whichever JID they came under. That's what the unread jaw shows already does (`settle_each` adds them to what lies past the position), so what's owed never covers what jaw shows unread.

- **If the phone keeps a thread per JID,** though, the counts are of that JID's thread. In my scene S5:
  - her number's history says 2 unread (`n1`, `n2`), with `l0` to `l3` from her LID between them, never read;
  - merged, the chat shows 2 unread and owes `n1`, `l0`, `l1` and `l2`;
  - so `n1`, which her number's own count calls unread, is owed. The LID's messages are owed by the merge's rule, its later position winning; `n1` by the bound.
- **Why it can stand.** Which reading is right can't be known from what the phone sends, and the bound matches what jaw shows. Counting only the JID's own messages would need the JID kept beside the counts, and would change unread too.

### The receipts themselves

- **Never for a message not read.** At every op of the 60 histories at the head, `outbox::owed` gave only messages from others at or before the position and not among the phone's own unread. It gave exactly what an independent query of the rows gives.
  - Some ops kept "told" only after the next op, as a slow loop does. The `TOLD` step's guard (`store/write/outbox.rs:195`: "told" still where the lot began) left nothing behind.
- **Twice only where it's harmless.** 120 of the 4,472 receipts the loop sent went for a message that had had one. Each came from a merge moving "told" back to the going chat's oldest untold message, or from X4b's "told" staying, both by design, and none was for a message counted unread.
- **No loop.** An empty lot leaves "told" where it was (`TOLD`'s `(?4, ?5) > (?2, ?3)`) and answers `Told::All` (`send/reads.rs:157`). The bound below "told", as when the counts call everything past it unread, gives one. So a chat left in `owing` with nothing to send costs one empty pass when it's next wanted, not a loop.
- **Kills.** Each of the 60 histories was killed before a random op: a copy of jaw.db with `VACUUM INTO`, and the oracle and any lot not yet kept, as they stood. Played on from the copy, each ended with the same chats, positions, "told", unread, messages, merges and quarantine, and the same receipts sent, as the run without the kill. The same held at the base and with the patch.

### H3 and H4 (`5d0d853`)

- **The walk.** `flatten` counts `chat_merges` once (`store/write.rs:1702-1703`) and calls jaw.db a circle only once a walk has taken that many hops. A walk along a chain takes at most the chain's length less two, so a chain is never refused. A walk that has taken as many hops as there are merges has repeated a chat, so it's on the circle, and `circle` reads it from there. The count runs only when `chained` finds a chain as jaw.db opens (`store/mod.rs:498`).
- **The refusals,** from my probe's `audit_circles`, opening copies whose `chat_merges` I edited:
  - two chats: "the merges of `351910000001@s.whatsapp.net` into `100000000009@lid` and `100000000009@lid` into `351910000001@s.whatsapp.net` go round in a circle, which no jawd makes; deleting the merge of `351910000001@s.whatsapp.net` into `100000000009@lid` (`DELETE FROM chat_merges WHERE gone = 2`) lets jaw.db open, the others then ending in `351910000001@s.whatsapp.net`'s chat";
  - a chat into itself: named so, with its own `DELETE`;
  - three with a fourth merging into them: the three named, not the fourth, and the number's merge to delete;
  - three LIDs and no number: the oldest's merge to delete, naming the oldest's chat.
- **The rule it follows.** Each `DELETE` names the merge whose going chat is the one to keep: the oldest number's, as `merge_direct_chats` keeps (`:1412`), or else the oldest. The rest then form a chain into it, which the next open flattens. The builder's tests open each after the `DELETE`, and end with one chat and every message. A `merge.started` record the deleted merge leaves is harmless, as round two of merge-chains found.
- **Long chains.** Chains of 70 and 200 merges, made the same way, open with every merge going into the number's chat, in 2.3 s and 0.67 s at a load of about 68. That's mostly opening and its synced commits; round two measured the flatten at about 45 µs a merge. With the bound back at 64 (`hop-64`), the builder's 70-merge test fails.

### H1 (`84dc1d3`)

The test puts a row under the name the LID's merge takes once its chat starts merging into the new number's, which no jawd leaves, then makes the join. With the rename a plain `UPDATE` (`rename-plain`), the test fails. Its end, the trigger dropped and the merges run, has one chat and nothing in quarantine.

### H40 and H43 (`0db03fa9`, `9316ec69`)

- **H40's test** is the m2-deleted audit's scene C1. It checks that both merges go into her number's chat, then names `m1` in each of the three chats. Since the step refuses all three, it pins which check refuses: the reader's own reasons in her second LID's chat, the step's in the other two, and the step's for a reaction and an edit stopped after finding their message. It fails with `react`'s check of `deleting` off, with `edit`'s, with `TARGET_IN`'s lookup replaced by 0, and with the step's clause for copies merging in off.
- **H43's sentence** on `edit` (`protocol/README.md:381`) says what the step refuses: `SHOWN_DELETED`'s third clause (`store/write/outbox.rs:263`) covers edits as it covers reactions and votes.

### Plans and cost

- **Plans.** The range adds two statements, both in `plans.txt`, and the plan test passes at the head and on the trial rebase.
  - `came_elsewhere`: `SEARCH messages USING INDEX messages_by_time (chat=? AND at>? AND at<?)`, with a list subquery searching `chats` by key and `person_jids_by_person`, and a Bloom filter. No temporary b-tree, no `OFFSET`.
  - `SELECT count(*) FROM chat_merges`: `SCAN chat_merges`, allowed in `plans.rs:18-21` as "counted once as jaw.db opens with a chain of merges an earlier jawd left". `chat_merges` holds the merges under way.
  - `b2cd82f2` reuses `chat_read`'s and `told_now`'s statements, as it says, and runs on a reader, in `owed`.
  - The patch's two statements plan as `SEARCH person_jids USING COVERING INDEX person_jids_by_person (person=?)` and as `came_elsewhere`'s does, with a scalar subquery on `person_jids`' primary key.
- **`came_elsewhere`'s walk** (U1). It reads every message from others between "told" and the read until it finds one from another JID, and walks the whole range when there's none.
  - That's about 0.15 µs a message: 0.31 ms over 2,000 and 3.1 ms over 20,000. It's the same for a person with one JID, since SQLite walks the range even when the list of other JIDs is empty.
  - At the head it runs only while something is owed, so its range is what was read or sent here since WhatsApp was last told. That's large only after a long backlog is read here while WhatsApp can't be reached.
  - A phone read covering 20,000 such messages took 3.4 to 4.5 ms in all, within the 5 ms live budget.

### The trial rebase onto `7262a15`

- `git rebase origin/main` in my clone applied all seven commits with no conflict, so there's nothing to resolve for the lander. `git range-diff` finds each of the seven unchanged (`=`).
- Main's eleven commits touch seven files, none of them the branch's: the jaw-client tests; jaw's `bench.rs`, `run.rs` (the frame probe) and two test files; and jawd's `notify/tests.rs` and `work/serial.rs`, whose change is inside its `mod tests`. None is in the data layer.
- On the result, the store, merge, rules, outbox, send, sync and plan tests (`store::`, `sync::`, `send::`) passed, 378 of 378 with 8 ignored, in 305 s at a load of about 65. At the head they passed the same, in 324 s.

## Findings

### Data loss

None.

### Inconsistency

- **I1. While nothing is owed, a read, one of yours or history under one of the person's JIDs moves the position past messages from others that came under another, and they count read with nothing owed.**
  - **Where:** `came_elsewhere` (`store/write.rs:994`) answers false while "told" is None. So `set_read` speaks (`:956`) and leaves "told" None (`:969-971`), and `settle_each` leaves None as it is (`:891`). For a read by a JID merged away, "told" stays None too (`:965-971`).
  - **What happens:** scenes S1 to S4 and S3b, and 182 of the 184 such messages in my histories at the head, in 29 of the 60. It also happens during a merge, and surfaces at its end. It's main's, and the range leaves it as it was.
  - **Why it's an inconsistency:** jaw.db's "told" says WhatsApp knows of reads that, by the rule the range enforces, it can't. The receipts the design owes for what jaw counts read are never owed.
  - **Fix:** Appendix B's patch, or its equivalent:
    - in one-to-one chats whose person has more than one JID, take "told" as `told_now(was)` while it's None, in `set_read` and `settle_each`, and keep it there when the read, the message or the history doesn't speak for what lies between;
    - let a phone read speak for what came under its own JID;
    - leave groups and single-JID chats as they are.
    With it, the 378 store, sync and send tests pass, the plan test among them, and my histories find none of I1. Test S1 to S4, S3b and S7, and widen the builder's histories' check, as above.

### Tests (missing)

This blocks approval along with I1.

- **T1. Nothing tests `came_elsewhere`'s upper end.**
  - **Where:** `(at, n) <= (?4, ?5)` (`store/write.rs:997`).
  - **What happens:** with it made `<` (`range-high`), every test of the builder's passes, and so do my histories. But my scene S6 fails:
    - her number's `n0` to `n4` are owed because you sent `s0` here, and her LID is joined to her with no chat of its own;
    - history from her LID, newer, arrives;
    - at the head, `n0` to `n4` stay owed. With `<`, the history's own message is the range's end and the only one from her LID, so it isn't seen, "told" moves past `n0` to `n4`, and nothing is owed.
  - **Fix:** a test of S6's shape. Or `c0f2dd3c`'s second scene (her number's chat never read, her LID's read on the phone with nothing from her in it, merged, owing `n0` to `n4`) followed by history from her LID newer than everything, `n0` to `n4` still owed.

### Unpredictable performance

None that breaks a budget.

- **U1 (follow-up). `came_elsewhere` walks every message from others between "told" and the read** when none came under another JID: about 0.15 µs each, 3.1 ms over 20,000, a single-JID chat's as long as any. Today that happens only while something is owed, and is large only after a long backlog is read here offline. Appendix B's patch calls `set_read`'s walk only for chats whose person has more than one JID, which takes a single-JID chat's read over 20,000 from 3.6 ms to 0.25 ms. `settle_each`'s existing walk could take the same guard.

### Other (follow-ups; none blocks)

- **O1. The phone's counts are taken as the merged chat's newest,** whichever JID they came under (S5). It matches the unread jaw shows; doing otherwise would need the counts' JID kept.
- **O2. Three mutants survive that can only owe more, or change nothing:**
  - `came_elsewhere`'s lower end made `>=` (`range-low`) only adds the message "told" stands on, which WhatsApp was told of;
  - without `mine = 0` (`mine-ok`), nothing changes, since your messages have no sender among the person's JIDs;
  - without the kinds filter (`kinds-ok`), a line about the chat under another JID keeps "told" back.
  None needs a test.
- **The gate's timing test.** In two of my mutant runs, main's `linked_again_after_a_logout_that_cut_a_catch_up_short_…` failed under load ("no notification came"). It passed alone three times out of three, twice with the switch it had failed with and once with none.

## Measurements

Method:
- **One scratch clone** of the worktree in `/tmp/jaw-audits/merge-followups/jaw`, with one build folder and `CARGO_INCREMENTAL=0`. I switched it with `git checkout` between `9316ec69`, `dec0fe1`, the trial rebase and my scratch branches, and copied out each test binary before the next build.
- **Builds** were debug, dependencies at the workspace's `opt-level = 2`, run under `nice` on an Apple M1 Ultra with 20 cores shared with other agents, at loads of 58 to 72.
- **The probe** was a test module of my own (Appendix A), with its scenes S1 to S7, the measurements, H3's and H4's scenes, and the random histories.
- **The switches** were runtime switches in a scratch commit, each read from `JAW_AUDIT_MUT`, so one build served every mutant and the prototypes. Each mutant was run against the 175 merge, rules, outbox and sending tests (`sync::merge::`, `store::rules`, `store::write::outbox`, `send::tests`), six threads at a time. The receipts mutants were also run against the 60 histories.
- **The patch** (Appendix B) was a commit on `9316ec69` with no switch.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, no Go engine was run, and nothing was sent to WhatsApp: every jaw.db was a test's own, in the scratch folder. I stopped no process. My first mutant runner was stopped when the call that started it returned, and I ran it again in full; no run was doubled.

Results:
- **Tests.**

  | Build | `store::`, `sync::`, `send::` | Time |
  | --- | --- | --- |
  | The head, `9316ec69` | 378 passed, 8 ignored | 324 s, 6 threads, load 65 |
  | The trial rebase onto `7262a15` | 378 passed, 8 ignored | 305 s, 4 threads, load 65 |
  | The patch (Appendix B) | 378 passed, 8 ignored | 319 s, 6 threads, load 67 |

  With every switch off, the switches build passed the 175 focused tests in 89 s. Both prototypes of I1's fix passed them too.

- **Each part switched off.** "The 24 histories" is the builder's random-histories test; "mine" counts what my 60 histories found beyond what they find at the head.

  | Switch | What it changes | The builder's tests failing | Mine |
  | --- | --- | --- | --- |
  | `x4b-off` | `set_read`'s check of what came under another JID off (`c090cf7`) | X4b's first test, the counts test, the 24 histories | 41 receipts cancelled, 3 passed |
  | `counts-exception` | the exception for reads with counts back | the counts test, the 24 histories | 22 cancelled |
  | `own-counted` | `came_elsewhere` counting the chat's own JID | X4b's first test | none; it can only owe more |
  | `range-low` | `came_elsewhere`'s lower end `>=` | none (O2) | none |
  | `range-high` | its upper end `<` | none (T1) | none; S6 fails |
  | `mine-ok` | without `mine = 0` | none (O2) | none |
  | `kinds-ok` | without the kinds filter | none (O2) | none |
  | `settle-off` | `settle_each`'s check off (`c0f2dd3c`) | its three tests, the 24 histories | 66 cancelled, 28 passed |
  | `settle-elsewhere-off` | `settle_each` without `came_elsewhere` | the number's and the history's tests, the 24 histories | 23 cancelled, 13 passed |
  | `flag-off` | `mine_by_another` never set | the LID's test, the 24 histories | 2 cancelled |
  | `bound-off` | `owed`'s bound off (`b2cd82f2`) | the scene's test, the counts test, the 24 histories | 48 failed checks owing a message counted unread |
  | `hop-64` | the flatten's bound back at 64 (H3) | the 70-merge test | |
  | `kept-oldest` | the circle's merge to delete the oldest's, not the number's (H4) | the circle of three | |
  | `rename-plain` | the rename a plain `UPDATE` (H1) | H1's test | |
  | `react-deleting-off` | `react`'s check of `deleting` off (H40) | H40's test | |
  | `edit-deleting-off` | `edit`'s check off (H40) | H40's test | |
  | `target-in-0` | `TARGET_IN`'s lookup replaced by 0 (H40) | H40's test | |
  | `step-clause-off` | the step's clause for copies merging in off | H40's test, and main's three tests of the copy merging in | |

- **The random histories,** 60 of them, 25,214 ops, each killed before a random op and played on from the copy. Each history plays:
  - Ana's three chats, made in a random order with 1 to 230 messages;
  - then her messages under each JID, live and from history; yours from other devices and sent here; phone reads by each JID, to a time (a third with counts) or naming a message; reads here; her joins, in one of four orders; merge steps; the receipts loop, sometimes keeping "told" only after the next op; and earlier changes again;
  - then every merge stepped, the receipts sent, everything sent again, and both once more.

  | Build | Histories with a finding | Owed a message counted unread | Counted read, untold and not owed | Receipts sent (for one that had had one) | Kills ending otherwise |
  | --- | --- | --- | --- | --- | --- |
  | The base, `dec0fe1` | 44 | 2 histories, 11 ops | 404 | 2,979 (92) | 0 |
  | The head | 30, 29 with I1 | 0 | 184: 141 by a read or one of yours under another JID with nothing owed, 41 out of merges, 2 the oracle's | 4,472 (120) | 0 |
  | The patch | 2, the oracle's | 0 | 2, the oracle's | 4,785 (161) | 0 |

  The X4-kept form of the fix gave the same as the patch, with 5,006 receipts sent (164). Each 60-history run took 72 to 134 s, four at a time.

- **`came_elsewhere`'s walk.** Her number's chat with *n* messages from her between "told" and a phone read, none from another JID, timed as the statement alone, 15 runs on a side connection, and as the read's whole transaction (`Applied::took`):

  | *n* | The person's JIDs | The statement, median (most) | The read's transaction |
  | --- | --- | --- | --- |
  | 2,000 | 2 | 0.31 ms (0.46) | 0.58 ms |
  | 20,000 | 2 | 3.09 ms (4.18) | 3.4 to 4.5 ms |
  | 20,000 | 1 | 3.08 ms (3.74) | 3.6 ms; 0.25 ms with the patch, which doesn't walk there |

  A phone read past *n* unread with nothing owed, the walk the patch adds, median of 3:

  | *n* | At the head | With the patch |
  | --- | --- | --- |
  | 1,000 | 0.31 ms | 0.47 ms |
  | 20,000 | 0.27 ms | 3.41 ms |


## Appendix A: the random histories and scenes (`audit.rs`)

To run them again:
- Put this file at `crates/jawd/src/sync/merge/audit.rs`, and add to the end of `crates/jawd/src/sync/merge/tests.rs`:

  ```rust
  #[path = "audit.rs"]
  mod audit;
  ```

- `PROBE_SEEDS=0..60 PROBE_AT_ONCE=4 cargo test -p jawd audit_histories -- --ignored --nocapture` plays the histories and prints what they found, by kind and by seed.
- `audit_scenes` prints S1 to S7, `audit_measure` the walk's costs, `audit_circles` H4's refusals and the long chains, and `audit_trace` (`PROBE_TRACE=<seed>`) each op of a history with every chat after it.
- Set `TMPDIR` to the audit's folder, since every history makes homes of its own. Take the module out again before running the plan test, which reads its SQL too.

```rust
//! The merge-followups audit's random histories, round one. Ana has a number and two LIDs, so her
//! chats can chain. Each history makes her chats in a random order, then, in a random order: her
//! messages under each JID, live and from history; yours from other devices and sent here; reads on
//! the phone by each JID, to a time (a third of them with the phone's counts) or naming a message;
//! reads here; her joins, in a random order or all at once; merge steps; and the receipts loop,
//! sometimes keeping how far WhatsApp was told only after the next op, as a slow loop does. It ends
//! with the merges stepped, the receipts sent, everything sent again, and both once more.
//!
//! After every op, for each chat no merge waits on, it checks:
//! - P1: `outbox::owed` gives only messages from others the chat counts read, the phone's own unread
//!   left out, and the same ones an independent query gives;
//! - P2: each message from others the chat counts read that WhatsApp hasn't been told of is owed.
//!   The oracle says WhatsApp is told of a message by a receipt sent for it, or by a read on the
//!   phone, one of yours from another device or history read as it comes, under the JID it came
//!   under and at or past it, as the branch's rules say;
//! - P4: unread is as its rows give it.
//! A copy of jaw.db is kept as a kill would leave it before a random op, the rest played from it,
//! and the end compared, receipts included.
//!
//! `PROBE_SEEDS=a..b`, `PROBE_AT_ONCE`, `PROBE_TRACE=<seed>` (prints each op and the chats after it).
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};

use crate::store::outbox::{self, Owing, Step, Written};
use crate::store::{Change, Lane, Message, Person, Read, Store};
use crate::work::Priority;

use super::super::waiting;

const NEW: &str = "351910000002@s.whatsapp.net";
const LID: &str = "100000000009@lid";
const LID2: &str = "100000000010@lid";
const JIDS: [&str; 3] = [NEW, LID, LID2];
const AT: i64 = 1_792_000_000_000;
const THROUGH: i64 = i64::MAX;

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
        (self.next() % n.max(1) as u64) as usize
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

#[derive(Clone, Debug)]
pub(super) enum Op {
    Theirs { jid: &'static str, id: String, at: i64, live: bool },
    Mine { jid: &'static str, id: String, at: i64 },
    Here { jid: &'static str, id: String, at: i64 },
    Phone { jid: &'static str, at: i64, ids: Vec<String>, counts: Option<u32> },
    ReadHere { jid: &'static str, id: String },
    Join(Vec<&'static str>),
    Step(usize),
    Rest,
    /// The receipts loop; with `late`, how far WhatsApp was told is kept only after the next op.
    Receipts { late: bool },
    Again(usize),
}

impl Op {
    fn label(&self, ops: &[Op]) -> String {
        match self {
            Op::Theirs { live: true, .. } => "theirs".into(),
            Op::Theirs { live: false, .. } => "history".into(),
            Op::Mine { .. } => "mine elsewhere".into(),
            Op::Here { .. } => "sent here".into(),
            Op::Phone { counts: Some(_), .. } => "phone read with counts".into(),
            Op::Phone { ids, .. } if !ids.is_empty() => "phone read naming a message".into(),
            Op::Phone { .. } => "phone read to a time".into(),
            Op::ReadHere { .. } => "read here".into(),
            Op::Join(_) => "join".into(),
            Op::Step(_) => "step".into(),
            Op::Rest => "rest".into(),
            Op::Receipts { late: false } => "receipts".into(),
            Op::Receipts { late: true } => "receipts, told late".into(),
            Op::Again(i) => format!("again: {}", ops[*i].label(ops)),
        }
    }

    fn jid(&self, ops: &[Op]) -> Option<&'static str> {
        match self {
            Op::Theirs { jid, .. }
            | Op::Mine { jid, .. }
            | Op::Here { jid, .. }
            | Op::Phone { jid, .. }
            | Op::ReadHere { jid, .. } => Some(jid),
            Op::Again(i) => ops[*i].jid(ops),
            _ => None,
        }
    }
}

fn text(jid: &str, id: &str, at: i64, live: bool) -> Message {
    Message {
        chat: jid.into(),
        id: id.into(),
        at,
        sender: Some(jid.into()),
        kind: "text",
        text: Some(id.into()),
        summary: id.into(),
        live,
        ..Message::default()
    }
}

fn changes(op: &Op, ops: &[Op]) -> Vec<Change> {
    match op {
        Op::Theirs { jid, id, at, live } => vec![Change::Message(text(jid, id, *at, *live))],
        Op::Mine { jid, id, at } => {
            vec![Change::Message(Message { sender: None, mine: true, status: Some("sent"), ..text(jid, id, *at, true) })]
        }
        Op::Here { jid, id, at } => {
            let message = Message { sender: None, mine: true, status: Some("pending"), ..text(jid, id, *at, true) };
            vec![Change::Outbox(Step::Enqueue {
                message: Box::new(message),
                kind: "text",
                raw: vec![1],
                key: None,
                written: Written::default(),
            })]
        }
        Op::Phone { jid, at, ids, counts } => vec![Change::Read(Read {
            chat: jid.to_string(),
            at: *at,
            ids: ids.clone(),
            phone: counts.map(|u| (u, 0)),
            here: false,
        })],
        Op::ReadHere { jid, id } => {
            vec![Change::Read(Read { chat: jid.to_string(), ids: vec![id.clone()], here: true, ..Read::default() })]
        }
        Op::Join(jids) => {
            vec![Change::Person(Person { jids: jids.iter().map(|j| j.to_string()).collect(), ..Person::default() })]
        }
        Op::Again(i) => changes(&ops[*i], ops),
        Op::Step(_) | Op::Rest | Op::Receipts { .. } => Vec::new(),
    }
}

fn again_able(op: &Op) -> bool {
    matches!(op, Op::Theirs { .. } | Op::Mine { .. } | Op::Phone { .. } | Op::ReadHere { .. } | Op::Join(_))
}

pub(super) fn history(seed: u64) -> Vec<Op> {
    let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xA0D1);
    let mut t = AT;
    let mut next = 0usize;
    let mut fresh = |prefix: &str| {
        next += 1;
        format!("{prefix}{next}")
    };
    let mut came: HashMap<&'static str, Vec<String>> = HashMap::new();
    let mut any: Vec<String> = Vec::new();
    let mut ops: Vec<Op> = Vec::new();
    let mut order = JIDS.to_vec();
    rng.shuffle(&mut order);
    for jid in order {
        if rng.chance(0.15) {
            continue;
        }
        let size = rng.pick(&[1usize, 2, 5, 12, 40, 230]);
        for _ in 0..size {
            t += 1;
            if rng.chance(0.08) {
                let id = fresh("y");
                any.push(id.clone());
                ops.push(Op::Mine { jid, id, at: t });
            } else {
                let id = fresh(&jid[..1]);
                came.entry(jid).or_default().push(id.clone());
                any.push(id.clone());
                ops.push(Op::Theirs { jid, id, at: t, live: true });
            }
        }
        if rng.chance(0.6) {
            let counts = rng.chance(0.3).then(|| 1 + rng.below(6) as u32);
            ops.push(Op::Phone { jid, at: t - rng.below(size) as i64, ids: Vec::new(), counts });
        }
    }
    let joins: Vec<Vec<&'static str>> = match rng.below(4) {
        0 => vec![vec![NEW, LID, LID2]],
        1 => vec![vec![LID, LID2], vec![NEW, LID]],
        2 => vec![vec![NEW, LID], vec![LID, LID2]],
        _ => vec![vec![NEW, LID2], vec![LID2, LID]],
    };
    let total = 40 + rng.below(60);
    let mut join_at: Vec<usize> = joins.iter().map(|_| rng.below(total)).collect();
    join_at.sort_unstable();
    let mut joins = joins.into_iter().zip(join_at).peekable();
    for i in 0..total {
        while joins.peek().is_some_and(|(_, at)| *at == i) {
            ops.push(Op::Join(joins.next().unwrap().0));
        }
        let jid = rng.pick(&JIDS);
        let op = match rng.below(100) {
            0..=24 => {
                t += 1 + rng.below(20) as i64;
                let id = fresh(&jid[..1]);
                came.entry(jid).or_default().push(id.clone());
                any.push(id.clone());
                Op::Theirs { jid, id, at: t, live: true }
            }
            25..=29 => {
                let id = fresh("h");
                came.entry(jid).or_default().push(id.clone());
                any.push(id.clone());
                Op::Theirs { jid, id, at: t - 1 - rng.below(3_000) as i64, live: false }
            }
            30..=35 => {
                t += 1;
                let id = fresh("y");
                any.push(id.clone());
                Op::Mine { jid, id, at: t }
            }
            36..=38 => {
                t += 1;
                let id = fresh("s");
                Op::Here { jid, id, at: t }
            }
            39..=50 => {
                let counts = rng.chance(0.33).then(|| 1 + rng.below(6) as u32);
                Op::Phone { jid, at: t - rng.below(600) as i64, ids: Vec::new(), counts }
            }
            51..=56 => match came.get(jid).filter(|c| !c.is_empty()) {
                Some(c) => Op::Phone { jid, at: t, ids: vec![c[rng.below(c.len())].clone()], counts: None },
                None => continue,
            },
            57..=64 if !any.is_empty() => Op::ReadHere { jid, id: any[rng.below(any.len())].clone() },
            65..=76 => Op::Receipts { late: rng.chance(0.3) },
            77..=90 => Op::Step(rng.below(3)),
            _ => {
                let able: Vec<usize> = (0..ops.len()).filter(|&i| again_able(&ops[i])).collect();
                if able.is_empty() {
                    continue;
                }
                Op::Again(able[rng.below(able.len())])
            }
        };
        ops.push(op);
    }
    for (join, _) in joins {
        ops.push(Op::Join(join));
    }
    ops.push(Op::Rest);
    ops.push(Op::Receipts { late: false });
    let end = ops.len();
    for i in 0..end {
        if again_able(&ops[i]) {
            ops.push(Op::Again(i));
        }
    }
    ops.push(Op::Rest);
    ops.push(Op::Receipts { late: false });
    ops
}

#[derive(Clone, Debug)]
struct ChatRow {
    n: i64,
    id: String,
    position: Option<(i64, i64)>,
    /// The newest place counted read: the position, but before the phone's own unread messages.
    read_to: Option<(i64, i64)>,
    told: Option<(i64, i64)>,
    known: bool,
    phone_unread: i64,
    unread: i64,
    merging: bool,
}

#[derive(Clone, Debug)]
struct MessageRow {
    id: String,
    chat: i64,
    at: i64,
    n: i64,
    jid: Option<String>,
    theirs: bool,
}

struct Snap {
    chats: Vec<ChatRow>,
    messages: Vec<MessageRow>,
    /// What `outbox::owed` gives each chat no merge waits on.
    owed: HashMap<i64, Vec<String>>,
}

impl Snap {
    fn chat(&self, n: i64) -> Option<&ChatRow> {
        self.chats.iter().find(|c| c.n == n)
    }
    fn message(&self, id: &str) -> Option<&MessageRow> {
        self.messages.iter().find(|m| m.id == id)
    }
}

fn snapshot(db: &Connection) -> anyhow::Result<Snap> {
    let mut chats: Vec<ChatRow> = db
        .prepare(
            "SELECT c.n, c.id, c.read_at, c.read_n, coalesce(c.phone_unread, 0), c.told_at, c.told_n, c.unread,
                EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = c.n OR m.keep = c.n), c.read_known
            FROM chats c WHERE c.kind = 'direct' ORDER BY c.n",
        )?
        .query_map([], |r| {
            let at: Option<i64> = r.get(2)?;
            let read_n: Option<i64> = r.get(3)?;
            let told_at: Option<i64> = r.get(5)?;
            let told_n: Option<i64> = r.get(6)?;
            Ok(ChatRow {
                n: r.get(0)?,
                id: r.get(1)?,
                position: at.map(|at| (at, read_n.unwrap_or(THROUGH))),
                read_to: None,
                told: told_at.map(|at| (at, told_n.unwrap_or(THROUGH))),
                known: r.get(9)?,
                phone_unread: r.get(4)?,
                unread: r.get(7)?,
                merging: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    for c in &mut chats {
        c.read_to = match c.position {
            None => None,
            Some(p) if c.phone_unread <= 0 => Some(p),
            Some(p) => db
                .query_row(
                    "SELECT at, n FROM messages WHERE chat = ?1 AND mine = 0 AND kind NOT IN ('system', 'call')
                        AND (at, n) <= (?2, ?3)
                    ORDER BY at DESC, n DESC LIMIT 1 OFFSET ?4",
                    params![c.n, p.0, p.1, c.phone_unread.min(1_000)],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?,
        };
    }
    let messages: Vec<MessageRow> = db
        .prepare(
            "SELECT m.id, m.chat, m.at, m.n, j.jid, m.mine = 0 AND m.kind NOT IN ('system', 'call')
            FROM messages m LEFT JOIN person_jids j ON j.n = m.sender ORDER BY m.n",
        )?
        .query_map([], |r| {
            Ok(MessageRow {
                id: r.get(0)?,
                chat: r.get(1)?,
                at: r.get(2)?,
                n: r.get(3)?,
                jid: r.get(4)?,
                theirs: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut owed = HashMap::new();
    for c in chats.iter().filter(|c| !c.merging) {
        match outbox::owed(db, c.n)? {
            Owing::Receipts(lot) => {
                owed.insert(c.n, lot.messages.into_iter().map(|(id, _)| id).collect());
            }
            Owing::Nothing => {
                owed.insert(c.n, Vec::new());
            }
            Owing::Merging { .. } => {}
        }
    }
    Ok(Snap { chats, messages, owed })
}

/// The chat a JID names now, as chat_jid finds it.
fn chat_named(db: &Connection, jid: &str) -> anyhow::Result<Option<i64>> {
    let own: Option<i64> = db
        .query_row(
            "SELECT coalesce(m.keep, c.n) FROM chats c LEFT JOIN chat_merges m ON m.gone = c.n WHERE c.id = ?1",
            [jid],
            |r| r.get(0),
        )
        .optional()?;
    if own.is_some() {
        return Ok(own);
    }
    Ok(db
        .query_row(
            "SELECT c.n FROM person_jids j JOIN chats c ON c.person = j.person AND c.kind = 'direct'
            WHERE j.jid = ?1 AND NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = c.n) ORDER BY c.n LIMIT 1",
            [jid],
            |r| r.get(0),
        )
        .optional()?)
}

/// What WhatsApp has been told, by the oracle.
#[derive(Clone, Default)]
struct Oracle {
    /// For each JID, places up to which WhatsApp knows her messages under it are read.
    told_by: HashMap<String, Vec<(i64, i64)>>,
    /// How many receipts went for each message.
    receipted: BTreeMap<String, u32>,
    /// Receipts that went for a message that had had one.
    twice: u32,
}

impl Oracle {
    fn told(&self, m: &MessageRow) -> bool {
        self.receipted.contains_key(&m.id)
            || m.jid.as_ref().and_then(|j| self.told_by.get(j)).is_some_and(|b| b.iter().any(|&b| (m.at, m.n) <= b))
    }
}

#[derive(Default)]
pub(super) struct Found {
    /// P2 failures that appeared at an op, by how and by what.
    pub by: BTreeMap<String, u32>,
    pub examples: BTreeMap<String, Vec<String>>,
    pub hard: Vec<String>,
    pub twice: u32,
    pub receipts: u32,
    pub ops: usize,
    pub killed: usize,
}

impl Found {
    fn note(&mut self, key: String, example: String) {
        *self.by.entry(key.clone()).or_default() += 1;
        let examples = self.examples.entry(key).or_default();
        if examples.len() < 3 {
            examples.push(example);
        }
    }
    pub fn add(&mut self, other: Found) {
        for (k, v) in other.by {
            *self.by.entry(k).or_default() += v;
        }
        for (k, v) in other.examples {
            let mine = self.examples.entry(k).or_default();
            for e in v {
                if mine.len() < 3 {
                    mine.push(e);
                }
            }
        }
        self.hard.extend(other.hard);
        self.twice += other.twice;
        self.receipts += other.receipts;
        self.ops += other.ops;
        self.killed += other.killed;
    }
}

/// P2: messages counted read and untold, by the oracle, that aren't owed.
fn untold(snap: &Snap, oracle: &Oracle) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for c in snap.chats.iter().filter(|c| !c.merging) {
        let Some(to) = c.read_to else { continue };
        for m in snap.messages.iter().filter(|m| m.chat == c.n && m.theirs && (m.at, m.n) <= to) {
            if oracle.told(m) {
                continue;
            }
            if !c.told.is_some_and(|told| (m.at, m.n) > told) {
                out.insert(m.id.clone(), c.n);
            }
        }
    }
    out
}

/// P1 and P4, which must always hold.
fn hard(snap: &Snap, at: &str) -> Vec<String> {
    let mut out = Vec::new();
    for c in snap.chats.iter().filter(|c| !c.merging) {
        let theirs: Vec<&MessageRow> = snap.messages.iter().filter(|m| m.chat == c.n && m.theirs).collect();
        let after = theirs.iter().filter(|m| c.position.is_none_or(|p| (m.at, m.n) > p)).count() as i64;
        let want = (after.min(1_000) + c.phone_unread).min(1_000);
        if c.unread != want {
            out.push(format!("{at}: {} shows {} unread, its rows {want}", c.id, c.unread));
        }
        let Some(owed) = snap.owed.get(&c.n) else { continue };
        let mut between: Vec<&&MessageRow> = match (c.told, c.read_to) {
            (Some(told), Some(to)) => theirs.iter().filter(|m| (m.at, m.n) > told && (m.at, m.n) <= to).collect(),
            _ => Vec::new(),
        };
        between.sort_by_key(|m| (m.at, m.n));
        let mut expect: Vec<String> =
            between.into_iter().take(outbox::RECEIPTS_AT_ONCE).map(|m| m.id.clone()).collect();
        for id in owed {
            let Some(m) = snap.message(id) else {
                out.push(format!("{at}: {} owes {id}, which it doesn't hold", c.id));
                continue;
            };
            if !m.theirs || c.read_to.is_none_or(|to| (m.at, m.n) > to) {
                out.push(format!("{at}: {} owes {id}, which it counts unread or is yours", c.id));
            }
        }
        let mut got = owed.clone();
        got.sort();
        expect.sort();
        if got != expect {
            out.push(format!("{at}: {} owes {got:?} by outbox::owed, {expect:?} by its rows", c.id));
        }
    }
    out
}

async fn query<T: Send + 'static>(
    store: &Store,
    f: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> T {
    store.reads().query(Priority::Interactive, f).await.unwrap()
}

/// Lots of receipts worked out but not yet kept as told: (chat, from, upto).
type Lots = Vec<(i64, (i64, i64), (i64, i64))>;

struct Run {
    store: Store,
    oracle: Oracle,
    late: Lots,
    seed: u64,
    found: Found,
    trace: bool,
}

/// A copy of jaw.db as a kill now would leave it, in a folder of its own.
fn killed(home: &Path) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db.execute("VACUUM INTO ?1", [copy.path().join("jaw.db").to_str().unwrap()]).unwrap();
    copy
}

impl Run {
    async fn receipts(&mut self, late: bool) {
        let mut lots: Lots = Vec::new();
        for _ in 0..20 {
            let chats = query(&self.store, outbox::owing).await;
            let mut more = false;
            for chat in chats {
                let Owing::Receipts(lot) = query(&self.store, move |db| outbox::owed(db, chat)).await else { continue };
                for (id, _) in &lot.messages {
                    let times = self.oracle.receipted.entry(id.clone()).or_default();
                    if *times > 0 {
                        self.oracle.twice += 1;
                        self.found.twice += 1;
                    }
                    *times += 1;
                    self.found.receipts += 1;
                }
                more |= lot.messages.len() == outbox::RECEIPTS_AT_ONCE;
                lots.push((lot.chat, lot.from, lot.upto));
            }
            if late || !more {
                break;
            }
            self.keep(std::mem::take(&mut lots)).await;
        }
        if late {
            self.late = lots;
        } else {
            self.keep(lots).await;
        }
    }

    async fn keep(&self, lots: Lots) {
        if lots.is_empty() {
            return;
        }
        let steps = lots.into_iter().map(|(chat, from, upto)| Change::Outbox(Step::Told { chat, from, upto })).collect();
        self.store.apply(Lane::Live, steps).await.unwrap();
    }

    async fn step(&self, k: usize) -> bool {
        let waiting = waiting(&self.store).await.unwrap();
        if waiting.is_empty() {
            return false;
        }
        let merging = waiting[k % waiting.len()].clone();
        let applied = self.store.apply(Lane::Background, vec![Change::MergeStep(Some(merging.clone()))]).await.unwrap();
        assert_eq!(applied.quarantined, 0, "seed {}: a step of {} failed", self.seed, merging.reference);
        true
    }

    /// Plays op `i`, with the oracle's part, and checks after it.
    async fn play(&mut self, ops: &[Op], i: usize) {
        let op = &ops[i];
        let mut late = std::mem::take(&mut self.late);
        // The loop keeps one lot before working out the next, so a late lot lands before them.
        if matches!(op, Op::Receipts { .. }) {
            self.keep(std::mem::take(&mut late)).await;
        }
        let before = query(&self.store, snapshot).await;
        match op {
            Op::Step(k) => {
                self.step(*k).await;
            }
            Op::Rest => {
                for _ in 0..200 {
                    if !self.step(0).await {
                        break;
                    }
                }
                assert!(waiting(&self.store).await.unwrap().is_empty(), "seed {}: the merges never ended", self.seed);
            }
            Op::Receipts { late } => self.receipts(*late).await,
            _ => {
                let changes = changes(op, ops);
                let lane = if matches!(op, Op::Theirs { live: false, .. }) { Lane::Background } else { Lane::Live };
                let applied = self.store.apply(lane, changes).await.unwrap();
                assert_eq!(applied.quarantined, 0, "seed {}, op {i}: quarantined", self.seed);
            }
        }
        if !late.is_empty() {
            self.keep(late).await;
        }
        self.tell(ops, op, &before).await;
        let after = query(&self.store, snapshot).await;
        let at = format!("seed {}, op {i} ({})", self.seed, op.label(ops));
        let hard = hard(&after, &at);
        self.found.hard.extend(hard);
        let was = untold(&before, &self.oracle);
        let now = untold(&after, &self.oracle);
        for (id, chat) in &now {
            if was.contains_key(id) {
                continue;
            }
            let how = match before.message(id) {
                None => "arrived counted read".to_string(),
                Some(m) => match before.chat(m.chat) {
                    Some(c) if c.merging => "came out of a merge counted read".to_string(),
                    Some(c) if c.read_to.is_some_and(|to| (m.at, m.n) <= to) => "cancelled while still read".to_string(),
                    Some(c) if c.told.is_none() => "read past with nothing owed".to_string(),
                    Some(_) => "read past, told past it".to_string(),
                    None => "from a chat gone".to_string(),
                },
            };
            let m = after.message(id).unwrap();
            let by = op.jid(ops).map(|j| if m.jid.as_deref() == Some(j) { " by its own JID" } else { " by another JID" });
            let key = format!("{how}: {}{}", op.label(ops), by.unwrap_or(""));
            let chat = after.chat(*chat).map(|c| c.id.clone()).unwrap_or_default();
            self.found.note(key, format!("{at}: {id} from {} in {chat}", m.jid.clone().unwrap_or_default()));
        }
        if self.trace {
            println!("{at}");
            for c in &after.chats {
                println!(
                    "    {} pos {:?} to {:?} told {:?} phone {} unread {} merging {} owes {:?}",
                    c.id,
                    c.position,
                    c.read_to,
                    c.told,
                    c.phone_unread,
                    c.unread,
                    c.merging,
                    after.owed.get(&c.n)
                );
            }
            if !now.is_empty() {
                println!("    untold and not owed: {:?}", now.keys().collect::<Vec<_>>());
            }
        }
        self.found.ops += 1;
    }

    /// The oracle's part of op `op`: what WhatsApp is told by it.
    async fn tell(&mut self, ops: &[Op], op: &Op, before: &Snap) {
        let op = match op {
            Op::Again(i) => &ops[*i],
            op => op,
        };
        match op.clone() {
            Op::Mine { id, jid, .. } => {
                let place = query(&self.store, move |db| {
                    Ok(db.query_row("SELECT at, n FROM messages WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
                })
                .await;
                if let Some(place) = place {
                    self.oracle.told_by.entry(jid.to_string()).or_default().push(place);
                }
            }
            Op::Theirs { id, jid, live: false, .. } if before.message(&id).is_none() => {
                let found = query(&self.store, move |db| {
                    Ok(db
                        .query_row("SELECT chat, at, n FROM messages WHERE id = ?1", [id], |r| {
                            Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?)))
                        })
                        .optional()?)
                })
                .await;
                if let Some((chat, place)) = found {
                    // Read as it comes only while nobody has said where its chat was read, as the
                    // chat was before this op (a chat it made had no position).
                    if !before.chat(chat).is_some_and(|c| c.known) {
                        self.oracle.told_by.entry(jid.to_string()).or_default().push(place);
                    }
                }
            }
            Op::Phone { jid, at, ids, counts } => {
                let bound = query(&self.store, move |db| {
                    let Some(chat) = chat_named(db, jid)? else { return Ok(None) };
                    let named: Option<(i64, i64)> = db
                        .query_row(
                            "SELECT at, n FROM messages WHERE chat = ?1 AND id IN (SELECT value FROM json_each(?2))
                            ORDER BY at DESC, n DESC LIMIT 1",
                            params![chat, serde_json::to_string(&ids)?],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?;
                    let to = named.unwrap_or((at, THROUGH));
                    let Some(k) = counts.filter(|&k| k > 0) else { return Ok(Some(to)) };
                    Ok(Some(
                        db.query_row(
                            "SELECT at, n FROM messages WHERE chat = ?1 AND mine = 0 AND kind NOT IN ('system', 'call')
                                AND (at, n) <= (?2, ?3)
                            ORDER BY at DESC, n DESC LIMIT 1 OFFSET ?4",
                            params![chat, to.0, to.1, k],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?
                        .unwrap_or((i64::MIN, i64::MIN)),
                    ))
                })
                .await;
                if let Some(bound) = bound {
                    self.oracle.told_by.entry(jid.to_string()).or_default().push(bound);
                }
            }
            _ => {}
        }
    }
}

/// Everything a run's end holds, for comparing a run killed and played on with one that wasn't.
async fn digest(store: &Store) -> Vec<String> {
    query(store, |db| {
        let mut lines = Vec::new();
        let sqls = [
            "SELECT c.id, c.unread, c.read_at, r.id, c.read_known, c.phone_unread, c.told_at, t.id, c.marked_unread
            FROM chats c LEFT JOIN messages r ON r.n = c.read_n LEFT JOIN messages t ON t.n = c.told_n ORDER BY c.id",
            "SELECT c.id, m.id, m.at FROM messages m JOIN chats c ON c.n = m.chat ORDER BY 2",
            "SELECT g.id, k.id FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep ORDER BY 1",
            "SELECT source, ref FROM quarantine ORDER BY 1, 2",
        ];
        for sql in sqls {
            let mut statement = db.prepare(sql)?;
            let columns = statement.column_count();
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let values = (0..columns)
                    .map(|i| Ok(format!("{:?}", row.get::<_, rusqlite::types::Value>(i)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                lines.push(values.join(" "));
            }
        }
        Ok(lines)
    })
    .await
}

pub(super) async fn one(seed: u64, trace: bool) -> Found {
    let ops = history(seed);
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mut kill_rng = Rng(seed ^ 0x6B11);
    let kill = kill_rng.below(ops.len());
    let mut run = Run { store, oracle: Oracle::default(), late: Vec::new(), seed, found: Found::default(), trace };
    let mut copy = None;
    for i in 0..ops.len() {
        if i == kill {
            copy = Some((killed(home.path()), run.oracle.clone(), run.late.clone()));
        }
        run.play(&ops, i).await;
    }
    let end = digest(&run.store).await;
    let ids: Vec<String> = query(&run.store, |db| {
        Ok(db.prepare("SELECT id FROM chats WHERE kind = 'direct'")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    })
    .await;
    if ids.len() != 1 {
        run.found.hard.push(format!("seed {seed}: ends with chats {ids:?}"));
    }
    let quarantined = run.store.count("quarantine").unwrap();
    if quarantined > 0 {
        run.found.hard.push(format!("seed {seed}: {quarantined} quarantined"));
    }
    let receipted = run.oracle.receipted.clone();
    let mut found = std::mem::take(&mut run.found);
    drop(run);
    if let Some((copy, oracle, late)) = copy {
        let store = Store::open(copy.path()).unwrap();
        let mut again = Run { store, oracle, late, seed, found: Found::default(), trace: false };
        for i in kill..ops.len() {
            again.play(&ops, i).await;
        }
        let killed_end = digest(&again.store).await;
        if killed_end != end {
            let missing: Vec<&String> = end.iter().filter(|l| !killed_end.contains(l)).take(6).collect();
            let extra: Vec<&String> = killed_end.iter().filter(|l| !end.contains(l)).take(6).collect();
            found.hard.push(format!("seed {seed}: killed before op {kill}, ends otherwise: missing {missing:?}, extra {extra:?}"));
        }
        if again.oracle.receipted != receipted {
            found.hard.push(format!("seed {seed}: killed before op {kill}, other receipts"));
        }
        found.hard.extend(again.found.hard);
        found.killed += 1;
    }
    found
}

fn seeds() -> Vec<u64> {
    let spec = std::env::var("PROBE_SEEDS").unwrap_or_else(|_| "0..40".into());
    if let Some((a, b)) = spec.split_once("..") {
        (a.parse().unwrap()..b.parse().unwrap()).collect()
    } else {
        spec.split(',').map(|s| s.parse().unwrap()).collect()
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn audit_histories() {
    let seeds = seeds();
    let at_once: usize = std::env::var("PROBE_AT_ONCE").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    let mut all = Found::default();
    let mut failing = Vec::new();
    for chunk in seeds.chunks(at_once) {
        let handles: Vec<_> = chunk.iter().map(|&seed| tokio::spawn(one(seed, false))).collect();
        for (handle, seed) in handles.into_iter().zip(chunk) {
            let found = handle.await.unwrap();
            if !found.by.is_empty() || !found.hard.is_empty() {
                failing.push(*seed);
                let ops: std::collections::BTreeSet<String> = found
                    .hard
                    .iter()
                    .filter_map(|h| h.split_once(" (").map(|(at, _)| at.to_string()))
                    .collect();
                println!("SEED {seed}: hard {} at {} ops; {:?}", found.hard.len(), ops.len(), found.by);
            }
            all.add(found);
        }
    }
    println!("histories {}, ops {}, killed and played on {}", seeds.len(), all.ops, all.killed);
    println!("receipts sent {}, of which for a message that had had one {}", all.receipts, all.twice);
    println!("histories with a finding: {} {failing:?}", failing.len());
    for (key, count) in &all.by {
        println!("P2 {count:>5}  {key}");
        for e in &all.examples[key] {
            println!("           {e}");
        }
    }
    for h in all.hard.iter().take(40) {
        println!("HARD {h}");
    }
    println!("hard failures: {}", all.hard.len());
    assert!(all.hard.is_empty(), "P1, P4 or a kill failed");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn audit_trace() {
    let seed: u64 = std::env::var("PROBE_TRACE").unwrap().parse().unwrap();
    let ops = history(seed);
    for (i, op) in ops.iter().enumerate() {
        println!("op {i}: {op:?}");
    }
    let found = one(seed, true).await;
    for (key, count) in &found.by {
        println!("P2 {count:>5}  {key}: {:?}", found.examples[key]);
    }
    for h in &found.hard {
        println!("HARD {h}");
    }
}

fn theirs(jid: &str, id: &str, at: i64) -> Change {
    Change::Message(text(jid, id, at, true))
}

fn phone(jid: &str, at: i64) -> Change {
    Change::Read(Read { chat: jid.into(), at, ..Read::default() })
}

fn join(a: &str, b: &str) -> Change {
    Change::Person(Person { jids: vec![a.into(), b.into()], ..Person::default() })
}

async fn merge_all(store: &Store) {
    for _ in 0..50 {
        let waiting = waiting(store).await.unwrap();
        let Some(first) = waiting.into_iter().next() else { return };
        store.apply(Lane::Background, vec![Change::MergeStep(Some(first))]).await.unwrap();
    }
    panic!("the merges never ended");
}

/// What chat `jid` owes, by ID, and its unread.
async fn owes(store: &Store, jid: &'static str) -> (Vec<String>, i64) {
    query(store, move |db| {
        let (n, unread): (i64, i64) =
            db.query_row("SELECT n, unread FROM chats WHERE id = ?1", [jid], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let owed = match outbox::owed(db, n)? {
            Owing::Receipts(lot) => lot.messages.into_iter().map(|(id, _)| id).collect(),
            Owing::Nothing => Vec::new(),
            Owing::Merging { .. } => vec!["(merging)".to_string()],
        };
        Ok((owed, unread))
    })
    .await
}

/// Sends what every chat owes and keeps how far WhatsApp was told.
async fn send_receipts(store: &Store) -> Vec<String> {
    let mut sent = Vec::new();
    for chat in query(store, outbox::owing).await {
        if let Owing::Receipts(lot) = query(store, move |db| outbox::owed(db, chat)).await {
            sent.extend(lot.messages.iter().map(|(id, _)| id.clone()));
            let told = Step::Told { chat: lot.chat, from: lot.from, upto: lot.upto };
            store.apply(Lane::Live, vec![Change::Outbox(told)]).await.unwrap();
        }
    }
    sent
}

/// X4b's merge: the number's chat read on the phone past n0 to n4, the LID's l0 to l4 never read,
/// merged, and the receipts it owes sent, so nothing is owed.
async fn merged_and_told() -> (tempfile::TempDir, Store) {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mut changes: Vec<Change> = (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect();
    changes.extend((0..5).map(|i| theirs(LID, &format!("l{i}"), AT + 10 + i)));
    store.apply(Lane::Live, changes).await.unwrap();
    store.apply(Lane::Live, vec![phone(NEW, AT + 5_000)]).await.unwrap();
    store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
    merge_all(&store).await;
    let sent = send_receipts(&store).await;
    println!("    merged, receipts sent for {sent:?}; then owes {:?}", owes(&store, NEW).await);
    (home, store)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn audit_scenes() {
    println!("S1: then l5 and l6 under her LID, unread, then a phone read by her number past them");
    let (_h, store) = merged_and_told().await;
    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 6_000), theirs(LID, "l6", AT + 6_001)]).await.unwrap();
    println!("    before the read: owes, unread {:?}", owes(&store, NEW).await);
    store.apply(Lane::Live, vec![phone(NEW, AT + 7_000)]).await.unwrap();
    println!("    after: owes, unread {:?}", owes(&store, NEW).await);

    println!("S2: then n5 and n6 under her number, unread, then a phone read by her LID past them");
    let (_h, store) = merged_and_told().await;
    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000), theirs(NEW, "n6", AT + 6_001)]).await.unwrap();
    store.apply(Lane::Live, vec![phone(LID, AT + 7_000)]).await.unwrap();
    println!("    after: owes, unread {:?}", owes(&store, NEW).await);

    println!("S3: then l5 and l6 under her LID, then one of yours from another device to her number");
    let (_h, store) = merged_and_told().await;
    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 6_000), theirs(LID, "l6", AT + 6_001)]).await.unwrap();
    let mine = Message { sender: None, mine: true, status: Some("sent"), ..text(NEW, "y0", AT + 7_000, true) };
    store.apply(Lane::Live, vec![Change::Message(mine)]).await.unwrap();
    println!("    after: owes, unread {:?}", owes(&store, NEW).await);

    println!("S3b: then n5 and n6 under her number, then one of yours from another device to her LID");
    let (_h, store) = merged_and_told().await;
    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000), theirs(NEW, "n6", AT + 6_001)]).await.unwrap();
    let mine = Message { sender: None, mine: true, status: Some("sent"), ..text(LID, "y0", AT + 7_000, true) };
    store.apply(Lane::Live, vec![Change::Message(mine)]).await.unwrap();
    println!("    after: owes, unread {:?}", owes(&store, NEW).await);

    println!("S4: her LID's l0 to l4 never read, her number's n0 from history, merged; then more history by her number, newer");
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let history = |jid: &str, id: &str, at| Change::Message(text(jid, id, at, false));
    let mut changes: Vec<Change> = (0..5).map(|i| theirs(LID, &format!("l{i}"), AT + 10 + i)).collect();
    changes.push(history(NEW, "n0", AT));
    store.apply(Lane::Live, changes).await.unwrap();
    store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
    merge_all(&store).await;
    println!("    merged: owes, unread {:?}", owes(&store, NEW).await);
    let sent = send_receipts(&store).await;
    println!("    receipts sent for {sent:?}");
    store.apply(Lane::Background, vec![history(NEW, "n1", AT + 20)]).await.unwrap();
    println!("    after n1: owes, unread {:?}", owes(&store, NEW).await);

    println!("S6: her number's n0 to n4, never read, her LID joined with no chat; you send s0 here; then history by her LID, newer");
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store.apply(Lane::Live, (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect()).await.unwrap();
    store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
    let sent = Message { sender: None, mine: true, status: Some("pending"), ..text(NEW, "s0", AT + 5, true) };
    let send = Step::Enqueue { message: Box::new(sent), kind: "text", raw: vec![1], key: None, written: Written::default() };
    store.apply(Lane::Live, vec![Change::Outbox(send)]).await.unwrap();
    println!("    sent here: owes, unread {:?}", owes(&store, NEW).await);
    store.apply(Lane::Background, vec![Change::Message(text(LID, "l5", AT + 6, false))]).await.unwrap();
    println!("    after her LID's history: owes, unread {:?}", owes(&store, NEW).await);

    println!("S7: a group with messages from two members, read on the phone past them");
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    const GROUP: &str = "120363000000000001@g.us";
    let in_group = |id: &str, from: &str, at| {
        Change::Message(Message { chat: GROUP.into(), sender: Some(from.into()), ..text(GROUP, id, at, true) })
    };
    store.apply(Lane::Live, vec![in_group("g0", NEW, AT), in_group("g1", "351920000003@s.whatsapp.net", AT + 1)]).await.unwrap();
    store.apply(Lane::Live, vec![phone(GROUP, AT + 10)]).await.unwrap();
    println!("    after: owes, unread {:?}", owes(&store, GROUP).await);

    println!("S5: n0, n1, n2 under her number, its history read with the phone's 2 unread (n1, n2); l0 to l3 under her LID, between n1 and n2, never read; merged");
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mut changes = vec![theirs(NEW, "n0", AT), theirs(NEW, "n1", AT + 10), theirs(NEW, "n2", AT + 20)];
    changes.extend((0..4).map(|i| theirs(LID, &format!("l{i}"), AT + 12 + 2 * i)));
    store.apply(Lane::Live, changes).await.unwrap();
    let counted = Read { chat: NEW.into(), at: AT + 20, phone: Some((2, 0)), ..Read::default() };
    store.apply(Lane::Live, vec![Change::Read(counted)]).await.unwrap();
    println!("    before: number's owes, unread {:?}; LID's {:?}", owes(&store, NEW).await, owes(&store, LID).await);
    store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
    merge_all(&store).await;
    println!("    merged: owes, unread {:?}", owes(&store, NEW).await);
}

fn median(mut xs: Vec<Duration>) -> (Duration, Duration) {
    xs.sort();
    (xs[xs.len() / 2], *xs.last().unwrap())
}

const CAME_ELSEWHERE: &str = "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
                AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IN
                    (SELECT j.n FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.n = ?1 AND j.jid <> c.id))";

/// came_elsewhere's longest walk, and what a phone read costs the writer around it: her number's
/// chat with `n` messages from her number between "told" and the read, none from another JID.
async fn measured_chat(n: usize, two: bool, prefix: &str) -> (tempfile::TempDir, Store, i64) {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    if two {
        store.apply(Lane::Live, vec![theirs(LID, "l0", AT), theirs(NEW, "first", AT + 1)]).await.unwrap();
        store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
        merge_all(&store).await;
    } else {
        store.apply(Lane::Live, vec![theirs(NEW, "first", AT + 1)]).await.unwrap();
    }
    store.apply(Lane::Live, vec![phone(NEW, AT + 5)]).await.unwrap();
    send_receipts(&store).await;
    let mut t = AT + 10;
    for chunk in 0..n / 500 {
        let changes: Vec<Change> = (0..500)
            .map(|i| {
                t += 1;
                theirs(NEW, &format!("{prefix}{}", chunk * 500 + i), t)
            })
            .collect();
        store.apply(Lane::Background, changes).await.unwrap();
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    (home, store, t)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn audit_measure() {
    for (n, two) in [(2_000usize, true), (20_000, true), (20_000, false)] {
        let (home, store, t) = measured_chat(n, two, "n").await;
        // Read here up to the newest: all of it owed, "told" where the phone left it.
        let here = Read { chat: NEW.into(), ids: vec![format!("n{}", n - 1)], here: true, ..Read::default() };
        store.apply(Lane::Live, vec![Change::Read(here)]).await.unwrap();
        let (chat, told, at): (i64, (i64, i64), i64) = query(&store, |db| {
            Ok(db.query_row("SELECT n, told_at, told_n, read_at FROM chats WHERE id = ?1", [NEW], |r| {
                Ok((r.get(0)?, (r.get(1)?, r.get(2)?), r.get(3)?))
            })?)
        })
        .await;
        let side = Connection::open(home.path().join("jaw.db")).unwrap();
        let plan: Vec<String> = side
            .prepare(&format!("EXPLAIN QUERY PLAN {CAME_ELSEWHERE}"))
            .unwrap()
            .query_map(params![chat, told.0, told.1, at, THROUGH], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let mut runs = Vec::new();
        for _ in 0..15 {
            let began = std::time::Instant::now();
            let found: bool = side
                .prepare_cached(CAME_ELSEWHERE)
                .unwrap()
                .query_row(params![chat, told.0, told.1, at, THROUGH], |r| r.get(0))
                .unwrap();
            runs.push(began.elapsed());
            assert!(!found, "nothing from another JID in the range");
        }
        let (mid, most) = median(runs);
        // The phone read itself, by her number past it all, which the writer times.
        let took = store.apply(Lane::Live, vec![phone(NEW, t + 1)]).await.unwrap().took;
        println!(
            "came_elsewhere over {n} messages, person with {} JIDs: median {mid:?}, most {most:?}; the read's \
             transaction {took:?}; plan {plan:?}",
            if two { 2 } else { 1 }
        );
    }
    // With nothing owed: a phone read by her number past n unread messages from her number, the walk
    // Appendix B's patch adds (run with it to see that walk).
    for n in [1_000usize, 20_000] {
        let mut took = Vec::new();
        for _ in 0..3 {
            let (_home, store, t) = measured_chat(n, true, "u").await;
            took.push(store.apply(Lane::Live, vec![phone(NEW, t)]).await.unwrap().took);
        }
        let (mid, most) = median(took);
        println!("a phone read past {n} unread with nothing owed: median {mid:?}, most {most:?}");
    }
}

/// The refusals H4 writes, and how long a chain an earlier jawd left takes to open (H3).
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn audit_circles() {
    const OLD: &str = "351910000001@s.whatsapp.net";
    const HOSTED: &str = "100000000010@hosted.lid";
    const TAIL: &str = "100000000011@lid";
    let opened = |path: std::path::PathBuf| -> Result<(Store, Duration), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let began = std::time::Instant::now();
            let _ = tx.send(Store::open(&path).map(|s| (s, began.elapsed())).map_err(|e| format!("{e:#}")));
        });
        rx.recv_timeout(Duration::from_secs(60)).expect("opening jaw.db never ended")
    };
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let changes = vec![theirs(LID, "l0", AT), theirs(OLD, "o0", AT + 1), theirs(HOSTED, "h0", AT + 2), theirs(TAIL, "t0", AT + 3)];
    store.apply(Lane::Live, changes).await.unwrap();
    drop(store);
    let n = |db: &Connection, jid: &str| -> i64 { db.query_row("SELECT n FROM chats WHERE id = ?1", [jid], |r| r.get(0)).unwrap() };
    for (name, merges) in [
        ("two", vec![(LID, OLD), (OLD, LID)]),
        ("itself", vec![(LID, LID)]),
        ("three and a tail", vec![(LID, OLD), (OLD, HOSTED), (HOSTED, LID), (TAIL, HOSTED)]),
        ("three without a number", vec![(LID, HOSTED), (HOSTED, TAIL), (TAIL, LID)]),
    ] {
        let copy = killed(home.path());
        let db = Connection::open(copy.path().join("jaw.db")).unwrap();
        for (gone, keep) in &merges {
            db.execute("INSERT INTO chat_merges (gone, keep) VALUES (?1, ?2)", [n(&db, gone), n(&db, keep)]).unwrap();
        }
        drop(db);
        match opened(copy.path().to_path_buf()) {
            Ok(_) => println!("{name}: opened"),
            Err(e) => println!("{name}: {e}"),
        }
    }
    for merges in [70i64, 200] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let lid = |i: i64| format!("1000000{i:05}@lid");
        let mut changes = vec![theirs(OLD, "o0", AT)];
        changes.extend((0..merges).map(|i| theirs(&lid(i), &format!("l{i}"), AT + 1 + i)));
        store.apply(Lane::Live, changes).await.unwrap();
        let jids = [vec![OLD.to_string()], (0..merges).map(lid).collect()].concat();
        store.apply(Lane::Live, vec![Change::Person(Person { jids, ..Person::default() })]).await.unwrap();
        drop(store);
        let copy = killed(home.path());
        let db = Connection::open(copy.path().join("jaw.db")).unwrap();
        for i in 0..merges - 1 {
            db.execute(
                "UPDATE chat_merges SET keep = (SELECT n FROM chats WHERE id = ?2) WHERE gone = (SELECT n FROM chats WHERE id = ?1)",
                [lid(i), lid(i + 1)],
            )
            .unwrap();
        }
        drop(db);
        let (store, took) = opened(copy.path().to_path_buf()).unwrap();
        let into: Vec<String> = query(&store, |db| {
            Ok(db
                .prepare("SELECT DISTINCT k.id FROM chat_merges m JOIN chats k ON k.n = m.keep")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
        println!("a chain of {merges} merges: opened in {took:?}, every merge going into {into:?}");
    }
}
```

## Appendix B: the patch for I1

On `9316ec69`, with `plans.txt` written again by `JAW_PLANS=write`. With it, the store, sync and send tests pass, 378 of 378, and the histories find none of I1. It leaves `set_read`'s comment and the tests to the builder.

```diff
diff --git a/crates/jawd/src/store/plans.txt b/crates/jawd/src/store/plans.txt
index 5011f944..a85a9ec0 100644
--- a/crates/jawd/src/store/plans.txt
+++ b/crates/jawd/src/store/plans.txt
@@ -1212,6 +1212,20 @@ SELECT at, n FROM messages WHERE chat = ?1 AND id IN (SELECT value FROM json_eac
 UPDATE chats SET marked_unread = ?2, v = ?3 WHERE n = ?1
     SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)
 
+-- src/store/write.rs
+SELECT count(*) > 1 FROM person_jids WHERE person = (SELECT person FROM chats WHERE n = ?1 AND kind = 'direct')
+    SEARCH person_jids USING COVERING INDEX person_jids_by_person (person=?)
+    SCALAR SUBQUERY 1
+      SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)
+
+-- src/store/write.rs
+SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5) AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IS NOT (SELECT j.n FROM person_jids j WHERE j.jid = ?6))
+    SCAN CONSTANT ROW
+    SCALAR SUBQUERY 2
+      SEARCH messages USING INDEX messages_by_time (chat=? AND at>? AND at<?)
+      SCALAR SUBQUERY 1
+        SEARCH j USING COVERING INDEX sqlite_autoindex_person_jids_1 (jid=?)
+
 -- src/store/write.rs
 SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5) AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IN (SELECT j.n FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.n = ?1 AND j.jid <> c.id))
     SCAN CONSTANT ROW
diff --git a/crates/jawd/src/store/write.rs b/crates/jawd/src/store/write.rs
index 165dda23..4bccd50a 100644
--- a/crates/jawd/src/store/write.rs
+++ b/crates/jawd/src/store/write.rs
@@ -880,6 +880,16 @@ fn settle_each(db: &Connection, stamps: &mut Stamps, unsettled: Vec<(i64, Unsett
                 }
             }
             None => {
+                // While WhatsApp knew as much as jaw, it was told up to where jaw was read, and what
+                // moved the position past a message that came under another JID says nothing of it.
+                if now.told.is_none() && now.at != was.at && several_jids(db, chat)? {
+                    if let Some(to) = now.at {
+                        let from = told_now(db, chat, &was)?;
+                        if moved.mine_by_another || came_elsewhere(db, chat, Some(from), to)? {
+                            now.told = Some(from);
+                        }
+                    }
+                }
                 // What your other devices sent, and what history brought, tell WhatsApp only of what
                 // came under the JID they came by, so, as for a read (set_read), how far it was told
                 // doesn't move for one of yours that came under another of the person's JIDs, nor
@@ -953,7 +963,15 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
     let Some(was) = chat_read(db, chat)? else { return Ok(false) };
     let mut now = was.clone();
     let taken = !was.known || was.phone_unread.is_some() || was.at.is_none_or(|read| read < at);
-    let speaks = !read.here && own && !came_elsewhere(db, chat, was.told, at)?;
+    // In a chat whose messages can have come under several JIDs, a phone read speaks only for what
+    // came under its own, and WhatsApp knowing as much as jaw means it was told up to where jaw was
+    // read, which a read past a message that came under another doesn't move.
+    let several = !read.here && several_jids(db, chat)?;
+    let told = match was.told {
+        None if several => Some(told_now(db, chat, &was)?),
+        told => told,
+    };
+    let speaks = !read.here && if several { !came_other(db, chat, told, at, &user_jid(&read.chat))? } else { own };
     if read.here {
         if !taken {
             return Ok(false);
@@ -968,6 +986,8 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
         now.marked = false;
         if speaks {
             now.told = None;
+        } else if several {
+            now.told = told;
         }
     } else if speaks {
         // jaw is further on; WhatsApp now knows the chat is read this far.
@@ -988,6 +1008,29 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
     Ok(true)
 }
 
+/// Whether chat `chat` is a one-to-one chat whose person has more than one JID, so its messages
+/// can have come under several.
+fn several_jids(db: &Connection, chat: i64) -> Result<bool> {
+    Ok(db
+        .prepare_cached(
+            "SELECT count(*) > 1 FROM person_jids WHERE person = (SELECT person FROM chats WHERE n = ?1 AND kind = 'direct')",
+        )?
+        .query_row([chat], |r| r.get(0))?)
+}
+
+/// Whether chat `chat` has a message from others after `told` and up to `to` that came under
+/// another JID than `jid`.
+fn came_other(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64), jid: &str) -> Result<bool> {
+    let Some(from) = told else { return Ok(false) };
+    Ok(db
+        .prepare_cached(
+            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
+                AND mine = 0 AND kind NOT IN ('system', 'call')
+                AND sender IS NOT (SELECT j.n FROM person_jids j WHERE j.jid = ?6))",
+        )?
+        .query_row(params![chat, from.0, from.1, to.0, to.1, jid], |r| r.get(0))?)
+}
+
 /// Whether chat `chat` has a message from others after `told` and up to `to` that came under
 /// another of its person's JIDs than the chat's own, as a merge brings them.
 fn came_elsewhere(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64)) -> Result<bool> {
```

## Verdict

The range's own fixes are right and tested, and the rebase onto main is clean. Before landing:

1. **I1.** While nothing is owed, keep "told" where WhatsApp was told, for a read, one of yours or history that moves the position past a message from others that came under another of the person's JIDs. Appendix B's patch does it, for one-to-one chats whose person has more than one JID, letting a phone read speak for what came under its own JID. Update `set_read`'s comment to match. Test S1 to S4, S3b and S7, and widen the builder's random histories' check to each message counted read.
2. **T1.** Test `came_elsewhere`'s upper end with S6's shape, or `c0f2dd3c`'s second scene followed by history from her LID.
3. **A fresh audit** of the change.

O1, O2 and U1 can stand as follow-ups. Once approved, commit this note, and give every commit of the range that touches the data layer (all but `9316ec69`, which is docs only) the trailer `Data-Audit: docs/audits/2026-10-29-merge-followups.md`.

Verdict: changes needed

# Round two's appendices

## Appendix C: round two's probe

Round two's probe is Appendix A's `audit.rs` with the diff below applied. It adds:
- the receipts counted by lot, and by whether a message had had one, WhatsApp knew of it without one (by the oracle), or neither, with the first pass apart from the replay of every change at the end;
- `audit_scenes`, which checks each scene against what the head owes and prints the ones that differ: round one's S1 to S7, each scene's other side, and S8, S10, S11, S12 and S13;
- `audit_merge_window`, I2's twelve variants;
- in `audit_measure`, the walks I1 adds while nothing is owed, and `came_other` alone.

Run them as Appendix A says. With Appendix D's switches built in, `JAW_AUDIT_MUT=<switch>` turns one on. T2's tests are S10 and S11 in `audit_scenes`, and I2's is `audit_merge_window`, with the variant that moves nothing kept as it is.

```diff
--- a/crates/jawd/src/sync/merge/audit.rs (Appendix A)
+++ b/crates/jawd/src/sync/merge/audit.rs (round two)
@@ -1,4 +1,5 @@
-//! The merge-followups audit's random histories, round one. Ana has a number and two LIDs, so her
+//! The merge-followups audit's random histories, round two (round one's, with receipts counted by
+//! lot and by what WhatsApp knew, the first pass apart from the replay). Ana has a number and two LIDs, so her
 //! chats can chain. Each history makes her chats in a random order, then, in a random order: her
 //! messages under each JID, live and from history; yours from other devices and sent here; reads on
 //! the phone by each JID, to a time (a third of them with the phone's counts) or naming a message;
@@ -25,7 +26,7 @@
 use rusqlite::{Connection, OptionalExtension, params};
 
 use crate::store::outbox::{self, Owing, Step, Written};
-use crate::store::{Change, Lane, Message, Person, Read, Store};
+use crate::store::{Amend, Amendment, Change, Lane, Message, Person, Read, Store};
 use crate::work::Priority;
 
 use super::super::waiting;
@@ -428,6 +429,28 @@
     }
 }
 
+/// Receipts the loop sent in one phase of a history: the IDs, the lots (one MarkRead each in a
+/// one-to-one chat), and of the IDs, those for a message that had had one, those for one the oracle
+/// says WhatsApp knew of without one, and the rest.
+#[derive(Clone, Copy, Default, Debug)]
+pub(super) struct Sent {
+    pub ids: u32,
+    pub lots: u32,
+    pub twice: u32,
+    pub known: u32,
+    pub fresh: u32,
+}
+
+impl Sent {
+    fn add(&mut self, o: Sent) {
+        self.ids += o.ids;
+        self.lots += o.lots;
+        self.twice += o.twice;
+        self.known += o.known;
+        self.fresh += o.fresh;
+    }
+}
+
 #[derive(Default)]
 pub(super) struct Found {
     /// P2 failures that appeared at an op, by how and by what.
@@ -438,6 +461,8 @@
     pub receipts: u32,
     pub ops: usize,
     pub killed: usize,
+    /// The first pass, and the replay of every change at the end.
+    pub sent: [Sent; 2],
 }
 
 impl Found {
@@ -465,6 +490,8 @@
         self.receipts += other.receipts;
         self.ops += other.ops;
         self.killed += other.killed;
+        self.sent[0].add(other.sent[0]);
+        self.sent[1].add(other.sent[1]);
     }
 }
 
@@ -539,8 +566,17 @@
     seed: u64,
     found: Found,
     trace: bool,
+    /// The first op of the replay of every change at the end.
+    replay_from: usize,
+    /// 0 in the first pass, 1 in the replay.
+    phase: usize,
 }
 
+/// Where the replay of every change at the end begins: after the first rest and its receipts.
+fn replay_from(ops: &[Op]) -> usize {
+    ops.iter().position(|op| matches!(op, Op::Rest)).map_or(ops.len(), |i| i + 2)
+}
+
 /// A copy of jaw.db as a kill now would leave it, in a folder of its own.
 fn killed(home: &Path) -> tempfile::TempDir {
     let copy = tempfile::tempdir().unwrap();
@@ -551,18 +587,30 @@
 }
 
 impl Run {
-    async fn receipts(&mut self, late: bool) {
+    async fn receipts(&mut self, late: bool, before: &Snap) {
         let mut lots: Lots = Vec::new();
         for _ in 0..20 {
             let chats = query(&self.store, outbox::owing).await;
             let mut more = false;
             for chat in chats {
                 let Owing::Receipts(lot) = query(&self.store, move |db| outbox::owed(db, chat)).await else { continue };
+                let phase = self.phase;
+                self.found.sent[phase].lots += 1;
                 for (id, _) in &lot.messages {
+                    let known = before.message(id).is_some_and(|m| {
+                        m.jid.as_ref().and_then(|j| self.oracle.told_by.get(j)).is_some_and(|b| b.iter().any(|&b| (m.at, m.n) <= b))
+                    });
                     let times = self.oracle.receipted.entry(id.clone()).or_default();
+                    let sent = &mut self.found.sent[phase];
+                    sent.ids += 1;
                     if *times > 0 {
                         self.oracle.twice += 1;
                         self.found.twice += 1;
+                        sent.twice += 1;
+                    } else if known {
+                        sent.known += 1;
+                    } else {
+                        sent.fresh += 1;
                     }
                     *times += 1;
                     self.found.receipts += 1;
@@ -604,6 +652,7 @@
     /// Plays op `i`, with the oracle's part, and checks after it.
     async fn play(&mut self, ops: &[Op], i: usize) {
         let op = &ops[i];
+        self.phase = usize::from(i >= self.replay_from);
         let mut late = std::mem::take(&mut self.late);
         // The loop keeps one lot before working out the next, so a late lot lands before them.
         if matches!(op, Op::Receipts { .. }) {
@@ -622,7 +671,7 @@
                 }
                 assert!(waiting(&self.store).await.unwrap().is_empty(), "seed {}: the merges never ended", self.seed);
             }
-            Op::Receipts { late } => self.receipts(*late).await,
+            Op::Receipts { late } => self.receipts(*late, &before).await,
             _ => {
                 let changes = changes(op, ops);
                 let lane = if matches!(op, Op::Theirs { live: false, .. }) { Lane::Background } else { Lane::Live };
@@ -783,7 +832,17 @@
     let store = Store::open(home.path()).unwrap();
     let mut kill_rng = Rng(seed ^ 0x6B11);
     let kill = kill_rng.below(ops.len());
-    let mut run = Run { store, oracle: Oracle::default(), late: Vec::new(), seed, found: Found::default(), trace };
+    let replay = replay_from(&ops);
+    let mut run = Run {
+        store,
+        oracle: Oracle::default(),
+        late: Vec::new(),
+        seed,
+        found: Found::default(),
+        trace,
+        replay_from: replay,
+        phase: 0,
+    };
     let mut copy = None;
     for i in 0..ops.len() {
         if i == kill {
@@ -808,7 +867,8 @@
     drop(run);
     if let Some((copy, oracle, late)) = copy {
         let store = Store::open(copy.path()).unwrap();
-        let mut again = Run { store, oracle, late, seed, found: Found::default(), trace: false };
+        let mut again =
+            Run { store, oracle, late, seed, found: Found::default(), trace: false, replay_from: replay, phase: 0 };
         for i in kill..ops.len() {
             again.play(&ops, i).await;
         }
@@ -861,6 +921,12 @@
     }
     println!("histories {}, ops {}, killed and played on {}", seeds.len(), all.ops, all.killed);
     println!("receipts sent {}, of which for a message that had had one {}", all.receipts, all.twice);
+    for (name, s) in [("first pass", all.sent[0]), ("replay", all.sent[1])] {
+        println!(
+            "SENT {name}: ids {} in lots {}; had had one {}, known to WhatsApp without one {}, neither {}",
+            s.ids, s.lots, s.twice, s.known, s.fresh
+        );
+    }
     println!("histories with a finding: {} {failing:?}", failing.len());
     for (key, count) in &all.by {
         println!("P2 {count:>5}  {key}");
@@ -942,8 +1008,8 @@
 }
 
 /// X4b's merge: the number's chat read on the phone past n0 to n4, the LID's l0 to l4 never read,
-/// merged, and the receipts it owes sent, so nothing is owed.
-async fn merged_and_told() -> (tempfile::TempDir, Store) {
+/// merged, owing l0 to l4.
+async fn read_on_the_phone_and_merged() -> (tempfile::TempDir, Store) {
     let home = tempfile::tempdir().unwrap();
     let store = Store::open(home.path()).unwrap();
     let mut changes: Vec<Change> = (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect();
@@ -952,93 +1018,268 @@
     store.apply(Lane::Live, vec![phone(NEW, AT + 5_000)]).await.unwrap();
     store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
     merge_all(&store).await;
+    (home, store)
+}
+
+/// X4b's merge with the receipts it owes sent, so nothing is owed.
+async fn merged_and_told() -> (tempfile::TempDir, Store) {
+    let (home, store) = read_on_the_phone_and_merged().await;
     let sent = send_receipts(&store).await;
-    println!("    merged, receipts sent for {sent:?}; then owes {:?}", owes(&store, NEW).await);
+    assert_eq!(sent, ids("l", 0..5), "X4b's merge owes l0 to l4");
+    assert_eq!(owes(&store, NEW).await.0, Vec::<String>::new(), "nothing owed once they've gone");
     (home, store)
 }
 
-#[tokio::test(flavor = "multi_thread")]
-#[ignore]
-async fn audit_scenes() {
-    println!("S1: then l5 and l6 under her LID, unread, then a phone read by her number past them");
-    let (_h, store) = merged_and_told().await;
-    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 6_000), theirs(LID, "l6", AT + 6_001)]).await.unwrap();
-    println!("    before the read: owes, unread {:?}", owes(&store, NEW).await);
-    store.apply(Lane::Live, vec![phone(NEW, AT + 7_000)]).await.unwrap();
-    println!("    after: owes, unread {:?}", owes(&store, NEW).await);
+fn ids(prefix: &str, range: std::ops::Range<i64>) -> Vec<String> {
+    range.map(|i| format!("{prefix}{i}")).collect()
+}
 
-    println!("S2: then n5 and n6 under her number, unread, then a phone read by her LID past them");
-    let (_h, store) = merged_and_told().await;
-    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000), theirs(NEW, "n6", AT + 6_001)]).await.unwrap();
-    store.apply(Lane::Live, vec![phone(LID, AT + 7_000)]).await.unwrap();
-    println!("    after: owes, unread {:?}", owes(&store, NEW).await);
+fn mine_from(jid: &str, id: &str, at: i64) -> Change {
+    Change::Message(Message { sender: None, mine: true, status: Some("sent"), ..text(jid, id, at, true) })
+}
 
-    println!("S3: then l5 and l6 under her LID, then one of yours from another device to her number");
-    let (_h, store) = merged_and_told().await;
-    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 6_000), theirs(LID, "l6", AT + 6_001)]).await.unwrap();
-    let mine = Message { sender: None, mine: true, status: Some("sent"), ..text(NEW, "y0", AT + 7_000, true) };
-    store.apply(Lane::Live, vec![Change::Message(mine)]).await.unwrap();
-    println!("    after: owes, unread {:?}", owes(&store, NEW).await);
+fn counted(jid: &str, at: i64, unread: u32) -> Change {
+    Change::Read(Read { chat: jid.into(), at, phone: Some((unread, 0)), ..Read::default() })
+}
 
-    println!("S3b: then n5 and n6 under her number, then one of yours from another device to her LID");
+fn naming(jid: &str, id: &str) -> Change {
+    Change::Read(Read { chat: jid.into(), ids: vec![id.into()], ..Read::default() })
+}
+
+fn history_of(jid: &str, id: &str, at: i64) -> Change {
+    Change::Message(text(jid, id, at, false))
+}
+
+#[derive(Default)]
+struct Scenes {
+    differs: Vec<String>,
+}
+
+impl Scenes {
+    fn check(&mut self, name: &str, got: (Vec<String>, i64), want: (Vec<String>, i64)) {
+        if got == want {
+            println!("{name} ok: owes {:?}, unread {}", got.0, got.1);
+        } else {
+            println!("{name} DIFFERS: owes {:?}, unread {}; the head's {:?}, {}", got.0, got.1, want.0, want.1);
+            self.differs.push(name.to_string());
+        }
+    }
+}
+
+fn s(xs: &[&str]) -> Vec<String> {
+    xs.iter().map(|x| x.to_string()).collect()
+}
+
+/// Round one's scenes and round two's, each checked against what the head owes, so a switch that
+/// changes one shows as DIFFERS.
+#[tokio::test(flavor = "multi_thread")]
+#[ignore]
+async fn audit_scenes() {
+    let mut scenes = Scenes::default();
+
+    // S1: l5, l6 under her LID, then a read on the phone by her number past them; and before
+    // them, a read by her number past her number's n5 alone.
     let (_h, store) = merged_and_told().await;
+    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000)]).await.unwrap();
+    store.apply(Lane::Live, vec![phone(NEW, AT + 6_500)]).await.unwrap();
+    scenes.check("S1a, past her number's own", owes(&store, NEW).await, (vec![], 0));
+    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 7_000), theirs(LID, "l6", AT + 7_001)]).await.unwrap();
+    store.apply(Lane::Live, vec![phone(NEW, AT + 8_000)]).await.unwrap();
+    scenes.check("S1", owes(&store, NEW).await, (s(&["l5", "l6"]), 0));
+
+    // S2: n5, n6 under her number, then a read by her LID past them; once they've gone, l5 and a
+    // read by her LID past it alone.
+    let (_h, store) = merged_and_told().await;
     store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000), theirs(NEW, "n6", AT + 6_001)]).await.unwrap();
-    let mine = Message { sender: None, mine: true, status: Some("sent"), ..text(LID, "y0", AT + 7_000, true) };
-    store.apply(Lane::Live, vec![Change::Message(mine)]).await.unwrap();
-    println!("    after: owes, unread {:?}", owes(&store, NEW).await);
+    store.apply(Lane::Live, vec![phone(LID, AT + 7_000)]).await.unwrap();
+    scenes.check("S2", owes(&store, NEW).await, (s(&["n5", "n6"]), 0));
+    send_receipts(&store).await;
+    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 8_000)]).await.unwrap();
+    store.apply(Lane::Live, vec![phone(LID, AT + 9_000)]).await.unwrap();
+    scenes.check("S2a, past the LID's own", owes(&store, NEW).await, (vec![], 0));
 
-    println!("S4: her LID's l0 to l4 never read, her number's n0 from history, merged; then more history by her number, newer");
+    // S3: l5, l6 under her LID, then one of yours from another device to her number; before them,
+    // one of yours to her number past her number's n5 alone.
+    let (_h, store) = merged_and_told().await;
+    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000)]).await.unwrap();
+    store.apply(Lane::Live, vec![mine_from(NEW, "y0", AT + 6_500)]).await.unwrap();
+    scenes.check("S3a, past her number's own", owes(&store, NEW).await, (vec![], 0));
+    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 7_000), theirs(LID, "l6", AT + 7_001)]).await.unwrap();
+    store.apply(Lane::Live, vec![mine_from(NEW, "y1", AT + 8_000)]).await.unwrap();
+    scenes.check("S3", owes(&store, NEW).await, (s(&["l5", "l6"]), 0));
+
+    // S3b: n5, n6 under her number, then one of yours from another device to her LID.
+    let (_h, store) = merged_and_told().await;
+    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000), theirs(NEW, "n6", AT + 6_001)]).await.unwrap();
+    store.apply(Lane::Live, vec![mine_from(LID, "y0", AT + 7_000)]).await.unwrap();
+    scenes.check("S3b", owes(&store, NEW).await, (s(&["n5", "n6"]), 0));
+
+    // S4: her LID's l0 to l4 never read, her number's n0 from history, merged; then more history
+    // by her number, newer.
     let home = tempfile::tempdir().unwrap();
     let store = Store::open(home.path()).unwrap();
-    let history = |jid: &str, id: &str, at| Change::Message(text(jid, id, at, false));
     let mut changes: Vec<Change> = (0..5).map(|i| theirs(LID, &format!("l{i}"), AT + 10 + i)).collect();
-    changes.push(history(NEW, "n0", AT));
+    changes.push(history_of(NEW, "n0", AT));
     store.apply(Lane::Live, changes).await.unwrap();
     store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
     merge_all(&store).await;
-    println!("    merged: owes, unread {:?}", owes(&store, NEW).await);
-    let sent = send_receipts(&store).await;
-    println!("    receipts sent for {sent:?}");
-    store.apply(Lane::Background, vec![history(NEW, "n1", AT + 20)]).await.unwrap();
-    println!("    after n1: owes, unread {:?}", owes(&store, NEW).await);
+    scenes.check("S4, merged", owes(&store, NEW).await, (vec![], 5));
+    store.apply(Lane::Background, vec![history_of(NEW, "n1", AT + 20)]).await.unwrap();
+    let mut want = ids("l", 0..5);
+    want.push("n1".into());
+    scenes.check("S4", owes(&store, NEW).await, (want, 0));
 
-    println!("S6: her number's n0 to n4, never read, her LID joined with no chat; you send s0 here; then history by her LID, newer");
+    // S5: n0, n1, n2 under her number, read with the phone's 2 unread (n1, n2); l0 to l3 under her
+    // LID, between n1 and n2, never read; merged (round one's O1).
     let home = tempfile::tempdir().unwrap();
     let store = Store::open(home.path()).unwrap();
-    store.apply(Lane::Live, (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect()).await.unwrap();
+    let mut changes = vec![theirs(NEW, "n0", AT), theirs(NEW, "n1", AT + 10), theirs(NEW, "n2", AT + 20)];
+    changes.extend((0..4).map(|i| theirs(LID, &format!("l{i}"), AT + 12 + 2 * i)));
+    store.apply(Lane::Live, changes).await.unwrap();
+    store.apply(Lane::Live, vec![counted(NEW, AT + 20, 2)]).await.unwrap();
     store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
-    let sent = Message { sender: None, mine: true, status: Some("pending"), ..text(NEW, "s0", AT + 5, true) };
-    let send = Step::Enqueue { message: Box::new(sent), kind: "text", raw: vec![1], key: None, written: Written::default() };
-    store.apply(Lane::Live, vec![Change::Outbox(send)]).await.unwrap();
-    println!("    sent here: owes, unread {:?}", owes(&store, NEW).await);
-    store.apply(Lane::Background, vec![Change::Message(text(LID, "l5", AT + 6, false))]).await.unwrap();
-    println!("    after her LID's history: owes, unread {:?}", owes(&store, NEW).await);
+    merge_all(&store).await;
+    scenes.check("S5", owes(&store, NEW).await, (s(&["n1", "l0", "l1", "l2"]), 2));
 
-    println!("S7: a group with messages from two members, read on the phone past them");
+    // S6 (T1): her number's n0 to n4, never read, her LID joined with no chat; s0 sent here; then
+    // history by her LID, newer.
     let home = tempfile::tempdir().unwrap();
     let store = Store::open(home.path()).unwrap();
+    store.apply(Lane::Live, (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect()).await.unwrap();
+    store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
+    let sent = Message { sender: None, mine: true, status: Some("pending"), ..text(NEW, "s0", AT + 5, true) };
+    let send = Step::Enqueue { message: Box::new(sent), kind: "text", raw: vec![1], key: None, written: Written::default() };
+    store.apply(Lane::Live, vec![Change::Outbox(send)]).await.unwrap();
+    scenes.check("S6, sent here", owes(&store, NEW).await, (ids("n", 0..5), 0));
+    store.apply(Lane::Background, vec![history_of(LID, "l5", AT + 6)]).await.unwrap();
+    let mut want = ids("n", 0..5);
+    want.push("l5".into());
+    scenes.check("S6", owes(&store, NEW).await, (want, 0));
+
+    // S7: a group with messages from her number, her LID and Rui, read on the phone past them.
+    let home = tempfile::tempdir().unwrap();
+    let store = Store::open(home.path()).unwrap();
+    store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
     const GROUP: &str = "120363000000000001@g.us";
-    let in_group = |id: &str, from: &str, at| {
-        Change::Message(Message { chat: GROUP.into(), sender: Some(from.into()), ..text(GROUP, id, at, true) })
-    };
-    store.apply(Lane::Live, vec![in_group("g0", NEW, AT), in_group("g1", "351920000003@s.whatsapp.net", AT + 1)]).await.unwrap();
+    const RUI: &str = "351920000003@s.whatsapp.net";
+    let in_group =
+        |id: &str, from: &str, at| Change::Message(Message { sender: Some(from.into()), ..text(GROUP, id, at, true) });
+    let changes = vec![in_group("g0", NEW, AT), in_group("g1", LID, AT + 1), in_group("g2", RUI, AT + 2)];
+    store.apply(Lane::Live, changes).await.unwrap();
     store.apply(Lane::Live, vec![phone(GROUP, AT + 10)]).await.unwrap();
-    println!("    after: owes, unread {:?}", owes(&store, GROUP).await);
+    scenes.check("S7", owes(&store, GROUP).await, (vec![], 0));
 
-    println!("S5: n0, n1, n2 under her number, its history read with the phone's 2 unread (n1, n2); l0 to l3 under her LID, between n1 and n2, never read; merged");
+    // S8: n5 under her number, l5 under her LID, then a read on the phone by her number naming l5,
+    // the range's top end (came_other's <=).
+    let (_h, store) = merged_and_told().await;
+    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 6_000), theirs(LID, "l5", AT + 6_001)]).await.unwrap();
+    store.apply(Lane::Live, vec![naming(NEW, "l5")]).await.unwrap();
+    scenes.check("S8", owes(&store, NEW).await, (s(&["n5", "l5"]), 0));
+
+    // S10: n5 to n7 under her number, read on the phone by her number with its counts of 2 (n6,
+    // n7), so nothing is owed; then l5 under her LID, then one of yours to her LID (settle's
+    // told_now).
+    let (_h, store) = merged_and_told().await;
+    let changes = vec![theirs(NEW, "n5", AT + 6_000), theirs(NEW, "n6", AT + 6_001), theirs(NEW, "n7", AT + 6_002)];
+    store.apply(Lane::Live, changes).await.unwrap();
+    store.apply(Lane::Live, vec![counted(NEW, AT + 6_500, 2)]).await.unwrap();
+    scenes.check("S10, counted", owes(&store, NEW).await, (vec![], 2));
+    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 7_000)]).await.unwrap();
+    store.apply(Lane::Live, vec![mine_from(LID, "y0", AT + 8_000)]).await.unwrap();
+    scenes.check("S10", owes(&store, NEW).await, (s(&["n6", "n7", "l5"]), 0));
+
+    // S11: l5, l6 under her LID, read on the phone by her LID with its counts of 2, so nothing is
+    // owed; then n5 under her number, then a read by her number past it (set_read's told_now).
+    let (_h, store) = merged_and_told().await;
+    store.apply(Lane::Live, vec![theirs(LID, "l5", AT + 6_000), theirs(LID, "l6", AT + 6_001)]).await.unwrap();
+    store.apply(Lane::Live, vec![counted(LID, AT + 6_500, 2)]).await.unwrap();
+    scenes.check("S11, counted", owes(&store, NEW).await, (vec![], 2));
+    store.apply(Lane::Live, vec![theirs(NEW, "n5", AT + 7_000)]).await.unwrap();
+    store.apply(Lane::Live, vec![phone(NEW, AT + 7_500)]).await.unwrap();
+    scenes.check("S11", owes(&store, NEW).await, (s(&["l5", "l6", "n5"]), 0));
+
+    // S12: Rui, one JID, read on the phone past his r0 to r2, then r3 read here.
     let home = tempfile::tempdir().unwrap();
     let store = Store::open(home.path()).unwrap();
-    let mut changes = vec![theirs(NEW, "n0", AT), theirs(NEW, "n1", AT + 10), theirs(NEW, "n2", AT + 20)];
-    changes.extend((0..4).map(|i| theirs(LID, &format!("l{i}"), AT + 12 + 2 * i)));
-    store.apply(Lane::Live, changes).await.unwrap();
-    let counted = Read { chat: NEW.into(), at: AT + 20, phone: Some((2, 0)), ..Read::default() };
-    store.apply(Lane::Live, vec![Change::Read(counted)]).await.unwrap();
-    println!("    before: number's owes, unread {:?}; LID's {:?}", owes(&store, NEW).await, owes(&store, LID).await);
-    store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
-    merge_all(&store).await;
-    println!("    merged: owes, unread {:?}", owes(&store, NEW).await);
+    store.apply(Lane::Live, (0..3).map(|i| theirs(RUI, &format!("r{i}"), AT + i)).collect()).await.unwrap();
+    store.apply(Lane::Live, vec![phone(RUI, AT + 10)]).await.unwrap();
+    scenes.check("S12, read on the phone", owes(&store, RUI).await, (vec![], 0));
+    store.apply(Lane::Live, vec![theirs(RUI, "r3", AT + 20)]).await.unwrap();
+    let here = Change::Read(Read { chat: RUI.into(), ids: vec!["r3".into()], here: true, ..Read::default() });
+    store.apply(Lane::Live, vec![here]).await.unwrap();
+    scenes.check("S12", owes(&store, RUI).await, (s(&["r3"]), 0));
+
+    // S13: X4b's merge, owing l0 to l4, then a read on the phone by her LID past them alone.
+    let (_h, store) = read_on_the_phone_and_merged().await;
+    store.apply(Lane::Live, vec![phone(LID, AT + 5_000)]).await.unwrap();
+    scenes.check("S13", owes(&store, NEW).await, (vec![], 0));
+
+    println!("scenes that differ: {:?}", scenes.differs);
 }
 
+/// During a merge, a read on the phone by the kept chat's JID that speaks, then the going chat's
+/// oldest untold message moved out of turn (a copy of it arriving, a reaction naming it, a read
+/// here naming it) before the next step, whose merge_reads looks only at what's still there.
+#[tokio::test(flavor = "multi_thread")]
+#[ignore]
+async fn audit_merge_window() {
+    let hows = ["history copy", "live copy", "reaction", "read here", "phone read by the LID naming it", "none"];
+    let openers = ["a phone read by her number", "one of yours from another device to her number"];
+    for (opener, how) in openers.iter().flat_map(|o| hows.iter().map(move |h| (*o, *h))) {
+        let home = tempfile::tempdir().unwrap();
+        let store = Store::open(home.path()).unwrap();
+        let mut changes: Vec<Change> = (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect();
+        changes.extend((0..5).map(|i| theirs(LID, &format!("l{i}"), AT + 10 + i)));
+        store.apply(Lane::Live, changes).await.unwrap();
+        store.apply(Lane::Live, vec![phone(NEW, AT + 5_000)]).await.unwrap();
+        store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
+        let told = |db: &Connection| -> anyhow::Result<(Option<i64>, Option<i64>, Option<i64>)> {
+            Ok(db.query_row("SELECT read_at, told_at, told_n FROM chats WHERE id = ?1", [NEW], |r| {
+                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
+            })?)
+        };
+        let at_start = query(&store, told).await;
+        let open = if opener == "a phone read by her number" {
+            phone(NEW, AT + 6_000)
+        } else {
+            mine_from(NEW, "y-open", AT + 6_000)
+        };
+        store.apply(Lane::Live, vec![open]).await.unwrap();
+        let after_read = query(&store, told).await;
+        let change = match how {
+            "history copy" => Some(history_of(LID, "l0", AT + 10)),
+            "live copy" => Some(theirs(LID, "l0", AT + 10)),
+            "reaction" => Some(Change::Amend(Amend {
+                chat: LID.into(),
+                target: "l0".into(),
+                sender: Some(LID.into()),
+                mine: false,
+                at: AT + 6_100,
+                what: Amendment::React { emoji: "👍".into() },
+                raw: None,
+            })),
+            "read here" => Some(Change::Read(Read { chat: LID.into(), ids: vec!["l0".into()], here: true, ..Read::default() })),
+            "phone read by the LID naming it" => Some(naming(LID, "l0")),
+            _ => None,
+        };
+        if let Some(change) = change {
+            store.apply(Lane::Live, vec![change]).await.unwrap();
+        }
+        let where_l0: Option<String> = query(&store, |db| {
+            Ok(db
+                .query_row("SELECT c.id FROM messages m JOIN chats c ON c.n = m.chat WHERE m.id = 'l0'", [], |r| r.get(0))
+                .optional()?)
+        })
+        .await;
+        merge_all(&store).await;
+        println!(
+            "{opener}, then {how}: told at the merge's start {at_start:?}, after {after_read:?}; l0 then in {where_l0:?}; \
+             merged, owes and unread {:?}",
+            owes(&store, NEW).await
+        );
+    }
+}
+
 fn median(mut xs: Vec<Duration>) -> (Duration, Duration) {
     xs.sort();
     (xs[xs.len() / 2], *xs.last().unwrap())
@@ -1118,19 +1359,55 @@
             if two { 2 } else { 1 }
         );
     }
-    // With nothing owed: a phone read by her number past n unread messages from her number, the walk
-    // Appendix B's patch adds (run with it to see that walk).
+    // With nothing owed: a phone read by her number, or one of yours from another device to her
+    // number, past n unread messages from her number, the walks I1's fix adds (came_other in
+    // set_read, came_elsewhere in settle), for a person with two JIDs and with one.
     for n in [1_000usize, 20_000] {
-        let mut took = Vec::new();
-        for _ in 0..3 {
-            let (_home, store, t) = measured_chat(n, true, "u").await;
-            took.push(store.apply(Lane::Live, vec![phone(NEW, t)]).await.unwrap().took);
+        for two in [true, false] {
+            for what in ["a phone read", "one of yours from another device"] {
+                let mut took = Vec::new();
+                for _ in 0..3 {
+                    let (_home, store, t) = measured_chat(n, two, "u").await;
+                    let change = if what == "a phone read" { phone(NEW, t) } else { mine_from(NEW, "y-measure", t + 1) };
+                    took.push(store.apply(Lane::Live, vec![change]).await.unwrap().took);
+                    assert_eq!(owes(&store, NEW).await, (vec![], 0), "nothing from another JID: nothing owed");
+                }
+                let (mid, most) = median(took);
+                println!(
+                    "{what} past {n} unread with nothing owed, person with {} JIDs: the transaction's median {mid:?}, most {most:?}",
+                    if two { 2 } else { 1 }
+                );
+            }
         }
-        let (mid, most) = median(took);
-        println!("a phone read past {n} unread with nothing owed: median {mid:?}, most {most:?}");
     }
+    // came_other alone, over n messages from her number and none from another JID.
+    for n in [2_000usize, 20_000] {
+        let (home, store, t) = measured_chat(n, true, "o").await;
+        let (chat, at): (i64, i64) = query(&store, |db| {
+            Ok(db.query_row("SELECT n, read_at FROM chats WHERE id = ?1", [NEW], |r| Ok((r.get(0)?, r.get(1)?)))?)
+        })
+        .await;
+        let side = Connection::open(home.path().join("jaw.db")).unwrap();
+        let mut runs = Vec::new();
+        for _ in 0..15 {
+            let began = std::time::Instant::now();
+            let found: bool = side
+                .prepare_cached(CAME_OTHER)
+                .unwrap()
+                .query_row(params![chat, at, THROUGH, t + 1, THROUGH, NEW], |r| r.get(0))
+                .unwrap();
+            runs.push(began.elapsed());
+            assert!(!found, "nothing from another JID in the range");
+        }
+        let (mid, most) = median(runs);
+        println!("came_other over {n} messages: median {mid:?}, most {most:?}");
+    }
 }
 
+const CAME_OTHER: &str = "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
+                AND mine = 0 AND kind NOT IN ('system', 'call')
+                AND sender IS NOT (SELECT j.n FROM person_jids j WHERE j.jid = ?6))";
+
 /// The refusals H4 writes, and how long a chain an earlier jawd left takes to open (H3).
 #[tokio::test(flavor = "multi_thread")]
 #[ignore]
```

## Appendix D: the switches

A scratch commit on `01a550c0`, so one build served every mutant: `JAW_AUDIT_MUT` names the switches on, comma-separated, and with none on the build behaves as the head. `i2-fix` is Appendix E's patch. It also includes the probe in the merge tests. The plan test fails with it, since the switches' SQL isn't pinned.

```diff
diff --git a/crates/jawd/src/store/write.rs b/crates/jawd/src/store/write.rs
index ecdedc11..e2b8afc8 100644
--- a/crates/jawd/src/store/write.rs
+++ b/crates/jawd/src/store/write.rs
@@ -712,6 +712,12 @@ pub(super) const THROUGH: i64 = i64::MAX;
 /// Before every message: where a chat with no position is read up to.
 const START: (i64, i64) = (i64::MIN, i64::MIN);
 
+/// The audit's switches: whether mutant `name` is on, from JAW_AUDIT_MUT (comma-separated).
+pub(crate) fn audit_mut(name: &str) -> bool {
+    static MUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
+    MUT.get_or_init(|| std::env::var("JAW_AUDIT_MUT").unwrap_or_default()).split(',').any(|m| m == name)
+}
+
 /// Where a chat was read up to and how far WhatsApp has been told, as settle and set_read read it.
 #[derive(Clone, Debug, PartialEq, Eq)]
 pub(super) struct ChatRead {
@@ -907,13 +913,18 @@ fn settle_each(db: &Connection, stamps: &mut Stamps, unsettled: Vec<(i64, Unsett
             None => {
                 // While WhatsApp knew as much as jaw, it was told up to where jaw was read, and what
                 // moved the position past a message that came under another JID says nothing of it.
-                if now.told.is_none()
-                    && now.at != was.at
+                if !audit_mut("fix-out")
+                    && !audit_mut("settle-out")
+                    && now.told.is_none()
+                    && (audit_mut("settle-moved-off") || now.at != was.at)
                     && let Some(to) = now.at
-                    && several_jids(db, chat)?
+                    && (audit_mut("settle-guard-off") || several_jids(db, chat)?)
                 {
-                    let from = told_now(db, chat, &was)?;
-                    if moved.mine_by_another || came_elsewhere(db, chat, Some(from), to)? {
+                    let from =
+                        if audit_mut("settle-from-at") { was.at.unwrap_or(START) } else { told_now(db, chat, &was)? };
+                    if (moved.mine_by_another && !audit_mut("settle-flag-off"))
+                        || (!audit_mut("settle-elsewhere-off") && came_elsewhere(db, chat, Some(from), to)?)
+                    {
                         now.told = Some(from);
                     }
                 }
@@ -922,6 +933,7 @@ fn settle_each(db: &Connection, stamps: &mut Stamps, unsettled: Vec<(i64, Unsett
                 // doesn't move for one of yours that came under another of the person's JIDs, nor
                 // past a message from others that did, which a merge leaves owed.
                 let known = match known {
+                    _ if audit_mut("known-off") => known,
                     Some(to) if !moved.mine_by_another && !came_elsewhere(db, chat, now.told, to)? => Some(to),
                     _ => None,
                 };
@@ -991,12 +1003,27 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
     let Some(was) = chat_read(db, chat)? else { return Ok(false) };
     let mut now = was.clone();
     let taken = !was.known || was.phone_unread.is_some() || was.at.is_none_or(|read| read < at);
-    let several = !read.here && several_jids(db, chat)?;
+    let old = audit_mut("fix-out") || audit_mut("set-read-out");
+    let several = !read.here && (audit_mut("set-several-all") || several_jids(db, chat)?);
     let told = match was.told {
-        None if several => Some(told_now(db, chat, &was)?),
+        None if several && !audit_mut("set-told-none") => Some(if audit_mut("set-from-at") {
+            was.at.unwrap_or(START)
+        } else {
+            told_now(db, chat, &was)?
+        }),
         told => told,
     };
-    let speaks = !read.here && if several { !came_other(db, chat, told, at, &by)? } else { own };
+    let other_by = if audit_mut("other-own-chat") { jid.clone() } else { by.clone() };
+    let speaks = if old {
+        !read.here && own && !came_elsewhere(db, chat, was.told, at)?
+    } else {
+        !read.here
+            && if several {
+                (!audit_mut("set-x4-kept") || own) && !came_other(db, chat, told, at, &other_by)?
+            } else {
+                own
+            }
+    };
     if read.here {
         if !taken {
             return Ok(false);
@@ -1011,7 +1038,7 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
         now.marked = false;
         if speaks {
             now.told = None;
-        } else if several {
+        } else if several && !old && !audit_mut("set-not-kept") {
             now.told = told;
         }
     } else if speaks {
@@ -1048,26 +1075,30 @@ fn several_jids(db: &Connection, chat: i64) -> Result<bool> {
 /// another JID than `jid`.
 fn came_other(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64), jid: &str) -> Result<bool> {
     let Some(from) = told else { return Ok(false) };
-    Ok(db
-        .prepare_cached(
-            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
-                AND mine = 0 AND kind NOT IN ('system', 'call')
-                AND sender IS NOT (SELECT j.n FROM person_jids j WHERE j.jid = ?6))",
-        )?
-        .query_row(params![chat, from.0, from.1, to.0, to.1, jid], |r| r.get(0))?)
+    let low = if audit_mut("other-low") { ">=" } else { ">" };
+    let high = if audit_mut("other-high") { "<" } else { "<=" };
+    let mine = if audit_mut("other-mine") { "" } else { "AND mine = 0" };
+    let kinds = if audit_mut("other-kinds") { "" } else { "AND kind NOT IN ('system', 'call')" };
+    let not = if audit_mut("other-null") { "<>" } else { "IS NOT" };
+    let sql = format!(
+        "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) {low} (?2, ?3) AND (at, n) {high} (?4, ?5)
+            {mine} {kinds}
+            AND sender {not} (SELECT j.n FROM person_jids j WHERE j.jid = ?6))"
+    );
+    Ok(db.prepare_cached(&sql)?.query_row(params![chat, from.0, from.1, to.0, to.1, jid], |r| r.get(0))?)
 }
 
 /// Whether chat `chat` has a message from others after `told` and up to `to` that came under
 /// another of its person's JIDs than the chat's own, as a merge brings them.
 fn came_elsewhere(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64)) -> Result<bool> {
     let Some(from) = told else { return Ok(false) };
-    Ok(db
-        .prepare_cached(
-            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
-                AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IN
-                    (SELECT j.n FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.n = ?1 AND j.jid <> c.id))",
-        )?
-        .query_row(params![chat, from.0, from.1, to.0, to.1], |r| r.get(0))?)
+    let high = if audit_mut("range-high") { "<" } else { "<=" };
+    let sql = format!(
+        "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) {high} (?4, ?5)
+            AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IN
+                (SELECT j.n FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.n = ?1 AND j.jid <> c.id))"
+    );
+    Ok(db.prepare_cached(&sql)?.query_row(params![chat, from.0, from.1, to.0, to.1], |r| r.get(0))?)
 }
 
 /// Merges chat `gone`'s read position into `keep`'s, as a merge starts and again just before
@@ -1961,6 +1992,9 @@ fn moved_from_merging(db: &Connection, stamps: &mut Stamps, keep: i64, id: &str)
         .query_row(params![keep, id], |r| Ok((r.get(0)?, r.get(1)?)))
         .optional()?;
     let Some((gone, n)) = found else { return Ok(None) };
+    if audit_mut("i2-fix") {
+        merge_reads(db, stamps, keep, gone)?;
+    }
     let moved = move_merged(db, stamps, gone, keep, n)?;
     show_newest(db, stamps, gone)?;
     show_newest(db, stamps, keep)?;
@@ -2272,7 +2306,7 @@ fn upsert_message(db: &Connection, stamps: &mut Stamps, message: &Message) -> Re
         (true, Sent::Elsewhere) => {
             moved.mine = moved.mine.max(place);
             moved.mine_live |= message.live;
-            moved.mine_by_another |= user_jid(&message.chat) != chat_id;
+            moved.mine_by_another |= !audit_mut("flag-off") && user_jid(&message.chat) != chat_id;
         }
         (true, Sent::Forwarded) => {}
         (false, _) if !message.live => moved.history = moved.history.max(place),
diff --git a/crates/jawd/src/store/write/outbox.rs b/crates/jawd/src/store/write/outbox.rs
index d685b665..cbce9bc7 100644
--- a/crates/jawd/src/store/write/outbox.rs
+++ b/crates/jawd/src/store/write/outbox.rs
@@ -1285,6 +1285,7 @@ pub fn owed(db: &Connection, chat: i64) -> Result<Owing> {
     // Where WhatsApp would be told up to if it knew as much as jaw: the position, but before the
     // phone's own unread messages while its counts stand.
     let (at, n) = match super::chat_read(db, chat)? {
+        Some(_) if super::audit_mut("bound-off") => (at, n),
         Some(read) => super::told_now(db, chat, &super::ChatRead { told: None, ..read })?,
         None => (at, n),
     };
diff --git a/crates/jawd/src/sync/merge/tests.rs b/crates/jawd/src/sync/merge/tests.rs
index 4b52c577..40037bf8 100644
--- a/crates/jawd/src/sync/merge/tests.rs
+++ b/crates/jawd/src/sync/merge/tests.rs
@@ -1846,3 +1846,6 @@ async fn in_random_histories_of_reads_and_a_merge_a_phone_read_leaves_owed_what_
         no_read_cancels_what_it_cant_vouch_for(seed).await;
     }
 }
+
+#[path = "audit.rs"]
+mod audit;
```

## Appendix E: the patch for I2

On `01a550c0`. Each step already runs `merge_reads` before it moves messages (`store/write.rs:1733-1736`); this does the same before a message is moved out of turn. As `i2-fix`, the 183 focused tests pass, the 60 histories find what they find at the head, and the window scene owes `l0` to `l4` in all eight variants that lose it at the head. No statement is new.

```diff
--- a/crates/jawd/src/store/write.rs
+++ b/crates/jawd/src/store/write.rs
@@ -1961,6 +1961,9 @@
         .query_row(params![keep, id], |r| Ok((r.get(0)?, r.get(1)?)))
         .optional()?;
     let Some((gone, n)) = found else { return Ok(None) };
+    // As before a step's moves (merge_step): where the chat going was read is merged into the kept
+    // chat's first, so "told" stays before the oldest message there WhatsApp hasn't been told of.
+    merge_reads(db, stamps, keep, gone)?;
     let moved = move_merged(db, stamps, gone, keep, n)?;
     show_newest(db, stamps, gone)?;
     show_newest(db, stamps, keep)?;
```

# Verdict of round two (`01a550c0`)

I1 and T1 are closed, each by tests that fail without the fix, nothing in the range loses data, and the rebase onto `5230b9f` is clean. Before landing:

1. **T2.** Test the phone's counts in I1's "told", with S10 and S11 from Appendix C:
   - S10: one of yours to her LID after a read by her number with its counts of 2, owing `n6`, `n7` and `l5`;
   - S11: a read by her number after one by her LID with its counts of 2, owing `l5`, `l6` and `n5`.
   Each must fail with `told_now(was)` replaced by the position, in `settle_each` and `set_read` in turn.
2. **I2.** In `moved_from_merging`, run `merge_reads(keep, gone)` before the move (Appendix E). Test it with the window scene:
   - during the merge, a phone read by her number, or one of yours to her number;
   - then a copy of `l0`, live and from history, a reaction to it, and a read here of it, each owing `l0` to `l4` once the merge ends;
   - and her LID's own read naming `l0`, owing `l1` to `l4`.
3. **A fresh audit** of the change.

O1 to O5 and U1 can stand as follow-ups. Once a round approves, commit this note, and give the commits `scripts/check-data-audit.py` names the trailer `Data-Audit: docs/audits/2026-10-29-merge-followups.md`: today the eight of the range but `51d6fb9d`, which changes only `protocol/README.md`, and the new ones for T2 and I2.

Verdict: changes needed

# Round three's appendices

## Appendix F: round three's probe

Round three's probe is round two's (Appendix A with C's diff) with the diff below applied. It adds:
- an op for her reaction to a message;
- `history_window`, the window's generator, chosen with `PROBE_GEN=window`;
- `audit_merge_window3`, the 51 window scenes, each checked against what the rule says is owed;
- `audit_measure_i2`, `audit_measure_i2_mine` and `audit_measure_i2_here`, what the line costs, at its shortest and longest walks.

Run them as Appendix A says, with Appendix G's switches built in (`JAW_AUDIT_MUT=<switch>`): `PROBE_GEN=window PROBE_SEEDS=0..300 PROBE_AT_ONCE=4 cargo test -p jawd audit_histories -- --ignored --nocapture` for the window histories.

```diff
--- a/crates/jawd/src/sync/merge/audit.rs (round two: Appendix A with C)
+++ b/crates/jawd/src/sync/merge/audit.rs (round three)
@@ -78,6 +78,8 @@
     /// The receipts loop; with `late`, how far WhatsApp was told is kept only after the next op.
     Receipts { late: bool },
     Again(usize),
+    /// Round three: her reaction, under `jid`, to message `target`.
+    React { jid: &'static str, target: String, at: i64 },
 }
 
 impl Op {
@@ -97,6 +99,7 @@
             Op::Receipts { late: false } => "receipts".into(),
             Op::Receipts { late: true } => "receipts, told late".into(),
             Op::Again(i) => format!("again: {}", ops[*i].label(ops)),
+            Op::React { .. } => "reaction".into(),
         }
     }
 
@@ -106,7 +109,8 @@
             | Op::Mine { jid, .. }
             | Op::Here { jid, .. }
             | Op::Phone { jid, .. }
-            | Op::ReadHere { jid, .. } => Some(jid),
+            | Op::ReadHere { jid, .. }
+            | Op::React { jid, .. } => Some(jid),
             Op::Again(i) => ops[*i].jid(ops),
             _ => None,
         }
@@ -158,13 +162,156 @@
         }
         Op::Again(i) => changes(&ops[*i], ops),
         Op::Step(_) | Op::Rest | Op::Receipts { .. } => Vec::new(),
+        Op::React { jid, target, at } => vec![Change::Amend(Amend {
+            chat: jid.to_string(),
+            target: target.clone(),
+            sender: Some(jid.to_string()),
+            mine: false,
+            at: *at,
+            what: Amendment::React { emoji: "👍".into() },
+            raw: None,
+        })],
     }
 }
 
 fn again_able(op: &Op) -> bool {
-    matches!(op, Op::Theirs { .. } | Op::Mine { .. } | Op::Phone { .. } | Op::ReadHere { .. } | Op::Join(_))
+    matches!(
+        op,
+        Op::Theirs { .. } | Op::Mine { .. } | Op::Phone { .. } | Op::ReadHere { .. } | Op::Join(_) | Op::React { .. }
+    )
 }
 
+/// Round three's generator for I2's window (`PROBE_GEN=window`): her number's chat read on the
+/// phone past its messages, sometimes with its counts; her LID's never read, or read on the phone
+/// part way; sometimes her second LID's, for a chain; her JIDs joined early; then, while the merges
+/// wait, many reads on the phone by her number and yours from other devices to it, which leave
+/// "told" None, copies of her LIDs' messages, live and from history, reactions to them, reads here
+/// and on the phone naming them by either JID, her new messages, the receipts loop, and few steps.
+/// It ends as `history` does: every merge stepped, the receipts sent, everything again, and both
+/// once more.
+pub(super) fn history_window(seed: u64) -> Vec<Op> {
+    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x1D2B);
+    let mut t = AT;
+    let mut next = 0usize;
+    let mut fresh = |prefix: &str| {
+        next += 1;
+        format!("{prefix}{next}")
+    };
+    let mut ops: Vec<Op> = Vec::new();
+    // Her LIDs' messages from others, by the op that brings them, and every message's ID.
+    let mut lids: Vec<(usize, &'static str, String, i64)> = Vec::new();
+    let mut any: Vec<String> = Vec::new();
+    let chain = rng.chance(0.3);
+    let mut order: Vec<&'static str> = if chain { vec![NEW, LID, LID2] } else { vec![NEW, LID] };
+    rng.shuffle(&mut order);
+    for jid in order.clone() {
+        let size = 1 + rng.below(if jid == NEW { 12 } else { 25 });
+        for _ in 0..size {
+            t += 1 + rng.below(3) as i64;
+            let id = fresh(&jid[..1]);
+            any.push(id.clone());
+            if jid != NEW {
+                lids.push((ops.len(), jid, id.clone(), t));
+            }
+            ops.push(Op::Theirs { jid, id, at: t, live: true });
+        }
+    }
+    // Her number's chat read on the phone past everything; her LIDs' part way, at times.
+    let counts = rng.chance(0.2).then(|| 1 + rng.below(3) as u32);
+    ops.push(Op::Phone { jid: NEW, at: t + 1 + rng.below(50) as i64, ids: Vec::new(), counts });
+    for jid in [LID, LID2] {
+        if rng.chance(0.2) {
+            if let Some((_, _, _, at)) = lids.iter().filter(|l| l.1 == jid).nth(0) {
+                ops.push(Op::Phone { jid, at: *at + rng.below(10) as i64, ids: Vec::new(), counts: None });
+            }
+        }
+    }
+    let joins: Vec<Vec<&'static str>> = if chain {
+        match rng.below(3) {
+            0 => vec![vec![NEW, LID, LID2]],
+            1 => vec![vec![LID, LID2], vec![NEW, LID]],
+            _ => vec![vec![NEW, LID], vec![LID, LID2]],
+        }
+    } else {
+        vec![vec![NEW, LID]]
+    };
+    let mut joins = joins.into_iter();
+    ops.push(Op::Join(joins.next().unwrap()));
+    t += 1_000;
+    let total = 30 + rng.below(50);
+    for i in 0..total {
+        if i == total / 3 {
+            if let Some(join) = joins.next() {
+                ops.push(Op::Join(join));
+            }
+        }
+        let lid = (!lids.is_empty()).then(|| lids[rng.below(lids.len())].clone());
+        let op = match (rng.below(100), lid) {
+            (0..=17, _) => {
+                t += 1 + rng.below(20) as i64;
+                let counts = rng.chance(0.15).then(|| rng.below(3) as u32);
+                Op::Phone { jid: NEW, at: t, ids: Vec::new(), counts }
+            }
+            (18..=27, _) => {
+                t += 1;
+                let id = fresh("y");
+                any.push(id.clone());
+                Op::Mine { jid: NEW, id, at: t }
+            }
+            (28..=37, Some((op, _, _, _))) => Op::Again(op),
+            (38..=45, Some((_, jid, id, at))) => Op::Theirs { jid, id, at, live: false },
+            (46..=53, Some((_, jid, id, _))) => {
+                t += 1;
+                Op::React { jid, target: id, at: t }
+            }
+            (54..=59, Some((_, jid, id, _))) => Op::ReadHere { jid: rng.pick(&[jid, NEW]), id },
+            (60..=65, Some((_, jid, id, _))) => Op::Phone { jid: rng.pick(&[jid, NEW]), at: t, ids: vec![id], counts: None },
+            (66..=73, _) => {
+                t += 1 + rng.below(20) as i64;
+                let jid = rng.pick(&order);
+                let id = fresh(&jid[..1]);
+                any.push(id.clone());
+                Op::Theirs { jid, id, at: t, live: true }
+            }
+            (74..=81, _) => Op::Receipts { late: rng.chance(0.3) },
+            (82..=86, _) => Op::Step(rng.below(3)),
+            (87..=91, _) => {
+                t += 1;
+                let lid = if chain { rng.pick(&[LID, LID2]) } else { LID };
+                Op::Phone { jid: lid, at: t, ids: Vec::new(), counts: None }
+            }
+            (92..=95, _) => {
+                t += 1;
+                Op::Here { jid: NEW, id: fresh("s"), at: t }
+            }
+            _ => {
+                let able: Vec<usize> = (0..ops.len()).filter(|&i| again_able(&ops[i])).collect();
+                Op::Again(able[rng.below(able.len())])
+            }
+        };
+        ops.push(op);
+    }
+    for join in joins {
+        ops.push(Op::Join(join));
+    }
+    ops.push(Op::Rest);
+    ops.push(Op::Receipts { late: false });
+    let end = ops.len();
+    for i in 0..end {
+        if again_able(&ops[i]) {
+            ops.push(Op::Again(i));
+        }
+    }
+    ops.push(Op::Rest);
+    ops.push(Op::Receipts { late: false });
+    ops
+}
+
+/// The generator `PROBE_GEN` names: `window` for round three's, else round one's.
+fn generated(seed: u64) -> Vec<Op> {
+    if std::env::var("PROBE_GEN").as_deref() == Ok("window") { history_window(seed) } else { history(seed) }
+}
+
 pub(super) fn history(seed: u64) -> Vec<Op> {
     let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xA0D1);
     let mut t = AT;
@@ -827,7 +974,7 @@
 }
 
 pub(super) async fn one(seed: u64, trace: bool) -> Found {
-    let ops = history(seed);
+    let ops = generated(seed);
     let home = tempfile::tempdir().unwrap();
     let store = Store::open(home.path()).unwrap();
     let mut kill_rng = Rng(seed ^ 0x6B11);
@@ -945,7 +1092,7 @@
 #[ignore]
 async fn audit_trace() {
     let seed: u64 = std::env::var("PROBE_TRACE").unwrap().parse().unwrap();
-    let ops = history(seed);
+    let ops = generated(seed);
     for (i, op) in ops.iter().enumerate() {
         println!("op {i}: {op:?}");
     }
@@ -1276,10 +1423,285 @@
             "{opener}, then {how}: told at the merge's start {at_start:?}, after {after_read:?}; l0 then in {where_l0:?}; \
              merged, owes and unread {:?}",
             owes(&store, NEW).await
+        );
+    }
+}
+
+fn amend(jid: &str, target: &str, at: i64, mine: bool, what: Amendment) -> Change {
+    let sender = if mine { None } else { Some(jid.to_string()) };
+    Change::Amend(Amend { chat: jid.into(), target: target.into(), sender, mine, at, what, raw: None })
+}
+
+/// Round three's window scenes: I2's openers and moves, more of each, each checked against what
+/// the rule says is owed once the merges end. Her number's n0 to n4 read on the phone past them,
+/// her LID's l0 to l4 never read (with `chain`, her second LID's m0 to m2 too, older than l0),
+/// joined; then an opener; then the moves, each a list of changes applied one transaction each;
+/// then every merge stepped.
+#[tokio::test(flavor = "multi_thread")]
+#[ignore]
+async fn audit_merge_window3() {
+    let openers: Vec<(&str, Vec<Change>)> = vec![
+        ("a phone read by her number", vec![phone(NEW, AT + 6_000)]),
+        ("one of yours from another device to her number", vec![mine_from(NEW, "y-open", AT + 6_000)]),
+        ("a phone read by her number with its counts of 0", vec![counted(NEW, AT + 6_000, 0)]),
+    ];
+    let edit = || Amendment::Edit { text: Some("l0!".into()), spans: None, summary: "l0!".into() };
+    let all: Vec<String> = ids("l", 0..5);
+    let from1: Vec<String> = ids("l", 1..5);
+    type Moves = Vec<(&'static str, Vec<Change>, Vec<String>)>;
+    let moves: Moves = vec![
+        ("a copy of l0 from history", vec![history_of(LID, "l0", AT + 10)], all.clone()),
+        ("a live copy of l0", vec![theirs(LID, "l0", AT + 10)], all.clone()),
+        ("a reaction to l0", vec![amend(LID, "l0", AT + 6_100, false, Amendment::React { emoji: "👍".into() })], all.clone()),
+        ("an edit of l0", vec![amend(LID, "l0", AT + 6_100, false, edit())], all.clone()),
+        ("l0 starred here", vec![amend(LID, "l0", AT + 6_100, true, Amendment::Star { starred: true })], all.clone()),
+        (
+            "l0 read here, named by the LID",
+            vec![Change::Read(Read { chat: LID.into(), ids: vec!["l0".into()], here: true, ..Read::default() })],
+            all.clone(),
+        ),
+        (
+            "l0 read here, named by her number",
+            vec![Change::Read(Read { chat: NEW.into(), ids: vec!["l0".into()], here: true, ..Read::default() })],
+            all.clone(),
+        ),
+        ("a phone read by her LID naming l0, which tells WhatsApp of it", vec![naming(LID, "l0")], from1.clone()),
+        ("a phone read by her number naming l0, which doesn't", vec![naming(NEW, "l0")], all.clone()),
+        ("l0 deleted for you", vec![amend(LID, "l0", AT + 6_100, true, Amendment::DeleteForMe)], from1.clone()),
+        ("a copy of l1, not the oldest", vec![theirs(LID, "l1", AT + 11)], all.clone()),
+        ("copies of l0 and l1 in one transaction", vec![], all.clone()),
+        ("copies of l0, then of l1", vec![theirs(LID, "l0", AT + 10), theirs(LID, "l1", AT + 11)], all.clone()),
+        (
+            "a copy of l0, a phone read by her number again, then a copy of l1",
+            vec![theirs(LID, "l0", AT + 10), phone(NEW, AT + 7_000), theirs(LID, "l1", AT + 11)],
+            all.clone(),
+        ),
+        ("nothing", vec![], all.clone()),
+    ];
+    let mut differs = Vec::new();
+    let mut variants = 0;
+    for chain in [false, true] {
+        for (opened, open) in &openers {
+            for (moved, moving, want) in &moves {
+                if chain && !matches!(*moved, "a live copy of l0" | "nothing") {
+                    continue;
+                }
+                variants += 1;
+                let home = tempfile::tempdir().unwrap();
+                let store = Store::open(home.path()).unwrap();
+                let mut changes: Vec<Change> = (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect();
+                changes.extend((0..5).map(|i| theirs(LID, &format!("l{i}"), AT + 10 + i)));
+                if chain {
+                    changes.extend((0..3).map(|i| theirs(LID2, &format!("m{i}"), AT + 5 + i)));
+                }
+                store.apply(Lane::Live, changes).await.unwrap();
+                store.apply(Lane::Live, vec![phone(NEW, AT + 5_000)]).await.unwrap();
+                let person = if chain { vec![NEW, LID, LID2] } else { vec![NEW, LID] };
+                let person = Person { jids: person.into_iter().map(String::from).collect(), ..Person::default() };
+                store.apply(Lane::Live, vec![Change::Person(person)]).await.unwrap();
+                store.apply(Lane::Live, open.clone()).await.unwrap();
+                let told: Option<i64> = query(&store, |db| {
+                    Ok(db.query_row("SELECT told_at FROM chats WHERE id = ?1", [NEW], |r| r.get(0))?)
+                })
+                .await;
+                if told.is_some() {
+                    differs.push(format!("{opened}: told {told:?} after it, not None"));
+                }
+                let moving = if chain && *moved == "a live copy of l0" {
+                    vec![theirs(LID2, "m0", AT + 5)]
+                } else {
+                    moving.clone()
+                };
+                if *moved == "copies of l0 and l1 in one transaction" {
+                    store.apply(Lane::Live, vec![theirs(LID, "l0", AT + 10), theirs(LID, "l1", AT + 11)]).await.unwrap();
+                }
+                for change in moving {
+                    let applied = store.apply(Lane::Live, vec![change]).await.unwrap();
+                    assert_eq!(applied.quarantined, 0, "{opened}, then {moved}: quarantined");
+                }
+                merge_all(&store).await;
+                let mut want = want.clone();
+                if chain {
+                    want = ids("m", 0..3).into_iter().chain(want).collect();
+                }
+                // While the phone's counts stand, a read is taken even if it's earlier (set_read's
+                // `taken`), so one naming l0 moves the position back to it: l1 to l4 count unread,
+                // and only l0 is owed, unless the LID's read told WhatsApp of it.
+                let mut unread = 0;
+                if opened.contains("counts") && (moved.contains("read here") || moved.contains("naming l0")) {
+                    want = if moved.contains("her LID naming") { vec![] } else { s(&["l0"]) };
+                    unread = 4;
+                }
+                let got = owes(&store, NEW).await;
+                let name = format!("{}{opened}, then {moved}", if chain { "chain: " } else { "" });
+                if got.0 == want && got.1 == unread {
+                    println!("{name}: ok, owes {:?}", got.0);
+                } else {
+                    println!("{name}: DIFFERS, owes {:?} with unread {}, the rule {want:?} with {unread}", got.0, got.1);
+                    differs.push(name);
+                }
+            }
+        }
+    }
+    println!("window3: {} of {variants} variants differ: {differs:?}", differs.len());
+}
+
+/// Round three: what I2's line costs. Her number's chat with 5 messages read on the phone past
+/// them, her LID's with `n` never read, joined, and a phone read by her number, so "told" is None;
+/// then one live copy of her LID's oldest, or history bringing copies of all `n` at once, each
+/// moved out of turn. Run with JAW_AUDIT_MUT=i2-out to compare.
+#[tokio::test(flavor = "multi_thread")]
+#[ignore]
+async fn audit_measure_i2() {
+    async fn opened(n: i64) -> (tempfile::TempDir, Store) {
+        let home = tempfile::tempdir().unwrap();
+        let store = Store::open(home.path()).unwrap();
+        let mut changes: Vec<Change> = (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect();
+        changes.extend((0..n).map(|i| theirs(LID, &format!("l{i}"), AT + 10 + i)));
+        for lot in changes.chunks(500) {
+            store.apply(Lane::Live, lot.to_vec()).await.unwrap();
+        }
+        store.apply(Lane::Live, vec![phone(NEW, AT + 50_000)]).await.unwrap();
+        store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
+        store.apply(Lane::Live, vec![phone(NEW, AT + 60_000)]).await.unwrap();
+        (home, store)
+    }
+    let mode = std::env::var("JAW_AUDIT_MUT").unwrap_or_default();
+    for n in [5i64, 1_000] {
+        let mut took = Vec::new();
+        for _ in 0..7 {
+            let (_home, store) = opened(n).await;
+            took.push(store.apply(Lane::Live, vec![theirs(LID, "l0", AT + 10)]).await.unwrap().took);
+        }
+        let (mid, most) = median(took);
+        println!("I2 [{mode}] one live copy of the oldest, {n} in the chat going: median {mid:?}, most {most:?}");
+    }
+    for n in [200i64, 1_000] {
+        let (mut took, mut transactions) = (Vec::new(), Vec::new());
+        for _ in 0..3 {
+            let (_home, store) = opened(n).await;
+            let copies: Vec<Change> = (0..n).map(|i| history_of(LID, &format!("l{i}"), AT + 10 + i)).collect();
+            let applied = store.apply(Lane::Background, copies).await.unwrap();
+            took.push(applied.took);
+            transactions.push(applied.transactions);
+            merge_all(&store).await;
+            let owed = owes(&store, NEW).await.0;
+            println!("I2 [{mode}] history with copies of all {n}: the merge ends owing {} in the first lot", owed.len());
+            if mode.is_empty() {
+                assert_eq!(owed.len() as i64, n.min(outbox::RECEIPTS_AT_ONCE as i64), "what's owed");
+            }
+        }
+        let (mid, most) = median(took);
+        println!(
+            "I2 [{mode}] history with copies of all {n}: median {mid:?}, most {most:?}, transactions {transactions:?}, \
+             about {:?} a message",
+            mid / n as u32
         );
     }
 }
 
+/// Round three: I2's line where its walk is longest. merge_reads finds the going chat's oldest
+/// message from others past where WhatsApp was told of it, walking past yours, so with `n` of yours
+/// before her one message in her LID's chat, each of yours moved out of turn walks what's left of
+/// them: history bringing copies of all `n` during the merge walks about n²/2 in all. Oldest first
+/// and newest first. Run with JAW_AUDIT_MUT=i2-out to compare.
+#[tokio::test(flavor = "multi_thread")]
+#[ignore]
+async fn audit_measure_i2_mine() {
+    let mode = std::env::var("JAW_AUDIT_MUT").unwrap_or_default();
+    for n in [1_000i64, 3_000] {
+        for newest_first in [false, true] {
+            let (mut took, mut transactions) = (Vec::new(), Vec::new());
+            for _ in 0..3 {
+                let home = tempfile::tempdir().unwrap();
+                let store = Store::open(home.path()).unwrap();
+                let mut changes: Vec<Change> = (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect();
+                changes.extend((0..n).map(|i| mine_from(LID, &format!("y{i}"), AT + 10 + i)));
+                changes.push(theirs(LID, "l0", AT + 10 + n));
+                for lot in changes.chunks(500) {
+                    store.apply(Lane::Live, lot.to_vec()).await.unwrap();
+                }
+                store.apply(Lane::Live, vec![phone(NEW, AT + 50_000)]).await.unwrap();
+                store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
+                store.apply(Lane::Live, vec![phone(NEW, AT + 60_000)]).await.unwrap();
+                let mut copies: Vec<Change> = (0..n)
+                    .map(|i| {
+                        let mine = Message { sender: None, mine: true, status: Some("sent"), ..text(LID, &format!("y{i}"), AT + 10 + i, false) };
+                        Change::Message(mine)
+                    })
+                    .collect();
+                if newest_first {
+                    copies.reverse();
+                }
+                let applied = store.apply(Lane::Background, copies).await.unwrap();
+                took.push(applied.took);
+                transactions.push(applied.transactions);
+                merge_all(&store).await;
+                assert_eq!(owes(&store, NEW).await.0, s(&["l0"]), "[{mode}] l0 owed once the merge ends");
+            }
+            let (mid, most) = median(took);
+            println!(
+                "I2 [{mode}] history with copies of {n} of yours before her one message, {}: median {mid:?}, most {most:?}, \
+                 transactions {transactions:?}, about {:?} a message",
+                if newest_first { "newest first" } else { "oldest first" },
+                mid / n as u32
+            );
+        }
+    }
+}
+
+/// Round three: the walk at its longest. Her LID's l0, read on the phone by her LID, then `n` of
+/// yours sent here, which read the chat without telling WhatsApp, so "told" stays at l0, then her
+/// l1: merge_reads walks your `n` to find l1. Joined, a phone read by her number, then history
+/// bringing copies of your `n`, oldest first, each moved out of turn.
+#[tokio::test(flavor = "multi_thread")]
+#[ignore]
+async fn audit_measure_i2_here() {
+    let mode = std::env::var("JAW_AUDIT_MUT").unwrap_or_default();
+    for n in [1_000i64, 3_000] {
+        let (mut took, mut transactions) = (Vec::new(), Vec::new());
+        for _ in 0..3 {
+            let home = tempfile::tempdir().unwrap();
+            let store = Store::open(home.path()).unwrap();
+            let changes: Vec<Change> = (0..5).map(|i| theirs(NEW, &format!("n{i}"), AT + i)).collect();
+            store.apply(Lane::Live, changes).await.unwrap();
+            store.apply(Lane::Live, vec![theirs(LID, "l0", AT + 10)]).await.unwrap();
+            store.apply(Lane::Live, vec![phone(LID, AT + 10)]).await.unwrap();
+            let sent: Vec<Change> = (0..n)
+                .map(|i| {
+                    let message =
+                        Message { sender: None, mine: true, status: Some("pending"), ..text(LID, &format!("s{i}"), AT + 11 + i, true) };
+                    Change::Outbox(Step::Enqueue { message: Box::new(message), kind: "text", raw: vec![1], key: None, written: Written::default() })
+                })
+                .collect();
+            for lot in sent.chunks(500) {
+                store.apply(Lane::Live, lot.to_vec()).await.unwrap();
+            }
+            store.apply(Lane::Live, vec![theirs(LID, "l1", AT + 20 + n)]).await.unwrap();
+            store.apply(Lane::Live, vec![phone(NEW, AT + 50_000)]).await.unwrap();
+            store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
+            store.apply(Lane::Live, vec![phone(NEW, AT + 60_000)]).await.unwrap();
+            let copies: Vec<Change> = (0..n)
+                .map(|i| {
+                    let mine = Message { sender: None, mine: true, status: Some("sent"), ..text(LID, &format!("s{i}"), AT + 11 + i, false) };
+                    Change::Message(mine)
+                })
+                .collect();
+            let applied = store.apply(Lane::Background, copies).await.unwrap();
+            took.push(applied.took);
+            transactions.push(applied.transactions);
+            merge_all(&store).await;
+            println!("I2 [{mode}] sent here, {n}: the merge ends owing {:?}", owes(&store, NEW).await);
+        }
+        let (mid, most) = median(took);
+        println!(
+            "I2 [{mode}] history with copies of {n} of yours sent here before her l1: median {mid:?}, most {most:?}, \
+             transactions {transactions:?}, about {:?} a message",
+            mid / n as u32
+        );
+    }
+}
+
 fn median(mut xs: Vec<Duration>) -> (Duration, Duration) {
     xs.sort();
     (xs[xs.len() / 2], *xs.last().unwrap())
```

## Appendix G: the switches

A scratch commit on `4225199c`: round two's switches (Appendix D) but `i2-fix`, which is now the head's line, and round three's for that line. `i2-out` takes it out, `i2-after` runs `merge_reads` after the move instead, and `i2-skip-read`, `i2-skip-located` and `i2-skip-upsert` take it out for one way in each, which a thread-local names. With no switch on, the build behaves as the head. It also includes the probe in the merge tests, and the plan test fails with it, since the switches' SQL isn't pinned.

```diff
diff --git a/crates/jawd/src/store/write.rs b/crates/jawd/src/store/write.rs
index b29518f9..520f8883 100644
--- a/crates/jawd/src/store/write.rs
+++ b/crates/jawd/src/store/write.rs
@@ -712,6 +712,12 @@ pub(super) const THROUGH: i64 = i64::MAX;
 /// Before every message: where a chat with no position is read up to.
 const START: (i64, i64) = (i64::MIN, i64::MIN);
 
+/// The audit's switches: whether mutant `name` is on, from JAW_AUDIT_MUT (comma-separated).
+pub(crate) fn audit_mut(name: &str) -> bool {
+    static MUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
+    MUT.get_or_init(|| std::env::var("JAW_AUDIT_MUT").unwrap_or_default()).split(',').any(|m| m == name)
+}
+
 /// Where a chat was read up to and how far WhatsApp has been told, as settle and set_read read it.
 #[derive(Clone, Debug, PartialEq, Eq)]
 pub(super) struct ChatRead {
@@ -907,13 +913,18 @@ fn settle_each(db: &Connection, stamps: &mut Stamps, unsettled: Vec<(i64, Unsett
             None => {
                 // While WhatsApp knew as much as jaw, it was told up to where jaw was read, and what
                 // moved the position past a message that came under another JID says nothing of it.
-                if now.told.is_none()
-                    && now.at != was.at
+                if !audit_mut("fix-out")
+                    && !audit_mut("settle-out")
+                    && now.told.is_none()
+                    && (audit_mut("settle-moved-off") || now.at != was.at)
                     && let Some(to) = now.at
-                    && several_jids(db, chat)?
+                    && (audit_mut("settle-guard-off") || several_jids(db, chat)?)
                 {
-                    let from = told_now(db, chat, &was)?;
-                    if moved.mine_by_another || came_elsewhere(db, chat, Some(from), to)? {
+                    let from =
+                        if audit_mut("settle-from-at") { was.at.unwrap_or(START) } else { told_now(db, chat, &was)? };
+                    if (moved.mine_by_another && !audit_mut("settle-flag-off"))
+                        || (!audit_mut("settle-elsewhere-off") && came_elsewhere(db, chat, Some(from), to)?)
+                    {
                         now.told = Some(from);
                     }
                 }
@@ -922,6 +933,7 @@ fn settle_each(db: &Connection, stamps: &mut Stamps, unsettled: Vec<(i64, Unsett
                 // doesn't move for one of yours that came under another of the person's JIDs, nor
                 // past a message from others that did, which a merge leaves owed.
                 let known = match known {
+                    _ if audit_mut("known-off") => known,
                     Some(to) if !moved.mine_by_another && !came_elsewhere(db, chat, now.told, to)? => Some(to),
                     _ => None,
                 };
@@ -973,6 +985,7 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
     if !read.ids.is_empty() {
         if merging_into(db, chat)? {
             for id in &read.ids {
+                audit_caller("read");
                 moved_from_merging(db, stamps, chat, id)?;
             }
         }
@@ -991,12 +1004,27 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
     let Some(was) = chat_read(db, chat)? else { return Ok(false) };
     let mut now = was.clone();
     let taken = !was.known || was.phone_unread.is_some() || was.at.is_none_or(|read| read < at);
-    let several = !read.here && several_jids(db, chat)?;
+    let old = audit_mut("fix-out") || audit_mut("set-read-out");
+    let several = !read.here && (audit_mut("set-several-all") || several_jids(db, chat)?);
     let told = match was.told {
-        None if several => Some(told_now(db, chat, &was)?),
+        None if several && !audit_mut("set-told-none") => Some(if audit_mut("set-from-at") {
+            was.at.unwrap_or(START)
+        } else {
+            told_now(db, chat, &was)?
+        }),
         told => told,
     };
-    let speaks = !read.here && if several { !came_other(db, chat, told, at, &by)? } else { own };
+    let other_by = if audit_mut("other-own-chat") { jid.clone() } else { by.clone() };
+    let speaks = if old {
+        !read.here && own && !came_elsewhere(db, chat, was.told, at)?
+    } else {
+        !read.here
+            && if several {
+                (!audit_mut("set-x4-kept") || own) && !came_other(db, chat, told, at, &other_by)?
+            } else {
+                own
+            }
+    };
     if read.here {
         if !taken {
             return Ok(false);
@@ -1011,7 +1039,7 @@ fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
         now.marked = false;
         if speaks {
             now.told = None;
-        } else if several {
+        } else if several && !old && !audit_mut("set-not-kept") {
             now.told = told;
         }
     } else if speaks {
@@ -1048,26 +1076,30 @@ fn several_jids(db: &Connection, chat: i64) -> Result<bool> {
 /// another JID than `jid`.
 fn came_other(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64), jid: &str) -> Result<bool> {
     let Some(from) = told else { return Ok(false) };
-    Ok(db
-        .prepare_cached(
-            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
-                AND mine = 0 AND kind NOT IN ('system', 'call')
-                AND sender IS NOT (SELECT j.n FROM person_jids j WHERE j.jid = ?6))",
-        )?
-        .query_row(params![chat, from.0, from.1, to.0, to.1, jid], |r| r.get(0))?)
+    let low = if audit_mut("other-low") { ">=" } else { ">" };
+    let high = if audit_mut("other-high") { "<" } else { "<=" };
+    let mine = if audit_mut("other-mine") { "" } else { "AND mine = 0" };
+    let kinds = if audit_mut("other-kinds") { "" } else { "AND kind NOT IN ('system', 'call')" };
+    let not = if audit_mut("other-null") { "<>" } else { "IS NOT" };
+    let sql = format!(
+        "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) {low} (?2, ?3) AND (at, n) {high} (?4, ?5)
+            {mine} {kinds}
+            AND sender {not} (SELECT j.n FROM person_jids j WHERE j.jid = ?6))"
+    );
+    Ok(db.prepare_cached(&sql)?.query_row(params![chat, from.0, from.1, to.0, to.1, jid], |r| r.get(0))?)
 }
 
 /// Whether chat `chat` has a message from others after `told` and up to `to` that came under
 /// another of its person's JIDs than the chat's own, as a merge brings them.
 fn came_elsewhere(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64)) -> Result<bool> {
     let Some(from) = told else { return Ok(false) };
-    Ok(db
-        .prepare_cached(
-            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
-                AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IN
-                    (SELECT j.n FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.n = ?1 AND j.jid <> c.id))",
-        )?
-        .query_row(params![chat, from.0, from.1, to.0, to.1], |r| r.get(0))?)
+    let high = if audit_mut("range-high") { "<" } else { "<=" };
+    let sql = format!(
+        "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) {high} (?4, ?5)
+            AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IN
+                (SELECT j.n FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.n = ?1 AND j.jid <> c.id))"
+    );
+    Ok(db.prepare_cached(&sql)?.query_row(params![chat, from.0, from.1, to.0, to.1], |r| r.get(0))?)
 }
 
 /// Merges chat `gone`'s read position into `keep`'s, as a merge starts and again just before
@@ -1963,13 +1995,33 @@ fn moved_from_merging(db: &Connection, stamps: &mut Stamps, keep: i64, id: &str)
     let Some((gone, n)) = found else { return Ok(None) };
     // Where the chat going was read is merged into the kept chat's before the message moves, as
     // before a step's, since what's owed for it is found only while it's still there.
-    merge_reads(db, stamps, keep, gone)?;
+    let caller = AUDIT_CALLER.with(|c| c.replace(""));
+    let skip = audit_mut("i2-out")
+        || audit_mut("i2-after")
+        || (caller == "read" && audit_mut("i2-skip-read"))
+        || (caller == "located" && audit_mut("i2-skip-located"))
+        || (caller == "upsert" && audit_mut("i2-skip-upsert"));
+    if !skip {
+        merge_reads(db, stamps, keep, gone)?;
+    }
     let moved = move_merged(db, stamps, gone, keep, n)?;
+    if audit_mut("i2-after") {
+        merge_reads(db, stamps, keep, gone)?;
+    }
     show_newest(db, stamps, gone)?;
     show_newest(db, stamps, keep)?;
     Ok(moved)
 }
 
+thread_local! {
+    /// The audit's switches: which caller moved a message out of turn.
+    static AUDIT_CALLER: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
+}
+
+fn audit_caller(caller: &'static str) {
+    AUDIT_CALLER.with(|c| c.set(caller));
+}
+
 /// The chat and n of message `id` in the chat `chat_jid` (already resolved by chat_jid), moving it
 /// there first from a chat being merged into it.
 fn located(db: &Connection, stamps: &mut Stamps, chat_jid: &str, id: &str) -> Result<Option<(i64, i64)>> {
@@ -1982,7 +2034,10 @@ fn located(db: &Connection, stamps: &mut Stamps, chat_jid: &str, id: &str) -> Re
     Ok(match found {
         None => None,
         Some((chat, Some(n))) => Some((chat, n)),
-        Some((chat, None)) => moved_from_merging(db, stamps, chat, id)?.map(|n| (chat, n)),
+        Some((chat, None)) => {
+            audit_caller("located");
+            moved_from_merging(db, stamps, chat, id)?.map(|n| (chat, n))
+        }
     })
 }
 
@@ -2127,6 +2182,7 @@ fn upsert_message(db: &Connection, stamps: &mut Stamps, message: &Message) -> Re
     if db.prepare_cached("SELECT 1 FROM tombstones WHERE chat = ?1 AND id = ?2")?.exists(params![chat, message.id])? {
         return Ok(false);
     }
+    audit_caller("upsert");
     moved_from_merging(db, stamps, chat, &message.id)?;
     let sender = message.sender.as_deref().map(|jid| ensure_jid(db, stamps, jid)).transpose()?;
     let existing: Option<Kept> = db
@@ -2275,7 +2331,7 @@ fn upsert_message(db: &Connection, stamps: &mut Stamps, message: &Message) -> Re
         (true, Sent::Elsewhere) => {
             moved.mine = moved.mine.max(place);
             moved.mine_live |= message.live;
-            moved.mine_by_another |= user_jid(&message.chat) != chat_id;
+            moved.mine_by_another |= !audit_mut("flag-off") && user_jid(&message.chat) != chat_id;
         }
         (true, Sent::Forwarded) => {}
         (false, _) if !message.live => moved.history = moved.history.max(place),
diff --git a/crates/jawd/src/store/write/outbox.rs b/crates/jawd/src/store/write/outbox.rs
index d685b665..cbce9bc7 100644
--- a/crates/jawd/src/store/write/outbox.rs
+++ b/crates/jawd/src/store/write/outbox.rs
@@ -1285,6 +1285,7 @@ pub fn owed(db: &Connection, chat: i64) -> Result<Owing> {
     // Where WhatsApp would be told up to if it knew as much as jaw: the position, but before the
     // phone's own unread messages while its counts stand.
     let (at, n) = match super::chat_read(db, chat)? {
+        Some(_) if super::audit_mut("bound-off") => (at, n),
         Some(read) => super::told_now(db, chat, &super::ChatRead { told: None, ..read })?,
         None => (at, n),
     };
diff --git a/crates/jawd/src/sync/merge/tests.rs b/crates/jawd/src/sync/merge/tests.rs
index f13e6c5f..843a1130 100644
--- a/crates/jawd/src/sync/merge/tests.rs
+++ b/crates/jawd/src/sync/merge/tests.rs
@@ -1943,3 +1943,6 @@ async fn in_random_histories_of_reads_and_a_merge_a_phone_read_leaves_owed_what_
         no_read_cancels_what_it_cant_vouch_for(seed).await;
     }
 }
+
+#[path = "audit.rs"]
+mod audit;
```

# Verdict of round three (`4225199c`)

T2 and I2 are closed, each by tests that fail without the change, the rebase onto `c50a2ba3` is clean, and nothing in the range loses data or leaves jaw.db inconsistent. U1, U2 and O1 to O5 can stand as follow-ups.

To land: commit this note in the branch, and give the ten commits `scripts/check-data-audit.py c50a2ba3 4225199c` names the trailer `Data-Audit: docs/audits/2026-10-29-merge-followups.md`. They're all but `ed7df873`, which changes only `protocol/README.md`: `0674f718`, `de32fa08`, `126c1e53`, `e92ef62a`, `c7204409`, `5163a967`, `758f0890`, `b416e095`, `ae8e9e3f` and `4225199c`, by today's hashes. The script reads the note's last `Verdict:` line, which is this round's, so `make check-audit` then passes. If main moves first, the lander rebases and re-checks:
- `git range-diff` finds the eleven unchanged, or each conflict resolved by keeping both sides. A change on main to `set_read`, `settle_each`, `several_jids`, `came_other`, `came_elsewhere`, `told_now`, `merge_reads`, `merge_step`, `moved_from_merging`, `move_merged`, `located`, `upsert_message`'s `mine_by_another`, `outbox::owed` or the `Told` step, or a new place that changes a message's chat, needs another round first.
- A conflict in `plans.txt` is resolved by writing it again (`JAW_PLANS=write cargo test -p jawd plans`) on the merged sources alone, and the plan test passes with no new plan scanning a large table or sorting in a temporary b-tree.
- The store, sync and send tests pass on the result, the 187 focused among them, S10's, S11's and the window test with them.
- If media in (M3, `m3-media-in`) lands first: it changes `fold`, `upsert_message`'s media and `apply_amendment` in `store/write.rs`, adds migration 20, `store/write/media.rs`, `copies.rs` and about 380 lines of `plans.txt`, and touches none of the functions above. Its media steps (`store/write/media.rs:470`) and HD copies (`Amendment::Hd`) find their message through `located`, so one they name while its chat merges moves through `moved_from_merging`, and I2's line covers it; M3 moves messages no other way. Expect conflicts in `plans.txt`, and perhaps beside `upsert_message`. With M3 in, the window test and the merge tests must still pass.

Verdict: approved
