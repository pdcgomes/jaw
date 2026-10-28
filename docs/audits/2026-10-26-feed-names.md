# Audit: the names a rename reaches

# Round four: 279d781, round three's T3, and the trial rebase onto e2af6a1

Range: `1e6c580..279d781` in `feed-names` (worktree `~/src/jaw-names`), 28 October 2026. Auditor: database-expert. Rounds three, two and one are kept below as written. The rounds' verdicts are at the end in turn, and this round's is the note's last line. Line numbers are at `279d781` unless they say otherwise, and paths are under `crates/jawd/src/` unless they say otherwise.

The branch is four commits on main's `1e6c580`:
- `3788685`, `54a79c7` and `523d3e1` are round three's `e27eff2`, `cbe117f` and `9666937`, rebased. `git range-diff` finds each identical, patch and message, and the branch's diff over its base is round three's, byte for byte.
- Main's three commits from `32b8eed`, where round three's trial rebase went, to `1e6c580` are docs only: designs, the Mac's plan and the protocol's presence and typing. So round three's reading stands.
- `279d781` changes only `feed/tests.rs`:
  - Appendix F's test replaces Appendix D's, under the same name (`:1859-1897`).
  - Appendix E's two probes follow it (`:1899-2104`), each ignored, with a reason that says how long it takes and the command that runs it (`:2030`, `:2071`).
  - A script found each appendix in the file as printed, line for line, with only the two `#[ignore]` lines added.

There's still no schema, migration, PRAGMA, index, trigger, writer, transaction or acknowledgement change.

## Summary

- **T3 is closed.** Appendix F's test fails 3 of 3 against each mutation of `store/read.rs:343`, and passes 3 of 3 on the branch and on the trial rebase.
  - With a bare `mp.v`, the row that mentions Bea first keeps `@Ana Lopes` at `v` 7, where a fresh query has `@Ana Martins` at `v` 7, as round three predicted.
  - With `min(mp.v)`, both rows keep `@Ana Lopes`, at `v` 5 and 6, where a fresh query has `@Ana Martins` at 7.
- **The ignored probes are as printed, and run as their reasons say.**
  - Each run line rebuilds nothing and runs its one probe.
  - On the branch they pass in 40.5 s and 22.2 s, at a load of about 17.
  - Against each mutation they fail at seed 1 within their first eight steps: `min(mp.v)` at steps 0 and 3, the bare `mp.v` at 0 and 7, as in round three.
- **Nothing is lost and nothing is inconsistent, on the branch or on `e2af6a1`.**
  - The branch still writes nothing.
  - On main, `people` and `person_jids` have the same seven writers as on the branch, and every write to a message that changes whom its row names sets its `v` to a new seq.
  - What main's rows newly show, a poll's tally and a link's thumbnail, moves the message's own `v`, which the branch's max keeps.
- **The trial rebase onto `e2af6a1` has two conflicts, in the first two commits, and one right resolution** (below):
  - In `3788685`, main and the branch each add a 28th column to `MESSAGE_COLUMNS`. Keep both: main's thumbnail at 27, then the quoted JID at 28, 29 columns in all, and `quoted` read from 28.
  - That last line merges without a conflict, still reading 27, and compiles that way, so it has to be fixed by hand. Left as it is, it fails the quote test, among six feed tests.
  - `plans.txt` conflicts in both commits: take main's side, then write it again with the plan test.
  - `523d3e1` and `279d781` replay with no conflict.
- **On the result, round two's guarantees hold.** The probes, with counters, give round two's counts exactly:
  - 417,929 comparisons with no `v` going back, and each of the 28,862 rows that showed something new at a higher `v` (154 of them had moved chat in a merge);
  - 600 re-watches from a seq, each matching a fresh query.
- **On the result, every test passes but main's own timing tests under load.**
  - The plan test passes, and Appendix F passes 3 of 3.
  - In my two runs of the suites, one of main's checkpointer tests failed in the first and one of its settings socket tests in the second. Each passes alone 3 of 3.
  - `cargo fmt --check` and clippy pass.
- **The presence interaction is a cost, not an inconsistency, and this branch needs nothing more for it.** What it needs is on `folders`, with one test for whichever of the two lands second (below).

## T3, against its rule

Each mutation is a one-line edit of `store/read.rs:343`, built into its own debug binary, which was checked to hold its own version of that line and no other. In Appendix F's test Bea's rename is seq 7 and Ana's is 8, and the connection watches again from 7.

| `store/read.rs:343` | Appendix F's test | after watching again, the interface holds | where a fresh query has |
| --- | --- | --- | --- |
| `max(mp.v)`, the branch | passes 3 of 3, here and on the trial rebase | what a fresh query has | both rows at Ana's rename's `v` |
| `min(mp.v)`, the oldest | fails 3 of 3 | g1 at `v` 5 and g2 at `v` 6, each with `@Ana Lopes` | each at `v` 7, with `@Ana Martins` |
| a bare `mp.v`, the first mention's | fails 3 of 3 | g1 right, at `v` 8; g2, which mentions Bea first, at `v` 7 with `@Ana Lopes` | g2 at `v` 7, with `@Ana Martins` |

With the oldest version, neither row rises past the seq it's watched from. With the first mention's, the row that mentions Bea first stays at Bea's rename. Either way the interface keeps Ana's old name.

## The probes

| | the branch, by its run line | `min(mp.v)` | a bare `mp.v` | the trial rebase |
| --- | --- | --- | --- | --- |
| `a_rows_version_never_goes_back_and_moves_whenever_what_it_shows_changes` | passes, in 40.5 s at load 17-18 | fails at seed 1, step 0, in 0.4 s | fails at seed 1, step 0, in 2.0 s | passes, in 159.5 s at load 53-61 beside a build, and in 54.5 s at load 45-48 with counters |
| `windows_watched_again_from_their_seq_match_fresh_queries_through_many_mentions_and_joins` | passes, in 22.2 s at load 17 | fails at seed 1, step 3, watched again | fails at seed 1, step 7, watched again | passes, in 38.5 s at load 57-61, and in 25.2 s with counters |

- The run lines are `cargo test -p jawd -- --ignored a_rows_version` and `cargo test -p jawd -- --ignored windows_watched_again`, and I ran them as printed.
  - Cargo finished in under a second without compiling.
  - It ran the one probe in the unit tests' binary (433 filtered out) and nothing in the three integration tests' binaries.
  - Each filter names only its probe among jawd's ignored tests.
- The reasons' "about a minute" and "about half a minute" in debug are right: 40 to 60 s and 22 to 39 s, here and for the builder.

## The trial rebase onto e2af6a1

I ran `git rebase e2af6a1` from `279d781` in my clone. Lines in this section are on the result unless they say otherwise.

### 3788685: both sides add a column to MESSAGE_COLUMNS

- Main's links work added a 28th column, the link preview's kept thumbnail: `iif(instr(m.detail, '"thumbnail":"') > 0, (SELECT h.sha256 FROM thumbnails h WHERE …), NULL)`. `message_row` reads it as column 27 (`store/read.rs:540`), and `MESSAGE_COLUMN_COUNT` became 28.
- The branch adds its own 28th column, `m.reply ->> 'from'`, read as `quoted: r.get(27)?`, and sets the count to 28 as well.
- Git conflicts on the end of the columns and the count, which sit together. It merges `quoted: r.get(27)?` without a conflict, since main's change to `message_row` is elsewhere.

The right resolution keeps both columns and moves one reader:
- `MESSAGE_COLUMNS` ends with main's thumbnail column, a comma, and `m.reply ->> 'from'` (`:353-354`).
- `MESSAGE_COLUMN_COUNT` is 29 (`:357`). `search` reads each hit's `t` and highlight after the columns, so the count has to be right.
- `quoted: r.get(28)?` (`:589`). Main's `previewed` goes on reading 27 (`:540`).
- The rest of main's side of the conflict stays as it is: the poll statements, `Tally` and `Polls`.

So the end of `MESSAGE_COLUMNS`, and the count after it, read:

```rust
        iif(instr(m.detail, '\"thumbnail\":\"') > 0, (SELECT h.sha256 FROM thumbnails h
            WHERE h.sha256 = m.detail ->> '$.link.thumbnail'), NULL),
        m.reply ->> 'from'";

/// How many columns MESSAGE_COLUMNS has, for queries that select more after them.
const MESSAGE_COLUMN_COUNT: usize = 29;
```

and the last field `message_row` reads is `quoted: r.get(28)?,`.

Putting the quoted JID first and the thumbnail at 28 would be as right, but would change main's line too.

Each likely slip compiles, since both columns are `Option<String>`, and each is caught:
- **`quoted` left at 27, reading the thumbnail.** In my build of it, six feed tests fail. They include the quote test, `a_window_is_told_the_new_name_of_someone_its_rows_only_quote`, and the random-writes test at seed 4, step 124, where round one's mutation leaving quotes out failed.
- **The count left at 28.** `search` would read each hit's quoted JID as its `t`, an integer, and fail any search that finds something. The store's, rules' and sync's search tests call it. I reasoned this one and didn't build it.

### plans.txt, in 3788685 and 54a79c7

The plan test writes this file, so the right resolution is to take main's side (`git checkout --ours`) and write it again once the code is resolved, with `JAW_PLANS=write cargo test -p jawd plans`. I did that in each of the two commits. Each then built with no warning, and its plan test passed.
- In `3788685` the file changes as the original commit changed it: the five message statements gain the quoted JID after main's thumbnail column, and `jids_of` is pinned.
- In `54a79c7` the message statements plan as on the branch, with main's thumbnail lookup added (`SEARCH h USING PRIMARY KEY (sha256=?)`).
  - Their subqueries are numbered one higher, which is why 171 lines change where 159 did on the branch.
  - The search's statement is pinned as on the branch. Main's `is_statement` takes the branch's one-line change without a conflict.
- The plans have no new scan. Their only temporary b-trees are the reactions' grouping and ordering, which the plan test allows, as on main.

### The rest

- `54a79c7`'s `read.rs` merges without a conflict, and correctly. It's the branch's change on top of main's `message_row`, polls and thumbnails included: the row's `v` as the newest of its message's and its people's, the quote's person by a join, and the mentions' newest `v`.
- `523d3e1` and `279d781` replay with no conflict, and `git range-diff` finds each identical.
- Over `e2af6a1`, every file's change is the branch's over `1e6c580`, except `plans.txt` and three lines of `read.rs`: where the quoted JID's column goes, the count, and the index.
- The fourth commit's tree is `3267c7b3ca7541e498c072a3e6780bb401665857`. A rebase onto `e2af6a1` resolved this way gets the same tree, before this note is committed.

### Main's code since 1e6c580, against the branch's rule

What round two's reasons rest on holds on the result. Lines here are main's.
- **Who writes `people` and `person_jids`.** The same seven statements as on the branch, in `ensure_jid`, `upsert_person` and `join` (`store/write.rs:1164-1167`, `:1289-1308`, `:1329-1349`).
  - Main changed their order: a person's new JIDs go in before the joins (`:1295-1301`), `rehome` runs after them (`:1303`), and `join` first gives the polls both people voted in a new version (`:1327`).
  - The kept person's `v` is still set last (`:1305`), to a seq of the same transaction.
  - Nothing deletes a `person_jids` row, and no trigger writes either table.
- **Who writes `messages`.** Every update that changes what a row shows sets `v` to a new seq. Main's new ones are among them:
  - the kinds backfill's conversion (`store/write/kinds.rs:44-52`), which rewrites spans and quotes but never the sender;
  - a poll counted again, after a vote or a join, and a poll's `max` (`store/write/polls.rs:61-64`);
  - a kept thumbnail (`store/write/links.rs:43`).

  The one update outside tests that sets no `v` starts a disappearing message's timer (`store/write/outbox.rs:179`). It was there at the base, and names nobody.
