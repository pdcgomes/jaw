# Audit: what jawd couldn't show, converted again

Range: `dc48139..d4fb149` in `unknown-kinds` (worktree `~/src/jaw-unknown`), 27 October 2026. Auditor: database-expert. The branch sits on `m2-content` (head `dc48139`), which isn't merged and is under an audit of its own (`docs/audits/2026-10-26-m2-content.md` in `~/src/jaw-m2c`): only these two commits are audited here, with m2-content's `backfills` table, migrations 13 and 14, the step runner (`store/write/links.rs`'s `backfill()`) and `sync/backfill.rs` read as their base. Line numbers are at `d4fb149`, and paths are under `crates/jawd/src/` unless they say otherwise.

The two commits, with nothing uncommitted in the worktree but this note:
- `4504700`: the converter's new sentences and kinds; encrypted edits and RSVPs kept in `pending` (kinds `secret` and `rsvp`) with `own_action` for yours; the dedupe of kept amendments in `amend_message` (`kinds::kept_already`); migration 15, a schema step, and the `kinds` backfill (`store/write/kinds.rs`); `remove_row`, split out of `delete_row`; and `kinds_backfill` and `encrypted_kept` in `jawd status`.
- `d4fb149`: the terminal's `invite` kind. Not the data layer, and needs no trailer.

## In short

Changes needed: a data-loss rule, a missing test, and a budget.

- **D1, data loss.** The backfill writes over the text a later edit gave a row whenever the row's raw protobuf is the original message, not an edit standing in for it.
  - `SHOWN`'s guard compares the row's `edited_at` with the conversion's, and that is NULL for any message that isn't an edit. The comparison is then NULL, so the converted text wins.
  - An applied edit keeps no raw, so its words are gone from jaw.db for good, while the row still says it was edited and the phone shows them.
  - Two scenes reach it through the writer. The owner's jaw.db has no such row today.
  - The fix is `coalesce(?12, -1)`, as `fold` writes the same comparison.
- **T1, a rule with no test.** Nothing fails with the converted message's event, or its chat's, left out; the test checks only that `v` rose. The feed builds windows' changes from events alone, so an open window would keep showing "jaw can't show this yet".
- **P1, a budget.** A step converts up to 64 messages in one savepoint that can't end early.
  - 64 business templates in a row took 8.1 to 9.1 ms p50 a step. Main's links backfill takes 3.8 ms over the same 64 messages.
  - Live writes beside it took 4.6 to 6.5 ms p50 and 10.9 to 31.9 ms p99.
  - A step of 16 brings both to the links backfill's level.
  - The owner's jaw.db has no run that dense.
- **What holds:**
  - Nothing is acknowledged on these commits, and each step's changes commit with its `after`, so a kill loses whole steps only.
  - Replays of an encrypted edit wait once, live and history copies alike, and two from the same second both wait.
  - A second pass changes nothing.
  - Every raw protobuf is kept byte for byte, the moved ones in `pending`, which is class raw.
  - Unread, mentions, message counts, links and search follow from triggers and `settle`.
  - Migration 15 is a schema step with its own number, its lock line and a fault at every statement passing.
  - Every new statement searches by key or index, and all but the status count are pinned; status counts aren't, on main either.
- **On the owner's jaw.db**, a copy opened with no engine: all 419 rows that said "jaw can't show this yet" convert. 393 are shown, 26 wait in `pending` (2 of them yours, which leave tombstones), and none are left. The 7 with an edit are stand-ins with no later edit, so D1 doesn't touch them.

## What I checked

- **The diff and its base.** I read the whole range, and the m2-content code it builds on:
  - the step runner, which reads 64 messages by rowid from `after` and moves `after` in the same step;
  - `sync/backfill.rs`, which sends 64 steps of each backfill left and backs off when one is quarantined;
  - `make_groups`, a savepoint per step;
  - `amend_message`, `apply_pending`, `upsert_message`'s merge rules, `fold`, merges and `settle`;
  - the triggers on `messages`;
  - the feed's reading of events.
- **The owner's data.** A copy of the owner's jaw.db was taken with `sqlite3 -readonly ... ".backup"`, at schema version 10 with 86,754 messages.
  - I decoded every row the backfill takes with the branch's own converter (`audit_k0`).
  - I simulated the runner's steps over it for density, and timed each real step.
  - I ran the builder's measurement on it twice.
- **Scenes of my own**, in a test module of my clone, described where they're used: k1 and k1b (D1), k14 to k16 (P1).
- **Switches.** Each rule `kinds.rs` adds was switched off in turn against the kinds tests; see Measurements.
- **SQLite's own code** (the bundled 3.53.2) for what `NEXT_UNSHOWN`'s `raw IS NOT NULL` reads (H1).
- **The gate at `d4fb149`**: fmt, clippy, jawd's whole suite twice, its integration tests, and jaw's layout tests.

## Findings

### Data loss

- **D1. A later edit's text is written over when the row's raw protobuf is the original message.**
  - **Where:** `store/write/kinds.rs:29-34` (`SHOWN`), as `show()` binds it (`:75-92`): `text = iif(edited_at > ?12 AND summary IS NOT ?13, text, ?3)`, and the same for `spans` and `summary`. `?12` is the conversion's `edited_at`, which only an edit has (`convert/mod.rs:671-686`).
  - **What happens:** for a row whose raw is a message rather than an edit, `edited_at > NULL` is NULL, so `iif` takes the converted text, whatever edit came later.
    - Edits keep no raw once applied (`schema.rs:462-464`), so the edit's words exist nowhere else. The row keeps `edited_at`, and says it was edited, with the words from before the edit; the phone shows the edit.
    - That breaks "an edit wins by its time", and loses what WhatsApp sent.
  - **Two scenes, each failing at `d4fb149`:**
    - **k1.** Ana's business template in a group is a row of kind `unknown`, as an earlier jawd left it. Her edit, a plain `conversation` "It ships on Tuesday now", goes through this jawd's own converter and writer as a live write, and the row reads it (kind still `unknown`, `edited_at` set). After one backfill step the row is `text` "Your order ships on Monday", `edited_at` unchanged.
    - **k1b.** The same row reached the other way: her edit came first and stood in for the message, then history brought the template, which an earlier jawd made kind `unknown`. `upsert_message` keeps the edit's text, since an edit wins by its time (`store/write.rs:1754-1757`), and takes the template's raw. The backfill gives it the template's text.
  - **Why the tests pass:** the builder's two edited rows (`ai5`, `ai6`, `store/write/kinds/tests.rs:200-224`) are stand-ins, whose raw is the edit itself, so `?12` is the edit's own time.
  - **On the owner's jaw.db:** none of its 142 unknown rows has a summary other than "jaw can't show this yet", and its 7 with an edit are stand-ins whose `edited_at` equals their raw's (`audit_k0`). But the shape is any kind a jawd couldn't show, later edited into words it could. And `SHOWN` is the statement every later kinds backfill will reuse.
  - **The fix:** `edited_at > coalesce(?12, -1)` in the three `iif`s, as `fold` compares edits (`store/write.rs:1585`). With it, k1, k1b and the builder's three kinds tests pass. Pin it with k1 and k1b, or equivalents.

### Inconsistency

None beyond D1 and T1.
- What the backfill moves to `pending` waits once however often it comes. Live and history copies share their `at`, since both take the message's own second (`convert/mod.rs:91`, `:135`), and their own ID tells two from the same second apart.
- A replay after the row went makes no row, since the conversion has none, and yours meet their tombstone.
- What a step changes and its `after` commit together (`store/write/links.rs:75-82`), so a step taken twice does nothing twice. The builder's second pass shows it.

### Tests (missing)

These block the approval: each is a rule the change touches that no test fails without.

- **T1. The events of a converted row.**
  - **Where:** `show()` writes the message's event (`store/write/kinds.rs:96`) and, for the chat's newest, the chat's (`:98-100`).
  - **What happens:** with either left out, all of the kinds module's tests pass. The test checks that `v` rose (`store/write/kinds/tests.rs:542`, `:594`), and `v` rises either way.
    - The feed builds windows' changes from `events` alone: a `message` event names its chat's windows (`feed/mod.rs:191-203`).
    - Without one, a window open on a community keeps showing "jaw can't show this yet" for a line just converted, until it's watched again.
  - **The fix:** for each row converted, assert an `events` row of type `message`, key `n` and its chat at the row's new `v`, and the chat's event at its `v` when it was the chat's newest. For each row removed, assert its `message` event.
- **T2. D1's rule**, as above.

### Unpredictable performance

This breaks a budget, so it can't stand as a follow-up.

- **P1. A step converts up to 64 messages in one savepoint that can't end early.**
  - **Where:** the runner's step is 64 messages looked at (`store/write/links.rs:19`), and each one wanted is taken in the same group (`:70-74`). A transaction ends only between groups (`store/mod.rs:860-903`).
    - For links and polls most messages need nothing.
    - Here each one wanted is a whole conversion: `convert::message`, the `UPDATE` with its FTS and link triggers, and the event, or the move to `pending` with its delete.
  - **Measured** (k14, k15; release; method below):

    | 64 messages in a row of | A step, p50 / p99 | Load |
    | --- | --- | --- |
    | templates, words and two links | 8.1 to 9.1 / 14.8 to 17.2 ms | 25 to 66 |
    | encrypted edits | 4.4 to 4.6 / 9.4 to 10.5 ms | 62 to 66 |
    | community lines | 2.0 / 3.8 to 5.6 ms | 62 to 66 |
    | main's links backfill, the same messages with their two links | 3.8 to 3.9 / 9.9 to 10.8 ms | 25 to 27 |

    | Live writes, one every 10 ms | p50 | p99 | Most | Load |
    | --- | --- | --- | --- | --- |
    | alone | 0.48 to 0.57 ms | 1.7 to 8.6 ms | 5.0 to 24.1 ms | 39 to 53 |
    | beside the kinds backfill, 12,800 templates in a row | 4.6 to 6.5 ms | 10.9 to 31.9 ms | 21.5 to 61.6 ms | 45 to 53 |
    | beside main's links backfill, the same messages | 1.8 to 2.8 ms | 8.6 to 9.4 ms | 9.9 to 11.2 ms | 41 to 50 |
    | beside the kinds backfill with a step of 16 | 2.4 to 2.5 ms | 8.8 to 11.3 ms | 12.4 to 16.4 ms | 39 to 47 |

  - **Against the budgets:** a background transaction is at most 3 ms with its commit, and a live message's write 5 ms p99. A dense kinds step is two to three times the first, and twice what main's links backfill costs on the same messages; the writes beside it pass the second further than beside links.
    - With a step of 16, a dense step took 2.8 to 3.3 ms p50 and the writer's runs 3.2 to 4.2 ms p99, at a load of 39 to 47. The slowest round trips of those two runs were 38 and 123 ms.
  - **How likely:** WhatsApp accounts commonly hold a business's chat of templates, and history converts a conversation at a time, so its rows are consecutive by `n`.
    - The owner's jaw.db has no such run. Its densest steps hold 42, 32 and 24 community lines and 21 unknown rows (1,356 steps, 93 converting any). Timed one step to a transaction on a copy, the slowest took 9.2 ms, at a load of about 21, and converted nothing (H1).
    - The builder's million made-up messages put one of each kind in a thousand, so no step converts more than two, which hides it.
  - **The fix:** a step of its own for this backfill (16 measured as above), or let a step end after a few conversions or about a millisecond of them, with `after` set to the last message taken; then measure the dense case again. Writing the converted text through `fts_backlog`, as new messages do, rather than the update trigger would also take the FTS and link documents out of the step.

### Hygiene (follow-ups; none blocks)

- **H1. Each step reads every message's raw protobuf whole.**
  - **Where:** `NEXT_UNSHOWN` (`store/write/kinds.rs:22-24`).
  - **What happens:** in a result column, the bundled SQLite 3.53.2 codes `raw IS NOT NULL` as a plain `OP_Column` of the whole value (`sqlite3.c:118010`). The `typeof` shortcut that skips the content is only for jump contexts (`:119122`), and `AND` evaluates both sides unless one has a subquery (`exprCodeTargetAndOr`, `:117566`). So every message's raw is read, overflow pages and all.
    - On the owner's jaw.db that's 31.6 MB over the whole backfill: 86,746 raws, 382 bytes on average, 629 over 4 KB (mostly link previews with their thumbnails).
    - It's a likely cause of its slowest steps being ones that converted nothing.
  - **The fix:** `iif(kind = 'unknown' OR kind = 'system' AND summary = '…', NOT deleted AND raw IS NOT NULL, 0)`, whose condition is tested first, or `typeof(raw) <> 'null'`.
- **H2. The converter now runs on the writer's thread.**
  - **Where:** `store/write/kinds.rs:49`. Drain converts in `spawn_blocking` and goes on if a conversion panics (`sync/drain.rs:348`, `:449`). The writer (`store/mod.rs:600`) catches nothing.
  - **What happens:** a panic in `convert::message` on some old row's raw would end the writer, and with it every commit, the inbox's and so every ack. The backfill would take the same step again at the next start.
    - I found no new code that panics in a release build. `expires * 1000` (`convert/kinds.rs:537`) wraps there, as the group invite's always has, though it panics in a debug build.
    - But this backfill hands the converter every raw an earlier jawd couldn't read.
  - **The fix:** convert under `catch_unwind`, and treat a panic as that row's failure (H3).
- **H3. A step that fails every time stops this backfill and slows the others.** This is m2-content's H7, carried over.
  - **What happens:** one bad row rolls back and quarantines its whole step (`store/mod.rs:871-893`). `backfill::run` then backs off from 1 s to 5 minutes for all three backfills (`sync/backfill.rs:34-52`). This backfill does far more per message than links or polls.
  - **The fix:** a savepoint per message: quarantine the message (source `kinds`, its chat and ID, its raw) and go on.
- **H4. A row that becomes a poll gets no options.**
  - **Where:** `show()` writes no `poll_options`, which `upsert_message` does for a new poll (`store/write.rs:1845-1847`). The polls backfill may be done by then, or past the row.
  - **How likely:** none of this branch's kinds is a poll, unless a comment or a group status wraps one (both are unwrapped, `convert/mod.rs:276-288`).
  - **The fix:** `polls::options(db, n, &message.poll)` when the conversion has options, so the next kinds backfill can't miss it.
- **H5. `jawd status` counts only some of `pending`.**
  - **Where:** `encrypted_kept` counts kinds `secret` and `rsvp` (`state.rs:235`; its plan is a covering search of `pending_by_kind`).
  - **What's not counted:** encrypted reactions, pins and undecryptable votes kept for a later jawd, and receipts waiting for messages jaw doesn't have. The owner's `pending` holds 1,911 of those receipts, 40 stars, a pin and a revoke.
  - **The fix:** count `pending` by kind in `stored`.
- **H6. `crates/jawd/src/convert/` isn't in the audited paths** (`scripts/check-data-audit.py:15-25`).
  - This branch moves two kinds from message rows to `pending` in the converter alone (`convert/mod.rs:393-399`), and whatever a converter returns is what jaw.db holds.
  - **The fix:** add the path, as the rule asks when the data layer grows.

### Outside this range

- **An edit jaw can't read keeps no raw.** main's edit path applies it with no text and the "can't show" summary, and keeps nothing of it once applied (`apply_amendment`, `store/write.rs:2022-2027`). So the case `SHOWN` leaves on purpose, "unless jaw couldn't show that either", can never be recovered: the edit is lost before jaw understands it.
  - Keeping an unreadable edit's raw (in `pending` as a kept `edit`, or beside its row) would let the next kinds backfill apply it.
  - The owner's jaw.db has no row in that state today: no stand-in has a later edit, and no row of a known kind reads "jaw can't show this yet".

### The builder's decisions

- **Live group events write no line.** Agreed. A line needs a key that history's copy shares, and `events.GroupInfo` has none, so both would be rows. History's lines keep their stub's raw, so a later jawd can word them better.
- **Lines name people by number or a LID's digits.** Not a data-layer question. It matches the classic lines, and since the raw stays, names can come later without asking the phone.
- **Kept `secret` and `rsvp` have no bound.** Agreed, as data rather than a queue:
  - They don't wait for a message that may never come, as votes do; they wait for a jawd that reads them.
  - For the rows the backfill removes, `pending` now holds the only copy. It's class raw (`store/schema.rs:20-21`), never rebuilt, so a bound would have to drop data.
  - They're counted (H5 asks for the rest).
  - Nothing reads them by a scan: `kept_already` and `apply_pending` search `pending_by_target_at`, and the count `pending_by_kind`.
- **FTS through `messages_fts_update` rather than `fts_backlog`.** Correct, as edits do it:
  - A row converted had no text, so it was never in the backlog.
  - The trigger deletes a document only where there was text, and the step's commit carries the new segment, which the writer merges later (`touches_fts` counts `Backfill`).
  - It is part of P1's cost, though (see P1's fix).

