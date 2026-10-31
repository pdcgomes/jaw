# Audit: M2's places and events (`m2-more`)

# Round three: `c50a2ba3..aa848fc6`

Range: `c50a2ba3..aa848fc6` in `m2-more` (worktree `~/src/jaw-m2more`), 30 October 2026. Auditor: database-expert. It's 18 commits on `c50a2ba3`: round two's 13, rebased from `5230b9f` with no conflict, and five that answer round two:
- `3887db83`: I3's fix, with P22 adopted and the new statement's plan pinned;
- `5fc20cbf`: T6, with P24 adopted;
- `994620e6`: T7, with P14 adopted;
- `77dd7bbf`: T8, with P23 adopted, and sync's `backfill` module opened to the crate;
- `aa848fc6`: round one's M10 caught, with P16 adopted and a harness that restarts from a copy of jaw.db after every commit.

Outside tests, the five change only `SEALED_BETWEEN` and its comments (15 lines of `store/write/events.rs`), its pinned plan (`store/plans.txt`) and the one line that opens `sync::backfill`. Line numbers are at `aa848fc6`, and paths are under `crates/jawd/src/` unless they say otherwise. The migrations are still 20 to 23 on this base. Main is at `8e0a63d0` now, 11 commits past `c50a2ba3`, all under `mac/` and `docs/plans/`. Media in (`42a31eaf`, migration 20) hasn't landed; this round's trial puts the range on top of it.

## What I did

Everything ran in one clone (`/tmp/jaw-audits/m2-places-events-3/jaw`), with one build folder and `CARGO_INCREMENTAL=0`, under `nice`. Revisions were switched there by `git checkout`, and each binary was copied out before the next build. Every test binary ran in a process group of its own, which a timeout kills whole, with `TMPDIR` inside the audit's folder. The trial's conflicts were resolved in a git worktree of the clone, in the same folder and never built.
- **Reading:** the five new commits whole, the rebase against round two's head, and the code around `known_again`: `set_aside` and its names, `take` and its set-asides, `left_to_take` and the note in `unopened`, `upsert_person` and `join`, and a merge's `follow`.
- **The head's tests** (`aa848fc6` with my probes, no switches):
  - **jawd's unit tests:** 731 passed and 2 failed, as in the builder's gate. The two are main's timing tests of the feed's long-request lane and the settings socket, code this range doesn't touch, and each passed alone 3 runs of 3. The plan test and `kept`'s test pass.
  - **The upgrade from version 1** (`tests/upgrading.rs`) passes.
- **Switches:** round two's, put back at run time by `JAW_AUDIT_MUT`, and four new ones on I3's clause. Each ran against the 200 data-layer tests around the range and against my probes.
- **Probes:** round two's 13, as written then (P3, which round one wrote to fail on O1, left out; P10 over 600 messages), and three new ones on I3: P31, P32 and P34. All ran at the head and on the trial.
  - P31 and P32 are scenes: jaw.db is copied after every commit.
  - From each copy, as jawd would find it after a kill there, a new store takes the rest of the scene and must end where the run that wasn't stopped did.
- **The landing:** a trial onto media in's head, its migrations renumbered 21 to 24, built and tested, and the upgrade from a jaw.db media in's jawd made, killed at a few points, both jawds on the demo engine.
- **Measurements:** what a person new to jaw costs with the clause and without it, in the same binary.

The machine was loaded by other agents throughout, at 45 to 65 on 20 cores. No Go engine was built or run, nothing was sent to WhatsApp, nothing under `~/Library/Application Support/jaw` was opened, and every process I started was stopped by its own PID.

## In short

Approved. I3 is closed, and so are T6 to T8; P16 now catches round one's M10. Nothing in the branch is lost or inconsistent. Landing it on media in needs three things beyond renumbering, each tested on the trial (L1 to L3), and the clause makes round two's U3 costlier (a follow-up).

- **I3, closed:**
  - With the new clause off, P22's adopted test fails, and no other of the builder's 200 does.
  - So do my P22, P31 (killed after every commit, and in both orders, which end the same) and P34 (an answer from a JID the join takes away, for two events at once).
  - The plan is pinned: two covering range searches for each of the person's JIDs, no scan or sort.
- **T6 to T8, closed, and M10 caught:** each adopted test fails with its own rule off, alone among the 200.
- **The rebase** changed nothing but context.
- **The trial on media in:**
  - **The rebase:** 7 of the 18 commits conflict, in 12 files. Every conflict resolves by keeping both sides, and the migrations become 21 to 24 with their lock hashes unchanged.
  - **The tests:** 815 of the trial's unit tests pass, all but `kept`'s and the plan test, which pass with L1 and L3. So do the upgrade from version 1 and my probes.
  - **The upgrade:** from a jaw.db media in's jawd made, killed at 0.2 s, 1 s or 3 s, or twice, every run ends the same.
- **L1, test gap at landing:** `kept`'s test fails on the landed tree. Nothing seeds media's tables, and media's seed row breaks migration 20's `CHECK`s, which no test met while 20 was the last migration. The patch below fixes it.
- **L2, other at landing:** the demo account gives media's photo and this branch's pin one ID. On the trial the pin takes the photo's row and keeps its file. Give the pin `3EB0DEMO0000000C`.
- **L3, other at landing:** `plans.txt` must be written again. Its new plans are exactly both sides' foreign-key checks together.
- **U3 grows (unpredictable performance, a follow-up):**
  - A person new to jaw cost 8.5 to 9.0 ms with 10,000 events noted. With the clause it's 18.7 ms, and 30 to 31 ms with 10,000 answers set aside.
  - Noted events are a handful in practice, so no budget breaks at the sizes jaw sees. Round two's fix for U3 takes the clause off the live path with the rest of the search.
- **Follow-ups:** U3 to U5 and H1 to H6 stand, as do round one's O1, O2, O4, O5 and U2. H7 is media in's.

## What was asked, checked

### The rebase onto `c50a2ba3`

- **Patches:** `git range-diff 5230b9f4..c63ee786 c50a2ba3..42a658b0` pairs round two's 13 commits with the rebased ones, every one `=`.
- **Trees:** the rebased head differs from round two's head by exactly what main changed underneath. `git diff 5230b9f4 c50a2ba3` and `git diff c63ee786 42a658b0` are the same 6,208 lines but for two hunk headers' line numbers in `sync/tests.rs` and the blob IDs of three files both sides touch.
- **Main underneath:** `5230b9f4..c50a2ba3` is the keepalive fix and the Mac harness. It touches `sync/mod.rs`, `sync/tests.rs` and `send/tests.rs` beside this range, as round two's trial onto `abfa15a4` found, but nothing it changes or calls.

### I3: a set-aside is tried again when its answerer gains a JID

- **The fix** (`store/write/events.rs:103`): `SEALED_BETWEEN` gains a third `OR EXISTS`, a row of quarantine with source `answer` or `edit` named from `<chat> <event> <jid> ` up to `<chat> <event> <jid>!`, for each of the person's JIDs.
- **The range is exact:**
  - Every name `set_aside` gives starts that way (`:538`): its chat, its event, its sender, then its own ID or a hash, with a second name's hash or a merge's ` <n>` only after them.
  - `pending.sender` and `person_jids.jid` are both written through `user_jid` (`store/write.rs:2376`, `:1378`), so a device's JID can't hide one.
  - A space sorts just before `!`, so under BINARY collation the range holds exactly the names with that prefix. The space after the JID cuts off another JID or event ID that merely begins with this one.
  - Only `set_aside` writes quarantine under `answer` and `edit`; failed changes are kept as `amendment`, and media in's copies as `copies`, in the trial too.
- **The plan is pinned** (`store/plans.txt:1980`), and the plan test passes at the head. For each event noted, the new subquery reads the person's JIDs by `person_jids_by_person`, then searches `quarantine_by_ref`, a covering index, by `(source, ref)` range for each: two seeks a JID, one for each source. Nothing new scans or sorts; the one scan is `unopened`'s, allowed since round two (U3).
- **Nothing else needs it:**
  - The author's side was whole already: a join of the event's author starts the event again from the first by the clause on `m.sender`, and that step brings back everything set aside for it.
  - An edit opens only from its author, so its set-asides are the author's side too. With edits left out of the new clause (I3d below), every test passes, as it should.
- **The note an event keeps in `unopened` outlives its set-asides:**
  - `set_aside` doesn't touch it.
  - A try clears it only at `Left::Done` (`:353`), which `take` reaches once `SET_ASIDE_FOR` finds nothing left (`:415`), after every set-aside of the event has come back to `pending`. So `STILL_SEALED`, which looks only in `pending`, is right where it's asked.
- **A merge under way is the one place the range misses, and the merge makes up for it** (P32):
  - Starting a merge renames the set-asides to the chat kept at once (`follow`, `store/write.rs:1650`). The event moves with a later step, so in between the range, built from the event's chat as it is, finds nothing.
  - The event's move retakes it from the first (`apply_pending`, then `events::arrived`), which brings the set-asides back under their new name.
  - P32 passes, killed after each of its commits. After the join nothing is left to take, since the range finds nothing, so it's the merge's step moving the event that opens the answer.

### The adopted tests, and the rules switched off

Each switch turned off one rule, at run time, in one build of the head with my probes. Each ran against the 200 data-layer tests around the range (events, again, polls, links, kinds, rules, schema, merges, sync's test of what waits for an event, send's events and places, convert), and against my probes. The schema's two slowest tests, which run every migration again and which no switch touches, ran unswitched only. With no switch, all 202 and my 15 probes pass.

| Switch | Rule turned off | The builder's tests that fail | My probes that fail |
| --- | --- | --- | --- |
| I3 | set-asides found by their answerer's JIDs (round two's I3) | P22's adopted test, alone | P22, P31, P34 |
| I3a | that, and what waits in `pending` from their JIDs | P22's, and the I1 test | P22, P31, P34 |
| I3b | set-asides found by every JID of the person, not only the first | P22's, alone | P22, P31, P34 |
| I3d | edits among the set-asides found | none, as it should be: an edit opens only from the event's author, whose clause finds its event | none |
| D1b | a row leaves `pending` only once quarantine keeps it (T6) | P24's adopted test, alone | P24 |
| U1f | a step goes on from where the last got to (T7) | P14's, alone | P14 |
| U1j | a join's retries wake the background (T8) | P23's, alone | P23 |
| M10 | what's reached for an event deleted for everyone goes (round one's M10) | P16's, alone | P16 |
| I1b | the events a person sent are tried again | the I1 test, and P23's, whose join is the author's | P2, P15, P23 |
| I1c | the events their answers in `pending` came for are tried again | the I1 test, alone | P31, whose other order joins first |