- **What rows newly show.**
  - A poll's tally counts each person once through `person_jids`, so a join changes it. Main's join moves those polls' own `v`, and main's test of it passes on the result (`a_join_that_makes_two_voters_one_reaches_a_window_of_their_poll_and_a_reconnect_from_before_it`).
  - A kept thumbnail moves its message's `v`.
  - Find's message hits are the windows' rows now (main's `1c13a65`), so they carry the branch's `v` too, and main's test of that passes.
- **System lines name people in their stored sentence**, by number until names are resolved (`convert/mod.rs:839-879`), so a rename changes nothing they show. A later change that resolves those names as the row is read comes under this branch's rule, as `MESSAGE_COLUMNS`' comment says.
- jaw bundles the same SQLite at both heads (`libsqlite3-sys` 0.38.2).

## The presence interaction

On `folders` as it is now (`cc585c0`, 27 October), `people.v` is the roster row's version. Each of these writes moves it and records a `person` event:
- a change of presence (`store/write/folders.rs:617-646`), typing and recording among them (`:630-633`), from WhatsApp's chat presence (`presence.rs:216`);
- following someone or not (`:662`, `:670`), and forgetting everyone's presence (`:684`);
- a new about (`:715`);
- `touch_person` (`store/write.rs:1143`), when a person's one-to-one chat or its avatar changes (`:1241`), or they're filed in a folder (`folders.rs:303-308`).

Once both land, each of those writes would:
- move the `v` of every message row naming that person, as its sender, the writer it quotes or someone it mentions. Every window holding such rows would send them again to every interface, a watch from a seq would bring them again, and the terminal and the Mac would lay them out again;
- mark every messages window whose rows name the person, and read it again. Main's rule already does that for the rows they sent, and this branch adds those that quote or mention them.

Nothing is lost or shown wrong, since a row upserted at a higher `v` with nothing new is the same row again. It's a cost, paid at presence's rate. Typing would be the most of it, since `folders` still writes typing into `people`. Main's protocol now says typing is never written and moves no `v` ("Nothing written", under "Presence and typing in chats"). Each start and pause of someone's typing would send again, in every open window, every row they sent, are quoted in or are mentioned in.

The coordinator's decision fixes this once it covers all of the above:
- **Every write above that changes no name moves a version of the roster's own, not `people.v`.** That's presence, but also following, forgetting, about and `touch_person`: each is rarer than presence, but each would send every row naming the person again.
- **Typing leaves `people` altogether**, as the protocol says.
- **The events, if those writes still record a `person` event.** The messages windows naming the person would still be read again for nothing, though none of their rows is sent. An event of the roster's own, which messages windows don't follow, would spare those reads. That's `folders`' call.
- **The protocol.** Main's protocol says presence is "sent when a person's `v` changes"; that sentence changes with the rest.

This branch needs nothing more for it:
- Its rule is right whatever moves `people.v`. A move with nothing new shown costs pushes, and loses no name.
- The rule relies on the other direction, that whatever changes a name moves `people.v`. That's tested: round two's table, and Appendix F.
- The test against the cost can only be written where both are. It belongs in two places:
  - **On `folders`, with the change:** a store test that presence, typing, following, forgetting, about, a one-to-one chat's avatar and filing a person each leave `people.v` as it was. It means something on `folders` alone, and it pins what this branch's rule reads.
  - **In whichever lands second**, so in `folders` if this branch lands first: a feed test that a presence change of someone a window's rows send, quote and mention moves no message row's `v` and sends no message row. With this branch and `folders` as it is now, it would fail, by the code above. I didn't build the two together.
- H6 below would put the contract into main's code before `folders` rebases onto it.

## Findings

### Data loss: none

The branch writes nothing, and the trial rebase changes no writer.

### Inconsistency: none

Round two's reasons hold on the branch and on the trial rebase (above), and the probes measured it on the trial, with round two's counts.

### Test gaps: none left

T3 is closed. Every part of the rule has a test in the default suite that fails without it. Appendix E's probes are in as ignored tests, and they fail against both mutations.

### Unpredictable performance: none on this branch

The read path is the one rounds two and three measured, so their measurements stand. On the trial rebase, the message statements gain only main's thumbnail lookup, a search by primary key. The presence interaction's cost lies in `folders`' writes (above).

### Other (follow-ups; none blocks)

- **H5 is closed.** The probes are ignored tests, with reasons that say how long they take and how to run them.
- **H6, new: say the other half of the contract.** `MESSAGE_COLUMNS`' comment says that whatever changes how a row names someone must move that person's `people.v` (`store/read.rs:326-328`). Adding that nothing else should, since each move sends every row naming them again, would put that on main where `folders`' builder will find it.
- **H4 stands:** the plan test finds statements by their first word.
- **Round one's H1, H2 and H3 stand**, and so do its three items on main: a chat row's `v` under a rename, an event and a new `v` when `me` changes, and an event for the chat being merged away at each step of a merge.

### Outside this branch

The failures in my runs were main's tests, under the load of other agents' builds, and each passed alone:
- `store::tests::the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it`, saying "the checkpointer never flushed jaw.db", failed in the run of the feed, socket and store tests at load 40 to 64. It passed alone 3 of 3, at load 61.
- `sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`, a settings test, waited out its 10 s for an answer in the run of every test at load 50 to 54. It passed alone 3 of 3, in 3.2 to 3.7 s at load 52. The kinds audit's H12 has main's settings socket tests slowed by `jawd status`.

## Measurements

### Method

- One clone of the worktree, `/tmp/jaw-audits/feed-names-4r/jaw`, at `279d781`, and a branch `trial` in it rebased onto `e2af6a1`. Its `target` was the audit's one build folder.
- Every build was a debug build, the profile `make test` uses, with `CARGO_INCREMENTAL=0` and `nice`. Each binary was copied out as its build finished.
- Each mutation was an exact one-line edit of `store/read.rs:343`, built, copied out, and checked out again. Each copied binary was checked to hold its own version of the line and no other. The rebase's slip was the same kind of edit at the trial's `:589`.
- The counts came from the trial's probes with four counters and two `eprintln!` lines added, which I then checked out again.
- `TMPDIR` was under the scratch folder. Nothing touched the owner's data, `~/Library/Application Support/jaw`, or any process but mine. `folders` was read from its branch with `git show`. The folder is deleted.
- Load (the one-minute average) was 16 to 64 while I worked, with other agents building beside me. Each figure says its own.

### The runs

| | the branch, `279d781` | the trial rebase |
| --- | --- | --- |
| a debug build of jawd's tests | 66 s, from nothing | 50 s |
| the plan test | passes | passes, and in each of the two commits that conflict |
| Appendix F's test | passes 3 of 3 | passes 3 of 3 |
| Appendix E's probes | pass, by their run lines (above) | pass, with round two's counts |
| the 248 feed, socket and store tests that aren't ignored | not run | 247 pass, in 288 s at load 40-64; the other is main's checkpointer test |
| every jawd test | not run | 524 of the 525 unit tests that aren't ignored pass, in 379 s at load 50-54, the other being main's settings socket test; the four integration tests pass |
| `cargo fmt -p jawd --check` and `cargo clippy -p jawd --all-targets -- -D warnings` | not run | pass |

I didn't run `make check-generated` or the other crates' tests, which the branch doesn't touch.

# Round three: 9666937, round two's T2, and the rebases onto 3d60818 and 32b8eed

Range: `3d60818..9666937` in `feed-names` (worktree `~/src/jaw-names`), 27 October 2026. Auditor: database-expert. Rounds two and one are kept below as written. Round two's verdict is the section before this round's, at the end, and this round's verdict is the note's last line. Line numbers are at `9666937`, and paths are under `crates/jawd/src/` unless they say otherwise.

The branch is three commits on main's `3d60818`:
- `e27eff2` and `cbe117f` are round two's `06d0968` and `5a9e6a4`, rebased from `4126db2`. `git range-diff` finds each identical, patch and message, so round two's reading of them stands.
- `9666937` adds Appendix D's test, byte for byte as printed: `a_window_watched_again_shows_the_new_name_of_the_older_of_two_people_a_row_mentions` (`feed/tests.rs:1859-1888`). It changes nothing else.

There's still no schema, migration, PRAGMA, index, trigger, writer, transaction or acknowledgement change. Main's code since `4126db2` (each interface's layout, and the WAL's bound) writes neither `people` nor `person_jids`. So the writers a row's `v` depends on are the three round two read, five lines lower in `store/write.rs` (`:1091`, `:1194`, `:1253`), and the seqs they take come from `Stamps` as before.

## Summary

- **Nothing is lost and nothing is inconsistent.** The branch still writes nothing, and its read path is byte for byte the one round two audited.
- **T2 is closed for the oldest version, but not for a version taken by its place in the spans (T3, a test gap, must change).**
  - With `min(mp.v)` at `store/read.rs:343`, Appendix D's test fails 3 of 3 and the branch's 70 other feed and socket tests pass, as the builder says.
  - With the aggregate left off, a bare `mp.v`, every test of the branch passes 3 of 3, Appendix D's among them. Across all of jawd's unit tests, only my probe fails.
  - The reason: SQLite fills a bare column in an aggregate from one row, here the first mention it reads. In Appendix D's test Ana is both mentioned first and renamed last, so the first mention's version happens to be the newest.
  - What that misses: a row mentions Bea, then Ana. Bea is renamed, then Ana. The row keeps Bea's version, and a window watched again from its seq keeps `@Ana Lopes`.
  - The gap is in the test I wrote in round two. The fix is Appendix F, Appendix D's test with a second row that mentions the two the other way round. It fails 3 of 3 with either mutation, passes 3 of 3 on the branch, and passes `cargo fmt --check` and clippy.
- **Round two's results hold on `9666937`, and on it rebased onto `32b8eed`.** On each, Appendix E's probes give round two's counts exactly:
  - 417,929 comparisons with no `v` going back, and each of the 28,862 rows that showed something new came with a higher `v` (154 of them had moved chat in a merge);
  - 600 re-watches from a seq, each matching a fresh query.
- **The rebase onto `3d60818` is as the builder says.** It had no conflict, and main's changes to the four files both touch are in other places in each.
- **The trial rebase onto `32b8eed` is clean, and there's nothing to resolve.**
  - The three commits replay with no conflict, each identical to its original, and the branch's diff over main comes out byte for byte the same. Main's six commits since `3d60818` touch none of the branch's files.
  - On the result, every jawd test passes but one of main's timing tests in the full suite under load, which passes alone. The plan test, the 40-seed sweep and the probes pass too.
- **No new scan or temporary b-tree, and every statement the branch touches is pinned.** The six message statements and `jids_of` are in `plans.txt`, and the plan test passes at both heads. The only temporary b-trees in them group and order one message's reactions, which the plan test allows by name, as on main.
- **Appendix E: leaving it out of `make test` is acceptable once Appendix F is in.** Then every part of the rule has a test in the default suite that fails without it. Keeping both probes as ignored tests, run by hand like the 40-seed sweep, is worth doing (H5).
- **The builder's gate.** Its failures were main's timing tests. They fail here too under load and pass alone, and I found no failure the branch causes.

## Round two's T2, against its rule

Each mutation is a one-line edit of `store/read.rs:343`, built into its own debug binary, the profile `make test` uses. "Fails" and "passes" mean 3 of 3 unless the cell says otherwise. The plan test isn't counted, since it fails on any change to a statement's text until `plans.txt` is written again.

| `store/read.rs:343` | Appendix D's test, as adopted | Appendix F's test | the branch's 70 other feed and socket tests | Appendix E's two probes, once each |
| --- | --- | --- | --- | --- |
| `max(mp.v)`, the branch | passes | passes | pass | pass |
| `min(mp.v)`, the oldest | fails: the row keeps `@Ana Lopes` at `v` 5, where a fresh query has `@Ana Martins` at `v` 6 | fails | pass, but for one timing test once, at load 67 | fail at seed 1, at steps 0 and 3 |
| a bare `mp.v`, no aggregate | **passes** | fails: the row mentioning Bea first keeps `@Ana Lopes` at `v` 7, where a fresh query has `@Ana Martins` at `v` 7 | pass; and of all 426 of jawd's own unit tests that ran, none failed but two timing tests at load 99, which passed alone 3 of 3 | fail at seed 1, at steps 0 and 7 |

Why a bare `mp.v` takes the first mention:
- In an aggregate query with no `min()` or `max()`, SQLite takes a bare column's value from one of the group's rows, and promises nothing about which.
- The mentions' subquery runs `json_each` over the spans as its outer loop, then finds each mention's person by key (`plans.txt:246-248`). So the row SQLite keeps is the first mention's.
- In the system's SQLite 3.51, with that plan, it was the first mention in each of four rows of two and three mentions, whichever was newest. jaw's own SQLite, 3.53.2, did the same in the mutant (the table above).
- So a row whose first mention is the newest gets the right `v` by accident, and that is Appendix D's only row.

The branch's other tests of names and windows can't tell the newest from any other choice: as round two found, their rows mention one person at most.

## Appendix E: in `make test`, ignored, or left out

My call: leaving the two probes out of the default suite is acceptable once Appendix F's test is in, and nothing more must be in `make test`. Adding them as ignored tests, as the builder offers, is better.
- With Appendix F, each part of the rule has a test in the default suite that fails without it:
  - the sender's, the quoted writer's and the mentioned people's parts: the three re-watch tests;
  - the newest of the mentioned people, in either order: Appendix F;
  - the message's own `v`, and the writers moving `people.v` on a join, a new JID, a push name or a verified name: the randomized test, `windows_match_fresh_queries_through_renames_joins_new_people_and_edits` (round two's table).
- What only Appendix E checks is breadth: rows mentioning up to three people, merges of one-to-one chats, and the design's rule itself, that a row's `v` never goes back and moves whenever what it shows changes. That's worth running by hand before changing this rule, as the 40-seed sweep is run.
- Alone in debug they took 51-53 s and 26-27 s here, at load 66-77, as the builder found: about 2.6 s a seed for each. Every mutation above fails them at seed 1, within its first eight steps. So a two-seed version of each in the default suite would cost about 5 s and still catch these mutations, if the suite should keep one. That's a choice, not a must.

## The rebases

### `4126db2` to `3d60818`

