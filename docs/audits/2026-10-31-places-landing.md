# Audit: places and events landing on main (`m2-more` onto `3878dd66`)

Range: `d5965757` in `m2-more` (worktree `~/src/jaw-m2more`), 19 commits on `c50a2ba3` approved in round three of `docs/audits/2026-10-29-m2-places-events.md`, rebased onto `origin/main` at `3878dd66`, 31 October 2026. Auditor: database-expert.

Only the landing is in scope: what the rebase onto today's main does beyond keeping both sides, and the change that typing's eighth `sync::run` parameter needs. Everything else in the branch stands as the m2 note approved it.

Main has moved 58 commits since `c50a2ba3`: the Mac app's panes (under `mac/` and `docs/plans/`), media in (migration 20 and its landing changes), typing (protocol 2 and `sync::run`'s eighth parameter), `stopping`'s test fix, and read receipts after a merge.

Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at my rebased head with the patch folded in. The worktree's lander was mid-rebase, so I only read the worktree, and rebased `d5965757` in a clone of my own. In the agent store I wrote this note, the patch, my rebased branch as a bundle, and my check scripts.

## In short

Approved. Nothing is marked data loss or inconsistency.

- **The rebase keeps both sides, and the m2 note's landing changes are made.**
  - 8 of the 19 commits conflict, in 15 files: the trial's 12, two of typing's (`crates/jaw/src/demo.rs` and `sync/tests.rs`), and `store/schema/kept.rs`, which conflicts only because L1 goes into the places commit (O1).
  - Every conflict keeps both sides' lines. The migrations become 21 to 24, their lock hashes unchanged.
  - L1 and L2 are in the places commit, and `plans.txt` is written again at each of the 8 commits that change it (L3).
  - Two independent ways give the same tree. My rebase, and round three's trial merged onto `3878dd66`, differ only where they should: `plans.txt` (written again), the m2 note, and the three files that merge conflicts in, where my head keeps both sides.
- **One change goes beyond keeping both sides, and it's one line.**
  - The branch adds one `sync::run` call, in `sending()` (`send/places/tests.rs:31`), the rig the places and events tests share. It passes the seven arguments from before typing.
  - The patch gives it typing's argument, `crate::feed::Typing::default()`, the same default main's own test helpers pass. It's test code only.
  - A default `Typing` starts nothing, follows nothing and writes nothing, so the tests behave as they did on the branch, where sync ignored typing.
- **Nothing else is needed.**
  - Without the patch, `cargo check -p jawd --all-targets` fails with exactly one error, at that call.
  - With it, clippy and rustfmt pass, and so do jawd's unit tests: 856 of 873 pass, two of main's timing tests fail under load and pass alone 3 of 3 (O5), and 15 measurements are ignored.
  - The upgrade from version 1 to 24 passes, and so do jawd's other end-to-end tests and jaw-proto's fixtures.
  - With the patch folded into the places commit, all 19 commits build. At each of the 8 commits that change `plans.txt`, the file written again holds exactly both sides' entries, and kept's test and the plan tests pass.
- **The rule can stand for the other waiting branches,** under the five conditions below.
  - Of the seven, only contact cards (two calls) and chat actions (one) need it today.
  - The library, disappearing, communities, media out and presence already pass eight arguments at every call.

## The rebase onto `3878dd66`

Main's schema hasn't moved since round three's trial:
- Its `migrations.lock`, `store/schema.rs`, `store/schema/kept.rs`, `tests/upgrading.rs` and the test fixtures are byte for byte those of media in's `42a31eaf`, the trial's base.
- Main's last migration is still 20 (`20 schema cc0f2d3a…9e03`).
- What main changed since the trial (typing, read receipts) touches no migration.

**What each conflict was, and how I resolved it.** The other 11 commits applied by themselves.

| # | Commit | Files that conflict | Resolution |
| --- | --- | --- | --- |
| 1 | `cee94d4b` (test deadlines) | `store/mod.rs` | As the trial (below) |
| 2 | `92e96e55` (places) | `migrations.lock`, `store/schema.rs`, `store/write.rs`, `store/write/links.rs`, `store/plans.txt`, `jaw-proto/tests/fixtures.rs`, `protocol/README.md` | As the trial, then L1, L2 and the patch, and `plans.txt` written again |
| 3 | `751535c7` (the terminal draws a place) | `crates/jaw/src/demo.rs` | New since the trial |
| 4 | `3dfde819` (events) | `migrations.lock`, `store/write.rs` (5 hunks), `store/write/links.rs`, `store/schema/kept.rs`, `convert/mod.rs`, `engine/demo.rs`, `store/plans.txt`, fixtures, README, `scripts/check-data-audit.py` | As the trial; `kept.rs` is new (O1) |
| 6 | `976437ed` (test steps bounded) | `store/mod.rs` | As the trial |
| 9 | `912c2803` (edits bounded) | README | Both clauses |
| 10 | `8568591e` (steps, migration 23) | `migrations.lock`, `store/schema/kept.rs`, `sync/backfill.rs`, `sync/tests.rs`, `store/plans.txt`, README | As the trial; `sync/tests.rs` and `kept.rs` are new |
| 11 | `9284c76b` (unopened, migration 24) | `migrations.lock`, `store/schema/kept.rs`, `store/plans.txt`, README | As the trial; `kept.rs` is new |

In more detail:
- **1, test deadlines:** media's two test functions, then `STEPS_WITHIN`.
- **2, places:**
  - places becomes migration **21** (`121c1e12…08b6`), with its comment `// 21,`;
  - `mod again;` goes before `pub mod copies;`;
  - `BACKFILLS` lists `COPIES` before `PLACES`, and places' arm takes media's fourth field, `STEP`;
  - the fixtures keep `media.save`'s line and add `send.location` to the `Sent` line;
  - the `status` row is merged word by word.