- **Each adopted test fails with its own rule off, and no other test of the builder's does:** P22's for I3 and I3b, P24's for D1b, P14's for U1f, P23's for U1j and P16's for M10. The builder's own switches said the same of 39 tests; these are 200.
- **P32 passes under every switch.** It shows a merge opening the answer, not the clause (above).
- **The builder's five tests are my probes' scenes,** read side by side. Each checks what mine did and a little more: P22's that nothing stays set aside or noted, P24's that quarantine holds only the two planted rows, whose names it reads through the product's own `OLDEST`.

### Landing on media in: the trial

I cherry-picked `c50a2ba3..aa848fc6` onto media in's head, `42a31eaf` (on `abfa15a4`, so without main's Mac commits, which touch nothing here), resolving by hand. Seven of the 18 commits conflict, in 12 files. The other 11 apply as they are: `git range-diff` shows 10 of them `=`, and commit 8 (`0184c9cc`) `!` only for where a test sits in `store/rules.rs`. What each conflict is, and how I resolved it:

| Commit | Files | Resolution |
| --- | --- | --- |
| `98dd9061` (test deadlines) | `store/mod.rs` | both: media's two test functions, then `STEPS_WITHIN` |
| `eb2e2bb3` (places) | `store/schema.rs`, `migrations.lock` | media's migration 20 whole, then places' as **21**, its text unchanged, lock line `21 schema 121c1e12…08b6` |
| | `store/write/links.rs` | media gave each backfill its own step size, a fourth field; places takes `STEP`, after media's `COPIES` in `BACKFILLS` |
| | `store/write.rs`, `plans.txt`, `jaw-proto/tests/fixtures.rs`, `protocol/README.md` | both sides' lines: `mod again` beside `pub mod copies`; both sides' plans; `send.location` with `media.save`; both clauses in the `status` row |
| `1d1c53fc` (events) | `migrations.lock`, `store/schema.rs` | events as **22** (`e37a90af…c779d`); `schema.rs` merges by itself, its comment renumbered |
| | `store/write.rs` | the fold: events' answers folded, then media's `media::fold`, which replaces the two `media` statements main had there; `Amendment`: media's `Hd` and events' `Answer` and `EventEdit` (`hd`, `answer`, `event_edit`, which don't meet); `apply_amendment`: both arms |
| | `store/write/links.rs` | `EVENTS` as `PLACES` is, after it |
| | `convert/mod.rs`, `engine/demo.rs`, `scripts/check-data-audit.py`, `plans.txt`, fixtures, README | both sides' imports, paths, plans and lines |
| `0023eaa8` (test steps bounded) | `store/mod.rs` | media's two test functions, then `Steps` with its new comment |
| `8b7fa887` (edits bounded) | README | both clauses |
| `ba585291` (steps, migration 22) | `migrations.lock`, `plans.txt`, README | as **23** (`0896c5ee…d6ed`), with `schema.rs`'s comment renumbered; both sides' plans and clauses |
| | `sync/backfill.rs` | only the module's comment: media's sentence, then events' |
| `f12f0069` (unopened, migration 23) | `migrations.lock`, `plans.txt`, README | as **24** (`5aca4a61…01e0`), likewise |

- **Two resolutions carry weight.** The fold must not keep main's two old `media` statements beside `media::fold`: media in's `media` has no `path` any more, so the statement wouldn't prepare, and the plan test and media's fold tests would fail. And `links.rs`'s `STEP` for places and events keeps their steps at 64 messages, as approved, where media's copies take 16.
- **Nothing else names a number.** The migrations' texts don't name theirs, the tests find the version before by a migration's text (`position(|m| m.contains("SELECT 'places'"))`), and every "migration 20" left in the tree is media's. `kept.rs` merged by itself, with both sides' tables and classes.
- **The trial's tests** (the resolved tree with my probes):
  - **jawd's unit tests:** 815 passed and 2 failed: `kept`'s test (L1) and the plan test (L3). Media's tests are among those that pass, and so are the events', the merges' and the demo account's.
  - **The fixes:** with L1's patch and `plans.txt` written again, all 17 schema tests and both plan tests pass, among them `kept`'s, the frozen texts, the fingerprints, a fault at every statement of every migration, and every earlier version keeping its rows. With L2's, all ten tests of the demo account pass.
  - **The rest:** the upgrade from version 1 to 24 passes, and so do my 16 probes, all of round two's that pass at the head and P31, P32 and P34.
- **The upgrade from media in's jaw.db, killed:**
  - **The jaw.db:** media in's jawd (`42a31eaf`), on the demo engine, made one at version 20 in a folder of the audit's: 54 tables and media's demo account, a photo among it with its copy and its file in the cache (142,342 bytes). It ran 25 s and stopped cleanly.
  - **The runs:** the trial's jawd, on the demo engine, took copies of it: straight through; killed (SIGKILL) 0.2 s, 1 s or 3 s into its first start, then started again; or killed at 0.5 s, then at 2 s, then started a third time.
  - **What each left,** every run the same: version 24 and 57 tables, `integrity_check` ok and `foreign_key_check` empty; the photo's copy, its file and the cache's count as they were; the demo's event with its two answers, and its pin; nothing in `pending`, quarantine, `openings` or `unopened`, and every backfill done (copies, places and events).
  - **Before L2,** the same five runs ended the same as each other too, but with no photo: the pin had its row, and with it the photo's copy (L2).

## Findings

### Data loss

None.

### Inconsistency

None in the branch. I3 is closed (above).

### Test gaps

- **L1. On the landed tree, `kept`'s test fails: nothing seeds media's tables, and media's seed row breaks migration 20's `CHECK`s.**
  - **Where:** `store/schema/kept.rs`, its `SEEDS` and `seed()`, as media in left them.
  - **Why no test met it:** the test seeds the version before each migration and checks that the migration keeps the rows. On media in alone, 20 is the last migration, so version 20 is never seeded.
  - **What happens on the trial:** the test seeds versions 20 to 23 too.
    - `media`'s seed row, a 1-byte `sha256` and state `here`, can't be written to migration 20's `STRICT` table, which wants 32 bytes and one of its five states: "a row of media at version 20 wasn't written".
    - Nothing seeds `media_files`, `media_made` or `media_drop`, which are jaw's own.
    - `media_made`'s one row is written by migration 20 itself, so a plain `INSERT` can't seed it.
  - **The fix,** tested on the trial (`kept-seeds.patch` in the store), as below:
    - `media`'s seed takes a 32-byte `sha256` and state `expired`, valid in both tables and kept by migration 20's reset;
    - `media_files`, `media_made` and `media_drop` get a row each;
    - `seed()` upserts, so a row a migration wrote itself takes the seed's values.

    With it, `kept`'s test passes, as do the other 16 schema tests and the plan tests. Put it in the places commit, the first whose migration follows media's, so that every commit passes.

```diff
@@ const SEEDS: &[(&str, &[(&str, &str)])] = &[ (media's row)
-            ("sha256", "x'01'"),
+            ("sha256", "x'0101010101010101010101010101010101010101010101010101010101010101'"),
@@
-            ("state", "'here'"),
+            ("state", "'expired'"),
@@ (at the end of SEEDS)
+    (
+        "media_files",
+        &[
+            ("sha256", "x'0101010101010101010101010101010101010101010101010101010101010101'"),
+            ("path", "'media/files/01.jpg'"),
+            ("bytes", "48000"),
+            ("small", "'media/files/01.small.jpg'"),
+            ("large", "'media/files/01.large.jpg'"),
+            ("previews_bytes", "9000"),
+            ("thumbnail", "'media/files/01.thumb.jpg'"),
+            ("thumbnail_bytes", "800"),
+            ("used_at", "1790000001010"),
+            ("made", "4"),
+        ],
+    ),
+    ("media_made", &[("id", "1"), ("made", "5"), ("settled", "3")]),
+    ("media_drop", &[("sha256", "x'0202020202020202020202020202020202020202020202020202020202020202'")]),
@@ fn seed(db: &Connection, version: usize) {
-        let sql = format!("INSERT INTO \"{table}\" ({}) VALUES ({})", names.join(", "), values.join(", "));
+        let set: Vec<String> = names.iter().map(|name| format!("{name} = excluded.{name}")).collect();
+        let sql = format!(
+            "INSERT INTO \"{table}\" ({}) VALUES ({}) ON CONFLICT DO UPDATE SET {}",
+            names.join(", "),
+            values.join(", "),
+            set.join(", ")
+        );
```

### Unpredictable performance

- **U3 grows with I3's clause.** Round two's U3, a person known by a JID new to jaw reading every row of `unopened` in the transaction that learns the JID, now also searches quarantine for each row: two seeks for each of the person's JIDs.
  - **The cost** (under "Measurements"): with 10,000 events noted, a person new to jaw cost 9.0 ms without the clause and 18.7 ms with it. With 10,000 answers set aside as well, it's 8.6 ms and 30 to 31 ms.
  - **The live budget:** on a straight line through these numbers, the count of events noted past which that transaction is over its 5 ms falls from about 5,500 to about 2,700, or to about 1,600 with 10,000 answers set aside.
  - **At the sizes jaw sees** it's within budget: 0.46 ms with 100 noted, and 2.3 ms with 1,000. Events whose answers stay sealed are rare, and plans.rs says "a handful".
  - **The fix is U3's:** note the person and leave the search to a background step, as the retries already are, which takes this clause off the live path with the rest. Or drive the search from the person's JIDs with an index.

### Other

- **L2. The demo account gives media's photo and this branch's pin one ID, `3EB0DEMO00000008`, both Ana's in her chat.**
  - **Why:** each branch took the next free ID, and git merges the two lists without a conflict.
  - **What happens on the trial:** the pin comes second and takes the photo's row. jaw.db then has a location holding the photo's copy and file, and no photo. In the upgrade, a photo media in's jawd kept became the pin.
  - **Why no test met it:** the pin's test finds it by that ID (`main.rs:1021`), and none reads the demo's photo; media's tests build their own made-up worlds.
  - **The fix,** tested on the trial (`demo-id.patch`): the pin takes `3EB0DEMO0000000C` in `engine/demo.rs` and in its test. All ten demo tests pass, and the upgrade keeps the photo, the pin and the event apart.
  - A test that the demo's messages each have an ID of their own would keep the next merge from doing this again. Typing (M6) adds no demo ID.
  - It's the demo's made-up account, never the owner's data, so it's not an inconsistency of jawd's. But it has to be fixed at landing.
- **L3. `plans.txt` must be written again at landing.** `JAW_PLANS=write` gives a file that differs from both sides' entries together only as it should:
  - its entries follow the files' order;
  - the two old `media` statements of main's fold are gone, since `media::fold` replaced them;
  - the plans of `DELETE FROM messages WHERE n = ?1` and of the `INSERT INTO messages` now list both sides' foreign-key checks, exactly their union, with no scan or sort.

  Check that its diff says only that.
- **H7. `kept.rs` doesn't know `media`'s columns of jaw's own from migration 20 on** (media in's, a follow-up).
  - `CLASSES` lists them as before 20: `path`, the previews, `state` and `progress`.
  - Migration 20 added `wanted`, `asked`, `arrived`, `retried_at`, `cancelled`, `error` and `direct_path`, which its `CREATE` calls jaw's own. From 21 on, a migration dropping one passes `kept`'s test.
  - The fix: declare them, and give them values in the seed.