- `git range-diff 4126db2..5a9e6a4 3d60818..cbe117f` finds `e27eff2` identical to `06d0968`, and `cbe117f` to `5a9e6a4`.
- Main's changes to the four files both touch are elsewhere in each:
  - `store/plans.rs`: main added two reasons to `ALLOWED` (`:66-73`); the branch changed `is_statement` (`:208`).
  - `store/plans.txt`: main added the layout's plans (at its lines 180, 455 and 519); the branch changed the message statements' (227-425).
  - `store/read.rs`: main added two layout reads, `chat_named` and `interface_state` (`:696-726`); the branch's changes are at `:13-639`.
  - `protocol/README.md`: main changed the layout's sections and a few others; the branch changed line 73 alone.
- The plan test passes at `9666937`, so the layout's statements and the branch's are pinned side by side.

### `9666937` onto `32b8eed`, tried in my clone

- `git rebase 32b8eed` replayed the three commits with no conflict. `git range-diff` finds each identical to its original, and the result's diff over `32b8eed` is byte for byte `git diff 3d60818 9666937`.
- Main's six commits since `3d60818` change `store/mod.rs` (the writer's WAL bound and the checkpointer), the demo engine and account, `main.rs`, the design and their audit. `store/write.rs` isn't among them, and neither is any file of the branch's.
- On the result, with Appendix F in place:
  - the plan test passes;
  - 426 of the 427 unit tests that aren't ignored pass, and the four integration tests. The one that failed is main's `indexing_for_search_waiting_for_the_wal_gets_room_with_nothing_else_committing` (34.2 s against its 30 s), which passed alone 3 of 3;
  - the 40-seed sweep passes, and Appendix E's probes give round two's counts.
- So the builder can rebase as it stands; no line this note cites moves.

## Findings

### Data loss: none

The branch writes nothing.

### Inconsistency: none

Round two's reasons a row's `v` never goes back, and a watch from a seq gets every new name, hold on the rebased code:
- The read path (`store/read.rs:324-460`, `:630-639`) is round two's, byte for byte.
- The writers of `people` and `person_jids` are the three round two read (`store/write.rs:1091`, `:1194`, `:1253`), and main added none. `Stamps` hands out seqs as before.
- The probes measured it again at both heads (above).

### Test gap (must change before landing)

**T3. A row's `v` takes the newest of its mentioned people's versions, whatever their order** (`store/read.rs:343`).
- Appendix D's test pins the newest against the oldest, but not against a version taken by its place. With a bare `mp.v`, which SQLite fills from the first mention, every test of the branch passes 3 of 3, Appendix D's among them.
- Leaving the aggregate off is an easy slip, and SQLite accepts it without a word.
- What it would miss: a row mentions Bea, then Ana. Bea is renamed, then Ana. The row keeps Bea's version, and a window watched again from its seq keeps `@Ana Lopes` until the row changes. That is R1's inconsistency, back for the second person a row mentions.
- Fix: replace the test at `feed/tests.rs:1859-1888` with Appendix F's. It adds a row mentioning the two the other way round, so the first mention's version and the last's are each wrong for one of the rows. It fails 3 of 3 with the bare `mp.v` and with `min(mp.v)`, passes 3 of 3 on the branch, and passes `cargo fmt --check` and `cargo clippy -p jawd --all-targets -- -D warnings`.

### Unpredictable performance: none

The read path is unchanged, so round two's measurements stand. A 100-row window's read is 1% to 9% slower at p50, a 50-hit search page about 3%, and `jawd bench feed` shows no difference, all far inside the budgets. Appendix F adds a row to one test.

### Other (follow-ups; none blocks)

- **H5. Appendix E's probes, as ignored tests.** As above. An ignore reason saying how long they take and how to run them, as the 40-seed sweep's does, would do.
- **H4 stands.** The plan test finds statements by their first word (`store/plans.rs:207-222`), so it skips SQL built any other way, silently.
- **Round one's H1, H2 and H3 stand.**
- **So do round one's three items on main**, inconsistencies outside this branch: a chat row's `v` under a rename, an event and a new `v` when `me` changes, and an event for the chat being merged away at each step of a merge.

### Outside this branch

The gate's timing failures are main's tests. In my runs, at load 58 to 99:
- `a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce` failed once in ten runs of the feed suite, at load 67: one lone message of ten took 72.7 ms.
- `indexing_for_search_waiting_for_the_wal_gets_room_with_nothing_else_committing` failed in both of my runs of every jawd test, at 34.2 s and 38.1 s against its 30 s. It passed alone 6 of 6.
- `a_burst_in_a_chat_is_one_notification_with_its_newest_message` failed once, at load 99, and passed alone 3 of 3.
- The builder's `a_read_whose_task_panics_leaves_nothing_under_way_for_its_window_to_wait_on` is main's, from the feed audit's changes (`75217e5`). It passed in each of my ten runs of it.

## Measurements

### Method

- One clone of the worktree, `/tmp/jaw-audits/feed-names-3/jaw`, switched with `git checkout` between `9666937` and my trial rebase of it onto `32b8eed`. Its `target` was the audit's one build folder. Every build was a debug build, the profile `make test` uses, with `CARGO_INCREMENTAL=0` and `nice`, and each binary was copied out as its build finished.
- Each mutation was an exact one-line replacement at `store/read.rs:343`, built, copied out, and checked out again. Each copied binary was checked to hold its own version of that line, and no other.
- My probes were appended to `feed/tests.rs`: Appendix E's two, with counters, and a test of two rows mentioning Ana and Bea in either order. For the last builds, Appendix F replaced Appendix D's test in place, as printed below.
- `TMPDIR` was under the scratch folder. The integration tests start jawd with temporary `JAW_HOME`s and a silent stand-in engine. Nothing touched the owner's data, `~/Library/Application Support/jaw`, or any process but mine. The folder is deleted.
- Load (the one-minute average) was 52 to 99 while I worked, with other agents building beside me. Each figure says its own.

### The suites

| | `9666937` | the trial rebase |
| --- | --- | --- |
| the plan test | passes | passes |
| the feed and socket modules: 55 feed tests, the 40-seed sweep ignored, and 16 socket tests, with my two-order test | 72 pass, in 77 s at load 76-77 | 72 pass, in 94 s at load about 70 |
| every jawd test, with Appendix F | not run | 426 of the 427 unit tests that aren't ignored pass, at load 64-68; the four integration tests pass |
| the ignored 40-seed sweep | not run | passes, in 162 s at load 53-59 |
| `cargo fmt -p jawd --check` and `cargo clippy -p jawd --all-targets -- -D warnings`, with Appendix F | not run | pass |

I didn't run `make check-generated` or the other crates' tests, which the branch doesn't touch.

### The mutations

The table under "Round two's T2" has them. In all, against each mutant the feed and socket modules ran three times (72 tests each time), Appendix E's probes once each, and, against the bare `mp.v`, every jawd unit test once. Appendix F's test, as printed, ran three times against each mutant and three times on the branch.

The SQLite check ran in the system's `sqlite3` 3.51.0, in memory, with `people` and `person_jids` as jaw has them and four rows mentioning two or three people in different orders, whose plan was jaw's: `json_each`, then the two searches by key. A bare `mp.v` gave the first mention's version in every row, `max` the newest and `min` the oldest. jaw bundles SQLite 3.53.2 (`libsqlite3-sys` 0.38.2).

### Appendix E's probes, alone, in debug

| | the trial rebase | `9666937` | the builder |
| --- | --- | --- | --- |
| `a_rows_version_never_goes_back_and_moves_whenever_what_it_shows_changes` (20 seeds of 200 commits) | 53 s, load 71→67 | 51 s, load 77→75 | 46-63 s, load 53-72 |
| `windows_watched_again_from_their_seq_match_fresh_queries_through_many_mentions_and_joins` (10 seeds of 150 commits) | 26 s, load 67→66 | 27 s, load 75→70 | 22-27 s |
| what they found | 417,929 comparisons, 28,862 rows showing something new, 154 moved chat; 600 re-watches | the same | — |

Against the mutations each failed within a second or two, at seed 1.

## Appendix F: Appendix D's test, with both orders

