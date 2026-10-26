# Audit: the follow-ups to M2's message actions

# Second audit: `3644d99`, the first audit's T6, T7 and rebase, and H25, H26 and H29

Range: `cc7fad7..3644d99` in `m2-followups` (worktree `~/src/jaw-m2`), 26 October 2026. Auditor: database-expert. The first audit is kept below as written, with its verdict at the end, and this audit's verdict is the note's last line. Line numbers are at `3644d99`, and paths are under `crates/jawd/src/` unless they say otherwise.

The branch is two commits on `cc7fad7`, which is still main's head (`git ls-remote`), with nothing uncommitted but this note:
- `3063023` is the first audit's `2b36ea6` rebased: `git range-diff af459e5..2b36ea6 cc7fad7..3063023` finds the patch unchanged.
- `3644d99` answers the first audit. Its one change to product code is in `send/outbox.rs:393-396`, where the withdrawal's notice now waits on what the step changed. The rest is tests (`send/tests.rs`, `store/write/outbox.rs`, `engine/demo.rs`) and a comment in `crates/jaw/src/wire/tests.rs`.

No SQL, schema, migration, PRAGMA, index, plan or acknowledgement changed. `store/schema.rs` and `store/migrations.lock` are byte for byte as on `cc7fad7` and on `af459e5`, and `store/plans.rs` and `plans.txt` as at `3063023`.

## In short

Approved. Everything the first audit asked for is done, and I checked each part by switching it off. Nothing loses data or leaves it inconsistent.

- **T6 is pinned.** The test now refuses, at the thousand, a deletion for you of your own queued message and of your unconfirmed one, and checks that each takes no place. It fails with the first audit's switch (any row of the message frees a place) and under six other switches that widen, narrow or break what frees one. One narrower widening still passes the suite: a failed action aimed at the message counted as freeing a place (H31).
- **T7 is pinned.** The new test fails when edits aren't withdrawn, when an edit's notice says "reaction", and when it says nothing.
- **H26 can't hide a notice that's owed, or say one twice.** The notice now follows only a step that dropped the row, which can happen once per row. The one case it now keeps quiet, a receipt landing between the attempt's read and its step, is one where the reaction went and is stored; at `3063023` that case said it wasn't sent. No suite test reaches that case (H32).
- **H25 and H29 are done.** Without the group event, the demo's test fails in 10.0 s.
- **What the first audit found holding still holds:** the thousand, the withdrawal in every order it tried, migration 10 and its lock, the pinned plans and the derived counters.
- **New, in the first commit:** a reaction or an edit still to go whose message is deleted for everyone and then for you before it goes is sent, with no notice, against the README's new sentence (H30). jaw.db stays right, so it stands with H24 as a follow-up.
- **The gate passes at `3644d99`:** formatting and clippy are clean, and jawd's and jaw's suites passed whole in one run at load 33 to 51. Two load-sensitive tests that are main's, unchanged here, failed once each in earlier runs at load 20 to 53, and passed alone.

## The first audit's items

- **T6, done** (`send/tests.rs:2091`). 998 failed messages of yours, one queued and one unconfirmed fill the outbox. A deletion for you of someone else's message, of the queued one and of the unconfirmed one is each error 5, leaves the count at (1,000, 998) and leaves the message there. A deletion for you of a failed one is taken, and its place goes to the app state patch (1,000, 997). With the first audit's switch, `FAILED_ROW` (`store/write/outbox.rs:238`) widened to any row of the message, the test fails at the queued message. The other switches are in Measurements.
- **T7, done** (`send/tests.rs:2190`). An edit is queued while WhatsApp can't be reached, then your phone deletes its message for everyone. Once WhatsApp can be reached, only the message has gone (`kinds_sent` names an edit "an edit"), the one notice is "Your edit wasn't sent: the message was deleted for everyone", and the row is deleted and not edited. With the first audit's switch, `Outboxed::withdrawn` (`store/write/outbox.rs:1106`) narrowed to reactions, it fails with `["chego às 20h", "an edit"]` sent.
- **The rebase, done,** as above.
- **H25, done** (`engine/demo.rs:791`). A 10-second timeout wraps the loop. With the group event switched off, the test fails in 10.00 s alone (load 26), listing the four events that came, and the whole suite finishes in 94 s, where the first audit's run hung until it was killed after 10 minutes.
- **H26, done** (`send/outbox.rs:394`, and the test at `store/write/outbox.rs:2347`). The notice is pushed only when `applied.changed > 0`. The new store test queues two reactions on Ana's message, leaves one waiting for a receipt, revokes the message and withdraws both: only the queued one goes, and the unconfirmed one's row stays. It fails if the writer doesn't check that the row is still queued, which the first audit found nothing tested, and if the writer withdraws nothing. The reaction test (`send/tests.rs:2145`) and T7's test now collect the notices for half a second, so a second one fails them.
- **H29, done** (`crates/jaw/src/wire/tests.rs:1103`).
- **H24, H27 and H28** aren't touched by `3644d99`, and stand as follow-ups.