- **3, the terminal draws a place:** one import list keeping typing's `Typist, TypistState, Typists` and the branch's `Location`.
- **4, events:**
  - events becomes **22** (`e37a90af…c779d`). `schema.rs` merges by itself, so its comment is renumbered to `// 22,` by hand;
  - the fold folds events' answers, then calls `media::fold`, never main's two old `media` statements;
  - the `Amendment` variants are `Hd`, then `Answer` and `EventEdit`, and both sides' arms are kept;
  - `EVENTS` goes after `PLACES`, with `STEP`;
  - both sides' imports, paths, plans and clauses are kept.
- **6, test steps bounded:** media's two test functions, then `Steps` with its comment.
- **10, steps:**
  - **23** (`0896c5ee…d6ed`), with its comment `// 23,`;
  - the backfill module's comment has media's sentence, then events';
  - `sync/tests.rs` is new since the trial: typing's test and its `typed` helper, then the branch's background-steps test, both added at the end.
- **11, unopened:** **24** (`5aca4a61…01e0`), with its comment `// 24,`.

**How I checked the resolutions,** three ways:
- **Commit by commit:** each resolved commit adds and removes the same lines as its original, compared as multisets, except where it should differ:
  - the renumbering (four lock lines and four `// N,` comments);
  - `BACKFILLS` gaining `COPIES`, and the two dispatch arms gaining `STEP`;
  - L1, L2 and the patch;
  - the `status` row, whose word changes are exactly the commit's own;
  - `plans.txt`.