For `feed/tests.rs`, in place of the test at `:1859-1888`. It uses only helpers the branch's tests have.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn a_window_watched_again_shows_the_new_name_of_the_older_of_two_people_a_row_mentions() {
    let bed = Bed::new(brisk());
    let (writer, ana, bea) = (person(DIRECT), person(DIRECT + 1), person(DIRECT + 2));
    // One row mentions Ana first and the other Bea first, so neither the first mention's version
    // nor the last's passes for the newest.
    let both = |first: &str, second: &str| {
        json!([
            {"t": "mention", "s": "@a", "person": first}, {"t": "text", "s": " and "},
            {"t": "mention", "s": "@b", "person": second}
        ])
        .to_string()
    };
    let ab = Message { spans: Some(both(&ana, &bea)), ..text(GROUP, "g1", 2_000, Some(&writer), "@a and @b") };
    let ba = Message { spans: Some(both(&bea, &ana)), ..text(GROUP, "g2", 2_001, Some(&writer), "@b and @a") };
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(named(&ana, "Ana Lopes")),
            Change::Person(named(&bea, "Bea Lopes")),
            Change::Person(named(&writer, "Rui")),
            Change::Message(ab),
            Change::Message(ba),
        ])
        .await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;
    let seq = bed.apply(vec![Change::Person(named(&bea, "Bea Martins"))]).await;
    watcher.settle(seq).await;
    check(&bed, &watcher, &watched, "Bea renamed").await;

    // The connection goes, and Ana, the older of the two, is renamed.
    let renaming = bed.apply(vec![Change::Person(named(&ana, "Ana Martins"))]).await;
    let (again, watched) = reconnected(&bed, &watcher, &watched).await;
    check(&bed, &again, &watched, "watched again after Ana's rename").await;
    for id in ["g1", "g2"] {
        assert_eq!(again.windows[&watched[0].0].rows[id]["v"], renaming, "{id}'s version is Ana's rename's");
    }
}
```

# Round two: 5a9e6a4, round one's R1 and T1

Range: `4126db2..5a9e6a4` in `feed-names` (worktree `~/src/jaw-names`), above all `5a9e6a4`, 27 October 2026. Auditor: database-expert. Line numbers are at `5a9e6a4`, and paths are under `crates/jawd/src/` unless they say otherwise.

The branch is two commits on main's `4126db2`:
- `06d0968` is round one's `90b8c67`, rebased. `git range-diff` finds the two identical, and main moved only the database expert's brief, so round one's reading of it stands.
- `5a9e6a4` answers round one. It changes what a message row reads, and nothing that writes:
  - A message row's `v` is the newest of its message's own and those of the people it names (`store/read.rs:330`, `:441`): its sender's (`p.v`), the quoted writer's (`rp.v`) and the newest of its mentioned people's.
  - The quoted writer is now found by a left join on `person_jids`' unique `jid` (`:359-360`), where it was a correlated subquery.
  - The mentions' subquery returns `{"names": …, "v": …}`, their names with the newest of their versions (`:342-345`, parsed at `:376-382` and `:403-407`). It no longer leaves out people jaw knows no name for: they come with a null name, which `mentions_named` treats as no name, as it treated their absence (`:391`), and their `v` counts.
  - The six message statements are pinned again. The plan test now finds the search's, which starts with `{MESSAGE_COLUMNS},` and so was never planned or pinned on main (`store/plans.rs:200`).
  - Five tests (`feed/tests.rs:1635-1857`), and the protocol: a message row's `v` moves when anyone it names gets a new name, so only a chat row comes again with the same `v` and a new name (`protocol/README.md:73`).

There's no schema, migration, PRAGMA, index, trigger, writer, transaction or acknowledgement change. The feed's marking is as round one audited it: `feed/client.rs`, `feed/mod.rs` and `feed/window.rs` are the same at `06d0968` and `5a9e6a4`.

## Summary

- **R1 is closed.** A rename now raises the `v` of every row that shows the person renamed, so a window watched again from its seq gets the new name, and the terminal lays the row out again.
  - Each of the builder's three re-watch tests fails 3 of 3 with its part of the rule off. So does the randomized test, whose second connection now and then reconnects from what it held.
  - My re-watch probe watched windows again 600 times from their seq, through renames, joins and rows mentioning up to three people, and each time they held what a fresh query does.
- **A row's `v` never goes back, through joins, merges, people deleted and people made later.**
  - By the code (below): only a rename, a join, a new JID or a new person moves a person's `v`, always to a seq above everything committed before it. A person is deleted only in a join, after their JIDs move to the kept person. No JID is ever removed. A merge moves each message with a new `v` of its own.
  - By measurement: my probe read every message row of every chat after each of 4,000 commits of one to three random changes. These included joins, merges of one-to-one chats, new people and rows mentioning up to three people. In 417,929 comparisons no `v` went back, and each of the 28,862 rows that showed something new came with a higher `v`. 154 of them had moved chat in a merge.
- **T1 is closed.** With `meet` looking up only one item, the builder's test of six named people among seven strangers fails 3 of 3, at the first or second commit, while round one's six name tests pass.
- **T2, a rule with no test (must change).** The mentions' part of a row's `v` is the newest of its mentioned people's versions (`max(mp.v)`, `store/read.rs:343`). With the oldest instead, `min(mp.v)`, every one of the branch's 13 tests of names and windows passes, 3 of 3, because in all of them a row mentions one person at most. A window watched again then keeps the old name of someone mentioned beside someone renamed earlier. My test of a row that mentions two people fails 3 of 3 with it, and passes on the branch (Appendix D).
- **Every other mutation of the rule fails a branch test, 3 of 3.** That covers leaving out the sender's, the quote's or the mentions' part, or the message's own `v`. It also covers the write side: a join or a new JID leaving the kept person's `v` as it was, and a push name or a verified name not moving it.
- **The plans have no new scan or temporary b-tree.** The quote's person costs two index searches, as its subquery did. The only temporary b-trees are the grouping and ordering of one message's reactions, as on main, where the plan test allows them with a reason.
- **The cost is a few percent, far inside the budgets.**
  - A 100-row window whose rows each mention two people reads in 1.13–1.31 ms at p50, against 1.05–1.20 ms on `06d0968`: 1% to 9% slower, about 5% in the median pair. A 50-hit search page is about 3% slower. The budget for a 100-message page is 5 ms.
  - In `jawd bench feed`, a message reaches four interfaces as fast as on `06d0968`, both quiet and while 1,000 chats no window shows are renamed.
- **Round one's results hold.** On the branch's release build all 57 feed tests, the 10 socket tests and the plan test pass, and so does the 40-seed sweep of racing connections. Of every jawd test, only two of the writer's timing tests fail, which fail on `06d0968` too at this load.
- **The follow-ups:** H4, the plan test skipping statements it doesn't recognise, silently; and round one's H1, H2, H3 and its three items on main, which stand.

## Round one's must-changes, each against its rule

Each mutation was a one-line edit, built into its own release binary and run three times against:
- the branch's 13 tests of names and windows: round one's six, the three random-windows tests, the three re-watch tests and the T1 test;
- my three probes (Measurements).

"Fails" means 3 of 3. The randomized test is `windows_match_fresh_queries_through_renames_joins_new_people_and_edits`.

| Rule | Mutation | What fails without it |
| --- | --- | --- |
| A row's `v` counts its sender's (`store/read.rs:330`) | `p.v` left out | the sender's re-watch test, the randomized test (seed 1, step 15, at a reconnect), my two randomized probes |
| … its quoted writer's (`:330`) | `rp.v` left out | the quote's re-watch test, the randomized test (seed 1, step 25), my two randomized probes |
| … its mentioned people's (`:441`) | left out | the mention's re-watch test, the randomized test (seed 1, step 25), my three probes |
| … the newest of them (`:343`) | the oldest, `min(mp.v)` | none of the branch's; my three probes. This is T2 |
| … and its message's own (`:330`) | `m.v` left out | the randomized test (seed 1, step 6), my two randomized probes |
| A join or a new JID moves the kept person's `v` (`store/write.rs:1210`, `:1231-1234`) | the kept person's `v` left as it was | the randomized test (seed 1, step 15), my two randomized probes; my re-watch probe fails on its live connection too, where a row came back with an older `v` and the interface kept the one it held |
| A push name or a verified name moves a person's `v` (`:1231-1234`) | only a contact name or a join moves it | the randomized test (seed 1, step 25), my two randomized probes |
| `meet` looks up every item of the smaller set (`feed/client.rs:475`) | only one | the T1 test (`@Someone` where it wants `@Bea 0`), the randomized test (seed 1, step 15 or 20), my re-watch probe |

The two write-side rules aren't in the diff, but the branch makes them carry the message rows' versions, as its comment on `MESSAGE_COLUMNS` says (`store/read.rs:326-328`). The randomized test pins both.

Dropping the mentions' name filter changes nothing a row shows, since a null name is read as no name. It does count a nameless person's `v`. That only ever moves a row's `v` without anything shown changing, after a join or a new JID, and the window it's in is marked for that change anyway.

## Findings

### Data loss: none

The branch writes nothing. It changes the version a message row is read with, and which statements the plan test pins.

### Inconsistency: none

**R1 is closed: a rename raises the `v` of the rows it changes.**

What a message row shows of people comes from three lookups (`store/read.rs:330-345`, `:355-360`): its sender, by `m.sender`; whoever wrote what it quotes, by `reply ->> 'from'`; and each person it mentions, through `json_each(m.spans)`. Its `v` is now the largest of its message's own and those people's `people.v`.

**Why a row's `v` never goes back.**
- The message's own `v` only rises. Every write to a message sets it to a new seq: its upsert, an edit, a revoke, a reaction, a star, a status, a merge step and an action from the outbox (`store/write.rs:1678`, `:1853-1874`, `:1916`, `:1409`, `store/write/outbox.rs:258`).
- Three functions write `people`, and each moves a person's `v` only to a seq of its own transaction:
  - `ensure_jid` makes a person for a JID jaw hasn't seen (`store/write.rs:1092-1095`).
  - `upsert_person` sets the kept person's `v` whenever a name, push name or verified name changes, a JID is added or people are joined (`:1210`, `:1228-1238`).
  - `join` runs only inside `upsert_person`. It moves the gone person's JIDs to the kept one, then deletes the gone person (`:1257`, `:1270`), and the caller sets the kept person's `v`.
- Every seq a transaction takes is above every seq committed before it (`Stamps::seq`, `:439-441`). So after a join, the kept person's `v` is above the gone person's old one, even when the kept person was first touched earlier in the same transaction.
- No code deletes a `person_jids` row, and only `join` moves one. `jid` is unique and `person` is NOT NULL (`store/schema.rs:62-66`), so each lookup finds at most one person, and a JID that has a person always has one. The new left join can't repeat a row.
- So a rename raises a term of the max, a join swaps the gone person's term for a higher one, and a person made later adds a term. A person is deleted only in a join, and their JIDs then have the kept person's higher `v`. Nothing lowers the max.
- A window reads its rows and its seq in one transaction (`feed/window.rs:86-87`), so no row's `v` is above the seq its window is as of.

**Why a watch from a seq gets every new name.**
- A watch with `since` sends again every held row whose `v` is above it (`feed/client.rs:491`). jaw-client watches each window again with that window's own newest seq (`crates/jaw-client/src/client.rs:680`, `:1002`).
- Whatever changes how a row names someone moves that person's `v`, and records an event for them in the same commit. So a live window showing the row is marked by round one's rule, and pushes the row with its new `v`.
- What else a message row shows: the outbox's overlay, whose actions move the message's `v`; and "You" for a quote of yours, from `sync_state`'s `me`, which moves no `v` and records no event (round one's item on main, below).

`message_row` now fails the read if the mentions' JSON doesn't parse, where it ignored a bad one before. SQLite builds that JSON itself, from JIDs that matched `person_jids`, names that are text or null, and an integer max, and spans are `json_valid` (`store/schema.rs:476`). So it always parses.

### A rule without a test (must change before landing)

**T2. A row's `v` takes the newest of its mentioned people's versions** (`store/read.rs:343`).
- With `min(mp.v)` in place of `max(mp.v)`, all 13 of the branch's tests of names and windows pass, 3 of 3. So would any other choice of one mentioned person's `v`.
- In every one of those tests a row mentions one person at most: `mentioning` makes one mention (`feed/tests.rs:84`), and the random-writes test's spans (`:388`) and the randomized test's messages and edits (`:1771`, `:1811`) have one each.
- What it would miss: a row mentions Ana and Bea. Bea is renamed, then Ana, whose `v` is the older. With the oldest taken, the row's `v` stays at Bea's rename, and a window watched again from its seq after Ana's rename keeps `@Ana Lopes`.
- My test of exactly that (Appendix D) fails 3 of 3 with the oldest taken, and with the mentions' part left out, and passes on the branch. My two randomized probes (Appendix E) fail 3 of 3 with it too.
- Fix: adopt Appendix D's test. Appendix E's two probes are worth adopting with it. They cover rows mentioning several people, merges of one-to-one chats, and the design's rule itself, that a message's `v` moves when anything it shows changes and never goes back. On the release builds here they took from 25 s to about a minute, with the load.

### Unpredictable performance: none

Nothing breaks a budget (Measurements).
- A rename costs the same reads as in round one, and its rows were already pushed again to live windows.
- What's new is a join or a new JID that changes nothing shown, as when WhatsApp's LID map first gives people their LIDs. It now moves the `v` of the rows naming those people, so a window marked for it sends them again where round one sent nothing. That's bounded by the windows' rows, and happens once per such change.

### Hygiene (follow-ups; none blocks)

- **H4. The plan test finds statements by their first word.**
  - `is_statement` plans a string literal only when its first word is an SQL keyword or one of six names (`store/plans.rs:199-213`).
  - The search's statement starts with `{MESSAGE_COLUMNS},`, so main never planned or pinned it, and the branch's fix covers that one comma.
  - A statement built from any other name would be skipped again, silently.
  - Having the test fail on a literal that expands to SQL (`SELECT` and `FROM`, `INSERT INTO`, `UPDATE`, `DELETE FROM`) without being planned would close it.
- **Round one's H1, H2 and H3 stand.** H1 stays exact for the reason round one gave: every JID that moves, and now every `people.v` that moves, comes with an event for the person in the same commit.
- **Round one's three items on main stand:**
  - The protocol now says chat rows come again with the same `v` and a new name, which is what they do. The terminal's chat list still keeps the old name then (round one's terminal probe), so that follow-up is still the chat rows' `v`.
  - A quote of yours that turns to "You" also comes again with the same `v`, which the protocol's new sentence doesn't mention. When `me` gets an event, the rows it changes should get a new `v` too.
  - A window on a chat being merged away keeps its rows and total during the merge's steps.

## Measurements

### Method

- One clone of the worktree, `/tmp/jaw-audits/feed-names-2/jaw`. I switched it between `5a9e6a4` and `06d0968` with `git checkout`. Its `target` was the audit's one build folder: every build was release, with `CARGO_INCREMENTAL=0`, and each binary was copied out as its build finished.
- `TMPDIR` and `JAW_HOME` were under that folder. `jawd bench feed` ran with `--engine demo`, and starts no engine. Nothing touched the owner's data, `~/Library/Application Support/jaw` or any process but mine. The folder is deleted.
- My probes were appended to `feed/tests.rs` at `5a9e6a4`. The timing probe was a module of its own under `feed/`, the same at both revisions, as `feed/mod.rs`, `client.rs` and `window.rs` are.
- Each mutation was an exact one-line replacement, built, copied out, and checked out again.
- Load (one-minute average) was 33 to 80 while I worked, with other audits building beside me. Each figure says its own.

### The suites

- **Every jawd unit test** on the branch's release build, 8 threads, load 58–69: 381 of 383 pass.
  - The two that fail are the outbox's commit-time tests (`in_a_group_of_a_thousand_a_step_of_regrouping_looks_at_one_message_so_it_stays_short` and `when_most_of_a_thousand_leave_steps_are_sized_by_who_was_sent_to_and_skip_what_is_read`). They hold the writer's steps to 3 ms, and the branch doesn't touch the writer.
  - The second passed when run alone. Run alone three times on each side at load 52–70, the first failed 2 of 3 on `06d0968` and 3 of 3 on the branch, by turns on regrouping and on letting go. The other audits were writing to the same disk.
- **The plan test** passes, and so do the 57 feed and 10 socket tests.
- **The ignored sweep**, `many_connections_match_fresh_queries_through_moves_catch_ups_failing_reads_and_stalls_for_40_seeds`, passes in 214 s at load 57–63.
- **Load timeouts.** In the mutants' runs, beside a build at load 60–80 with four tests at once, two tests the mutation didn't touch hit the harness's 10 s wait for the feed (`feed/tests.rs:225`). One of them passed 30 of 30 alone on the branch at load 78–80.
- **Formatting:** `cargo fmt --all --check` passes at `5a9e6a4`. I didn't run clippy or `make check-generated`.

### My probes

On the branch's release build, at load 58–79:

| Probe | What it does | Branch |
| --- | --- | --- |
| `a_rows_version_never_goes_back_and_moves_whenever_what_it_shows_changes` | 20 seeds of 200 commits, each of one to three random changes among ten people with a number and a LID each. The changes: renames by each of the three names; joins with and without a name; new people; messages to three groups and to one-to-one chats under either JID, mentioning up to three people and quoting what was sent or someone jaw may not know yet; edits that replace the mentions; deletes; a merge step after one commit in three. After each commit every message row of every chat is read fresh and compared with the last read, by its n | passes: 417,929 comparisons, 28,862 rows showing something new and each at a higher `v`, 154 moved chat by a merge, 660 joins asked for |
| `windows_watched_again_from_their_seq_match_fresh_queries_through_many_mentions_and_joins` | the same account without one-to-one chats, 10 seeds of 150 commits. One connection settles after every commit. Another takes nothing, and every one to four commits watches its windows again from what it held, by seq and ids | passes: 600 re-watches |
| `a_window_watched_again_shows_the_new_name_of_the_older_of_two_people_a_row_mentions` | a row mentions Ana and Bea. Bea is renamed, the connection goes, and Ana, whose `v` is the older, is renamed. The window is watched again from its seq | passes |

The counts came from a build of the same probes with counters added. Appendices D and E have them as they'd be adopted, without the counters; that text builds without warnings, passes `cargo fmt --check`, and passes on the branch.

### The cost of a window and a search page

The timing probe sets up:
- 3,000 named people, 2,000 of whom send, are mentioned and are quoted;
- 64 groups of 150 messages, each mentioning two of them, one in four also quoting the message before.

Then, on one reader connection, it reads the newest 100 rows of 40 groups 25 times over, and a 50-hit search page 200 times. It ran on both release builds, interleaved, eight times each: four with `06d0968` first at load 71–75, four with the branch first at load 61–72.

| Over eight runs each | `06d0968` | `5a9e6a4` |
| --- | --- | --- |
| a 100-row window's read, p50 over 1,000 | 1.05–1.20 ms | 1.13–1.31 ms |
| … p90 | 1.56–2.17 ms | 1.80–3.19 ms |
| … p99 | 2.54–3.94 ms | 2.85–5.60 ms |
| a 50-hit search page, p50 over 200 | 0.83–0.91 ms | 0.89–0.94 ms |
| … p99 | 1.13–1.42 ms | 1.16–1.27 ms |

- Pair by pair, the window's p50 was 1.1% to 8.8% higher on the branch, 4.1% and 5.7% in the middle two pairs. The search page's p50 was 0.7% to 9.6% higher, about 3.4% at the median.
- The worst p99, 5.60 ms, came in one run whose p90 was also out of line, 3.19 ms where the branch's others were 1.80–2.14. The other seven of the branch's p99s were 2.85–3.78 ms, and `06d0968`'s reached 3.94 ms.
- The builder's 4% (1.00–1.03 ms against 0.95–0.99, at load 56–59) is within this.

### A message's way to the interfaces: `jawd bench feed`

The command was `jawd --engine demo bench feed --interfaces 4 --far 1000`, on both release builds, interleaved, three rounds each with `06d0968` first, at load 33–66. It writes 2,000 messages a second for 5 s. Then come 50 lone messages 200 ms apart, quiet, and 50 more while the unread counts and names of 1,000 chats no window shows change 2,000 times a second.

| Over three rounds each | `06d0968` | `5a9e6a4` |
| --- | --- | --- |
| in the burst, commit to push, p50 | 28.0–29.5 ms | 27.7–28.1 ms |
| a lone message, p50 | 0.85–0.87 ms | 0.86–0.88 ms |
| … the worst interface's p99 | 1.96–3.65 ms | 2.16–4.76 ms |
| while busy, p50 | 1.01–1.28 ms | 1.04–1.20 ms |
| … the worst interface's p99 | 1.76–2.59 ms | 1.72–3.27 ms |
| jawd's CPU with the interfaces watching | 54.4–55.2% of a core | 56.5–57.4% |

- Each p99 is the slowest of 50 messages, and moved with the load on both sides. All are inside the 5 ms budget.
- The branch's CPU was 1.3 to 2.6 points of a core higher in each round, in line with its window reads costing a few percent more.

## Appendix D: the test for T2

For `feed/tests.rs`. It uses only helpers the branch's tests have.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn a_window_watched_again_shows_the_new_name_of_the_older_of_two_people_a_row_mentions() {
    let bed = Bed::new(brisk());
    let (writer, ana, bea) = (person(DIRECT), person(DIRECT + 1), person(DIRECT + 2));
    let spans = json!([
        {"t": "mention", "s": "@a", "person": ana}, {"t": "text", "s": " and "},
        {"t": "mention", "s": "@b", "person": bea}
    ]);
    let message = Message { spans: Some(spans.to_string()), ..text(GROUP, "g1", 2_000, Some(&writer), "@a and @b") };
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(named(&ana, "Ana Lopes")),
            Change::Person(named(&bea, "Bea Lopes")),
            Change::Person(named(&writer, "Rui")),
            Change::Message(message),
        ])
        .await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;
    let seq = bed.apply(vec![Change::Person(named(&bea, "Bea Martins"))]).await;
    watcher.settle(seq).await;
    check(&bed, &watcher, &watched, "Bea renamed").await;

    // The connection goes, and Ana, the older of the two, is renamed.
    let renaming = bed.apply(vec![Change::Person(named(&ana, "Ana Martins"))]).await;
    let (again, watched) = reconnected(&bed, &watcher, &watched).await;
    check(&bed, &again, &watched, "watched again after Ana's rename").await;
    assert_eq!(again.windows[&watched[0].0].rows["g1"]["v"], renaming, "the row's version is Ana's rename's");
}
```