## What else was asked, checked

- **H26 can't hide a notice that's owed.** A notice is owed when the attempt's step drops the row unsent, and that's exactly when `changed` is above 0:
  - `changed` counts the changes that changed something (`store/mod.rs:645-659`). The batch holds one, the step, whose answer is the writer's `withdrawn` (`store/write/outbox.rs:408`): true only after `acted(went = false)` has dropped the row, which it does whenever the row is still queued.
  - A live write is answered only after its commit, and nothing between the commit and the answer can turn it into an error (`store/mod.rs:1185-1211`, `:1258-1267`). One rolled back is made again from the start, so its count is of what committed, and one made in an inbox group's commit that fails is made again alone (`:985`).
  - So the step changes nothing only when the row stopped being queued after the attempt read it, or when the step is quarantined. In the second case nothing was withdrawn: the row stays queued until an attempt at a later start withdraws it, and that attempt says so. At `3063023` both attempts would have said it.
  - The per-chat queue rules out a second attempt, and the outbox's other writes touch only failed or unconfirmed rows, a message's own row, or an action's target. So the only thing that can stop a queued action being queued meanwhile is a receipt for it (`receipt_for_action`, `store/write/outbox.rs:592`), for an action that may have gone. My scene injects one exactly there: the step changes nothing, the reaction is stored, and nothing is said. With the check removed, the same scene says "Your reaction wasn't sent" over the stored reaction.
  - A deletion for you landing between the read and the step (H30's order) leaves the row queued with no target: the step drops it, `changed` is 1, and the notice is said.
- **Nor say one twice.** `outbox.n` is `AUTOINCREMENT`, and the step drops a row only while it exists and is queued, so at most one step per row reports a change, whichever attempts race. Saying it twice in the attempt fails two suite tests and five scenes.
- **What the first audit found holding:**
  - **The thousand.** Its code is unchanged since `2b36ea6`. The first audit's random sequences, run again on `3644d99`, reached 1,000 and never passed it, and left no failed row that nothing could send. Its scene at the thousand gives the same counts, and its races (a deletion for you at once with a retry, or with a send) end at 1,000.
  - **The withdrawal in every order.** The first audit's scenes give its table's outcomes, row for row. A reaction is withdrawn once, with one notice, when the revoke comes before the writer takes it, after it's queued, twice and replayed (events and `v` unchanged by the second and the replay, `unread` 2 before and after), from history's stub, and through either merge fold. An edit is withdrawn with its notice. H24's orders behave as the first audit found them.
  - **Migration 10 and its lock,** unchanged, as above. Its test and the frozen-text test pass, and the first audit's mutants give its table: each guard caught, and the `app_state` exclusion surviving (H28).
  - **The pinned plans.** No statement changed, and the plan test passes.
  - **Counters.** The withdrawal is still `acted(went = false)`, which writes no count.

## Findings

### Data loss

None.

### Inconsistency

None.

### Tests (missing)

None. Each rule the change touches has a test that fails without it (Measurements). The two switches the suite misses are written down below (H31, H32), with why neither needs to hold the branch.

### Unpredictable performance

None. `3644d99` adds no statement and no write: the notice's check reads a count the writer already returns. The first audit's measurements stand.

### Hygiene (follow-ups; none blocks)

- **H30. A reaction or an edit still to go is sent when its message is deleted for everyone and then for you before it goes.**
  - **Where:** in `3063023`, the first commit, not `3644d99`. A deletion for you deletes the message's row (`delete_row`, `store/write.rs:953`), and `outbox.target` is `ON DELETE SET NULL` (`store/schema.rs:672`), so the action loses its target. `OUTBOXED` then reads `coalesce(t.deleted, 0)` as 0 (`store/write/outbox.rs:218`), and `Outboxed::withdrawn` is false. A deletion for you isn't refused for a message deleted for everyone (`send/actions.rs:384`), and one from your phone's app state goes through `delete_row` too (`store/write.rs:1819`).
  - **What happens:** measured with WhatsApp out of reach until both deletions were in.
    - A reaction to Ana's message, which she deletes for everyone and you then delete for you: the reaction is sent, nothing is said, and nothing is stored.
    - An edit of your message, which your phone deletes for everyone and you then delete for you: the edit is sent, with its new text (`["chego às 20h", "an edit"]`), and nothing is said.
    - The README's new sentence (`protocol/README.md:312`) says neither is sent, and that a notice says so. Outside a test, both deletions have to reach jawd while the action still waits: behind its message, behind its chat's earlier sends, or between tries.
  - **Why not inconsistency:** jaw.db keeps nothing of either action. The message is gone for you, with its tombstone; the action's row goes once it's sent; and a receipt for it meets the action's own tombstone. Nothing dangles or is counted wrong. What differs is what WhatsApp is sent, as in H24.
  - **Fix:** with H24, decide the withdrawal while the message's row still says it was deleted for everyone. For example, the revoke's transaction, or a deletion for you of a message deleted for everyone, could withdraw your actions still queued on it, with the notice said by whoever asked; or the action's row could keep that its message was deleted for everyone.
- **H31. T6's test doesn't see a kept action counted as freeing a place.**
  - **Where:** `FAILED_ROW` (`store/write/outbox.rs:238`) and the check at `:530`.
  - **What happens:** with `FAILED_ROW` widened to a failed row of the message or aimed at it, `(message = ?1 OR target = ?1)`, the whole suite passes. At the thousand, a deletion for you of a message with a reaction kept for a late receipt is then taken. It frees nothing, since kept actions don't count (`WAITING`, `:211`) and the deletion removes only the message's own failed row (`FORGOTTEN`, `:236`), so the outbox goes to 1,001. The code as written is right: my scene's deletion is refused with error 5 and leaves (1,000, 1,000). Under the switch the outbox stays bounded, but at 2,000, with `ACTIONS_KEPT`'s thousand.
  - **Why it needn't block:** the predicate is pinned against both natural widenings (any row of the message, any failed row anywhere), against removing it, and against the deletion leaving the failed row. This one needs a new condition on another column. Still, the first audit's T6 was this kind of gap, and a later change that drops kept actions on a deletion for you would widen both statements in just this way.
  - **Fix:** adopt `audit5_t6_past_a_thousand_a_deletion_for_you_of_a_message_with_a_kept_reaction_is_refused` (25 lines in `/tmp/jaw-m2f2-audit-s.diff`), now or with the next change to the outbox.
- **H32. Nothing in the suite reaches H26's check.**
  - **Where:** `send/outbox.rs:394`.
  - **What happens:** with the notice said whatever the step changed, as at `3063023`, the whole suite passes. Only a receipt landing between two awaits of one attempt reaches the check, and the suite can't put one there without a hook. My scene does it with a switch.
  - **Why it needn't block:** it decides what interfaces are told, not what jaw.db holds, which is right either way; and the writer's half, which does decide jaw.db, is now pinned.
  - **Fix:** if a test hook between an attempt's read and its step is ever added, the scene becomes a test.
- **Outside this range: two tests that fail under load.**
  - jaw's keypress bench (`crates/jaw/src/bench.rs:513`) failed once at load 20 to 25, with p99 61 ms against its 16 ms, beside five other tests. Alone it passed twice, with p99 3.2 to 3.3 ms.
  - `stopping`'s `jawd_whose_engine_is_missing_stops_on_sigterm_within_its_bound` failed once at load 32 to 53. After its 1.5 s wait, jawd's log had only "SIGTERM while starting", from before the supervisor's first retry. It passed 3 of 3 alone.
  - Neither touches anything this branch changes, and both passed in the third full run.
- **Still standing:** H24, H27 and H28 from the first audit; the M2 audit's H13, H14, H16, H17 and H19, and its first look's H1, H2, H7 and H9 to H12.

## Measurements

Method:
- **Scratch clones** of `~/src/jaw-m2` at `3644d99`, each with its own `CARGO_TARGET_DIR`, seeded by APFS clones of the first audit's build caches, which were only read:
  - `/tmp/jaw-m2f2-audit`, unmodified, for the gate, the repeats and the tests run alone;
  - `/tmp/jaw-m2f2-audit-s`, with the first audit's switches and scenes (`/tmp/jaw-m2f-audit-s.diff`) carried over, its switch in `attempt` redone over `3644d99`'s check, and my switches and `audit5_*` scenes added (`/tmp/jaw-m2f2-audit-s.diff`). A switch is on when `JAW_AUDIT_MUT` names it.
- **Builds:** debug, with dependencies optimised as the workspace sets them, SQLite among them.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other builders. One-minute load averages ran from 20 to 62, given with each figure.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, and no Go engine was run. jaw's tests on a real jawd ran the debug jawd with `--engine demo` and `JAW_HOME` in temporary folders; `stopping` ran jawd with a missing engine and with a shell script that sleeps. I stopped no process.

Results:
- **The gate at `3644d99`:**
  - `cargo fmt --all --check`: clean. `cargo clippy --workspace --all-targets -- -D warnings`: clean, in 66 s at load 36 to 51.
  - `cargo test -p jawd -p jaw --no-fail-fast -- --test-threads=6` with `JAWD` set, in 190 s at load 33 to 51: jaw 204 passed, the 9 on a real jawd among them; jawd 333 passed and 3 ignored; `starting` 1 and `stopping` 2 passed.
  - Two earlier runs: the first stopped at jaw's keypress bench (203 passed and 1 failed, at load 20 to 25, before jawd's suite); the second passed everything but `stopping`'s first test, in 211 s at load 32 to 53 (Hygiene).