- **Outside this range:** a message arriving again under its key as another kind keeps the old kind's file copy. That's how L2 shows. Only colliding IDs reach it, and WhatsApp's are unique per sender.
- **Round two's U3 to U5 and H1 to H6 stand, as do round one's O1, O2, O4, O5 and U2.**

## Measurements

Method: `cargo test` (the dev profile, whose dependencies, SQLite among them, build at `opt-level = 2`) of the head with my probes and switches, on this Mac, at a load of 48 to 64 from other agents' work, on 20 cores. Every number is from a store in a fresh folder.

- **A person new to jaw, with events noted and answers set aside** (`audit3_measure_a_person_new_to_jaw_with_events_noted_and_answers_set_aside`):
  - **The store:** 10,000 events noted in `unopened`, each keeping an answer sealed for Ana's LID, and 10,000 answers set aside in quarantine for them by other senders, under the names `set_aside` gives.
  - **The measure:** 30 people, each new to jaw by a number and a LID, each a live transaction, the first 5 left out.
  - **The runs:** with the clause and without it (`JAW_AUDIT_MUT=I3`), the same binary, alternated three times.

  | Run | With the clause (p50, worst of 25) | Without it |
  | --- | --- | --- |
  | 1 | 31.2 ms, 35.4 ms | 8.50 ms, 10.4 ms |
  | 2 | 30.1 ms, 33.2 ms | 8.57 ms, 9.05 ms |
  | 3 | 31.4 ms, 34.2 ms | 8.72 ms, 15.1 ms |

- **The same with nothing set aside** (round two's `audit2_measure_a_person_known_by_a_new_jid_with_events_unopened`, one run each):

  | Noted | With the clause (p50, worst) | Without it | Ana's join with the clause, then the retries |
  | --- | --- | --- | --- |
  | 0 | 0.26 ms, 0.81 ms | 0.36 ms, 2.11 ms | 0.32 ms, none |
  | 100 | 0.46 ms, 0.73 ms | 0.32 ms, 0.57 ms | 1.29 ms, then 38 ms in 6 transactions |
  | 1,000 | 2.30 ms, 6.77 ms | 1.31 ms, 9.83 ms | 1.79 ms, then 233 ms in 50 |
  | 10,000 | 18.7 ms, 23.1 ms | 8.95 ms, 16.4 ms | 15.9 ms, then 1.89 s in 471 |

  - **Against round two:** without the clause, 8.95 ms at 10,000 is close to round two's 7.5 ms on a release build. The statement's own work is SQLite's, at `-O2` in both builds, so the clause's share is the same in either.
  - **The join:** Ana's join finds her events by the author's clause, which comes first, so the new clause costs it little.
- **Kills:**
  - P31: the bound, the join and the steps took 4 commits in one order and 3 in the other, each restarted from its copy. Both orders end the same.
  - P32: the join and the merge took 2 commits in each of its two runs.
  - The upgrade's five runs each took about 25 s, 20 s of it the jawd left running after its last start.
- **The switches:** each ran the 200 tests in 100 to 140 s, and my probes in 120 to 690 s, the longest while the trial built and ran its tests beside it.

## The probes

They're kept with the switches, the scripts, the trial and the logs in this auditor's store: `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/m2-places-events-round3/`.
- **`probes.patch`** applies to `aa848fc6`:
  - It puts round two's probes in `store/write/events/tests/audit2/tests.rs`, with its harness opened to its sibling, and round three's in `…/audit3/tests.rs`.
  - Both files are named `tests.rs`, so the plan test leaves them out.
- **`switches.diff`** applies on top, and `scripts/switch3.py` makes it.
- **`trial.bundle`** is the trial: `42a31eaf..trial-probes`, the 18 commits as resolved, my probes, and L1, L3 and L2 as commits of their own.
- **`kept-seeds.patch` and `demo-id.patch`** are L1's and L2's fixes, on the trial's tree.
- **P31** (I3 through kills, in both orders): Bea's answer kept sealed for her LID, `pending` filled, then the change that sets it aside and the join that teaches jaw her LID, in either order, with a copy after every commit. It passes at the head and on the trial, and fails with I3, I3a, I3b or I1c.
- **P32** (I3 through a merge): your event in Ana's LID chat, her answer by her LID sealed for her number, set aside; then the join that merges her chats. It passes under every switch: it's the merge that opens it.
- **P34** (a JID the join takes away, two events): Bea known by her number, and as someone else by her second number; her answers to e1 and e2 come by the second, sealed for her LID, and are set aside; one person change names all three. It fails with I3, I3a or I3b.

# Round two: `5230b9f..c63ee786`

Range: `5230b9f..c63ee786` in `m2-more` (worktree `~/src/jaw-m2more`), 30 October 2026. Auditor: database-expert. It's 13 commits: round one's five, rebased onto `folders` (places' migration now 20, events' 21), and eight that answer round one (T1, D1, I2, O3, U1 with migration 22, I1 with migration 23, T2 to T5, O6). Line numbers are at `c63ee786`, and paths are under `crates/jawd/src/` unless they say otherwise.

## What I did

Everything ran in one clone (`/tmp/jaw-audits/m2-places-events-2/jaw`), with one build folder, `CARGO_INCREMENTAL=0`, under `nice`. Every test binary ran in a process group of its own, which a timeout kills whole, with `TMPDIR` inside the audit's folder.
- **Reading:** the eight new commits whole, and the code around them: `events::take` and everything that calls it (an event's arrival, a merge moving or folding it, the events backfill, the steps), `set_aside` and `bounded`, `known_again` and `upsert_person`, the writer's new notify, and sync's backfill task.
- **The head's tests:** the data-layer tests at `c63ee786` (events, places, polls, links, kinds, convert, sync, send, plans, schema, rules and main's store tests), 289 passed and 3 failed. The three are main's timing tests, which this range doesn't touch: two of the writer's live-write tests and sync's engine-stop test. Run alone, the two writer tests passed 3 runs of 3; the sync test failed under load in round one too.
- **Switches:** round one's 12 again, and 24 new ones, each turning off one rule this round adds (listed under "The switches"), switched at run time by `JAW_AUDIT_MUT`. I ran each against the builder's 192 tests around the range, and the four that passed them against my probes.
- **Probes:** round one's P1, P2, P3, P7 and P10, ported to the steps and run as written then; and P11 to P16 and P22 to P24, new. Six of them (P10 to P13, P15, P16) are scenes: jaw.db is snapshotted after every commit, and from each snapshot, as jawd would find it after a kill there, a new store takes the rest of the scene and must end where the uninterrupted run did.
- **The rebase:** a trial rebase onto `abfa15a4`, built and tested; and the upgrade from a jaw.db main's jawd made, killed at a few points, with both binaries on the demo engine.
- **Measurements:** on a release build of the rebased tree, at the load other agents left.

The machine was loaded throughout, at 44 to 88 on 20 cores. No Go engine was built or run, nothing was sent to WhatsApp, nothing under `~/Library/Application Support/jaw` was opened, and every process I started was stopped by its own PID.

## In short

Changes needed, but small. D1 is closed: nothing set aside is lost, P1 included, and nothing set aside is named by a row key that's given again. Only the record of a row whose taking failed still is (H1), and that row stays where it was. I2, O3, T1 to T5 and O6 are closed. U1 holds up through kills, merges, joins and replays. I1 is closed for everything kept in `pending`, but not for an answer that was set aside past the bound before its answerer's JID was learned. Three rules this round adds have no test that fails without them.

- **I3, inconsistency (rare):** the retry when someone gains a JID looks for their sealed answers only in `pending`. An answer kept sealed for an event jaw has, then set aside past the 10,000, isn't tried again when jaw learns its answerer's JID. It stays sealed in quarantine, so whether it counts depends on whether the overflow or the join came first (P22). The event's author side works, since every set-aside comes back once its event is tried again. Reaching it needs `pending` past its bound.
- **T6 to T8, tests missing:** every test of the builder's passes with each of these rules off, and a probe of mine fails:
  - **T6:** a row leaves `pending` only once quarantine keeps it (P24).
  - **T7:** a step goes on from where the last got to (P14).
  - **T8:** a join's retries wake the background (P23).
- **What holds:**
  - **Kills:** killed after any commit of an event's steps, a merge or a join mid-way, or replays, every restart ends where the uninterrupted run did: P10 to P13, P15 and P16, 122 restarts in the run after the rebase.
  - **Switches:** the builder's tests now catch 32 of my 36, 11 of round one's 12 among them, and my probes catch the other four.
  - **Rebase:** the trial rebase onto `abfa15a4` replays all 13 patches unchanged, builds, and passes the 237 data-layer tests and the upgrade from version 1.
  - **Upgrade:** a jaw.db main's jawd made upgrades to version 23 identically whether it runs straight through or is killed at 0.2 s, 1 s, 3 s, or twice.
- **Follow-ups:** U3 to U5 and H1 to H6, none blocking; round one's O1, O2, O4, O5 and U2 stand.

## What was asked, checked

### D1, with P1 and a name already held

- **The rule** (`store/write/events.rs:519`): `set_aside` names a row by its event, its sender and its own message ID. The ID comes from what an opened answer does, or from a sealed one's raw protobuf (`kinds::own_id`). A row with neither is named by a hash of what it holds. If another row holds the name already, the hash goes after it. A row leaves `pending` (`:551`) only once quarantine holds it, or a copy of it the same in every field, under one of its names.
- **P1, round one's, as written then:** both bulk fills of 9,999 answers are opened for real through the steps, and `pending` drains through the store. Rui's second answer waits as `pending` n 1, as his first did. Both are kept, as `… r1` and `… r2`, and his newest counts. Passes.
- **A name already held:**
  - The builder's test gives Rui's ID again with other content, which is kept under its hashed name. The same answer again is kept once.
  - P24 plants other rows under both of an answer's names. The answer stays in `pending`.
  - Both pass. With rows named by their n and deleted whatever the insert did (D1a), four of the builder's tests fail. With a held name taking no second name (D1c), or a copy the same in every field not taken as kept (D1d), the name test fails, and each leaves the row in `pending`, so nothing is lost.
  - With the row deleted whatever the inserts did, but names as now (D1b), every test of the builder's passes, and P24 fails, losing the answer: T6. Only a 64-bit hash collision or a planted row reaches that case, but it's the rule the fix rests on.
- **The bound:** each change sets aside 16 at most (`:70`). A row whose names both stay taken stays in `pending`, where the next change tries it again.

### Row keys others hold

- **`pending.n`** (no `AUTOINCREMENT`, given again once `pending` empties) is no longer the name of anything set aside for an event. Two things still hold it:
  - **`openings.n`,** where an event's steps go on from. It's a place in the `(at, n)` order, not a name. A row that lands behind it is one kept sealed or one whose taking failed, each tried again only from the first, by a join or the next time the event comes. Or it's one a merge moved there, and the merge starts the event again from the first (`apply_pending` takes it through `events::arrived`). P11 to P15 hold this through kills.
  - **The quarantine row of a row whose taking failed** (`:449`, named `<chat> <event> <n>`) is a record: the row stays in `pending`. See H1.
- **`quarantine.n`:** the suffix a merge's rename gives a set-aside onto a name taken (`… <n>`) is held only while that row is. A row with the same n can come only once it's gone.
- **`person_jids.n`,** which `event_answers.who` holds, is never deleted. **`messages.n`,** which `openings` and `unopened` hold, is `AUTOINCREMENT`, and both cascade with their event.
- **Outside the range:** main's set-aside votes are still named by `pending.n` (m2-content's H3).