## Appendix E: the randomized probes

For `feed/tests.rs`, after Appendix D's test. They use only helpers the branch's tests have.

```rust
/// How many people the account of versions has, each with a number and a LID.
const VERSIONED_PEOPLE: usize = 10;

fn versioned_number(k: usize) -> String {
    format!("3519400000{k:02}@s.whatsapp.net")
}

fn versioned_lid(k: usize) -> String {
    format!("{}@lid", 84_726_193_530_000u64 + k as u64)
}

fn versioned_groups() -> Vec<String> {
    (0..3).map(|g| format!("1203630000000003{g:02}@g.us")).collect()
}

/// A made-up account whose people each have a number and a LID; its rows mention up to three of
/// them and quote what was sent or someone jaw may not know yet, and, when `direct`, some go to
/// one-to-one chats, which merge when a number and a LID are found to be one person.
struct Versions {
    rng: Rng,
    clock: i64,
    next: u64,
    sent: Vec<Message>,
    direct: bool,
}

impl Versions {
    fn new(seed: u64, direct: bool) -> Versions {
        let rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        Versions { rng, clock: 1_700_000_000_000, next: 0, sent: Vec::new(), direct }
    }

    fn jid(&mut self) -> String {
        let k = self.rng.below(VERSIONED_PEOPLE);
        if self.rng.below(2) == 0 { versioned_number(k) } else { versioned_lid(k) }
    }

    fn spans(&mut self) -> Option<String> {
        let many = self.rng.below(4);
        if many == 0 {
            return None;
        }
        let mut spans = vec![json!({"t": "text", "s": "hey "})];
        for _ in 0..many {
            let whom = self.jid();
            let number = whom.split('@').next().unwrap().to_string();
            spans.push(json!({"t": "mention", "s": format!("@{number}"), "person": whom}));
            spans.push(json!({"t": "text", "s": " "}));
        }
        Some(Value::Array(spans).to_string())
    }

    fn message(&mut self) -> Change {
        self.clock += 1 + self.rng.below(2_000) as i64;
        self.next += 1;
        let sender = self.jid();
        let chat = if self.direct && self.rng.below(3) == 0 {
            sender.clone()
        } else {
            self.rng.pick(&versioned_groups()).clone()
        };
        let mut message = text(&chat, &format!("b{}", self.next), self.clock, Some(&sender), "words");
        message.spans = self.spans();
        if self.rng.below(3) == 0 {
            let (id, from) = match self.rng.below(2) {
                0 if !self.sent.is_empty() => {
                    let quoted = self.rng.pick(&self.sent).clone();
                    (quoted.id, quoted.sender)
                }
                _ => ("elsewhere".to_string(), Some(self.jid())),
            };
            message.reply = Some(json!({"id": id, "from": from, "summary": "quoted"}).to_string());
        }
        self.sent.push(message.clone());
        Change::Message(message)
    }

    fn change(&mut self) -> Change {
        match self.rng.below(12) {
            0..=3 => self.message(),
            4..=6 => {
                let jid = self.jid();
                let n = self.rng.next() % 100;
                Change::Person(match self.rng.below(3) {
                    0 => Person { jids: vec![jid], name: Some(format!("Name {n}")), ..Person::default() },
                    1 => Person { jids: vec![jid], push_name: Some(format!("push {n}")), ..Person::default() },
                    _ => Person { jids: vec![jid], verified_name: Some(format!("Biz {n}")), ..Person::default() },
                })
            }
            7 => {
                let k = self.rng.below(VERSIONED_PEOPLE);
                let name = (self.rng.below(2) == 0).then(|| format!("Joined {}", self.rng.next() % 100));
                Change::Person(Person { jids: vec![versioned_number(k), versioned_lid(k)], name, ..Person::default() })
            }
            8 if !self.sent.is_empty() => {
                let k = self.rng.below(self.sent.len());
                let mut edited = self.sent[k].clone();
                self.clock += 1;
                edited.edited_at = Some(self.clock);
                edited.text = Some(format!("edited {}", self.clock));
                edited.spans = self.spans();
                self.sent[k] = edited.clone();
                Change::Message(edited)
            }
            9 if !self.sent.is_empty() => {
                let gone = self.sent.swap_remove(self.rng.below(self.sent.len()));
                Change::DeleteMessage { chat: gone.chat, id: gone.id }
            }
            _ => self.message(),
        }
    }
}

/// Every message row of every chat, as a fresh query has it, by its n.
async fn every_message_row(store: &Store) -> HashMap<i64, Value> {
    store
        .reads()
        .query(crate::work::Priority::Interactive, |db| {
            let mut all = HashMap::new();
            for chat in read::chats(db, false, None, 100_000)? {
                for row in read::messages(db, &chat.id, Around::Newest, 1_000_000)? {
                    all.insert(row.n, serde_json::to_value(message_row(row)).unwrap());
                }
            }
            Ok(all)
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rows_version_never_goes_back_and_moves_whenever_what_it_shows_changes() {
    let without_v = |row: &Value| {
        let mut row = row.clone();
        row.as_object_mut().unwrap().remove("v");
        row
    };
    for seed in 1..=20u64 {
        let bed = Bed::new(brisk());
        let mut world = Versions::new(seed, true);
        let seeding: Vec<Change> = (0..20).map(|_| world.message()).collect();
        bed.apply(seeding).await;
        let mut was = every_message_row(&bed.store).await;
        for step in 0..200 {
            let many = 1 + world.rng.below(3);
            let changes: Vec<Change> = (0..many).map(|_| world.change()).collect();
            bed.apply(changes).await;
            if world.rng.below(3) == 0 {
                bed.store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
            }
            let now = every_message_row(&bed.store).await;
            for (n, row) in &now {
                let Some(before) = was.get(n) else { continue };
                let (v, v0) = (row["v"].as_u64().unwrap(), before["v"].as_u64().unwrap());
                assert!(
                    v >= v0,
                    "seed {seed}, step {step}: message {n}'s v went back from {v0} to {v}:\nwas {before}\nnow {row}"
                );
                if without_v(row) != without_v(before) {
                    assert!(
                        v > v0,
                        "seed {seed}, step {step}: message {n} shows something new at v {v}:\nwas {before}\nnow {row}"
                    );
                }
            }
            was = now;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_watched_again_from_their_seq_match_fresh_queries_through_many_mentions_and_joins() {
    for seed in 1..=10u64 {
        let bed = Bed::new(brisk());
        let mut world = Versions::new(seed, false);
        let seeding: Vec<Change> = (0..30).map(|_| world.message()).collect();
        bed.apply(seeding).await;
        let wants: Vec<Want> = versioned_groups()
            .into_iter()
            .map(|chat| Want::Messages { chat, anchor: None, before: 8, after: 0 })
            .collect();
        let mut live = bed.watcher(64);
        let live_watched = watch_all(&mut live, &wants).await;
        // The other connection takes no pushes; every few commits it's gone and comes back from what
        // it held, as jaw does after a reconnect or a resync.
        let mut away = bed.watcher(64);
        let mut away_watched = watch_all(&mut away, &wants).await;
        let mut gap = 0;
        for step in 0..150 {
            let many = 1 + world.rng.below(3);
            let changes: Vec<Change> = (0..many).map(|_| world.change()).collect();
            let seq = bed.apply(changes).await;
            live.settle(seq).await;
            check(&bed, &live, &live_watched, &format!("seed {seed}, step {step}, live")).await;
            if gap == 0 {
                (away, away_watched) = reconnected(&bed, &away, &away_watched).await;
                check(&bed, &away, &away_watched, &format!("seed {seed}, step {step}, watched again")).await;
                gap = world.rng.below(4);
            } else {
                gap -= 1;
            }
        }
    }
}
```

# Round one: 90b8c67, kept as written

# Audit: the names a rename reaches

Range: `be0c463..90b8c67` in `feed-names` (worktree `~/src/jaw-names`), 26 October 2026. Auditor: database-expert. Line numbers are at `90b8c67`, and paths are under `crates/jawd/src/` unless they say otherwise.

`90b8c67` is one commit on main's `be0c463`, which carries the socket's lane. It changes which messages windows a change to people marks:
- Each messages window keeps, beside its senders, the JIDs its rows mention or quote someone by, each hashed to a `u64` (`feed/window.rs:153-156`, `feed/client.rs:210`, `:420`), as the read it settled from found them (`:522`).
- The feed reads, with each batch of events, every JID the people changed in it have (`feed/mod.rs:190-196`, `store/read.rs:617-625`). A messages window is marked when its senders meet the people changed, or the JIDs it names meet theirs (`client.rs:682-690`), through `meet`, which looks up the smaller set's items in the larger (`:473-476`).
- The message query returns the quoted JID as it is, as a 28th column (`store/read.rs:349`), and a kept read's bytes count the named set (`client.rs:352-355`).
- Six tests (`feed/tests.rs:1487-1633`), the random-writes test mentioning and quoting (`:383-398`), and the protocol's sentence on rows that come again with the same `v` (`protocol/README.md:73`).