## Measurements

Method:
- **One folder, `/tmp/jaw-audits/unknown-kinds`**, deleted once this note was written:
  - one clone of the worktree, checked out at `d4fb149`, built with `CARGO_INCREMENTAL=0` in its own `target` (2.1 GB);
  - debug for tests, scenes and switches, release for measurements.
  - The scenes and switches lived in an untracked test module and edits of that clone. The gate ran with them stashed.
- **The owner's jaw.db** was copied into the folder with `sqlite3 -readonly "file:…/jaw.db?mode=ro" ".backup …"`, and each run took a fresh copy of that copy.
  - Copies were opened only by tests, through `Store::open`, with no engine.
  - No `session.db` or other engine file was copied or opened, no engine was run, the owner's jawd (PID 8376) wasn't touched, and no process was stopped.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other agents. One-minute load averages ran from 21 to 82, given with each figure.
- **Scenes:**
  - **k0** decodes, on the copy, every row the backfill takes, with the branch's converter and `me` from `sync_state`, and says what each becomes.
  - **k1 and k1b** are D1's.
  - **k14** builds a version-14 jaw.db of 20,000 text messages in 199 chats, indexed, with 6,400 in a row of one kind: templates, words and two links; encrypted edits; or community lines. It times each converting step alone, one step to a transaction, or main's links backfill on the same messages with their two links in spans.
  - **k15** builds 40,000 messages with 12,800 templates in a row. It sends a live text message every 10 ms to a chat of its own, for 3 s alone and then while the backfill runs as `backfill::run` sends it (64 steps at a time), or main's links backfill over the same messages.
  - **k16** times each step of the backfill on a copy of the owner's jaw.db, one step to a transaction.

Results:
- **The owner's jaw.db** (version 10, 86,754 messages, 142 of kind unknown and 277 lines that couldn't be worded):
  - **What each becomes (k0):**
    - 277 lines become lines, 74 rows text, 7 stand-ins for Meta AI's answers text, 26 rows lines, 8 invites and 1 an image.
    - 26 go to `pending`: 23 encrypted edits from others, 2 of yours with their own deletion, and 1 RSVP.
    - None are left. The 7 stand-ins' `edited_at` equals their raw's.
  - **The builder's measurement**, release, twice at a load of 72:
    - migrated from version 10 in 1.77 and 1.61 s (migration 11's data step and its backup included);
    - backfilled in 0.29 and 0.56 s, in 64 and 86 transactions of 0.98 and 1.14 ms p50 and 10.5 and 19.1 ms p99 (of so few, about the largest);
    - none left, and 26 more in `pending`.
    - The builder's own runs were 2.1 to 2.5 s in 41 to 53 transactions, on 418 rows; the owner's jawd has kept one more since.
  - **Steps (k16)**, release, twice at a load of about 21: 1,356 steps, 93 converting any. The slowest step of each run took 9.21 and 7.45 ms and converted nothing; the next took 5.0 to 5.8 ms, converting nothing or one message.
  - **Density**, simulating the runner's steps on the copy: the densest steps hold 42 and 32 lines in one chat, 24 lines in four, and 21 unknown rows in one.
- **Dense steps (k14)** and **live writes (k15):** under P1.
- **Switches** (debug, at a load of 49), each against the kinds module's tests:

  | Switched off | Tests failing |
  | --- | --- |
  | nothing | none |
  | `SHOWN`'s edit guard, the conversion's text always taken | 1 |
  | the converted message's event | **none** (T1) |
  | the chat's event | **none** (T1) |
  | the chat's new version | 1 |
  | unread and mentions counted again | 1 |
  | the row's raw for what waits (the conversion's, encoded again, instead) | 1 |
  | your own deletion | 1 |
  | the row's removal | 1 |
  | kept amendments never deduped | 2 |
  | deduped without their own ID | 2 |

- **D1's fix**, `coalesce(?12, -1)` in the clone: k1, k1b and the three kinds tests pass.
- **The gate at `d4fb149`:**
  - `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
  - The touched modules' tests (kinds, the converter, the schema, plans, links, polls): 95 passed, at a load of 58.
  - jawd's whole suite, debug:
    - First run: 484 passed and 1 failed, at a load of 33 to 45. The failure was main's `an_idle_store_empties_its_wal_and_then_its_checkpointer_sleeps_until_the_next_commit`, "the checkpointer woke 2 times with nothing to do", in code this branch doesn't touch. Alone it passed 3 runs of 3.
    - Second run: 485 of 485, at a load of 30 to 59.
  - `starting` 1, `stopping` 2 and `upgrading` 1 passed, and so did jaw's 13 layout tests.

## Verdict

Changes needed. Nothing acknowledged depends on these commits, the moves to `pending` keep every raw byte for byte, replays change nothing twice, and the owner's jaw.db converts in full and exactly. Before this lands:
1. **D1:** `SHOWN` compares a row's `edited_at` with `coalesce(?12, -1)`, so a later edit's text stays on a row whose raw is the message, tested with k1 and k1b or equivalents.
2. **T1:** the tests check the `events` rows of each converted row and of its chat, and of each row removed.
3. **P1:** a step converts few enough messages to keep a dense run within the background budget (a step of 16, or one ending after a few conversions), measured again on a run of 64 templates and with live writes beside it.

Then a second audit, of those changes only. H1 to H6 may stand as follow-ups; H2 and H3 are the ones worth doing with P1. To land, once approved: give `4504700` (the commit touching `crates/jawd/src/store/`, `sync/` and the converter) the trailer `Data-Audit: docs/audits/2026-10-27-unknown-kinds.md`, commit this note in the branch, and run `make check-audit`.

Verdict: changes needed

## Second look

Range: `d4fb149..7ee5e86` in `unknown-kinds` (worktree `~/src/jaw-unknown`), 27 October 2026. Auditor: database-expert. One commit, `7ee5e86`, the builder's answer to the first look, with nothing uncommitted in the worktree but this note. Line numbers are at `7ee5e86`, and paths are under `crates/jawd/src/` unless they say otherwise.

### In short

Changes needed: three tests, and no code. Nothing in the commit loses data or leaves it inconsistent, and D1, T1 and P1 are fixed and tested. But three rules the branch touches have no test that fails without them.

- **D1 is fixed.** `SHOWN` compares a row's `edited_at` with `coalesce(?12, -1)` (`store/write/kinds.rs:38-43`).
  - Rows 14 and 15 of the builder's test are the first look's k1b and k1, and fail without it.
  - My s1 drives k1b through the writer (the edit first, standing in, then an earlier jawd's unknown template) and fails without it too.
- **T1 is fixed.** The test checks each converted row's event at its new version and with its chat, each removed row's, and the chat's whose newest it was (`store/write/kinds/tests.rs:779-784`, `:837-839`). Each fails with its record taken out.
- **P1 is fixed.** A step converts for a millisecond past its first conversion (`store/write/links.rs:61-95`, `store/write/kinds.rs:21-24`). At a load of 24 to 33:
  - a dense step takes 1.83 to 2.09 ms p50 with its commit, of which its work is 1.14 to 1.18 ms p50 and 1.40 to 1.43 ms p99;
  - live writes beside the backfill take 1.42 to 1.44 ms p50 and 10.1 to 13.2 ms p99, against 2.31 to 2.89 and 14.8 to 22.9 beside main's links backfill;
  - with the time switched off, as at the first look, the same dense steps take 7.17 to 7.94 ms p50, and live writes beside them 4.36 to 4.80 ms p50.
- **H1 to H4 hold.** The savepoints and quarantine lose nothing: a failure rolls back exactly, search and links included, keeps its raw protobuf, and the step goes on.
- **Missing tests, which block:**
  - T3, the stamps' undo after a message's rollback: without it, a chat's new version is left with no event, as my s3 shows.
  - T4, a transient error still failing the step.
  - T5, the edit guard's "unless jaw couldn't show that either", from `4504700`.
- **On the owner's jaw.db**, a copy opened with no engine, all 419 rows convert as at the first look: 26 go to `pending` with 2 tombstones, none are left and none are quarantined, in 0.35 s with a step to a transaction.
- **Follow-ups:**
  - the commit message's dense-run figures aren't the steps' (H7);
  - a converted poll doesn't count its own votes (H8);
  - a `kinds` quarantine entry outlives its message's conversion (H9).
- **Outside this range:** votes for a message jaw couldn't show are dropped, and a message's links cost the writer about quadratically in their number (500 links: 0.65 to 0.69 s).

### What I checked

- **The diff and the code around it:**
  - the runner's time and `after` (`store/write/links.rs:61-95`), and `CONVERTING` (`store/write/kinds.rs:21-24`);
  - `backfilled`'s savepoint, catch and quarantine (`store/write/kinds.rs:53-105`);
  - `Stamps::mark`, `undo`, `seq` and `record` (`store/write.rs:447-482`);
  - `make_groups`, the writer's transactions, `Pace` and `retrying` (`store/mod.rs:860-963`, `:1125-1142`, `:1599-1640`);
  - `settle_each` (`store/write.rs:807-875`), the vote path, `polls::options`, the triggers on `messages`;
  - FTS5's savepoint methods in the bundled SQLite 3.53.2 (`sqlite3.c:263304-263349`), and the statement savepoints that call them (`:100615-100629`).
- **Switches:** each rule the commit adds, and the guard it changes, switched off in turn, against the kinds tests and my scenes.
- **Scenes of my own**, in a test module of my clone:
  - s1, D1's k1b through the writer;
  - s2, a rollback leaving search and links exact;
  - s3, T3's;
  - s4, H8's;
  - s5, FTS5's segments a savepoint per conversion leaves.
- **Measurements:**
  - k14, dense steps; k15, live writes beside the backfill;
  - k16, each step on a copy of the owner's jaw.db;
  - k17 and k18, very large rows;
  - the builder's own measurement;
  - the time limit's effect, from a second build with it switched off, run interleaved with the head's.
- **The gate at `7ee5e86`**, with my module and instrumentation stashed.

### Findings

#### Data loss

None.

#### Inconsistency

None in the code as it is. T3 is an inconsistency the code prevents, but no test holds it to that.

- **The savepoint per message is exact.** It nests inside the writer's group savepoint, and `ROLLBACK TO` undoes the message's row, its events, what it put in `pending` and `tombstones`, and both FTS5 indexes.
  - s2 rolls one back once it has written. messages_fts's postings then match the text exactly (`index_against_text`), and links_fts's integrity-check passes.
  - No trigger or conflict clause rolls back a whole transaction: the schema has no `RAISE(ROLLBACK)` and no `ON CONFLICT ROLLBACK`. So the savepoint is always there to roll back to.
- **A transient error is retried.** It leaves the savepoint open and propagates; the group's savepoint and the transaction's rollback cancel it, and `retrying` tries the transaction again until it goes (`store/mod.rs:1125-1142`).
- **The quarantine keeps the row's raw protobuf byte for byte**, under `kinds` and `<chat> <id>`, idempotent on that key.
  - Every reader of `quarantine` filters by its own source, and `jawd status` counts these in `quarantined` (`state.rs:213`).
  - `NAMED` can't fail on a row `UNSHOWN` found: `messages` is `STRICT`, with `id TEXT NOT NULL`.
- **A panic in the converter becomes that message's failure.** `catch_unwind` holds `convert::message` alone, which owns nothing of the writer's. No profile sets `panic = "abort"` (`Cargo.toml:61-63`), and jawd sets no panic hook.
- **The runner's early stop always moves on.** `after` becomes the last message looked at before the first not taken, and a step stops only after a take. Links and polls, which have no time, step as before.

#### Tests (missing)

These block the approval: each is a rule the branch touches that no test fails without.

- **T3. The stamps' undo after a message's rollback.**
  - **Where:** `stamps.undo(mark)` (`store/write/kinds.rs:64`), after `ROLLBACK TO kinds_message` (`:63`).
  - **What happens without it:** the stamps keep the rolled-back message's seqs, among them its chat's when it was the chat's newest.
    - A later change to that chat in the same transaction takes the stale seq (`store/write.rs:466-468`) and writes no event, as the chat counts as stamped already (`:471-474`).
    - Settle's recount of the chat's unread messages is such a change (`:862-871`).
    - So the chat's version moves to a seq whose event was rolled back, and no window learns its new count.
  - **s3 shows it.** Ana's chat holds two messages jawd couldn't show. Its newest, a template, fails once it has written, and her placeholder converts to a line in the same transaction, so settle counts her unread again, 2 to 1. With the undo, the chat's version has its event. Without it, the version is 2, and there's no event.
  - **Why the tests pass without it:** the failing template in `a_message_whose_conversion_fails_or_panics_is_kept_in_quarantine_and_the_rest_are_converted` isn't its chat's newest, and its chat's unread count doesn't change.
  - **The fix:** s3 or an equivalent. Mine calls `backfilled` for both messages, then `settle`, in one transaction on an in-memory jaw.db. Through the store, the two steps can fall into two transactions under load, which hides it.
- **T4. A transient error still fails the step.**
  - **Where:** `Err(e) if transient(&e) => Err(e)` (`store/write/kinds.rs:61`).
  - **What happens without it:** a full disk or an I/O error mid-conversion is rolled back to the message's savepoint and kept in quarantine, and the backfill moves past a message with nothing wrong with it. It stays "jaw can't show this yet" for good. SQLite usually undoes only the statement on such an error and keeps the transaction, so this is reachable.
  - Nothing fails with the arm taken out.
  - **The fix:** a test hook that returns `SQLITE_FULL` once for a message, as `FAIL_SHARED` and `FAIL_BEFORE_FIRST` do (`store/mod.rs:468`, `:1295-1302`), and a test that the message converts on the writer's retry, with nothing in quarantine under `kinds`.
- **T5. The edit guard's other half: "unless jaw couldn't show that either."**
  - **Where:** `AND summary IS NOT ?13` in `SHOWN`'s three `iif`s (`store/write/kinds.rs:39-41`), from `4504700`, beside D1's fix. The first look switched the guard off whole, which one test caught; this half alone, no test catches.
  - **What happens without it:** a row whose later edit jaw couldn't read keeps no text and "jaw can't show this yet", as main's edit path leaves it (`store/write.rs:2022-2027`). It takes the converted kind, and `NEXT_UNSHOWN` never looks at it again, so what its raw protobuf says is never shown.
  - All five kinds tests pass without it.
  - **The fix:** a row in `before_kinds`, a template with a later edit jaw couldn't show, that converts to the template's words and keeps its `edited_at`.

#### Unpredictable performance

None in this range. P1 is fixed (see Measurements). What's left over the budgets is main's, under "Outside this range".

#### Hygiene (follow-ups; none blocks)

- **H7. The commit message's dense-run figures aren't the steps'.**
  - `measure_the_kinds_backfill` prints `store.report().run`. That's the writer's last 1,024 transactions, its FTS5 merge and index steps among them (`store/mod.rs:1457`, `:1475`), and in the dense run mostly the 213 steps past the templates.
  - Run on this build, it says 0.09 to 0.11 ms p50 and 2.06 to 2.07 ms p99, at a load of 56 to 61. The same steps, timed in their own transactions, took 1.83 to 2.09 ms p50 and 8.3 to 9.6 ms p99 (k14).
  - P1 holds on the real figures, but the record misstates them.
  - **The fix:** time each step from its own transaction, the converting ones apart, and correct the message.
- **H8. A converted poll doesn't count its own votes.** This is what H4 left.
  - s4: a comment that's a poll, with one vote in its raw protobuf's `poll_updates`, converts to a poll with its two options and no voters.
  - `convert::message` gives the vote as an amendment, which `show()` doesn't apply (`store/write/kinds.rs:92-101` takes the row alone).
  - Votes set aside for the poll in quarantine don't come back either, as `apply_pending` brings them back for a new poll (`store/write.rs:1889-1891`).
  - Nothing is lost: the vote stays in the row's raw protobuf. None of the owner's 419 rows is a poll.
  - **The fix:** when a row becomes a poll, apply the conversion's votes for it, and bring back what was set aside.
- **H9. A `kinds` quarantine entry outlives its message's conversion.**
  - Entries under `inbox`, `history_raw` and `merge` go when their item later succeeds (`store/write.rs:618`, `:624`, `:1481`). Nothing removes one under `kinds`, so a later kinds backfill converting the row would leave it, still counted in `quarantined`.
  - **The fix:** delete it as the row converts.
- **H5 and H6 stand** as the first look left them.

#### Outside this range

- **Votes for a message jaw couldn't show are dropped.**
  - A vote applies only to a row of kind `poll` (`store/write.rs:2087-2096`). For any other row it changes nothing and is kept nowhere: not in `pending`, and its inbox row goes with the transaction.
  - So a poll jaw showed as unknown (one in a comment, before this branch) loses its live votes for good. History's stay in its raw protobuf (H8).
  - Keeping a vote whose message isn't a poll in `pending`, as a vote that comes before its poll waits, would let the kinds backfill apply it. This is m2-content's vote path.
- **A message's links cost the writer about quadratically in their number** (k18, one message arriving new, at a load of 28 to 32):
  - 0.61 ms with 65,000 characters and no links, and 7.3 ms with 50 links in 2,000 characters;
  - 12, 36 and 122 ms with 50, 100 and 200 links in 65,000 characters;
  - 0.65 to 0.69 s with 500 links, in 16,000 characters or 65,000.
  - The inbox's commits, and so every ack, wait meanwhile.
  - The kinds backfill converts such a row at the same cost: k17's took 0.67 to 0.69 s, against 0.66 to 0.68 s arriving new.
  - It's m2-content's links views, whose places count over all of a message's link spans for each one (`store/schema.rs:784-792`). A bound on the links kept for a message, or the places counted in one pass, would fix it.
- **Background transactions of a few costly groups run past 3 ms with their commit.**
  - A transaction ends after the group in which its work passes 3 ms, and allows for its commit only from 16 groups on (`store/mod.rs:922`, `:934-946`).
  - Beside live writes, the kinds backfill's took 4.08 to 4.15 ms p50 with their commits (3.31 to 3.33 ms of work), and main's links backfill's 3.91 to 5.90 ms. This is main's pacing.

### Measurements

Method:
- **One folder, `/tmp/jaw-audits/unknown-kinds-2`**, deleted once this note was written:
  - one clone of the worktree at `7ee5e86`, built with `CARGO_INCREMENTAL=0` in its own `target` (2.1 GB at most); debug for switches and scenes, release for measurements;
  - my scenes and measurements in an untracked test module, and a test-only record of each background transaction's time, with and without its commit, in `Writer::transaction`; both were stashed for the gate;
  - a second release binary with the kinds backfill's time switched off (64 conversions a step, as at `d4fb149`), copied out before switching back, for the interleaved comparison.
- **The owner's jaw.db** was copied with `sqlite3 -readonly "file:…/jaw.db?mode=ro" ".backup …"` and read only as `immutable`.
  - Each run took a copy of that copy, opened by a test through `Store::open`, with no engine.
  - No `session.db` or other engine file was copied or opened, no engine was run, the owner's jawd (PID 8376) wasn't touched, and no process was stopped.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other agents; another agent's jawd suite ran beside some of these. One-minute load averages ran from 18 to 74, given with each figure.
- **Scenes:**
  - **s1:** Ana's edit applied live first, standing in for her template; then the template, as an earlier jawd converted it (kind unknown, saying so), through the background lane; then the kinds backfill.
  - **s2:** three templates with a link, the middle one failing once it has written (the builder's hook); then search, `links`, `index_against_text`, and links_fts's integrity-check.
  - **s3:** under T3. **s4:** under H8.
  - **s5:** ten `UPDATE`s firing an FTS5 trigger in one transaction, bare, each in a savepoint, and each in one inside an outer savepoint, counting the index's data rows.
- **Measurements:**
  - **k14:** a version-14 jaw.db (12 for main's links backfill) of 20,000 messages in 200 chats, indexed for search. The first 6,400 are a business's templates with two links, in a row in its chat, read to its end: of kind unknown for kinds, or text with their links in spans for links. Each step goes one to a transaction and is timed from it; three rounds of each, interleaved, the build without the time alternated with the head's.
  - **k15:** the same, with 40,000 messages and 12,800 templates. A live text message goes every 10 ms to a chat of its own, timed from send to answer: for 3 s alone after a warm-up, then while the backfill runs as `backfill::run` sends it (64 steps of each backfill left to a batch). Background transactions are timed as in k14.
  - **k16:** each step of the kinds backfill on a copy of the owner's jaw.db, one to a transaction, with the counts before and after.
  - **k17:** one template of 65,000 characters with 500 links, converted alone in a step, and the same message arriving new, five times.
  - **k18:** one message arriving new, by size and by links, three times each.

Results:
- **Dense steps (k14)**, release, at a load of 28 to 33, a step to a transaction:

  | 6,400 templates with two links in a row | p50 | p90 | p99 | Over 5 ms | Messages a step |
  | --- | --- | --- | --- | --- | --- |
  | kinds, with the time | 1.83 to 2.09 ms | 2.24 to 2.80 | 8.33 to 9.61 | 25 to 31 of 609 to 719 | 9.0 to 10.6 |
  | kinds, 64 a step, as at `d4fb149` | 7.17 to 7.94 ms | 10.7 to 13.0 | 14.2 to 26.1 | all 100 | 64 |
  | main's links backfill, the same messages | 3.68 to 4.22 ms | 4.19 to 5.26 | 10.4 to 14.6 | 6 to 15 of 100 | 64 |

  - **Their work alone**, three more rounds at 31: kinds 1.14 to 1.18 ms p50, 1.22 to 1.26 p90 and 1.40 to 1.43 p99, 5.4 at most (2 of 582 to 655 over 3 ms); main's links 2.87 to 2.98, 3.03 to 3.16 and 3.38 to 3.66.
  - **The tail is commits.** The slowest kinds steps took 8.4 to 11.8 ms with their commits and 1.1 to 1.3 ms of work (one 5.4), so their commits were stalling on the shared disk. Main's links steps' tail is the same: 7.1 to 11.2 ms, of which 2.8 to 3.7 ms was work.
  - **An earlier run**, at 30 to 39: kinds 1.71 to 2.00 ms p50, links 3.69 to 4.72.
  - **FTS5:** messages_fts held 268 to 317 data rows after the run, against 266 for links. In s5, ten conversions leave ten segments whether bare, with a savepoint each or nested, so the savepoints add none.
- **Live writes beside (k15)**, release, at a load of 24 to 30, one every 10 ms:

  | Live writes | p50 | p90 | p99 | Most | Over 5 ms |
  | --- | --- | --- | --- | --- | --- |
  | alone | 0.45 to 0.52 ms | 0.57 to 0.70 | 1.38 to 5.50 | 3.4 to 7.9 | 0 to 4 of 301 |
  | beside kinds, with the time | 1.42 to 1.44 ms | 2.39 to 2.90 | 10.1 to 13.2 | 10.8 to 17.9 | 3 to 6% |
  | beside main's links backfill | 2.31 to 2.89 ms | 4.48 to 5.83 | 14.8 to 22.9 | 14.8 to 22.9 | 8 to 13% |
  | beside kinds, 64 a step | 4.36 to 4.80 ms | 7.45 to 8.18 | 18.2 to 21.6 | 23.8 to 26.4 | 40 to 47% |

  - **Background transactions:** kinds 4.08 to 4.15 ms p50 with their commits, of which 3.31 to 3.33 ms was work (3.70 to 4.12 p99); links 3.91 to 5.90, of which 3.26 to 4.50; 64 a step, 6.99 to 7.45, of which 5.91 to 6.11.
  - **The backfill of 12,800** took 1.83 to 1.91 s, in 429 to 434 transactions; links' took 0.75 to 0.92 s, and 64 a step 1.68 to 1.80 s.
  - **Two earlier sets**, at 41 to 52 and 66 to 74, had live writes alone at up to 14.6 ms p50 and 134 ms p99. Where alone was quiet, they ranked the three the same way.
- **The owner's jaw.db (k16)**, at a load of about 39:
  - It was at version 10, with 86,856 messages, and migrated to 15, its backup included, in 0.86 s.
  - The backfill took 1,360 steps, 0.08 ms p50, 1.77 ms p99 and 8.78 ms at most, 0.35 s in all. The five slowest (4.5 to 8.8 ms) converted nothing.
  - Its 419 unshown rows became 0: 26 fewer messages, 26 more in `pending` and 2 more tombstones. Quarantine held 18 before and after, none under `kinds`.
- **Very large rows (k17, k18):** under "Outside this range".
- **The builder's measurement** (`JAW_MEASURE_DENSE=6400 JAW_MEASURE_MESSAGES=20000`), twice at a load of 56 to 61: "0.09 to 0.11 ms p50 and 2.06 to 2.07 ms p99" over 864 to 900 transactions, backfilled in 6.2 to 6.5 s (H7).
- **Switches** (debug, at a load of 18 to 27), each against the kinds module's tests and my scenes:

  | Switched off | Tests failing |
  | --- | --- |
  | D1's `coalesce`, back to `edited_at > ?12` | 1, and s1 |
  | the guard's `summary IS NOT ?13` | **none** (T5) |
  | the converted row's event | 1 |
  | its chat's event | 1 |
  | the removed row's event, in `remove_row` | 1 |
  | the step's time | 1 |
  | `after` past the first message not taken | 3 |
  | a step that stops early on the last page ending the backfill | 3 |
  | `catch_unwind` | 1 |
  | the SQL rollback | 1, and s2 |
  | the stamps' undo | **none** (T3); s3 fails |
  | the quarantine | 1 |
  | the quarantine's raw protobuf | 1 |
  | a transient error kept in quarantine | **none** (T4) |
  | a failure failing its whole step, as before | 1 |
  | the poll's options | 1 |

- **The gate at `7ee5e86`:**
  - `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean, at a load of 38.
  - jawd's suite: 487 passed, with 10 measurements ignored; `starting` 1, `stopping` 2 and `upgrading` 1 passed, at a load of 29 to 46.

