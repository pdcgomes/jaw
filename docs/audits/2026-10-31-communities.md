# Audit: communities grouped in the chat list (M2)

# Round three: `77d44257..60f83020`

Range: `77d44257..60f83020` in `communities` (`~/src/jaw-communities`), 1 November 2026. Auditor: database-expert. Fourteen commits on `origin/main` `77d44257`, which has the library (migration 25, a data step): round two's eleven, rebased, and the fix round's three tests. The migration is 26 here. Main has since moved to `5dfefb1e`, presence, which has no migration.

## In short

- **Verdict: approved.** Nothing is lost and nothing is inconsistent, and every rule the branch touches has a test that fails with the rule switched off.
- **Round two's items are fixed as the note meant.** T5, T6 and O4 are P8, P9 and P10 adopted as printed, each a commit of its own. Each fails with its rule switched off, and is then the only failure among the 53 community, folder, reconnect and typing tests: T5's under N5, N6 and N7, T6's under N9, O4's under N3 and N4. K9, N10 and the protocol's sentence on a mute running out stay follow-ups, as round two allowed.
- **The rebase kept both sides and nothing more.** I replayed each of the 14 commits onto its rebased parent with `git merge-tree`, which no rerere can reach:
  - each rebased tree is the clean replay's outside the seven conflicted files;
  - each conflict is resolved by both sides, but for the renumbering to 26;
  - authors, dates and trailers are unchanged, and only two messages changed, each to say 26.
- **The library is as it was beside communities.** Probe P11 holds library windows and chat lists, communities open and closed, on one connection through five changes and a reconnect, and each window is what a fresh watch gives. 21 of the library's 23 switches fail its tests, the six its round three left surviving among them, now killed by its adopted T4, T5 and T7. The other two are equivalent, as its audit said.
- **`kept.rs` is right for both sides.** It is main's file with exactly round two's three changes. Kept's test passes through 26, and the K switches come out as in round two.
- **I1 holds beside the library's windows.**
  - `settle` drops `since` only for the tree and chat lists.
  - A library window is always watched whole: its `since` is a time, and it's watched with no `have`. So the I1 filter can't reach it; my switch that lets it is equivalent.
  - The I1 switch still fails its six tests.
- **The full suite at the head passes under the flakes rule, but for two of media's timing tests that aren't on the list (O6).**
  - `make test` ended with exit 2: jawd's unit tests had 909 passed and 8 failed.
  - Six of the failures are on the landers' list. Two aren't by name: media's photo-flood and save tests (O6), which the branch doesn't touch.
  - All eight pass alone, 3 of 3 each, and the targets `make test` didn't reach pass on their own.
- **Landing:** contact cards takes 26 next, and this branch then takes 27. I tried that landing: this head merged onto contact cards' landing trial, each conflict kept both ways, the migration 27. `plans.txt` written again changed nothing, and holds exactly contact cards' entries with this branch's. See "Landing" below the verdict.

## Round two's items, each checked

I applied every switch myself, in one build of this head with each rule behind `crate::mutant(NAME)`, chosen by `JAW_MUT`:

- round two's 42 switches as its `switches.py` wrote them, with kept's moved from migration 25 to 26;
- the library's 23 from its round three, written again at this head;
- two of mine;
- probe P11.

Each switch ran from the clone's crate with the sources back at the head, so the plan test read the branch's own statements. M9 edited the source and ran the head's own build. Method and counts are under "Measurements".