### I1, I2, U1 and O3, each switched off

- **I1** (`known_again`, `:315`, from `upsert_person`, `store/write.rs:1422`): six switches each fail the builder's test: no retry on a join, the person's own events left out, the events their answers came for left out, and no note in either of the two places, or the note never cleared. P2 passes, round one's as written. So do P15, a join while the event's steps are under way (it starts them again from the first), and P23 below. The gap is I3.
- **I2** (`fold`, `store/write.rs:1927`): with the guard off text, spans and summary, or off raw and envelope, both the event's test and the text message's (`store::rules`) fail. With round one's M19, the guard off detail, the event's test fails.
- **U1:**
  - **Caught by the builder's tests:**
    - every answer taken at once (U1a);
    - those set aside left there (U1b);
    - no cap on what one change sets aside (U1c);
    - no savepoint around a row (U1d);
    - the writer never telling the background (U1e);
    - what comes back left in quarantine too (U1g);
    - an event's opening never ended (U1i).
  - **Not caught by them:**
    - **Each step from the first again (U1f):** every test of the builder's passes. An event with more than a step's worth of what doesn't open ahead of what does then never finishes: each step takes the same 64 again, for as long as jawd runs. P14 catches it: T7.
    - **A join's retries not waking the background (U1j):** every test of the builder's passes. What the join leaves waits until another commit wakes the background or jawd starts again. P23 catches it: T8.
- **O3:** edits left unbounded, left out of what a merge moves, or left out of what comes back with the event: each fails the builder's edit tests.
- **Merges, joins and replays mid-way,** each killed after every commit:
  - **P12:** a merge that moves an event, or folds its copy into the one kept, while its steps are under way. Every answer counts once, in the chat kept, and nothing is left behind.
  - **P13:** answers arriving again, and a newer answer of Rui's, while the steps are under way. His older answer, taken after by the steps, changes nothing.
  - **P16:** an event deleted for everyone while its steps are under way. What was left for it goes. With round one's M10, which keeps it, P16 fails.

### The new tables, and the migrations

- **Classes:** `openings` and `unopened` are declared jaw's own in main's `kept.rs`, each with a seed row, and main's schema-rule test passes at the head and after the rebase. That's right for both:
  - Dropping `openings` would leave answers waiting for steps that never come.
  - Dropping `unopened` would leave sealed answers no join would ever try again.
  - Both hang on `messages (n)` with `ON DELETE CASCADE`, as `event_answers` does, so they go with their event.
- **Migrations 22 and 23** are schema steps, each one `CREATE TABLE` of an empty `STRICT` table. Their lock lines are `22 schema 0896c5ee…d6ed` and `23 schema 5aca4a61…01e0`. Neither text names its number, so renumbering moves only the lock lines and comments. Main's frozen-migrations, fault-at-every-statement and fingerprint tests pass, and so does the upgrade from version 1 (`tests/upgrading.rs`), at the head and after the rebase.
- **The upgrade from main's newest jaw.db, killed:**
  - **The jaw.db:** main's jawd (`abfa15a4`), on the demo engine, made one at version 19 in a folder of the audit's: 50 tables and main's demo account, ran 25 s and stopped cleanly.
  - **The runs:** the rebased branch's jawd (`f753f5c9`), on the demo engine, took copies of it:
    - straight through;
    - killed (SIGKILL) 0.2 s, 1 s or 3 s into its first start, then started again;
    - killed at 0.5 s, then at 2 s, then started a third time.
  - **What each left,** every run the same:
    - version 23 and 53 tables, with `integrity_check` ok and `foreign_key_check` empty;
    - the demo's event and pin in, with the event's two answers;
    - nothing in `pending`, quarantine, `openings` or `unopened`, and every backfill done.
  - main's demo account is small (a poll and four texts), so the kills test the migrations and the start, and P10 and P11 test the backfills and the steps.

### Plans

- **Every new statement's plan is pinned** (`store/plans.txt`), and the plan test passes at the head and after the rebase.
- **Three scans are allowed with why:**
  - `EXISTS` on `openings` stops at its first row.
  - The first event with something left (`FIRST_OPENING`) goes down the rowid with `LIMIT 1`.
  - `SEALED_BETWEEN` reads every row of `unopened`, for which `plans.rs:43` says "a handful". See U3.
- **No statement sorts in a temporary b-tree:** the `ORDER BY`s ride `pending_by_kind (kind, n)` and `pending_by_target_at`.
- **The resume point is read by `at` alone:** `NEXT_WAITING` pages with `(at, n) > (?, ?)`, but SQLite seeks it on `at` and filters `n`. So rows still in `pending` at the resume point's second, which are those kept sealed, are read again by every page. See U4.

### The trial rebase onto `abfa15a4`

`git replay --onto abfa15a4 5230b9f..c63ee786` replays all 13 commits with no conflict, and `git range-diff` shows every patch unchanged (`=`): `f753f5c9` is the rebased head. Main's keepalive commits touch `sync/mod.rs`, `sync/tests.rs`, `send/tests.rs` and the protocol's README beside this range, but nothing it changes or calls (`sync::run` keeps its seven arguments). The rebased tree builds, and its 237 data-layer tests pass (main's new keepalive sync tests among them), as does the upgrade from version 1. The lander has nothing to resolve.

### The switches

Each turned off one rule, at run time, against the builder's 192 tests around the range (events, again, polls, links, kinds, convert, sync, send, plans, schema's classes and rules). "Caught" names the test that failed. The plan test failed in every switched run, unswitched included, since the switches add statements of their own; it passed at the head.

| Switch | Rule turned off | Caught by |
| --- | --- | --- |
| M1, M2 | the fold keeps the kept copy's newer answer, and its newer edit's detail | `a_fold_keeps_the_kept_copys_newer_answer…` (T2) |
| M3 | the read takes each person's newest answer | the fold test |
| M4, M5 | a JID found as you is you; your answer of none isn't yours | `an_answer_from_a_jid_found_as_yours…`, `your_answer_of_none…` (T3) |
| M6 | a set-aside renamed onto a taken name keeps its own row | `the_same_answer_set_aside_under_both…` (T5) |
| M7, M8 | a row read again, or recounted by a join, gives its chat a new version | `an_event_its_backfill_reads_again…` (T4), `a_join_that_counts…` |
| M10 | what waits for an event deleted for everyone goes | none of the builder's; P16 |
| M11 | the backfills write only what changes | two places tests |
| M19 | the fold gives a deleted kept copy no detail | the I2 event test |
| M29 | an answerer is tried by the JID they came by | 13 tests |
| D1a | names by own ID, not `pending.n` | four set-aside tests |
| D1b | a row leaves `pending` only once kept | none of the builder's; P24 |
| D1c, D1d | a second name; a same copy taken as kept | the name test |
| I2a, I2b | the deleted copy's guard on words, and on raw and envelope | the event's and the text message's fold tests |
| O3a, O3b, O3c | edits bounded, moved by a merge, brought back | the edit tests |
| U1a, U1b, U1c, U1d | 16 at once; set-asides back; 16 set aside a change; a savepoint a row | the steps, set-aside and failure tests |
| U1e | the writer tells the background | sync's background test |
| U1f | a step goes on from where the last got to | none of the builder's; P14 |
| U1g, U1i | what comes back leaves quarantine; an opening ends | the steps and set-aside tests |
| U1j | a join's retries wake the background | none of the builder's; P23 |
| I1a to I1f | the retry on a join, its two kinds of event, the two notes, the note cleared | `what_was_sealed_between_jids…` |

## Findings

### Data loss

None. D1 is closed (above).

### Inconsistency

- **I3. An answer kept sealed for an event jaw has, then set aside past the bound, isn't tried again when jaw learns its answerer's JID.**
  - **Where:** `SEALED_BETWEEN` (`store/write/events.rs:97`) picks the events in `unopened` that the person sent, or that a row in `pending` from one of their JIDs came for. It doesn't look at what's set aside in quarantine.
  - **What happens** (P22):
    - Ana's event `e1` is in; Bea's answer comes by her number, sealed for her LID, and stays sealed in `pending`, and `e1` is noted.
    - `pending` passes its 10,000 and sets the oldest aside, Bea's among them, as `… e1 <Bea> b1`.
    - Jaw then learns Bea's LID is hers. `e1` isn't tried again, and her answer never counts.
    - Had the join come before the overflow, it would count. The event's author side has no such gap: a join of Ana's starts `e1` again from the first, which brings back everything set aside for it.
  - **How likely:** rare. It needs `pending` past its bound with answers kept for events jaw has, which round one's O4 says can build up, and then a join for one of them. Nothing is lost: the answer is kept, and comes back if the event is ever tried again.
  - **The fix:** have `SEALED_BETWEEN` also find an event whose set-asides came from one of the person's JIDs, by name. They're named `<chat> <event> <sender> …`, so a range on `quarantine_by_ref` per JID, from `<chat> <event> <jid> ` to `<chat> <event> <jid>!`, finds them without a scan. Adopt P22 as its test.

### Tests (missing)