- **File by file, against the trial:** I transplanted each of the trial's resolutions onto my commit's parent with `git merge-file`. Wherever that merges cleanly, it gives my file byte for byte. It conflicts only in the three files typing touches (the `status` row, jaw's demo imports, `sync/tests.rs`), and in `kept.rs`, where L1 sits elsewhere in the trial.
- **The whole tree:** the trial's head, with its probes reverted, merged onto `3878dd66` with `42a31eaf` as base, differs from my head only in:
  - `plans.txt`, which I wrote again;
  - the m2 note, which the trial didn't have;
  - the three files that merge conflicts in, where my head has both sides' lines and nothing else.

  The `status` row at my head is the word-by-word merge of main's row with the branch's whole change to it.

The 19 messages and authors are unchanged, and the same 15 commits carry the m2 note's trailer.

**To check a resolution against mine,** compare each file's blob at the rebased commit (`git rev-parse --short=12 <commit>:<path>`). These are the blobs of my branch `landed`, with the patch folded in and `plans.txt` written again:
- **1:** `store/mod.rs` `afd82104038c`.
- **2:**
  - `jaw-proto/tests/fixtures.rs` `b23b45b5618e`;
  - `migrations.lock` `d932455eaf8b`;
  - `store/schema.rs` `63b029538af5`;
  - `store/write.rs` `1bd71bc26aa0`;
  - `store/write/links.rs` `b9610dea4864`;
  - `protocol/README.md` `eae8f9674371`;
  - `store/schema/kept.rs` `9931837e804b` (L1);
  - `engine/demo.rs` `a943050b358a` and `main.rs` `7ccaa3a42d6b` (L2);
  - `send/places/tests.rs` `041e927ff221` (the patch).
- **3:** `crates/jaw/src/demo.rs` `acfdacc2df93`.
- **4:**
  - `jaw-proto/tests/fixtures.rs` `90471837a60f`;
  - `convert/mod.rs` `33cc405ea5a7`;
  - `engine/demo.rs` `c0c821e25511`;
  - `migrations.lock` `8ebb3ad9ff94`;
  - `store/schema/kept.rs` `17bea3f867ee`;
  - `store/write.rs` `38cdcc076489`;
  - `store/write/links.rs` `bbd8bfad9116`;
  - `protocol/README.md` `99094da24ace`;
  - `scripts/check-data-audit.py` `6d1f28687e02`;
  - `store/schema.rs` `43c10d6712c0`.
- **6:** `store/mod.rs` `be1a07c19d5a`.
- **9:** `protocol/README.md` `4993a50e75b5`.
- **10:**
  - `migrations.lock` `98df69e98471`;
  - `store/schema/kept.rs` `e25fc4b09305`;
  - `sync/backfill.rs` `15018a27ef77`;
  - `sync/tests.rs` `488461603b33`;
  - `protocol/README.md` `f296f9498c5d`;
  - `store/schema.rs` `c99621fdd521`.
- **11:**
  - `migrations.lock` `657501176d75`;
  - `store/schema/kept.rs` `6f9259dfb837`;
  - `protocol/README.md` `9c00e4ea801c`;
  - `store/schema.rs` `2a8b66923ff2`.
- **`store/plans.txt`,** as the plan test writes it at the eight commits that change it:

  | # | Commit | Blob |
  | --- | --- | --- |
  | 2 | `92e96e55` | `e37c3823e3ca` |
  | 4 | `3dfde819` | `f97b7f5c321e` |
  | 7 | `5fc81894` | `ebbe6b51f306` |
  | 8 | `45a9cfb9` | `68318a537480` |
  | 9 | `912c2803` | `edc56de47df2` |
  | 10 | `8568591e` | `0672a164ea39` |
  | 11 | `9284c76b` | `879462bcec64` |
  | 14 | `4ddc2793` | `dc2f6dfb0db1` (also the head's) |

`integration/places-landing.bundle` in the agent store holds the branch, `3878dd66..landed`. Its commit hashes differ from a lander's, but its trees can be compared with `git diff`.

### The m2 note's landing changes

- **L1, in the places commit:** media's seed row gets a 32-byte hash and the state `expired`, `media_files`, `media_made` and `media_drop` get a row each, and `seed()` upserts, as the m2 note prints it.
  - At the places commit the seeds end at `poll_voters`, so media's three rows go after it, and three later commits conflict there (O1).
  - At the head, `kept.rs` is the trial's with L1, byte for byte (`6f9259dfb837`).
  - Kept's test passes at each of the eight commits I wrote `plans.txt` at, the four that add migrations among them, and at the head.
- **L2, in the places commit:** the pin is `3EB0DEMO0000000C`, in `engine/demo.rs` and in `main.rs`'s test.
  - At the head, each of the demo account's twelve IDs belongs to one message. `…05` and `…09` appear again only where the poll's vote and the event's answers name their poll and event.
  - The place's socket test passes at the places commit, the events commit and the head; the event's at the events commit and the head.
- **L3, at each commit that changes `plans.txt`:** I wrote the file again with `JAW_PLANS=write` and compared it entry by entry with main's and the original commit's (`plancheck.py`).
  - Every entry is main's, or the commit's change since `c50a2ba3`.
  - From the events commit on, both sides change two entries: the foreign-key checks of `DELETE FROM messages WHERE n = ?1` and of `INSERT INTO messages`. Each has exactly the union of both sides' lines.
  - Nothing new scans or sorts.
  - The head's file has 621 entries.

## Typing's argument: the one change beyond keeping both sides

**Where.** The branch adds one call of `sync::run`, in `sending()` (`send/places/tests.rs:31`), the rig the places and events tests share (`send/events/tests.rs` imports it).
- It passes the seven arguments `sync::run` took before typing.
- `sending()` is a copy of main's helper in `send/poll/tests.rs` as it was at the branch's base. From there to `3878dd66`, main's helper changed in one line only: `crate::feed::Typing::default(),` in its call (`send/poll/tests.rs:40`).
- Every other call at the head is main's and passes eight: jawd's `start` (`main.rs:571`), the two benches, and the media, send, link, poll and sync tests.
- Without the patch, `cargo check -p jawd --all-targets` on the rebased head fails with exactly one error: E0061, 7 arguments where 8 are taken, at `send/places/tests.rs:31`, in jawd's test target. jawd's binary itself builds.
- The branch adds no bench call and never touches `start`'s call.

**The patch** is `integration/places-on-main.diff` in the agent store: one line, `crate::feed::Typing::default(),` after `Default::default(),` (`send/places/tests.rs:39`).
- `git apply --check` takes it on the rebased head.
- `send/places/tests.rs` there is byte for byte the branch's own (`d7641d9f821e`), so the patch applies to any rebase that leaves that file alone.

**Test code only.** `send/places.rs:115` declares the file as `#[cfg(test)] pub(super) mod tests;`.

**A default `Typing` starts nothing, follows nothing and writes nothing.** I read this at the head, in `feed/typing.rs`:
- **Making one spawns nothing.** `Typing::default()` (`:110`) is `Typing::new(Timing::default())` (`:117`), which builds its state in memory: an intake behind a mutex, a `Notify`, a broadcast channel of 1,024 rounds and four counters. It holds no `Store` and no engine.
- **Only `start()` (`:180`) spawns its task.** That task is the only code of typing's that reads jaw.db, to name who's typing, and it never writes. Only jawd's `start` (`main.rs:595`) and typing's own tests call `start()`.
- **sync uses it in one place,** `typing.message(…)` for each message from someone else (`sync/mod.rs:375`). That returns at once while nothing is watched.
  - Only `heard()` adds to what's watched, and sync never calls it: the engine's typing events go nowhere (`sync/mod.rs:396`).
  - Only the engine's `hear_typing` calls `heard()`, and only jawd's `start` wires that up (`main.rs:563`). The rig doesn't, and its demo engine (`DemoEngine::sharing`) has no typists anyway.
- **Typing follows nobody.** Following presence is `presence::Following`'s job. The rig passes it `Default::default()`, which the landing-integration note found inert: it makes a channel and spawns nothing until `start()`.

So in the rig the default `Typing` does nothing at all. That's how sync behaved on the branch's base, before typing existed, so the places and events tests see what they saw when the m2 note approved them.

## The rule for the other waiting branches

**The rule:** "an inert default for a parameter main added to `sync::run`, at test and bench calls only, counts as keeping both sides". It can stand, so that a branch applying it needn't come back for an audit, when all of these hold:

1. **The parameter is main's.** Main added it in a landing an audit approved, and main's own test and bench callers pass the default. Today that's the seventh parameter, `following: presence::Following`, with `Default::default()`, and the eighth, `typing: crate::feed::Typing`, with `crate::feed::Typing::default()`.
2. **The default is inert at the landed tree.** Making it spawns no task, subscribes to nothing, sends nothing to the engine and writes nothing to jaw.db or any file. Whatever `sync::run` hands it may at most be held in memory, bounded, and the rig never reads it. That holds today for both:
   - `Following` holds at most 1,024 presences and drops the rest, counted.
   - `Typing` holds at most 1,024 changes and drops the rest; on main, nothing reaches it in a rig.

   A branch that changes the parameter's type, its `Default`, or what `sync::run` does with it isn't covered. Its own audit covers that.
3. **The calls are test and bench calls.** Each is in a `#[cfg(test)]` module, or in bench code that only `jawd bench …` reaches, on a scratch folder it removes. jawd's own call in `start` stays main's line as it is. A conflict there isn't covered.
4. **Nothing else changes.** The change adds exactly that argument and nothing else. The test at that call doesn't depend on what the parameter does: it was written before the parameter existed. A branch test that does depend on it (one that uses typing or presence in the rig) isn't covered.
5. **The compiler shows it's complete.** Without the change, `cargo check -p jawd --all-targets` fails exactly at those calls, with E0061. With it, clippy and the branch's tests pass. Each argument is folded into the commit that adds its call, so every commit builds.

**Why that counts as keeping both sides.** Main's side is the signature, kept whole. The branch's side is what its tests do, unchanged: with an inert default a test runs exactly as it did on its base, where the parameter didn't exist.

**How a lander records it.** Name the rule and this note in the landing report and in the commit's message. The commit keeps its own branch's `Data-Audit` trailer, which every data-layer commit already carries, so it doesn't need this note to be at its head. Anything beyond the five conditions needs another look.

**The seven branches,** as their heads stood at 16:40 UTC (the library and disappearing checked again at 17:33, after they moved):

| Branch | Head | Base | Calls needing the argument on `3878dd66` |
| --- | --- | --- | --- |
| contact cards (`send-contact`) | `360e8bf5` | `5230b9f4` | two |
| chat actions (`chat-actions`) | `f414e65c` | `9b3fbdf4` (media in, before typing) | one |
| the library (`library`) | `50d43bf2` | `3878dd66` | none |
| disappearing (`disappearing`) | `19f42e0d` | `3878dd66` | none |
| communities (`communities`) | `19e3851f` | `7e047803` (typing) | none |
| media out (`media-out`) | `778f373d` | `3878dd66` | none |
| presence (`presence`) | `439e4753` | `3878dd66` | none |

What each needs, and what falls outside the rule:
- **Contact cards:** both calls are test code, `send/contact/tests.rs:50` (from `1fd3eca1`) and T2's test, `store/write/links/tests.rs:379` (from `9057f44e`). Two of its landing's changes are beyond the rule:
  - its arm in the backfill dispatch (`store/write/links.rs`) needs media's fourth field, a data path the rule doesn't cover;
  - its demo cards' IDs, `…08` and `…09`, meet media's photo and this branch's event (T1).
- **Chat actions:** the call is test code, `a_full_app_state_refetch_that_fails_spends_no_try_while_whatsapp_cant_be_reached` (`sync/tests.rs:1473`, from `af56fdd4`). Nothing else from the rule: its change to what `sync::run` passes `refresh::run` is its own, and audited.
- **The library, disappearing and communities:** every call passes eight already.
- **Media out:** every call passes eight, its own `media/outgoing/bench.rs:87` and `media/outgoing/tests.rs:76` among them.
- **Presence:** `sync::run` still takes eight there. Presence changes `Typing` and has `sync::run` pass the engine's typing to `heard()`, which is its own audit's matter. A default `Typing` stays inert on its tree: at most 1,024 changes held in memory, with no task to take them.

The rule covers all seven, now and if main adds another parameter before they land, provided that parameter's default meets condition 2.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gap

- **T1 (follow-up, from the m2 note's L2): nothing tests that the demo account's messages each have an ID of their own.**
  - **Where:** `engine/demo.rs`, `DemoEngine::new()`'s script.
  - **What happens:** each branch takes the next free ID, and git merges the lists without a conflict. Contact cards' two cards are `…08` and `…09`, which are media's photo and, once this lands, this branch's event. A collision makes the later message take the earlier one's row.
  - **Why it doesn't block:** it's the demo's made-up account, never the owner's data.
  - **Fix:** a test that every message in `new()`'s script has an ID no other message has. Contact cards' lander should give its cards free IDs (`…0D` on).

### Unpredictable performance

None new. Round three's U3 growth, and U3 to U5, stand as follow-ups.

### Other

- **O1. L1 in the places commit makes `kept.rs` conflict in three later picks.**
  - **Where:** the events, steps and unopened commits (`3dfde819`, `8568591e`, `9284c76b`).
  - **What happens:** each adds its seed row at the end of `SEEDS`, where L1 put media's three rows.
  - **Fix:** keep both sides, the commit's row first and media's three last. Then `kept.rs` at the head is the trial's with L1, byte for byte (`6f9259dfb837`), and kept's test passes at every commit I ran it at.
- **O2. The patch belongs in the places commit, folded in rather than on top.**
  - **What happens otherwise:** from the places commit on, jawd's tests don't build without it. With the patch as a commit on top, neither L3's `JAW_PLANS=write` nor any test could run at the 18 commits from the places commit to the m2 note's, and `git bisect` would meet them unbuilt.
  - **Fix:** apply it at the places commit's stop, or put it in later with `git commit --fixup` onto the rebased places commit and `git rebase -i --autosquash`. The autosquash goes in without a conflict and gives the same tree; the events commit's later changes to `send/places/tests.rs` don't meet the line.
- **O3. `plans.txt` must be written at four commits where the rebase doesn't conflict in it.**
  - **What happens:** the set-aside naming, the merge fold, edits bounded and I3 commits (`5fc81894`, `45a9cfb9`, `912c2803`, `4ddc2793`) change it, and the merge gives them both sides' entries. From the events commit on, those entries sit in the order the merge put them, where the plan test compares the file byte for byte.
  - **In my rebase:** each merged file differs from the written one by 20 lines of order alone.
  - **Fix:** write it with `JAW_PLANS=write` at each of the eight commits, never resolving it by hand, and compare the blobs above.
- **O4. Four commit messages name the migrations by their old numbers.**
  - **Where:** places says "Migration 20", events "Migration 21", steps "Migration 22" and unopened "Migration 23". The code, the lock and the m2 note's landing call them 21 to 24.
  - **Why it doesn't block:** history only.
  - **Fix (optional):** reword those four when landing, or leave them; the m2 note's landing and this note record the numbers.
- **O5. Two of main's timing tests failed in the full run at a load of about 58, and each passed alone 3 of 3:** the long-request lane (`feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`) and the socket's connection-full test (`sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`). Both are on the landers' lists, and this range touches neither.

## Measurements

**Method:**
- **Scratch:** one folder, `/tmp/jaw-audits/places-landing`, deleted once this note was written. It held one clone of `~/src/jaw-m2more` (refs as of 15:56 UTC), with main's `3878dd66` in it and round three's `trial.bundle` fetched in.
  - Its `target` was the only build folder, built with `CARGO_INCREMENTAL=0`, with `TMPDIR` inside the folder.
  - Revisions were switched with `git checkout`.
- **Bounds:** every build and test ran under `nice -n 10` and `gtimeout --kill-after`, which signals the whole process group.
- **Doubled commands:** each step took a lock named for it, so a doubled command would have been refused. None was: 33 locks, and each of the 33 logs has one start and one end.
- **Machine:** Apple M1 Ultra, 20 cores, macOS 26.6.2, rustc 1.99.1. Other agents kept the load at 51 to 69.
- **What I didn't touch:**
  - I opened nothing under `~/Library/Application Support/jaw`, copied no `session.db`, and ran no Go engine; the tests ran their demo and scripted engines.
  - Nothing was sent to WhatsApp.
  - I signalled no process.
  - I only read the other branches, by `git show` and `git grep` in `~/src/jaw`.

**Runs:**

| Step | Result | Took | Load |
| --- | --- | --- | --- |
| `cargo check -p jawd --all-targets`, rebased head without the patch | exit 101: one error, E0061 at `send/places/tests.rs:31`, in jawd's test target | 76 s | 56 to 59 |
| `cargo clippy --workspace --all-targets -- -D warnings`, with the patch | 0, no warnings | 115 s | 57 to 60 |
| `cargo fmt --all --check` | 0 | 3 s | 60 |
| `JAW_PLANS=write cargo test -p jawd plans` at the head | written; both plan tests pass; 621 entries, as both sides make them | 107 s | 56 to 62 |
| `cargo test -p jawd --no-fail-fast` at the head (with the patch and the written `plans.txt`) | unit tests: 856 passed, 2 failed (O5), 15 ignored measurements. End to end: `installed` 1, `starting` 1, `stopping` 4 and `upgrading` 1, all pass | 1,461 s | 55 to 60 |
| O5's two tests alone, 3 runs each | 6 of 6 pass, in 5.3 to 9.9 s | 50 s | 60 to 68 |
| `cargo test -p jaw-proto` at the head | 16 unit tests and the 3 fixtures tests pass | 36 s | |
| The patch put in by `--fixup` and `--autosquash` | no conflict; the tree is the head's with the patch | | |
| Each of the 11 commits that don't change `plans.txt`, patch folded: `cargo check -p jawd --all-targets` | 11 of 11 exit 0 | 24 to 211 s | 51 to 69 |
| Each of the 8 that do: `JAW_PLANS=write cargo test -p jawd --bin jawd plans`, `plancheck.py`, then kept's test and both plan tests | 8 of 8 written; 0 entries not as both sides make them (587, 609, 611, 611, 611, 617, 621 and 621 entries); 3 of 3 pass at each | 59 to 349 s each | 51 to 69 |
| At the places and events commits: all of `store::schema`, and the demo account's place and event socket tests | places 16 passed, events 17 passed, 2 measurements ignored at each | 767 s, 716 s | |
| `scripts/check-data-audit.py 3878dd66 landed` | 0 | | |

**The schema's tests** include kept's test, the frozen migration texts and their fingerprints, a fault at every statement of every migration, and every earlier version keeping its rows.

**The upgrade test** generates a version-1 jaw.db as the spike left it, and runs the landed jawd on it with the demo engine through every migration to 24, converting its history and keeping layouts across a restart.

**What I didn't run:**
- **An upgrade from a jaw.db main's jawd made, killed along the way.** Round three's trial did that, from media in's version 20 to 24, and main's schema files are byte for byte the trial's base.
- **jaw's own tests and the Mac app's.** clippy builds jaw's tests, which is where the one resolution there (an import list) lives.

## Verdict

Nothing is marked data loss or inconsistency. The one change beyond keeping both sides is a test rig's argument. It's inert, and the build and the tests where it lives show it's enough. Every rule this landing touches has its test passing at the commits that bring it: kept's, the plans', the schema's, the demo account's, and the upgrade from version 1. T1 and O1 to O5 are follow-ups or landing guidance.

## Landing

From `~/src/jaw-m2more`, rebase `d5965757` onto `origin/main` as the table says:
- **At the places stop,** put in L1, L2 and the patch: `git apply "<agent store>/integration/places-on-main.diff"`.
- **`plans.txt`:** write it with `JAW_PLANS=write cargo test -p jawd --bin jawd plans`, never resolving it by hand.
  - During the rebase, do that at each stop that conflicts in it: the places, events, steps and unopened stops.
  - Then do it at the other four (O3): mark each `edit` in a `git rebase -i origin/main`, and write it again at any stop that conflicts in it on the way.
- **Check the blobs above.** At the rebased m2 note's commit, `git diff` against `landed` from `integration/places-landing.bundle` should be empty.

**The trailer:** the places commit (from `92e96e55`), which holds the patch, takes `Data-Audit: docs/audits/2026-10-31-places-landing.md`, beside the m2 note's trailer it carries.

**This note:** commit it as `docs/audits/2026-10-31-places-landing.md`, docs only, after the m2 note's commit. `make check-audit` passes only once it's committed.

**If main moves again, re-check:**
- **The migrations:** main's last is still 20, and `store/schema.rs` and `kept.rs` are as they are now. Otherwise move 21 to 24 up once more (lock lines and `// N,` comments only), and seed any new tables in `kept.rs` as L1 seeds media's.
- **`sync::run`:** it still takes these eight parameters, which the build says. A ninth means the rule again, at `sending()`, if its default meets condition 2.
- **The demo IDs:** `…0C` is still free, and every demo message has an ID of its own. If contact cards lands first with `…09`, this branch's event needs another ID (T1).
- **The range-diff:** against `landed`, it shows only main's context and the trailers.
- **`plans.txt`:** at the eight commits, each is what the plan test writes, holding main's entries with the commit's changes, and both sides' lines where both change one. `integration/places-landing-scripts/plancheck.py` checks it.
- **The tests:** kept's, the plans', the schema's, the places', the events', sync's, the merges' and the demo account's tests pass, as do the upgrade from version 1 and `make check-audit`.

Anything beyond keeping both sides in a data-layer path needs another look.

Verdict: approved

# Round two: the demo's Saturday

Range: `ff58460d` in `m2-more` (worktree `~/src/jaw-m2more`), 31 October 2026: round one's 19 commits as the lander rebased them onto `3878dd66`, and this note's commit. Auditor: database-expert.

Only the lander's fix to `saturday()` is in scope (`places-landing/saturday.diff` in the agent store), with a test for it. Everything else stands as round one and the m2 note approved it. Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at the head with the fix and the test folded in.

## In short

Approved. Nothing is marked data loss or inconsistency.

- **The slip is the events commit's, and only the demo's.** `saturday()` (`engine/demo.rs:697`), "Next Saturday at 9:00 UTC after `now_ms`", puts Rui's made-up event `(2 - d).rem_euclid(7).max(1)` days on, where `d` is the UTC day's weekday counted from Thursday. On a Saturday that's 0, made 1: the Sunday after.
  - jaw's end-to-end test of the demo's event wants a Saturday (`crates/jaw/tests/composing.rs:150`), so it fails all day every Saturday (UTC), as it did for the lander. It failed here too, at 19:58 UTC: "Sun 1 Nov, 09:00 – 15:00".
- **The fix is right on every day and at every hour.** `match … { 0 => 7, n => n }` gives 9:00 UTC on the first Saturday after the day `now_ms` falls on: as before from Sunday to Friday, and a week on from a Saturday, at any hour of it.
  - I compiled the old function and the fixed one on their own and ran them at every minute of 2020 to 2035 and every hour of 1900 to 2199 (11,045,112 instants). The fixed one always gives 9:00 UTC on a Saturday, one to seven days on and 9 to 177 hours ahead. It equals the old one at every instant not on a Saturday; on a Saturday it's the old one's Sunday plus six days.
  - **Against the doc:** "next Saturday", said on a Saturday, is a week on, and that's what `.max(1)` was for: never the same day. Read as "the next instant that's 9:00 UTC on a Saturday", a Saturday before 9:00 would give that same day, which the fix doesn't. The tests and the terminal take either, since any Saturday shows "Sat ". O6 has the comment say which.
- **A test, to go into the events commit with the fix:** `engine::demo::tests::the_demos_event_is_at_9_utc_on_the_first_saturday_after_today_a_week_on_from_a_saturday` (`engine/demo.rs:1777`). `saturday()` takes the time it's given, so the test pins it at ten instants from Thursday to the next Friday, and at every minute of three weeks. The ten include Saturday at midnight, just before 9:00, at 9:00 and just before midnight, and the Friday and Sunday either side.
  - Without the fix, it fails at "Saturday at midnight": `1793523600` (Sunday 1 November, 9:00 UTC) where it wants `1794042000` (Saturday 7 November).
  - Run on its own against six mutants of `saturday()`, it fails on each: the original `.max(1)`, the same Saturday, the same Saturday before 9:00, Sunday, 10:00, and milliseconds for seconds. With the fix, it passes.
  - It belongs in the events commit, beside the function it pins. The end-to-end test catches the slip only on Saturdays, and only where jaw's tests run.
- **It's the demo's alone.** `saturday()` gives only the start and end of the demo engine's made-up event. Nothing of a real account, the store's rules or the owner's data reaches it, and the demo's IDs and everything else it makes are as they were.
- **Nothing else in the branch has the same slip.** `saturday()` is the only day the branch works out. The rest of its date arithmetic is fixed instants, each on the weekday its name says, and conversions between seconds and milliseconds.
- **The tests pass with it.** At the head with the fix and the test folded into the events commit, these pass: the three composing tests, the 72 events and demo tests, jawd's other tests that run the demo engine, the upgrade from version 1, clippy and rustfmt. At the folded events commit, the new test and the demo account's tests pass, and at the next commit, the three composing tests.

## What was asked, checked

### Every day and every hour

What `ahead` gives, by the weekday (UTC) of the day `now_ms` falls on:

| Day of `now_ms` (UTC) | Before | With the fix |
| --- | --- | --- |
| Thursday | 2 | 2 |
| Friday | 1 | 1 |
| Saturday, at any hour | 1, the Sunday after | 7, the Saturday after |
| Sunday | 6 | 6 |
| Monday | 5 | 5 |
| Tuesday | 4 | 4 |
| Wednesday | 3 | 3 |

- **Either side of midnight UTC:**
  - From Friday 23:59:59.999, the event is that Saturday, 9 hours on. From Saturday 00:00, it's the Saturday after, a week and 9 hours on.
  - From Saturday 23:59:59.999, and from Sunday 00:00 a millisecond later, it's the same Saturday, six days and 9 hours on.
  - Before the fix, every instant from Saturday 00:00 to 23:59:59.999 gave the Sunday after.
- **Before and after 9:00 on a Saturday:** both give the Saturday after. The event is never on the same day, so it's always ahead, and never more than a week and 9 hours on.
- **What the terminal and the test expect:** jaw draws an event's start as `%a %-d %b, %H:%M` in the zone it has when it starts (`crates/jaw/src/fmt.rs:80`, `crates/jaw/src/view.rs:331`), with no "today" or "tomorrow". So the test's "Sat " is there whenever 9:00 UTC on a Saturday is a Saturday where the test runs: from UTC−9 to UTC+14 (O7).
  - East of UTC+9, the end (15:00 UTC) is the Sunday, which `planned()` writes on the same line, "Sat … – Sun …". That's still one line with "Sat ".
  - jawd works out `now` once, when the demo engine starts (`DemoEngine::new()`), and the terminal draws what it's given, so a run across midnight sees one Saturday throughout.

### Only the demo's

- `saturday()` is private to `engine/demo.rs`. Its one caller is `DemoEngine::new()`'s script, for the start and end of Rui's event `3EB0DEMO00000009` (`engine/demo.rs:471`), which go into that event's raw protobuf as `start_time` and `end_time`.
- `DemoEngine::new()` runs only for `--engine demo` (`main.rs:415`), and jawd's engine is Go unless told otherwise (`main.rs:47`). Its other callers are tests: the demo engine's own, and the store's ack test (`store/mod.rs:2043`).
- jaw doesn't link jawd, and jaw's own demo has no event. A debug build of the Mac app runs `jawd --engine demo` in a folder of its own and never opens the owner's (`mac/README.md`), and none of the app's tests names the demo's event.
- jawd stores the event as it stores any. The diff changes no statement, rule, table or ack, and the demo plays the same script in the same order, each message waiting for its ack as before.
- The diff changes only `ahead`. The demo's twelve IDs, `…01` to `…0C`, are each one message's. `…05` and `…09` appear again only where the votes and the answers name their poll and event, as round one found. On every day but a Saturday, the event's start and end are what they were; on a Saturday, they're six days later.

### The same slip elsewhere

I swept every line the branch adds, outside `docs/`, `plans.txt` and the test fixtures, for day and weekday arithmetic: `rem_euclid`, `div_euclid`, `86_400`, `3_600`, weekday names, "next", "tomorrow", `.max(1)`, chrono, `start_time` and `end_time`.
- `saturday()` is the only day the branch works out.
- The branch's other instants are fixed, each on the weekday its name or comment gives:
  - Saturday 31 October 2026 at 9:00 UTC in jaw's layout tests (`crates/jaw/src/layout.rs`);
  - Saturday 17 October 2026, 10:00 to 16:00 UTC, in the events, converter and store tests and the protocol's examples.
- The rest are conversions: seconds to milliseconds in the converter (`convert/kinds.rs:494`), and back in `send.event` (`send/events.rs:89`), which takes only starts after the epoch.
- jaw's `planned()` formats with chrono and works out no day. Main's `fmt::weekday` counts from Thursday, as it should.
- The composing test is the only test, in jaw, jawd or the Mac app, that names a weekday from the real clock.
- Not of the kind: the converter's `s * 1000` is unchecked, but that's main's (`convert/kinds.rs:461` at `3878dd66`), and the branch only adds the filter for values that aren't positive.

### Folding it in

- `integration/places-saturday.diff` in the agent store is the lander's `saturday.diff` line for line, with the test. It's 53 lines, takes `engine/demo.rs` from `c0c821e2` to `5f96b8fc`, and applies to `ff58460d`.
- No commit after the events commit touches `engine/demo.rs`: the file is `c0c821e2` from there to the head.
- In my clone I committed it as a `--fixup` of the events commit (`3c4ed927`) and ran `git rebase -i --autosquash origin/main`. It went in without a conflict:
  - The first three commits keep their hashes.
  - From the events commit on, each of the 17 trees is the tree of the commit it replaces, with `engine/demo.rs` as `5f96b8fc0815` and nothing else changed.
  - The messages are unchanged, and the events commit keeps its one m2 trailer.
  - The head is `ff58460d` with the patch, byte for byte: the tree I ran the tests on.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gap

- **T2 (closed by the test): nothing pinned `saturday()`.**
  - **Where:** `engine/demo.rs:697`, from the events commit.
  - **What happens:** only jaw's end-to-end test reached it, through a jawd and a terminal on the real clock, so the slip could show only on a Saturday (UTC). jawd's own test of the demo's event (`the_demo_engines_account_shows_an_event_and_its_answers_and_takes_yours_through_the_socket`) reads its name, place and answers, not its time.
  - **Fix:** the test above, in the events commit with the fix.

### Unpredictable performance

None. The test makes 30,250 calls of integer arithmetic, in under a millisecond.

### Other

- **O6 (optional, in the same fixup): the comment could say what a Saturday gives.**
  - **Where:** `engine/demo.rs:696`.
  - **What happens:** on a Saturday before 9:00, "Next Saturday at 9:00 UTC after `now_ms`" can be read as that same day. The fix and its test give the Saturday after.
  - **Fix:** for example, ``/// 9:00 UTC on the first Saturday after `now_ms`'s day (UTC), a week on from a Saturday, in seconds since the epoch.``
- **O7 (follow-up): the composing test depends on the zone it runs in.**
  - **Where:** `crates/jaw/tests/composing.rs:150`, from the terminal's event commit.
  - **What happens:** jaw draws the event in the machine's zone, and 9:00 UTC on a Saturday is a Friday west of UTC−9 (Hawaii, the Marquesas), where the test fails every day, with or without the fix. This Mac (GMT today) and GitHub's runners (UTC) are well inside the range.
  - **Fix:** pin the zone the test's jaw draws in.

## Measurements

**Method:**
- **Scratch:** one folder, `/tmp/jaw-audits/places-saturday`, deleted once this round was written.
  - It held one clone of `~/src/jaw-m2more` at `ff58460d`, with `origin/main` set to `3878dd66`, which `git ls-remote` showed was still GitHub's main at 19:36 UTC.
  - Its `target` was the only build folder, built with `CARGO_INCREMENTAL=0` and with `TMPDIR` inside the folder.
  - Revisions were switched with `git checkout`, and the fixup and autosquash were made on a branch of the clone's own.
- **Bounds:** every build and test ran under `nice -n 10` and `gtimeout --kill-after`.
- **Doubled commands:** each step took a lock named for it, so a doubled command would have been refused. None was: 14 locks, and each of the 13 logs has one start and one end.
- **Machine:** Apple M1 Ultra, 20 cores, macOS 26.6.2, rustc 1.99.1. Other agents kept the load at 39 to 65.
- **What I didn't touch:**
  - I opened nothing under `~/Library/Application Support/jaw`, copied no `session.db` and ran no Go engine. The tests ran the demo engine in folders of their own, with `JAW_ENGINE` naming nothing.
  - Nothing was sent to WhatsApp, and I signalled no process.
  - In `~/src/jaw-m2more` I only read: it's on `m2-more` at `ff58460d`, clean, before and after.

**Runs:**

| Step | Result | Took |
| --- | --- | --- |
| Without the fix, the test added: the new test | fails at "Saturday at midnight", `1793523600` for `1794042000` | 16 s, after a 135 s build |
| Without the fix: the composing test of the demo's event, at 19:56 UTC | fails, "Sun 1 Nov, 09:00 – 15:00" | 146 s |
| The old and fixed `saturday()` compiled on their own, at 11,045,112 instants | the fixed one always 9:00 UTC on a Saturday, one to seven days on; equal to the old one at every instant not on a Saturday | 5 s |
| The test's body on its own, against the fix and six mutants | passes with the fix; fails on each mutant | |
| With the fix: the new test | passes | 45 s, after a 77 s build |
| With the fix: `cargo test -p jaw --test composing` | 3 of 3 pass; the event shows "Sat 7 Nov, 09:00 – 15:00" | 46 s |
| With the fix: jawd's tests matching `event` and `demo`, and the store's ack test | 72 pass, 1 measurement ignored | 100 s |
| With the fix: jawd's two other tests that start the demo engine, the stop bound's and the failed start's | 2 of 2 pass | 90 s |
| With the fix: `cargo test -p jawd --test upgrading` | passes | 71 s |
| With the fix: `cargo clippy -p jawd --all-targets -- -D warnings` | 0, no warnings | 48 s |
| With the fix: `cargo fmt --all --check` | 0 | 3 s |
| The fixup and the autosquash | no conflict; the trees as above | |
| At the folded events commit: the new test, the demo engine's tests and the demo account's four socket tests | 11 of 11 pass | 62 s |
| At the folded terminal's event commit: `cargo test -p jaw --test composing` | 3 of 3 pass, "Sat 7 Nov" | 51 s |

**What I didn't run:** jawd's whole suite, and `make test`. Round one ran jawd's suite on this tree without the fix. The fix changes one demo function and adds one test, and every test that runs the demo engine ran here. The lander's gate runs the rest.

## Verdict

Nothing is marked data loss or inconsistency. The fix is right at every hour of every day, it reaches only the demo's made-up event, and the test that pins it fails without it. T2 closes with that test going in beside the fix; O6 is optional, and O7 is a follow-up.

## Landing

- **Fold the fix and the test into the events commit,** in `~/src/jaw-m2more`, on `m2-more` at `ff58460d`:
  1. `git apply "<agent store>/integration/places-saturday.diff"`, then `git commit --fixup=3c4ed927`. If O6 is taken, change the comment before committing.
  2. `git rebase -i --autosquash origin/main`, with a `break` after the events commit's fixup, to add its trailer at that stop.
- **The events commit's trailer:** it takes `Data-Audit: docs/audits/2026-10-31-places-landing.md` beside its m2 trailer, once each, with one `Co-authored-by`, as the places commit carries them. Amend it as the lander amended the places commit, with `trailer.ifexists=addIfDifferent`, so the shell's `Co-authored-by` isn't doubled.
  - A sentence for its message, if the lander wants one: "The demo's event is at 9:00 UTC on the first Saturday after the day the demo starts, a week on from a Saturday, where from a Saturday it fell on the Sunday after (docs/audits/2026-10-31-places-landing.md, round two)."
- **This note:** copy the store's working copy into the note's commit, the last one, as `docs/audits/2026-10-31-places-landing.md`, byte for byte, docs only. Its last `Verdict:` line is this round's.
- **Re-check before pushing:**
  - `git diff ff58460d <new head>` is the patch (with O6's comment, if taken) and this round of the note, and nothing else.
  - The first three commits keep their hashes. From the events commit on, each tree differs from the one of the commit it replaces only in `engine/demo.rs` (`5f96b8fc0815`, or O6's blob), and the last one's in the note too.
  - Every message is as it was, except the events commit's, which gains the trailer (and the sentence, if added). The places commit keeps its two trailers, and every commit has one `Co-authored-by`.
  - The gate at the new head: `make check-generated`, `make lint`, `make build`, `make test` and `make check-audit`. `make test` should now get to jawd's suite. Round one's O5 lists the two timing tests that fail under load and pass alone.
  - If main moves: round one's re-checks, and that no commit after the events commit touches `engine/demo.rs`.

Verdict: approved