There's no schema, migration, PRAGMA, index, trigger, writer, transaction or acknowledgement change: the branch reads and marks, and writes nothing. It adds one statement, `jids_of`, whose plan is pinned: a search of `person_jids_by_person` for each person, in a list subquery over `json_each`. The message statements plan as on main; only their text gains the column. The store's 103 tests pass, the plan test among them.

## Summary

- **On a live connection, every rename reaches exactly the windows whose rows show the person renamed, and no other window is read.** That holds for:
  - senders, mentions and quotes, and quotes of quotes;
  - mentions an edit adds or takes away;
  - a person made later for a JID a row names;
  - a number and a LID found to be one person, through the join, its chats' merge and each merge step;
  - a connection taking another's kept read.

  My probes check every window against a fresh query after every change. They include ten seeds of 150 random renames, joins, new people, edits and deletes, with two connections. Each of the builder's rules fails a test without it, 3 of 3, but two (below).
- **R1, inconsistency, must change: a rename leaves the rows' `v` where it was.**
  - A messages window watched again from its seq, as jaw does after every reconnect and resync, keeps the old name in every row it held.
  - The terminal lays out a message once per `v`, so it keeps drawing the old name even on a live connection.

  My probes show both, on the branch and on main. It predates the branch for senders, but it's in the rule the branch implements. The branch's protocol edit writes it down for mentions and quotes, against both designs, which say a rename raises the version of the rows it changes. The fix is to give a message row the version of what it shows.
- **T1, a rule with no test, must change.** With `meet` looking up only one of the smaller set's items, all of the builder's tests pass, since in each of them one of the two sets has a single item. My randomized probe fails 3 of 3.
- **The bound holds, within about 1%.**
  - A window's named JIDs are a few percent of its kept read: 217 JIDs, 2.3 KB, beside 52.8 KB of rows' JSON for 100 rows that mention two people each.
  - They're billed at 77% of what is allocated, as `senders` is on main.
  - No test checks that they're billed at all (H2).
- **Nothing costs more than on main.**
  - A rename no window shows costs the branch less CPU than main (0.56–0.59 ms against 0.79–0.94), since `meet` looks up one item where main tested every sender of every window.
  - A rename the windows show costs the same, with 256 windows open: 7.4–7.7 ms against 7.6–7.9.
  - In `jawd bench feed`, a message reaches four interfaces as fast as on main, both quiet and while 1,000 chats no window shows are renamed a second.
  - Reading the JIDs of 4,096 people changed at once takes 2.1 ms.
- **The socket lane hasn't moved.** The branch changes nothing in `sock.rs`. Its 50 feed and 10 socket tests pass, the ignored 40-seed sweep of racing connections among them.
- **The rest are follow-ups:**
  - H1: the JIDs aren't read in the events' snapshot, as the commit says, though the result is exact anyway.
  - H2: the named set's bill, and a test for it.
  - Two gaps on main that the probes found outside the branch.

## What holds, and why

What a message row shows of people comes from three lookups at read time (`store/read.rs:328-344`), and from `sync_state`'s `me` for "You":
- its sender's person, by `m.sender`;
- the quoted JID's person, by `reply ->> 'from'`;
- each mentioned JID's person, through `json_each(m.spans)`.

So a row shows something new in one of three cases:
- **The row changes.** Its message event marks its chat's windows.
- **One of those people gets a new name.** `upsert_person` records an event for that person (`store/write.rs:1228-1241`). The batch's JIDs, read after its events, hold every JID the person has, so a window that names one of them meets the batch, and a window they sent a row in meets it by the person's n.
- **A JID a row names gets its first person or moves to another.** That happens only in `ensure_jid`, `upsert_person` and `join`, and each records an event, in the same commit, for the person who has the JID now (`write.rs:1092-1095`, `:1210-1241`). That person's JIDs, read after, include it.

A window's named set comes from the same read as the rows its interface holds (`settle`, `client.rs:503-525`), so it names exactly what they name. A kept read carries its own set, as it carries `senders`. A connection takes a kept read only if the read has every change the connection has heard (`client.rs:664`), so it never takes one with a name older than a rename it heard. The builder's kept-read test pins that.

What doesn't hold is the version a renamed row comes with (R1), and the `me` that "You" depends on, which changes without an event (on main, below).

## The builder's rules, each against a mutation

Each mutation was built into its own binary and run three times against:
- the builder's six tests;
- the random-writes test;
- the kept-bytes test;
- five of my probes: quote of a quote, edit, quote of someone jaw learns of later, the randomized probe, and the join through a merge.

The runs were at load 75 to 90. "Fails" means 3 of 3. My probes are named in Measurements.

| Rule | Mutation | What fails without it |
| --- | --- | --- |
| A window is marked where the JIDs it names meet the people changed's (`client.rs:689`) | main's rule, senders only | the builder's six tests, the random-writes test (seed 2, step 147), and four of my probes: quote of a quote, edit, quote of someone jaw learns of later, randomized (seed 1, step 21) |
| … and only there | every messages window marked by any change to people | the rename-reads test (4 reads where 2), my quote of a quote (4 for 3) and edit (5 for 4) probes |
| A window names whom its rows mention (`window.rs:156`) | mentions left out | the three mention tests, the kept-read and rename-reads tests, the random-writes test, my edit and randomized probes |
| … and whom they quote (`window.rs:154`) | quotes left out | the quote test, the random-writes test (seed 4, step 124), my quote of a quote, quote of someone jaw learns of later and randomized probes |
| A change to people carries the JIDs of every person changed (`mod.rs:192-194`) | only the first person's | the number-and-LID test (`@84726193520417` where a fresh query has `@Dora Reis`), my randomized probe (seed 1, step 94) |
| A window keeps what the read it settled from names (`client.rs:522`) | an empty set kept | the same eleven as main's rule: all but the kept-bytes test and my join probe |
| A connection takes a kept read only with every change it heard (`client.rs:664`) | without `heard` | the kept-read test (`@Bea Lopes` where `@Bea Martins`) |
| `meet` looks up every item of the smaller set (`client.rs:475`) | only one | none of the builder's; my randomized probe (seed 1, step 15). This is T1 |
| The named set is billed in the kept bytes (`client.rs:355`) | left out | nothing. This is H2 |

With main's rule, the mention test holds `@Bea Lopes` where a fresh query has `@Bea Martins`. The test of a person jaw learns of later holds `@351910000009` for `@Cris Neves`, and the rename-reads test makes 2 reads where it wants 3.

My join probe passes with main's rule too, since its group window has rows the joined person sent, so senders alone mark it. The builder's number-and-LID test and my randomized probe are what tell the rules apart there.

## Findings

### Data loss: none

The branch writes nothing; it changes what's read and which windows are marked.

### Inconsistency (must change before landing)

**R1. A rename leaves the rows' `v` where it was, so a watch from a seq, and the terminal, keep the old name.** The places are `feed/client.rs:491`, `crates/jaw-client/src/client.rs:680`, `crates/jaw/src/view.rs:1019-1031` and `protocol/README.md:73`.

**What happens.** jawd resolves the names a message row shows at read time, and gives the row `v = m.v`, the message's own version (`store/read.rs:328`). A rename changes the names and leaves `m.v` alone, so the row comes again with the new name and the same `v`, as the branch's protocol sentence says. Two things then miss the new name.

**A watch from a seq skips the renamed rows.**
- A watch with `since` treats every row its interface holds as unchanged if the row's `v ≤ since` (`client.rs:491`).
- jaw-client watches a messages window again with its last seq and the ids it holds (`jaw-client/src/client.rs:680`) in two cases:
  - after every reconnect (`:811-818`);
  - after every `resync` (`:918-930`), which is what jawd sends an interface that falls 5 s behind. A contacts sync, renaming thousands at once, makes that likelier.
- So an interface that missed a rename keeps the old name in every row it held, until something else changes the row.
- My probes `audit_a_reconnect_from_its_seq_shows_the_new_name_of_someone_mentioned`, `…_someone_quoted` and `…_a_sender` show it (Appendix A). In each, a window holds a row and its connection goes. The person is renamed, and the window is watched again from its seq with its ids. Each fails 3 of 3 on the branch and on main: the interface holds `@Bea Lopes`, or `Bea Lopes` as the quote's author or the sender, where a fresh query has `Bea Martins`.

**The terminal draws the old name, even on a live connection.**
- It lays out each message once per `v` and shape (`crates/jaw/src/view.rs:1019-1031`), and forgets a layout only when its row is removed (`:533-537`).
- The lines it keeps hold the sender's name (`crates/jaw/src/layout.rs:394`), the quote's author (`:407`) and the text's mentions.
- So a row pushed again with the same `v` and a new name changes the window's model, but not the screen.
- My terminal probes show it (Appendix C). After a push with the same `v`, the window holds "Bea Martins" and the screen still says "Bea Lopes", as the sender and as the quote's author. With `v` moved, the screen says "Bea Martins".
- So most of what the branch now pushes for mentions and quotes never reaches the screen for a message already laid out, and on main neither does a sender's new name.

**The designs say the opposite.**
- The jaw design: "A rename raises the version of the rows it changes, so a reconnect from a seq is always exact" (`docs/plans/2026-10-16-jaw-design.md:218`).
- jaw-mac's design keys its layout cache on it: "A message's `v` changes when anything it shows changes, so an edit, a reaction or a sender's new name lays it out again" (`docs/plans/2026-10-26-jaw-mac-design.md:191`).
- The protocol's sentence describes the code instead, and the branch extends it from senders to mentions and quotes.

**Why it's this branch's.** It has been so for senders since `24ed2f2`, on 16 October. But which rows show a rename is the rule this branch implements, and the branch writes the gap into the protocol for mentions and quotes, against both designs. The branch gives live connections the new names; reconnects, resyncs and the terminal still miss them, as they miss senders on main.

**The fix I recommend: give a message row the version of what it shows.**
- Make it `v = max(m.v, the sender's people.v, the quoted person's people.v, the largest people.v among the people it mentions)`, and say so in the protocol again.
- `people.v` already takes the write's seq exactly when the way rows name someone can change: a new name, push name or verified name, a join, a new JID, a new person (`store/write.rs:1092-1095`, `:1214-1238`). Nothing else writes it.
- The sender's person is already joined (`p`), and the quote's and the mentions' subqueries already read `people` (`store/read.rs:332`, `:343`). So each can return its `v` beside its name in the same lookups, for instance as a `json_object`, without new scans.
- A row's `v` never goes down: a join raises the kept person's `v`, and the rows naming the gone person's JIDs then name the kept one.
- With it:
  - `changed` sends renamed rows again on a watch from a seq;
  - the terminal lays them out again;
  - jaw-mac's cache works as designed;
  - the design's rule holds again.
- Pin the changed plans, and time a window's read: 1.5 ms for 100 rows here.

The other way is to keep `v` and change three things:
- a watch from a seq sends again every row that names someone changed after `since`, which needs the newest person event read in the window's snapshot;
- the terminal keys its layouts on a row's content rather than its `v`;
- jaw-mac's design changes.

That leaves more places to get right.

Either way, adopt my three reconnect probes as tests. With the fix each passes, and without it each fails. A jawd test that a rename raises the `v` of the rows it changes would pin the terminal's side too.

The same holds for chat rows, on main and outside the branch:
- A chat row's `v` is `c.v` alone (`store/read.rs:245`), while a direct chat's name comes from its person, and a group's preview from its last sender.
- My terminal probe shows the list keeping "Tiago Fonseca" when the row comes again with the same `v` and a new name. With `v` moved, it shows the new name.
- Chat lists come whole on a reconnect (`jaw-client/src/client.rs:673-677`), so only the terminal's cache misses it (`crates/jaw/src/view/draw.rs:437`).
- The same derivation would close it. It can follow this branch, since the branch doesn't touch chat lists.

### A rule without a test (must change before landing)

**T1. `meet` looks up every item of the smaller set** (`client.rs:473-476`), for senders and for named JIDs.
- With `small.iter().take(1)`, every one of the branch's tests passes 3 of 3, the random-writes test included. In each of them one of the two sets has a single item: one person renamed with one JID, or a window naming one JID.
- My randomized probe fails 3 of 3, at seed 1, step 15. Its windows name up to a dozen JIDs, and a person found to have a number and a LID carries two.
- Fix: a test where both sets have several items and meet at only one. For example:
  - a window whose rows mention six people;
  - six commits, each renaming one of them together with seven people no row names;
  - with one item looked up, the test fails unless every lookup happens to pick the shared one, about one chance in 47,000.
- Or adopt my randomized probe (Appendix B).

### Unpredictable performance: none

The measurements break no budget (see Measurements).

### Hygiene (follow-ups; none blocks)

- **H1. The JIDs aren't read in the events' snapshot.**
  - The commit says a change to people carries their JIDs "read with the events", and the brief I was given said "in the same snapshot as the events".
  - `follow`'s closure runs `read::events` and then `read::jids_of` on a reader with no transaction around them (`feed/mod.rs:190-196`; `store/read.rs:66-76` runs the closure as it is). So each statement takes its own snapshot, and a commit can land between them.
  - It's exact anyway. A JID only ever moves to a person who gets an event in the same commit: `join` moves the gone person's JIDs to the kept one, whose update is then forced and recorded (`store/write.rs:1222-1241`), and a new JID sets the same flag. So a JID that `jids_of` misses, because a later commit moved it, comes with that commit's batch. One it sees early only marks a window that didn't need it.
  - Reading both in one transaction would make the claim true, and keep it true if a writer ever moved a JID without an event. It costs nothing.