### Verdict

Changes needed: three tests, and no code. D1, T1 and P1 are fixed and each fails a test without its fix. The commit's savepoints, catch and quarantine lose nothing and keep search, links, `pending` and the events exact. On the owner's jaw.db the backfill converts all 419 rows as the first look found they should. Before this lands:
1. **T3:** a test that a message rolled back once it has written leaves its chat no stale version: s3, or one like it, which fails without `stamps.undo(mark)`.
2. **T4:** a test that a transient error in a conversion fails the transaction rather than quarantining the message, which converts on the writer's retry.
3. **T5:** a test that a row whose later edit jaw couldn't show takes what its raw protobuf converts to.

Then a third look, at those tests only. H5 to H9 and what's outside this range may stand as follow-ups; H7 is worth correcting in the commit message while the trailers go on. To land, once approved: give `4504700` and `7ee5e86`, and the commit with the tests, the trailer `Data-Audit: docs/audits/2026-10-27-unknown-kinds.md`, commit this note in the branch, and run `make check-audit`.

Verdict: changes needed

## Third look

Range: `2bee521..b726129` in `unknown-kinds` (worktree `~/src/jaw-unknown`), 27 October 2026. Auditor: database-expert. The branch was rebased after the second look. It is now `4504700`, `d4fb149`, `2bee521` (the second look's `7ee5e86`, its message reworded) and `b726129` (the three tests), on `m2-content` at `dc48139`, with nothing uncommitted in the worktree but this note. Line numbers are at `b726129`, and paths are under `crates/jawd/src/` unless they say otherwise.

### In short

Changes needed: two tests, and no code. The rebase changed no code, H7's correction is accurate, and the test hooks can't reach a release build. T4 and T5 are tested: each new test fails with its rule taken out. But T3's test fails at the head whenever it runs beside the other kinds tests, jawd's whole suite included, so it can't hold its rule in the gate. And the edit guard's copy for `spans` has no test at all, nor its copy for `summary` for an edit jaw could read.

- **The rebase changed no code.**
  - `7ee5e86` is still in the object store, reachable from the worktree's reflog, and its tree is `2bee521`'s (`75301bad`).
  - `git range-diff dc48139 7ee5e86 2bee521` shows `4504700` and `d4fb149` unchanged, and `2bee521` differing from `7ee5e86` in its message alone.
  - `b726129` changes two files: five test-only lines in `store/write/kinds.rs`, and the kinds tests.
- **H7's correction is accurate.** The message now gives the second look's own figures for the dense run, rounded:
  - steps of 1.8 to 2.1 ms p50 with their commits (measured 1.83 to 2.09), where they took 7.2 to 7.9 (7.17 to 7.94);
  - live writes beside them at 1.4 ms p50 (1.42 to 1.44), where they took 4.4 to 4.8 (4.36 to 4.80);
  - at a load of 24 to 33, as measured;
  - and it says the builder's own 0.10 ms p50 and 2.2 ms p99 were the writer's last 1,024 transactions, FTS5's merges among them, not the steps.
- **The hooks can't reach a release build.** Every use of them is under `#[cfg(test)]`, and a release build of `jawd` at `b726129` holds none of their IDs or messages (Measurements).
- **T4 is tested.** With `Err(e) if transient(&e) => Err(e)` taken out, `a_disk_filling_as_a_message_converts_fails_its_step_which_is_tried_again_and_converts_it` fails 3 of 3: the message filling the disk is kept in quarantine and stays unconverted.
- **T5 is tested for text and summary.** With ` AND summary IS NOT ?13` taken out of the three `iif`s, the kinds test fails 3 of 3 at row 17, which keeps no text and "jaw can't show this yet".
- **T3's test fails at the head beside the other kinds tests (T3, blocking).** It needs its four steps in one background transaction, which the writer ends after 3 ms of work. Beside the other kinds tests, the step taking just the failing template took 6.3 to 27.4 ms. The test failed every time I ran it that way, and in both whole-suite runs. Alone it passes, and with the undo taken out it fails at its real check, so the check is right but its setting isn't.
- **The edit guard's copy for `spans` has no test (T6, blocking).** With the guard taken out of `spans` alone, or out of `summary` alone, nothing fails. None of the rows the guard decides has spans, and none whose edit stays has a summary.
- **The gate at `b726129`:** fmt and clippy clean. jawd's suite ran twice: T3's test failed in both, and two of the base's checkpointer tests in the first only.
- **Follow-ups carried:** H5, H6, H8 and H9 as the second look left them, and H7's measurement, which still prints the writer's last 1,024 transactions. Also what each look put outside its range.

### What I checked

- **The rebase:** each commit's parent and tree, the range-diff from `dc48139`, and a word diff of `7ee5e86`'s and `2bee521`'s messages, checked against the second look's Measurements.
- **The diff `2bee521..b726129`**, and the code its tests drive:
  - `backfilled`'s savepoint, transient arm, rollback and undo, and the hooks in `converted_again` (`store/write/kinds.rs:53-110`);
  - the runner's millisecond (`store/write/links.rs:72-95`);
  - `make_groups`, `Pace::ends`, `retrying`, `Writer::transaction` and `background_changes` (`store/mod.rs:860-903`, `:944-946`, `:1125-1142`, `:1599-1676`);
  - `Stamps::mark`, `undo`, `seq` and `record`, `transient`, `settle_each`, and main's edit (`store/write.rs:447-509`, `:807-875`, `:2022-2026`).
- **Switches:** each of the three rules taken out as the builder did it, and the edit guard taken out one column at a time, each in its own build, against the kinds tests.
- **Where T3's time goes:** a build printing each step's time and where a background transaction ends early, and one timing the phases of the failing conversion.
- **A scene of my own, u1:** T3's test with the store's background transactions given a minute.
- **A release build**, searched for the hooks.
- **The gate at `b726129`.**
- No copy of the owner's jaw.db this time: `b726129` changes nothing a release build runs.

### Findings

#### Data loss

None. `b726129` changes nothing a release build runs.

#### Inconsistency

None in the code. T6 is an inconsistency the code prevents, but no test holds it to that.

#### Tests (missing)

These block the approval.

- **T3. Its test fails at the head beside the other kinds tests, so it doesn't hold the stamps' undo in the gate.**
  - **Where:** `a_message_rolled_back_leaves_its_chat_no_version_without_an_event` (`store/write/kinds/tests.rs:173-201`), at its check that the four steps took one transaction (`:189`).
  - **What happens:** the stale stamp lives only within one transaction (`Stamps::begin`), so the scene needs the failing template and the placeholder converted in the same one, which settle then counts at its end. Through `Store::open` that rests on time:
    - A step converts for 1 ms past its first take (`store/write/links.rs:79-82`). With the test run alone, taking the failing template took 1.08 to 1.26 ms, so in every run I timed the step stopped after it, and the placeholder went to the next step.
    - The writer ends a background transaction after a group whose work has passed 3 ms (`store/mod.rs:944-946`). Alone, the first step stays under that, and the next step joins it.
    - Beside the other kinds tests, in one process, that first step took 6.3 to 27.4 ms. The transaction ended after it every time, with nothing waiting, and the placeholder converted in a second transaction.
  - **At the head:** it passed 40 of 40 alone, and 5 of 5 with the kinds tests one at a time. It failed 11 of 11 beside them, in the touched modules' run, and in both whole-suite runs, each time at `:189`.
  - **Its check is right.** With `stamps.undo(mark)` taken out, alone, it failed 9 of 10 at the version's event (`:200`: no event for the chat, at version 2), and once at `:189`.
  - The second look warned of this: "through the store, the two steps can fall into two transactions under load, which hides it". Checking for one transaction turns hiding into failing, but it also fails with the rule in place.
  - **The fix:** give the scene a transaction that can't end early, and check it still fails without `stamps.undo(mark)` at the chat's event.
    - `small_chats_of_history_go_many_to_a_transaction_while_inbox_rows_come_one_at_a_time` makes one (`store/mod.rs:2347-2353`): `Store::open_with(home, Limits { batch_time, yield_after, ..LIMITS })`, both long. Or call `backfilled` for both messages and then `settle` in one transaction, as my s3 did.
    - My u1 is the builder's test opened with a minute for both. Beside the other kinds tests, it passed 5 of 5 at the head and failed 5 of 5 without the undo, at the chat's event. Its steps split as T3's did (12.1 to 24.4 ms), in one transaction every time, so it holds the rule whether or not the steps split.
- **T6. The edit guard's copy for `spans` has no test, nor its copy for `summary` for an edit jaw could read.**
  - **Where:** `SHOWN`'s three `iif`s (`store/write/kinds.rs:39-41`), from `4504700` and from D1's fix in `2bee521`. T5's switch takes the guard's second half out of all three at once, which row 17 catches. Taken out of one column at a time, against the kinds tests:

    | Switched off | Tests failing |
    | --- | --- |
    | ` AND summary IS NOT ?13` in all three, as the builder did | 1, at row 17 |
    | the same in `text` alone | 1, at row 17 |
    | the same in `summary` alone | 1, at row 17 |
    | the same in `spans` alone | **none** |
    | the whole guard in `spans`: `spans = ?4` | **none** |
    | the whole guard in `summary`: `summary = iif(?5 IS ?3, NULL, ?5)` | **none** |

  - **Why:** neither the rows whose edit stays (6, 14 and 15) nor the one whose message converts (17) has spans on either side. Nor do the rows whose edit stays have a summary on either side: an edit's summary is NULL when it's its text (`store/write.rs:2024`), as a conversion's is when its words are plain (`convert/kinds.rs:38-41`, and `SHOWN`'s `iif(?5 IS ?3, NULL, ?5)`).
  - **What a slip there would do, unseen:**
    - With `spans` taken from the conversion, a row whose later edit added a link or a mention would keep the edit's words but the message's spans. Their offsets would point into other words, its links would be derived from the wrong spans, and the edit's mentions would be gone, since an applied edit keeps no raw.
    - With the spans copy missing T5's half, row 17 would show the template's words with no spans and no link.
    - With `summary` taken from the conversion, a chat's preview would say what the message said before its edit.
  - **The fix:** give the template of rows 14, 15 and 17 formatting and a link, say "Your order ships on *Monday*: https://jaw.example/track", so that its conversion has spans and a summary of its own (a summary is the spans' words on one line, `convert/spans.rs:98-107`). Then check each row's spans, summary and links beside its text:
    - 14 and 15 keep their edit's: no spans, no summary, no link;
    - 17 takes the template's, with its link in `links`, and keeps its `edited_at`, which the second look asked T5 to check too.

#### Unpredictable performance

None in this range.

#### Hygiene (follow-ups; none blocks)

Nothing new. Carried:
- **H5:** `jawd status` counts only some of `pending` (`state.rs:235`).
- **H6:** `crates/jawd/src/convert/` still isn't among the audited paths (`scripts/check-data-audit.py:16-23`).
- **H7, its measurement:** the message is corrected, but `measure_the_kinds_backfill` still prints `store.report().run`, the writer's last 1,024 transactions (`store/write/kinds/tests.rs:700`), rather than each step from its own transaction.
- **H8:** a converted poll doesn't count its own votes.
- **H9:** a `kinds` quarantine entry outlives its message's conversion.

#### Outside this range

Carried: an edit jaw can't read keeps no raw (the first look); votes for a message jaw couldn't show are dropped, a message's links cost the writer about quadratically in their number, and background transactions of a few costly groups run past 3 ms with their commit (the second look). New:
- **Two of the base's checkpointer tests failed once.**
  - `the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end` and `the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it` failed in the first whole-suite run, at a load of up to 61, each with "the checkpointer never flushed jaw.db".
  - Each passed 3 of 3 alone, and both passed in the second run. This branch changes one `pub use` line in `store/mod.rs`.
- **A step ran 5 to 25 times slower beside other stores in its own process than beside them in another.**
  - Taking T3's failing template took 1.08 to 1.26 ms with its test run alone, and 1.15 to 1.25 ms with the rest of the kinds tests running in a second process. With them in the same process, it took 6.3 to 27.4 ms.
  - Timing its phases, reading, decoding and converting the template took at most 0.27 ms. `show()`, the update with its triggers and events, took up to 8.2 ms.
  - So something the connections of one process share is the likely cause. The bundled SQLite gives them two:
    - it's built with `SQLITE_ENABLE_MEMORY_MANAGEMENT` (libsqlite3-sys 0.38.2's `build.rs:162`), under which every connection in a process shares one page cache group and its mutex (`sqlite3.c:58370-58371`);
    - and it keeps memory statistics (`SQLITE_DEFAULT_MEMSTATUS`, `:15125`), behind another process-wide mutex, which jawd never reads.
  - I didn't measure this in jawd, where the writer shares its process with the readers and the checkpointer. A live write's p99 with readers busy beside it would say whether it matters.

