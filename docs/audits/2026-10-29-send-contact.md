# Audit: contact cards, both ways (`send-contact`)

# Round three: `e12d8d34..f3d93673`, the integration onto main

Range: `e12d8d34..f3d93673` in `send-contact`, seventeen commits, 1 November 2026. Auditor: database-expert. Paths are under `crates/` unless they say otherwise, and line numbers are at `f3d93673`.

The worktree stayed at `f3d93673`, with nothing uncommitted but this note. Its base, `e12d8d34`, holds what round two's base `5230b9f` didn't: media in (migration 20), typing, read receipts, places and events (migrations 21 to 24) and the Mac's work. Round two's seventeen commits are `88938765` to `f3d93673` here, in the same order. Two builders made the integration; their report is `reports/contact-integration.md` in the agent store.

Main moved during the audit. The library landed, and `origin/main` is now `77d44257`. Presence's lander has presence rebased onto it (`presence-land-trial`, `154979a2`), not yet pushed.

Everything was measured in one clone of the worktree at `/tmp/jaw-audits/send-contact-3/jaw`, built with `CARGO_INCREMENTAL=0` under `nice`, at a load of 33 to 74 on a shared Mac. Mutants were switched in and out there, and two trial landings were made from it. Nothing ran against the owner's folder or engine, and nothing was sent to WhatsApp. I ran the tests again rather than taking the integration's report for them.

Rounds two and one follow, as written; round two's "its verdict ends this note" was so until now. Round three's verdict ends the note, after round two's.

## In short

- **Approved.** Nothing loses data or leaves jaw.db inconsistent.
  - The seven changes beyond keeping both sides are right, and nothing else changed.
  - The migration's text, every statement, and the cards' rules and tests are round two's.
- **The range against round two.** Commits 3, 6, 8 to 11 and 13 to 17 are unchanged; 1, 2, 4, 5, 7 and 12 changed.
  - Of the 48 files the two ranges touch, 26 carry round two's lines exactly.
  - In the other 22, every difference is a conflict kept both ways or one of the seven changes.
- **The driver's arm** is main's four fields with round two's values: 64 messages a step, a millisecond for conversions, and each step going on from where the last stopped. The head's driver is main's with that one line added.
- **T5, a test gap, a follow-up.** Nothing pins the cards' step bound or their budget.
  - With no step bound, with copies' 16, or with no budget, every test passed, jawd's whole unit suite among them.
  - Main pins both for places and events, and links' bound.
  - It doesn't block. The values are round two's, which round one measured costing what main's own backfill does. And neither decides what's kept or how a row merges.
- **Tests at the head pass:** jawd's 73 targeted tests, the upgrade from version 1 through 25, jaw-proto's, the terminal's card tests end to end, and the engine's card tests.
- **Landing after the library and presence.** I made a trial landing onto both, and it builds and passes, but for one terminal test, whose words presence changes. The cards take:
  - migration 26;
  - `…0F` for Ana's card (the library's document has `…0D`);
  - main's Up loop in `failures.rs`, in place of the seventh press;
  - a looser check of the chat's header in their own end-to-end test.

## What was asked, checked

### The range against round two

`git range-diff 5230b9f..360e8bf5 e12d8d34..f3d93673`:

- **Unchanged:** commits 3, 6, 8 to 11 and 13 to 17.
- **Changed:**
  - Commit 1 changed by conflicts with places and events and by typing's argument, its message gaining the sentence naming the rule.
  - Commit 2 changed by conflicts, the arm and the number, its message saying "migration 25".
  - Commit 4 changed by the demo's IDs and the seventh Up.
  - Commit 5 (the terminal) changed by conflicts and the boxed card.
  - Commit 7 (the protocol) changed by conflicts.
  - Commit 12 changed by typing's argument, its message gaining the same sentence.

I compared the branch's own change, file by file, against round two's: `git diff -U0 5230b9f 360e8bf5` against `git diff -U0 e12d8d34 f3d93673`, each file's added and removed lines without their positions.

- **26 of the 48 files are line for line round two's.** Among them are `store/write/contacts.rs`, `convert/kinds.rs`, `jaw-proto/src/vcard.rs`, `send/contact.rs`, `store/write/outbox.rs`, `plans.txt` and the engine's `send.go`. So are the cards' own test files, but for typing's argument and `earlier`'s version (changes 2 and 4 below).
- **In the other 22, each difference is one of these:**
  - **Main's lines beside the branch's**, where both added at one place:
    - `BACKFILLS` (`store/write/links.rs:25`);
    - the `use` lists in `state.rs` and `store/mod.rs`;
    - the comments in `sync/backfill.rs` and `send/mod.rs`;
    - the requests in `send/tests.rs` and the fixtures' test;
    - the terminal's fields and imports;
    - the protocol's words.
  - **The seven changes below.**
  - **One line the branch drops, rightly.** Round two's change to the synced-commit test's outbox query (`SELECT id FROM outbox WHERE n = 1`) is main's already (`send/tests.rs:1276`), so the merge keeps it once.

### The seven changes beyond keeping both sides

**1. The driver's arm** (commit 2): `CONTACTS => (contacts::NEXT_CARDS, contacts::backfilled, Some(kinds::CONVERTING), STEP)`, at `store/write/links.rs:83`.

- **The rest of the driver is main's.** `backfill` (`:66-115`) is main's byte for byte but that line.
- **Round two's driver read every backfill's next messages `STEP` at a time.** It ended a step's reading at `STEP` too. Main's takes both from the arm's fourth field, and media's copies take 16 there. With `STEP`, the cards read as round two's did:
  - **Bounded:** each step reads the next 64 messages (`LIMIT ?3`, `:86-88`). It converts cards until its conversions have had a millisecond past the first card (`:97-99`).
  - **Resumable:** a step that stopped short writes `after` as its last message looked at, in its own transaction (`:105-108`). A step killed halfway is rolled back and taken again.
  - **Ends:** the row goes once a step has looked at everything it read and read fewer than 64 messages, or once it reaches `upto`.
- **Its tests pass where they live.** That's T2 in `store/write/links/tests.rs`, and the five card backfill tests in `store/write/contacts/tests.rs`.
- **The arm's reading and taking are pinned.** My mutants of them are killed (Mutants below).
- **Its bound and budget aren't pinned** (T5).

**2. Typing's argument** at the branch's two calls of `sync::run`: `send/contact/tests.rs:58` (commit 1) and T2's, `store/write/links/tests.rs:381-390` (commit 12). Both pass `crate::feed::Typing::default()`.

The rule in `docs/audits/2026-10-31-places-landing.md` has five conditions, and they hold:

1. **The parameter is main's,** and main's own test calls pass that default.
2. **The default is inert at the landed tree.** I read it on main and on presence's landing trial (`154979a2`):
   - `Typing::default()` builds its state in memory and spawns nothing until `start()`.
   - The cards' send rig runs `DemoEngine::sharing`, which has no typists on either tree.
   - T2's rig has no engine at all.
3. **Both calls are in `#[cfg(test)]` modules.**
4. **Nothing else changed at them.** The comparison above finds the one line in `send/contact/tests.rs`, and in T2's the same call laid out by rustfmt.
5. **The compiler shows it's complete.** `sync::run` takes eight arguments on main, the library and presence, so the branch's seven couldn't build.

Both commits' messages name the rule and the note.

**3. `store/schema/kept.rs`** is main's byte for byte: the range doesn't touch it, and round two's didn't either.

- The cards add no table. Their migration adds a row to `backfills`, which main classes as jaw's own (`kept.rs:72`) and seeds with a `links` row (`:369`).
- `kept`'s test seeds version 24 and runs the cards' 25 over it, and passes.

**4. The migration renumbered to 25** (commit 2). It's the last of `MIGRATIONS` (`store/schema.rs:1205-1212`), after places and events' 21 to 24.

- **Its text is round two's:** one `INSERT … SELECT` from `sqlite_sequence`, a schema step.
- **The lock line keeps round two's hash** (`migrations.lock:40`): `25 schema 3732b60a8d8a149874e0a84bdad22e7f9885505839e15bac4a7c020b3f2b79e4`. So `each_merged_migration_is_still_the_text_it_was_merged_with` holds the text to round two's.
- **The comment says 25.**
- **`earlier` builds the version just before** (`store/write/contacts/tests.rs:63-71`): `MIGRATIONS[..24]`, `user_version` 24, "A jaw.db at version 24". With 25 instead, four of the five card backfill tests fail (Mutants).
- **T2's jaw.db at version 13** doesn't move with a renumbering.
- **Commit 2's message** says "migration 25".
- **Main's runner takes it after 24, in order.** The every-earlier-version test and `a_data_step_runs_in_a_transaction_of_its_own_between_runs_of_schema_steps` check that runner.

**5. The demo cards' IDs** (commit 4) are `…0D` for Ana's dentist (`engine/demo.rs:519`) and `…0E` for the climbers' two (`:523`). The socket test that names them (`main.rs:1160-1161`) says the same.

- Main's script holds `…01` to `…0C`, so at the head each message has an ID of its own. `…05` and `…09` recur only where votes and answers name their poll and event.
- On today's main the library's document has `…0D` (O7).

**6. The terminal's boxed card** (commit 5, outside the data layer): `contact: Option<Box<Contact>>` (`jaw/src/data.rs:302`), as places boxed its own fields, for clippy's `large_enum_variant`.

- serde reads a box as what it holds, so the protocol is unchanged.
- The terminal's card tests pass.

**7. `jaw/tests/failures.rs:325`** (commit 4, outside the data layer) presses Up seven times.

- At the head, Ana's chat holds six messages: the texts `…01` and `…03`, the link `…04`, the photo `…08`, the pin `…0C` and the card `…0D`.
- Its 10 tests pass. The library's loop replaces it (Landing).

### Tests at the head

In debug, as the gate builds:

| Suite | Result | Took | Load |
| --- | --- | --- | --- |
| jawd's targeted unit tests (`contact card backfill plans store::schema demo_engines_account engine::demo each_sending_call kinds store::write::links`) | 73 passed, 6 measurements ignored, 0 failed | 846 s | 41 to 64 |
| jawd's `upgrading`: a version-1 jaw.db through every migration to 25, on the demo engine | passed | 14 s | 64 to 74 |
| jaw-proto | 25 passed (1 ignored) and its 3 fixture tests | 1 s | 70 |
| The terminal end to end: `composing`, `contacts`, `failures` | 3, 1 and 10 passed | 99, 177 and 108 s | 63 to 65 |
| The terminal's card tests (`card contact`) | 4 passed | 7 s | 63 |
| The engine's card tests (`go test ./internal/wa -run 'Contact\|EveryKind'`) | 3 passed | 1 s | 59 |

Among the 73:
- T1 to T4 and O3's tests;
- the five card backfill tests on `earlier` at 24;
- T2;
- the demo account's cards through the socket;
- the synced-commit test;
- both plan tests;
- every schema test: the frozen texts, unique numbers, a fault at every statement of every migration, every earlier version keeping its rows, the data step between runs of schema steps, and `kept`'s.

The integration's gate at `f3d93673` passed with listed flakes only; I didn't run the whole gate again.

### Mutants

One build held every mutant, each switched on by an environment variable, and it was built in debug, as the gate builds. With none set, the code is the head's. A control run with none set passed the same 9 tests the mutants were run against: the five card backfill tests, T2, and places' and events' backfill tests.

| Mutant | Result |
| --- | --- |
| The arm reads the next messages as places' does (`NEXT_PLACES`) | killed: the five card backfill tests and T2 |
| The scheduler leaves the cards out, as `BACKFILLS` without `CONTACTS` would (`sync/backfill.rs:32`) | killed: T2, the cards "never done" after 30 s |
| `earlier` builds version 25, the cards' own, not 24 | killed: four of the five card backfill tests |
| The arm takes no step bound (`i64::MAX` for `STEP`) | **survived** the 9, and jawd's whole unit suite (T5) |
| The arm takes no conversion budget (`None`) | **survived** the 9, and jawd's whole unit suite (T5) |
| The arm takes copies' step, 16 | **survived** the 9 (T5) |

**The whole suite with the last mutants on.** jawd's whole unit suite ran with no bound and no budget both on: 870 passed, 15 ignored and 4 failed, in 1,816 s at a load of 65 to 70. The four were the feed's lane test, the socket's connection-full test, and two store timing tests (`live_writes_go_in_with_a_stream_of_inbox_rows…` and `the_wal_stays_bounded_while_history_pours_in…`). Each opens a fresh jaw.db, where the cards' arm never runs, and each passed alone twice with both mutants still on.

### The library, now on main

The library meets the cards in three places, and none changes what the cards do:

- **Its triggers on `messages`.** Its migration adds `shelf`, a generated column of `kind`, `deleted` and `view_once`, null for a contact. It rewrites `messages_fts_update` and `messages_moved` to keep `library_fts` and `shelves` for items.
  - The cards' backfill writes `text`, `spans`, `summary`, `detail` and `v`, never a column `shelf` comes from.
  - So for a card's row, only the messages index's part of `messages_fts_update` fires, as on main.
  - The update's plan is still a search by rowid at the trial below, whose plan tests pass.
- **`store/write/links.rs`.** It changes `link()`, which now writes its message's event. The driver and `BACKFILLS` are as they were.
- **`write::index`** takes the stamps now. The cards don't call it.

### Two trial landings

Both were made in a worktree of the clone, its conflicts resolved by scripts, then built and tested in the clone. Neither is the branch.

**Onto the library** (`77d44257`, main now): three commits conflict. The other 14 apply by themselves.

- **Commit 2**, `migrations.lock` and `store/schema.rs`, where both sides add a migration 25:
  - The library's 25, a data step, goes whole, with its own closing `",`, then the cards' as 26. Its text is unchanged, and its lock line is `26 schema 3732b60a…`, the same hash.
  - `earlier` moves to version 25.
  - The two migrations share their closing lines, so git's hunk leaves the library's side without its closing `",`.
- **Commit 4**, `engine/demo.rs` and `failures.rs`:
  - The library's document goes first, then the cards, Ana's card moved to `…0F`, and `main.rs`'s test names `…0F`.
  - `failures.rs` is taken as main has it, so commit 4 no longer touches it.
  - The script then holds `…01` to `…10`, each once but `…05` and `…09`.

**Onto presence's landing trial** (`154979a2`, 8 commits on the library), from the first: commits 1 to 4 apply by themselves.

- **Commit 5** conflicts in `jaw/src/view/tests.rs`, where presence's test and the cards' three are added at one place.
  - Kept both ways, presence's first.
  - The closing brace both sides share falls outside git's hunk. The integration's first builder lost it that way on 31 October, and the terminal's tests didn't build.
  - The resolved file holds every line of presence's.
- **The other 12** apply by themselves. Trial head `be60d56d`.

At `be60d56d`:

| Check | Result | Took | Load |
| --- | --- | --- | --- |
| `JAW_PLANS=write cargo test -p jawd --bin jawd plans` | 3 passed; the written `plans.txt` is the merge's byte for byte: presence's file and the cards' six entries | 20 s | 70 |
| jawd's targeted unit tests (the head's, and `store::write::again`) | 81 passed, 7 measurements ignored, 0 failed | 1,377 s | 33 to 73 |
| jawd's `upgrading`, version 1 through 26 | passed | 5 s | 33 |
| The terminal: `composing`, `failures` | 3 and 10 passed, `failures` on main's loop with seven messages in Ana's chat | 46 and 32 s | 38 to 50 |
| The terminal: `contacts` | **failed** at its last assertion (`jaw/tests/contacts.rs:183`), Dr.ª Marta's chat open with `╭ +351213456789 · online ─`, where it looks for `╭ +351213456789 ─`; everything before it passed, Ana's card in the first frame beside the library's voice message among it | 17 s | 38 |
| The terminal's card tests, and presence's view test beside them | 5 passed | 1 s | 35 |
| The engine's card tests | 3 passed | 1 s | 35 |

Among the 81: `kept`'s, seeding 25 and running 26; T2; the card backfill tests on `earlier` at 25; and the demo account's cards with `…0F`. The schema tests passed too: the frozen texts, unique numbers, a fault at every statement, every earlier version, and the data step between runs of schema steps.

The one failure is presence's header saying the person's state, which the cards' test was written before; it's O8.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gaps

**T5. Nothing pins the cards' step bound or their budget** (a follow-up; it doesn't block).

- **Where:** `store/write/links.rs:83`, the arm's third and fourth fields.
- **What happens:** with the step unbounded, or 16, or the budget gone, every test passes, jawd's whole unit suite among them (Mutants).
  - The cards' one test that runs the whole backfill loops until it's done, so the step's size doesn't matter to it.
  - The hostile card's step is over one card, so the budget doesn't either.
- **Why it's open now.** At round two the cards' step was the driver's one `STEP`, which links' test pins (`links/tests.rs:265`). Media's fourth field made it each arm's own, so that test no longer reaches the cards'. The budget wasn't pinned at round two either: round two's mutants didn't try it, and they should have.
- **What's at stake is the writer's budget, not the data.** One step reading every message to `upto` would hold the writer for as long as reading them takes. A step converting 64 large cards would hold it for as long as their conversions take. Neither changes what a card's row ends as, nor what's kept.
- **Why it doesn't block:**
  - The values at the head are round two's. Round one measured the backfill with them at 28.2 s per million messages, with the writer's p99 at 5.27 ms against 5.74 ms for main's `kinds` backfill on the same binary and load.
  - Main has left an unpinned step bound as a follow-up before (media in's H30).
- **Fix:** two tests beside the card backfill tests, as places' are (`again/tests.rs:197` and `a_step_reading_64_locations_in_a_row_again_ends_once_it_has_for_a_while`, `:303`):
  - One step over a jaw.db from `earlier` with 200 messages, cards among them, reads 64 at most: `after` is 64 or less.
  - One step over 64 cards in a row ends once it has converted for a while, its `after` short of the 64th.
  - Each fails with no bound or no budget.

### Unpredictable performance

None new. U1 stands, as round two left it: the backfill and the reader are round two's code.

### Other (follow-ups; none blocks)

- **O6. `earlier` names its version as a number** (`store/write/contacts/tests.rs:65-71`).
  - Each renumbering edits three of its lines: twice so far, and a third time when it lands.
  - The library's tests find their migration by its text (`schema::before_the_library()`), so they don't move when it's renumbered.
  - Fix, if wanted: find the cards' migration the same way, as the place of the one that inserts `'contacts'` into `backfills`.
- **O7. Main's T1 meets this landing: nothing tests that the demo's messages have IDs of their own.** That's `docs/audits/2026-10-31-places-landing.md`'s T1.
  - The library's document took `…0D`, which Ana's card has here.
  - Messages are unique by `(chat, id)` (`messages_by_id`), and the two are in different chats, so neither would take the other's row in the demo's jaw.db.
  - But the script would hold one ID twice, and anything that finds a demo message by its ID alone would find either.
  - The lander moves Ana's card to `…0F` (Landing). Main's T1 fix, a test that every message in `DemoEngine::new()`'s script has an ID of its own, would catch the next one.
- **O8. Presence's header meets the cards' end-to-end test** (`jaw/tests/contacts.rs:183`, outside the data layer).
  - With presence, a one-to-one chat's header says its person's state, `╭ +351213456789 · online ─`.
  - The test looks for `╭ +351213456789 ─` and fails at the trial, after everything else in it has passed.
  - Fix at landing: look for the number at the header's start, `"╭ +351213456789 "`, which holds with a state or without. It's a change beyond keeping both sides, in the terminal's test, so name it in the landing report. It needs no data audit.

### Outside this range

- **Round two's follow-ups stand as they were,** since the code they're about is round two's: O4 (which of several `FN` lines names a card), O5 (the terminal reads a vCard at each layout) and U1.
- **X1 stands on main** (`77d44257`): `sync/drain.rs:382` still goes round to the same rows when a conversion panics.
- **X2 stands on main** as a design question.

## Measurements

- **The clone and its scratch.**
  - All of it sat in `/tmp/jaw-audits/send-contact-3/`: one clone of the worktree, the trials' worktree beside it, logs, scripts and `TMPDIR` (so every test's jaw.db and `JAW_HOME`).
  - The clone's `target` was the only build folder, built with `CARGO_INCREMENTAL=0`. Revisions were switched with `git checkout`: `f3d93673`, the mutants' build at `f3d93673`, then the trial `be60d56d`. Go's packages came from Go's shared build cache.
  - The folder reached 3.4 GB and is deleted.