- **H2. The named set's bill.**
  - `capacity × 8` (`client.rs:355`) is 77% of what hashbrown allocates: `buckets × 9 + 16`, with the capacity at 7/8 of the buckets. That's as for `senders` on main.
  - Each window's `State` holds a copy (`:522`) that isn't billed, as its `senders` and `held` aren't.
  - With the term removed, every test passes, the kept-bytes test included.
  - At about 1% of a kept read it doesn't threaten the 16 MiB bound. A test that a window naming many JIDs is billed more than one naming none would pin the term.
- **H3, the feed audit's, stays**, as the commit says.

Two JIDs that hash alike only mark a window that didn't need it, as the doc comment on `hashed` says (`feed/mod.rs:49-55`).

### On main, outside this branch

- **A quote of yours says "You" only once its window is read for another reason.** `me` lives in `sync_state`, and writing it records no event (`Change::Remember`), so no window is marked when jaw learns its own LID. My probe `audit_a_quote_of_yours_says_you_once_jaw_knows_your_jids` fails 3 of 3 on the branch and on main: the quote keeps the raw LID where a fresh query has "You". An event when `me` changes, marking the windows that quote any JID in it, would close it.
- **A window on a chat being merged away keeps its rows and total during the merge's steps, when no unread count moves.**
  - Each step moves up to 200 messages to the kept chat, and their events carry the kept chat, so the gone chat's window isn't marked.
  - My probe `audit_a_window_on_a_chat_being_merged_away_follows_each_step_when_nothing_unread_moves` fails 3 of 3 on both. After the first step, the window's `total` is 230 where a fresh query has 30.
  - It lasts until the merge ends. My join probe checks the gone chat's window once the merge is done, and it matches then.
  - An event for the gone chat in each step would close it.
- **Chat rows' names**, as under R1.

## Measurements

### Method

- I cloned the worktree to `/tmp/jaw-names-audit/branch` at `90b8c67`, and added a worktree of that clone at `be0c463` for main. The user asked for `/tmp/jaw-names-audit` rather than the brief's `/tmp/jaw-audits/<topic>`.
- Both checkouts built into one target folder, which the brief warns against. To guard against it:
  - I copied each binary out the moment its build finished;
  - I confirmed each was the revision it should be. Main's reports no named set, fails the probes the branch fixes, and lists 7 fewer tests.
- Every run had `JAW_HOME` and `TMPDIR` under that folder. `jawd bench feed` ran with `--engine demo`, and starts no engine. Nothing touched the owner's data, `~/Library/Application Support/jaw` or any process but mine. The folder is deleted.
- My probes were appended to `feed/tests.rs` in both checkouts, the same block in each but for the cost probe's line that reads the named set, using only what main's tests have. The terminal's probes were appended to `crates/jaw/src/view/tests.rs` in the branch's checkout.
- Each mutation was a one-line source edit (listed in the table above), built into its own binary, then reverted with `git checkout`.
- Load (one-minute average) was 41 to 145 while I worked; each figure below says its own.

### The suites and the probes

- **The branch's feed and socket tests:** 50 and 10. On its debug build all pass but the ignored sweep, in two runs at loads 87 and 145; the sweep is below.
- **The ignored sweep:** `many_connections_match_fresh_queries_through_moves_catch_ups_failing_reads_and_stalls_for_40_seeds` passes on its release build, in 93 s at load 46–53.
- **The store's tests:** 103 pass, among them the plan test, at load 42–62.

My ten probes, three rounds on each side's release build, at load 58–88:

| Probe | Branch | Main |
| --- | --- | --- |
| `audit_a_quote_of_a_quote_names_only_whoever_wrote_what_it_quotes`: r3 quotes r2, which quotes r1; renaming r1's author reads only r2's window, renaming r2's reads both | passes | fails: r2 keeps "Ana" |
| `audit_a_mention_an_edit_adds_or_takes_away_is_followed`: an edit adds a mention, she's renamed, an edit takes it away, she's renamed again | passes | fails: renamed after the edit, the row keeps "@Xana" where a fresh query has "@Xana Reis" |
| `audit_a_quote_of_someone_jaw_learns_of_later_takes_their_name` | passes | fails: the quote keeps the JID where a fresh query has "Cris Neves" |
| `audit_windows_match_fresh_queries_through_renames_joins_new_people_and_edits`: six people with a number and a LID each, three groups, 10 seeds of 150 random changes, two connections, the second settling one step in four | passes | fails (seed 1, step 21) |
| `audit_windows_follow_a_number_and_lid_joined_through_their_chats_merge`: number first and LID first, 230 messages under the LID so the merge takes two steps, group rows sent by, mentioning and quoting each JID, then a rename by the LID | passes | passes (senders mark it) |
| the three `audit_a_reconnect_from_its_seq_…` probes | fail (R1) | fail |
| `audit_a_quote_of_yours_says_you_once_jaw_knows_your_jids` | fails (on main) | fails |
| `audit_a_window_on_a_chat_being_merged_away_…` | fails (on main) | fails |

The terminal's three probes (Appendix C) fail on the branch's checkout as described under R1. Each passes its control, the same push with `v` moved.

### The windows' cost: the audit's probe

The probe ran on both release builds, interleaved branch then main, three rounds each, at load 76–82. It sets up:
- 2,000 named people who send, are mentioned and are quoted in 64 groups of 150 messages: every message mentions two of them, and one in four quotes the message before;
- 1,000 named people no row names;
- 8 connections, each watching 32 windows of a group's newest 100 rows, so each group is watched by four connections.

Then it runs:
- 1,000 renames, each its own commit, of people no window names, at 500 a second;
- 1,000 renames of people the windows name;
- 200 lone messages to a watched group.

CPU is from `getrusage` over each phase, in the one process that also holds the eight interfaces parsing every line, about 50 KB each.

| | Branch | Main |
| --- | --- | --- |
| a 100-row window's read, p50 over 40 × 25 | 1.50–1.59 ms | 1.42–1.59 ms |
| its named JIDs, and their capacity | 217, 224 (billed 1,792 B; allocated 2,320 B) | none |
| its rows' JSON | 52,772 B | 52,772 B |
| a rename no window shows: CPU | 0.56–0.59 ms | 0.79–0.94 ms |
| … window reads and lines sent | 0 and 0 | 0 and 0 |
| a rename the windows show: CPU | 7.45–7.71 ms | 7.56–7.91 ms |
| … window reads, lines sent | 789–810, 2,823–2,910 | 839–881, 3,058–3,273 |

- **A rename no window shows costs the branch less.** For each of 256 windows, main tested every sender, about 95, against the people changed. `meet` looks up the one person changed.
- **A rename the windows show costs the same CPU.** The reads and lines aren't comparable: each group was read about five times a second, a quarter of what the 50 ms pace allows, so at this load they're bound by the machine. The branch's slightly costlier reads (hashing the named JIDs) leave it a few fewer.
- **The probe's lone messages measure its own spacing.** They're 50 ms apart, which is the feed's `burst`, so about half wait out the rest of a window's pace. Quiet, p99 was 50–55 ms on both sides, and p50 ranged from 1.4 to 29 ms on both. The bench below times lone messages properly.

Reading and hashing the JIDs of the people changed, on the branch, with 8,192 people of two JIDs each, three runs at load 79:

| People changed | 1 | 16 | 256 | 4,096 |
| --- | --- | --- | --- | --- |
| `jids_of` and hashing | 0.003 ms | 0.011 ms | 0.13 ms | 2.1 ms |

It runs on a reader, once for each batch of events that changes people. 4,096 is a batch's most, as in a contacts sync.

### A message's way to the interfaces: `jawd bench feed`

The command was `jawd bench feed --engine demo --interfaces 4 --far 1000`, on both release builds, interleaved, six rounds each: three with the branch first at load 65–82, and three with main first at load 42–43. It writes 2,000 messages a second for 5 s. Then come 50 lone messages 200 ms apart, quiet, and 50 more while the unread counts and names of 1,000 chats no window shows change 2,000 times a second: 1,000 renames a second, each its own commit.

| Over six rounds | Branch | Main |
| --- | --- | --- |
| in the burst, commit to push, p50 | 27.9–31.0 ms | 27.0–28.8 ms |
| a lone message, p50 | 0.90–1.01 ms | 0.95–1.01 ms |
| … the worst interface's p99 | 2.01–5.12 ms | 2.00–3.97 ms |
| while busy, p50 | 1.39–1.73 ms | 1.30–1.61 ms |
| … the worst interface's p99 | 2.07–3.59 ms | 2.07–15.81 ms |
| jawd's CPU with the interfaces watching | 61.9–76.7% of a core | 65.0–73.8% |

- The busy p50 was higher on the branch in five pairs of six, by 0.09 to 0.35 ms, and lower in one, by 0.18 ms.
- That's within each side's own spread, and the work the branch adds per batch can't account for 0.1 ms: one indexed read of a person's JIDs, 3 µs for one person, and hashing them.
- Every figure is far inside the budgets. The three tails above 5 ms came once each, as the machine's load moved: 5.1 ms on the branch, and 7.3 and 15.8 ms on main.

### The bound

- **How big a window's named set gets.** It holds at most one quoted JID per row, plus the distinct JIDs its rows mention.
  - In the probe's group, where every message mentions two people, a 100-row window names 217 JIDs.
  - Hashbrown allocates 256 buckets of 9 bytes plus 16 for them, 2,320 B, and the bill counts 1,792 B. Beside 52,772 B of rows' JSON, the set is about 4% of the kept read, and the undercount about 1%.
  - At the cap of 2,001 rows of that group, the set would name about 1,790 JIDs: 18.4 KB allocated, 14.3 KB billed, beside about 1.06 MB of rows.
- **The 16 MiB bound holds within about 1% for such windows.**
- **Each window's own copy isn't billed (H2).** At 32 windows of 100 such rows, that's about 74 KB for a connection.
- **The batches.** Each batch of events carries the JIDs of the people changed in it, up to 4,096 people's. A client that lags keeps up to `CHANGES_KEPT` batches (1,024). The batch's `people` set is of that order on main already, and the JIDs add about twice as much again. Only millions of person changes while a client lags would make it large.

## Appendix A: the reconnect probes

For `feed/tests.rs`. They use only helpers main's tests have.

```rust
fn au_named(jid: &str, name: &str) -> Person {
    Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() }
}

fn au_mention(chat: &str, id: &str, at: i64, sender: &str, whom: &str) -> Message {
    let number = whom.split('@').next().unwrap_or(whom);
    let spans = json!([{"t": "text", "s": "ask "}, {"t": "mention", "s": format!("@{number}"), "person": whom}]);
    Message { spans: Some(spans.to_string()), ..text(chat, id, at, Some(sender), &format!("ask @{number}")) }
}

fn au_quote(chat: &str, id: &str, at: i64, sender: &str, quoted: &str, whom: &str) -> Message {
    let reply = json!({"id": quoted, "from": whom, "summary": "quoted words"});
    Message { reply: Some(reply.to_string()), ..text(chat, id, at, Some(sender), "a reply") }
}

fn au_group(before: usize) -> Want {
    Want::Messages { chat: GROUP.into(), anchor: None, before, after: 0 }
}

/// Watches `want` again on a new connection with `since` and the ids `held` holds, as jaw does
/// after a reconnect or a resync, and says what its interface holds then.
async fn au_rewatch(bed: &Bed, held: &Model, want: &Want, since: u64) -> Model {
    let mut again = bed.watcher(64);
    let (method, mut params) = want.params();
    params["since"] = since.into();
    params["have"] = json!(held.ids());
    let result = again.client.call(method, params).await.unwrap();
    let mut model = held.clone();
    model.take(&result, "rows");
    model
}

async fn au_reconnect_after_rename(case: usize) {
    let bed = Bed::new(brisk());
    let (writer, renamed) = (person(DIRECT), person(DIRECT + 1));
    let message = match case {
        0 => au_mention(GROUP, "g1", 2_000, &writer, &renamed),
        1 => au_quote(GROUP, "g1", 2_000, &writer, "q0", &renamed),
        _ => text(GROUP, "g1", 2_000, Some(&renamed), "hi"),
    };
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(au_named(&renamed, "Bea Lopes")),
            Change::Person(au_named(&writer, "Rui")),
            Change::Message(message),
        ])
        .await;
    let watched = watch_all(&mut watcher, &[au_group(10)]).await;
    watcher.settle(seq).await;
    check(&bed, &watcher, &watched, "before").await;
    let held = watcher.windows[&watched[0].0].clone();
    drop(watcher);
    bed.apply(vec![Change::Person(au_named(&renamed, "Bea Martins"))]).await;
    let model = au_rewatch(&bed, &held, &watched[0].1, held.seq).await;
    model.assert_matches(&fresh(&bed.store, &watched[0].1).await, "watched again from its seq after the rename");
}

#[tokio::test(flavor = "multi_thread")]
async fn audit_a_reconnect_from_its_seq_shows_the_new_name_of_someone_mentioned() {
    au_reconnect_after_rename(0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn audit_a_reconnect_from_its_seq_shows_the_new_name_of_someone_quoted() {
    au_reconnect_after_rename(1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn audit_a_reconnect_from_its_seq_shows_the_new_name_of_a_sender() {
    au_reconnect_after_rename(2).await;
}
```