- **T6. D1's own guard:** a row leaves `pending` only once quarantine keeps it (`store/write/events.rs:551`). With it deleted whatever the inserts did, every test passes. Adopt P24, which plants rows under both of an answer's names.
- **T7. Where a step goes on from** (`left_to_take`, `:341`). With each step going on from the first again, every test passes. Adopt P14: 150 answers that don't open ahead of 10 that do, which then never finishes.
- **T8. A join's retries wake the background** (`stamps.opening()` in `known_again`, `:320`). With it off, every test passes. Adopt P23: a join leaves five answers to try, and the background must take them within 20 s. Either `sync::backfill` is opened to the crate for the test, as I did, or the test lives in sync's tests.
- P16 also catches round one's M10, which round one left with O2; it's worth adopting with them.

### Unpredictable performance

U3 to U5 are follow-ups; none breaks a budget at the sizes jaw sees (see "Measurements").
- **U3. `known_again` reads every row of `unopened`** whenever someone is known by a JID new to jaw (`upsert_person`'s `joined`), and starts again from the first every event it finds, in the transaction that learned the JID, which may be a live one.
  - **The cost:** a person new to jaw cost 0.19 ms with 100 events noted, 0.91 ms with 1,000, and 7.5 ms with 10,000, past the live budget.
  - **Why it can grow:** nothing bounds `unopened` but the events that keep something sealed, and a note stays while what's sealed for its event is set aside.
  - **The fix:** find the events by the JID, from an index on what's kept, rather than reading every note. Or note the person and leave the search to a background step, as the retries themselves already are.
- **U4. The resume point is read by `at` alone** (`NEXT_WAITING`, `:73`). Rows still in `pending` at its second are read again by every page.
  - Measured in the `sqlite3` shell, with jaw's index: 20,000 rows in one second read 19,964 index entries to give 64. 10,000 in one second took 0.8 ms a page.
  - Only rows kept sealed stay behind a resume point, and the bound keeps them to 10,000, so it stays within a background step.
  - Writing the condition as `at > ?3 OR (at = ?3 AND n > ?4)` doesn't help. A seek on `(at, n)` would need `n` as a column of the index.
- **U5. A step's millisecond isn't counted against the writer's 3 ms.** The steps that take what waits fill each background transaction, every row of them costing an opening, and the last step runs on past the writer's 3 ms for up to its own millisecond: 3.8 to 3.9 ms p50 and 5.0 to 5.6 ms p99, where main's polls backfill's steps, which mostly find nothing to do, run 0.12 ms p50. Live writes still wait for no more than a step (3.3 to 3.9 ms p99 beside them). A step could end at what's left of the writer's budget.

### Other (follow-ups; none blocks)

- **H1. A row whose taking failed is recorded in quarantine as `opening`, named `<chat> <event> <n>`** by its `pending.n`, and left in `pending`.
  - The record is never cleared once a later try takes the row.
  - A later failure under a reused n overwrites an older record's error and raw.
  - Nothing is lost, since the row itself stays in `pending`. Name it by the row's own ID as set-asides are, and clear it once the row is taken.
- **H2. An event deleted for everyone keeps the answers taken before its deletion.** How many depends on when the background's steps ran: P16's restarts differ in nothing else. Nothing reads them, since a deleted event counts nothing, and main keeps reactions and votes the same way. Deleting an event's answers as it's deleted for everyone would make it exact.
- **H3. An event deleted for you while its steps are under way** leaves what still waited for it in `pending` for good. `delete_row` doesn't clear what waits, as on main for every kind, and its tombstone keeps the event from coming back. It holds places in the bound, as round one's O5 does.
- **H4. A step that fails whole holds up every event behind it.** `FIRST_OPENING` always takes the event with the smallest n, and sync's backfill task waits from 1 s to 5 minutes before trying it again. Only a set-aside whose JSON doesn't read back into `pending` gets past the per-row savepoint, and `set_aside` never writes one, so I found no way to it.
- **H5. `pending` can stay past its bound.** Answers that come back for an event and stay sealed leave it over its 10,000 until later changes set them aside, 16 each. That's U1c's trade for short transactions; `encrypted_kept` shows it.
- **H6. The account's own JIDs, learned later, try nothing again.** Your answers from another device, sealed for a JID of yours that `me` doesn't list yet, are retried only when someone's person changes. Jaw learns its JIDs at linking, so this is an edge.
- **Round one's O1, O2, O4, O5 and U2 stand.** I1 now opens what O4 kept in `pending`, but not what's set aside (I3).

## Measurements

Method: `cargo test --release` (thin LTO, one codegen unit) of the rebased tree, `f753f5c9` with the audit's probes and no switches, on this Mac, at a load of 44 to 49 from other agents' work, on 20 cores. Every number is from a store in a fresh folder.

- **An event arriving with sealed answers waiting** (the builder's `measure_an_event_arriving_with_answers_waiting`, run twice, and my `audit2_measure_live_writes…`). The event's own transaction, its 16 openings included:

  | Waiting | The event's transaction | The rest |
  | --- | --- | --- |
  | 0 | 2.42 and 2.39 ms | none |
  | 16 | 3.96 and 3.67 ms | none |
  | 100 | 4.60 ms (the first of a run, preparing its statements) | 2 transactions |
  | 1,000 | 4.25 ms | 11 transactions, 67 ms in all |
  | 9,999 | 2.95, 2.97, 3.15, 3.38, 3.81 and once 8.67 ms | 106 to 108 transactions, 0.47 to 1.0 s in all |

  - **Against round one:** 9,999 waiting took 413 ms there.
  - **The background transactions** that take the rest run 3.83 to 3.92 ms p50 and 5.0 to 5.6 ms p99. The writer fills each to its 3 ms, and its last step, which runs for a millisecond once begun, can go past that (U5).
- **Live writes beside the steps** (`audit2_measure_live_writes_while_an_event_is_taken_in_steps`): a live message every 5 ms while 9,983 answers are taken in the background, as sync's task takes them. In two runs, 58 and 56 writes took 1.50 and 1.28 ms p50, 3.88 and 3.26 ms p99, and 5.30 and 3.90 ms at worst. That's within the 5 ms live budget at p99.
- **A person known by a new JID** (`audit2_measure_a_person_known_by_a_new_jid_with_events_unopened`), with N events noted in `unopened`, each keeping an answer sealed for Ana's LID. Each row is a live transaction:

  | Noted | A person new to jaw (p50, worst of 25) | Ana's join (one run) | Then the retries |
  | --- | --- | --- | --- |
  | 0 | 0.23 and 0.31 ms | 0.21 ms | none |
  | 100 | 0.19 and 0.22 ms | 0.27 ms | 19 ms, 3 transactions |
  | 1,000 | 0.91 and 1.14 ms | 0.98 ms | 183 ms, 32 transactions |
  | 10,000 | 7.50 and 8.12 ms | 8.79 ms | 1.83 s, 317 transactions |

  - Ana's join leaves the retries to the background. At 10,000 noted, every person new to jaw is past the 5 ms budget for a live message (U3).
- **Main's baseline,** in the same run: main's `measure_the_polls_backfill_and_what_votes_cost` at 200,000 messages backfilled in 49 transactions of 0.12 ms p50 and 2.85 ms p99. Its steps mostly find nothing to do, where each of the steps above opens up to 64 answers.
- **The resume point's read** (`sqlite3` 3.51, in memory, jaw's table and index): 10,000 rows kept in one second took 0.8 ms a page, and 20,000 read 19,964 index entries to give 64 (U4).
- **Kills:** the scenes after the rebase took 24 (P10), 22 (P11), 16 and 18 (P12), 12 (P13), 25 (P15) and 5 (P16) commits, each restarted from its snapshot, 122 in all. At the head, under more load, they took more steps; P11 took 34.
- **The upgrade:** each of the five runs took about 23 s, 20 s of it the jawd left to finish after its last start.

## The probes

They're kept with the switches, the scripts and the logs in this auditor's store: `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/m2-places-events-round2/`.
- **`audit2.rs`** is the probes' module, which goes in `store/write/events/tests/` with `mod audit2;` at the end of `tests.rs`. P23 needs `sync`'s `backfill` module made `pub(crate)`.
- **`switches-and-probes.diff`** applies to `c63ee786`.
- **The scenes** share a harness that snapshots after every commit and restarts from each snapshot.
- **P1, P2** (round one's, as written then): pass at the head and after the rebase. D1 and I1 hold for them.
- **P3** (O1): still fails, as round one said it would.
- **P7** (O5) and **P10** (round one's backfill kills, over 1,500 messages and now with the steps): pass.
- **P11:** an event arriving with 205 answers waiting, 48 of them set aside and 5 sealed for a JID jaw doesn't know, killed after any of its commits (22 after the rebase and 34 at the head, as many steps as the 1 ms limit gives under the load). Passes: 200 count, the 5 stay kept and noted, and nothing is left set aside.
- **P12 to P16:** pass (above).
- **P22:** fails at the head: I3.
- **P23, P24:** pass at the head, and fail with U1j and D1b.

The fix for I3 should turn P22 green, and P14, P23 and P24 cover T6 to T8.

# Round one: `b7e7ce7a` (kept as written)

Range: `7262a152..b7e7ce7a` in `m2-more` (worktree `~/src/jaw-m2more`), 29 October 2026. Auditor: database-expert. `7262a152` is today's `origin/main`. Line numbers are at `b7e7ce7a`, and paths are under `crates/jawd/src/` unless they say otherwise.

The range is five commits, and nothing is uncommitted but this note:
- `8dd73c7f`: a deadline (`STEPS_WITHIN`, 60 s) on every test loop over backfill or merge steps, in main's kinds, links and polls tests;
- `6c86bc35`: places: `send.location`, the `location` row, search by name and address, migration 19 and its `places` backfill;
- `61769ff8`: the terminal draws a place;
- `f6f086d4`: events: `send.event`, the `event` row with its counts derived as windows read it, `event_answers`, sealed answers and edits opened by jawd, migration 20 and its `events` backfill, the demo's event;
- `b7e7ce7a`: the terminal draws an event.

## What I did

Everything ran in one clone at `b7e7ce7a` (`/tmp/jaw-audits/m2-places-events/jaw`), with one build folder, `CARGO_INCREMENTAL=0`, under `nice`. Every test binary ran in a process group of its own, which a timeout kills whole, with `TMPDIR` inside the audit's folder.
- **Reading:** the range whole against `7262a152`, with the code around it: the writer's amendments, `apply_pending`, `fold`, `follow`, `join`, the backfill loop, the drain and the read side.
- **The head's tests:** its data-layer tests (events, places, polls, links, kinds, send, sync, convert, plans, schema), and the upgrade from a version-1 jaw.db.
- **Switches:** 13 of my own, each turning off one rule the range adds, switched at run time by `JAW_AUDIT_MUT`. I ran each against the builder's tests, then against my probes.
- **Probes:** ten scenes (P1 to P10), among them a kill after every committed step of both backfills.
- **Measurements:** on a release build, both backfills over a million messages, an event arriving with up to 9,999 answers waiting, and a window of events with 500 answers each, with main's own polls backfill as a baseline at the same load.

The machine was loaded by other agents throughout: 52 to 75 on 20 cores. No Go engine was built or run, nothing was sent to WhatsApp, nothing under `~/Library/Application Support/jaw` was opened, and every process I started has ended. The switches, probes, scripts and logs are kept in this auditor's store, `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/m2-places-events-round1/`. Its `switches-and-probes.diff` applies to `b7e7ce7a`.

## In short

Changes needed. Places and events are kept durably, their counts are derived and never kept, and both backfills are resumable, idempotent and cheap. But one path loses an answer, two leave what jaw shows wrong for good, and one breaks the live budget.

- **D1, data loss:** a sealed answer set aside past the 10,000 is named by `pending.n`, which `pending` reuses. The next answer set aside under that name is deleted and kept nowhere. P1 shows it end to end: Rui's newer answer is lost, and his older one counts. It's rare, but it's on the main path, since every live answer to an event jaw hasn't seen yet comes sealed.
- **I1, inconsistency:** an answer sealed for a JID that jaw learns is the author's (or the answerer's) only afterwards never opens. The join that would open it doesn't try it again, so the counts depend on what arrived first (P2).
- **I2, inconsistency:** an event deleted for everyone gets its words back, and search finds it, when a merge folds in a copy of it edited since. The range guards `fold`'s new `detail` line but not main's `text`, `spans` and `summary` beside it (P8).
- **U1, breaks a budget:** an event arriving with sealed answers waiting opens them all in its one transaction. On a release build that's 5.9 ms for 100, 30 ms for 1,000 and 413 ms for 9,999, against 10 ms p99 for an incoming message to be durable and acknowledged.
- **Tests:** the new 60 s deadline failed the events step test under load, where it passes alone in 3.9 s (T1). Eight of my 13 switches pass every test of the builder's: the fold's newer-wins rules (both of them), the read side's "you", the chat's new version, the set-aside rename's suffix, and the deleted copy's guard. My probes catch all but one of them.
- **What holds:**
  - **Sending:** `send.location` and `send.event` answer after one synced commit with their outbox rows, keyed, as `send.text` does, and `call: true` is error 4, as the protocol says.
  - **Migrations:** 19 and 20 are correct schema steps; the version-1 upgrade passes through them.
  - **Plans:** every new plan is pinned, with no scan or temporary b-tree.
  - **Backfills:** both took 6.3 s over a million messages, in 742 transactions, 0.91 ms p50 and 4.18 ms p99 at load 68; main's polls backfill gave 5.08 ms p99 at the same load. Killed after any of 79 committed steps and started again, they ended byte for byte as an uninterrupted run did, and taken again they told the feed nothing.
  - **The deadlines commit:** none of its tests can pass by timing out.

## What was asked, checked

### Counts derived

- **Never kept.** An event's row keeps `going`, `maybe` and `not_going` at 0, and `mine` null, as the converter writes them. Windows count them as they read the event, from its `event_answers` rows joined to `person_jids` (`store/read.rs:420`). Each person's newest answer counts once, by `(at, id)`. Your JIDs are you, and so is any JID of a person who answered by one of them. An answer of none counts nowhere, and a deleted event counts nothing. Nothing adjusts a count by hand, so nothing can drift, and there's nothing for a background check to compare.
- **Each JID's latest is kept** (`store/write/events.rs:76`): an answer replaces its JID's row only if it's newer by the answerer's clock, then by ID. So a replay or an older answer changes nothing and gives no new version, whichever way it comes: live and sealed, from history and opened, or kept by an earlier jawd. A person's several JIDs each keep their own row, and the read takes the newest of them.
- **Joins:** a join gives the events both people answered a new version (`events.rs:192`), before the JIDs move, so windows count them again. A join whose people didn't both answer changes no count.
- **Merges and kills:** below. The one hole is I1: an answer that can't open until a join comes stays uncounted after it.
- **Switches:** taking each person's last row rather than the newest is caught by the builder's fold test. Counting someone found as you as someone else (M4), and showing your answer of none as yours (M5), pass every test of the builder's; P5b and P5 catch them (T3).

### Idempotent upserts and quarantine

- **Edits** of an event apply only from its author and only when newer (`store/write.rs:2404`). A copy of the event arriving again keeps what its edit made it, place and time included (`upsert_message`). The backfills write a row again only where that changes it. My switch M11 (writing every time) is caught by two places tests.
- **Quarantine.** A location or event whose reading again fails or panics is kept in quarantine with its raw while the rest go on (`kinds::guarded`, shared with main's kinds backfill). A step that fails is rolled back, and `sync::backfill` waits from 1 s to 5 minutes before trying it again.
- **What can't be opened is kept:** in `pending`, as kind `rsvp` or `secret`. Answers that wait are bounded, 10,000 of each form, and past that the oldest are set aside whole in quarantine. They come back when their event comes, in the chat a merge keeps (P9 checks the rename of one set aside under both chats). The bound's set-aside loses data in one order: D1. Sealed edits aren't bounded (O3), and an answer that will never open holds a place for good (O4).
- **Live sealed answers and edits** of events leave the inbox in their conversion's transaction, their raw going on to `pending`. Other sealed changes stay in the inbox, as on main (`sync/drain.rs:210`).

### Sending

- `send.location` (`send/places.rs:66`) and `send.event` (`send/events.rs:43`) take `send.poll`'s path. The key and the chat are read first. Then, under the enqueue lock, one `Step::Enqueue` goes through `durably`, whose commit is synced before the answer.
- In the writer, `enqueue` (`store/write/outbox.rs:643`) checks the key again, and the outbox's bound, inside the transaction. It writes the message, its outbox row, its recipients and its key together. So two calls with one key make one message, and nothing is answered before a synced commit; main's `each_sending_call_that_keeps_something_is_one_synced_commit` now takes both methods.
- Each carries the chat's disappearing timer and is shown at once, from the converter's own reading of what it sends. So the row and its echo agree, a start or end truncated to the second included. Bad coordinates are -32602 and long names error 5. An event goes with a fresh 32-byte secret, kept in its row's raw, which its answers open with (`an_answer_to_your_event_opens_with_the_secret_it_went_with`).
- **`call: true` is error 4.** The protocol's Composing section says so in as many words ("`call` true is error 4 for now: the phones ask WhatsApp's server for a call link, which jaw's engine can't yet"), as does the design's Sending. The Errors table doesn't list it yet (O6).

### Merges

- **What moves:** an event in the chat going moves with its `event_answers`, since they hang on its `n`. Answers waiting in `pending` follow their chat (`follow`, `store/write.rs:1525`), and so do those set aside, renamed to the kept chat. One whose new name is taken keeps a row of its own; P9 holds the same answer set aside under both chats, and switched off (M6), the join fails on the unique key and is quarantined.
- **The fold:** where both chats have the event, `fold` (`:1809`) folds the copy's answers in, each only where it's newer than its JID's in the kept copy. It takes the copy's `detail` only with its newer edit, and the copy's answers go only after they've been copied.
  - The builder's test covers the copy being newer. P4 covers the kept copy being newer, for an answer from the same JID and for its edit, and passes at the head. With the newer-wins rule switched off (M1), or the copy's detail taken whatever its edit (M2), every builder's test passes and P4 fails (T2).
  - The hole is I2: a kept copy that was deleted for everyone takes the edited copy's words.

### The backfills

- **Resumable and bounded:** each step reads at most 64 messages past `after`, by `n` (an index range), and takes those of its kind not deleted or edited. It ends once it has read again for a millisecond, after one at least, and moves `after` in the same transaction. `sync::backfill` sends 64 steps of each backfill left, and the writer fits as many in a transaction as its budget allows.
- **Idempotent:** a row is written again only if what it shows or what finds it changes. A row the converter wrote since, or an edit made since, is left alone (the builder's tests, and M11).
- **Reported:** `jawd status` says `places_backfill` and `events_backfill` (messages still to look at), with `answers_waiting`, `answers_set_aside` and `encrypted_kept`.
- **Safe to kill at every step** (P10): a jaw.db at version 18 with 1,500 messages, then one with 5,000. Every 20th was a location and every 50th an event with three sealed answers an earlier jawd kept, one deleted for everyone, and one edited live before the backfill reached it.
  - Both backfills ran to the end, with a snapshot (`VACUUM INTO`) after each committed step: 24 and 79 steps. From each snapshot, as jawd finds jaw.db after a kill there, a new store finished them.
  - Every one ended identical to the uninterrupted run: every message's shown fields, every answer, what waits, quarantine, `person_jids` and `backfills`.
  - Taken again from the start, with the store open, they changed nothing and told the feed nothing.
  - A kill inside a transaction is rolled back by SQLite, and the range adds no file or ack outside one.
- **Timed:** under "Measurements". Both took 6.3 s over a million messages, and their transactions stayed within what main's own backfill does at the same load.

### The migrations

- **What they do:**
  - **19** (`store/schema.rs:977`) adds one `backfills` row, `places`, from 0 to the newest message's `n`.
  - **20** (`:985`) creates `event_answers` with `event_answers_by_who`, and adds the `events` row. The table is `STRICT` and `WITHOUT ROWID`, with primary key `(message, who)`, `CHECK`s on `answer` and `guests`, and foreign keys to `messages` (with `ON DELETE CASCADE`) and to `person_jids`.
- **Their cost:** both are schema steps, one row each and an empty table, so they cost the same however big jaw.db is. They run with every pending schema step in one `IMMEDIATE` transaction with `user_version`. On the million-message jaw.db, opening the store ran both in 73.8 ms, the open included.
- **Their lock lines** are `19 schema 121c1e12…08b6` and `20 schema e37a90af…c779d`. The fingerprints are computed from the migrations themselves (`schema.rs:1083`). Main's generic tests cover both and pass at the head: every earlier version keeps its rows, a fault at every statement of every migration, frozen texts, unique numbers. So does the upgrade from a version-1 jaw.db with the demo engine (`tests/upgrading.rs`). Neither text names its own number, so renumbering moves only the lock lines and the `// 19,` and `// 20,` comments.
- **Class:** `event_answers` declares itself jaw's own, as `poll_voters` is. A live answer's sealed raw is deleted once it's opened and applied, so the table can't be rebuilt from raw. Nothing drops it.

### The deadlines commit (`8dd73c7f`)

- **None can pass by timing out.** Each of the ten loops now asserts inside the loop that `STEPS_WITHIN` hasn't passed: five in the kinds tests, four in the links tests, and the polls tests' `merged`. So a step that fails each time fails its test at the deadline instead of spinning. Each loop's assertions after it are unchanged, the deadline is an `assert!`, never a `break`, and the two measurements now fail on a step that fails.
- **They can now fail by timing out,** and so can the places and events tests that came with the same constant. At the head, under a load of 55 to 72, the events step test (`store/write/events/tests.rs:705`) failed at its deadline ("the backfill never ended") with 326 tests beside it. Alone, it passed in 3.9 s. On a busy machine each step reads one event again, so its 291 events take about 290 round trips to the writer. See T1.

### Plans

- I read every new statement's pinned plan (`store/plans.txt`). Each is an index or primary-key search, or a rowid range for the backfills' 64 rows.
  - The `ORDER BY`s ride their indexes: `pending_by_kind (kind, n)` for the oldest answers, `person_jids_by_person` for a person's JIDs in `n` order, and `pending_by_target_at` for what waits.
  - The join's lookup of events both people answered goes through `person_jids_by_person` and `event_answers_by_who`.
  - The foreign keys' checks, as a message is written or deleted and as a person's JID is made, search the new table by its keys.
- The plan test passed at the head. No statement scans a table or sorts in a temporary b-tree.

### My switches

Each turns off one rule, at run time, against the builder's tests around the range: 164 of events, places, polls, links, convert, sync and send. "Pass" means only main's timing tests failed: `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean` and `the_engine_next_word_waits_until_the_account_it_said_before_is_stored`, which failed in the unswitched run too.

| Switch | Rule turned off | The builder's tests | My probes |
| --- | --- | --- | --- |
| M1 | the fold takes the copy's answer only if newer than its JID's in the kept copy | pass | P4 fails |
| M2 | the fold takes the copy's `detail` only with its newer edit | pass | P4 fails |
| M3 | the read takes each person's newest answer, not their last row | the fold test fails | |
| M4 | someone answering by one of your JIDs is you | pass | P5b fails |
| M5 | an answer of none isn't yours | pass | P5 fails |
| M6 | a set-aside answer renamed onto a name taken keeps its own row | pass | P9 fails: the join is quarantined |
| M7 | a row read again that's its chat's last gives the chat a new version | pass | P6 fails |
| M8 | a recount that's its chat's last gives the chat a new version | pass | none; the chat list shows no counts |
| M10 | what's kept sealed for an event that comes or moves deleted is deleted | pass | none (it keeps more) |
| M11 | the backfills write a row only if that changes it | two places tests fail | |
| M19 | the fold gives a deleted kept copy no `detail` | pass | P8 fails, on `detail` too |
| M29 | an answerer jaw doesn't know yet is tried by the JID they came by | six tests fail | |

### The gate

At the head, the data-layer tests: 317 passed and 2 failed. One was the events step test at its deadline (T1); the other was main's `an_engine_that_doesnt_disconnect_in_time_…`, which this range doesn't touch. The version-1 upgrade passed. I didn't run `make lint` or the workspace build; the builder reports them passing.

## Findings

### Data loss

- **D1. A sealed answer set aside past the bound is named by `pending.n`, which `pending` reuses, and the next answer set aside under that name is deleted and kept nowhere.**
  - **Where:** `store/write/events.rs:40-50`.
    - `SET_ASIDE` names a row `chat target sender coalesce(effect ->> '$.id', n)`. A sealed answer waits as `{"do":"keep","kind":"rsvp"}`, which has no `id`, so it's named by its `pending.n`, not by "its own ID" as the comment above it says.
    - `pending` has no `AUTOINCREMENT`, so an `n` comes back once every row above it has gone.
    - On a conflict the `INSERT` does nothing (`ON CONFLICT (source, ref) DO NOTHING`), and `DROP_SET_ASIDE` deletes the row anyway.
  - **What happens** (P1):
    - Rui's sealed answer, going, to `e1`, which jaw hasn't seen, waits as `pending` n 1.
    - 9,999 sealed answers to `x1` fill the bound, and the next answer sets Rui's aside as `… e1 <Rui> 1`.
    - `x1` comes and opens its 9,999 answers, and `e2` its one: `pending` is empty.
    - Rui answers `e1` again, not going. His answer waits as `pending` n 1 again.
    - 9,999 answers to `x2` fill the bound once more, and the next sets his second answer aside under the same name: the insert is skipped and the row deleted.
    - `e1` comes and shows Rui going. His newer answer is in neither `pending` nor `quarantine`.
  - Every step went through the store but the two bulk inserts of 9,999 waiting answers, which write the rows a live sealed answer writes, as the builder's `fill_pending` does. The drain was real: each answer opened with its event's secret.
  - **How likely:** it needs `pending` to drain below the set-aside row's `n` between two answers of one person to one event jaw hasn't seen, both past the bound. That's rare. But it's on the main path: every live answer to an event jaw hasn't seen yet comes sealed. It's m2-content's H3, a follow-up there because only standalone history votes went without an ID.
  - **The fix:**
    - Name a sealed answer by its own message ID: its raw's `key.id`, as `kinds::own_id` reads it, carried in the kept row's `effect` or computed in Rust as it's set aside.
    - Delete from `pending` only what's now in quarantine with the same raw (an `EXISTS` on the ref and `raw ->> '$.raw' = hex(pending.raw)`), so a conflict never deletes.
    - Or give `pending` `AUTOINCREMENT`, as the design asks of row keys others hold. That's a rebuild of a small raw table, and it closes the votes' H3 too.
    - Adopt P1 as its test, quickened by a side connection's `DELETE` for the drain.

### Inconsistency

- **I1. An answer sealed for a JID of its event's author, or of its answerer, that jaw learns only afterwards, never opens.**
  - **Where:** what's kept sealed opens only when its event arrives, moves with a merge, or the events backfill reaches it (`apply_pending`, `store/write.rs:2130`; `again.rs:43`). A join (`join`, `:1360`) gives the events both people answered a new version, but doesn't try what's kept sealed again.
  - **What happens** (P2): Ana's event `e1`, with Ana known by her number only. Rui's answer comes sealed for Ana's LID, can't open, and is kept. Then jaw learns that the LID is hers. The event still counts nobody, and the answer stays sealed for good. Had the join come first, it would count: the end state depends on the order the answer and the join arrive in.
  - **How likely:** an event sent by number and answered by LID, as a group moves to LID addressing. jaw learns an author's LID from `SenderAlt` only once she writes again.
  - **The fix:** when a join gives a person JIDs, open again what's kept sealed from them and for the events they sent, applying what opens as `apply_pending` does. That's the `pending` rows of kind `rsvp` or `secret` whose `sender` is one of their JIDs, or whose event's sender is. It's bounded by the 10,000, and a join is rare. Adopt P2 as its test.

- **I2. An event deleted for everyone gets its words back, and is found by search, when a merge folds in a copy of it edited since.**
  - **Where:** `fold` (`store/write.rs:1809`). This range guards the new `detail` line with `NOT k.deleted`, but the `text`, `spans` and `summary` lines beside it are main's, and unguarded (`iif(g.edited_at > coalesce(k.edited_at, -1), g.text, k.text)`). Only a copy that goes deleted is deleted again after the fold (`if revoked`), not one that's kept.
  - **What happens** (P8): Ana's event is in her number's chat and in her LID's. She deletes it for everyone in the first, and her edit ("Gone zebra") reaches the second. They merge.
    - The kept event is still `deleted`, with no `detail`.
    - But its `text` is "Gone zebra" again, `messages_fts` has it, and a search for "zebra" finds it.
    - So a deletion for everyone doesn't stick through a merge, and where each copy's change landed decides the end state.
  - Before this range an event's edit changed nothing, so events couldn't hit this. By reading, main's text messages can, through the same lines (outside this range).
  - **The fix:** give `text`, `spans` and `summary` the guard `detail` has (`NOT k.deleted AND …`), which mends text messages too. Adopt P8 as its test; with the range's guard on `detail` switched off (M19), P8 fails on `detail` as well.

### Tests (missing)

- **T1. The deadline can fail a test that would pass.** `STEPS_WITHIN` is 60 s of wall time (`store/mod.rs:70`), and a backfill step stops reading again once it has had a millisecond, after one row at least. On a busy machine each step reads one event again, so the number of steps grows with the load.
  - At the head, under a load of 55 to 72, the events step test (291 events) failed at its deadline with 326 tests beside it. Alone, it passed in 3.9 s.
  - **The fix:** bound the loops by steps rather than time. A step that doesn't fail moves `after` on by a row at least, so rows plus one steps suffice. Or give the deadline minutes: the `quarantined == 0` checks already catch a failing step at once.
- **T2. The fold's rules in the other order:** the kept copy's newer answer from the same JID, and its newer edit's `detail`. M1 and M2 pass every test of the builder's. Adopt P4.
- **T3. The read side's "you":** an answer by a JID found as you (M4), and your answer of none (M5). Adopt P5b and P5.
- **T4. A row read again by the backfills gives its chat a new version** when it's the chat's last (M7). Adopt P6.
- **T5. A set-aside answer renamed onto a name the kept chat has** keeps a row of its own (M6). Without it the join fails on the unique key, and the person and their chats are never joined. Adopt P9.

### Unpredictable performance

- **U1. An event arriving with sealed answers waiting opens them all in its one transaction.**
  - **Where:** `apply_pending` (`store/write.rs:2130`). Each answer opened reads the event's row and decodes its raw, finds both people's JIDs, makes the key, opens the seal, makes the answerer if they're new, and writes the answer.
  - **The cost** (release, load 67): 100 answers waiting take 5.9 ms, 1,000 take 29.8 ms, and 9,999 take 413 ms, each one transaction on the writer. The budget for an incoming message to be durable and acknowledged is 10 ms p99, and every live write and ack waits behind this one. The same happens where a merge moves such an event, and in a backfill step that reaches one.
  - **How likely:** answers come before their event when it can't be decrypted yet, or comes with history later. A big group's event gets hundreds of answers.
  - **The fix:** open what waits for an event in bounded steps after the event's own transaction, as a backfill keyed on it does, 64 a step, the counts landing as they go. Or cap each transaction's openings and leave the rest to the background lane.

### Other (follow-ups; none blocks)

- **U2. A page of events with many answers passes the page budget.** A 50-row window with 26 events of 500 answers each read in 5.2 ms p50 and 6.0 ms worst; the budget is 5 ms for 100 rows. Each event reads all its answers with a join to `person_jids`, as polls do (m2-content's H1). The fix is H1's: find the account's keys once, and count from an index.
- **O1. Two edits of an event in one second end as they came** (P3): `x1` then `x2` shows `x1`'s, and the other order shows `x2`'s.
  - An edit is timed by its message's timestamp, which is in seconds, and the first applied wins (`edited_at < ?6`, `store/write.rs:2408`).
  - Main's text edits take `<=` (`:2319`), so there the last applied wins; the builder's report says "as for text edits", which is the other way.
  - Break ties by the edit's message ID, here and for text.
- **O2. `Opening::Gone` drops more than events' sealed changes.** `sealed_for_events` is true for every `secret` kind. So an encrypted edit of any message, which jaw can't read yet, is deleted from `pending` (`store/write.rs:2157`), or never kept (`:2230`), when its message is deleted for everyone.
  - Main kept those for a later jawd. Nothing a later jawd could show is lost, since a deletion for everyone sticks, but main's rule changes for kinds this range doesn't read, untested (M10 passes every test).
  - Narrow it to `opens_here`'s kinds, or say so in the design and test it.
- **O3. Sealed edits waiting for an event jaw hasn't seen aren't bounded.** Only answers have the 10,000 (`events.rs:234`). Any member can send one, since its author is checked only once the event is there. `encrypted_kept` counts them. Bound them with the answers.
- **O4. A sealed answer that won't open waits among the 10,000 for good.** One for an event jaw has, sealed with a secret or JIDs that don't open it, holds a place in the bound. Once set aside, it never comes back, since `set_aside_back` runs only as the event arrives. It's kept, and counted in `encrypted_kept` and `answers_set_aside`: m2-content's H2, for answers.
- **O5. What's kept sealed for an event deleted for everyone stays in `pending`** (P7): the events backfill skips deleted events, so their kept answers hold places in the bound. They're harmless.
- **O6. The protocol's Errors table** doesn't list an event's `call` under 4, or its limits under 5; the Composing section does.

### Outside this range

- **Main's text messages and I2:** the same `fold` lines can give a message deleted for everyone the words of a copy edited since. I found this by reading; P8 shows it for events. I2's fix mends both.
- **The votes' H3** is D1's mechanism for votes without an ID. Giving `pending` `AUTOINCREMENT` closes both.
- **Main's sync timing tests** `an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean` and `the_engine_next_word_waits_until_the_account_it_said_before_is_stored` failed under this load, in runs with and without my switches.

## Measurements

Method: `cargo test --release` (the workspace's release profile: thin LTO, one codegen unit) of `b7e7ce7a` with my tests and no switches, on this Mac, at a load of 66 to 75 from other agents' work, on 20 cores. Each is one run.

- **Both backfills, a million messages** (`audit_measure_the_places_and_events_backfills`):
  - **The file:** a jaw.db at version 18, built outside jawd, 228 MB. It held 1,000,000 messages in 2,000 groups:
    - 4,000 locations (one in 200), whose names need trimming, so each reads again;
    - 1,000 events (one in 1,000), each with its secret, a place, and 5 sealed answers an earlier jawd kept, 5,000 in all;
    - text otherwise, each message with a raw.
  - **Migrations:** opening the store ran 19 and 20 in 73.8 ms, the open included.
  - **The run:** both backfills went as `sync::backfill` sends them, 64 steps of each to an apply. They took **6.27 s**: 742 transactions in 245 applies, each transaction **0.91 ms p50 and 4.18 ms p99** (all 742 among the writer's last 1,024). An apply of 128 steps took 7.1 ms p50, 35.6 ms p99 and 92.8 ms at worst.
  - **What it left:** every location and event read again, the 5,000 answers opened, nothing left kept, nothing quarantined.
  - **Main's baseline:** main's own `measure_the_polls_backfill_and_what_votes_cost`, on the same build at 1,000,000 messages and the same load, took 27.8 s, at 0.07 ms p50 and **5.08 ms p99** a transaction. So the p99 above 3 ms is the machine's load, as it is for main's backfill; no step of this range's runs long.
- **An event arriving with sealed answers waiting** (`audit_measure_an_event_arriving_with_answers_waiting`), each answer from a sender of its own:

  | Waiting | Applied in | Transactions |
  | --- | --- | --- |
  | 100 | 5.9 ms | 1 |
  | 1,000 | 29.8 ms | 1 |
  | 9,999 | 413 ms | 1 |

  The debug build in P1 took 1.53 s for 9,999.
- **A window of events with many answers** (`audit_measure_a_window_of_events_with_many_answers`): the newest 50 messages of a group, 26 of them events with 500 answers each (13,000 in all), read 60 times, the first 10 left out. It took **5.23 ms p50 and 6.02 ms at worst**. In the baseline run, main's window of 100 polls with 50 voters each took 2.95 ms p50.
- **Kills** (P10, debug): 1,500 messages gave 24 steps, and 5,000 gave 79, each restarted from its snapshot. Every restart ended identical to the uninterrupted run, and a second pass from the start told the feed nothing. The 5,000 run took 397 s with all its restarts.

## The probes

All are in `switches-and-probes.diff`, at the end of `store/write/events/tests.rs`, named `audit_p1` to `audit_p10`. The measurements are there too, as ignored tests. At the head:
- **P1** fails: D1.
- **P2** fails: I1.
- **P3** fails: O1, by design of the probe.
- **P4** passes: the fold's other order.
- **P5** and **P5b** pass: your answer of none, and a JID found as you.
- **P6** passes: the chat's new version.
- **P7** passes: what's kept for a deleted event.
- **P8** fails: I2.
- **P9** passes: the set-aside rename.
- **P10** passes: kills after every step.

The fix for each finding should turn its probe green, and P4, P5, P5b, P6 and P9 cover T2 to T5.

## Verdict

Changes needed. Before this lands:
- **D1:** name a sealed answer set aside by its own message ID, or never delete what wasn't set aside, or give `pending` `AUTOINCREMENT`; with P1.
- **I1:** open again what's kept sealed when a join gives its people JIDs; with P2.
- **I2:** guard `fold`'s `text`, `spans` and `summary` as its `detail` is; with P8.
- **U1:** open what waits for an event in bounded steps, so its arrival keeps within the live budget.
- **T1 to T5:** a deadline that load can't trip, and the tests for the rules my switches turned off unnoticed.

U2 and O1 to O6 are follow-ups.

Verdict: changes needed

# Verdict of round two (`c63ee786`)

Changes needed, and small. Nothing is lost, and D1, I2, O3, T1 to T5 and O6 are closed. U1 keeps to the live budget and holds through kills, merges, joins and replays. The rebase onto `abfa15a4` is clean. Before this lands:
- **I3:** have the retry on a join also find an event whose answers set aside in quarantine came from one of the person's JIDs, by a range on their names, with P22.
- **T6 to T8:** adopt P24, P14 and P23, the tests of D1's guard (nothing leaves `pending` that isn't kept), of where a step goes on from, and of a join waking the background.

U3 to U5 and H1 to H6 are follow-ups, as round one's O1, O2, O4, O5 and U2 still are.

**For round three:** the range will be these 13 commits, plus the fixes, on wherever main is then: `f753f5c9` is them replayed onto `abfa15a4`, unchanged. The migrations stay 20 to 23 if this lands before another branch takes 20; if one does, all four move up together, which changes only their lock lines and the `// 20,` to `// 23,` comments.

Verdict: changes needed

# Verdict of round three (`aa848fc6`)

Approved. Nothing in the branch is lost or inconsistent, and every rule this round touches has a test that fails without it, and alone among the builder's 200:
- I3's clause: P22's adopted test;
- D1's guard: P24's;
- where a step goes on from: P14's;
- a join waking the background: P23's;
- what's left for an event deleted for everyone: P16's.

U3's growth and H7 are follow-ups, as U3 to U5, H1 to H6 and round one's O1, O2, O4, O5 and U2 still are; none breaks a budget at the sizes jaw sees. L1 to L3 are for the landing, and each is tested on the trial.

## Landing

- **Which commits take the trailer.**
  - `Data-Audit: docs/audits/2026-10-29-m2-places-events.md` goes on the 15 commits that touch the data layer, named as at `aa848fc6` (their rebased hashes will differ): `98dd9061`, `eb2e2bb3`, `1d1c53fc`, `0023eaa8`, `bd7847e7`, `0184c9cc`, `8b7fa887`, `ba585291`, `f12f0069`, `8a4b6e69`, `3887db83`, `5fc20cbf`, `994620e6`, `77dd7bbf` and `aa848fc6`.
  - `bdfb5758` and `dbb9b5a9`, the terminal's, and `42a658b0`, the protocol's Errors table, touch none, though `git rebase -x` giving every commit the trailer does no harm.
  - L1's and L2's changes go into the places commit, which takes the trailer anyway.
  - Commit this note in the branch, and land from `~/src/jaw-m2more`.
- **The renumbering after media in.** Media in lands first, as migration 20. These become:
  - places 21 (`21 schema 121c1e12…08b6`);
  - events 22 (`22 schema e37a90af…c779d`);
  - the openings 23 (`23 schema 0896c5ee…d6ed`);
  - the unopened 24 (`24 schema 5aca4a61…01e0`).

  The lock lines keep their hashes and `schema.rs`'s four `// N,` comments move; nothing else names a number. To land:
  - resolve the seven conflicting commits as the trial's table does;
  - put L1's seeds and L2's ID in the places commit, so every commit passes `kept`'s and the demo's tests;
  - write `plans.txt` again wherever it conflicts, checking that its diff is only what L3 says.

  `trial.bundle` in the store is the result to compare against.
- **What the lander re-checks if main moves.**
  - **Main so far:** since `c50a2ba3` it has moved only under `mac/` and `docs/plans/`, to `8e0a63d0`, which touches nothing here.
  - **If media in changes before it lands:** look again where the trial met it: `kept.rs`, `engine/demo.rs`, `links.rs`'s dispatch, the fold, `sync/backfill.rs` and the schema.
  - **If typing (M6) lands first:** it brings no migration and no demo ID, but shares 17 files with this branch, among them `plans.txt`, `state.rs`, `store/read.rs`, `sync/mod.rs`, `send/`, the README and the design. Rebuild, and run the plan test and the data-layer tests again.
  - **If any other migration lands first:** move these four up once more, lock lines and comments only, and seed its tables in `kept.rs` as L1 seeds media's.
  - **Each time:**
    - `make check-generated`, `make lint`, `make build` and `make test`, with `kept`'s and the plan tests, the schema's, the events', the merges', media's and the upgrade from version 1 among them;
    - the demo account's messages, each with an ID of its own;
    - `make check-audit`.

Verdict: approved