- **T5, closed** by `194e1291`, `a_community_a_list_holds_for_itself_and_for_groups_of_it_counts_once_in_its_total`, P8 as printed. It fails under:
  - N5 (a folder of yours' exclusion dropped): `folder:9`'s total is 4 for 3 rows;
  - N6 (Unread's): `unread`'s is 6 for 5;
  - N7 (a smart folder's): `smart:10`'s is 5 for 4.
- **T6, closed** by `32a6d0f7`, `a_community_a_list_doesnt_hold_sits_where_the_groups_of_it_there_do_though_its_own_message_is_newer`, P9 as printed. It fails under N9 (`own` always true), with Unread showing only `["Ana", "Regata"]`.
- **O4, closed** by `60f83020`, `a_closed_communitys_last_is_an_open_ones_when_two_groups_newest_messages_share_a_millisecond`, P10 as printed. It fails under N3 (open's tie broken the other way), closed showing `x1`, and under N4 (closed's), closed showing `f1`.
- **The rest of round one's and two's switches** at this head:
  - I1 fails its six tests: the four `watched_again_after_…`, the reconnect test and the typing test.
  - Every other switch is killed by the tests round two names: MSGS, TYPING, M1 to M9, N1, N2, N11, N12, N15, O1, O2, O3's three (by `find`'s test), T4B, TMEMBER, and K1, K3, K4, K5, K7, K9M and K10.
  - K2, K6, K8 and K10M, which model main's old `kept.rs`, pass, as they should. So does K9, round two's one-row trade-off.
- **The follow-ups:**
  - **K9:** a second media seed row, an HD copy kept `expired`; K9 still passes.
  - **N10:** the held window's own filter, main's rule, untested on main too; N10 still passes every test of its set.
  - **The protocol's Smart folders sentence** on a mute that runs out isn't written. The fix round left it so as not to run the gate again.
  - **U1 and X1** stand as round one wrote them.

## The rebase onto the library

- **Every conflict kept both sides.** `git merge-tree` of each original commit onto its rebased parent conflicts in the trial's seven files and no others. In each, the resolution is main's side then the branch's, but for two things.
  - **Two rows, where each side changed one line.** Both changes are kept on that line: the README's `watch.move` row has "and `open` for a chat list" and "a library window keeps its kind and filters". `feed/tests.rs` has `mod communities;` before the library's `pub(super) fn ms`.
  - **The renumbering:** the lock's `26 schema ddc73e7f…`, its hash unchanged; `schema.rs`'s `// 26` comment; and its two docs.
- **Nothing beyond that in a data-layer path.**
  - Every rebased commit's tree is a clean replay's outside those files.
  - `kept.rs` is main's with exactly round two's change: its added and removed lines are the same, line for line.
  - `plans.txt` is the library's 689 entries with exactly this branch's 40 added or changed and 28 removed or changed against `e12d8d34`, none planned otherwise, and the plan test passes on it.
- **What the library meets in this branch's code,** read where the two touch:
  - **`settle`** drops `since` for `Kind::Folders` and `Kind::Chats` only.
  - **`library.watch`** calls `watch` with no `since` and an empty `have`, its own `since` being a time. With nothing held, every row comes, so hello's `since` changes nothing.
  - **`relocate` and the flush** pass no `since`.
  - **`watch.move`** keeps each kind's spec: a library window its filters, a chat list its `open` when the move leaves it out.
  - **The shared reads** are keyed by the whole spec, `open` included.
  - **The rows:** a library row's `chat_name` comes from `library::chat_names`, which reads its own columns. The column this branch adds to `CHAT_SELECT` is its last, so neither changes the other.
- **A library window and a chat list each get exactly what they should** (P11, which passes at the head).
  - **The windows,** on one connection: All open, All with `open: []`, Unread, the Communities folder with Bairro open, and four library windows: photos across chats, `all`, Festa's and Bairro's own.
  - **What the library shows:** each photo with its own chat's name. A group's is the group's, a community's own is the community's, and Regata's is its own though jaw doesn't have its community.
  - **The changes:** a photo in a group, the group renamed, it leaving its community, another archived and a community learned late. After each, every window equals a fresh watch.
  - **The reconnect:** on a new connection with hello's `since`, a photo and a word arrive meanwhile and the group goes back to its community. Each chat list watched again with `have`, and each library window watched whole, equals a fresh watch too.
  - **Two library switches fail it,** a window not keeping its chats and not read again when they change, beside the library's own test.
- **The library's rules still fail their tests.**
  - Of its 23 switches, 21 fail the library's tests and P11.
  - The six its round three left surviving (its `results.tsv`) fail now: `State.counts` and `State.chats` not kept, `older` and `counts` on every push, the window not read again when a chat its items are in changes, and the window read with the conversations. They're caught by the three tests it adopted at landing, and P11 catches the two of chats too.
  - `note` cleared and the gone chat's arm are equivalent, as its audit said.
- **`kept.rs`** is right for main's seeds and this branch's:
  - main's seven derived classes of the library;
  - this branch's media columns of jaw's own, with values of their own in the `downloading` seed row;
  - plain inserts but `media_made`'s;
  - main's one set of rows for `media_files`, `media_made` and `media_drop`.

  The library adds no seed row, and kept's test passes through 26. K1, K3, K4, K5, K7, K9M and K10 fail it, with this branch's migration at 26 in K1, K3 and K10.

## Findings

### Data loss

None.

### Inconsistency

None.

### Tests (missing)

None. T5 and T6 are closed. Every switch of the branch's rules fails a test, but N10, which is main's rule, and K9, the one-row trade-off; both are follow-ups.

### Unpredictable performance

- **U1** stands as round one wrote it, a follow-up. The window's read is round two's: `store/lists/communities.rs` is unchanged by the rebase.

### Other

- **O5. The Mac's parity plan has two steps 3.9.**
  - Contact cards' landing trial adds "3.9 Contact cards" and this branch "3.9 Communities in the chat list", at the same place of `docs/plans/2026-10-27-jaw-mac-parity-plan.md`.
  - This branch's terminal commit (`3f05d1ca`) says "The parity plan's step 3.9 says what the Mac needs for it".
  - `mac/parity.toml` doesn't name this branch's step: it adds no request or push.
  - **Fix, at landing:** the later to land takes 3.10, this branch after contact cards, and its terminal commit's message says 3.10.
- **O6. Two of media's timing tests that aren't on the landers' list failed in full runs.**
  - **`a_save_waiting_for_its_file_holds_up_nothing_else_on_its_connection`** failed in both of my full runs, the head's and the trial's ("status came first: None": neither answer came within its 5 s). It passed alone 3 of 3 at each.
  - **`a_flood_of_photos_never_holds_up_a_message`** failed once, at the head ("a message waited 501 ms behind photos"), and passed alone 3 of 3.
  - Both runs were at loads of 36 to 80, the head's with my switch runs beside it. The branch touches nothing of media's.
  - **For the list:** the save test has been seen twice, so it belongs on the landers' list; the flood test is to watch for a second sighting. Neither blocks this branch.
- **Carried:**
  - K9's second media seed row, N10's catch-up test, and the protocol's Smart folders sentence on a mute that runs out.
  - The library's own equivalents (`note`, the gone chat), as its audit left them.

### Outside this range

- **X1** (main's) as round one wrote it.

## Measurements

I worked in a scratch clone at `60f83020`, with debug test builds (`CARGO_INCREMENTAL=0`) under `nice`. The load was 25 to 80 on 20 cores, from other agents' builds and tests.

**The replay.**
- `git merge-tree --write-tree --merge-base=<original>~1 <rebased>~1 <original>` for each of the 14 commits, the three new ones replayed from the fix round's `bcd57089`, `4f5d1c16` and `052b9aa4`. I compared each result's tree with the rebased commit's.
- With diff3 markers, I checked each conflicted file's resolution against both sides kept, in either order.

**The switches.**
- **The build:** `switches.py` made 45 edits in 9 files, each matching once: round two's 42 switches, the library's 23 and two of mine. P11 went in as `feed/tests/audit_r3.rs`. One build served them all.
- **How each ran:** from `crates/jawd`, with the sources back at the head, `--skip measure_`, against its rule's set:
  - **the community set, 53 tests:** the community tests (20), `feed::tests::folders::` (15), the folder-store tests (15), the reconnect and typing tests, and P11;
  - **`find`'s 22;**
  - **kept's test;**
  - **the three plan tests;**
  - **the library set, 17:** `feed::library::` (14), its two tests in `feed::tests`, and P11.
- **With no switch:** every set passes. The community set takes 40 s, the library set 13 s.
- **M9:** one member read written `NOT INDEXED` on disk, then the head's own build's plan test run on it, which failed. Then the source went back.

| | Rule switched off | Result | Caught by |
| --- | --- | --- | --- |
| N5, N6, N7 | `total` counting a community that a folder of yours, Unread or a smart folder holds itself | killed | T5's test alone: totals 4, 6 and 5 for 3, 5 and 4 rows |
| N9 | a community's own place counted where the list doesn't hold it | killed | T6's test alone: Unread `["Ana", "Regata"]` |
| N3, N4 | a tie in time broken the other way, open or closed | killed | O4's test alone: closed `x1`, then `f1` |
| I1 | `since` kept for chat lists | killed | the four `watched_again_after_…`, reconnect, typing |
| MSGS, TYPING | `since` dropped for a chat's messages too; the typing rule | killed | reconnect and typing; typing |
| M1, M2 | the versions a community takes with its groups | killed | counts, folders' re-watch; and newest group leaves |
| M3 to M8, N1, N2, N11, N12, N15, O1, O2, T4B | round two's | killed | the tests round two names |
| M9, TMEMBER | a member read scanning `chats`; `{MEMBER}` unknown to the plan test | killed | the plan test (`SCAN c`) |
| O3A to O3C | `find`'s three | killed | `a_community_ranks_by_the_newest_of_itself_and_every_group_of_it` |
| K1, K3, K4, K5, K7, K9M, K10 | kept's, migration 26 for K1, K3 and K10 | killed | kept |
| K2, K6, K8, K10M | the same with main's old `kept.rs` | pass | (controls) |
| K9 | migration 20 resetting `expired` | passes | (one row; follow-up) |
| N10 | a held window keeping a chat the list left | passes | (main's; follow-up) |
| the library's, 15 | its kind, a result's counts, older and total, a push as messages', pushes' older, counts and total never, an empty push for every read, what reads it again (people, a chat's messages, its chat, any message), `shows`, `chats` | killed | the library's tests |
| the library's, 6 | `State.counts` and `State.chats` not kept, `older` and `counts` on every push, not read again when a chat its items are in changes, the window read with the conversations | killed | its adopted T4, T5 and T7; P11 the two of chats |
| the library's, 2 | `note` cleared; the gone chat's arm | pass | (equivalent, as its audit said) |
| L-SINCE (mine) | a library window watched with its `since` given as a seq | passes | (equivalent: watched with no `have`) |
| C-MOVE-OPEN (mine) | `watch.move` ignoring a new `open` | killed | `a_community_a_window_hasnt_open_is_one_row_…` |

**The full suite at the head.**
- **The run:** `make test`, under `nice`, `timeout -k 60 7200` and `TMPDIR` in scratch, from 20:29 to 21:21, at a load of 36 to 67 with my switch runs beside it.
- **The result:** exit 2. The engine's three Go packages and the 13 Rust targets before jawd's unit tests passed. jawd's unit tests had 909 passed, 8 failed and 18 ignored, in 2,778 s.
- **The eight failures, each run alone 3 of 3, all passed:**
  - **on the list:** the lane (`a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`), the half-closed connection (`a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`), media's starts check, the socket's connection-full test, the WAL bound ("a live write waited 1.38 s") and the checkpointer's flush;
  - **not on the list by name:** media's flood and save tests (O6).
- **The targets `make test` didn't reach,** each run on its own, all passed:
  - jawd's `installed`, `starting`, `stopping` (4) and `upgrading` (version 1 through 26);
  - wa-proto's library and `roundtrip` (3);
  - the doc tests (none in any crate).

**The trial landing.**
- **The trial:** `f0d35804`, made in the object store alone by `trial.py`: `git merge-tree` of `60f83020` onto contact cards' landing trial `52699741` (contact cards' 17 commits on presence's trial `154979a2`), from `77d44257`.
  - Five files conflict: the lock, `schema.rs`, jaw-client's tests, the parity plan, and the README twice.
  - The script refuses any conflict but these and keeps both sides in each, making this branch's migration 27 and its parity step 3.10.
  - Landed presence (`5dfefb1e`) is its trial with the landing note and a test of the library's added. Neither meets this branch.
- **`plans.txt`:** built at the trial, `JAW_PLANS=write` changed nothing. It holds contact cards' 696 entries with this branch's 40 and 28: 708, none extra, missing or planned otherwise.
- **jawd's unit suite at the trial:** 942 passed, 7 failed and 18 ignored, in 2,835 s at a load of 50 to 80.
  - Among the passes: kept's test through 27, the fault at every statement (data step 25, then 26 and 27 in one schema transaction), both migration tests, the 20 community tests, contact cards' four backfill tests, the library's 14, `find`'s 22, presence's 20, the reconnect and typing tests, and the plan tests.
  - **The seven failures,** each passing alone 3 of 3: the lane, the half-closed connection, the lone change's coalescing, the connection-full test, the checkpointer's flush and its locks test, all on the list; and media's save test again (O6).
- **`upgrading` at the trial:** from version 1 through 27, it passed.

# Round two: `e12d8d34..fa932866`

Range: `e12d8d34..fa932866` in `communities` (`~/src/jaw-communities`), 1 November 2026. Auditor: database-expert. Eleven commits on `origin/main` `e12d8d34`, which has places and events (migrations 21 to 24): round one's three, rebased, and the fix round's eight. The migration is 25 here.

## In short

- **Verdict: changes needed,** for two test gaps (T5, T6), each closed by a probe printed below that passes on the head and fails with its rule switched off. Nothing is lost and nothing is inconsistent.
- **Every round-one item is fixed as the note meant,** and each fails its tests when I switch its rule off: I1 by fix (a), T1 to T4, O1 to O3. U1 stays a follow-up and X1 is main's.
- **I1 holds** for a mute running out, a folder moved, a rule changed and a folder made a favourite: a chat list watched again is read whole at that moment. Live windows were already right, and no other promise of the protocol changes.
- **`kept.rs`'s reconciliation is right** in each of its three changes, places and events' landing holds under it, and the fix round's K1 to K8 reproduce. What the other waiting branches do with theirs is under "`kept.rs`".
- **The two test gaps** are in code round one approved, found with finer switches:
  - T5: a chat list's `total` would count a community twice where Unread, a folder of yours or a smart folder holds it for itself and for a group of it, and no test would fail.
  - T6: a community a list doesn't hold, whose own message is newer than its groups', would sit at its own place, and it and its groups would leave the window (Unread shows only Ana and Regata), and no test would fail.
  - The code is right in both cases.
- **One other (O4):** O2's tie-break between two groups' messages in the same millisecond is untested.
- **Landing:** a trial with the library landed first and this branch's migration as 26 merges with seven keep-both conflicts. `kept.rs` merges cleanly, `plans.txt` written again holds exactly both sides' entries, and on the trial kept's test through 26, the schema, plan, library and community tests, the fault at every statement and the upgrade from version 1 all pass.

## Round one's findings, each checked

I applied every switch myself: one build with each rule behind an environment variable, run from the clone's crate with its sources back at `fa932866` (the plan test reads them). The logs' method and counts are under "Measurements".

- **I1, closed by fix (a).**
  - **The change:** `settle` (`feed/client.rs:652`) drops `since` for chat lists, as it did for the tree. A `have` entry holds a zero hash (`:1276`), so `changed` sends every row of the window, and `remove` lists the rows `have` named that left. jawd already read the window whole, so the only new cost is on the wire: the window's rows, with an open community's groups.
  - **Tests:** P1, P2, P3 and P7 as the four `watched_again_after_…`. The reconnect test's chat list comes whole. The typing test checks its left-out row on a quiet chat's messages now, with a precondition that there's one to leave out.
  - **Switches:**
    - I1 off (`kind != Kind::Folders`) fails all six.
    - The typing rule off fails the typing test alone.
    - `since` ignored for a chat's messages too fails the reconnect test and the typing test's quiet chat, so the moved check isn't vacuous.
  - **It holds for each of the four:** a watch again reads the list at its own `now`. So a mute that has run out is reflected (P3 sleeps past it), and nothing about a folder moved, a rule changed or a folder made a favourite (each of which stamps only folder rows) can be missed.
  - **Live windows were already right.** `touched_by` (`feed/client.rs:851`) reads a chat list again on any chat, person or folder change and pushes each row whose hash changed, so the three folder changes reach live windows. A mute running out reaches a live window at its next read for a change, as a smart folder's `waiting` does on main. The protocol's Smart folders paragraph says so only of `waiting`; adding mutes there would be accurate.
  - **The protocol's other promises hold:**
    - a chat's messages and a roster still come back with only what changed (the third switch above);
    - `watch.move` compares by hash and never used `since`;
    - the tree is as it was;
    - the same-`v` rule now names a community's sums;
    - jaw-client and the Mac send no `since` for chat lists (`crates/jaw-client/src/client.rs:742`, `mac/JawKit/Sources/Client/Client.swift:224`), so neither changes.
- **T1, closed.** `{MEMBER}` is in `is_statement`, and the six member reads are pinned. Each searches `chats_by_community`; a folder's read also searches `folder_chats_by_chat`, and a smart folder's rule's subqueries and O2's lookup of `messages` go by key. M9 (`NOT INDEXED`) fails the plan test with `SCAN c`, and so does `{MEMBER}` taken out of `is_statement`.
- **T2, closed.** M5, M6 and M7 each fail `hasnt_open…`. So do my N1 and N2: All's and Communities' closed reads without their archived filter.
- **T3, closed.** M4 fails both new tests, and O1's too.
- **T4, closed.** M8 fails the held test, and so does a held read that ignores `open`.
- **O1, closed.** The groups' newest message put back in `community_row` fails its test, and so do M4 and N15 (a community keeping its own `last` over its groups').
- **O2, closed.** Its tie-break is untested (O4 below).
- **O3, closed.** Each of the three switches fails the test: the community map emptied, `find`'s tie by a community's own `last_at`, and the recent list by it.
- **U1, a follow-up.** The fix round measured it on the rebased branch: All in 3.92 ms at p50 and 8.01 ms at p99 with every community closed, 7.93 and 11.42 ms with every one open, against main's 1.34 and 3.33 ms. The window's read hasn't changed since.
- **X1:** main's, unchanged.
- **Round one's M1 to M3** still fail their tests (two, three and one).

## Findings

### Data loss

None.

### Inconsistency

None.

### Tests (missing)

- **T5. Nothing tests that a chat list's `total` counts a community once where Unread, a folder of yours or a smart folder holds it for itself and for a group of it.**
  - **Where:**
    - `brought` (`store/lists.rs:409`) counts the communities a list holds only for their groups, the rows it has beside its chats.
    - Each form leaves out a community the list holds itself: Unread's one itself unread (`:414`), a folder's one filed in it (`:422`), a smart folder's one its rule takes (`:435`).
    - `one_row_wherever…` checks seven lists' totals, but none of these three lists holds Bairro itself there, so the exclusions never apply. The one statement All, Archived and Communities share is tested, through All, and Groups never holds a community.
  - **Switches:** N6, N5 and N7 each drop one exclusion, so such a community counts twice. Each passes every `feed::` and `store::` test but timing ones (under "Measurements").
  - **In use:** `total` is how many rows the whole list has, which an interface sizes its scroll bar by (the protocol's Windows). A regression here would count one row twice.
  - **Fix:** adopt P8. Bairro is marked unread beside its unread groups and filed in Family beside Festa, and a smart folder holds Bairro whole; each list's `total` must equal its rows. It passes on the head and fails under each of N5, N6 and N7.
- **T6. Nothing tests that a community a list doesn't hold sits where the groups of it there do, even when its own newest message is newer. If it sat at its own place, it and those groups would leave the window.**
  - **Where:**
    - `gather` (`store/lists/communities.rs:141`, `:163`) counts a community's own place only when the list holds it (`own`).
    - A window gives a community from the row it reads at the community's place (`:114`).
    - A list that doesn't hold the community never reads a row at its own place. So if `own` were true there, the community would never be given, and its groups, waiting for it, would be skipped.
  - **Switch:** N9 (`own` always true) passes every `feed::` and `store::` test but the long-request lane. With P9's set-up, Unread then shows only `["Ana", "Regata"]`: Bairro and its two unread groups are gone.
  - **In use:** Unread, Groups, folders of yours and smart folders hold communities' groups without the community. And a community has messages of its own: history writes "Group added to the community: …" to its chat (`convert/tests.rs:776`), and such a line can be newer than its groups' messages.
  - **Fix:** adopt P9. Bairro gets a message of its own, read, newer than every group's. In Unread, Groups, Family and Mentions me it must sit at its newest group's place with them, and in All, which holds it, at its own. It passes on the head and fails under N9. This is round one's T3 the other way round.

### Unpredictable performance

- **U1** stands as round one wrote it: a follow-up.

### Other

- **O4. O2's tie-break is untested.**
  - Open and closed both take, of two groups whose newest messages share a millisecond, the one with the larger `n` (`communities.rs:150`, `:155`).
  - My N3 and N4 each reverse one side, so closed and open would show different messages. Each passes every test I ran.
  - **Fix:** adopt P10 (two groups' messages in one millisecond, closed as open), which fails under each. A follow-up; it may stand with an approval.
- **K9. One media seed row exercises one branch of a condition.**
  - With `downloading` seeded, a migration that changes only `expired` rows passes kept's test: my K9 does it to migration 20's reset, which is moot, since 20's lock line freezes it.
  - With main's `expired`, the reverse passes: my K10, a later migration resetting `downloading` to `remote` as 20 did, which the branch's row catches.
  - So `downloading` is the better single row, being the state a reset like 20's touches. A second media row, an HD copy kept `expired`, would hold both. A follow-up.
- **N10. The held window's own filter is untested.** A chat that leaves the list leaves a window held while catching up (`communities.rs:288`). That's main's rule, which main's held read had too, carried into `among`. My N10 (the filter off) passes every `feed::` and `store::` test, main's catch-up tests among them, so no test held it on main either. A follow-up for the catch-up tests.
- **`plans.rs` now skips folders named `tests`,** for `feed/tests/`. The only such folder holds tests. Noted; nothing to do.

### Outside this range

- **X1** (main's) as round one wrote it.

## `kept.rs`

**Each of the three changes is right.**

- **`CLASSES` gains media's seven columns of jaw's own** (`kept.rs:52`): `error`, `wanted`, `asked`, `arrived`, `retried_at`, `cancelled` and `direct_path`. Migration 20's own class comment names exactly these, with `state` and `progress`, which were there already. K1, a migration changing `wanted`, fails the test; K2, the same with `wanted` left out as main's list has it, passes.
- **Media's seed row is `downloading`,** with a value of its own in each of the seven columns (`:263`).
  - Each value is neither NULL nor its default, as `filled()` requires.
  - Migration 20's `CHECK`s accept the row: `wanted` is set, as `downloading` and `arrived` require.
  - At version 19, migration 20's reset takes it to `remote`, which the test checks. K7 (the reset taking it to `gone`) fails; K8, the same with main's `expired`, passes.
  - It has the one-row trade-off of K9 above.
- **`seed()` writes every row with a plain `INSERT` but `media_made`'s,** which `INSERT OR REPLACE` writes over the row migration 20 makes (`:518`, `:521`).
  - K4 (`media_made` plain) fails at seeding.
  - K5 (a seed row twice) fails at seeding; K6, the same with main's upsert of every row, passes.
  - Places and events upserted every row only so that `media_made`'s row would take the seed's values (its audit's L1). The plain inserts passing at every version show no other migration writes a seeded table's row into an empty `jaw.db`.
- **One set of rows for `media_files`, `media_made` and `media_drop`:** main's, once each in `SEEDS`. K3, migration 25 changing that `media_files` row, fails.

**Places and events' landing holds under it.** Kept's test passes at every migration from 2 to 25 with plain inserts and `downloading`, its 21 to 24 among them. Its seed rows for `event_answers`, `openings` and `unopened` go in plainly, and K3 catches a change to its `media_files` row.

**What the other waiting branches do with theirs.** `git merge-tree` of each against this head:

- **The library** adds seven derived classes and no seed row. Its migration writes no row of a seeded table into an empty `jaw.db`: its inserts read `messages` and `links`, and its FTS settings go into a derived table. `kept.rs` merges cleanly, and on the trial kept's test passes at 25 and 26. Nothing to do.
- **Media out and chat actions:**
  - Media out adds `outgoing` and `outgoing_drop`, and chat actions `app_state_awaiting`, each with its class and seed row at the end of `CLASSES` and `SEEDS`. Those merge with this branch's `kept.rs` without a conflict, landing before or after it.
  - Under plain inserts, a seed row that collides with a row a migration or trigger wrote fails at seeding ("give SEEDS what it needs") instead of quietly replacing it. Media out's new trigger on `media_files` drops only a file no copy names, and media's seed row names the seeded file, so nothing collides.
  - If a migration of theirs writes a seeded table's row by design, as 20 writes `media_made`'s, they add that table to `MADE_WITH_THEIR_ROW`.
  - They don't go back to upserting every row, and they keep media's `downloading` row with its seven columns.
- **Contact cards** doesn't change `kept.rs`. Its migration's `backfills` row needs messages (`WHERE name = 'messages' AND seq > 0`), so it writes nothing into an empty `jaw.db`, and beside the seed's `links` row it collides with nothing.
- **Disappearing** is on `9c745877`, before places and events. Its `kept.rs` carries `kept-media-seeds`' design: `SEEDS_SINCE`, with media `waiting_for_phone` from 20 on and its own rows for `media_files`, `media_made` and `media_drop`, each written over.
  - It conflicts with main's `kept.rs` whether this branch has landed or not.
  - Its lander keeps one design, this branch's: plain inserts, `MADE_WITH_THEIR_ROW`, media `downloading` with the seven columns, and main's one set of the three tables' rows. It drops `SEEDS_SINCE`, its rows and its `seed()`, and adds only its `opened` class and seed row.
  - Kept as well, `SEEDS_SINCE` would quietly leave main's rows for those tables unused from 20 on.
- **`kept-media-seeds`** (`4c72954c`) is superseded by places and events and this branch, and shouldn't land.
- **Whichever lands first,** the later one keeps the earlier one's `CLASSES` and `SEEDS` lines. If this branch is the later one, its lander applies its three changes on top, as the fix round did over places and events'.

## Landing

For the round that approves it.

- **The order:**
  - The library lands first, as migration 25, a data step.
  - Presence then lands with no migration and no change to the lock, `schema.rs` or `kept.rs`.
  - This branch then takes 26.
  - If another waiting branch with a migration lands first (media out, chat actions, contact cards and disappearing each claim 25 now), it takes the next free number the same way.
- **Taking the number:**
  - Rebase with rerere off. In `migrations.lock`, main's lines, then `26 schema ddc73e7f…`, with its kind and hash as they are.
  - In `MIGRATIONS`, `COMMUNITIES_GATHERED` goes after the library's text.
  - Its `// 25` comment, its constant's doc and its measurement's doc say 26, and so do the jawd commit's and O3's messages.
  - Its text is unchanged, so its hash holds, and its tests find it by its text.
- **What the trial met** (the library merged into this head, as a rebase's last commit would have it; "Measurements" has the method):
  - Seven files conflict, each resolved by keeping both sides:
    - the lock;
    - `MIGRATIONS`;
    - `read.rs`, the library's `chat_name` beside `OWN_JOINS`;
    - `window.rs`'s imports;
    - `feed/tests.rs`, `mod communities;` beside the library's `pub(super) fn ms`;
    - the protocol's `watch.move` row, which gets `open` for a chat list and keeps "a library window keeps its kind and filters";
    - `plans.txt`, written again.
  - `kept.rs` merges cleanly.
  - Against presence alone, only jaw-client's tests and the protocol's text conflict. Presence's own landing on the library meets `feed/client.rs`, `feed/window.rs` and `feed/tests.rs`, so this branch's rebase will meet presence's resolution of those three.
- **A data step, then this schema step:** 26 runs in a transaction of its own after the library's 25. Both run in `Store::open`'s `migrate`, before any reader opens, so no chat list is read between them, and the one backup is taken before 25.
- **The lander re-checks:**
  - `JAW_PLANS=write`, then that `plans.txt` holds exactly main's entries and this branch's: 40 added or changed and 28 removed or changed against `e12d8d34`, as on the trial.
  - Kept's test at 26.
  - The two migration tests, which build `jaw.db` at the version before it.
  - The fault at every statement, now with a data step before 26.
  - `upgrading` from version 1.
  - The community, folder, reconnect, typing and `find` tests, with presence's watch answers in.
  - The full gate, and `make check-audit`.
- **Trailers:** `Data-Audit: docs/audits/2026-10-31-communities.md` goes on the ten commits that touch the data layer, as `scripts/check-data-audit.py` lists them: `3b15dc39`, `d5a5f20f`, `4123fb9d`, `a06b57a8`, `d0296509`, `c717f06a`, `dfb001f6`, `0d5ec652`, `d3d3d014` and `fa932866`, and any commit that adopts P8 to P10. Not `0cc35cfc`, the terminal and jaw-client.

## Measurements

The work was done in a scratch clone at `fa932866` with debug test builds (`CARGO_INCREMENTAL=0`), at a load of 43 to 72 on 20 cores from other agents' builds.

**The switches.**

- `switches.py` put each rule behind `crate::mutant(NAME)`, chosen by `JAW_MUT`, with each replacement matching exactly once.
- One build served them all. The binary was run from `crates/jawd` with the sources restored to `fa932866`, so the plan test read the branch's own statements.
- M9 edited the source on disk and ran the head's binary.
- Each switch ran against its rule's tests:
  - the community, folder, reconnect and typing tests, 34 of them;
  - `find`'s 22;
  - kept's test;
  - the two plan tests.

| | Rule switched off | Result | Caught by |
| --- | --- | --- | --- |
| I1 | `since` dropped for chat lists | killed | the four `watched_again_after_…`, reconnect, typing |
| | the typing rule (a watch whose rows all stay out says nobody types) | killed | typing |
| | `since` dropped for a chat's messages too | killed | reconnect, typing's quiet chat |
| M1 | `communities_follow` stamps nothing | killed | counts, folders' re-watch |
| M2 | the community a group leaves isn't touched | killed | newest group leaves, counts, folders' re-watch |
| M3 | `total` without `brought` | killed | `one_row_wherever…` |
| M4 | a community's own place left out | killed | T3's two, O1's |
| M5 to M7 | closed in Unread, a folder, a smart folder, every group summed | killed | `hasnt_open…` |
| N1, N2 | All's and Communities' closed reads without their archived filter | killed | `hasnt_open…` |
| M8 | held, a group under its community also its own row | killed | the held test |
| | the held read ignoring `open` | killed | the held test |
| M9 | a member read `NOT INDEXED` | killed | the plan test (`SCAN c`) |
| | `{MEMBER}` unknown to `is_statement` | killed | the plan test |
| O1 | the groups' newest in `community_row` | killed | O1's |
| N15 | a community's own `last` kept over its groups' | killed | 12 community tests |
| O2 | closed picks by place | killed | O2's |
| O3 | the community map emptied; `find`'s tie, the recent list by its own `last_at` | killed (3) | the O3 test |
| N11 | a window around a group not around its community | killed | `one_row_of_a_window…` |
| N12 | `children_mentions` from unread | killed | 12 community tests |
| N5, N6, N7 | `total` counting a community a folder, Unread, a smart folder holds itself | **survived** | (T5); P8 |
| N9 | a community's own place counted where the list doesn't hold it | **survived** | (T6); P9 |
| N3, N4 | a tie in time broken the other way, open or closed | **survived** | (O4); P10 |
| N10 | a held window keeping a chat the list left | **survived** | (main's) |
| K1 | migration 25 changing `media.wanted` | killed | kept |
| K2 | the same, `wanted` not jaw's own (main's `CLASSES`) | passes | |
| K3 | migration 25 changing places' `media_files` row | killed | kept |
| K4 | `media_made` seeded by a plain `INSERT` | killed | kept, at seeding |
| K5 | a seed row twice | killed | kept, at seeding |
| K6 | the same, every row upserted (main's `seed()`) | passes | |
| K7 | migration 20 resetting `downloading` to `gone` | killed | kept |
| K8 | the same, media seeded `expired` (main's) | passes | |
| K9 | migration 20 resetting `expired` to `remote` | passes | (one row; moot for 20) |
| K9M | the same, media seeded `expired` | killed | kept |
| K10 | migration 25 resetting `downloading` to `remote` | killed | kept |
| K10M | the same, media seeded `expired` | passes | |

**The survivors, more widely:**

- N3 to N7, N9 and N10 also ran against all 339 tests of `feed::` and `store::` but the schema and plan tests, 7 to 10 minutes each. With no switch, that set fails only the long-request lane, on the landers' list.
- N7 and N9 failed nothing else.
- N6 also failed the live writes beside inbox rows, and N10 the WAL bound, both on the list.
- N5 also failed the live writes, and two timing tests that aren't on the list: `no_window_starves…` and `the_checkpointer_flushes…`. Those two passed alone twice with N5 and twice without.
- N3 failed nothing else. N4 also failed `a_window_whose_read_fails_is_read_again_at_its_pace_then_resynced` ("no entry found for key"), which passed alone twice with N4 and twice without.

**The probes:** built into the head beside the switches. P8, P9 and P10 pass with no switch on. P8 fails under each of N5, N6 and N7; P9 under N9 (Unread `["Ana", "Regata"]`); P10 under N3 and N4.

**The trial landing:**

- The library's head `77d44257` was merged into `fa932866` with rerere off, the conflicts resolved as "Landing" says, and this branch's migration made 26 (lock line, comment, two docs).
- It builds with no warning. `JAW_PLANS=write` wrote `plans.txt` with 701 entries: exactly the library's 689 with this branch's 40 added or changed and 28 removed or changed against `e12d8d34` (`plancheck.py`), with none extra, none missing and none planned otherwise.
- On it, these passed:
  - 106 tests in 170 s: the schema tests but the fault test, the plan tests, kept's test through 26, both migration tests at 26, the library's feed and store tests, the community, folder, reconnect and typing tests, and `find`'s;
  - `upgrading`, from a version-1 `jaw.db` through 25 and 26, in 11.6 s;
  - the fault at every statement of every migration, the library's data step before 26 among them, in 1,408 s at a load of about 60.

**At the head:** the schema and plan tests (18, without the fault test) and the community, folder, reconnect, typing, `find` and folder-store tests (71) pass.

## The probes

P8 (T5), P9 (T6) and P10 (O4), in that order, as `probe_t5_…`, `probe_t6_…` and `probe_o4_…`. They go in `crates/jawd/src/feed/tests/communities.rs`, beside its helpers, renamed as its tests are named. Each compiled and ran as printed.

```rust
/// Probe for the audit's T5: a community a list holds for itself and for a group of it is one row,
/// counted once in the list's total, in Unread, a folder of yours and a smart folder.
#[tokio::test(flavor = "multi_thread")]
async fn probe_t5_a_community_a_list_holds_for_itself_and_a_group_counts_once_in_total() {
    let bed = Bed::new(brisk());
    let family = bairro(&bed).await;
    bed.apply(vec![Change::Chat(Chat { id: BAIRRO.into(), marked_unread: Some(true), ..Chat::default() })]).await;
    file(&bed, &family, Item::Chat(BAIRRO.into())).await;
    let whole = ruled(&bed, "Bairro whole", &format!(r#"{{"community":"{BAIRRO}"}}"#)).await;
    let mut watcher = bed.watcher(64);
    let lists: [(&str, &[&str]); 3] = [
        ("unread", &["Ana", "Bairro 3/1", "  Avisos", "  Festa", "Regata"]),
        (&family, &["Ana", "Bairro 2/1", "  Festa"]),
        (&whole, &["Bairro 3/1", "  Avisos", "  Festa", "  Xadrez"]),
    ];
    for (folder, want) in lists {
        let held = Held::top(&mut watcher, folder).await;
        assert_eq!(shape(&held.model), want, "{folder}");
        assert_eq!(held.model.total, want.len() as u64, "{folder}: Bairro, held for itself and its groups, counts once");
    }
}

/// Probe for the audit's T6: a community a list doesn't hold sits where the newest of the groups
/// of it the list holds does, its own newer message left out, and brings them.
#[tokio::test(flavor = "multi_thread")]
async fn probe_t6_a_community_a_list_doesnt_hold_sits_at_its_groups_not_its_own_newer_message() {
    let bed = Bed::new(brisk());
    let family = bairro(&bed).await;
    bed.apply(vec![said(BAIRRO, "k1", AT + 50)]).await;
    bed.apply(vec![read_up_to(BAIRRO, "k1")]).await;
    let mut watcher = bed.watcher(64);
    let mentions = smart(&mut watcher, "Mentions me").await;
    let lists: [(&str, &[&str]); 5] = [
        ("all", &["Bairro 3/1", "  Avisos", "  Festa", "  Xadrez", "Ana", "Regata", "Rui"]),
        ("unread", &["Ana", "Bairro 3/1", "  Avisos", "  Festa", "Regata"]),
        ("groups", &["Bairro 3/1", "  Avisos", "  Festa", "  Xadrez", "Regata"]),
        (&family, &["Ana", "Bairro 2/1", "  Festa"]),
        (&mentions, &["Bairro 2/1", "  Festa"]),
    ];
    for (folder, want) in lists {
        let held = Held::top(&mut watcher, folder).await;
        assert_eq!(shape(&held.model), want, "{folder}: Bairro, and the groups of it there");
        summed(&held.model, folder);
    }
}

/// Probe for the audit's O4: two groups whose newest messages share a millisecond show the same
/// one as their community's last, open or closed.
#[tokio::test(flavor = "multi_thread")]
async fn probe_o4_a_tie_in_time_between_groups_breaks_alike_open_and_closed() {
    let bed = Bed::new(brisk());
    bed.apply(vec![
        person(RUI, "Rui"),
        community(BAIRRO, "Bairro"),
        group(FESTA, "Festa", BAIRRO),
        group(XADREZ, "Xadrez", BAIRRO),
        said(FESTA, "f1", AT + 10),
        said(XADREZ, "x1", AT + 10),
    ])
    .await;
    let mut watcher = bed.watcher(64);
    let open = Held::top(&mut watcher, "all").await;
    let closed = Held::watch(&mut watcher, json!({"folder": "all", "around": "top", "after": 50, "open": []})).await;
    assert_eq!(closed.row(BAIRRO)["last"], open.row(BAIRRO)["last"], "closed as open, in a tie");
}
```

# Round one: `7e047803..19e3851f` (kept as written)

Range: `7e047803..19e3851f` in `communities` (`~/src/jaw-communities`), 31 October 2026. Auditor: database-expert.

The round's first auditor stopped at about 02:16 with no note written; its scratch held a clone and the start of a build, both now deleted. This note is the round's whole audit.

## In short

- **Verdict: changes needed**, for one inconsistency and four test gaps. Nothing is lost.
- **The branch adds no write path.** What it changes in the data layer is how chat lists are read, one schema step (migration 21), and the `kept` test. jaw.db stays consistent through every change I tried, a kill at any of the test's nine commits included, and the migration is right, atomic and cheap (31.4 ms beside a million messages).
- **One inconsistency (I1):** a community's row in a list now depends on the list, and four things change what a list holds of its groups without writing a chat row: a folder moved under another, a smart folder's rule changed, a folder made a favourite, and a group's mute running out. Its `v` stays, so a re-watch from `since` keeps the row stale. This is the folders audit's I1 again, for a new reason; no interface takes that path today.
- **Test gaps:** the light reads of a closed community's groups aren't pinned by the plan test (T1); the closed path is tested only where it can't differ from a wrong read (T2); nothing tests a community at its own place (T3) or the held window's rule for groups (T4). Six of my fourteen mutants survive, each one of these.
- **Other:** two edge cases where a community's `last` isn't the newest of the groups under it (O1, O2), `find`'s ranking of communities (O3), the window's cost of reading and skipping groups (U1), and history re-linking a group, which is main's (X1).
- **On today's main** (`3878dd66`) the trial rebase is clean, and the tests pass but for five timing tests under load, each passing alone. For landing, `kept.rs` meets two other versions of its media seeds, and git merges their seed rows without a marker (L1).

## What the branch is

- `e7928e53`, the protocol: a community is one row wherever its groups would show, `children_mentions`, `open` on `chats.watch` and `watch.move`, `total` counting a community held only for its groups, and four fixtures. It touches `feed/window.rs` (one field), so it's in the data layer.
- `5e8c5fde`, jawd: `store/lists/communities.rs` (the window's rows), `open`, `total` (`lists::brought`), migration 21, `kept.rs` for media's version 20, the demo account's two communities, and the tests.
- `19e3851f`, the terminal and jaw-client: outside the data layer (no path `check-data-audit.py` lists).

## Findings

### Data loss

None.

### Inconsistency

- **I1. A community's row in a list changes with no new version when the list's hold on its groups changes without a chat row being written, so a re-watch from `since` keeps it stale.**
  - **Where:**
    - A community's row sums and sits by the groups of it that the list holds, worked out as the window is read (`store/lists/communities.rs:138`, `feed/window.rs:251`). The only thing that gives it a new version is a write to one of its groups or to itself (`communities_follow`, `store/write.rs:861`, and `upsert_chat`'s touch of the community a group leaves, `:1330`).
    - A re-watch with `since` and `have` leaves out every held row whose `v` is at most `since` (`feed/client.rs:630`), and a watch without its own `since` takes hello's (`:1270`).
    - Four things change which groups a list holds with no chat row written:
      - moving a folder of yours (`store/write/folders.rs:371`), which stamps only folder rows;
      - changing a smart folder's rule (`:547`), which stamps only the folder;
      - making a folder a favourite (`:563`), which stamps the folder and not its chats, so a smart folder asking for `favourites` gains them;
      - time: a group's mute running out in a smart folder that asks for `muted`.
  - **Reproduced** (probes in the scratch clone, each watching the list, changing it, then watching again with the seq and the ids it held, as the branch's own `come_back` does):
    - P1: Family holds Ana and Festa; More, holding Avisos, moves under Family. Family's Bairro is held at "2/1" and is "3/1" fresh, its `v` 33 in both.
    - P2: Mentions me's rule becomes `{"unread":true}`. Bairro is held at "2/1" and is "3/1" fresh.
    - P3: a smart folder of muted chats holds Bairro for Festa (muted for 3 s) and Avisos (muted for an hour). Once Festa's mute ends, Bairro is held at "3/1" at Festa's place and is "1/0" at Avisos's fresh, its `v` 32 in both.
    - P7: Family is a favourite and a smart folder asks for favourites; More, holding Avisos, becomes one too. Bairro is held at "2/1" and is "3/1" fresh.
    - Each fails the same way on the branch's head and on the trial rebase.
  - **Why it's new:** on main a chat row's content never depended on the list it's in, so a list gaining or losing chats only added or removed rows, which `have` handles. Here a community's `children_unread`, `children_mentions`, `last` and `sort` are per list.
  - **Its reach:** as with the folders audit's I1, no interface takes this path today. jaw-client watches chat lists again whole (`since: None`, `crates/jaw-client/src/client.rs:742`), and so does the Mac, and neither's hello sends `since`. The store is right and so are live pushes, which compare rows by hash. It breaks the protocol's promise for reconnecting ("rows in `have` that haven't changed since `since` are left out") and the branch's own claim that a window watched again from `since` is exact, which its tests check for every change they make except these.
  - **Fix, either:**
    - (a) Have jawd ignore `since` for chat lists, as `settle` already ignores it for the folder tree (`feed/client.rs:650`), and say in the protocol's Reconnecting section that chat lists, like the tree, are watched again whole. Both interfaces already do this, since a chat list held while catching up already sends rows with an old sort and the same `v`. This covers time (P3), which stamping can't without a timer.
    - (b) Or stamp the communities whose sums change: a folder move stamps the communities of the groups under the moved folder; a rule change, and a folder made a favourite, stamp every community the list held or holds (tens). Then document P3's case as the protocol already documents a smart folder's waiting.
    - Either way, adopt P1, P2 and P7 (printed under "The probes") as tests, and P3 too under (a).

### Tests (missing)

- **T1. The light reads of a closed community's groups aren't planned or pinned.**
  - `community_members`' six statements (`store/lists.rs:533`; All and Archived share one) all begin `{MEMBER}`, and `is_statement` (`store/plans.rs:278`) knows only `{CHAT_SELECT}`, `{PERSON_SELECT}` and a few others, so the plan test skips them. `MEMBER`'s own text is skipped as a fragment.
  - M9 confirms it: a member read written to scan `chats` passes the plan test.
  - **Fix:** add `"{MEMBER}"` to `is_statement` and write `plans.txt` again. I did, on the trial: all six search `chats_by_community`, a folder's also `folder_chats_by_chat`, and the smart folder's rule's subqueries search by key, with no scan or temporary b-tree. So the plans are right; they're only unpinned.
- **T2. A closed community's sums are tested only where they can't differ from a wrong read.**
  - The closed path (`community_members`, `store/lists.rs:533`) is checked only by `hasnt_open…`, closed against open, in the five built-in folders.
  - Its Unread data can't tell a read that sums every group from one that sums the list's: the one read group, Xadrez at +8, is older than the unread ones, so it neither moves the place nor changes the sums. M5 survives.
  - Folders of yours and smart folders aren't checked at all. M6 and M7 survive.
  - **Fix:** run `hasnt_open…`'s comparison in a folder of yours and a smart folder too, and give Unread a read group newer than its unread ones.
- **T3. Nothing tests a community sitting at its own place.**
  - A community's place is the newest of itself and the groups the list holds (`store/lists/communities.rs:158`). M4 leaves "itself" out and no test fails.
  - Without it, a community a list holds for itself alone, as P4's Bairro in Unread, has no place and leaves the list, though `total` counts it; and a pinned community sits at its newest group's place instead of among the pinned.
  - **Fix:** test a community held for itself alone (P4's set-up) and one pinned above its groups.
- **T4. The held window's rule that a group under its community comes once, and not at all closed, is untested.**
  - While catching up, a chat list reads only the rows it holds (`communities::among`, `store/lists/communities.rs:266`), skipping a group whose community is among them.
  - M8 drops the skip and `while_catching_up…`, which checks only Bairro's counts, still passes. Each group of an open community would then come twice in one read, and a closed community's groups would come as rows while it's held.
  - **Fix:** in that test, check that a closed community's groups stay out of the window and an open one's come once.

### Unpredictable performance

- **U1. A chat list's window reads in full, and drops, every group above its last row whose community sits elsewhere; and an open community's groups are read in full twice.**
  - **Where:** `communities::below` and `above` (`store/lists/communities.rs:162`, `:206`) read the list in pages of whole chat rows, each with `CHAT_SELECT`'s subqueries, and drop each group whose community is given elsewhere. `gather` then reads an open community's groups again (`community_groups`), and a closed one's through the light read.
  - **What it costs** grows with the groups of the communities near the top of a list, not with the window, the messages or the account's age. It's bounded by 1,000 groups a community (`GROUPS_MOST`) and by the communities jaw has.
  - **Measured** (under "Measurements"): with 20 communities of 25 groups the newest of 3,000 chats, All's window of 60 reads in 3.6 ms p50 and 9.3 ms p99 with every community closed, and 8.4 ms p50 and 19.3 ms p99 holding 560 rows with every one open, as an interface sending no `open` gets it, the Mac today. Main reads the same account's window, 78 rows holding two or three communities, in 1.3 and 3.0 ms.
  - **The budget:** a window's read is what a live change costs before it reaches a chat list, which the builder holds to the 5 ms p99 of a message on its way to interfaces. Opening a folder has 30 ms. At this size, closed is over the 5 ms at p99 (9.3 ms) and open at p50 too (8.4 ms), where main met it (3.0 ms p99). At the demo account's size (two communities of 3 and 2 groups) the builder measured All's top 100 within it, at 3.9 ms p99 at most. A conversation's own window goes first, so a message reaches it on time either way.
  - **So it stands as a follow-up,** since accounts this large in communities are rare. The light pages are worth taking in round two: they're the part of the cost main didn't have.
  - **Follow-ups:**
    - read the list's pages lightly (`n`, `sort`, `kind`, `community`) and whole rows only for what the window shows, so a skipped group costs an index entry;
    - read an open community's groups once per read;
    - the Mac's parity step 3.9, which sends `open` as the terminal does.

### Other

- **O1. A community a list holds for itself alone shows, as its `last`, the newest message of every group of it, ones the list doesn't hold too.**
  - When no group under it has a message, `chat_rows` keeps the row's own `last` (`feed/window.rs:263`), which `CHAT_JOINS` reads as the newest of all its groups' (`store/read.rs:288`), for `find`.
  - P4: Bairro marked unread with every group read is in Unread alone (0/0), and its `last` is Biblioteca's "b1", from an archived group Unread doesn't hold. The protocol says "its own, with none under it".
  - **Fix:** with no group under it, show the community's own `last_message`.
- **O2. A closed community's `last` can differ from the open one's.**
  - Closed, it takes the `last` of the group with the newest `last_at` (`store/lists/communities.rs:150`); open, the newest of its groups' `last` by `at`. Deleting a group's newest message for you keeps that group's place and shows the message before it, so the two differ.
  - P5: Festa's `f2` (at +30) deleted for you; Xadrez's `x1` is at +20, Festa's `f1` at +10. Open shows `x1`, closed `f1`, both at Festa's place. The protocol says "the newest of their messages", which is `x1`.
  - **Fix:** have the light read take each group's last message's `at` and pick by it.
- **O3. `find` and the palette's recent chats rank a community by its own messages now.** `find`'s index reads each chat's `last_at` for ties and for the recent list (`find/index.rs:600`, `:506`), and without the trigger a community's is its own, mostly 0, so a community ranks below chats it used to rank above. It's a ranking, not data; if it should rank by its groups', the index can take their newest as it reads every chat at start.

### Outside this range

- **X1. History can re-link a group a live event unlinked.** History's conversations set a chat's `community` from `parent_group_id` (`convert/history.rs:295`), and `upsert_chat`'s merge is the last writer's (`store/write.rs:1284`). A blob the phone made before a group left its community, converted after the live unlink, puts it back until its group info comes again. It's main's, as are the other conversation flags history sets; this branch only makes a community more visible. A follow-up for sync: let history set `community` only on a group that has none.

## What I checked

### Durability

- The branch adds no acknowledgement, synced commit, file or PRAGMA, and leaves the VFS alone.
- Migration 21 is a schema step, so it runs in the one `BEGIN IMMEDIATE` transaction the pending schema steps share with `user_version`. `a_fault_at_any_statement_of_any_migration…` passes with it, so a fault at any of its statements leaves jaw.db as it was.

### Idempotence, order and kills

- **No new write path.** Communities are still written only by main's `upsert_chat`, an upsert whose `WHERE` writes nothing and records no event when nothing changes, and by `communities_follow`, which stamps a community with its groups' changes in their transaction.
  - A community learned again as it was changes nothing (`learned_late…` checks the feed's seq doesn't move).
  - A group learned before its community, after it, or naming a chat jaw knows only as a group ends as a run in the other order, its own row untouched (`learned_late…`).
  - Joins, leaves, archives, reads, a message, a mention and a merge each leave every list as a fresh watch gives it (`a_communitys_counts…`, re-watched in six lists).
- **Kills.** Nothing the branch shows is stored, so nothing can be left half done: a community's sums, `last` and place are worked out as each window is read. `killed_after_any…` copies jaw.db after each of nine commits (`VACUUM INTO`), finishes the run on each copy and compares every list and every chat row with a run never killed. It passes, and M1 and M2 fail the tests that hold the versions it relies on.
- **Out of order, outside this range:** history can re-link a group a live event unlinked (X1, main's).

### Derived values

- **The trigger was the one value kept by hand.** `chats_community_followed` raised a community's `last_at` to its groups' newest and never lowered it, which kept a community at the place of a group that had left (the folders audit's H12). With it gone, a community's `last_at` is its own messages' again, as every chat's is, and where it sits in a list is derived as the list is read.
- **Its sums are derived per read:** `children_unread`, `children_mentions` and `last` come from the groups of it the list holds (open: their rows; closed: the light read), never stored. The exceptions to the protocol's rule are O1 and O2.
- **`total`** is the folder's kept count of its chats plus `brought`, counted from the communities jaw has (`store/lists.rs:409`). Each of its five forms matches the list's own rule: All, Archived and Communities count a community not in the list with a group in it; Unread one not unread itself with an unread group; Groups every community with a group in it; a folder of yours one not filed with a group filed; a smart folder one its rule refuses with a group it takes. `one_row_wherever…` checks seven lists' totals and M3 fails it.
- **Folders' own counts** are untouched: migration 21 changes no column `chats_recounted` follows, and P6 shows `drift()` empty and `folder_chats.sort` following `chats.sort` across the migration, through `chats_resorted`.

### The migration

- **Nothing else relied on the trigger.** I read every reader of a community's `last_at` and `sort`:
  - chat lists, which now gather it;
  - `folder_chats.sort`, which `chats_resorted` copies, so it follows (P6);
  - `find`'s ranking, which breaks ties and orders the palette's recent chats by `last_at` (`find/index.rs:600`, `:506`), so a community there now ranks by its own messages rather than its groups' (O3);
  - `rules.rs`, a test helper only;
  - no index includes `last_at` but `chats_by_community`, whose rows are groups, not communities.
- **An older jaw.db's `last_at` comes out right.** The update gives each community the newest of its own messages, or 0, as `show_newest` does after a merge (`store/write.rs:1812`). It can't restore one thing the trigger overwrote: a community whose own newest message was deleted for you kept its place under jawd's rule, and the migration gives it the message before's. Communities rarely have messages of their own, and their groups place them anyway.
- **Versions.** Communities and every group naming one take new versions after the feed's last seq, each with its event, in the same numbering. The prune always keeps the newest event, so `max(seq)` is the feed's last.
- **Kind, class and lock.** It's a schema step, as declared: it drops a trigger, makes a partial index on `chats` (thousands of rows), and updates the rows of communities and their groups, reading `messages` only by index. It rebuilds nothing, so it needs neither a data step's backup nor `foreign_key_check`. It makes no table, so `kept.rs` needs no class. Its lock line, `21 schema ddc73e7f…`, is the SHA-256 of its text. No merged migration changed: the lock and `MIGRATIONS` are only appended to. `the_folders_migration_gives…` now runs migration 19 alone, which is right, since 21 gives versions of its own.
- **Its cost** is under "Measurements".

### `kept.rs`

- **The change is right.**
  - Media's seed row takes a 32-byte `sha256` and the state `downloading`. Migration 20's `CHECK`s accept them, with `wanted` set as the downloading state requires, and migration 20's reset still turns the state from version 19 into `remote`.
  - The seed adds values for media's columns of jaw's own from 20 on. `CLASSES` lists them, exactly as migration 20's comment declares them: `error`, `wanted`, `asked`, `arrived`, `retried_at`, `cancelled`, `direct_path`.
  - `media_files`, `media_made` and `media_drop` get rows, with a value of its own in every column.
  - `seed()` upserts only `media_made`, the one table its migration writes a row into.
  - M13 (migration 21 changing `wanted`) fails the test only because of the media columns; M14 shows the upsert is needed.
- **The other three versions:** places and events' L1 patch, chat actions' `7b5fc48e`, and `kept-media-seeds`' `4c72954c`, which is `7b5fc48e` made to land on main first.
  - None lists media's newer columns, so with any of them a migration that lost `wanted` or `direct_path` would pass.
  - L1 upserts every seed row (`ON CONFLICT DO UPDATE`), which would let two colliding seed rows pass unseen. It also seeds the state `expired`, which migration 20's reset keeps, so its branch to `remote` goes untested.
  - `4c72954c` and `7b5fc48e` keep `here` before 20 and `gone` from 20 (`SEEDS_SINCE`), and upsert only `media_made` (`MADE`), as this branch does.
- **Which the second to land keeps** is under "Landing".

### Plans

- **Pinned, and as they should be:**
  - `brought`'s five counts walk the partial index `chats_known_communities`, which holds only communities, and look up each one's groups on `chats_by_community`. `plans.rs` allows the walk, "tens at most".
  - `community_row` and `place`'s lookup search by the primary key and the `id` index.
  - The chat row's new `children_mentions` subquery searches `chats_by_community`.
- **Not pinned:** the six member reads (T1). Planned on the trial, all search `chats_by_community`.
- **No `OFFSET`, no temporary b-tree** in any new statement: `community_members` has no `ORDER BY`, and the open path's `community_groups` reads `chats_by_community` in its order.

### On today's main

- **Main is `3878dd66`**: read receipts after a merge, twelve commits past the branch's base. Places and events hasn't landed. Its worktree was mid-rebase onto `3878dd66` at 16:00 and back at its own head, `d5965757`, by 17:00, so its landing is under way or restarting.
- **The trial rebase onto `3878dd66` is clean**: all three commits apply with no conflict (`git merge-tree`, commit by commit), and migration 21 keeps its number. Main's twelve commits and the branch share four files, `write.rs`, `plans.rs`, `plans.txt` and the protocol's README, and each merges on its own; the plan test passes on the merged `plans.txt`.
- **Its tests**, on the trial tree, in the scratch clone, at a load of 58 to 65 on 20 cores:
  - jawd's unit suite: 809 passed and 5 failed. All five failed on time: a lone change waiting 88 ms, the long-request lane, `sock`'s full connection (`Elapsed`), media's start check, and the WAL bound's "a live write waited 1.44 s". None is in community code, and each passed alone twice.
  - jawd's integration tests, `upgrading`'s first run from a version-1 jaw.db among them: 7 of 7 passed.
  - jaw-proto's: 16 and 3 passed, the new fixtures among them.
  - The probes behave on the trial as on the branch's head: P1 to P3 and P7 (below) fail as I1 says, P4 and P5 print as O1 and O2 say, P6 passes.
- **With `kept-media-seeds` landed first** (`4c72954c`, put on `3878dd66` the same way), the jawd commit conflicts only in `kept.rs`, in `seed()`'s two hunks. Git also keeps both branches' seed rows for `media_files`, `media_made` and `media_drop` with no conflict marker. L1 under "Landing" says what to keep; that resolution passes `kept`'s test on the trial.

### Mutation

Each mutant is one exact replacement in the scratch clone at `19e3851f`, rebuilt (`CARGO_INCREMENTAL=0`) and run against its rule's tests (the community, folders and folder-store tests, or the migration and `kept` tests), then put back. A mutant of migration 21's text had its lock line rehashed, so stores still open and only the rule's own tests can catch it.

| | Rule switched off | Result | Caught by |
| --- | --- | --- | --- |
| M1 | `communities_follow` returns at once | killed | counts through changes; folders' re-watch from `since` |
| M2 | the community a group leaves isn't touched | killed | `newest_group_leaves…`, counts, folders' re-watch |
| M3 | `total` leaves out communities held for their groups | killed | `one_row_wherever…` |
| M4 | a community's own place left out of where it sits | **survived** | (T3) |
| M5 | closed in Unread, every group summed | **survived** | (T2) |
| M6 | closed in a folder of yours, every group summed | **survived** | (T2) |
| M7 | closed in a smart folder, every group summed | **survived** | (T2) |
| M8 | in a held window, a group under its community also its own row | **survived** | (T4) |
| M9 | a light member read that scans `chats` | **survived** | (T1) |
| M10 | migration 21 without its `last_at` update | killed | `the_communities_migration_sorts…` |
| M11 | migration 21 keeping the trigger | killed | the same |
| M12 | migration 21 without new versions | killed | `…gives_communities_and_their_groups_new_versions…` |
| M13 | migration 21 changing `media.wanted`, jaw's own | killed | `kept`, through this branch's media columns |
| M14 | `media_made` seeded by a plain `INSERT` | killed | `kept`, at seeding |

## Measurements

Release builds (`cargo test --release`, `CARGO_INCREMENTAL=0`) in the scratch clone, at a load of 53 to 68 on 20 cores from other agents' builds. The branch is measured on the trial rebase onto `3878dd66`, and main on `3878dd66` itself, so the two differ only by the branch.

**A chat list's window, the same made-up account on both.** The account has 20 communities of 25 groups each (one an announcement group) and one message in each group, so the groups are the newest 500 of 3,000 chats, written through the store as the builder's measurement writes them. Each figure is 200 `chats.watch` of 60 after `top` through the feed's client, each timed from request to result, with its `unwatch` untimed (`audit_measure_a_chat_list_with_communities`, a scratch test). On the branch, `open: []` closes every community and no `open` opens them all; main has neither, so its two runs are one case. Each cell is rows, then p50 and p99:

| List | main | branch, every community closed | branch, every community open |
| --- | --- | --- | --- |
| All | 78, 1.31 and 3.01 ms | 60, 3.64 and 9.26 ms | 560, 8.42 and 19.25 ms |
| Unread | 60, 0.57 and 1.12 ms | 60, 3.69 and 8.39 ms | 560, 8.11 and 11.40 ms |
| Groups | 60, 0.55 and 1.84 ms | 20, 3.64 and 7.94 ms | 520, 8.20 and 14.95 ms |
| Communities | 78, 1.30 and 7.27 ms | 20, 3.61 and 8.38 ms | 520, 8.11 and 15.06 ms |

Main's 60 rows hold two or three communities with their groups. The branch's hold all twenty as one row each, so its window shows more of the list for the same `after`, and reads every group of those twenty to do it (U1).

**The builder's measurement**, `measure_a_chat_list_with_communities`, on the trial: 50 watches and unwatches each, p50 and p99. These agree with the builder's 12.8 and 22.1 ms open and 3.5 to 4.1 and at most 8.6 ms closed.

| List | every one open | none open | one open |
| --- | --- | --- | --- |
| All | 11.37 and 17.68 ms | 4.07 and 7.59 ms | 4.23 and 6.03 ms |
| Unread | 9.47 and 14.19 ms | 3.79 and 5.68 ms | 4.43 and 5.90 ms |
| Groups | 9.06 and 16.83 ms | 3.59 and 7.40 ms | 3.82 and 9.39 ms |
| Communities | 8.97 and 13.96 ms | 3.64 and 6.36 ms | 3.91 and 11.54 ms |

**Migration 21**, `measure_the_communities_migration` with `JAW_MEASURE_MESSAGES=1000000`, on the trial: 31.4 ms beside a million messages, 3,000 chats and six communities of ten groups. That's in its `BEGIN IMMEDIATE`, with the fingerprint and commit, and `drift()` is empty after. The builder had 33.1 to 35.5 ms.

**The member reads' plans**, planned on the trial with `{MEMBER}` taught to the plan test (T1). Each searches `chats c USING INDEX chats_by_community (community=?)`. A folder of yours adds `SEARCH f EXISTS USING COVERING INDEX folder_chats_by_chat (chat=? AND folder=?)`. A smart folder's rule adds its subqueries, each searching by key: `messages` by its primary key, `favourites` by its chat and folder indexes, and `folder_chats_by_chat`.

**The tests' own times:** the fault-at-every-statement test took about 12 minutes on the head; the trial's jawd unit suite took 1,554 s.

## Landing

For the round that approves it; nothing here blocks.

- **L1. `kept.rs` meets two other versions of the same seeds.**
  - The versions: `kept-media-seeds` (`4c72954c`, chat actions' `7b5fc48e` made to land on main first, for every branch waiting with a migration) and places and events' L1 patch.
  - **Whichever lands second keeps:**
    - this branch's media columns of jaw's own in `CLASSES` (`error`, `wanted`, `asked`, `arrived`, `retried_at`, `cancelled`, `direct_path`), with its media seed row's values for them. No other version has them, migration 20 declares them, and M13 shows the test catches a migration changing one only with them.
    - one way of seeding media's state across migration 20. That's `SEEDS_SINCE` (`here` before 20, `gone` from 20) if `kept-media-seeds` is on main, or this branch's `downloading`; not L1's `expired`, which leaves the reset to `remote` untested.
    - one upsert named for `media_made` (`MADE`, or this branch's `MADE_WITH_THEIR_ROW`), not L1's upsert of every seed row.
    - one set of seed rows for `media_files`, `media_made` and `media_drop`.
  - **Git keeps both sets of seed rows with no conflict marker**, and marks only `seed()`'s two hunks. Under a plain `INSERT` the second `media_files` row fails the test at seeding; under L1's upsert of every row it would pass unseen.
  - **Tested:** `kept-media-seeds`' file with this branch's media columns and media row, and one set of seed rows, passes `kept`'s test on the trial.
- **Renumbering:** migration 21 takes the next free number when it lands: 21 if it's next, 25 after places and events' 21 to 24. Its lock line keeps its kind and hash (`schema ddc73e7f…`); only the number and its comment's "21" change, and its tests find it by its text.
- **Trailers:** `e7928e53` and `5e8c5fde` touch the data layer and take the approving note's `Data-Audit` trailer. `19e3851f` touches none.
- **As main moves, the lander re-checks:**
  - `kept`'s test at the new number, after any migration that lands first;
  - the two migration tests, which build jaw.db at the version before 21 by hand, against whatever columns the migrations before it then have;
  - the plan test, since `plans.txt` merges;
  - the community tests, if a change to `upsert_chat`, `communities_follow` or folders lands first.

## The probes

In `crates/jawd/src/feed/tests/communities.rs`, beside its helpers; each compiled and ran as printed but for its `eprintln!`s. P1, P2, P3 and P7 fail on the branch. P1, P2 and P7 pass once I1 is fixed either way; P3 passes only under fix (a), since a mute running out writes nothing. P4 prints what O1 says, and P5 fails as O2 says. To adopt P4, assert Bairro's `last` is null.

```rust
fn n_of(folder: &str) -> i64 {
    folder.split_once(':').unwrap().1.parse().unwrap()
}

fn wall_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

/// P1: a folder of yours moved under another changes which groups that one holds.
#[tokio::test(flavor = "multi_thread")]
async fn probe_p1_a_folder_moved_under_another() {
    let bed = Bed::new(brisk());
    let family = bairro(&bed).await;
    let more = folder(&bed, "More").await;
    file(&bed, &more, Item::Chat(AVISOS.into())).await;
    let mut watcher = bed.watcher(64);
    let mut held = Held::top(&mut watcher, &family).await;
    let (more, family_n) = (n_of(&more), n_of(&family));
    let moved = step(&bed, |done| Step::Move { folder: more, parent: Some(family_n), position: None, done }).await;
    assert_eq!(moved, Outcome::Done(None));
    held.come_back(&mut watcher, "P1: More moved under Family").await;
}

/// P2: a smart folder's rule changed changes which groups it holds.
#[tokio::test(flavor = "multi_thread")]
async fn probe_p2_a_smart_folders_rule_changed() {
    let bed = Bed::new(brisk());
    bairro(&bed).await;
    let mut watcher = bed.watcher(64);
    let mentions = smart(&mut watcher, "Mentions me").await;
    let mut held = Held::top(&mut watcher, &mentions).await;
    let n = n_of(&mentions);
    let ruled = step(&bed, |done| Step::Rule { folder: n, rule: r#"{"unread":true}"#.into(), done }).await;
    assert_eq!(ruled, Outcome::Done(None));
    held.come_back(&mut watcher, "P2: Mentions me asking for unread").await;
}

/// P3: a group's mute running out takes it out of a smart folder of muted chats with no write.
#[tokio::test(flavor = "multi_thread")]
async fn probe_p3_a_groups_mute_running_out() {
    let bed = Bed::new(brisk());
    bairro(&bed).await;
    let now = wall_ms();
    bed.apply(vec![
        Change::Chat(Chat { id: FESTA.into(), muted_until: Some(now + 3_000), ..Chat::default() }),
        Change::Chat(Chat { id: AVISOS.into(), muted_until: Some(now + 3_600_000), ..Chat::default() }),
    ])
    .await;
    let made = step(&bed, |done| Step::Create {
        name: "Muted".into(),
        parent: None,
        rule: Some(r#"{"muted":true}"#.into()),
        done,
    })
    .await;
    let muted = match made {
        Outcome::Done(Some(n)) => format!("smart:{n}"),
        other => panic!("{other:?}"),
    };
    let mut watcher = bed.watcher(64);
    let mut held = Held::top(&mut watcher, &muted).await;
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    held.come_back(&mut watcher, "P3: Festa's mute ran out").await;
}

/// P4: a community a list holds for itself alone, none of its groups.
#[tokio::test(flavor = "multi_thread")]
async fn probe_p4_a_communitys_last_with_none_of_its_groups_there() {
    let bed = Bed::new(brisk());
    bairro(&bed).await;
    bed.apply(vec![
        read_up_to(AVISOS, "a1"),
        read_up_to(FESTA, "f2"),
        Change::Chat(Chat { id: BAIRRO.into(), marked_unread: Some(true), ..Chat::default() }),
    ])
    .await;
    let mut watcher = bed.watcher(64);
    let unread = Held::top(&mut watcher, "unread").await;
    eprintln!("P4: {:?}; Bairro's last there: {}", shape(&unread.model), unread.row(BAIRRO)["last"]);
}

/// P5: a closed community's last against an open one's, once a group's newest message is deleted
/// for you.
#[tokio::test(flavor = "multi_thread")]
async fn probe_p5_open_and_closed_last_after_a_delete_for_me() {
    let bed = Bed::new(brisk());
    bed.apply(vec![
        person(RUI, "Rui"),
        community(BAIRRO, "Bairro"),
        group(FESTA, "Festa", BAIRRO),
        group(XADREZ, "Xadrez", BAIRRO),
        said(FESTA, "f1", AT + 10),
        said(XADREZ, "x1", AT + 20),
        said(FESTA, "f2", AT + 30),
    ])
    .await;
    bed.apply(vec![Change::DeleteMessage { chat: FESTA.into(), id: "f2".into() }]).await;
    let mut watcher = bed.watcher(64);
    let open = Held::top(&mut watcher, "all").await;
    let closed = Held::watch(&mut watcher, json!({"folder": "all", "around": "top", "after": 50, "open": []})).await;
    assert_eq!(open.row(BAIRRO)["last"], closed.row(BAIRRO)["last"], "P5: closed as open");
}

/// P7: a folder of yours made a favourite brings its chats into a smart folder asking for
/// favourites, and stamps the folder alone.
#[tokio::test(flavor = "multi_thread")]
async fn probe_p7_a_folder_made_a_favourite() {
    use crate::store::folders::Target;
    let bed = Bed::new(brisk());
    let family = bairro(&bed).await;
    let more = folder(&bed, "More").await;
    file(&bed, &more, Item::Chat(AVISOS.into())).await;
    let made = step(&bed, |done| Step::Create {
        name: "Favs".into(),
        parent: None,
        rule: Some(r#"{"favourites":true}"#.into()),
        done,
    })
    .await;
    let favs = match made {
        Outcome::Done(Some(n)) => format!("smart:{n}"),
        other => panic!("{other:?}"),
    };
    let family_n = n_of(&family);
    let slotted = step(&bed, |done| Step::Favourite { slot: 1, target: Some(Target::Folder(family_n)), done }).await;
    assert_eq!(slotted, Outcome::Done(None));
    let mut watcher = bed.watcher(64);
    let mut held = Held::top(&mut watcher, &favs).await;
    let more_n = n_of(&more);
    let slotted = step(&bed, |done| Step::Favourite { slot: 2, target: Some(Target::Folder(more_n)), done }).await;
    assert_eq!(slotted, Outcome::Done(None));
    held.come_back(&mut watcher, "P7: More made a favourite").await;
}
```

P6, in `store/schema.rs`'s tests, files Bairro and Festa in a folder at version 20 (`communities_before_they_were_gathered`, then `INSERT INTO folders (n, name, position, v) VALUES (30, 'Bairro', 9, 1); INSERT INTO folder_items (folder, chat) VALUES (30, 1), (30, 2);`), runs `COMMUNITIES_GATHERED`, and asserts `drift()` as before and each chat's `sort` as its folder keeps it. It passes; adopt it if you like.

## Verdict

Nothing is lost, and jaw.db stays consistent: the branch writes nothing new, the trigger it drops was the one value kept by hand, and migration 21 is right, atomic and cheap. The kill test, the migration's and `kept`'s tests hold, and each of their rules fails its test when switched off.

What blocks is I1. A re-watch from `since` keeps a community's row stale after a folder moves, a smart folder's rule changes, a folder becomes a favourite or a group's mute runs out. That's the inconsistency the folders audit's I1 named for the same promise, and no interface takes the path today. Beside it, four rules the change makes are untested (T1 to T4), and the approval needs each tested.

For round two:

- **I1:** have jawd watch chat lists again whole, ignoring `since` as it does for the tree, and say so in the protocol (both interfaces already do it); or stamp the communities a folder move, a rule change and a favourite change, and document the mute's case. Adopt P1, P2, P3 and P7 as tests.
- **T1:** add `{MEMBER}` to `is_statement` and pin the six member reads' plans.
- **T2:** compare closed with open in a folder of yours and a smart folder, and give Unread a read group newer than its unread ones, so M5 to M7 fail.
- **T3:** test a community a list holds for itself alone and one pinned above its groups, so M4 fails.
- **T4:** check, in the held window's test, that a closed community's groups stay out and an open one's come once, so M8 fails.
- **Follow-ups that may stand with an approval:** O1 and O2, each a line or two and worth doing now; O3; U1; X1, which is main's.

Verdict: changes needed

# Verdict of round two (`fa932866`)

Nothing is lost and nothing is inconsistent.

- **Round one's inconsistency is closed.** jawd watches chat lists again whole, which holds for every change the note named, time included, and its six tests fail without it.
- **T1 to T4 and O1 to O3 are closed,** each with tests that fail with its rule off.
- **The `kept.rs` reconciliation is right** in each of its three changes, and places and events' landing holds under it.

What blocks is two rules the branch makes that no test holds. The code is right in both.

- A chat list's `total` counts a community once where Unread, a folder of yours or a smart folder holds it for itself and for a group of it (T5).
- A community a list doesn't hold sits at the place of its groups there, not at its own newer message (T6). Without that, it and its groups leave the window.

For round three:

- **T5:** adopt P8, so N5, N6 and N7 fail.
- **T6:** adopt P9, so N9 fails.
- **Follow-ups that may stand with an approval:**
  - O4 (adopt P10 beside the other two; it costs nothing);
  - K9's second media seed row;
  - N10's catch-up test;
  - the protocol's Smart folders sentence naming a mute that runs out;
  - U1;
  - X1, which is main's.
- **Then** land as "Landing" says: after the library and presence, as migration 26.

Verdict: changes needed

# Verdict of round three (`60f83020`)

Nothing is lost and nothing is inconsistent, and every rule the branch touches is tested.

- **Round two's items are closed:** T5 and T6 by tests that fail with their rules switched off, and O4 too.
- **The rebase onto the library** kept both sides and nothing more.
- **The library's rules and this branch's** still fail their tests when switched off.
- **`kept.rs`** is right for main's seeds and this branch's.
- **I1 holds** beside the library's windows, which are always watched whole.
- **The full suite passes under the flakes rule,** but for two of media's timing tests that aren't on the list (O6). They pass alone, and the branch doesn't touch them.

**Follow-ups that stand with the approval:**

- K9's second media seed row;
- N10's catch-up test;
- the protocol's Smart folders sentence on a mute that runs out;
- U1;
- O5, at landing;
- O6, for the flakes list;
- X1, which is main's.

## Landing

- **The order:**
  - Presence is on main (`5dfefb1e`, no migration).
  - Contact cards lands next as migration 26, a schema step, and this branch then takes 27.
  - If another branch with a migration lands first, this one takes the next free number the same way.
- **Taking 27:** rebase onto main once contact cards is on it, with rerere off (`git -c rerere.enabled=false rebase`). The trial, this head onto contact cards' landing trial, met these conflicts, each resolved by keeping both sides:
  - **`migrations.lock`:** main's lines, then `27 schema ddc73e7fea956fe6fafea2d6ab0ed98fb33e404a5994f76e836c71dc24f6bc5c`, kind and hash as they are;
  - **`schema.rs`'s `MIGRATIONS`:** contact cards' text, then `COMMUNITIES_GATHERED` with its comment saying `// 27, a schema step`. Its constant's doc ("Migration 27's text") and its measurement's doc ("Migration 27 beside a million messages") say 27 too;
  - **the protocol README:**
    - the methods table: presence's `chats.watch` row and this branch's `watch.move` row;
    - the folder bullets: this branch's "A folder's chats" and "Communities in a chat list", then presence's "The roster";
  - **jaw-client's tests:** both;
  - **the parity plan:** contact cards' 3.9, then this branch's step as 3.10 (O5).
- **The messages** say it too:
  - the jawd commit's "Migration 27, a schema step, after places and events' 21 to 24, the library's 25 and contact cards' 26";
  - O3's "migration 27's trigger";
  - the terminal commit's "The parity plan's step 3.10".

  The migration's text is unchanged, so its hash holds and its tests find it by its text.
- **`kept.rs`** merges cleanly: neither presence nor contact cards changes it.
  - It stays main's file with this branch's three changes: media's seven columns of jaw's own in `CLASSES`; the `downloading` seed row with values for them; and plain inserts but `media_made`'s, with main's one set of rows for `media_files`, `media_made` and `media_drop`.
  - Contact cards' migration inserts a `backfills` row named `contacts`, which collides with no seed row. Kept's test passed through 27 on the trial.
- **`plans.txt`:** write it with `JAW_PLANS=write` at each commit that changes it. It must hold exactly main's entries with this branch's 40 added or changed and 28 removed or changed: 708 on the trial, as merged.
- **Trailers:**
  - `Data-Audit: docs/audits/2026-10-31-communities.md` goes on the 13 commits that touch the data layer, as `scripts/check-data-audit.py` names them. They are each of these as rebased: `25604c31` (the protocol), `505a340e` (jawd), `82f19405` (I1), `4c75fe6a` (T1), `3fe094c8` (T2), `31dde8a0` (T3), `c0d0bacd` (T4), `6d4ae95f` (O1), `e8364a8c` (O2), `90ac1637` (O3), `194e1291` (T5), `32a6d0f7` (T6) and `60f83020` (O4).
  - It doesn't go on `3f05d1ca`, the terminal and jaw-client.
  - The note goes in, committed in the branch.
- **The lander re-checks,** after the rebase:
  - `git range-diff 77d44257..60f83020 origin/main..HEAD` shows only the renumbering, the conflicts' both sides and the trailers;
  - kept's test at 27;
  - the two migration tests, which build `jaw.db` at the version before it;
  - the fault at every statement, now with the library's data step 25 and contact cards' 26 before 27;
  - `upgrading` from version 1;
  - the community, folder, reconnect, typing, `find`, library and contact cards' backfill tests, with presence's and contact cards' code in. Contact cards' backfill stamps a chat through `stamps`, so the chat's community follows in the same transaction;
  - the full gate under the flakes rule, and `make check-audit`.
- **On the trial these all passed:** kept's test, the migration tests, the fault test, `upgrading` and the tests named above. 942 of the 949 unit tests run passed, and the other seven are timing tests that pass alone 3 of 3. `plans.txt` written again changed nothing. The details are under "Measurements".

Verdict: approved