## Appendix B: the randomized probe, which pins T1

For `feed/tests.rs`, with Appendix A's helpers.

```rust
/// A made-up account whose people each have a number and a LID, sent, mentioned and quoted by
/// either, renamed by either, and now and then found to be one person.
struct AuWorld {
    rng: Rng,
    clock: i64,
    next: u64,
    sent: Vec<Message>,
}

const AU_PEOPLE: usize = 6;

fn au_phone(k: usize) -> String {
    format!("3519200000{k:02}@s.whatsapp.net")
}

fn au_lid(k: usize) -> String {
    format!("{}@lid", 84_726_193_520_000u64 + k as u64)
}

fn au_groups() -> Vec<String> {
    (0..3).map(|g| format!("1203630000000001{g:02}@g.us")).collect()
}

impl AuWorld {
    fn jid(&mut self) -> String {
        let k = self.rng.below(AU_PEOPLE);
        if self.rng.below(2) == 0 { au_phone(k) } else { au_lid(k) }
    }

    fn message(&mut self) -> Change {
        self.clock += 1 + self.rng.below(2_000) as i64;
        self.next += 1;
        let groups = au_groups();
        let chat = self.rng.pick(&groups).clone();
        let id = format!("a{}", self.next);
        let sender = self.jid();
        let mut message = text(&chat, &id, self.clock, Some(&sender), "words");
        match self.rng.below(3) {
            0 => {
                let whom = self.jid();
                let number = whom.split('@').next().unwrap().to_string();
                let spans = json!([{"t": "text", "s": "hey "}, {"t": "mention", "s": format!("@{number}"), "person": whom}]);
                message.spans = Some(spans.to_string());
            }
            1 if !self.sent.is_empty() => {
                let quoted = self.rng.pick(&self.sent).clone();
                let reply = json!({"id": quoted.id, "from": quoted.sender, "summary": quoted.summary});
                message.reply = Some(reply.to_string());
            }
            _ => {}
        }
        self.sent.push(message.clone());
        Change::Message(message)
    }

    fn change(&mut self) -> Change {
        match self.rng.below(10) {
            0..=3 => self.message(),
            4..=5 => {
                let jid = self.jid();
                let n = self.rng.next() % 100;
                let person = match self.rng.below(3) {
                    0 => Person { jids: vec![jid], name: Some(format!("Name {n}")), ..Person::default() },
                    1 => Person { jids: vec![jid], push_name: Some(format!("push {n}")), ..Person::default() },
                    _ => Person { jids: vec![jid], verified_name: Some(format!("Biz {n}")), ..Person::default() },
                };
                Change::Person(person)
            }
            6 => {
                let k = self.rng.below(AU_PEOPLE);
                Change::Person(Person { jids: vec![au_phone(k), au_lid(k)], ..Person::default() })
            }
            7 if !self.sent.is_empty() => {
                let k = self.rng.below(self.sent.len());
                let whom = self.jid();
                let mut edited = self.sent[k].clone();
                self.clock += 1;
                edited.edited_at = Some(self.clock);
                edited.text = Some(format!("edited {}", self.clock));
                edited.spans = (self.rng.below(2) == 0).then(|| {
                    let number = whom.split('@').next().unwrap().to_string();
                    json!([{"t": "mention", "s": format!("@{number}"), "person": whom}]).to_string()
                });
                self.sent[k] = edited.clone();
                Change::Message(edited)
            }
            8 if !self.sent.is_empty() => {
                let gone = self.sent.swap_remove(self.rng.below(self.sent.len()));
                Change::DeleteMessage { chat: gone.chat, id: gone.id }
            }
            _ => self.message(),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn audit_windows_match_fresh_queries_through_renames_joins_new_people_and_edits() {
    for seed in 1..=10u64 {
        let bed = Bed::new(brisk());
        let mut world =
            AuWorld { rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1), clock: 1_700_000_000_000, next: 0, sent: Vec::new() };
        let seeding: Vec<Change> = (0..30).map(|_| world.message()).collect();
        bed.apply(seeding).await;
        let mut wants: Vec<Want> =
            au_groups().into_iter().map(|chat| Want::Messages { chat, anchor: None, before: 6, after: 0 }).collect();
        wants.push(Want::Chats { archived: false, anchor: None, before: 0, after: 5 });
        // Two connections on the same windows, the second settling only now and then, so it takes
        // the first's kept reads.
        let (mut first, mut second) = (bed.watcher(64), bed.watcher(64));
        let first_watched = watch_all(&mut first, &wants).await;
        let second_watched = watch_all(&mut second, &wants).await;
        for step in 0..150 {
            let change = world.change();
            let seq = bed.apply(vec![change]).await;
            first.settle(seq).await;
            check(&bed, &first, &first_watched, &format!("seed {seed}, step {step}, the first")).await;
            if world.rng.below(4) == 0 {
                second.settle(seq).await;
                check(&bed, &second, &second_watched, &format!("seed {seed}, step {step}, the second")).await;
            }
        }
    }
}
```

## Appendix C: the terminal's probes

For `crates/jaw/src/view/tests.rs`. Each passes its control (`v` moved), then fails at the same `v`.

```rust
fn audit_drawn_after(rename: impl Fn(&mut Message, &str), bump: bool) -> String {
    let (mut v, rx) = view(100, 24);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down><up><esc>");
    assert_eq!(v.open.as_deref(), Some("climbing"));
    let mut m = theirs("named1", "9999999999990~000000001", NOW, "the text");
    rename(&mut m, "Bea Lopes");
    arrive(&mut v, "climbing", vec![m.clone()]);
    let before = frame(&mut v);
    assert!(before.contains("Bea Lopes"), "{before}");
    rename(&mut m, "Bea Martins");
    if bump {
        m.v += 1;
    }
    arrive(&mut v, "climbing", vec![m.clone()]);
    assert_eq!(v.messages.as_ref().unwrap().get("named1").map(|held| held.v), Some(m.v), "the window holds the row pushed");
    frame(&mut v)
}

fn audit_sender(m: &mut Message, name: &str) {
    m.from = Some(Sender { id: "person:77".into(), name: name.into(), colour: 0 });
}

fn audit_quoted(m: &mut Message, name: &str) {
    m.reply = Some(crate::data::Quote { id: None, from: Some(name.into()), colour: 0, summary: "earlier".into() });
}

#[test]
fn audit_a_senders_new_name_pushed_with_the_same_v_is_drawn() {
    let moved = audit_drawn_after(audit_sender, true);
    assert!(moved.contains("Bea Martins") && !moved.contains("Bea Lopes"), "with v moved:\n{moved}");
    let same = audit_drawn_after(audit_sender, false);
    assert!(same.contains("Bea Martins") && !same.contains("Bea Lopes"), "the same v with a sender's new name:\n{same}");
}

#[test]
fn audit_a_quoted_persons_new_name_pushed_with_the_same_v_is_drawn() {
    let moved = audit_drawn_after(audit_quoted, true);
    assert!(moved.contains("Bea Martins") && !moved.contains("Bea Lopes"), "with v moved:\n{moved}");
    let same = audit_drawn_after(audit_quoted, false);
    assert!(same.contains("Bea Martins") && !same.contains("Bea Lopes"), "the same v with a quoted person's new name:\n{same}");
}

fn audit_chat_drawn_after(bump: bool) -> String {
    let (mut v, rx) = view(100, 40);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down>");
    let before = frame(&mut v);
    let mut tiago: Chat = v.chats.get("tiago").unwrap().clone();
    assert!(before.contains(&tiago.name), "{before}");
    tiago.name = "Tiago Martins Reis".into();
    if bump {
        tiago.v += 1;
    }
    v.apply(Update::Chats(jaw_client::Update { upsert: vec![tiago.clone()], total: Some(13), ..Default::default() }));
    assert_eq!(v.chats.get("tiago").map(|held| held.name.clone()), Some(tiago.name.clone()), "the list holds the row pushed");
    frame(&mut v)
}

#[test]
fn audit_a_chats_new_name_pushed_with_the_same_v_is_drawn() {
    let moved = audit_chat_drawn_after(true);
    assert!(moved.contains("Tiago Martins Reis"), "with v moved:\n{moved}");
    let same = audit_chat_drawn_after(false);
    assert!(same.contains("Tiago Martins Reis"), "the same v with a chat's new name:\n{same}");
}
```

## Verdict

Changes needed before this lands:
- **R1:** a rename must raise the `v` of the message rows it changes, so a watch from a seq and the terminal get the new name. The derivation is above. The protocol should say so again, and my three reconnect probes become tests that fail without it. The other way is a watch from a seq that resends every row naming someone changed since, plus a terminal cache keyed by content, plus a change to jaw-mac's design.
- **T1:** a test that fails when `meet` looks up only one of the smaller set's items.

Then ask for another audit. These can stand as follow-ups:
- H1: one transaction for the events and their JIDs;
- H2: the named set's bill and its test;
- H3, the feed audit's;
- chat rows' `v` under a rename;
- an event when `me` changes;
- an event for the chat being merged away at each step.

Verdict: changes needed

# Verdict of round two (5a9e6a4)

R1 and T1 are closed. A rename now raises the `v` of the message rows it changes, a row's `v` never goes back, and `meet`'s rule has its test.

One thing must change before this lands:
- **T2:** a test that fails when a row's `v` takes any but the newest of its mentioned people's versions. Appendix D's fails 3 of 3 with the oldest taken, and passes on the branch. Appendix E's two randomized probes are worth adopting with it.

Then ask for another look. These can stand as follow-ups:
- H4: the plan test failing on SQL it doesn't plan;
- round one's H1, H2 and H3;
- chat rows' `v` under a rename;
- an event when `me` changes, with a new `v` for the rows it changes;
- an event for the chat being merged away at each step.

Verdict: changes needed

# Verdict of round three (9666937)

Nothing marked data loss or inconsistency remains. Round two's results hold on the rebased code and on a trial rebase onto `32b8eed`, which is clean, with nothing to resolve. But the rule T2 was about still has a slip no test catches, so the branch can't land yet.

What must change before landing:
1. **T3:** replace Appendix D's test (`feed/tests.rs:1859-1888`) with Appendix F's, which adds a row mentioning the two the other way round. It fails 3 of 3 with the oldest version taken and with the aggregate left off, and passes 3 of 3 on the branch.
2. **Then ask for another look.** If nothing else changes, it only needs to check that test against those two mutations.

To land, once approved:
1. Rebase onto main. At `32b8eed` there's nothing to resolve.
2. Give the commits that touch the data layer, all three, the trailer `Data-Audit: docs/audits/2026-10-26-feed-names.md`.
3. Commit this note in the branch, and run `make check-audit`.

These may stand as follow-ups:
- H5: Appendix E's two probes as ignored tests;
- H4: the plan test failing on SQL it doesn't plan;
- round one's H1, H2 and H3;
- chat rows' `v` under a rename;
- an event when `me` changes, with a new `v` for the rows it changes;
- an event for the chat being merged away at each step.

Verdict: changes needed

# Verdict of round four (279d781)

T3 is closed, and the probes are in and run as printed. Nothing marked data loss or inconsistency remains, and every rule the change touches has a test that fails without it. Round two's guarantees hold on the trial rebase onto `e2af6a1`. The branch can land.

To land:
1. Rebase onto main. At `e2af6a1`, resolve as "The trial rebase onto e2af6a1" in round four says:
   - in `3788685`, keep both columns, main's thumbnail first, set `MESSAGE_COLUMN_COUNT` to 29, and read `quoted: r.get(28)?`;
   - in `3788685` and `54a79c7`, take `plans.txt` from main and write it again with `JAW_PLANS=write cargo test -p jawd plans`.

   The fourth commit's tree is then `3267c7b3ca7541e498c072a3e6780bb401665857`. If main has moved on, the result must have these two resolutions and no other, and the plan test and the quote test must pass. Anything more to resolve in the data layer needs another look.
2. Give all four commits, each of which touches the data layer, the trailer `Data-Audit: docs/audits/2026-10-26-feed-names.md`.
3. Commit this note in the branch, and run `make check-audit`.

These stand as follow-ups, and none of them blocks:
- With `folders`' version of presence:
  - on `folders`, a store test that presence, typing, following, about, a one-to-one chat's avatar and filing a person leave `people.v` alone;
  - in whichever of the two lands second, a feed test that such a change moves no message row's `v` and sends no message row;
  - typing out of `people`, as the protocol says, and main's sentence on presence being sent when a person's `v` changes, changed with it.
- H6: the other half of the contract, in `MESSAGE_COLUMNS`' comment.
- H4: the plan test failing on SQL it doesn't plan.
- Round one's H1, H2 and H3.
- Chat rows' `v` under a rename; an event when `me` changes, with a new `v` for the rows it changes; an event for the chat being merged away at each step.

Verdict: approved