- **The new and changed tests, 5 runs each alone,** at load 21 to 23: 30 of 30 passed. T6's took 0.15 to 0.20 s, T7's 0.63 to 0.66 s, the reaction test 0.74 to 0.81 s, the store test 0.10 to 0.15 s, the demo's under 0.01 s, and the window test 0.28 to 0.33 s.
- **Switches,** each against the whole jawd suite at 6 threads, two at a time, 94 to 138 s a run at load 34 to 59. As in the first audit, three tests were left out: the plan test, which reads the switches' own SQL; H19's flaky test; and the migration fault-injection test, which no switch touches. The scenes are the first audit's and mine.

| Switched off | Suite's tests failing | Scenes failing |
| --- | --- | --- |
| Nothing | none | none |
| T6, the first audit's: any row of the message frees a place | 1 (T6's) | 1 |
| T6: a failed or unconfirmed row frees one | 1 (T6's) | 1 |
| T6: a failed or queued row frees one | 1 (T6's) | 1 |
| T6: any failed row in the outbox frees one | 1 (T6's) | 3 |
| T6: a failed row of the message, or aimed at it, frees one | **none** (H31) | 1 |
| T6: the deletion for you leaves the failed row | 2 | 2 |
| H21: the thousand checked as before | 1 (T6's) | 1 |
| H21: no check | 2 | 3 |
| T7, the first audit's: reactions withdrawn, edits not | 1 (T7's) | 1 |
| T7: an edit's notice says "Your reaction" | 1 (T7's) | 1 |
| T7: no notice for an edit | 1 (T7's) | 1 |
| H22: no notice | 2 | 5 |
| H22: no withdrawal | 3 | 6 |
| H22: the writer withdraws nothing | 4 | 6 |
| H26: the writer doesn't check the row is still queued | 1 (the store test) | none |
| H26: the notice said whatever the step changed | **none** (H32) | none |
| H26: the notice said twice | 2 | 5 |
| H25: the demo's group event | 1 (the demo's, at 10 s) | none |

- **H26's case, injected:** a receipt for the reaction applied after the attempt read the row and before its step, at load 44. As at `3644d99`, the step changed nothing, nothing was said, and 1 reaction was stored. With the notice said whatever the step changed, the same happened, and "Your reaction wasn't sent: the message was deleted for everyone" was said.
- **H30's two orders,** at load 54: as in Hygiene. The deletion for you was taken each time, and the reaction's row was left aimed at nothing until it was sent.
- **H31's scene:** at `3644d99`, error 5 and (1,000, 1,000); with the widened `FAILED_ROW`, taken and (1,001, 1,000), at load 24.
- **The first audit's random sequences** on `3644d99`, at load 20 to 24: 600 steps on seed `0x5eed` in 5.0 s, and 1,500 on each of seeds 1, 2 and 3 in 11.0 to 13.5 s. The most waiting was 1,000 in each, and no failed row was left that nothing could send. Each 1,500-step run had 178 to 205 deletions for you taken and 1 refused at the thousand, 191 to 215 retries, and 242 to 295 sends refused at the thousand.
- **The first audit's scenes,** at load 24 to 26: all ten passed, with the outcomes in its table. At the thousand, a deletion for you at once with a retry left (1,000, 996), and at once with a send, which was refused with error 5, (1,000, 995).
- **Migration 10's mutants:** as in the first audit.

# First audit: `2b36ea6` (kept as written)

Range: `origin/main..2b36ea6` in `m2-followups`, on `af459e5`, 26 October 2026. Auditor: database-expert. Line numbers are at `2b36ea6`, and paths are under `crates/jawd/src/` unless they say otherwise.

The branch stayed at `2b36ea6` throughout, one commit, with nothing uncommitted. `origin/main` moved during the audit, from `af459e5` to `cc7fad7` (the feed for many interfaces, its audit, and a rule's wording). Those commits add no migration and touch nothing this branch changes but other paragraphs of `protocol/README.md`. In a scratch clone the branch rebases onto `cc7fad7` without conflicts, and the gate passes there (Measurements). It needs that rebase before it lands.

## In short

Changes needed: two tests are missing. Nothing here loses data or leaves it inconsistent, and each claim of the commit holds where it says it does.

- **H21 is closed.** A deletion for you of a message of yours that failed takes the place its failed row frees, so a full outbox can be emptied. No sequence I could make passed the thousand: random runs of sends, deletions, retries, reactions and forwards, 5,100 steps in all, reached 1,000 and never 1,001. But the condition that keeps the bound, that only a *failed* row frees a place, has no test (T6).
- **H22 is closed for an action still to go.** A reaction or an edit still queued or held when its message is deleted for everyone is withdrawn once, with one notice, and leaves nothing behind. That holds whichever way the deletion comes: before or after the action is queued, while it's held, arriving twice or replayed, from history, or through a merge fold in either direction. Counters are untouched, and a replay changes nothing. The edit half has no test (T7).
- **H22 isn't closed for a reaction that has gone, or may have.** One in flight when the deletion comes, or waiting for a receipt then, still ends up on the deleted row and shown, and so do others' reactions, as on main. That's how the phones differ from jaw, not jaw.db from WhatsApp, so it stands as a follow-up (H24).
- **H23 is closed.** Migration 10's text and `migrations.lock` are unchanged, and its test now fails with either guard it missed switched off.
- **The rest holds.** Mentions refused now name the person and the group, read-only and with each statement pinned. The demo's group goes through jawd's normal paths, idempotently. The new statements cost 1 to 5 µs at p99, and the withdrawal's transaction 1.0 to 3.8 ms at p99, inside the live budget.

## What was asked, checked

- **The thousand can't be passed.**
  - Every insert into the outbox checks the thousand: messages, actions and forwards before writing, and a deletion for you (`store/write/outbox.rs:530`) unless its message has a failed row. When it has one, the same transaction inserts the app state row and `delete_row` removes the failed row (`FORGOTTEN`, the same predicate on the same `n`), so the count stays.
  - Nothing else moves a row into the count. Retries, failures and unconfirmed rows only change states that already count. Failed app state rows are dropped, never left failed. Kept actions never count again.
  - Measured: at the thousand, a deletion for you of your own queued or unconfirmed message is refused with error 5, and of a failed one taken with the count unchanged. Asked again, it's error 3 and writes nothing.
  - Racing: a deletion for you at once with `send.retry` of the same message, or with a `send.text`, leaves 1,000 either way, and the send is refused.
  - Random runs (seed `0x5eed` for 600 steps, and seeds 1 to 3 for 1,500 each): the most waiting was 1,000, and no failed row was left that nothing could send.
- **A deletion for you that frees nothing is still refused when full.** It is, for someone else's message (the builder's test) and for your own queued or unconfirmed one (my scratch test), but only the first is in the suite (T6).
- **The withdrawal in every order.** My scratch scenes are in `/tmp/jaw-m2f-audit-s/crates/jawd/src/send/tests.rs`, named `audit4_*`. Each that withdraws sends nothing, leaves no outbox row or reaction on the deleted message, and pushes exactly one notice.

| Order | What happens |
| --- | --- |
| The revoke commits after `react` checked the message, before the writer takes the action | Queued, then withdrawn at its attempt |
| The revoke after the action is queued, WhatsApp out of reach | Withdrawn once it can be reached (the builder's test, and mine) |
| The action held behind your unconfirmed message, which your phone deletes | Released, then withdrawn (the builder's test) |
| The revoke arriving twice, and again after the withdrawal | Events and `v` unchanged by the second and third, `unread` unchanged, one notice |
| History's revoke stub (its amendment, then its deleted row) | Withdrawn |
| A merge folding the copy reacted to onto her number's copy deleted for everyone, and the other way round | The action is aimed at the kept copy, deleted, then withdrawn |
| An edit queued, then your phone deletes the message | Withdrawn, with "Your edit wasn't sent" |
| The reaction in flight when the revoke comes | Done once WhatsApp answers: on the deleted row and shown (H24) |
| The reaction unconfirmed when the revoke comes | Shown on the deleted row from the outbox; a receipt then stores it there (H24) |
| The same, kept after its day, then a late receipt | Hidden while kept, "WhatsApp refused your reaction" said, then stored and shown on the deleted row (H24) |
| Rui's reaction arriving before his message does, then history's stub of it | Kept on the deleted row, as on main (H24) |

- **Counters stay derived.** The withdrawal is `acted(went = false)`: it drops the row and gives the message, and the chat when it's the newest, a new `v` with its events. It writes no count, and `unread` was the same before and after.
- **Told once.** The notice is pushed by the attempt that withdraws. A row is in at most one chat's queue or the held set at a time, and a chat's jobs run one at a time, so no two attempts can withdraw it. The notice ignores whether the step changed anything, though (H26).
- **Plans.** `FAILED_ROW` and `STILL_QUEUED` are index and rowid probes, pinned in the outbox's plan test; `CHAT_NAME` is pinned in sending's; all three are in `plans.txt`. `OUTBOXED` gains `coalesce(t.deleted, 0)` from a join it already made, so its pinned plans by `n` and by ID are unchanged. No scan of a large table, no temporary b-tree and no `OFFSET` were added.
- **The demo's group.**
  - It's a `GroupChange` event, the script's last, after the three messages' acks. It's kept in the inbox, drained and converted (`convert/people.rs:86`) as a real engine's is, into a chat upsert with the name and `set_members`, which writes only a role that differs. Played again at every start, it writes nothing new.
  - It touches only the made-up group's JID and two made-up numbers. It reads or writes no file, never touches `session.db` or the spool, and starts no Go engine.
  - `jawd demo` still refuses jaw's own folder, a folder with a `session.db`, and one with a `jaw.db` it didn't make (`demo/mod.rs:117`, unchanged).
  - `jawd --engine demo` behaves as on main, as the linked-at-hello and demo-load audits recorded. It runs on `JAW_HOME`, or jaw's own folder when that's unset, where it writes the made-up account's chats, now the group's name and members too, kept out only by `jaw.lock` while the owner's jawd runs. Refusing jaw's own folder there, as `jawd demo` does, would close that, outside this range.
- **No new migration.** `store/schema.rs` and `store/migrations.lock` are byte for byte as on `af459e5`. Only migration 10's test changed.

## Findings

### Data loss

None.

### Inconsistency

None.

### Tests (missing)

These block approval: each is a rule the change touches that no test fails without.

- **T6. Only a failed row frees a place, and nothing tests that.**
  - **Where:** `store/write/outbox.rs:238` (`FAILED_ROW`, `state = 'failed'`) and `:530`. The test is `send/tests.rs:2091`.
  - **What happens:** with `FAILED_ROW` widened to any row of the message, a deletion for you of your own queued or unconfirmed message passes at the thousand. Its row stays (a queued one still goes, as on the phone, and an unconfirmed one waits out its day), so each takes a place and frees none. Measured: from 1,000, deleting a queued message and then an unconfirmed one left 1,001 and then 1,002, where `2b36ea6` refuses both with error 5 and stays at 1,000. The whole suite passes; only my scratch test fails. The builder's test refuses only someone else's message, which has no outbox row at all.
  - **Fix:** in that test, or one beside it, put a queued and an unconfirmed message of yours in the full outbox and check that a deletion for you of each is error 5 and leaves the count as it was. `audit4_b1_past_a_thousand_a_deletion_for_you_of_a_queued_or_unconfirmed_message_is_refused` does it in 30 lines.
- **T7. The edit half of the withdrawal has no test.**
  - **Where:** `Outboxed::withdrawn` (`store/write/outbox.rs:1106`) covers `"reaction" | "edit"`. Both new tests are reactions (`send/tests.rs:2133`, `:2166`).
  - **What happens:** with `withdrawn` narrowed to reactions, the suite passes, and an edit of a message deleted for everyone goes to WhatsApp. jaw.db is unharmed, since an edit never changes a deleted row, but the commit and the README say it isn't sent.
  - **Fix:** a test in which an edit of a message you sent is queued while WhatsApp can't be reached and your phone then deletes the message for everyone. Once WhatsApp can be reached, only the message has gone, and the notice is "Your edit wasn't sent: the message was deleted for everyone". `audit4_a8_an_edit_on_its_way_to_a_message_your_phone_deletes_for_everyone_isnt_sent` does it.

### Unpredictable performance

None breaks a budget.
- The new statements cost 1.2 to 2.6 µs at p50 and up to 4.9 µs at p99.
- A deletion for you of a failed message no longer scans the outbox to count it (`WAITING`, 230 µs at p50 beside 2,000 rows), and one of anything else adds a 1.5 µs probe. Both sit in a synced commit of 6.7 ms at p50.
- The withdrawal is a live transaction of 0.12 to 0.29 ms at p50 and 1.0 to 3.8 ms at p99, under the 5 ms live budget at load 17 to 27.

### Hygiene (follow-ups; none blocks)

- **H24. A reaction that has gone, or may have, still shows on a deleted message.** H22 is closed only for actions still to go.
  - **Where:** a reaction is kept whatever the row's `deleted` says (`store/write.rs:1824`), unlike an edit (`:1845`), and a revoke clears none (`:1848`). Windows read reactions whatever `deleted` says (`store/read.rs:337`), and the outbox overlay applies a queued or unconfirmed reaction to a deleted row (`store/overlay.rs:71`), where its edit arm checks (`:61`).
  - **What happens:**
    - In flight when the revoke comes: WhatsApp's answer does it (`acted`, `store/write/outbox.rs:573`), so 👍 is stored on the deleted row and shown.
    - Unconfirmed: the overlay shows it on the deleted row for up to a day, and a receipt (`receipt_for_action`, `:592`) stores it there. With no receipt, it's hidden while kept, with "WhatsApp refused your reaction: no receipt came to say it had gone the first time", and a late receipt within 30 days stores and shows it.
    - Queued while WhatsApp can't be reached: windows show it on the deleted row until the withdrawal, against the README's "the row shows without it" (`protocol/README.md:312`).
    - Others' reactions stay on a deleted row in every order, as on main, including one that waited in `pending` for a message history then brings as a revoke stub.
    - By reading, not reproduced: a reaction resumed after a crash may already have reached WhatsApp. Withdrawn, it's dropped and said not to have been sent, and its receipts are absorbed by its tombstone.
  - **Why not inconsistency:** in each case jaw.db holds what WhatsApp has, and no order leaves anything dangling, counted wrong or written twice. What differs is display: the phones show no reactions on a deleted message. The M2 audit graded H22 the same.
  - **Fix:** one rule, that a deleted message shows no reactions. Windows would leave out a deleted row's reactions (`MESSAGE_COLUMNS`), and the overlay's reaction arm would skip a deleted row as its edit arm does. That closes every order, yours and others', and drops nothing. Optionally, the revoke's own transaction could withdraw your reactions and edits on its message in any state, so none waits in the outbox aimed at a deleted message.
- **H25. Without the group event, the demo's test hangs instead of failing.**
  - **Where:** `engine/demo.rs:782` loops on `rx.recv()` until five events have come.
  - **What happens:** with the event removed it waits for ever. I stopped that run after 10 minutes; in CI it would hold the job until its timeout. The rest of jawd's suite passes without the event, so this test is the only one in jawd that sees it.
  - **Fix:** a `tokio::time::timeout` around the loop.
- **H26. The withdrawal's notice doesn't check that anything was withdrawn.**
  - **Where:** `send/outbox.rs:392` pushes it after `settle` whatever `applied.changed` says, where the no-receipt path checks it (`:353`).
  - **What happens:** the writer's `withdrawn` (`store/write/outbox.rs:408`) changes nothing if the row stopped being queued after the attempt read it. Only a receipt for a resumed action could do that (the per-chat queue rules out a second attempt), and the notice would then contradict what's stored. The writer's `STILL_QUEUED` check is untested for the same reason: with it switched off, nothing fails.
  - **Fix:** push the notice only when `changed > 0`.
- **H27. A deletion for you refused at the thousand can first move its message out of a merging chat.**
  - **Where:** `delete_for_me` now calls `located` (`store/write/outbox.rs:514`) before the thousand check (`:530`), and `located` moves a message out of a chat being merged.
  - **What happens:** when the deletion is then refused with error 5, the move stays committed. That's the same move the merge would make, with its events, so nothing is inconsistent. But "nothing is written" past the thousand no longer holds strictly in that case.
  - **Fix:** none needed. It's noted for whoever next reads that test's name.
- **H28. Migration 10's test doesn't see its `app_state` exclusion.**
  - **Where:** `store/schema.rs:690`.
  - **What happens:** removing `'app_state'` from the list survives the test. It guards rows that can't exist, since failed app state is dropped, never left failed, and removing such a row would be right anyway.
  - **Fix:** none needed; the text is frozen.
- **H29. A stale comment in jaw's wire tests.** `crates/jaw/src/wire/tests.rs:1103` still says the demo engine doesn't send a group's members. It does now, so the test's fallback branch is one the demo no longer takes.
- **Still standing from earlier audits:** the M2 audit's H13, H14, H16, H17 and H19, and its first look's H1, H2, H7 and H9 to H12. H19's flaky test failed once in my scratch baseline at load 17, and passed in both gates.

## Measurements

Method:
- **Scratch clones** of `~/src/jaw-m2` at `2b36ea6`, each with its own `CARGO_TARGET_DIR` in `/tmp`, seeded by an APFS clone of an earlier round's build cache, which was only read:
  - `/tmp/jaw-m2f-audit`, unmodified, for the gate, the runs alone, jaw's wire tests and, on a scratch branch, the rebase onto `cc7fad7`;
  - `/tmp/jaw-m2f-audit-s`, with switches turned on by `JAW_AUDIT_MUT` and my `audit4_*` scenes and migration mutants (`/tmp/jaw-m2f-audit-s.diff`);
  - `/tmp/jaw-m2f-audit-b`, with an ignored timing test (`/tmp/jaw-m2f-audit-b.diff`).
- **Builds:** debug, with dependencies optimised as the workspace sets them, SQLite among them.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other builders. Load averages ran from 11 to 47, given with each figure.
- **Not touched:** anything under `~/Library/Application Support/jaw`. No `session.db` was copied, and no Go engine was run. jaw's wire tests ran the debug jawd with `--engine demo` in temporary folders. The only process I stopped was my own test binary (PID 54722, hanging under H25's switch), by PID, with SIGKILL after SIGTERM didn't end it.

Results:
- **The gate at `2b36ea6`,** `cargo test -p jawd -p jaw -- --test-threads=6` with `JAWD` set, in 156 s at load 11 to 20:
  - jaw: 204 passed, the 9 on a real jawd among them;
  - jawd: 318 passed and 2 ignored;
  - `starting` 1 and `stopping` 2 passed.
- **Rebased onto `cc7fad7`,** the same command, in 183 s at load 15 to 21: jaw 204 passed, jawd 331 passed and 3 ignored, and `starting` and `stopping` passed.
- **The six new and changed tests, 5 runs each alone,** at load 27 to 31: 30 of 30 passed, in 0.00 to 0.25 s each. The WAL test passed 5 of 5 too (6.8 to 9.1 s), after failing once in a mutation run at load 30, where a live write waited 1.15 s.
- **jaw's tests on a real jawd,** 5 runs of the 9 at load 23 to 29: 45 of 45 passed, 15 to 16 s a run. I didn't measure main's one in ten again. The new wait is sound: the group's name is the script's last event, after the messages' acks, and the inbox converts in order.
- **Switches,** each against the whole jawd suite at 6 threads, two at a time, at load 20 to 47. Three tests were left out: the plan test, which reads my switches' own SQL; H19's flaky test; and the migration fault-injection test, since no switch touches a migration.

| Switched off | Suite's tests failing | My scenes failing |
| --- | --- | --- |
| Nothing | none | none |
| H21: the thousand checked as before | 1 | 1 |
| H21: any row of the message frees a place, not only a failed one | **none** (T6) | 1 |
| H21: no check at all | 2 | 2 |
| H22: no withdrawal | 2 | 5 |
| H22: reactions withdrawn, edits not | **none** (T7) | 1 |
| H22: no notice | 1 | 5 |
| H22: the writer withdraws nothing | 2 | 6 |
| H22: the writer doesn't check the row is still queued | none (H26) | none |
| Mentions refused by IDs | 1 | none |
| Mentions refused without the number for an unnamed person | 1 | none |
| The demo's group event | the demo test hangs (H25); the other 326 pass | none |

- **Migration 10's test against mutants of its text,** run on its own rows. Each of these is caught: either inner guard (H23's two), each of the three conditions, the outer state guard, and the outer kind guard. The `app_state` exclusion survives (H28).
- **Random sequences** of `send.text`, a deletion for you, `send.retry`, `react` and `forward` on random messages, with WhatsApp in and out of reach and refusing every send. They started from 985 failed messages and 15 of Ana's, checking the outbox after each step:
  - 600 steps on seed `0x5eed`, and 1,500 on each of seeds 1, 2 and 3, about 10 s each;
  - the most waiting was 1,000 in every run, and no failed row was left deleted for you or for everyone;
  - each 1,500-step run had 180 to 211 deletions for you taken and 1 to 3 refused at the thousand, 193 to 212 retries, and 238 to 295 sends refused at the thousand.
- **Timing** on a jaw.db of 2,000 chats and 200,000 messages. Its outbox held 1,000 waiting (800 failed messages of yours and 200 queued reactions on messages deleted for everyone) and 1,000 kept actions. Two runs at load 17 to 27; statements 20,000 times each with random keys:

| What | p50 | p99 |
| --- | --- | --- |
| `FAILED_ROW` | 1.2 to 1.6 µs | 1.9 to 2.8 µs |
| `STILL_QUEUED` | 1.4 to 1.5 µs | 2.2 to 2.6 µs |
| `OUTBOXED` by n, with its new column | 2.3 to 2.6 µs | 4.2 to 4.9 µs |
| `CHAT_NAME` | 1.4 to 1.5 µs | 2.1 to 2.2 µs |
| `WAITING`, for comparison | 230 µs | 290 µs |
| A deletion for you at the thousand, synced as the API's (200) | 6.7 to 6.8 ms | 15 to 16 ms |
| The withdrawal, a live transaction (200) | 0.12 to 0.29 ms | 1.0 to 3.8 ms |
| The same withdrawal replayed, changing nothing | 11 to 19 µs | 62 to 179 µs |

  After the 200 deletions for you, 1,000 were still waiting; after the withdrawals, no queued reaction was left.

## Verdict

Nothing here loses data or leaves it inconsistent, and the follow-ups do what they say. Before landing:
- add the two missing tests, T6 (at the thousand, a deletion for you of your own queued or unconfirmed message is refused) and T7 (an edit of a message deleted for everyone isn't sent, and that's said);
- rebase onto `origin/main`, which moved to `cc7fad7` during the audit and passes the gate with this branch on it.

Once they're added, a short second audit can confirm each fails with its switch above (`h21_any_state`, `h22_edit_off` in `/tmp/jaw-m2f-audit-s`). H24 to H29 can stand as follow-ups; H24 is best done next, since it's the rest of H22.

Verdict: changes needed

# Verdict of the second audit (`3644d99`)

T6, T7 and the rebase are done, and so are H25, H26 and H29; I checked each by switching it off. Nothing marked data loss or inconsistency remains, and every rule the change touches has a test that fails without it: the thousand's exception for a deletion for you (T6's test fails under seven of the eight ways I broke it), the withdrawal of an edit (T7's fails under each of the three edit switches and the four general ones), the notice said once, the writer's check that a row is still queued, and the demo's group.

What's left before landing:
1. Commit this note in the branch.
2. Give both commits, which touch the data layer, the trailer `Data-Audit: docs/audits/2026-10-26-m2-followups.md`.
3. Run `make check-audit`.

These may stand as follow-ups:
- H30, best done with H24 next, since together they're the rest of H22; until then, the README's new sentence promises more than jawd does in one order;
- H31, whose scene is cheap to adopt now;
- H32, and H27 and H28 from the first audit.

Verdict: approved