### Measurements

Method:
- **One folder, `/tmp/jaw-audits/unknown-kinds-3`**, deleted once this note was written:
  - one clone of the worktree at `b726129`, built with `CARGO_INCREMENTAL=0` in its own `target` (3.2 GB at most, with debug, release and clippy's builds);
  - debug builds for the tests, switches and scenes, and a release build for the hooks;
  - `TMPDIR` set inside the folder, so the tests' homes were made there;
  - each switch, scene and piece of instrumentation an edit of that clone, built on its own, its test binary copied out, and the clone back at `b726129` before the gate.
- No jaw.db of the owner's, `session.db` or other engine file was copied or opened. No engine ran: the integration tests use stand-ins (a missing engine, a script that sleeps, and `--engine demo`). The owner's jawd (PID 8376) wasn't touched, and no process was stopped.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other agents. One-minute load averages ran from 16 to 63, given with each figure.

Results:
- **T3's test at the head** (debug):

  | Run | Passed | Load |
  | --- | --- | --- |
  | alone | 40 of 40 | 17 to 32 |
  | the kinds tests, one at a time | 5 of 5 | 18 to 19 |
  | the kinds tests, in parallel | 0 of 11 | 17 to 20 |
  | the touched modules (kinds, the converter, schema, plans, links, polls) | 0 of 1 | 21 to 30 |
  | jawd's whole suite | 0 of 2 | 16 to 61 |

  Each failure was at `:189`: two transactions where it checks for one.
- **Where its time goes** (debug, instrumented; the time from the step's first take to where it stops):
  - alone, 1.08 to 1.26 ms (5 runs, at 29);
  - alone, with the rest of the kinds tests running in a second process, 1.15 to 1.25 ms (5, at 31);
  - beside them in the same process, 6.3 to 27.4 ms (5, at 24 to 25), the transaction ending after that step every time, with nothing waiting;
  - its phases, at about 61: reading, decoding and converting the template at most 0.27 ms, `show()` up to 8.2 ms.
- **Switches** (debug), each in its own build:

  | Switched off | What fails | Load |
  | --- | --- | --- |
  | nothing (the head) | T4's test passes 3 of 3, the kinds test 3 of 3 alone | 30 to 32 |
  | `stamps.undo(mark)` (`store/write/kinds.rs:64`) | T3's test, alone: 9 of 10 at the chat's event (`:200`), 1 at `:189` | 35 to 36 |
  | `Err(e) if transient(&e) => Err(e)` (`:61`) | T4's test, 3 of 3, at "both converted": 1 of 2 | 32 |
  | ` AND summary IS NOT ?13` in the three `iif`s (`:39-41`) | the kinds test, 3 of 3, at row 17 (`:823`) | 30 to 31 |
  | the edit guard, one column at a time | under T6 | 59 to 63 |

  - Against the kinds tests in parallel, 3 runs each: the transient arm's and the guard's switches each failed their own test every time, beside T3's test failing as at the head; the undo's left T3's test failing alone, as at the head.
- **u1** (debug, beside the kinds tests): passed 5 of 5 at the head, at 24 to 25. With the undo taken out, it failed 5 of 5 at the chat's event (none, at version 2), at 30.
- **The hooks:**
  - In the source, every use of `tests::` in `store/write/kinds.rs` is under `#[cfg(test)]` (`:84-85`, `:102-108`), and the module is `#[cfg(test)] mod tests;` (`:208-209`). Nothing outside the kinds module names the hooks, and no build script sets `cfg(test)`.
  - The release build of `jawd` at `b726129` (2 min 37 s, at 21 to 33) holds none of `3EB0FILLS`, `3EB0FAILS`, `3EB0PANICS`, "the disk filled, as a test asks", "a conversion that fails once it has written" or "a conversion that panics, as a test asks". It does hold the backfill's own "kept message" and `kinds_message`, and the unit-test binary holds all of them.
- **The gate at `b726129`:**
  - `cargo fmt --all --check`: clean. `cargo clippy --workspace --all-targets -- -D warnings`: clean on all seven crates, at 19 to 22.
  - jawd's suite, debug:
    - first run: 486 passed and 3 failed, with 10 ignored, in 285 s, at 16 to 61. The failures were T3's test and the base's two checkpointer tests (above). `starting` 1, `stopping` 2 and `upgrading` 1 passed after it, at 21.
    - second run, with `--no-fail-fast`: 488 passed and 1 failed (T3's test), with 10 ignored, in 281 s, at 26 to 46; `starting` 1, `stopping` 2 and `upgrading` 1 passed.
  - The touched modules' tests: 98 passed and 1 failed (T3's test), with 5 ignored, in 108 s, at 21 to 30.

### Verdict

Changes needed: two tests, and no code. The rebase changed no code, H7's correction is accurate, the hooks are test-only, and T4's and T5's tests each fail without their rule. Before this lands:
1. **T3:** its test holds the stamps' undo at the head beside the other tests, in a transaction that can't end early (`Store::open_with` with a long `batch_time` and `yield_after`, as u1 does), or with `backfilled` and `settle` called in one transaction. It still fails without `stamps.undo(mark)`, at the chat's event.
2. **T6:** a test fails with the edit guard taken out of `spans` alone or out of `summary` alone: the template of rows 14, 15 and 17 with formatting and a link, and their spans, summaries and links checked beside their text, with row 17's `edited_at`.

Then a fourth look, at those tests only. H5 to H9 (H7's measurement) and what's outside the range in all three looks may stand as follow-ups. To land, once approved: give `4504700`, `2bee521` and the commit with the tests the trailer `Data-Audit: docs/audits/2026-10-27-unknown-kinds.md`, commit this note in the branch, and run `make check-audit`.

Verdict: changes needed

## Fourth look

Range: `b726129..781265c` in `unknown-kinds` (worktree `~/src/jaw-unknown`), 27 October 2026. Auditor: database-expert. One commit, `781265c`, the builder's answer to the third look, on the third look's `b726129`, with nothing uncommitted in the worktree but this note. Line numbers are at `781265c`, and paths are under `crates/jawd/src/` unless they say otherwise.

### In short

Changes needed: two inconsistencies, and no test missing. T3 and T6 are fixed: each test now fails with its rule taken out, and T3's passed every run I made, at loads up to 71.

The builder's argument about the guard's first half has the right conclusion but the wrong reason. Judging it, I found a state the argument misses, which none of the looks had found: a message that comes onto a row standing in for it after the upgrade, before the backfill reaches that row. The design asks that "code works with the data in either state until one is done", and two such rows come out wrong.

- **T3 is fixed.**
  - `a_message_rolled_back_leaves_its_chat_no_version_without_an_event` gives a background transaction 500 ms of work, and 250 ms while something waits (`store/write/kinds/tests.rs:190-191`). Nothing waits in it.
  - At the head it passed 25 of 25 beside the other kinds tests, at loads of 15 to 61, and in each of my three runs of jawd's whole suite.
  - Timed, its four steps took 28 to 74 ms in one transaction beside the kinds tests. Beside the whole store module, at loads up to 71, they took 57 to 181 ms.
  - If a step ever took long enough to split them, the test would fail at its check of one transaction (`:193`), rather than pass without testing the undo.
  - With `stamps.undo(mark)` taken out, it failed 7 of 7, each time at the chat's event (`:204`), not at its check of one transaction.
- **T6 is fixed.**
  - The template now reads "Your order ships on \*Monday\*: https://jaw.example/monday" (`:267`), and the edit "It ships on \_Tuesday\_ now" (`:102`). Both have spans and a summary of their own.
  - Rows 14, 15 and 17 are checked for kind, text, spans, summary and `edited_at`, against what the converter makes of the template and of the edit (`:838-850`). The links are checked message by message (`:947-960`).
  - Taken out of `spans` or out of `summary`, the whole guard fails the test at row 14. Its second half, out of any one column or all three, fails it at row 17.
  - D1's `coalesce` taken out of any one column fails at row 14, which no test caught in `spans` or `summary` before. So does `edited_at` written from the conversion.
- **The guard's first half needs no test, but not for the builder's reason.**
  - The commit says a row jaw couldn't show has a summary other than its sentence only once a later edit gave it one. My q1 reaches such a row through this jawd's writer with no later edit:
    - Ana's edit comes first and stands in for her template;
    - her template then comes live, as an earlier jawd made it, of kind unknown;
    - the row keeps the edit's words, and, the template being a live copy, the edit's raw protobuf (`store/write.rs:1754-1757`, `:1819`).
    - There the first half is false, since `edited_at` is the raw edit's own time, and the second half is true.
  - Even there, the row's text, spans and summary are what its raw protobuf converts to, as no converter jawd has had gives an edit's words other text or spans (`convert/spans.rs` is as `7fcd8eb` made it, and this branch only adds kinds). So the first half changes nothing a writer can reach, in any of the three columns.
  - Taken out of `text`, `spans`, `summary` or all three, it fails no test, my scenes included. A test for it would need a state no writer makes. The commit message's reason should be put right (H10).
- **I1, an inconsistency.** A message jaw can now show, coming onto a row that stands in for it with an edit an earlier jawd couldn't read, says "jaw can't show this yet" for good.
  - It keeps that edit's missing words and takes its own kind, `text`, which the backfill never takes.
  - In my q3 it's Meta AI's answer, coming by history or live before the backfill reaches the row; the owner's jaw.db holds 7 such rows. Had the backfill come first, the row would show the edit's words.
- **I2, an inconsistency.** Converting an edit standing in for a message that has since come, the backfill gives the message the edit's quote, forwarding, expiry, view-once and mention. An edit standing in has none of them.
  - In my q2b, Meta AI's answer comes live before the backfill reaches the row. It has no text yet, quotes your question, mentions you, and expires after a day. This jawd can't show it but reads its context.
  - After the backfill the row shows the edit's words, as it should, but has lost the quote, the mention and the expiry. The message would never be erased, and its chat's mentions fall from 1 to 0.
  - A later kinds backfill reusing `SHOWN` would do the same to rows this jawd makes (q2).
- **Neither state is on the owner's jaw.db.** On a copy:
  - no edited message has a live envelope;
  - none of the 419 rows the backfill takes reads other than the sentence;
  - only kinds `unknown` and `system` read the sentence.
  - Each needs a message to arrive in the short window before the backfill reaches the row standing in for it. The base's audit counted an inconsistency that needs one order of arrival as blocking (its I3), and so do I.
- **Nothing else changed.** `781265c` touches only the kinds tests, and its parent is the third look's `b726129`.
- **The gate at `781265c`:**
  - `cargo fmt` and clippy are clean, and the integration tests passed.
  - jawd's suite ran three times, failing 2, 1 and 0 tests, all of them main's, in code this branch doesn't change. T3's test passed in every run.
  - The base, run twice the same way, failed 2 each time, all main's checkpointer tests, two of them the same ones as at the head.
- **Follow-ups carried:** H5, H6, H7's measurement, H8 and H9, and a new H10.

### What I checked

- **The diff `b726129..781265c`**, and the code its tests drive: `SHOWN` and `show()` (`store/write/kinds.rs:38-43`, `:125-156`), the undo (`:64`), and when the writer ends a background transaction (`store/mod.rs:347-367`, `:937-946`).
- **For the guard's first half**, every writer of a message's text, spans, summary and `edited_at`:
  - `upsert_message`'s merge (`store/write.rs:1751-1757`, `:1814-1820`);
  - an edit (`:2022-2026`) and a revoke (`:2028-2036`);
  - `fold` (`:1581-1592`) and `SHOWN`;
  - how a stand-in is made (`:607-612`, `convert/mod.rs:671-691`);
  - every migration (the one that rebuilds `messages` copies those columns as they are, `store/schema.rs:495-499`, and none sets them);
  - the converter's history for the kinds an edit carries (`convert/spans.rs`, `convert/kinds.rs`, from `7fcd8eb` to `781265c`);
  - and what `context_of` reads, at `dc48139` and now (`convert/mod.rs:528-538`).
- **Switches:** one build of my clone takes `SHOWN`'s text from an environment variable and can skip the stamps' undo. Each switch ran against the kinds module and my scenes.
- **Scenes of my own**, q1, q2, q2b and q3, in the kinds tests of my clone (Measurements).
- **A copy of the owner's jaw.db**, for the states the scenes reach.
- **The gate at `781265c`**, and the base's suite at `dc48139` for the failures outside the branch.

### Findings

#### Data loss

None in this range.

#### Inconsistency

These block approval. Both are in `4504700`'s backfill, which earlier looks read. I found them while judging the builder's argument. Each needs a message to come onto a row standing in for it between the upgrade and the backfill reaching that row. The design asks that "code works with the data in either state until one is done" (Migrations, on backfills).

- **I1. A message jaw can now show, coming onto a row an earlier jawd made of an edit of it that it couldn't read, says "jaw can't show this yet" for good.**
  - **Where:**
    - `upsert_message` keeps a row's edit over a copy without one, even an edit jaw couldn't show (`store/write.rs:1754-1757`), and takes the copy's kind (`:1816`).
    - `NEXT_UNSHOWN` takes only rows of kind `unknown` and system lines saying so (`store/write/kinds.rs:29-31`).
  - **What happens (q3):** a row as an earlier jawd made of Meta AI's edit of its answer, which it couldn't read, standing in for the answer. This jawd reads the answer.
    - Backfill first: the row shows the edit's words, "Lisbon is the capital.", and keeps them when the answer then comes by history.
    - Answer first, by history: kind `text`, no text, and "jaw can't show this yet". The answer's raw protobuf has replaced the edit's (see "Outside this range"). The backfill, run to its end, leaves it so.
    - Answer first, live: the same, but with the edit's raw protobuf kept.
    - Before this branch both converted to kind unknown, so the row stayed one the backfill takes. Now the message's own kind is one it doesn't take.
  - **How likely:**
    - The owner's jaw.db holds 7 such rows, standing in for Meta AI's answers that never came.
    - It has no row of another kind that reads the sentence.
    - The answer would have to come while the backfill is still short of its row. Alone, the backfill took 0.21 to 0.35 s over that jaw.db at the earlier looks; after an upgrade from version 10 it shares the lane with the links and polls backfills.
  - **The fix:**
    - Take any row reading "jaw can't show this yet", whatever its kind, in `NEXT_UNSHOWN`. `SHOWN` then shows the edit's words where its raw protobuf is kept (live), and the answer's where it isn't, as T5's rule has it.
    - Or let `upsert_message` not keep an edit jaw couldn't show over a copy it can show, as `SHOWN`'s second half does.
    - Pin it with q3's two cases where the answer comes first.
- **I2. Converting an edit standing in for a message that has since come, the backfill replaces the message's quote, forwarding, expiry, view-once and mention with the edit's, which an edit standing in hasn't got.**
  - **Where:**
    - `SHOWN` takes `reply`, `forwarded`, `expires_at`, `view_once` and `mentions_me` from the conversion, whatever it is (`store/write/kinds.rs:42`).
    - The message an edit stands in as has none of them (`convert/mod.rs:673-690`).
    - On a row whose message came after its edit, those columns are the message's (`store/write.rs:1816-1820`). The raw protobuf the backfill converts is the edit's, kept because the message came live (`:1819`).
  - **What happens (q2b):** q3's row, then Meta AI's answer, live, with no text yet. It quotes your question, mentions you, and expires after a day.
    - This jawd can't show the answer but reads its context (`convert/kinds.rs:432-438`, `:502-510`; `convert/mod.rs:536`).
    - Before the backfill, the row is of kind unknown, with the quote, `mentions_me` 1 and `expires_at` a day on, and its chat has 1 mention.
    - After it, the row is of kind `text` with the edit's words, which is right. But it has no quote, `mentions_me` is 0, `expires_at` is NULL, and the chat has 0 mentions.
    - So the message never goes, against "Erased when asked", and the chat no longer counts the mention. Nothing is lost: the envelope keeps the message.
  - **How likely:**
    - Earlier jawds read no context for a kind they couldn't show (`context_of` at `dc48139`, `convert/mod.rs:465-482`), and a live copy carries no ephemeral fields (`convert/mod.rs:73-98`). So, as I1, it needs the message to come after the upgrade.
    - A later kinds backfill reusing `SHOWN` would meet it on rows this jawd makes. q2 shows it with a business's buttons that have nothing to say, quoting, mentioning you, and expiring, through a backfill set going again.
  - **The fix:** when what the raw protobuf converts to is an edit standing in, leave those five columns as the row has them.
    - On a row that only stands in, they're the edit's already.
    - On one its message came onto, they're the message's.
    - Pin it with q2b.

#### Tests (missing)

None beyond what I1's and I2's fixes need (q3 and q2b, or equivalents). T3 and T6 are closed.

#### Unpredictable performance

None in this range.

#### Hygiene (follow-ups; none blocks)

- **H10. The commit message's reason for leaving the guard's first half untested isn't so.**
  - It says a row jaw couldn't show has a summary other than its sentence only once a later edit gave it one. q1, a stand-in whose message came live, is a row that has one with no later edit.
  - What holds is that such a row's text, spans and summary are what its raw protobuf converts to, by every converter jawd has had.
  - That goes for `text` and `summary` too, where no test holds the first half either.
  - Correct the message when the trailers go on.
- **Carried:**
  - H5: `jawd status` counts only some of `pending` (`state.rs:235`).
  - H6: `crates/jawd/src/convert/` isn't among the audited paths (`scripts/check-data-audit.py:16-23`).
  - H7's measurement: `measure_the_kinds_backfill` prints the writer's last 1,024 transactions (`store/write/kinds/tests.rs:725`).
  - H8: a converted poll doesn't count its own votes.
  - H9: a `kinds` quarantine entry outlives its message's conversion.

#### Outside this range

- **New: an edit standing in for its message loses its raw protobuf when the message comes by history.**
  - `upsert_message` takes a history copy's raw protobuf over the row's (`store/write.rs:1819`). So an edit jaw couldn't read is gone then: q3, answer first by history.
  - It's main's merge rule, kin to the first look's "an edit jaw can't read keeps no raw".
  - Keeping such an edit's raw protobuf in `pending` as a kept edit would let a later backfill apply it.
- **New: main's tests that fail under load.** None is in code this branch changes, whose one line in `store/mod.rs` is a `pub use`.
  - `store::tests::the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it` ("the checkpointer never flushed jaw.db") failed in the head's first suite and the base's second.
  - `store::tests::the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end` ("… never flushed jaw.db while the writer appended") failed in the head's second suite and both of the base's.
  - `store::tests::an_idle_store_empties_its_wal_and_then_its_checkpointer_sleeps_until_the_next_commit` ("the checkpointer woke 2 times with nothing to do") failed in the base's first suite, and in one of its two runs of the store module's own tests. It's the one the first look saw fail at the head.
  - `store::tests::a_reader_that_lets_go_while_a_restart_waits_for_it_doesnt_keep_the_wal_from_starting_again` failed once in three runs of the store module beside the switchboard, at 31 to 38. At the base it failed once in two runs of the store module's own 57 tests, at 33 to 38, and at the head it failed in neither of two, at 31 to 32.
  - `notify::tests::a_burst_in_a_chat_is_one_notification_with_its_newest_message` ("2 unread" where it expected 3) failed in the head's first suite, at a load near 55. The base's two suites didn't fail it, and the notification module passed 10 of 10 at each revision, at 30 to 33. It's in `notify/`, which the branch doesn't touch.
  - Each of the five passed 3 of 3 alone at each revision, at 24 to 38.
- **Carried:**
  - an edit jaw can't read keeps no raw (the first look);
  - votes for a message jaw couldn't show are dropped, a message's links cost the writer about quadratically in their number, and background transactions of a few costly groups run past 3 ms with their commit (the second look);
  - a step ran 5 to 25 times slower beside other stores in its own process (the third look).

### Measurements

Method:
- **One folder, `/tmp/jaw-audits/unknown-kinds-4`**, deleted once this note was written:
  - one clone of the worktree, at `781265c` and `dc48139` in turn, built with `CARGO_INCREMENTAL=0` in its own `target` (2.1 GB at most, with debug and clippy's builds);
  - `TMPDIR` set inside the folder;
  - the head's and the base's unit-test binaries copied out, with the clone at the head whenever the head's ran, since its tests read `plans.txt` and `migrations.lock` from the checkout.
- **The switchboard**, a local commit in the clone:
  - `AUDIT_SHOWN` replaces `SHOWN`'s text, and `AUDIT_NO_UNDO` skips `stamps.undo(mark)`;
  - T3's test prints how long its four steps took;
  - my scenes are added to the kinds tests.
  - With nothing set, and with `SHOWN`'s own text through the variable, every kinds test and scene passed.
- **The owner's jaw.db** was copied with `sqlite3 -readonly "file:…/jaw.db?mode=ro" ".backup …"`, and the copy read only as `immutable`, by `sqlite3`. No jawd opened it.
- **Safety:** no `session.db` or other engine file was copied or opened, and no engine ran (the integration tests use stand-ins). The owner's jawd (PID 8376) wasn't touched, and no process was stopped.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other agents. One-minute load averages ran from 15 to 71, given with each figure.
- **Scenes:**
  - **q1:** Ana's edit (`tuesday`, at `AT + 50`) live through this jawd, standing in for her template `t1`. Then the template, live with an envelope, as an earlier jawd converted it (kind unknown, the sentence). Then the backfill, set going again over the row, as the builder's test does.
  - **q2:** the same, with a business's buttons that have nothing to say, which this jawd reads the context of, quoting a message, mentioning you, and expiring after a day.
  - **q2b:** a version-14 jaw.db holding one row, as an earlier jawd made of Meta AI's edit of its answer, which it couldn't read, standing in for it, as the owner's 7 do. This jawd opens it. Meta AI's answer comes live, with no text yet, quoting your question, mentioning you, and expiring after a day. Then the backfill runs as migration 15 left it.
  - **q3:** the same row. The answer, with its text, comes by history or live, before the backfill or after it.

Results:
- **T3's test** (debug):

  | Run | Passed | Load |
  | --- | --- | --- |
  | the kinds tests, at the head | 25 of 25 | 15 to 61 |
  | the same, timed by the switchboard | 10 of 10; its steps 28 to 74 ms, one transaction each | 40 to 43 |
  | the whole store module, timed | 3 of 3; its steps 57 to 181 ms, one transaction each | 31 to 71 |
  | jawd's whole suite, at the head | 3 of 3 | 20 to 59 |
  | without `stamps.undo(mark)`, beside the kinds tests | 0 of 7, each at the chat's event (`:204`) | 30 to 62 |

  A background transaction ends early only at 1,000 rows, once its work has had the batch's time (here 500 ms), at its commits' pace from 16 rows on (it takes 4), or once its work has had 250 ms here while an inbox row or a live write waits (`store/mod.rs:937-946`). None waits in the test.
- **Switches** (debug), each run twice against the kinds module's 7 tests and my 4 scenes, at loads of 26 to 62:

  | Switched off | What fails |
  | --- | --- |
  | nothing, `SHOWN` through the variable | nothing |
  | the whole guard in `spans` (`spans = ?4`) | the kinds test, at row 14 (`:849`) |
  | the whole guard in `summary` | the kinds test, at row 14 |
  | the guard's second half in `spans` | the kinds test, at row 17 |
  | its second half in `summary` | the kinds test, at row 17 |
  | its second half in `text` | the kinds test, at row 17 |
  | its second half in all three | the kinds test, at row 17 |
  | its first half in `spans` | nothing |
  | its first half in `text` | nothing |
  | its first half in `summary` | nothing |
  | its first half in all three | nothing |
  | D1's `coalesce` in `spans` | the kinds test, at row 14 |
  | D1's `coalesce` in `summary` | the kinds test, at row 14 |
  | D1's `coalesce` in `text` | the kinds test, at row 14 |
  | `edited_at = ?12` added | the kinds test, at row 14 |
  | `stamps.undo(mark)` | T3's test, at the chat's event (7 of 7) |

  q1 passed under every switch: whichever part of the guard is taken out, its row comes out the same. q2, q2b and q3 record what happens rather than check it.
- **Scenes** (debug, at loads of 21 to 25):
  - **q1:**
    - Before the backfill: kind unknown; the edit's text, spans and summary; `edited_at` `AT + 50`; an envelope; the edit's raw protobuf. The guard's first half is false, its second true.
    - After it: kind `text`, with the same text, spans, summary and `edited_at`.
  - **q2:**
    - Before: kind unknown, the edit's words, a quote, `mentions_me` 1, `expires_at` `AT + 86,400,000`, and the chat's mentions 1.
    - After: kind `text`, the edit's words, no quote, 0, NULL and 0.
  - **q2b:**
    - Before: kind unknown, no text, the sentence, a quote, `mentions_me` 1, `expires_at` `AT + 86,400,000`, the chat's mentions 1, and 1 message for the backfill.
    - After: kind `text`, "Lisbon is the capital.", no quote, 0, NULL and 0.
  - **q3:**

    | First | Kind | Text | Summary | `edited_at` | Raw protobuf |
    | --- | --- | --- | --- | --- | --- |
    | the backfill | `text` | "Lisbon is the capital." | none (its text) | `AT + 5` | the answer's |
    | the answer, by history | `text` | none | "jaw can't show this yet" | `AT + 5` | the answer's |
    | the answer, live | `text` | none | "jaw can't show this yet" | `AT + 5` | the edit's |

- **The owner's jaw.db** (a copy, version 10, 86,917 messages):

  | Rows | Count |
  | --- | --- |
  | the backfill takes (not deleted; kind unknown, or a system line saying so) | 419 |
  | of those, reading other than the sentence | 0 |
  | of those, edited | 7 |
  | of those, with a live envelope | 27 |
  | of those, edited and with a live envelope | 0 |
  | edited, of any kind, with a live envelope | 0 of 998 edited |
  | reading the sentence, by kind | `system` 277, `unknown` 142, no other |

- **The gate:**
  - `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean, at loads of 15 to 21.
  - jawd's suite, debug:

    | Run | Passed | Failed, all main's | Load |
    | --- | --- | --- | --- |
    | the head, first, through cargo with the integration tests | 487 | the notification burst; the checkpointer and its locks | 20 to 29, 55 midway |
    | the head, second | 488 | the checkpointer after an outrun pass | 27 to 31, 57 midway |
    | the head, third | 489 | none | 29 to 59 |
    | the base, first, through cargo with the integration tests | 472 | the idle store's checkpointer; the checkpointer after an outrun pass | 30 to 36, 69 midway |
    | the base, second | 472 | the checkpointer after an outrun pass; the checkpointer and its locks | 35 to 63 |

    Each run took 272 to 285 s, with 10 tests ignored at the head and 9 at the base. The integration tests (`starting` 1, `stopping` 2 and `upgrading` 1) passed at both.

  - Main's five failing tests each passed 3 of 3 alone at the head and at the base, at loads of 24 to 38. The notification module passed 10 of 10 at each, at 30 to 33. The store module's own 57 tests are under "Outside this range".

### Verdict

Changes needed: two inconsistencies, and no test missing. T3 and T6 are fixed, each test now failing with its rule taken out, and T3's holds at any load I could give it. The guard's first half needs no test. Before this lands:
1. **I1:** the backfill takes a row reading "jaw can't show this yet" whatever its kind. Or `upsert_message` doesn't keep an edit jaw couldn't show over a copy it can show. Either is tested with q3's two cases where the answer comes first.
2. **I2:** converting an edit standing in, the backfill leaves the row's `reply`, `forwarded`, `expires_at`, `view_once` and `mentions_me` as they are, tested with q2b.

Then a fifth look, at those changes only. H5 to H10, and what's outside the range in all four looks, may stand as follow-ups; H10 is worth doing while the trailers go on.

To land, once approved:
- give `4504700`, `2bee521`, `b726129`, `781265c` and the commit with the fixes the trailer `Data-Audit: docs/audits/2026-10-27-unknown-kinds.md`;
- commit this note in the branch;
- run `make check-audit`.

Verdict: changes needed

## Fifth look

Range: `4fcf5f5..d611dd6` in `unknown-kinds` (worktree `~/src/jaw-unknown`), 27 October 2026. Auditor: database-expert. The branch was rebased after the fourth look. It is now `4504700`, `d4fb149`, `2bee521`, `b726129`, `4fcf5f5` (the fourth look's `781265c`, its message reworded for H10) and `d611dd6` (I1 and I2), on `m2-content` at `dc48139`, with nothing uncommitted in the worktree but this note. Line numbers are at `d611dd6`, and paths are under `crates/jawd/src/` unless they say otherwise.

### In short

Changes needed: a data loss, and a missing test. I1 and I2 are fixed, and with either fix taken out the builder's new test fails. The rebase changed no code, and H10's correction is accurate.

But the backfill now takes any row saying "jaw can't show this yet", which brings every edited message to a rule from `4504700`. That rule tells an edit jaw couldn't read by its summary alone, and an edit jaw reads whose words sum up to the sentence has the same summary. There the backfill writes the message's words over the edit's.

- **The rebase changed no code.**
  - `781265c` is reachable from the worktree's reflog. Its tree is `4fcf5f5`'s (`4aa141bc`), on the same parent, `b726129`.
  - `git range-diff dc48139 781265c 4fcf5f5` shows the four commits below unchanged, and `4fcf5f5` differing from `781265c` in its message alone.
  - `d611dd6`'s tree is that of `65cc012` (`99a5965e`), the builder's commit on `781265c` before the rebase, so the fix is what was built and tested there.
- **H10's correction is accurate.** The message now gives the fourth look's reason: the guard's two halves disagree only on an edit standing in whose message then comes live as a kind jaw can't show, and there converting the row gives the text, spans and summary it has already. That still holds at `d611dd6` (under What I checked).
- **I1 is fixed.** `NEXT_UNSHOWN` takes a row saying the sentence whatever its kind (`store/write/kinds.rs:30-32`). With the predicate as it was, the new test fails at its check that the live answer's row shows what jaw can now (`store/write/kinds/tests.rs:287`).
- **I2 is fixed, and three of its five columns are tested.** `SHOWN` keeps the row's `reply`, `forwarded`, `expires_at`, `view_once` and `mentions_me` when the conversion is an edit standing in (`store/write/kinds.rs:45-47`, `?15` from `:98-102`).
  - With the quote, the expiry or the mention taken from the conversion, the new test fails at `:288`.
  - With forwarding or view-once taken from it, nothing of the builder's fails (T7).
- **D2, data loss.** Say Ana edits a message to "\*jaw\* can't show this yet", or to the sentence with a line break in it. Her edit's words are lost to the message's when the backfill reaches the row.
  - Four scenes fail at the head: through this jawd's writer and a later kinds backfill; on upgrade, by this backfill; a stand-in whose message came by history; and a template of kind unknown. The last fails before the widening too.
  - The owner's jaw.db has no such row.
  - The fix: tell an edit jaw couldn't read by its having no text, `(text IS NOT NULL OR summary IS NOT ?13)` in the guard, as every converter jawd has had writes the sentence with none. That alone passes every test and scene.
- **What else the wider set takes ends right.**
  - A readable message with an edit jaw couldn't read shows its own words and keeps when it was edited, as T5's rule has it. The new test's history row is that state, so it's tested.
  - A message whose own words sum up to the sentence is left as it is, version and all.
- **On the owner's jaw.db**, a copy opened with no engine: the old predicate and the new take the same 419 rows, and the backfill converts all of them as before.
  - None is left saying the sentence. 26 more wait in `pending`, with 2 more tombstones.
  - The 7 edited stand-ins keep their quote, forwarding, expiry, view-once and mention (none of them has any).
- **The gate at `d611dd6`:** fmt and clippy are clean. jawd's suite passed 489 of 490 and then 490 of 490, at loads of 29 to 61, and the base 474 of 474 twice. The failure was one of main's checkpointer tests, in code this branch doesn't change. It passed alone at both revisions, the base didn't fail it in my runs, and the fourth look saw the base fail it. Another of main's checkpointer tests failed at both revisions beside the store module's own tests.
- **Follow-ups carried:** H5, H6, H7's measurement, H8 and H9, and a new H11.

### What I checked

- **The rebase:** each commit's parent and tree, the range-diff from `dc48139`, `65cc012`'s tree against `d611dd6`'s, and a word diff of `781265c`'s and `4fcf5f5`'s messages.
- **H10's reason, at `d611dd6`:**
  - An edit's words convert through `Body::with_text` and `convert/spans.rs`, both as `7fcd8eb` made them, `spans::line` and `SUMMARY` included.
  - `extended_text` changed once since, in `aa0d11c`, in its link preview's `detail` alone. `context_of` only gained kinds after the ones earlier jawds read (`4504700`), so an edit's mentions read as they did.
  - The widening adds no row where the halves disagree. The first half fails only on a row with no edit, or where the conversion is an edit standing in at the row's own edit time. The second holds only where the row doesn't say the sentence, and every converter gives an unedited row of kind unknown the sentence. Of the rows the backfill takes, that leaves those of kind unknown reading an edit's words: the fourth look's q1.
- **The diff `4fcf5f5..d611dd6`**, and the code around it:
  - `NEXT_UNSHOWN`, `SHOWN`, `converted_again`, `unshown()` and `show()` (`store/write/kinds.rs:26-48`, `:79-121`, `:131-169`), and the runner (`store/write/links.rs:61-95`);
  - every writer of a row's text, spans and summary: `upsert_message`'s merge (`store/write.rs:1745-1829`), an edit (`:2022-2026`), `fold` (`:1581-1592`) and `SHOWN`;
  - how a stand-in is made (`convert/mod.rs:662-692`), and every body that says the sentence (`convert/kinds.rs:16`, `:160`, `:435`, `:448`; `convert/mod.rs:828`), in every converter since `7fcd8eb`;
  - how a summary is made (`convert/kinds.rs:35-48`, `convert/spans.rs:98-121`).
  - Migration 15's SQL is unchanged: the new words are in its Rust comment, outside the string, and `migrations.lock` isn't touched.
- **Scenes of my own** in the kinds tests of my clone: r1, r1u, r1b and r1k (D2), r2 and r3 (what else the widening takes), and r5 (T7).
- **Switches**, through environment variables read by one build: `NEXT_UNSHOWN`, `SHOWN` and `unshown()` each as at `4fcf5f5`, each of I2's five columns taken from the conversion, and D2's fix in parts.
- **The owner's jaw.db**, a copy: what each predicate takes, and the backfill on a copy of that copy.
- **The gate at `d611dd6`**, the base's suite at `dc48139`, and main's failing tests at both.

### Findings

#### Data loss

- **D2. An edit jaw reads whose words sum up to "jaw can't show this yet" loses them to the message's when the backfill reaches its row.**
  - **Where:** `SHOWN`'s guard, `edited_at > coalesce(?12, -1) AND summary IS NOT ?13` in `text`, `spans` and `summary` (`store/write/kinds.rs:42-44`), from `4504700`. And `NEXT_UNSHOWN` (`:30-32`), which since `d611dd6` brings every row saying the sentence to it.
  - **What happens:**
    - The guard's second half takes a row whose summary is the sentence for one whose later edit jaw couldn't read, and gives it what its raw protobuf converts to.
    - But a summary is a message's words with their formatting dropped and their whitespace run together (`convert/spans.rs:98-121`), and an edit's summary is kept whenever it isn't its text (`store/write.rs:2024`). So "\*jaw\* can't show this yet", or the sentence with a line break or two spaces in it, has the sentence for its summary and its own words for its text.
    - Where the row's raw protobuf is the message, or an earlier edit, the backfill writes their words over the edit's and keeps `edited_at`.
    - An applied edit keeps no raw protobuf, so its words are gone from jaw.db. The row says it was edited, with the words from before; the phone shows the edit.
  - **Four scenes, each failing at `d611dd6`:**
    - **r1:** Ana's "Meet at 5", then her edit to "\*jaw\* can't show this yet", both through this jawd's writer, then a kinds backfill set going over them, as a later jawd's would be. The row ends "Meet at 5", with no spans or summary and the edit's `edited_at`.
    - **r1u:** the same row as an earlier jawd left it, at version 14, then this jawd's own backfill after migration 15. It ends the same.
    - **r1b:** her edit first, standing in, then her message by history, whose raw protobuf the row takes (`store/write.rs:1819`), then a kinds backfill. It ends the same.
    - **r1k:** a template of kind unknown, as an earlier jawd left it, whose edit to those words came first and stood in. It ends as the template reads.
    - With `NEXT_UNSHOWN` as it was, r1k still fails, and r1, r1u and r1b pass. So the loss is `4504700`'s for rows of kind unknown, and `d611dd6` brings every edited message to it.
  - **How likely:**
    - The edit's words have to be exactly the sentence, lowercase and with a straight apostrophe, and formatted or spaced so that they aren't their own summary: the plain sentence makes none (`store/write.rs:1747`, `:2024`). Someone who has seen jaw's sentence is the likeliest to write it.
    - A merge makes the same row from two copies (`fold`, `store/write.rs:1585-1587`).
    - The owner's jaw.db has none. Every row there saying the sentence has no text, each of kind unknown or a system line, and no other message's text or summary holds the words.
  - **The fix:** tell the two apart by text.
    - Every converter jawd has had writes the sentence with no text: for kind unknown (`convert/kinds.rs:160`, `:435`, `:448`), a system line (`convert/mod.rs:828`), and so an edit it can't read (`convert/mod.rs:665-667`, applied at `store/write.rs:2022-2026`).
    - And every writer takes a row's text, spans and summary from one source: `upsert_message`, an edit, `fold` and `SHOWN`. So a row saying the sentence with text holds someone's words.
    - The guard, in the three `iif`s: `edited_at > coalesce(?12, -1) AND (text IS NOT NULL OR summary IS NOT ?13)`. With that alone, the kinds tests and all seven of my scenes pass.
    - `NEXT_UNSHOWN` and `unshown()` may also take the sentence only with no text, so a row of someone's words isn't taken at all. That alone isn't enough: r1k still fails, as a row of kind unknown is taken whatever it says. Written so that `text` is read only for a row saying the sentence (an `iif` or a `CASE`), it adds nothing to the walk; pin its plan.
    - Pin it with rows like r1u and r1k, each keeping its edit's text, spans and summary. Both fail at the head, and r1k fails without the guard's change.

#### Inconsistency

None beyond D2. I1 and I2 are fixed, and what else the wider set takes ends right:
- **A readable message with an edit jaw couldn't read applied** (r2): main's edit path leaves it no text and the sentence, its kind as it was (`store/write.rs:2022-2026`).
  - Before `d611dd6` it said "jaw can't show this yet" for good. Now it takes its own words and keeps `edited_at`, as T5's rule has it: the edit's words were lost when it was applied.
  - The new test's history row, `ai2`, is the same state when the backfill reaches it (kind text, no text, the sentence, `edited_at`, the message's raw protobuf), so the rule is tested there.
- **A message whose own words sum up to the sentence** (r3): its conversion says the sentence too, so `unshown()` leaves the row as it is, version and event included. With `unshown()` as it was, it's written again unchanged with a new version, which nothing tests and nothing needs to.
- **What `SHOWN` still takes from a stand-in, its kind and detail:**
  - For every pair this jawd meets, Meta AI's answers and a business's messages, both sides convert through `paragraphs` to text with no detail, so they agree.
  - Where the answer came with no text yet (the fourth look's q2b), the edit's kind is the one that shows its words.

#### Tests (missing)

This blocks the approval, with D2's test.

- **T7. I2's rule for forwarding and view-once.**
  - **Where:** `forwarded = iif(?15, forwarded, ?8)` and `view_once = iif(?15, view_once, ?10)` (`store/write/kinds.rs:45-46`).
  - **What happens without them:** a forwarded or view-once message that came live onto a stand-in loses its forwarding, or its view-once, when the backfill converts the edit. A view-once message would then show as any other.
  - Nothing fails with either taken from the conversion, as the new test's answers are neither. My r5, its live answer forwarded with a score of 3 and in a view-once wrapper, fails with either.
  - **The fix:** give the live answer a forwarding score and a view-once wrapper, as r5 does, and check `forwarded` and `view_once` beside the other three.

#### Unpredictable performance

None. The wider predicate reads the same columns by the same search, pinned in `plans.txt` (`SEARCH messages USING INTEGER PRIMARY KEY (rowid>? AND rowid<?)`), and `kind`, `text`, `summary` and `deleted` all come before `raw` in the row. On the owner's copy it takes the same 419 rows.

#### Hygiene (follow-ups; none blocks)

- **H11. Two descriptions still give the narrow set.**
  - The module's doc says a row of kind unknown, or a system line jawd had no sentence for (`store/write/kinds.rs:1-5`).
  - `unshown_count` counts only those (`store/write/kinds/tests.rs:732-742`). So the "left" that `measure_the_kinds_backfill` prints (`:844`) misses a row of another kind still saying the sentence. On the owner's copy I counted those apart: none.
  - **The fix:** both to the set `NEXT_UNSHOWN` takes, as D2's fix leaves it.
- **Carried:**
  - H5: `jawd status` counts only some of `pending` (`state.rs:235`).
  - H6: `crates/jawd/src/convert/` isn't among the audited paths (`scripts/check-data-audit.py:15-27`).
  - H7's measurement: `measure_the_kinds_backfill` prints the writer's last 1,024 transactions (`store/write/kinds/tests.rs:843`).
  - H8: a converted poll doesn't count its own votes.
  - H9: a `kinds` quarantine entry outlives its message's conversion.
  - H10 is done.

#### Outside this range

- **Main's tests that fail under load, at both revisions.** This branch changes `store/mod.rs` by one `pub use` line, and in `store/write.rs` only `amend_message` and `delete_message`.
  - `store::tests::the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it` failed in the head's first suite, at 31 to 60. It passed in the head's second suite and in both of the base's, and 3 of 3 alone at each revision. The fourth look saw the base fail it.
  - `store::tests::an_idle_store_empties_its_wal_and_then_its_checkpointer_sleeps_until_the_next_commit` ("the checkpointer woke 2 times with nothing to do") failed in 4 of 6 runs of the store module's own tests at the head and 1 of 5 at the base, at 26 to 34; interleaved, 2 of 3 against 1 of 3. It passed in all four whole suites, and 3 of 3 alone at the head.
    - Its store is new, so migration 15 leaves it no backfill, and the live text message it writes takes a path this branch doesn't change. The first and fourth looks saw it fail at the head and at the base.
- **Carried:**
  - an edit jaw can't read keeps no raw (the first look), and an edit standing in loses its raw protobuf when its message comes by history (the fourth look), the two reasons r2's row and the new test's `ai2` show the message's own words;
  - votes for a message jaw couldn't show are dropped, a message's links cost the writer about quadratically in their number, and background transactions of a few costly groups run past 3 ms with their commit (the second look);
  - a step ran 5 to 25 times slower beside other stores in its own process (the third look).

### Measurements

Method:
- **One folder, `/tmp/jaw-audits/unknown-kinds-5`**, deleted once this note was written:
  - one clone of the worktree, at `d611dd6` and `dc48139` in turn, built with `CARGO_INCREMENTAL=0` in its own `target` (1.8 GB, with debug and clippy's builds), with `TMPDIR` inside the folder;
  - the head's and the base's unit-test binaries copied out, each run with the clone at its own revision, since the tests read `plans.txt` and `migrations.lock` from the checkout;
  - the switchboard and my scenes, a local commit in the clone on `d611dd6`, built once and copied out. `AUDIT_NEXT` and `AUDIT_SHOWN` replace `NEXT_UNSHOWN`'s and `SHOWN`'s text, and `AUDIT_UNSHOWN` picks `unshown()`'s test. With the head's own statements through them, every test and scene ran as at the head.
- **The owner's jaw.db** was copied with `sqlite3 -readonly "file:…/jaw.db?mode=ro" ".backup …"`. That copy was read as `immutable` by `sqlite3`, and a copy of it was opened by the builder's `measure_the_kinds_backfill` through `Store::open`, with no engine.
- **Safety:** no `session.db` or other engine file was copied or opened, and no engine ran (the integration tests use stand-ins). The owner's jawd (PID 8376) wasn't touched, and no process was stopped.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other agents. One-minute load averages ran from 21 to 63, given with each figure.
- **Scenes** (in a group, Ana's messages, `AT` as the kinds tests have it):
  - **r1:** her "Meet at 5", then her edit (at `AT + 50`) to "\*jaw\* can't show this yet", both live through this jawd's writer; then a kinds backfill set going over every message and run to its end.
  - **r1u:** a version-14 jaw.db holding that row as an earlier jawd left it (kind text, the edit's text, spans and summary, `edited_at` `AT + 50`, the message's raw protobuf), then this jawd's own backfill.
  - **r1b:** her edit live first, standing in; then her message by history; then a kinds backfill set going.
  - **r1k:** a version-14 jaw.db holding her template, of kind unknown, with that edit's words and `edited_at` and the template's raw protobuf.
  - **r2:** her "Meet at 5", then her edit to a kind this jawd can't show (a request for a phone number), both live; then a kinds backfill set going.
  - **r3:** her message "\*jaw\* can't show this yet", never edited; then a kinds backfill set going, with its version and events counted.
  - **r5:** the new test's live case alone: an earlier jawd's stand-in for Meta AI's edit, then the answer live with its envelope, forwarded with a score of 3 and in a view-once wrapper; then the backfill.

Results:
- **Scenes at the head** (debug, at 22):

  | Scene | Before the backfill | After it |
  | --- | --- | --- |
  | r1, r1u, r1b | text; "\*jaw\* can't show this yet", its two spans, the sentence for summary; `edited_at` `AT + 50` | text; "Meet at 5", no spans or summary; `edited_at` `AT + 50` |
  | r1k | unknown; the same words, spans, summary and `edited_at` | text; "Your order ships on \*Monday\*: https://jaw.example/monday", its spans and summary; `edited_at` `AT + 50` |
  | r2 | text; no text, the sentence; `edited_at` `AT + 50` | text; "Meet at 5", no summary; `edited_at` `AT + 50` |
  | r3 | text; its words, the sentence for summary; version 3, 1 event | the same; version 3, 1 event |
  | r5 | text; no text, the sentence; the quote, forwarded 3, view-once, the mention, an expiry | text; "Lisbon is the capital."; the quote, forwarded 3, view-once, the mention and the expiry as before |

- **Switches** (debug), two runs each of the kinds module's 8 tests and my 7 scenes, the same both times, at loads of 23 to 62:

  | Switched | The builder's tests failing | My scenes failing |
  | --- | --- | --- |
  | nothing, or the head's statements through the switchboard | none | r1, r1u, r1b, r1k |
  | `NEXT_UNSHOWN` as at `4fcf5f5` | the new test, at `:287` | r1k, r2, r5 |
  | `unshown()` as at `4fcf5f5` | none | r1, r1u, r1b, r1k, r3 |
  | `reply` from the conversion | the new test, at `:288` | r1, r1u, r1b, r1k |
  | `expires_at` from the conversion | the new test, at `:288` | the same four |
  | `mentions_me` from the conversion | the new test, at `:288` | the same four |
  | `forwarded` from the conversion | **none** (T7) | the same four, and r5 |
  | `view_once` from the conversion | **none** (T7) | the same four, and r5 |
  | all five from the conversion | the new test, at `:288` | the same four, and r5 |
  | D2's guard, `(text IS NOT NULL OR summary IS NOT ?13)` | none | none |
  | `NEXT_UNSHOWN` and `unshown()` taking the sentence only with no text | none | r1k |
  | both | none | none |

- **The owner's jaw.db** (a copy, version 10, 86,934 messages):

  | Rows not deleted | Count |
  | --- | --- |
  | taken as at `4fcf5f5` (kind unknown, or a system line saying the sentence) | 419 |
  | taken at `d611dd6` (kind unknown, or saying the sentence) | 419 |
  | taken with D2's fix (kind unknown, or saying it with no text) | 419 |
  | saying the sentence, by kind | `system` 277, `unknown` 142 (7 of them edited), no other |
  | saying it with text | 0 |
  | holding "can't show this yet" anywhere in text or summary | 419, the same rows |

  - **The backfill at the head**, on a copy of that copy, by `measure_the_kinds_backfill` (debug, at 24 to 25): migrated to version 15 in 0.97 s, its backup included, and backfilled in 0.20 s, in 48 transactions of 0.61 ms p50 and 6.70 ms p99 with their commits.
  - After it, no row said the sentence. Messages went from 86,934 to 86,908, `pending` from 2,232 to 2,258, and tombstones from 159 to 161, with nothing in quarantine under `kinds`.
  - The 7 edited stand-ins became text, each keeping its `edited_at`, with no quote, forwarding, expiry, view-once or mention, as before.
- **The gate:**
  - `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean, at loads of 27 to 37.
  - The touched modules' tests (kinds, the converter, schema, plans, links, polls): 100 passed, with 5 ignored, at 21 to 23.
  - jawd's suite, debug:

    | Run | Passed | Failed | Load |
    | --- | --- | --- | --- |
    | the head, first, through cargo with the integration tests | 489 | the checkpointer and its locks (main's) | 31 to 60 |
    | the head, second | 490 | none | 29 to 61 |
    | the base, first, through cargo with the integration tests | 474 | none | 31 to 59 |
    | the base, second | 474 | none | 34 to 63 |

    Each took 268 to 275 s, with 10 tests ignored at the head and 9 at the base. The integration tests (`starting` 1, `stopping` 2 and `upgrading` 1) passed at both.
  - **Main's two failing tests**, beside the suites:

    | Run | The head | The base | Load |
    | --- | --- | --- | --- |
    | the checkpointer and its locks, alone | 3 of 3 passed | 3 of 3 passed | 25 to 26 |
    | the idle store's checkpointer, alone | 3 of 3 passed | not run | 28 |
    | the store module's 56 tests, one revision after the other | the idle store failed 2 of 3 | 2 of 2 passed | 26 to 34 |
    | the same, interleaved | the idle store failed 2 of 3 | the idle store failed 1 of 3 | 26 to 33 |

### Verdict

Changes needed: a data loss, and a missing test. The rebase changed no code, H10's correction is accurate, and I1 and I2 are fixed, each failing the builder's test with its fix taken out. Before this lands:
1. **D2:** `SHOWN`'s guard tells an edit jaw couldn't read by its having no text: `edited_at > coalesce(?12, -1) AND (text IS NOT NULL OR summary IS NOT ?13)` in the three `iif`s. `NEXT_UNSHOWN` and `unshown()` may take the sentence only with no text as well. It's tested with rows like r1u and r1k, each keeping its edit's text, spans and summary, and r1k fails without the guard's change.
2. **T7:** the new test's live answer is forwarded and view-once, and `forwarded` and `view_once` are checked beside the quote, the mention and the expiry.

Then a sixth look, at those changes only. H5 to H9, H11, and what's outside the range in all five looks may stand as follow-ups.

To land, once approved:
- give `4504700`, `2bee521`, `b726129`, `4fcf5f5`, `d611dd6` and the commit with the fixes the trailer `Data-Audit: docs/audits/2026-10-27-unknown-kinds.md`;
- commit this note in the branch;
- run `make check-audit`.

Verdict: changes needed

## Sixth look

Range: `d611dd6..79ba263` in `unknown-kinds` (worktree `~/src/jaw-unknown`), 27 October 2026. Auditor: database-expert. One commit, `79ba263`, the builder's answer to the fifth look, on the fifth look's `d611dd6`, with nothing uncommitted in the worktree but this note. Line numbers are at `79ba263`, and paths are under `crates/jawd/src/` unless they say otherwise.

### In short

Approved. D2, T7 and H11 are fixed, and every rule the commit touches fails a test with it taken out. The narrower `NEXT_UNSHOWN` and `unshown()` still take everything that should be converted, and leave alone only rows that hold someone's words. They also convert three kinds of row that `d611dd6` would have left unconverted for good.

- **Nothing else changed.** `79ba263`'s parent is `d611dd6`, and it changes four files:
  - `store/write/kinds.rs`: `NEXT_UNSHOWN`, `SHOWN`'s guard, `unshown()` and their comments;
  - `store/plans.txt`: the two statements' new text, with their plans as they were;
  - `store/schema.rs`: migration 15's comment, outside its string. All fifteen migrations' texts are the same at both revisions, and `migrations.lock` isn't touched;
  - the kinds tests.
- **D2 is fixed.** `SHOWN`'s guard is `edited_at > coalesce(?12, -1) AND (text IS NOT NULL OR summary IS NOT ?13)` in its three `iif`s (`store/write/kinds.rs:45-48`).
  - Row 18 of the builder's test is the fifth look's r1k: a template of kind unknown whose later edit says "\*jaw\* can't show this yet". It keeps the edit's text, spans and summary, as text (`store/write/kinds/tests.rs:1042-1057`).
  - With the text test taken out of all three `iif`s, or out of `text`, `spans` or `summary` alone, that test fails at row 18.
  - My w1 and w2, the fifth look's r1 and r1b through this jawd's writer, lose the edit's words to "Meet at 5" with the three statements as at `d611dd6`. At the head they're left as they were.
- **T7 is fixed.** The stand-ins' test makes Meta AI's answers forwarded, with a score of 2, and view-once (`store/write/kinds/tests.rs:187-188`, `:200-202`). It checks both beside the quote, mention, expiry and edit time (`:250-271`). With `forwarded` or `view_once` taken from the conversion, it fails at the live answer (`:297`).
- **H11 is fixed.** The module's doc, `NEXT_UNSHOWN`'s and `SHOWN`'s, migration 15's comment and `unshown_count` (`store/write/kinds/tests.rs:806-819`) all give the set `NEXT_UNSHOWN` takes. The measurement prints those counts, and says its timings are the writer's last 1,024 transactions rather than its steps, which settles H7 as well.
- **The narrower predicates take what they should, and what they take ends right.**
  - Every converter jawd has had writes "jaw can't show this yet" only with no text, and every writer takes a row's text, spans and summary from one source. So a row saying the sentence with text holds someone's words, an edit's or the message's own, and `NEXT_UNSHOWN` (`store/write/kinds.rs:33`) is right to leave it.
  - Every case the fourth and fifth looks raised that should convert still does, each held by the builder's tests or my scenes: earlier jawds' stand-ins, copies coming onto them by history or live (I1), an edit jaw couldn't read on a message it shows (r2, my w4), and templates edited either way round.
  - Rows holding someone's words are left as they were, version and events included: row 19, and my w1, w2 and w5.
  - `unshown()` (`:124`) now lets a message or an edit whose own words sum up to the sentence show as those words. At `d611dd6` each kind of row below stayed as it was, of kind unknown or saying the sentence, and every later kinds backfill would have taken it again and left it. Each fails with `unshown()` as it was:
    - a template saying those words (row 20, and my w6 with the plain sentence);
    - a template that came live onto its edit's stand-in, holding the edit's words under kind unknown (w3);
    - Meta AI's stand-in whose edit, readable now, says them, after its answer came live (w7).
- **On the owner's jaw.db**, a copy at version 10 with 86,943 messages, opened with no engine:
  - the predicates of `4fcf5f5`, `d611dd6` and `79ba263` take the same 419 rows, none of them with text;
  - the backfill converts all 419 as at the earlier looks: 393 stay, 26 go to `pending` with 2 tombstones, none is left, and nothing goes to quarantine;
  - the 7 edited stand-ins keep their edit times, and no other row changes.
- **The gate at `79ba263`:** fmt and clippy are clean, and the touched modules' 100 tests pass. jawd's suite passed 487, 489 and 489 of 490 in three runs, at loads of 30 to 62; each failure was one of main's tests, in code this branch doesn't change: the notification burst in two runs, and a checkpointer test in two. The base, interleaved, passed 474 of 474 twice. The checkpointer tests have failed at the base before; the notification burst hasn't in any base suite I or the earlier looks ran (Outside this range).
- **Follow-ups carried:** H5, H6, H8 and H9, and what's outside the range in all six looks.

### What I checked

- **The commit:** its parent, the diff `d611dd6..79ba263`, all fifteen migrations' texts at both revisions, and `migrations.lock`.
- **Where the sentence comes from:** every use of `kinds::UNKNOWN` at `79ba263` (`convert/kinds.rs:16`, `:160`, `:435`, `:448`; `convert/mod.rs:828`), and in each commit that changed its uses since `7fcd8eb`.
  - Each goes through `Body::new` (`convert/kinds.rs:29-31`) or `system()` (`convert/mod.rs:704-718`), both with no text.
  - `Body::with_text` gives a text its own line for summary (`convert/kinds.rs:35-48`). The converters that set a body's text after making it (documents, locations, contacts, events; `:254-378`) give summaries of their own.
- **Every writer of a row's text, spans and summary:** `upsert_message` (`store/write.rs:1745-1757`, `:1814-1823`), an edit (`:2022-2027`), a revoke (`:2028-2034`), `fold` (`:1581-1590`) and `SHOWN`. Each takes all three from one source, and no migration sets them.
- **The walk:** `NEXT_UNSHOWN`'s plan, and what the bundled SQLite 3.53.2 reads for it (`sqlite3.c:117566`, `:118010`, `:119122`), against the order of `messages`' columns (`store/schema.rs:465-488`).
- **Switches:** a local commit in my clone reads `NEXT_UNSHOWN`'s and `SHOWN`'s text from `AUDIT_NEXT` and `AUDIT_SHOWN`, and `AUDIT_UNSHOWN_OLD` puts `unshown()` back as at `d611dd6`. Each switch ran twice against the kinds module's 8 tests and my 7 scenes.
- **Scenes of my own,** w1 to w7, in the kinds tests of my clone (Measurements).
- **A copy of the owner's jaw.db:** what each predicate takes, and the backfill on copies of that copy.
- **The gate at `79ba263`,** and main's failing tests at `dc48139` as well.

### Findings

#### Data loss

None. D2 is fixed and tested.

#### Inconsistency

None.
- **What the narrower `NEXT_UNSHOWN` leaves holds someone's words.** A row it took at `d611dd6` and doesn't take now says the sentence with text.
  - No converter writes the sentence with text, and no writer takes a row's text from one source and its summary from another. So such a row's words are an edit's or the message's own, and there's nothing to convert.
  - The fifth look's r1, r1u, r1b and r3 are such rows. The builder's row 19 and my w1, w2 and w5 are left as they were, version and events included.
- **What it still takes converts:** kind unknown whatever its text, and any kind saying the sentence with no text.
  - That covers earlier jawds' lines and unknown kinds, their stand-ins for edits they couldn't read, copies that came onto those (I1), and an edit jaw couldn't read applied to a message it shows (w4).
  - With `4fcf5f5`'s predicate, the builder's I1 test still fails, and so do w4 and w7.
- **`unshown()`** (`store/write/kinds.rs:124`) counts a conversion as one jaw still can't show only when its kind is unknown or it says so with no text. So a conversion with words always shows, and the guard decides whose words:
  - a later edit's stay where jaw read it, as it left text (row 18, and rows 14 and 15);
  - where it couldn't, the conversion's words take the empty place and the row keeps its edit time, as T5's rule has it (row 17, and w4).
- **What `SHOWN` keeps from a copy is unchanged.** The stand-ins' test now holds all five of I2's columns.

#### Tests (missing)

None. Every rule the commit touches fails a test with it taken out (Measurements): the guard's text test in each of its three columns, `NEXT_UNSHOWN`'s text test, `unshown()`'s text test, and I2's rule for forwarding and for view-once.

#### Unpredictable performance

None.
- `NEXT_UNSHOWN`'s plan is as pinned, `SEARCH messages USING INTEGER PRIMARY KEY (rowid>? AND rowid<?)` (`store/plans.txt:593-594`), and so is `SHOWN`'s, `SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)`.
- The walk now reads every message's `text`.
  - In a result column, SQLite 3.53.2 evaluates both sides of an `AND` or `OR` unless one holds a subquery (`exprCodeTargetAndOr`, `sqlite3.c:117566`), and `IS NULL` there reads the whole value (`:118010`). Only a jump context skips the content (`:119122`).
  - But `summary` and `deleted` come after `text` and `spans` in the row (`store/schema.rs:475-482`). Reading them already passed over `text`'s bytes and overflow pages, as at `d611dd6`, so the walk reads no more pages. What it adds is a copy of each text into memory.
- On the owner's copy, the backfill took 0.26 to 0.33 s with the head's predicate and 0.27 to 0.29 s with `d611dd6`'s, interleaved, at a load of 32 to 37 (debug).

#### Hygiene (follow-ups; none blocks)

Nothing new.
- **H7 is settled.** The measurement now says its timings are the writer's last 1,024 transactions. Nothing times a step on its own, as the second look's k14 did; that's a choice, not a misstatement.
- **H10 and H11 are done.**
- **Carried:**
  - H5: `jawd status` counts only some of `pending` (`state.rs:235`).
  - H6: `crates/jawd/src/convert/` isn't among the audited paths (`scripts/check-data-audit.py:15-34`).
  - H8: a converted poll doesn't count its own votes.
  - H9: a `kinds` quarantine entry outlives its message's conversion.

#### Outside this range

- **Main's tests that fail under load.** This branch changes `store/mod.rs` by one `pub use`, and in `store/write.rs` only what `amend_message` does with kept amendments and `delete_row`'s split into `remove_row`. Neither is on these tests' paths.
  - `notify::tests::a_burst_in_a_chat_is_one_notification_with_its_newest_message` failed in two of the head's three suites, with "2 unread" and "coming" where it expects "3 unread" and "tonight?" (`notify/tests.rs:144`).
    - The base didn't fail it in either of its two suites, nor in the four the fourth and fifth looks ran. The branch's head failed it in one of the fourth look's three suites (at `781265c`), none of the fifth look's two (at `d611dd6`), and the builder's one run at `79ba263`.
    - It never failed outside a whole suite, at either revision: alone, in its module, or beside the kinds, links and polls tests in one process (Measurements).
    - It writes three live text messages through `upsert_message` into a new store, which migration 15 leaves no backfill, and expects one notification once 60 ms pass with no message (`notify/tests.rs:27-28`, `:139-144`). Nothing on that path changed, so I count it as main's: a window a loaded machine can pass between two writes. But the base hasn't failed it alike in any run I or the earlier looks made.
    - If it fails in the landing gate, run it alone; widening its window is main's to do.
  - `store::tests::the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it` failed in two of the head's three suites, and `store::tests::the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end` in one, each with "the checkpointer never flushed jaw.db" (`store/mod.rs:3240`, `:3309`).
    - The base passed both in my two suites, but the fourth look saw it fail the second in both of its suites, and the first in one.
    - Each passed 3 of 3 alone at each revision, and the store module's 56 tests passed twice at each.
- **Carried:**
  - an edit jaw can't read keeps no raw (the first look), and an edit standing in loses its raw protobuf when its message comes by history (the fourth look);
  - votes for a message jaw couldn't show are dropped, a message's links cost the writer about quadratically in their number, and background transactions of a few costly groups run past 3 ms with their commit (the second look);
  - a step ran 5 to 25 times slower beside other stores in its own process (the third look).

### Measurements

Method:
- **One folder, `/tmp/jaw-audits/unknown-kinds-6`**, deleted once this note was written:
  - one clone of the worktree, at `79ba263` and `dc48139` in turn, built with `CARGO_INCREMENTAL=0` in its own `target`, with `TMPDIR` inside the folder (2.8 GB at most, with debug and clippy's builds);
  - the head's and the base's unit-test binaries copied out, each run with the clone at its own revision, since the tests read `plans.txt` and `migrations.lock` from the checkout;
  - the switchboard and my scenes, a local commit in the clone on `79ba263`, built once and copied out. With nothing set, and with the head's own statements through it, every kinds test and scene passed.
- **The owner's jaw.db** was copied with `sqlite3 -readonly "file:…/jaw.db?mode=ro" ".backup …"`. That copy was read as `immutable` by `sqlite3`, and copies of it were opened by the builder's `measure_the_kinds_backfill` through `Store::open`, with no engine.
- **Safety:** no `session.db` or other engine file was copied or opened, and no engine ran (the integration tests use stand-ins). The owner's jawd (PID 8376) wasn't touched, and no process was stopped.
- **Machine:** an Apple M1 Ultra with 20 cores, shared with other agents. One-minute load averages ran from 20 to 62, given with each figure.
- **Scenes** (in a group, Ana's messages, `AT` as the kinds tests have it, a backfill "set going" being a `kinds` row over every message, run to its end, as a later jawd's would be):
  - **w1:** her "Meet at 5", then her edit (at `AT + 50`) to "\*jaw\* can't show this yet", both live through this jawd's writer; then a backfill set going.
  - **w2:** her edit live first, standing in; then her message by history, whose raw protobuf the row takes; then a backfill set going.
  - **w3:** her edit live first, standing in; then her template live as an earlier jawd converted it (kind unknown, the sentence), with an envelope, so the row keeps the edit's raw protobuf; then a backfill set going, twice.
  - **w4:** her "Meet at 5", then an edit of it to a request for her location, which jaw can't read, both live; then a backfill set going.
  - **w5:** her message "\*jaw\* can't show this yet", never edited; then a backfill set going.
  - **w6:** a version-14 jaw.db holding her template, of kind unknown, whose own words are "jaw can't show this yet"; the backfill after migration 15, then one set going.
  - **w7:** a version-14 jaw.db holding an earlier jawd's stand-in for Meta AI's edit of its answer, which it couldn't read, saying "\*jaw\* can't show this yet"; then the answer live, with its envelope, so the row keeps the edit's raw protobuf; then the backfill after migration 15.

Results:
- **The touched modules' tests at the head** (kinds, the converter, schema, plans, links, polls; debug): 100 passed, with 5 measurements ignored, in 106 s, at a load of 24 to 44.
- **Scenes at the head** (debug, at about 20):

  | Scene | Before the backfill | After it |
  | --- | --- | --- |
  | w1, w2 | text; "\*jaw\* can't show this yet", its two spans, the sentence for summary; `edited_at` `AT + 50` | the same, version and events too |
  | w3 | unknown; the edit's words, spans and summary; `edited_at` `AT + 50` | text; the same words; a new version and its event; the second backfill leaves it |
  | w4 | text; no text, the sentence; `edited_at` `AT + 50` | text; "Meet at 5", no summary; `edited_at` `AT + 50`; a new version and its event |
  | w5 | text; its words, the sentence for summary | the same, version and events too |
  | w6 | unknown; no text, the sentence | text; "jaw can't show this yet", no summary; the second backfill leaves it |
  | w7 | text; no text, the sentence; `edited_at` `AT + 5` | text; the edit's words, spans and the sentence for summary; `edited_at` `AT + 5` |

- **Switches** (debug), two runs each of the kinds module's 8 tests and my 7 scenes, the same both times, at loads of 34 to 57:

  | Switched | The builder's tests failing | My scenes failing |
  | --- | --- | --- |
  | nothing, or the head's statements through the switchboard | none | none |
  | the guard's text test, in all three `iif`s (as at `d611dd6`) | the kinds test, at row 18 (`:1056`) | none |
  | the same in `text` alone | the kinds test, at row 18 | none |
  | the same in `spans` alone | the kinds test, at row 18 | none |
  | the same in `summary` alone | the kinds test, at row 18 | none |
  | `NEXT_UNSHOWN`'s text test (as at `d611dd6`) | the kinds test, at row 19 (`:1089`) | w1, w2 and w5, each at its new version; w3 at its second backfill |
  | `unshown()`'s text test (as at `d611dd6`) | the kinds test, at row 20 (`:1056`) | w3, w6 and w7 |
  | both of those | the kinds test, at row 20 | w1, w2, w3, w6 and w7 |
  | all three as at `d611dd6` | the kinds test, at row 18 | w1 and w2, each losing the edit's words to "Meet at 5"; w3, w6 and w7 |
  | `NEXT_UNSHOWN` as at `4fcf5f5` (I1's) | the stand-ins' test, at the live answer (`:296`) | w4 and w7 |
  | `forwarded` from the conversion | the stand-ins' test, at the live answer (`:297`) | none |
  | `view_once` from the conversion | the stand-ins' test, at the live answer (`:297`) | none |

- **The owner's jaw.db** (a copy, version 10, 86,943 messages):

  | Rows not deleted | Count |
  | --- | --- |
  | taken by `4fcf5f5`'s predicate (kind unknown, or a system line saying the sentence) | 419 |
  | taken by `d611dd6`'s (kind unknown, or saying it) | 419 |
  | taken by `79ba263`'s (kind unknown, or saying it with no text) | 419 |
  | saying it, by kind | `system` 277, `unknown` 142 (7 of them edited), no other |
  | saying it with text, and of kind unknown with text | 0 and 0 |
  | holding "can't show this yet" anywhere in text or summary | 419, the same rows |

  - **The backfill**, on copies of that copy, by `measure_the_kinds_backfill` (debug), the head's build and the switchboard's with `d611dd6`'s predicate interleaved, at a load of 32 to 37:

    | Run | Migrated to 15 | Backfilled | Transactions | Their p50 and p99, the writer's last 1,024 |
    | --- | --- | --- | --- | --- |
    | the head, first | 1.00 s | 0.33 s | 48 | 0.39 and 4.86 ms |
    | `d611dd6`'s predicate, first | 0.99 s | 0.27 s | 50 | 0.34 and 4.90 ms |
    | the head, second | 1.01 s | 0.26 s | 56 | 0.50 and 6.67 ms |
    | `d611dd6`'s predicate, second | 1.14 s | 0.29 s | 51 | 0.52 and 4.85 ms |

  - After each, at version 15: messages went from 86,943 to 86,917, none saying the sentence and none of kind unknown; `pending` from 2,232 to 2,258; tombstones from 160 to 162; quarantine held 18, none under `kinds`; and the backfill was done.
  - The 393 rows that stayed are 303 lines, 81 text, 8 invites and an image, as the first look found. The 7 edited stand-ins became text, each keeping its `edited_at`, with no quote, forwarding, expiry, view-once or mention to keep.
  - No row outside the 419 changed its kind, text, spans, summary, `edited_at` or raw protobuf.
- **The gate:**
  - `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean, at loads of 29 to 36.
  - jawd's suite, debug:

    | Run | Passed | Failed, all main's | Load |
    | --- | --- | --- | --- |
    | the head, first, through cargo with the integration tests | 487 | the notification burst; the checkpointer and its locks; the checkpointer after an outrun pass | 30 to 62 |
    | the head, second | 489 | the checkpointer and its locks | 34 to 42 |
    | the base, first | 474 | none | 34 to 36 |
    | the head, third | 489 | the notification burst | 36 |
    | the base, second | 474 | none | 32 to 36 |

    Each took 269 to 329 s, with 10 tests ignored at the head and 9 at the base. The integration tests (`starting` 1, `stopping` 2 and `upgrading` 1) passed at the head.
  - **Main's three failing tests**, beside the suites, each with the clone at its own revision:

    | Run | The head | The base | Load |
    | --- | --- | --- | --- |
    | each alone, three times, interleaved | 3 of 3 passed, each | 3 of 3 passed, each | 28 |
    | the notification module, three times | 10 of 10, each time | 10 of 10, each time | 30 to 32 |
    | the store module's 56 tests, twice | 56 of 56, each time | 56 of 56, each time | 40 to 45 |
    | the burst in the notification module alone, ten times | none failed | none failed | 25 to 41 |
    | the same beside the kinds tests in one process, ten times | none failed | (no kinds tests) | 25 to 39 |
    | the same beside the links and polls tests in one process, six times | none failed | none failed | 27 to 44 |

### Verdict

Approved. The commit fixes D2 and T7 and corrects H11's descriptions, nothing else changed, and every rule it touches fails a test with it taken out. The narrower predicates take every row that should be converted and leave only rows that hold someone's words, and the owner's jaw.db converts in full, exactly as before. H5, H6, H8 and H9, and what's outside the range in all six looks, stand as follow-ups.

To land:
- `unknown-kinds` sits on `m2-content`, which lands first, under its own audit (`docs/audits/2026-10-26-m2-content.md` in `~/src/jaw-m2c`);
- give `4504700`, `2bee521`, `b726129`, `4fcf5f5`, `d611dd6` and `79ba263` the trailer `Data-Audit: docs/audits/2026-10-27-unknown-kinds.md`;
- commit this note in the branch;
- run `make check-audit`.

Verdict: approved