- **Bounds.** Every step ran under `nice -n 10` and `gtimeout --kill-after`, which signals the whole process group.
- **Doubled commands.** Each step took a lock named for it, so a doubled command would have been refused. None ran twice: 23 locks for 23 logs.
  - Each log has one start and one end.
  - The exception is the alone runs' log, which has one result for each of its eight runs.
- **Machine:** Apple M1 Ultra, 20 cores, rustc 1.99.1, go 1.27.2. Other agents kept the load at 33 to 74.
- **The comparison with round two.** For each of the 48 files, the added and removed lines of `git diff -U0 5230b9f 360e8bf5` were compared with those of `git diff -U0 e12d8d34 f3d93673`, positions and hunk headers left out. 26 are the same; the 22 others are read above.
- **Mutants:**
  - The switches were put in by a script that edits three files (the arm, the scheduler's loop and `earlier`), and taken out by `git checkout` after the runs.
  - The test binary was run directly with each variable set, against the 9 backfill tests, or T2 alone, or the card backfill tests alone, and then whole.
- **The trials:** scripted resolutions, each checking that the hunk was the one expected before joining it.
  - The two migrations: the library's whole, then the cards' as 26.
  - The demo's messages: the document, then the cards, Ana's at `…0F`.
  - `failures.rs` as main has it.
  - `view/tests.rs`: presence's test, its brace and a blank line, then the cards'.
  - Each was compared with the file it came from: the trials' `failures.rs` is main's byte for byte, and `view/tests.rs` drops none of presence's lines.
- **What I didn't touch:**
  - I opened nothing under `~/Library/Application Support/jaw`, copied no `session.db`, and ran no Go engine. The terminal's tests ran `jawd --engine demo` on temporary folders inside the scratch. The engine's Go tests ran on a made-up device in a temporary folder, calling its handlers directly.
  - Nothing was sent to WhatsApp.
  - I signalled no process.
  - I only read the library's and presence's worktrees, by `git fetch` into the clone and `git show`.

# Round two: `5230b9f..360e8bf5`, round one's eight findings and the rebase onto main

Range: `5230b9f..360e8bf5` in `send-contact`, seventeen commits, 30 October 2026. Auditor: database-expert. Paths are under `crates/` unless they say otherwise, and line numbers are at `360e8bf5`.

The worktree stayed at `360e8bf5`, with nothing uncommitted but this note. Round one's nine commits are `1fd3eca1` to `09a6bfc7` here, rebased onto main at `5230b9f4`, where folders took migration 19. Eight follow, one for each of round one's findings: `cf6cb530` (P1), `d8ddeca9` (T1), `9057f44e` (T2), `4bb733bb` (T3), `bca940b8` (T4), `47464b06` (O1), `aafde771` (O2) and `360e8bf5` (O3). Main has since moved to `abfa15a4`, with the keepalive fix.

Everything was measured in one clone of the worktree at `/tmp/jaw-audits/send-contact-2/jaw`, built with `CARGO_INCREMENTAL=0` under `nice`, with mutants switched in and out there, at a load of 60 to 79 on a shared Mac. Nothing ran against the owner's folder or engine, and nothing was sent to WhatsApp. The builder's numbers were measured again rather than taken.

## In short

- **Approved.** Nothing loses data or leaves jaw.db inconsistent, and every rule this round adds or changes has a test that fails without it. Round two's verdict ends this note, after round one's.
- **P1 is fixed.** The reader is linear on every shape I built or fuzzed, up to 16 MiB.
  - Round one's open-quote card of 256 KiB, 3,870 ms then, reads in 1.1 ms; at 1 MiB it takes 2.3 ms, and at 16 MiB 63 ms.
  - The costliest shapes I found, with a line or a part for every byte or two, cost 12 to 16 µs per KiB: 3.1 ms at 256 KiB, and up to 0.24 s at 16 MiB.
  - Five million fuzzed inputs: no panic, every bound held, and `first` always `read`'s first card.
  - Against round one's reader, every difference is one of the two edge cases the builder named, and real exporters' cards read the same. `card_name` reads only the first card.
- **The writer's budget, U1, a follow-up.** A live card never reads its vCard on the writer; a backfill step does.
  - A step over one large card holds the writer for as long as converting and writing it take: past 3 ms from about 256 KiB, whatever the card's shape, since writing the row alone takes 3.6 ms there; up to 0.43 s at the frame's 15 MiB, once.
  - The reader now costs at most as much again as the write. The same card arriving live already holds the writer for 97 to 139 ms at 15 MiB on main.
- **T1 to T4 and O3 are closed.** Each of their eight rules, switched off by me, fails its test and no other.
- **O1 and O2 are done.** The audit's check names the converter and the reader, and the protocol says who clears a draft.
- **The rebase is right.** Migration 20 is a schema step after folders' 19, its text and hash unchanged. Nothing else changed but context and `sync::run`'s new argument.
- **The trial rebase onto `abfa15a4` has no conflicts.** Its build, its suites and the engine's Go tests pass, but for two of main's timing tests, which pass alone.
- **My mutants:** 16, of which 15 were killed. The one that survived, a card's first `FN` winning over its last, is O4, a follow-up: this round didn't change that rule.

## What was asked, checked

### P1: the reader

**It's linear on every shape I could build.** `lines()` (`jaw-proto/src/vcard.rs:269-297`) reads each line's head once, at most 1 KiB of it (`Head::read`, `:248-262`), decides once whether the line is quoted-printable, and looks only at each joined piece's own last character. Joining is amortised: a line is copied once, when it first goes on. The cards' reading (`cards`, `:54-72`) parses each line's head once more, again at most 1 KiB, and decodes a card's names once, at its end. In a release build, the fastest of three runs, at a load of 61 to 69:

| Card | 256 KiB | 1 MiB | 4 MiB | 16 MiB |
| --- | --- | --- | --- | --- |
| A photo, base64 folded at 75 columns | 0.07 ms | 0.23 ms | 1.5 ms | 5.8 ms |
| Quoted-printable broken softly (`x=` lines) | 0.66 ms | 2.6 ms | 10.6 ms | 67 ms |
| A quote left open, folded lines ending in `=` | 1.1 ms | 2.3 ms | 10.8 ms | 63 ms |
| Blank lines | 2.9 ms | 11.4 ms | 45.6 ms | 193 ms |
| Short `FN:a` lines, folded (the time fuzz's worst) | 3.1 ms | 12.4 ms | 50.6 ms | 206 ms |
| One structured name of semicolons | 1.5 ms | 6.1 ms | 66 ms | 238 ms |

Round one measured the two hostile shapes at 98 ms and 3,870 ms at 256 KiB. Besides these, I timed 23 other shapes and two fuzzes for time. The shapes were heads of 1,023 and 1,024 bytes on every line, `FN` and `TEL` lines with 1 KiB heads, a head folded every few bytes, quoted-printable breaks and folds alternating, one quoted-printable name of 16 MiB, escapes, quotes toggling in every head, carriage returns alone, a 16 MiB line without a colon, `BEGIN` and `END` lines by the million, BOMs and lines of one property each. One fuzz repeated each of 400 random units of up to 12 vCard tokens to fill 256 KiB; the other timed 100 random texts of 256 KiB. The ten costliest units each grew 256 to 280 times from 64 KiB to 16 MiB, for 256 times the size. No shape costs more than about 16 µs per KiB. The costliest are those with a line, or a part, for every byte or two: each line costs about 11 ns. Only the structured name of semicolons cost more per KiB at 4 MiB than at 1 MiB (16 µs against 6), and it fell back to 15 µs at 16 MiB. That's millions of small allocations, one string for each part, not a pass made again.

**Panics, bounds and round one's reading.** I fuzzed `read`, `first` and `is_one` with 3,000,000 inputs of up to 64 vCard tokens, and 2,000,000 cards built line by line from a grammar. The grammar covers groups; parameters (quoted-printable, quoted, left open, and a 1 KiB one now and then); values ending in `=` and `==`; escapes and multi-byte text; folds and soft breaks; empty lines; mixed line ends; and cards begun and ended anywhere. None panicked and every bound held. `first` gave `read`'s first card every time, and whenever `is_one` held, `read` gave one card. I compared each input with round one's reader (`b8db246c`), and with that reader given each of the two changed rules, then both:

| Fuzz | `read` the same | Explained by the head bound | By the soft break | By both | Unexplained |
| --- | --- | --- | --- | --- | --- |
| 3,000,000 token inputs | 2,999,907 | 93 | 0 | 0 | 0 |
| 2,000,000 grammar cards | 1,924,109 | 73,438 | 2,330 | 123 | 0 |

`is_one` agreed on all the token inputs, and differed on 1,840 grammar cards, each explained the same way (1,381, 458 and 1). So nothing changed but the two edge cases the builder named. Each fuzz's slowest input, timed again, took 1 to 3 µs; the 20 and 54 ms seen once were the scheduler.

**The two changed edge cases are right for real cards.** I read nine cards as their exporters write them with both readers, and they gave the same cards and the same `is_one`:

- macOS Contacts' vCard 3.0, with a photo, labels, an address, a social profile and an IM;
- iCloud's, its photo folded, with a crop rectangle;
- WhatsApp's on an iPhone and on Android;
- Android Contacts' vCard 2.1, with quoted-printable names broken softly, one break followed by the property's own empty line;
- Outlook's vCard 2.1, with a quoted-printable note;
- Google's folded vCard 3.0;
- Samsung's two cards in one file;
- jaw's own `write`.

What each change means for cards like these:

- **A head of 1 KiB.** No exporter writes a property's name and parameters anywhere near 1 KiB. Apple's longest are a social profile's and a photo's crop rectangle, under 100 bytes. A line skipped loses that property alone. `BEGIN`, `END`, `FN`, `N` and `TEL` have heads of a few bytes.
- **A soft break only from its own line.** A quoted-printable encoder writes `=` as `=3D`, so what comes before a soft break's `=` is never a literal `=`. Android's encoder can end a value with a soft break and then the property's own line end, which leaves an empty line. Both readers read that alike, since a hex digit comes before the `=`. Only a card made by hand or broken, with `==` before an empty line, reads differently, and the new reading is the right one. The old one carried the break past the empty line and swallowed the line after it: in the grammar fuzz's first such case, a card's `END:VCARD`.

**`card_name` reads only the first card.** `vcard::first` is `cards(text, 1)` (`vcard.rs:49-51`), which returns at the first card's end, or at the next `BEGIN`, and `card_name` uses it (`jawd/src/convert/kinds.rs:318-321`). For a message of several cards, `card_name` runs once for each, and each reads only its own vCard's first card, so the cost stays linear in the message. My mutants of both are killed (Mutants below).

**The writer's budget.** A live card never reads its vCard on the writer: the inbox's conversion runs on a blocking thread (`sync/drain.rs:348`). The backfill's step does read it on the writer, inside the step's transaction (`store/write/contacts.rs:69-73`), as main's `kinds` backfill converts its rows. So I timed one step over a card of each shape, from a jaw.db at version 19, the fastest of three. Each card was timed twice: nameless, so the reader runs, and named by its message, so it doesn't and the step is otherwise the same. Release, at a load of 64 to 70:

| Card | 64 KiB | 256 KiB | 1 MiB | 4 MiB | 15 MiB |
| --- | --- | --- | --- | --- | --- |
| Folded as phones fold, nameless | 1.3 ms | 4.2 ms | 14.7 ms | 65 ms | 236 ms |
| The same, named by its message | 1.2 ms | 3.6 ms | 12.8 ms | 51 ms | 201 ms |
| A photo, nameless | 1.3 ms | 2.4 ms | 7.2 ms | 29 ms | 117 ms |
| The builder's hostile card, nameless | 1.5 ms | 4.5 ms | 16.2 ms | 62 ms | 250 ms |
| Blank lines, nameless | 2.5 ms | 9.2 ms | 28.0 ms | 116 ms | 433 ms |
| Blank lines, named | 1.6 ms | 6.2 ms | 17.3 ms | 76 ms | 278 ms |
| Short `FN:a` lines folded, nameless | 2.2 ms | 7.1 ms | 25.4 ms | 102 ms | 408 ms |
| The same, named | 1.3 ms | 3.7 ms | 13.9 ms | 51 ms | 206 ms |

The engine's link takes frames of up to 16 MiB (`engine/frame.rs:10`), so 15 MiB is the most a card can be. What this shows:

- **A step over one large card can pass 3 ms whatever its shape.** Writing the row is most of the cost. The named card of 256 KiB, its vCard never read, took 3.6 ms, since its vCard is in the row twice: compressed in `raw`, and escaped in `detail`.
- **The reader now adds at most about as much again:** 3 to 3.4 ms per 256 KiB in the costliest shapes, and nothing measurable for the builder's.
- **One step over a hostile card of 15 MiB holds the writer for up to 0.43 s,** once. That's U1.
- **The builder's test** holds its own hostile shape to twice a plain card of the same size, and that shape takes 1.06 times one. Blank lines take 2.2 times at 256 KiB, partly because their row is larger: a `\n` is two bytes in JSON. The test guards the quadratic shape, which is what it's for, and it passes three times out of three in release.

The same cards arriving live, for comparison: the writer's apply of the converted message, the fastest of three.

| Card | 256 KiB | 1 MiB | 4 MiB | 15 MiB |
| --- | --- | --- | --- | --- |
| Folded as phones fold, as a card | 3.2 ms | 8.0 ms | 64 ms | 119 ms |
| Blank lines, as a card | 2.9 ms | 10.9 ms | 48 ms | 139 ms |
| Short `FN:a` lines folded, as a card | 2.1 ms | 8.7 ms | 30 ms | 97 ms |
| The same bytes as a text message | 1.1 to 1.3 ms | 3.7 to 4.9 ms | 15 to 23 ms | 46 to 60 ms |

The conversion, off the writer, took at most 4.0 ms at 256 KiB and 252 ms at 15 MiB (short folded `FN` lines). Main's converter took 4.9 ms and 319 ms on the same bytes as a text message, and 603 ms for blank lines. So a hostile card holds back the drain, or a forward on a tokio worker (`send/forward.rs:71`), for no longer than a text of its size does on main. `send.contact` takes 256 KiB of vCards at most, so its check (`send/contact.rs:57`) costs at most 3 ms in the costliest shape.

### T1 to T4 and O3, each switched off

Each mutant was built into jawd's test binary and run against the eight tests for this round's rules. Each failed the one test meant for it and no other:

| Rule switched off | Test | What it said |
| --- | --- | --- |
| T1: a poll clears the draft again (`kind != "contact"` for `kind == "text"`, `store/write/outbox.rs:696`) | `a_card_or_a_poll_sent_reads_its_chat_and_leaves_what_was_typed_there…` | "and stays after a poll too": the draft was gone |
| T2: `contacts` out of `BACKFILLS` (`store/write/links.rs:19`) | `every_backfill_the_migrations_leave_is_done_as_jawd_runs_until_none_is_left` | `["contacts"]` of the four "never done", after 30 s |
| T3: the pending card keeps the expiry its conversion gave it (`send/contact.rs:121` gone) | `a_card_asked_again_with_its_key_goes_once_and_cards_in_a_disappearing_chat_carry_its_timer` | "its timer waits for it to go": an expiry while pending |
| T4: no arm passing a transient error up (`store/write/contacts.rs:45` gone) | `a_disk_filling_as_a_card_is_read_again_fails_its_step_which_is_tried_again_and_reads_it` | the card left unread, in quarantine |
| O3: a name of spaces taken for one card | `a_contact_row_is_its_name_and_vcard_for_one_card_or_several…` | summary `"Contact:"`, text `""` |
| O3: a name of spaces taken for several | the same | name `""` |
| O3: the summary not clipped | the same | "clipped: Contact: Marta Marta …" |
| O3: the backfill takes a conversion that isn't a card (`store/write/contacts.rs:79`) | `cards_an_earlier_jawd_left_are_read_again…` | "retold left as it was": the row overwritten as text |

- **T1** sends a poll after `draft.set` and finds the draft there, then text, which clears it.
- **T2** opens a jaw.db at version 13, from before 14, the first migration that leaves a backfill. 14, 15, 16 and 20 leave one, and `BACKFILLS` holds all four. It runs jawd's sync as it starts without an engine and waits for `backfills` to empty, so a later migration whose backfill the scheduler doesn't run fails it too.
- **T3** holds a card at the server, finds no expiry while it's pending, and finds one after `at` once it has gone.
- **T4**'s hook fails one card's read with `SQLITE_FULL` once it has written, the first time only. The step is rolled back and taken again, and both cards are read with nothing in quarantine.

### O1

`scripts/check-data-audit.py` names `crates/jawd/src/convert/` and `crates/jaw-proto/src/vcard.rs`, and `.cursor/rules/data-integrity.mdc` lists both. With the script's paths at `5230b9f`, a change to `convert/kinds.rs`, `convert/mod.rs` or `vcard.rs` alone wasn't flagged; at the head each is. `jaw-proto/src/lib.rs` and the terminal's `layout.rs` still aren't, rightly.

### O2

`send.poll`, `send.contact` and scheduling's rule, still proposed, say that an interface that makes a poll or cards from the box's text clears the draft itself with `draft.set` and empty text, which is what round one asked.

### The rebase

`git range-diff 7262a15..b8db246c 5230b9f..09a6bfc7` shows round one's nine commits changed only as the builder said:

- **Unchanged:** commits 3, 4, 6, 8 and 9.
- **Commit 1:** `sync::run`'s seventh argument, passed in `send/contact/tests.rs`.
- **Commit 2:** the migration is 20 in its comment, its commit message and its lock line, `20 schema 3732b60a…`. That's round one's hash, so its text is the same. `plans.txt` keeps folders' plans and the cards'. The backfill tests' `earlier` builds version 19 (`MIGRATIONS[..19]`, `user_version` 19), the version just before.
- **Commits 5 and 7:** context only, in the terminal's view tests and the protocol's `status` row.

Migration 20 is still a schema step of one `INSERT … SELECT` from `sqlite_sequence` (`store/schema.rs:994-1001`). From version 18, the runner (`steps`, `:1524-1537`) runs folders' 19 alone, as a data step, and then 20, each in its own transaction, in order. The only version-pinned tests are the backfill tests' `earlier` at 19 and T2's jaw.db at 13. The 13 doesn't move with a renumbering.

### The trial rebase onto `abfa15a4`

`git replay --onto abfa15a4 5230b9f..send-contact` rebases all seventeen commits with no conflicts, to `6ca1f246`. Its tree is the one `git merge-tree` makes of main and the branch (`901ff81f`). Main's eleven commits and the branch share two files, `send/tests.rs` (a test main added) and `protocol/README.md` (the keepalive's words), and the branch's changes to both are the same after the rebase. `sync::run`, `sync::starting` and `Shared`, which T2 uses, didn't change on main. At the trial head, in debug, at a load of 64 to 79:

- **It builds,** and the eight tests for this round's rules pass.
- **jaw-proto's suites pass:** 25 unit tests and 3 fixture tests.
- **jawd's 706 unit tests:** 691 passed and 13 are ignored measurements. The other two failed on time, main's long-request lane test (`feed::tests::a_long_request_holds_up…`) and its connection-full test (`sock::tests::a_connection_full_when_its_change_is_answered…`), both on the builder's list of main's timing tests. Each passed alone three times out of three, and neither involves a card.
  - Among the 691: the frozen-migrations, unique-numbers, fault-at-every-statement and every-earlier-version tests, and the data-step-between-schema-runs test.
  - Both plan tests, for the statements and the triggers, passed.
- **jawd's four integration suites pass,** the upgrade from every fixture among them.
- **The engine's Go packages pass,** `internal/wa` and `internal/link`, with the card tests and main's keepalive tests together.
- **The log shows each summary once,** so nothing ran twice.

### Mutants

Besides the eight above, I ran these. Each was built and run in the clone in debug, as the gate builds.

| Mutant | Result |
| --- | --- |
| Round one's reader, with the new tests | killed: 208 ms on the softly broken card of 256 KiB against 50 ms, and the padded line read |
| The same, in jawd | killed: a step over a hostile card of 64 KiB took 3.88 s, against 300 ms |
| `first` reading every card | killed: the first card of many in 200 ms, against 5 ms |
| `card_name` reading every card | killed: a card and 256 KiB after it in 77.6 ms, a plain card in 22.1 ms |
| No head bound | killed: the padded line read |
| A soft break judged by the whole line's end | killed: the `=` an empty line goes on from taken for a break |
| Quoted-printable decided again for each piece (the same reading, at 1 KiB a piece) | killed: 95 ms against 50 ms; in release it would pass |
| The first `FN` wins, rather than the last | **survived** jaw-proto's whole suite (O4) |

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gaps

None. Every rule this round adds or changes fails a test when it's switched off: round one's T1 to T4, O3's three rules, and P1's four (the reader's cost, the head bound, the soft break and the first card), each by the mutants above. The one untested rule I found, which of a card's several names wins, isn't one this round changed (O4).

### Unpredictable performance

**U1. A step of the `contacts` backfill over one large card holds the writer for as long as converting and writing it take** (follow-up; it doesn't block).

- **Where:** `store/write/contacts.rs:58-101`. Each card is read and converted on the writer, inside the step's transaction, as main's `kinds` backfill does its rows, and a step takes its first card whole.
- **Measured (release, load 64 to 70):**
  - A card folded as phones fold: 4.2 ms at 256 KiB, 14.7 ms at 1 MiB, 236 ms at 15 MiB.
  - The costliest hostile shape, blank lines: 9.2 ms, 28 ms and 433 ms.
  - Writing the row is most of it. The plain card named by its message, so that its vCard isn't read, took 3.6 ms, 12.8 ms and 201 ms.
  - A live write behind such a step waits that long, and so does every inbox commit and its acks.
- **Why it doesn't block:**
  - It's linear. P1 was the reader's quadratic cost, which is gone: the reader now costs at most as much again as writing the row.
  - Main's own paths already pay most of this for such a row. The same card arriving live holds the writer for 3.2 ms at 256 KiB and 97 to 139 ms at 15 MiB. Its conversion, off the writer, costs less than main's converter takes for a text of the same bytes.
  - It happens once a card, and only for cards jaw.db held before migration 20.
  - Only a hostile or broken sender makes a card past a few hundred KiB. WhatsApp's phones send cards of a few hundred bytes, and a step over a photo's card of 256 KiB took 2.4 ms.
- **Fix, if wanted (for `kinds` too):**
  - Convert outside the writer: read the raw protobuf on a reader connection, convert on the blocking pool, then write in the step with a guard on the row's `v`. The step then costs what the live write does.
  - Or bound the bytes `card_name` reads for a name, at the cost of the name of a card whose `FN` comes after a large photo.

### Other (follow-ups; none blocks)

- **O4. Which of a card's several `FN` or `N` lines names it has no test.**
  - The reader keeps the last of each (`Written::keep`, `jaw-proto/src/vcard.rs:129-134`), as round one's did, and the two readers agreed on the fuzzes' cards with several.
  - With the first kept instead, jaw-proto's whole suite passed, and none of jawd's tests has a card with two.
  - vCard 3.0 allows one `FN`, so only a broken or hostile card has several, and the converter and the backfill read such a card alike, so no row disagrees with another now. But this rule decides the name such a card's row keeps, and a later change could flip it unseen, leaving old rows and new ones to disagree until a backfill.
  - Fix: a card with two named `FN` lines, and one with a blank `FN` after a named one, beside `a_card_without_a_formatted_name_is_named_by_its_parts…`.
- **O5. The terminal reads a row's whole vCard each time it lays a card out** (`jaw/src/layout.rs:116`, `jaw/src/data.rs:310-311`; outside the data layer).
  - With the linear reader, a hostile card costs it up to about 3 ms a layout at 256 KiB, and 0.2 s at 15 MiB, against the 16 ms budget for a keypress drawn. A real card with a photo costs it under 0.1 ms.
  - Fix, if wanted: read a row's cards once, when the row arrives.

### Outside this range

- **X1** (round one's) still stands on main: a conversion that panics wedges the inbox's drain (`sync/drain.rs:348`). The fuzzes found no panic in the reader, and round one's found none in the converter's cards.
- **X2. Nothing bounds a message below the engine's 16 MiB frame.**
  - On main's live path, one large message holds the writer for as long as its row takes to write: a card of 15 MiB for 97 to 139 ms, a text of the same bytes for 46 to 60 ms. The live budgets are 5 and 10 ms.
  - Its conversion holds the drain for up to 0.6 s.
  - The raw protobuf must be kept whole, but what's derived from it for showing (`detail`, `text`, `spans`) could be bounded. That would bound this, U1 and every backfill's step alike. It's a design question for main.

## Measurements

All in the clone: at `360e8bf5` for the reader, the steps and the mutants, and at `6ca1f246` for the trial rebase's suites. The audit's scratch went into the clone only: `crates/jaw-proto/examples/vcard_audit/` (the reader's fuzzes, its shapes, the real cards, and round one's reader with its two switches), and two measurements appended to `store/write/contacts/tests.rs` for their runs and taken out again. The load was 60 to 79 throughout, from three other audits and from builds.

- **The reader (release):** each shape at 256 KiB, 1, 4 and 16 MiB. `read`, `first` and `is_one` were timed the fastest of three runs, or of one at 4 and 16 MiB, and the slowest of the three is given. The token fuzz was 3,000,000 inputs of up to 64 tokens, in 51 to 65 s, run twice with the same seed and the same results. The grammar fuzz was 2,000,000 cards in 104 s. The time fuzz was 400 units at 256 KiB and 100 random texts, in 0.7 s, then the ten costliest units at five sizes.
- **The steps (release):** each run made a jaw.db at version 19 holding one nameless card as an earlier jawd kept it, and opened it, which runs migration 20. It then applied the backfill's step as the scheduler applies it. `Applied::took` is the writer's time on it, commit included. The card was set back with a second connection and the step taken again, three times, and the fastest is given. The WAL reached 60 MiB at most, for the largest card.
- **The live writes (release):** each run made a jaw.db at the head's version, converted a message as the drain does (`convert::message`, live), and applied the result in the live lane. Three messages were written, and the fastest is given. The conversions are `convert::message` alone, the fastest of three.
- **Mutants:** each was built in debug, as the gate builds, and run against its tests, with the files put back by `git checkout` after each.

# Round one: `7262a15..b8db246c`, kept as written

# Audit: contact cards, both ways (`send-contact`)

Range: `7262a15..b8db246c` in `send-contact`, 29 October 2026. Auditor: database-expert. Round one.

Nine commits on `origin/main` (`7262a152`): `send.contact` and the outbox kind `contact`, the converter's `contact` row and jaw-proto's vCard reader (`898f9bde`); migration 19, a schema step leaving the `contacts` backfill, and `contacts_backfill` in status (`e972dba6`); the engine's ContactMessage and ContactsArrayMessage and its refusals (`9110c262`); the demo engine (`ef0bd026`); the terminal (`4f973106`); the parity ledger (`d4f18ec8`); the protocol and its limits (`ec34ec5f`, `dc2fc219`); and the scheduled-messages proposal made consistent (`b8db246c`, docs only).

Everything was measured in one clone of the worktree at `/tmp/jaw-audits/send-contact/jaw`, built with `CARGO_INCREMENTAL=0` under `nice`, mutants switched in and out there, at a load of 55 to 75 on a shared Mac (about 22 for the last recheck). Nothing ran against the owner's folder or engine, and nothing was sent to WhatsApp.

## In short

- **Verdict: changes needed,** for one performance finding that breaks the live budgets and four missing tests. Nothing loses data and nothing leaves jaw.db inconsistent.
- **P1, unpredictable performance (must be fixed).** jaw-proto's vCard reader is quadratic on a card whose folded or soft-broken lines end in `=`. In release, one nameless card of 256 KiB built that way held the `contacts` backfill's step on the writer for 3,987 ms, with a live write waiting 3,936 ms behind it. The same card stalls the inbox's conversion, `send.contact`'s check and a forward for as long. Real cards read in well under a millisecond.
- **T1 to T4, test gaps (add this round).** My mutants survived jawd's whole suite for four rules this branch touches: a poll leaving the chat's draft; the scheduler running the `contacts` backfill at all; a pending card in a disappearing chat waiting for the server before its timer starts; and the backfill trying a step again after a full disk rather than quarantining the card.
- **The rest holds.** `send.contact` inherits `send.text`'s rules (a synced commit before the answer, the key, the outbox and its thousand, a kill at any step). The backfill is idempotent, resumable and exact: in a dense run, every card ended in the new shape, there was one event per changed card, and the search index matched the text exactly. Migration 19 is a constant-cost schema step. The engine's messages, refusals and `kept` are right.
- **The poll's draft:** the change stands. Clearing a draft that isn't what was sent lost typed text; nothing relied on it.
- **`convert/` in the audit's paths:** it belongs there, and so does `crates/jaw-proto/src/vcard.rs` (O1).
- **The backfill's cost** with real cards is main's `kinds` backfill's: 28.2 s per million messages with one card in a hundred (35,000 messages a second), against 31.7 s for `kinds` on the same binary at the same load.

## What was asked, checked

### Untrusted vCards: bounded in what they give, not in what they cost

jaw-proto's `vcard::read` bounds what it returns: 256 cards, 16 numbers a card, names of 256 characters and numbers of 64. Nothing makes it panic. I fuzzed it with 3,000,000 inputs built from vCard's tokens (marks, folds, quoted-printable soft breaks, escapes, quotes, a BOM, NULs, multi-byte characters): none panicked, every bound held, and the slowest took 4.6 ms. I also fuzzed the converter with 200,000 contact messages built the same way (one card or an array of up to four; names and vCards present, blank or missing; yours or theirs). None panicked, each converted to the same row live and from history, and each had one `contact` object and a summary of at most 120 characters.

What it costs isn't bounded. `lines()` joins a folded or soft-broken line one piece at a time. After each piece that ends in `=`, it parses the whole line so far again (`Property::of(last)`, `vcard.rs:199`). That copies the value and, when a parameter opens a quote it never closes, walks the whole line looking for its colon. A card built that way is quadratic in its size, and `is_one` reads the same way. Measured in a release build of jaw-proto:

| Input | 64 KiB | 256 KiB | 1 MiB |
| --- | --- | --- | --- |
| A card with a photo, base64 folded at 75 columns | 0.04 ms | 0.07 ms | 0.42 ms |
| Quoted-printable soft breaks (`x=` lines) | 9.9 ms | 98 ms | 1,246 ms |
| Folded lines ending in `=`, a parameter's quote left open | 241 ms | 3,870 ms | 64,469 ms |

Where it's read:

- **In the writer, by the backfill.** `contacts::read_again` converts each card on the writer thread, inside the step's transaction (`store/write/contacts.rs:69-73`). The converter reads the vCard whenever the card's message gives no name (`convert/kinds.rs:317-319`). The step's millisecond budget only stops it from taking a second card; the first always runs whole. In release, one step over a nameless card of 256 KiB held the writer for 3,987 ms, and a live write asked 50 ms into the step waited 3,936 ms (247 ms at 64 KiB, 30 ms at 16 KiB). Every inbox commit and every ack waits behind such a step, and it happens again at every start until the step commits.
- **In the live converter.** The inbox's conversion (`sync/drain.rs:348`) runs off the writer, but the drain waits for each batch before starting the next. So one such card, from anyone who can message the owner, holds back every message after it for as long, at every start until it's drained. The engine's acks follow the inbox's commit, not the conversion, so nothing is lost; nothing new shows meanwhile.
- **On the socket's tasks.** `send.contact`'s check calls `is_one` (`send/contact.rs:57`), and a forward converts what it forwards (`send/forward.rs:71`, `:159`), both on one of jawd's four tokio workers rather than a blocking thread. 256 KiB, the most `send.contact` takes, holds a worker for 3.8 s.
- **In the terminal,** which reads a row's vCard each time it lays the card out (`jaw/src/layout.rs:116`, `data.rs:282`), so such a card on screen freezes it for as long per frame. That's outside the data layer, but the same fix covers it.

A real card never does this: a photo's lines end in `=` once at most. It takes a hostile or broken sender. WhatsApp carries cards with photos of a few hundred KiB, so the sizes above are reachable. This is P1.

### Polls no longer clear the chat's draft

Judged against the design, the change stands. A draft is jaw's own data ("What may be rebuilt": never dropped). `send.text` clears it because what was sent is what was typed there. A poll's question and options, or a card, aren't what was typed in the box, so clearing the draft when one went threw away text the owner had typed, with no way back. That breaks the seventh principle's "nothing is ever lost".

Nothing relied on the old behaviour. No interface sends `send.poll` yet: the terminal answers `/poll` with "polls come with M2", and the Mac's "Poll…" command is disabled until jawd serves it. The terminal's palette sends a card without touching the box, which now matches jawd. The protocol says it for both (`send.poll`: "but for its chat's draft, which stays"; `send.contact`: "as a poll's does").

Two things follow. An interface that someday builds a poll or a card from the box's own text (a `/poll` typed there) must clear the draft itself with `draft.set` and empty text, or the words come back when the chat opens again; that's O2. And the poll's half of the rule has no test (T1).

### `convert/` and the audit's paths

It belongs there. The converter decides every row jaw derives from raw:

- each message's kind, the text the search index holds, its summary and its detail;
- the amendments the store merges (edits, reactions, deletions, receipts, votes), with the targets, senders and times their merge rules go by;
- whether something waits in `pending` or is kept for a later jawd.

A converter change alone can make a row disagree between its live and history copies, so that every replay flips it, or give an amendment the wrong target or time. A converter change is also what makes a backfill necessary at all, as this branch's does. The data-integrity rule counts "any change that touches, or changes the behaviour of" what's written, and says to extend `scripts/check-data-audit.py` when the data layer grows.

It has been audited so far by luck. Of main's 15 commits that touch `crates/jawd/src/convert/`, 14 also touch a listed path, and the 15th (`5a8a4687`, tests only) carried a `Data-Audit` trailer anyway. Nothing in the check would stop a converter-only change. The same now holds for `crates/jaw-proto/src/vcard.rs`: its `read` decides the name a nameless card's row keeps (`text`, `summary` and `detail.contact.name`), on both the converter's and the backfill's paths. This is O1.

### Sending

`send_contact` (`send/contact.rs:90`) is `send_poll` with the cards in place of the poll, and every rule it inherits holds:

- **Durable before answered.** It writes through `durably` (`apply_durable`, a synced commit flushed in full) and answers from `enqueued` after it. `send.contact` is in `each_sending_call_that_keeps_something_is_one_synced_commit`, and my mutant writing it through an unsynced live apply was killed there.
- **The key.** It's checked before the write (`outbox::keyed`) and again by the writer inside the commit (`keyed_before`), so two asks at once write one message. The test sends the same key twice at once and once more after the card has gone, and counts one message and one send. The key is kept in `send_keys` in the commit that writes the card, and not at all when the outbox is full, so asking again later sends it.
- **The outbox.** The kind `contact` takes no branch of its own anywhere in the sending lane. `is_action`, `at_end`, `held` and the withdrawal rules name only the actions and the played kinds, and `queued`, `resume` and `attempt` read every kind alike. Forwards on main already wrote outbox rows of kind `contact`. So retries with the same ID, a refusal failing it until `send.retry`, a refusal after an attempt waiting a day for a receipt, both deletions, and the thousand (`full`, counted from the rows in the writer) all behave as for text. The branch tests retries, a refusal, both deletions and the thousand for a card.
- **Killed at any step.** Before the commit nothing is kept, and the interface's key sends it once. After it, `resume` queues the row again with the ID jawd made. That's `send.text`'s path, which the crash tests kill as a process at each point (`answered`, `held`, `sent`, `refused`), and a card's row takes the same path.
- **What it keeps.** The row is the converter's reading of the message it sends, so a card sent here and the same card coming back from another of the owner's devices are one row. `at` is when it was sent, and `expires_at` stays empty until the server has it, as for text; that last rule has no test for cards (T3).

### Receiving

- **One card and several** come out as one `contact` object: the name its message gives (trimmed), or else its vCard's; for several, their vCards one after another with a line end between them. A one-card array reads as that card, and an empty array as "Contact". The converter's test pins each case.
- **Replays.** The converter gives the same row live and from history (the fuzz compared them for 200,000 messages). The store's upsert keeps a summary only where it isn't the text, the same rule as the backfill's guard, so a card arriving again changes nothing.
- **Untrusted vCards** are bounded in what they give and never panic, but not in what they cost (P1). A panic on the inbox's path would be worse than slowness, since the drain would retry its batch for ever (X1), so the fuzz matters; it found nothing. The backfill catches a panic and keeps the card in quarantine with its raw protobuf.
- **Edits.** An edit applies to a message of any kind (`store/write.rs:2229`), and the backfill, unlike `kinds`, doesn't keep text a later edit gave. WhatsApp's clients can't edit a card, so only a hostile client could do this, and the backfill would then show the card's names again, which is what the card says. Noted, not a finding.

### The backfill

- **Steps.** It's main's backfill driver (`links::backfill`) with `contacts::NEXT_CARDS` and `contacts::backfilled`. It looks at 64 messages a step, from where the last step left off as `backfills` keeps it, and conversions end a step once they've had a millisecond. Each step is one transaction, so a step killed halfway is rolled back and taken again. The scheduler runs it only because it's in `BACKFILLS`, which nothing tests (T2).
- **Idempotent.** The update writes only where the text, spans, summary or detail differ (`contacts.rs:31-34`), and only then gives the card a new version, its event, and its chat a new version if the card is the chat's newest. Run again from the start over a million messages, it changed nothing.
- **Quarantined, never dropped.** A card whose conversion fails or panics is rolled back to its savepoint and kept in quarantine with its raw protobuf, keyed by `(source, ref)`, so failing twice keeps it once. The rest of the step goes on. A full disk instead fails the whole step, to be tried again later, as `kinds` does; nothing tests that (T4).
- **Consistent.** In the dense run below, every card ended in the new shape, exactly one event was written per card that changed (133,334), and the search index matched the text exactly (`index_against_text`: nothing stale, nothing missing). Merges move messages with `UPDATE OF chat`, so a card keeps its `n` and the backfill still finds it. The kind never changes, so unread and mention counts don't move, and the backfill rightly doesn't unsettle them. A card's spans stay empty, so no links come or go.
- **Reported.** `contacts_backfill` in `jawd status` is `upto - after`, the messages left to look at, as the other backfills report theirs.
- **Cost.** With real cards a step costs what main's `kinds` backfill's does (Measurements); with a hostile card, see P1.

### The migration

Migration 19 is a schema step: one `INSERT … SELECT` from `sqlite_sequence`, costing the same whatever jaw.db holds. It sits after main's 18, has its lock line, and does nothing on a jaw.db without messages. Its table's class is declared (`backfills`, jaw's own). The frozen-migrations, unique-numbers, fault-at-every-statement, every-earlier-version and upgrade-from-fixtures tests pass. Whichever branch that wants 19 lands second renumbers its migration, comment and lock line, and its backfill tests' `earlier`, which builds the version just before.

### The engine

One card goes as a ContactMessage and several as a ContactsArrayMessage, each with the chat's timer in its `contextInfo` (the array's, for several). whatsmeow sends them with the media types `vcard` and `contact_array`. `unsendable` refuses, for good and before anything goes, what jawd itself never makes: a card without its vCard, or an array that's empty or has a card without one. Its test checks that no refusal is retried. Incoming cards are `events.Message`, which `kept` already hands over from a replaced client, so its table needs nothing new. The three Go tests (`TestContactCards…`, `TestAContactCardThatCantBeRight…`, `TestEveryKindJawdMustStore…`) pass in the clone.

### Plans

The five new statements are pinned: keyed reads of `messages` by `n` (a range for the next 64, one row for a card), the chats row by `n`, `sync_state` by its key, and the update by `n`. The migration's `SCAN sqlite_sequence` covers that table's few rows, as every backfill's migration does. There's no temporary b-tree, no `OFFSET` and no `ANALYZE`. `send.contact` reuses `send.text`'s and `send.poll`'s statements.

### Mutants

The builder reported 39 mutants, all killed. I didn't rest on that, and ran eleven of my own in the clone. Each switched one rule off, then I built, ran the tests that should catch it, and, if none did, ran jawd's whole suite (`cargo test -p jawd`: 639 unit tests and the integration tests). At a load of about 70, the whole suite fails now and then on main's own timing tests (the builder saw two of them). A kill that came only from such a test was checked again by running that test alone on the mutant twice; it didn't count if the test passed both times, and every one did.

| Mutant | Result |
| --- | --- |
| `send.contact` writes through an unsynced live apply | killed: `each_sending_call_that_keeps_something_is_one_synced_commit` |
| The backfill's update loses its "only where something differs" guard | killed: `cards_an_earlier_jawd_left_are_read_again…` |
| The backfill changes a card without writing its event | killed: the same |
| A card whose conversion fails isn't rolled back to its savepoint | killed: `a_card_whose_conversion_fails_or_panics_is_kept_in_quarantine…` |
| A poll clears the chat's draft again (`kind != "contact"`) | **survived** (T1): only a restart-and-receipt timing test failed, and it passed alone on the mutant |
| `BACKFILLS` without `contacts`, so the scheduler never runs it | **survived** the whole suite (T2) |
| A pending card keeps the expiry its conversion gave it (`expires_at = None` gone) | **survived** (T3): only four timing tests in feed, sock, store and sync failed, each passing alone |
| The backfill quarantines a card for a full disk instead of failing the step | **survived** (T4): only the restart-and-receipt timing test failed, passing alone |
| A blank name the message gives is taken as the card's name | **survived** the whole suite (O3) |
| A card's summary isn't clipped to 120 characters | **survived** the whole suite (O3) |
| The backfill takes a conversion that isn't a card | **survived** (O3): only a live-writes timing test failed, passing alone |

## Findings

### Data loss

None.

### Inconsistency

None.

### Unpredictable performance

**P1. An untrusted vCard can hold the writer, the inbox's conversion and the socket's tasks for seconds to minutes.** This breaks the live budgets and must be fixed.

- **Where:** `crates/jaw-proto/src/vcard.rs:184-202`. `lines()` parses the whole logical line again after each folded or soft-broken piece ending in `=`, so a card built that way is quadratic in its size. Both `read` and `is_one` go through it.
- **Measured (release):** a nameless card of 256 KiB held one `contacts` backfill step on the writer for 3,987 ms, while a live write waited 3,936 ms behind it. At 64 KiB the step took 247 ms; at 16 KiB, 30 ms, already ten times the 3 ms background budget. The reader alone takes 64 s at 1 MiB.
- **The same read happens** in the live converter (holding back every later message's conversion, again at each start until it's drained), in `send.contact`'s check and in a forward of such a card (each on one of jawd's four tokio workers), and in the terminal at each layout.
- **Why it matters:** real cards stay linear (0.4 ms at 1 MiB with a photo), so it takes a hostile or broken sender, and nothing is lost. But the design counts anything that makes the writer slow at times as a defect, and the brief asks that untrusted vCards be bounded.

Fix:

1. Make `lines()` linear: decide whether a logical line is quoted-printable once, from its first physical line's head, and look only at each appended piece's trailing `=`.
2. Give up on a head that runs past a bound (1 KiB, say) without finding its colon.
3. Let `card_name` read only as far as the end of the first card.
4. Test both shapes above at 256 KiB and 1 MiB, either as a time bound or as a ratio (twice the size, about twice the time), plus one backfill step over such a card staying inside the writer's budget.

### Test gaps

Each of these is a rule the branch touches whose mutant survived jawd's whole suite. Each is a few lines beside a test the branch already has.

- **T1. Nothing tests that a poll leaves the chat's draft.** The branch changed `send.poll`'s behaviour (`store/write/outbox.rs:696`, `kind == "text"`), but the only test of the rule is the card's. Fix: send a poll after `draft.set` and check the draft stays, next to `a_card_sent_reads_its_chat_and_leaves_what_was_typed_there…`.
- **T2. Nothing tests that jawd runs the `contacts` backfill by itself.** The branch's tests take its steps directly (`Change::Backfill(CONTACTS)`). With `contacts` taken out of `links::BACKFILLS`, the only list the scheduler (`sync/backfill.rs:27`) runs, every test passed, and an upgraded jaw.db would keep its old cards' rows with `contacts_backfill` never reaching zero. Fix: either a test that every name a migration leaves in `backfills` is in `BACKFILLS`, read from the migrations' text so a later backfill can't be forgotten either; or a test that opens a jaw.db at 18 with cards the way jawd starts and waits for `contacts_backfill` to reach zero with the cards read again.
- **T3. Nothing tests that a pending card's timer waits for it to go.** The converter gives a pending card in a disappearing chat `expires_at` from its creation (`convert/mod.rs:361-365`), and only `message.expires_at = None` (`send/contact.rs:121`) undoes that. Without that line, a card still pending past its chat's timer would be erased here before it was sent. `send.text` has this test ("its timer waits for it to go", `send/tests.rs:1123`); cards don't. Fix: in `a_card_asked_again_with_its_key_goes_once_and_cards_in_a_disappearing_chat_carry_its_timer`, check that the pending row's `expires_at` is null and that it's set once the card has gone.
- **T4. Nothing tests that a full disk fails the backfill's step rather than quarantining the card.** `contacts.rs:45` passes a transient error up so the step is tried again; without that arm, a card would be quarantined for a full disk and never read again. The `kinds` backfill has a hook for this (`FILLS`, `kinds.rs:115`); `contacts` has only `PANICS` and `FAILS`. Fix: the same hook and an assertion that the card is read again on the next step, with nothing in quarantine.

### Other (follow-ups; none blocks)

- **O1. `crates/jawd/src/convert/` and `crates/jaw-proto/src/vcard.rs` aren't in `scripts/check-data-audit.py`'s paths.** See "`convert/` and the audit's paths" above. Fix: add both to `DATA_PATHS` and to the list in `.cursor/rules/data-integrity.mdc`. It changes nothing for this branch, whose converter commit touches listed paths too.
- **O2. The protocol should say who clears a draft that a poll or card was made from.** Once an interface builds one from the box's own text, it must clear the draft itself with `draft.set`, or the text comes back.
- **O3. Three of the new conversion rules have no test:** a blank name from the message counts as none, so the card takes its vCard's name; a card's summary is clipped to 120 characters, though its name comes from whoever sent it; and the backfill takes only a conversion that is still a card (`contacts.rs:79`). Fix: a card named `"  "` and one with a 1,000-character name in `a_contact_row_is_its_name_and_vcard…`, and a `contact` row whose raw protobuf is another kind in the backfill's test.

### Outside this range

**X1. A conversion that panics wedges the inbox's drain** (main's code, `sync/drain.rs:348`). `spawn_blocking` hands back the panic, and the loop goes round to the same rows without moving on or pausing. One such message stops every later one from being converted, spins a core, and does it again at every start. The design wants a change that fails kept in quarantine, never blocking the changes after it, as the backfills do with `catch_unwind`. This branch adds a parser of others' input to that path, but the fuzz found no panic in it, so this isn't the branch's to fix. It should be fixed on main: catch a panic per row in `convert_rows` and keep that row in quarantine.

## Measurements

All in the clone at `b8db246c`, with the audit's scratch tests added there only (`store/write/contacts/tests.rs` and `crates/jaw-proto/examples/vcard_cost.rs`, none in the branch). Test databases went in a temporary folder inside the audit's folder. Each measurement ran once, on a shared Mac whose load is given.

**The vCard reader.** `cargo run --release -p jaw-proto --example vcard_cost` times `read` and `is_one` on generated cards. The table under "Untrusted vCards" gives `read`. `is_one` was within a few per cent of it on the hostile inputs, and slower on many tiny cards: 5.4 ms at 256 KiB, the most `send.contact` takes. Fuzz: 3,000,000 inputs of up to 64 random vCard tokens, checked for panics and for every output bound, in 13.6 s; no panic, slowest 4.6 ms. The converter's fuzz (debug): 200,000 contact messages, converted live and from history and compared, in 9.7 s; no failure.

**A hostile card on the writer.** Each run built a jaw.db at version 18 holding one nameless card (a parameter's quote left open, folded lines ending in `=`) and a text after it, opened it (migration 19 runs), started one backfill step, and 50 ms later asked a live write (`Change::Remember`), timing both. Load 60 to 70.

| Card | Step on the writer (release) | Live write waited (release) | Step on the writer (debug) |
| --- | --- | --- | --- |
| 16 KiB | 30.4 ms | 0.3 ms (the step was done) | 359 ms |
| 64 KiB | 247 ms | 195 ms | 5,644 ms |
| 256 KiB | 3,987 ms | 3,936 ms | — |

**The backfill on a large jaw.db (release).** Each run built a jaw.db at version 18 with 2,000 one-to-one chats and messages of 300-byte raws, and cards among them as an earlier jawd kept them: a third named, a third nameless, and a third arrays of two in the old `contacts` shape, each with its real raw protobuf, compressed. It then opened the jaw.db, drove the backfill as the scheduler does (64 steps an apply) until it was done, and set it back to the start and ran it again. "Writer" is the writer's last 1,024 transactions, commit included.

| jaw.db | Load | Backfilled | Transactions | Writer p50 / p99 | Run again |
| --- | --- | --- | --- | --- | --- |
| 1,000,000 messages, 10,000 cards | 60 to 75 | 28.2 s, 35,457 messages a second | 497 | 0.45 ms / 5.27 ms | nothing changed |
| main's `kinds`, 1,000,000 messages, 2,000 to convert, same binary | 70 to 75 | 31.7 s, 31,579 messages a second | 939 | 1.15 ms / 5.74 ms | — |
| 200,000 messages, every one a card | 58 to 66 | 21.9 s, 9,136 cards a second | 1,479 | 3.74 ms / 6.72 ms | nothing changed |

- **The p99 above 3 ms** is the machine's load: main's own backfill shows the same at the same load.
- **The dense run's p50** is the writer packing steps up to its 3 ms cap and going over by the step in hand, which is main's policy.
- **Migration 19** with the open took 256 ms at a million messages (the open's checks; the step itself is one row).
- **After the dense run:** 0 cards in the old shape, 0 nameless, 200,000 as one `contact` object, 133,334 message events (one per changed card), and the search index against the text 0 stale and 0 missing.

**Tests at the head.** The 37 targeted tests for the new rules passed, as did the upgrade-from-fixtures test, jaw-proto's 26 tests and the engine's three. Jawd's whole suite passed on every mutant that survived it, each a one-line change away from the head.

## Verdict

What must change before this lands:

- **P1:** make the vCard reader linear and bounded, with its test.
- **T1 to T4:** add the four tests.

Every finding above was measured or mutated in the clone, not taken from the builder's report. O1 to O3 and X1 may follow.

Verdict: changes needed

# Verdict of round two (`360e8bf5`)

Round one's P1 and T1 to T4 are closed, and O1 to O3 are done. Nothing marked data loss or inconsistency remains, and every rule this round touches is tested. U1, O4, O5 and X2 stand as follow-ups, and X1 is still main's.

Landing:

- **The note and the trailers.** Commit this note in the branch. Give the ten commits that touch the data layer, as `scripts/check-data-audit.py` at the head counts it, the trailer `Data-Audit: docs/audits/2026-10-29-send-contact.md`:
  - 1, `1fd3eca1`: `send.contact`, the converter's card and the reader;
  - 2, `78d9e71c`: the migration and the backfill;
  - 3, `9ec65fb0`: the engine;
  - 4, `1d298a16`: the demo engine;
  - 10 to 14, `cf6cb530`, `d8ddeca9`, `9057f44e`, `4bb733bb` and `bca940b8`: P1 and T1 to T4;
  - 17, `360e8bf5`: O3.

  Giving every commit the trailer, as the rule's `git rebase -x` does, is as good.
- **The migration's number** at landing is **20**, if nothing has taken it by then; main is at 19. If another branch takes 20 first (media in is landing now, and places and events, the library, media out and M7 want it too), this one takes the next free number, N, in:
  - its comment, `store/schema.rs:994`;
  - its lock line, `migrations.lock:35`, as `N schema 3732b60a…`, the same hash;
  - its commit's message;
  - the backfill tests' `earlier`, `store/write/contacts/tests.rs:63-71`: `MIGRATIONS[..N-1]`, `user_version` N-1, and its comment.

  T2's version 13 stays as it is.
- **If main has moved past `abfa15a4`,** rebase (this round's trial onto `abfa15a4` had no conflicts). Then at the rebased head re-run:
  - `cargo test -p jawd -p jaw-proto`: the frozen-migrations, unique-numbers, fault-at-every-statement, every-earlier-version and plan tests, the upgrade from the fixtures, and this round's eight tests. T2's also fails if a later migration leaves a backfill the scheduler doesn't run.
  - the engine's Go tests;
  - `make check-audit`.

  If what main gained touches this branch's data paths, have that part audited again. Those are the contacts backfill or its driver (`store/write/contacts.rs`, and `store/write/links.rs`'s `backfill` and `BACKFILLS`), the converter's cards (`convert/kinds.rs`), the reader (`jaw-proto/src/vcard.rs`), the outbox's draft rule (`store/write/outbox.rs:696`) and `send.contact`'s path.

Verdict: approved

# Verdict of round three (`f3d93673`)

Nothing marked data loss or inconsistency remains.

- The seven changes beyond keeping both sides are right.
- Every rule they touch passes its test where it lives, but for the cards' step bound and budget, whose values are round two's: T5, a follow-up.
- O6 to O8 are follow-ups or landing guidance.
- U1, O4, O5, X1 and X2 stand as round two left them.

Landing:

- **Order.** The library has landed (`77d44257`). Presence lands next, with no migration. Then this branch, rebased onto main as it is then.
  - My trial onto presence's landing trial (`154979a2`) rehearses that.
  - If presence lands as another tree, resolve its conflicts afresh.
- **The rebase's conflicts,** as the trials met them, each kept both ways:
  - **Commit 2,** `migrations.lock` and `store/schema.rs`, where both sides add a migration 25: the library's whole, with its own closing `",`, then the cards' as below.
  - **Commit 4,** `engine/demo.rs`: the library's document, then the cards.
  - **Commit 4,** `jaw/tests/failures.rs`: take main's file whole, so commit 4 no longer touches it. The seventh Up goes, since main's loop presses Up until it's past the oldest of the seven messages Ana's chat then holds.
  - **Commit 5,** with presence, `jaw/src/view/tests.rs`: presence's test, then the cards' three, each with its closing brace.
  - **The rest** applied by themselves at the trials.
- **The migration's number is 26,** the next free after the library's 25, unless another branch takes it first; then it's the next free, N. It goes in four places, as before:
  - the comment, `// 26, a schema step`;
  - the lock line, after the library's `25 data d3e337e2…`: `26 schema 3732b60a8d8a149874e0a84bdad22e7f9885505839e15bac4a7c020b3f2b79e4`, the hash unchanged;
  - commit 2's message, "migration 26";
  - `earlier` in `store/write/contacts/tests.rs`: `MIGRATIONS[..25]`, `user_version` 25, "A jaw.db at version 25".

  T2's 13 stays.
- **The demo's IDs.** Ana's card moves off `…0D`, which is the library's document's, to `…0F`. That's in `engine/demo.rs` and in the socket test in `main.rs` (`card("351912345678@s.whatsapp.net", "3EB0DEMO0000000F")`). The climbers' cards keep `…0E`.
  - Nothing tests that each message has an ID of its own (O7), so count them: `rg -o '3EB0DEMO[0-9A-F]{8}' crates/jawd/src/engine/demo.rs | sort | uniq -c`.
  - It should give `…01` to `…10` once each, but `…05` and `…09` three times.
- **`plans.txt`:** write it with `JAW_PLANS=write cargo test -p jawd --bin jawd plans` at commit 2's stop, never by hand. At the trial it was main's file and the cards' six entries, as the merge left it.
- **O8, the terminal's test.** Once presence is on main, `jaw/tests/contacts.rs:183` looks for `"╭ +351213456789 "`, the number at the header's start. That's a change to commit 5, beyond keeping both sides and outside the data layer; name it in the landing report.
- **Typing's argument** stays under the places landing's rule, provided presence's landed tree is as at `154979a2` in three things:
  - `sync::run` takes eight arguments;
  - `Typing::default()` spawns nothing;
  - `DemoEngine::sharing` has no typists.
- **The note and the trailers.** Commit this note in the branch.
  - The ten commits that touch the data layer, as `scripts/check-data-audit.py` counts them, take `Data-Audit: docs/audits/2026-10-29-send-contact.md`. They're commits 1 to 4, 10 to 14 and 17: `88938765`, `1bd42aa5`, `2ee0069d`, `9d191929`, `ed8635a7`, `118df4a9`, `b235386f`, `1c68f492`, `9908d184` and `f3d93673` here, with new hashes after the rebase.
  - Giving every commit the trailer is as good.
- **Re-check at the rebased head:**
  - the gate: `make check-generated`, `make lint` (clippy over the boxed card beside presence's fields), `make build` and `make test`, judging timing failures against the landers' list;
  - by name, from that run or alone:
    - `kept`'s test, seeding 25 and running 26;
    - both plan tests;
    - the frozen-text, unique-numbers, fault-at-every-statement, every-earlier-version and data-step tests;
    - T2, and the card backfill tests on `earlier` at 25;
    - the demo account's cards with `…0F`;
    - jawd's `upgrading`, version 1 through 26;
    - the terminal's `contacts`, `failures` and `composing`;
    - the engine's card tests;
  - `make check-audit`, once the trailers are on.
- **What needs another look:** anything beyond the above. And if main gains a change to the cards' data paths before this lands, that part needs another look too. Those paths are:
  - `store/write/contacts.rs`;
  - `backfill` and `BACKFILLS` in `store/write/links.rs`;
  - the triggers on `messages` that the cards' update fires;
  - the converter's cards in `convert/kinds.rs`;
  - `jaw-proto/src/vcard.rs`;
  - the outbox's draft rule;
  - `send.contact`'s path.

  The library's changes, read above, need none. Presence's, at its landing trial, touch none of them.
- **T5** can follow on main, or go in this branch as a test of its own with an audit round for it.

Verdict: approved
