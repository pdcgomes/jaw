# Audit: chat actions from jaw

# Round six: `2f6ad6f4`, round five's tests

Range: `77d44257..2f6ad6f4` in `chat-actions` (87 commits on main's `77d44257`, the library), 2026-11-02. Auditor: database-expert. Main is `ea0f4134` now: media out has landed, as migration 27. Communities (`0dd286c9`, on `ea0f4134`) lands as 28. Disappearing (`d9acfb02`, still on `77d44257`, with a 26 of its own) may land before this branch.

Rounds five to one are kept as written below this round. This round's verdict comes at the end of the note, after theirs.

## In short

- **Verdict: approved.** T7 is closed: each of its two rules has a test made from round five's probes, and each test fails when its rule is switched off. Nothing but those tests changed. The full suite passes but for two timing tests on the landers' list, each passing alone 3 of 3.
- **What changed:** one commit, `2f6ad6f4` on `d42da1c1`, which adds 63 lines to the tests of `store/write/chat_actions.rs` and changes nothing else.
- **T7, closed:**
  - a refusal looks for a later wait under its own setting only: a refused pin beside another chat's later wait (P1) and a pinned chat's refused archive while its unpin waits (P2) each fail with `r5-later-wait-any-key`, and nothing else does;
  - a later patch's wait for an index starts with no `echo_by`: P3 fails with `r5-await-keeps-echo-by`, at its first check, and nothing else does.
- **Landing:** the next free number, 29 after communities' 28, or 30 if disappearing lands first; the lock line keeps its hash; the 65 commits `check-data-audit.py` names take the trailer. What the lander resolves and re-checks is under "Landing", below, and in the verdict at the end of the note.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gaps

None. T7 is closed (below).

### Unpredictable performance (follow-up; breaks no budget of its own)

- **P4 stands,** unchanged since round five: I4's `settled` adds about 0.5 ms to each merge's start. The follow-up for main is round four's: bound the merges a live transaction begins, or begin them in the background lane.

### Other

- **O8, the landers' list.** The full run failed two tests under load, the feed's lane test and the socket's connection-full test. Both are on the list, the branch changes neither the feed nor `sock.rs`, and each passed alone 3 of 3.
  - All the watched tests passed: the terminal's 41 `wire` tests, the logout test (round five's O7) and media's disk test.
  - So did four others from the list: the half-closed connection, the watch result waiting for room, media's save-waiting test and sync's engine-disconnect test.
- **O5's two guards** stay as safeguards, and O3's caveats stand.

### Hygiene (other; follow-ups)

- H2, H6 and H9 stand, as do round one's H1, H3 and H5, and P1 to P3.

## What was asked, checked

### T7's tests are the probes' rules

- **P1 and P3:** with names, comments and whitespace stripped, `a_refused_pin_beside_another_chats_later_wait_is_put_back` (`store/write/chat_actions.rs:2495`) and `a_later_patch_for_an_index_waits_for_its_own_echo_past_where_the_earlier_ones_had_to_be` (`:1598`) are round five's probes token for token.
- **P2:** `a_refused_archive_is_put_back_while_its_unpin_waits` (`:2513`) differs from its probe only in the archive patch's value, `archive_value(true, AT + 1_000)` for a bare time; with that value put back, it is the probe token for token. The change keeps the probe's meaning:
  - the writer keeps a patch's value only as its outbox row's payload (`app_state_row`, `store/write/outbox.rs:556`, from `queue`, `:299`) and as its provenance for the quarantine (`provenance`, `:198`);
  - it never decodes it: the writer's one decode (`newest`, `:472`) reads WhatsApp's values held in `app_state`, never jaw's patch;
  - so the value changes nothing the test checks, and P2 still builds the archive as the handler does, the archive and its unpin carrying one time (`send/chat_actions.rs:209-212`).

### Each fails under its switch

Round five's 37 switches (`chat-actions-audit-r5-switches.py`, its only change `HERE`), applied in the audit's clone of `2f6ad6f4` (each edit found once), built once and run with `WITH_SEND=1`:
- **switches off:** all 231 tests pass;
- **`r5-later-wait-any-key`:** 2 of 229 fail, P1 and P2, and nothing else. Alone, P1 fails at "Ana's chat unpinned again, Rui's mute still waiting" (still pinned at `AT`), and P2 at "the archive put back, the unpin standing" (still archived);
- **`r5-await-keeps-echo-by`:** 1 of 229 fails, P3, and nothing else. Alone, it fails at its first check, "the unpin's wait, not yet taken": the wait carries `echo_by` 7.

These are the fix round's counts and failing checks.

### Nothing else changed

`git diff d42da1c1 2f6ad6f4` is two hunks inside the writer's tests module (line 666 to the file's end): 63 lines added, none removed. No statement, plan, migration, lock line, `kept.rs` seed or commit boundary changed since round five, so round five's checks of durability, idempotence, a kill at every commit, the schema and the plans stand as written. `rustfmt --check` passes on the file.

### The full suite

At the head, in the audit's clone: the engine's `go test -race ./...`, then `cargo test --workspace --no-fail-fast`, so that every target runs. Only jawd's unit tests failed: two timing tests on the landers' list (O8). Every other target passed. The numbers are under Measurements.

### Landing

Trial merges of `2f6ad6f4` (`git merge-tree --write-tree` in the audit's clone, no working tree touched):
- **with main (`ea0f4134`), 13 files conflict:** `jaw-proto`'s `send.rs` and `tests/fixtures.rs`, `engine/demo.rs`, `send/mod.rs`, `migrations.lock`, `plans.txt`, `schema.rs`, `schema/kept.rs`, `store/write.rs`, `store/write/outbox.rs`, the engine's `send_test.go`, `protocol/README.md` and `scripts/check-data-audit.py`;
- **with communities (`0dd286c9`):** the same 13;
- **with disappearing (`d9acfb02`):** 12.

What each resolution keeps, beyond both sides:
- **The number.** Main holds 27, media out's (`27 schema 0c7b0505…`); communities takes 28 (`28 schema ddc73e7f…`). So the branch takes 29, or 30 if disappearing lands before it.
  - `migrations.lock`'s last line is `<n> schema 18e69a4d683f05b66b98cc4fb66633823ff6ea8cd8660d172cf09b2dfe669d7b`. Renumbering changes only the comment outside the migration's text, so the hash stays.
  - `schema.rs`'s comment, the test's name (`migration_<n>_keeps_one_wait_an_index_for_one_outbox_row_each`), the plan's lines and the six commit messages that name 26 say the new number. Those commits are `d69362f6`, `a1e63535`, `4f796cae`, `a5e376ad`, `678462e8` and `d42da1c1`; `2f6ad6f4` names none.
- **`kept.rs`:** main's, which holds media out's `outgoing` and `outgoing_drop` now, plus communities' classes and seeds, disappearing's `opened` if it lands first, and `app_state_awaiting`'s class (jaw's own) and seed with `named`. Its test then seeds the version before this branch's migration, so it's run again.
- **The trailer:** `scripts/check-data-audit.py 77d44257 2f6ad6f4` names 65 commits, round five's 64 and `2f6ad6f4`. Each takes `Data-Audit: docs/audits/2026-10-29-chat-actions.md`, and the note is committed in the branch. Main's version of the script names the same 65.
- **`check-data-audit.py`:** keeps main's `crates/jawd/src/convert/` and `crates/jaw-proto/src/vcard.rs`. The first covers the branch's `crates/jawd/src/convert/app_state.rs`, whose line can go.
- **The outbox's writer, against media out's `typed` draft rule** (`store/write/outbox.rs`):
  - `Step` keeps the branch's `Refused` beside media out's `Attach` and `Unsent` (and disappearing's `Disappearing`, if it's first), and `apply` takes every arm;
  - `SHOWN_DELETED` takes main's macro form, `concat!("SELECT ", shown_deleted!(), " FROM messages t WHERE t.n = ?1")`, made `pub(super)` for `star`;
  - the plan test's rows take both sides' statements; `plans.txt` is written again with `JAW_PLANS=write`, then read, and only both sides' statements may change;
  - the branch's app-state rows go in through `app_state_row`, never `outgoing()`, so `typed` doesn't meet them. Media out's draft tests are still re-run, with the branch's outbox and refusal tests.
- **Presence:** three predicates make the same test (online, catching up or phone offline): main's `presence::reachable(state)` and `send::presence::connected(connection)`, and the branch's `sync::reachable(connection)`. Keep one, and point the branch's callers at it: `Sending::start` in `send/mod.rs`, `send/presence.rs` and `sync/refresh.rs`.
- **Communities' sort:** its migration drops `chats_community_followed` and sorts a community by its own messages, and the feed places a pinned group's community among the pinned. `PINNED_ELSEWHERE` (`sort >= '1~'`) reads each row's own `pinned`, so a community counts only if its own row is pinned. Re-run the pin tests: at most three pinned, archived ones not counting, a chat merging away counting once, and a community's own row refused.
- **Disappearing, if it lands first,** at four points:
  - `Step::Disappearing` beside `Refused`;
  - `send/outbox.rs`'s attempt takes disappearing's `may_have_gone` with the branch's `(command, patch)`, disappearing's arms giving `None` for the patch;
  - `store/write.rs`'s `touches_fts` takes both sides' arms (the branch's `Change::AppState`, disappearing's `Expire` and `Once`);
  - `kept.rs` takes both seeds.
- **What the lander re-checks:**
  - the gate, judging timing failures against the landers' list;
  - the migration's test, `kept`'s, the lock and number tests, the fingerprint test and both plan tests;
  - `tests/upgrading.rs` from version 1 through the new number, with `--engine demo`;
  - the outbox's tests, media out's draft and HD tests, the pin tests and the demo engine's;
  - the writer's tests with round five's switches;
  - the trailers, then `make check-audit`.

## Measurements

All in a clone of the worktree under `/tmp/jaw-audits/chat-actions-6`, built with `CARGO_INCREMENTAL=0`, on this Mac at a load average of 46 to 70 from other agents' work. Everything ran under `nice`, with `TMPDIR` and `JAW_HOME` in the audit's folder.

- **The engine's tests** (`make engine`, then `go test -race ./...`): ok, four packages (`cmd/jaw-engine`, `internal/link`, `internal/wa`, and `enginepb`, which has no tests).
- **The workspace's tests at the head** (`cargo test --workspace --no-fail-fast`, 09:00 to 09:47):
  - **jawd's unit tests:** 1,021 passed, 2 failed, 16 ignored, in 2,397 s.
    - The failures are the feed's `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` ("the lane and the watch waited for the long requests") and the socket's `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push` ("nothing went missing": its 120 s timeout ran out).
    - With the same binary, each passed alone 3 of 3.
  - **Every other target passed:**
    - jaw 334 (2 ignored), and its composing, failures, settings and typing files 3, 10, 3 and 1;
    - jaw-client 24, and its jawd file 3;
    - jaw-media 23, and its pipeline 18 (1 ignored);
    - jaw-proto 16, and its fixtures 3;
    - jaw-version 0, jaw-voice 36 (2 ignored) and wa-proto's roundtrip 3;
    - jawd's installed 1, starting 1, stopping 4 and upgrading 1;
    - the doc tests.
- **Switches:** the switched binary took 1 min 55 s to build. Each run took 3 to 4 minutes, with 8 test threads.
- **The trailer:** `python3 scripts/check-data-audit.py 77d44257 2f6ad6f4` names 65 commits without it, and main's script (from `ea0f4134`) names the same 65.

# Round five: `d42da1c1`, round four's fixes

Range: `77d44257..d42da1c1` in `chat-actions` (86 commits on main's `77d44257`, the library), 2026-11-02. Auditor: database-expert. Main is still `7b1ad56e`. Media out (`81c45fa1` now, on main) is landing as migration 27 and communities (`60f83020`) as 28; disappearing (`fa03773b`, on `77d44257`, with a 26 of its own) may land before this branch.

Rounds four, three, two and one are kept as written below this round. This round's verdict comes at the end of the note, after theirs.

## In short

- **Verdict: changes needed, for two tests (T7).** Round four's asks are done: I6 is fixed as it asked, and each of T6's three rules has its test. The code holds everywhere this round looked. But two rules of the statements I6 wrote or rewrote fail no test when they're switched off, and each would lose a request or leave a chat wrong if it broke. The audit's probes are the two kinds of test needed.
- **What changed:** `git range-diff 77d44257..678462e8 77d44257..d42da1c1` shows the 83 commits round four audited unchanged, and three new ones: I6 (`a5e376ad`), T6 (`7c81c1d7`) and the plan's item 17 (`d42da1c1`, docs only).
- **I6, closed.** `app_state_awaiting` is keyed by the setting and the index named, `PRIMARY KEY (key, named)`, and each wait keeps an outbox row of its own (`UNIQUE (outbox)`). Every statement that reads, moves or clears a wait follows the new key. What comes under any index one of jaw's waiting patches named is held back until that patch's own echo, and each of the setting's other waits counts at its time. The probes N1 (in both orders), N2 and N3 are tests now, and each of I6's seven switches fails its tests.
- **T6, closed.** The echo known by its index, O2's tie inside `newest`'s loop, and a merge looking under a third JID of the person each have a test that fails with round four's switch.
- **T7, a test gap, two parts:**
  - a refusal looks for a later wait under its own setting only (`LATER_WAIT`'s `key = ?1`, new in I6). Looking under every setting fails none of 226 tests. Broken, a refused pin wouldn't be put back while jaw's later mute of another chat waits, and where WhatsApp holds nothing for the pin the chat would stay pinned for good;
  - a later patch's wait, taking an earlier one's place under its index, starts with no `echo_by` (`AWAIT`, which I6 rewrote). Keeping the earlier one's fails none of 226, nor any other of jawd's unit tests. Broken, the converter passing where the earlier patch's echo had to be ends the later one's wait, and the phone's change WhatsApp took before jaw's later patch shows: a request judged against it is lost (probe).
- **The refusal's new condition holds:** a refused patch puts back what it set only while no later wait of jaw's for its setting stands, by time and then index, as `newest` weighs them. When it does put back, the fetch that follows sets what WhatsApp holds. Its window until then is round one's H2, the same as before I6.
- **The migration:** still a declared schema step, its text changed, its lock line `26 schema 18e69a4d…` that text's hash, `kept.rs`'s seed valid; four statements changed and three new, each pinned, none scanning or sorting.
- **The full suite at the head** passes but for two timing tests on the landers' list, each passing alone 3 of 3: jawd's unit tests 1,018 passed, 2 failed, 16 ignored; everything else passes, the engine's race tests and the upgrade from a version 1 jaw.db among them.
- **Landing,** once approved: the next free number, 29 after media out and communities, or 30 if disappearing lands first too, its lock line keeping the hash; the 64 commits `check-data-audit.py` names take the trailer. What the lander re-checks is under "Landing", below.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gaps

- **T7. Two rules of the statements I6 wrote or rewrote fail no test when switched off.** Each is right in the code at the head. Each is reachable, and would lose a request or leave a chat wrong if it broke.
  - **A refusal looks for a later wait under its own setting.**
    - **Where:** `LATER_WAIT` (`store/write/chat_actions.rs:68`), new in I6, which `released` (`:587`) runs for a refused patch.
    - **Switched off:** with it looking under every setting (`r5-later-wait-any-key`), none of the 226 tests fails: the fix round's 189, and the 37 chat-action tests of `send::tests` and `send::chat_actions::tests`. No refusal test refuses a patch while a later patch for another setting or another chat waits (the one beside a later wait has it for the same setting), and `Step::Refused` is the only way to `released`.
    - **What it would do:** a refused patch isn't put back while any later patch of jaw's waits for any chat. A fetch can't correct that where WhatsApp holds nothing for the setting: probes P1 and P2 (under Measurements) leave Ana's chat pinned after its pin's refusal, Rui's later mute waiting, and a pinned chat's refused archive standing while its unpin waits.
    - **Fix:** a test of each: a refusal while another chat's later patch waits, and the refused archive of a pinned chat while its unpin waits. Both are in `chat-actions-audit-r5-probes.rs`.
  - **A later patch's wait starts with no `echo_by`.**
    - **Where:** `AWAIT`'s `echo_by = NULL` (`store/write/chat_actions.rs:47`), whose conflict target and assignments I6 rewrote. It's round three's T5 for `queue` rather than `merging`.
    - **Switched off:** with the later wait keeping the earlier one's `echo_by` (`r5-await-keeps-echo-by`), none of the 226 fails, nor any other of jawd's unit tests: run over all of them but the fault-injection test with the switch on, 1,014 passed and five timing tests failed under load, each passing alone 3 of 3 with it still on. No test queues a second patch for an index after WhatsApp took the first and before the converter passed where its echo had to be.
    - **What it would do:** that order comes whenever the converter is behind a sent patch, as at an engine start's catch-up. Probe P3: jaw's pin is taken, its echo due by inbox row 7; jaw's unpin follows before the converter gets there; once it passes 7, the unpin's wait is gone, and the phone's pin, later by its time but taken by WhatsApp before jaw's unpin, shows: pinned at `AT + 5000`, no wait left. A pin asked then is `Already`, and the unpin's echo then unpins the chat: lost on every device, as in I3.
    - **Fix:** a test of it: P3, whose first check alone fails with the switch.

### Unpredictable performance (follow-up; breaks no budget of its own)

- **P4 stands,** unchanged by I6: `settled` makes the same lookups, two keys for each of the four settings, now reading every wait under each rather than one.

### Other

- **O5. Two guards no ordinary input reaches.** Neither needs a test; each stays as a safeguard.
  - **`MOVED`'s "of two naming one index, the later stays"** (`store/write/chat_actions.rs:62`). With the move always replacing (`r5-moved-always`), none of 226 fails. Before I6 it decided which of two waits for one setting stayed (`i6-move-one-wait`, round four's text, fails 5). Keyed by the index named, it needs a wait under the kept chat's key that already names the gone chat's index. `chat_jid` gives the gone chat's JID its own chat until the merge starts, and jaw's handler names a chat merging away by the chat it goes into, so none is ever made.
  - **The hold-back ending a stale wait under another index** (`held_back`'s `after`, `:378`). With it ended only under its own index (`r5-after-own-only`), none fails. The drain ends every wait past each row it converts, so a stale wait meets a mutation only when the row where its echo had to be wasn't drained (it didn't decode, or is a kind this jawd doesn't know). There, counting it at its time would lean to what WhatsApp holds.
- **O6, the landers' list.** In the full run, the feed's lane test and the socket's connection-full test failed under load; both are on the list, and each passed alone 3 of 3. None of the watched tests failed: the disk nearly full and the flood of photos (media), the terminal's offline-history test, and the terminal's `wire` test (which passed among jaw's 334).
- **O7, a first sighting.** `send::tests::linked_again_after_a_logout_that_cut_a_catch_up_short_jawd_is_online_and_sends_and_notifications_go`, main's code, failed once under load in a switched run, and passed alone 3 of 3 with the switch still on. It isn't on the landers' list: one to watch.
- O3's caveats stand.

### Hygiene (other; follow-ups)

- **H2, with I6.** When the latest of a setting's waits is refused and an earlier one, naming another index, still waits, the chat goes to how it is when nothing set it, while `newest` counts the earlier wait at its time. Until that wait's echo, or the fetch after the refusal, the chat can show the default where WhatsApp will end with the earlier patch, and a request for the default is answered `Already`. Before I6 the same case forgot the earlier wait and showed the default too, until the same echo. H2's fix, putting back the newest held rather than the default, closes this one as well.
- **H6, with I6.** After a restart with the clock gone back, jaw's next patch can carry a time earlier than a wait still on its way. With two waits for a setting, `newest` then counts the earlier patch as the newer one, as the owner's rule says, so the chat ends with it, as it did before I6. Starting `next`'s `last` from `max(at)` in `app_state_awaiting` closes this too.
- H9, and round one's H1, H3, H5 and P1 to P3, stand.

## What was asked, checked

### I6 and T6, each switched off

The fix round's 29 switches and eight of mine, all behind `JAW_MUTANT` in one build of the head in the audit's clone, never in the worktree. Each ran against the fix round's tests: the chat actions' writer tests, the rules', the converter's app state, migration 26's, the writer's own, the outbox's, the merges' and the synced-commit test, 191 in all, 189 without the merges' two slow tests, which no switch touches. Mine ran against those and the 37 chat-action tests of `send/` too, 226 in all. With no switch every one of them passes. Every one of the fix round's 29 fails exactly as many tests as the fix round counted. The table is under Measurements.

- **I6** (`a5e376ad`), closed:
  - `queue` replaces only a wait naming the same index (`i6-queue-one-wait` fails 2); `merging` moves every wait beside the kept chat's (`i6-move-one-wait`, 5);
  - the hold-back reads every wait of the setting (`i6-held-first-wait`, 10), and an echo ends only its own (`i6-waited-by-key`, 11);
  - `settled` counts every wait under both chats' keys (`i6-settled-first-wait`, 1); `marking` looks for any wait for the chat's mark (`i6-marking-named-kept`, 2); a refusal puts back only while no later wait stands (`i6-released-any`, 1).
  - Mine: an echo known by its index alone, at any time (`r5-echo-index-only`), fails 33; a refusal looking under every setting, and a later wait keeping the earlier one's `echo_by`, fail none (T7); the two guards fail none (O5).
- **T6** (`7c81c1d7`), closed: `r4-echo-time-only` fails the test of the phone's change at the very millisecond of jaw's pin; `r4-tie-loop-first` the three values at one millisecond in all six orders; `r4-settled-no-person` the newest held under a third JID.
- **Round three's fixes and round four's switches,** written again for the new key, fail as the fix round counted, 1 to 11 each.

### The new key everywhere

Every statement on `app_state_awaiting`, with what it goes by now:

| Statement | Where | By |
| --- | --- | --- |
| `AWAIT` (queue) | `store/write/chat_actions.rs:47` | `ON CONFLICT (key, named)`: replaces only a wait naming the same index, `echo_by` cleared |
| `AWAITED_AT` (queue's check of one time) | `:43` | the key and the index named |
| `AWAITED` (the hold-back, the merge's choice) | `:52` | the key: every wait of the setting |
| `WAITED` (an echo, a stale wait, a move) | `:54` | the key and the index named |
| `WAITING_UNDER`, `MOVED` (the merge's move) | `:58`, `:62` | the gone chat's keys; `ON CONFLICT (key, named)`, the later staying |
| `AWAITING` (the mark checks) | `:45` | the key: any wait for the chat's mark |
| `WAITED_FOR`, `LATER_WAIT` (a refusal) | `:66`, `:68` | the outbox row; then the setting's key, by `(at, named)` |
| `PASSED` (the converter past `echo_by`) | `:56` | `echo_by` |
| `ECHO_BY`, `RELEASED` (sent; failed) | `store/write/outbox.rs:241`, `:243` | the outbox row, one wait's alone |
| `SAID` (history's fill-in) | `store/write.rs:1550` | the key under each of the chat's JIDs: any wait |
| `jawd status`'s `awaiting` | `state.rs:267` | a count of rows, now one a setting and index named |

- Nothing in `send/` or the engine reads the table. App-state outbox rows leave only by `sent` (after `ECHO_BY`), `failed` (with `RELEASED`) or a refusal (`released`); `Withdrawn` takes only reactions, edits and votes (and, with media out, HD copies), so no row leaves its wait behind another way.
- `chat_jid` resolves a JID in one hop, through the chat's own row, its merge or its person, and `repoint` aims the merges going into a chat that's merging away at the chat kept, right after `merging` moves the waits. So in a chain of merges, what WhatsApp names by any of the chain's JIDs finds its waits, as the three chats' test shows.
- A setting has at most one wait for each index jaw's patches named: the chat's JIDs, and for a star its sender's. Each has its own outbox row, so the table stays bounded by the outbox, and `jawd status` counts it.

### The refusal's new condition

`released` ends the refused patch's wait and puts back what it set only when no other wait for its setting is later by `(at, named)`, the order `newest` weighs them in.

- **A later wait stands:** the chat shows that patch's intent, which WhatsApp ends with under its index unless it's refused too, when its own refusal puts back. The fetch's values are weighed against it at its time. Putting back would undo that intent, so not putting back is right; the test of a refusal beside a later wait checks it.
- **No later wait:** as before I6. The default goes back in the refusal's transaction, and the fetch then sets what WhatsApp holds, each value weighed against any earlier wait still on its way.
- **A wait already gone:** replaced by a later patch naming its index, or ended by its echo or `echo_by`. `WAITED_FOR` finds nothing, and nothing is put back, as before.
- It never brings back a value WhatsApp no longer holds: it puts back only how a chat is when nothing set it, which the fetch corrects. The window until the fetch is H2's (Hygiene).
- It looks under its own setting only, as it must: that part is T7's first.

### The migration

- **A schema step, declared:** its comment says `26: … a schema step: one new table and an index on it`. The table is new in this migration, so the new primary key needs no rebuild, and the step costs the same however big jaw.db is. Its line in `migrations.lock` is `26 schema 18e69a4d…`, the text's hash (`each_merged_migration_is_still_the_text_it_was_merged_with` passes).
- **Its text:** `key TEXT NOT NULL CHECK (json_valid(key))`, `named` the same, `PRIMARY KEY (key, named)`, then `UNIQUE (outbox)`, so the outbox's index keeps its name (`sqlite_autoindex_app_state_awaiting_2`) and the outbox's pinned plans stand. `STRICT, WITHOUT ROWID`, its class (jaw's own) declared beside it.
- **Its test** (`migration_26_keeps_one_wait_an_index_for_one_outbox_row_each`): two waits for one setting naming different indexes stand side by side; two naming one index don't, nor two for one outbox row.
- **`kept.rs`:** the class and the seed stand; the seed's row names another index than its key, which the new key allows. `kept`'s test passes, as do the fingerprint test (`a_schema_changed_by_hand_is_refused`) and the number test.
- **Plans:** `AWAITED_AT`, `AWAIT`, `WAITED` and `MOVED` changed; `AWAITING`, `WAITED_FOR` and `LATER_WAIT` are new. Each is in `plans.txt` and the writer's plan test, searching the primary key (`key=?`, `key=? AND named=?`, or the key's range) or the outbox's index; none scans or sorts. The three plan tests pass.

### Durability and the rest of the standard

- **Nothing is acknowledged before a synced commit:** `act` still answers after `durably`, and `r4-unsynced` fails the synced-commit test. I6 adds no commit; it changes what the chat action's, the converter's, the merge's and the refusal's transactions write.
- **Idempotent:** a replayed chat action writes the same waits; a duplicate echo finds its wait gone and is weighed by time, changing nothing; a merge's start runs once (`merge_started`), and `merging` takes the waits off the gone chat's key as it moves them, so a second run moves nothing.
- **A kill at any commit:** each wait changes inside the transaction of what it belongs to, so a kill leaves both or neither.
- **Counters:** none changed. **Queues:** none new.

### The owner's rules

- **O1, newest by time:** what WhatsApp sends for a chat's setting applies only when it's newer, by its time and then its index, than all that's held for it under the chat's JIDs, each of jaw's waits counting at its time in place of its index's raw value. Switched off at the hold-back: no newer value ever found (`r5-held-no-times`) fails 23; the waits not counted (`r5-held-no-waiting`) 18; `o2-time-only`, the tie, 2.
- **`fix_blind`:** across JIDs the newest stands, whichever JIDs the phone uses, so a setting under a LID weighs alike with one under a number. Every JID of the person is looked under: at the hold-back (`r5-held-no-person` fails 4) and at a merge (`r4-settled-no-person` fails T6's third test).

### Landing

Not this round, since T7 stands; worked out for the round that's approved. Trial merges with `git merge-tree` (no working tree touched) of `d42da1c1` with main (`7b1ad56e`): 11 files conflict; with media out (`81c45fa1`): 13; with communities (`60f83020`): 3; with disappearing (`fa03773b`): 12.

- **The number:** the next free one when it lands. Main holds 26 (contact cards, `26 schema 3732b60a…`); media out takes 27 (`0c7b0505…`), communities 28. So this branch takes 29 after both, or 30 if disappearing lands before it too.
  - `migrations.lock`'s line goes last, as `<n> schema 18e69a4d683f05b66b98cc4fb66633823ff6ea8cd8660d172cf09b2dfe669d7b`: renumbering changes only the comment outside the text, so the hash stays, unless the next fix round changes the text.
  - `schema.rs`'s comment becomes `// <n>:`, after the migrations before it in landing order; the test becomes `migration_<n>_keeps_one_wait_an_index_for_one_outbox_row_each`.
  - The plan's lines that say 26, and the six commit messages that name it: the renumbering (`d69362f6`), H8's test (`a1e63535`), I5 (`4f796cae`), I6 (`a5e376ad`) and the two docs commits (`678462e8`, `d42da1c1`).
- **`kept.rs`:** main's, plus media out's seeds (`outgoing`, `outgoing_drop`), communities' classes and seeds, disappearing's (`opened`) if it lands first, and `app_state_awaiting`'s class and seed with `named`. Its test then seeds the version before this branch's migration, so it's run again.
- **The trailer:** every commit `scripts/check-data-audit.py` names takes `Data-Audit: docs/audits/2026-10-29-chat-actions.md`: 64 of the 86 now (round four's 62, I6's and T6's), and the next fix round's. The rule's `git rebase origin/main -x 'git commit --amend --no-edit --trailer "…"'` gives it to all of them, which does no harm. The note is committed in the branch.
- **The outbox's writer with media out** (`store/write/outbox.rs`, five regions):
  - `Step` keeps the branch's `Refused` beside media out's `Attach` and `Unsent` (and disappearing's `Disappearing`, if it's there first), and `apply` takes every arm;
  - `SHOWN_DELETED` takes media out's form (`concat!("SELECT ", shown_deleted!(), " FROM messages t WHERE t.n = ?1")`), made `pub(super)` for `star`, its plan pinned again;
  - the plan test's rows take media out's plans for `ACT` and `APP_STATE` (an insert into `outbox` now searches `outgoing` by its key, for its foreign key), beside the branch's `REFUSED_ROW`, `ECHO_BY` and `RELEASED`; `plans.txt` is written again with `JAW_PLANS=write`, then read: only both sides' statements may change. A refusal's `DROP_ROW` now also cascades into `outgoing`, by its primary key;
  - **`typed`, media out's draft rule:** a message that's what was typed in its chat (text, or a file whose caption goes) clears the draft. The branch's app-state rows go in through `app_state_row`, never through `outgoing()`, so the rule doesn't meet them. Re-run media out's draft tests and the branch's outbox and refusal tests after the merge, and check media out's HD copies count in `waiting`, which `full` refuses a chat action by.
- **Presence** (`send/mod.rs`): three predicates are the same test (online, catching up or phone offline): main's `presence::reachable(state)`, main's `send::presence::connected(connection)`, and the branch's `sync::reachable(connection)`, which the branch made of `connected`. Keep one, and point the branch's callers at it: `Sending::start`, `send/presence.rs` and `sync/refresh.rs`. Media out's `media` and `copies` fields of `Inner` go beside the branch's `live`.
- **Communities** (`migrations.lock`, `schema.rs`, `protocol/README.md`): its migration changes a community's `last_at`, and so where it sorts; the pinned range of `chats_by_sort` that `PINNED_ELSEWHERE` counts (`sort >= '1~'`) is a prefix of its own. Re-run the pin tests that count it (at most three pinned, archived ones not counting, a chat merging away counting once) and the refusal of a community's own row.
- **Disappearing, if it lands first** (twelve files): `Step::Disappearing` beside `Refused`; `send/outbox.rs`'s attempt takes disappearing's `may_have_gone` (`&& !played && !timer`) with the branch's `(command, patch)`, disappearing's arms giving `None` for the patch; `store/write.rs`'s `touches_fts` takes both sides' arms (the branch's `Change::AppState`, disappearing's `Expire` and `Once`); history's `unsaid` stands beside disappearing's timer rule, each applying to history's chat; `kept.rs` takes both seeds; `send/mod.rs` and `send/actions.rs` keep both sides.
- **What the lander re-checks:**
  - the gate (`make check-generated lint build test`), judging timing failures against the landers' list;
  - the migration's test, `kept`'s, the lock and number tests, the fingerprint test and both plan tests;
  - `tests/upgrading.rs` from version 1 through the new number, with `--engine demo`;
  - the outbox's tests (`store::write::outbox`, `send::outbox`), media out's draft and HD tests, the chat list's pin tests and the demo engine's (`engine::demo`);
  - the writer's tests with I4's, I5's, I6's, O2's and T7's switches (`chat-actions-audit-r5-switches.py` in the reports folder);
  - the trailers, then `make check-audit`.

## Measurements

All in a clone of the worktree under `/tmp/jaw-audits/chat-actions-5`, built with `CARGO_INCREMENTAL=0`, on this Mac (20 cores) at a load average of 21 to 77 from other agents' work, everything run with `nice`.

- **jawd's unit tests at the head** (the test binary of `cargo test --workspace`, 10 threads): 1,018 passed, 2 failed and 16 ignored, in 3,289 s. The two, the feed's `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` and the socket's `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`, are on the landers' list, and each passed alone 3 of 3.
- **The engine's tests** (`go test -race -count=1 ./...`, Go's build cache in the audit folder): ok, four packages.
- **The workspace's other tests at the head** (`cargo test --workspace --exclude jawd`, then jawd's integration files, then `--doc`): jaw 334 passed (2 ignored), its composing, failures, settings and typing files 3, 10, 3 and 1; jaw-client 24, and its jawd file 3; jaw-media 23, and its pipeline 18 (1 ignored); jaw-proto 16, and its fixtures 3; jaw-voice 36 (2 ignored); wa-proto's roundtrip 3; jawd's installed 1, starting 1, stopping 4 and upgrading 1 (a version 1 jaw.db upgraded through 26). None failed. The doc tests: exit 0.
- **Probes** (`chat-actions-audit-r5-probes.rs`), through the writer as the chat actions' tests do. With no switch all three pass.
  - **P1:** Ana's chat pinned at `AT`, Rui's muted for ever at `AT + 1000`, Ana's pin refused: unpinned, one wait left (Rui's). With `r5-later-wait-any-key`: still pinned.
  - **P2:** Ana's chat pinned, then archived at `AT + 1000`, two patches at one time (the archive, its unpin); the archive refused: not archived, not pinned. With `r5-later-wait-any-key`: still archived.
  - **P3:** jaw's pin at `AT` taken by WhatsApp with inbox rows 6 and 7 waiting, so its echo is due by 7; jaw's unpin at `AT + 1000`: its wait has no `echo_by`. The pin's echo at 6 is held back, and row 7 is drained. The phone's pin at `AT + 5000` at row 8: held back, unpinned, one wait; the unpin's echo at 9 ends it. With `r5-await-keeps-echo-by`: the unpin's wait carries 7, ends as the converter passes 7, and the phone's pin shows: pinned at `AT + 5000`, no wait left.
- **Switches**, applied in the audit's clone, built once, run one at a time with `JAW_MUTANT`, in parallel runners, against the 189 tests (the 191 less the two slow merge tests), or 226 for mine (with the 37 of `send/`). With no switch: 191, 189 and 226 pass.

| Switch | What it changes | Fails (of 189 / 226) | The fix round's count |
| --- | --- | --- | --- |
| `i5-hold-kept` | held back by order under the kept chat's JID, whatever index the patch named | 7 of 189 | 7 |
| `i5-named-by-time` | the named index weighed by time, not held back by order | 10 of 189 | 10 |
| `i5-skip-kept` | jaw's patch counts in place of the kept chat's JID's value | 2 of 189 | 2 |
| `o2-time-only` | a tie isn't newer, in the filter and the loop | 2 of 189 | 2 |
| `i4-combined` | start_merge leaves the combination | 7 of 189 | 7 |
| `i4-again-combined` | merge_settings_again leaves the combination | 1 of 189 | 1 |
| `i4-no-waiting` | at a merge jaw's waits don't count (their indexes still skipped) | 6 of 189 | 6 |
| `i4-earlier-wait-counts-raw` | at a merge only the latest wait counts, and the others' indexes count their raw values | 2 of 189 | 2 |
| `i4-gone-wait-stays` | at a merge only the gone chat's waits count | 10 of 189 | 10 |
| `i4-kept-wait-stays` | at a merge only the kept chat's waits count | 2 of 189 | 2 |
| `r3-move-no-star` | a merge moves no star's wait | 1 of 189 | 1 |
| `r3-move-no-echo-by` | a moved wait loses echo_by | 1 of 189 | 1 |
| `r4-echo-time-only` | an echo is known by its time alone, under any index | 1 of 189 | 1 |
| `r4-named-raw-counts` | the raw value under an index a waiting patch named counts beside the patch | 5 of 189 | 5 |
| `r4-tie-loop-first` | of tied values held, the first found stands (the raw loop only) | 1 of 189 | 1 |
| `r4-merging-named-moved` | a moved wait's named becomes the kept chat's index | 11 of 189 | 11 |
| `r4-queue-named-key` | a wait's named is its key, not the patch's index | 1 of 189 | 1 |
| `r4-back-kept-state` | at a merge, jaw's newest wait takes the kept chat's value | 9 of 189 | 9 |
| `r4-back-gone-state` | at a merge, jaw's newest wait takes the gone chat's value | 10 of 189 | 10 |
| `r4-settled-no-instead` | at a merge, WhatsApp's newest held isn't applied | 2 of 189 | 2 |
| `r4-settled-no-person` | at a merge, only the two chats' JIDs are looked under | 1 of 189 | 1 |
| `r4-unsynced` | a chat action commits on the live lane, not durably | 1 of 189 | 1 |
| `i6-queue-one-wait` | jaw's next patch for a setting takes the place of every wait for it (round four's AWAIT) | 2 of 189 | 2 |
| `i6-move-one-wait` | a merge leaves one wait a setting, the later patch's (round four's MOVED) | 5 of 189 | 5 |
| `i6-held-first-wait` | the hold-back reads one wait a setting, the first | 10 of 189 | 10 |
| `i6-waited-by-key` | an echo ends every wait for its setting | 11 of 189 | 11 |
| `i6-settled-first-wait` | at a merge, settled reads one wait a chat, the first | 1 of 189 | 1 |
| `i6-released-any` | a refused patch puts back whenever its own wait was there, a later wait or not | 1 of 189, 1 of 226 | 1 |
| `i6-marking-named-kept` | marking looks only for a wait naming the chat jaw keeps | 2 of 189 | 2 |
| `r5-later-wait-any-key` | a refusal looks for a later wait of any setting, not its own | 0 of 226 | — |
| `r5-moved-always` | a moved wait takes the place of one naming its index whatever their times | 0 of 226 | — |
| `r5-after-own-only` | the hold-back ends a wait past where its echo had to be only under its own index | 0 of 226 | — |
| `r5-echo-index-only` | an echo is known by its index alone, at any time | 33 of 226 | — |
| `r5-await-keeps-echo-by` | a later patch's wait keeps the echo_by of the earlier one it replaces | 0 of 226 | — |
| `r5-held-no-person` | the hold-back weighs only the kept chat's JID and the one named, not the person's others | 4 of 226 | — |
| `r5-held-no-waiting` | the hold-back counts no wait of jaw's at its time, nor skips their indexes | 18 of 226 | — |
| `r5-held-no-times` | the hold-back never finds a newer value held: what comes applies unless held back by order | 23 of 226 | — |

- **Trial merges** (`git merge-tree --write-tree`, nothing checked out): main `7b1ad56e` 11 files; media out `81c45fa1` 13, among them `kept.rs` (one region) and `store/write/outbox.rs` (five, four in the code and one of both sides' tests); communities `60f83020` 3 (`migrations.lock`, `schema.rs`, `protocol/README.md`); disappearing `fa03773b` 12, among them `store/write/outbox.rs` (`Step`), `store/write.rs` (two regions), `send/outbox.rs` (the attempt), `kept.rs` and `send/mod.rs` (four).
- **The trailer:** `python3 scripts/check-data-audit.py 77d44257 d42da1c1` names 64 commits without it.

# Round four: `678462e8`, round three's fixes and the sixth rebase

Range: `77d44257..678462e8` in `chat-actions` (83 commits on main's `77d44257`, the library), 2026-11-02. Auditor: database-expert. Main is now `7b1ad56e`: presence (no migration), contact cards (migration 26) and the Mac's pictures and files (under `mac/` and `docs/` only) have landed on it since `77d44257`. Media out (`d53934f0`) is landing as migration 27, and communities (`60f83020`) as 28 after it.

Rounds three, two and one are kept as written below this round. This round's verdict comes at the end of the note, after theirs.

## In short

- **Verdict: changes needed.** Round three's asks are done, and each fails a test when it's switched off: I5 with H10, O2, I4 with X2, T4 and T5. The migration's new column is a schema step, declared, seeded in `kept.rs` and locked by its text's hash. But the narrow case the plan leaves still loses a request (I6), in three shapes, one of which the plan doesn't name.
- **I6, inconsistency:** `app_state_awaiting` keeps one wait a setting. So when a wait for jaw's later patch takes the place of one for an earlier patch that named another index, the earlier index is forgotten. That happens as a merge moves the gone chat's wait onto the kept chat's (the plan's case), and as jaw's next patch for the setting, which after a merge names the kept chat, replaces a wait the merge moved (not the plan's case, and the likelier).
  - What comes under the forgotten index before its patch's echo is then weighed by time. The phone's change there, later by its time than jaw's later patch but taken by WhatsApp before jaw's earlier one, shows until the last echo.
  - A request for that value is answered `Already`, and the echoes then set jaw's value: the request is lost on every device (probes N1, N2, N3).
  - The chat always ends as WhatsApp holds it, and nothing is held longer than it should be. Only the window shows a value that won't last.
  - **Fix:** a wait for each index jaw's waiting patches named (`app_state_awaiting` keyed by the setting and the index named), so what comes under any of them is held back until that patch's echo. The migration isn't merged, so its text can change again.
- **What holds:**
  - Every switch of the fix round's fails its tests, to the count. Of the audit's eleven, eight fail theirs. Three fail none (T6, a test gap): an echo known by its index, O2's tie inside its loop, and the merge's look under a third JID of the person. Each is reached only on an exact millisecond tie or for a person with three JIDs.
  - Durability: a chat action still answers only after its synced commit.
  - Plans: the four statements I5 changed keep their plans, and I4 adds none.
  - The sixth rebase kept both sides at its five stops and changed nothing else.
- **Performance:** I4's `settled` adds about 0.5 ms to a merge's start in a release build, on the 1.3 ms it takes without it. That stands as a follow-up: a background batch is cut at 3 ms by time, and a live batch of many merges is past the live budget without it already.
- **Landing:** after media out (27) and communities (28), the migration takes 29, or 28 if it lands before communities. The trial merges with main and media out meet in thirteen files; the outbox's writer needs care beyond keeping both sides. What the lander re-checks is under "Landing", below.

## Findings

### Data loss

None.

### Inconsistency

- **I6. A wait forgets the index jaw's earlier patch named once a wait for a later patch, naming another index, takes its place. What the phone sent under the forgotten index before that patch's echo is then weighed by its time, and shows until the echoes undo it. A request judged against it is lost, as in I1, I3, I4 and I5.**
  - **Where:**
    - `app_state_awaiting` has one row a setting: `key` is its primary key (`store/schema.rs:1419`; "One row a setting at most", `:1413`).
    - So a later patch's wait takes the earlier one's place, `named` and all: `MOVED` as `merging` moves the gone chat's wait onto the kept chat's key, when the moved one is later (`store/write/chat_actions.rs:58-61`, `:550`), and `AWAIT` as `queue` keeps jaw's next patch for the setting (`chat_actions.rs:45-46`, `:301`).
    - jaw's next patch for a merged chat's setting names the kept chat: the handler names a chat by its own JID or the one it's merging into (`CHAT_TO_ACT_ON`, `send/chat_actions.rs:38`). So the AWAIT case comes about whenever jaw changes a setting again after a merge moved its wait from the gone chat.
    - `held_back` then holds back by order only what comes under the staying wait's `named` (`chat_actions.rs:369`), and `newest` leaves out only that index's raw value (`chat_actions.rs:452`).
  - **What happens** (probes N1, N2 and N3, under Measurements; Ana's chats under her number and her LID, separate until her LID is joined to her):
    - **N1, the plan's case:** jaw pins each of her chats, the second a millisecond later, and they merge. One wait stays, the later pin's. The phone's unpin, later than both pins by its time but taken by WhatsApp before jaw's first pin, comes under the index the first pin named. It's weighed by time and applied, so the chat shows unpinned. An unpin asked then is `Already`. The first pin's echo is held back, and the second's pins the chat; a full fetch agrees, since WhatsApp holds jaw's pins under both indexes. The unpin is lost on every device. The same happens with the chats' parts swapped.
    - **N2, not the plan's case:** jaw pins her LID's chat, the chats merge, and jaw's unpin of the merged chat, naming her number, takes the moved wait's place. The phone's pin under her LID, later than jaw's unpin by its time but taken by WhatsApp before jaw's pin there, is applied: pinned. A pin asked then is `Already`. jaw's pin's echo is held back, and the unpin's echo unpins the chat, as a full fetch then does. The pin is lost.
    - **N3:** jaw pins her LID's chat, and the phone's later unpin there, taken by WhatsApp first, is held back, as it should be. jaw then pins her number's chat, and the chats merge. The later pin's echo ends the one wait, and the unpin kept raw under her LID, which WhatsApp has replaced with jaw's earlier pin, is the newest held: unpinned. An unpin asked then is `Already`, and the earlier pin's echo pins the chat. Lost.
  - **What it isn't:** the chat ends as WhatsApp holds it in every case: after the last echo, and after a full fetch, the newest held stands. Nothing is held back longer than it should be. Only the window, from the phone's change to the last echo, shows a value that won't last.
  - **Reach:** narrower than round three's I5. Each case needs:
    - two of jaw's changes to one setting of a person's chats, with a merge before the first one's echo (both before the merge in N1 and N3, one on each side of it in N2);
    - the phone changing that setting later, by its time, than jaw's second change, while jaw's first hasn't reached WhatsApp yet.
    An engine start's full fetch brings these together: contacts come first and join the person's chats, while jaw's queued patches wait behind the fetch. The design's caveat for the app state whatsmeow hands over by itself doesn't cover it, since jaw knows each index it named, so it can be closed.
  - **Fix:** a wait for each index jaw's waiting patches for a setting named, rather than one a setting: `app_state_awaiting`'s primary key becomes `(key, named)`. The migration isn't merged, so its text can change again: its lock line and its test change, and `kept.rs`'s seed stays valid.
    - `queue` and `merging` replace a wait only under the same index named. One naming another index stays beside it until its own echo, its `echo_by` passing, or its outbox row going without going.
    - `held_back` holds back by order what comes under any index one of the setting's waits named, until that wait ends. Everything else is weighed by time, each wait counting at its time in place of its index's raw value.
    - `settled` counts every wait under the two chats' keys at its time, none of their indexes' raw values, and the newest stands.
    - `marking`, the put-back of a refused patch ("while it's its setting's latest" becoming the latest of its setting's waits), and the plans follow. Rows stay bounded by the outbox, since each wait keeps its own outbox row (`outbox` is UNIQUE).
  - **Tests:** N1 in both orders, N2 and N3, each failing without the fix.
  - If the owner instead accepts this window as one of the design's caveats, as O3's were, it can stand as "other", with the plan's decision and the design naming all three cases. That's the owner's call, not the audit's.

### Test gaps

- **T6. Three small parts of this round's rules fail nothing when switched off.** Each is reached only on an exact millisecond tie or for a person with three JIDs, so none would lose a request on its own often, but each is a rule this round touches.
  - **An echo known by its index:** with an echo known by its time alone, under any index of the setting (`r4-echo-time-only`, at `held_back`'s `own == named`, `store/write/chat_actions.rs:367`), none of the 151 tests fails. A change from another device under another JID of the chat, carrying exactly jaw's patch's time, would then end the wait. What comes afterwards under the index jaw's patch named would be weighed by time, which is I3's loss again.
  - **O2's tie inside `newest`'s loop:** with the first of tied values found standing (`r4-tie-loop-first`, `chat_actions.rs:456`), none fails. C1 has two JIDs, so the loop never compares two tied values and the last comparison decides. Three values for one setting at one millisecond, or one at jaw's own patch's time, would end by which JID comes first, which O2 was meant to stop.
  - **`settled` looking under the person's other JIDs:** with only the two chats' JIDs looked under (`r4-settled-no-person`, `chat_actions.rs:477`), none fails. For a person with two JIDs that's the same set. With a third JID that WhatsApp named the chat by, its value wouldn't count at the merge.
  - **Fix:** a test for each:
    - the phone's change under her number at exactly the time of jaw's pin under her LID leaves the wait, so the phone's earlier unpin under her LID is still held back;
    - three values tied in time, or one tied with jaw's waiting patch, end the same in any order;
    - a merge for a person with a third JID, the newest value held under it.

### Unpredictable performance (follow-up; breaks no budget of its own)

- **P4. I4's `settled` adds about 0.5 ms to each merge's start.**
  - **Measured** (Measurements, T1): in a release build, a merge's start took 1.85 to 1.90 ms with `settled` and 1.32 to 1.35 ms without it. In a debug build it was 2.07 and 1.43 ms. Most of it is SQLite's work: `settled`'s lookups for four settings, and `upsert_chat` writing the settled chat.
  - **Against the budgets:** a background batch is cut at 3 ms by time (`BATCH_TIME`), so there it means fewer merges a batch. A live transaction takes up to 64 changes (`LIVE_CHANGES`); one beginning many merges is past the 5 ms live budget without `settled` already (1.3 ms each), and about 40% further with it.
  - **Follow-up, for main:** bound the merges a live transaction begins, or begin them in the background lane.

### Other

- **O2, X2:** closed (C1, A5).
- **O3:** the caveats the owner chose stand.
- **O4, the landers' list:** `media::tests::downloads_wait_while_the_disk_is_nearly_full_and_go_on_once_there_is_room` failed in the full run under load ("3EB0WAITING never came to downloaded once there's room"), and passed alone 3 of 3. It's main's media code, which the branch doesn't change. It's a first sighting, to watch, as the landers' rule has it.

### Hygiene (follow-ups)

- H9, and round one's H1 to H3, H5 and H6 and P1 to P3, stand, as the plan's items 14 to 16 say.

## What was asked, checked

### Round three's fixes, each switched off

I applied the fix round's twelve switches and eleven of mine in the audit's clone, all behind `JAW_MUTANT` in one build, never in the worktree. Each ran against the same tests as the fix round's: the chat actions' writer tests, the rules', the converter's app-state tests, migration 26's, the writer's own and the merges', plus the synced-commit test and my three probes, which assert nothing. That's 153 tests, which all pass with no switch. For the switches after the first two, two of the merges' tests were left out: they test reads and messages moving in a merge, which no switch touches, and they took 200 of each run's 220 seconds. The counts are under Measurements.

- **I5** (`4f796cae`), closed:
  - A wait keeps the index jaw's patch named (`named`). `queue` keeps the patch's own index there, and `merging` moves it with the wait.
  - `held_back` holds back by order only what comes under it, and weighs everything else by time, jaw's patch counting in place of that index's raw value only.
  - Switches: the kept chat's JID held back by order (`i5-hold-kept`) fails 4 tests, D0 and D1's, E1's and F1's among them; the named index weighed by time (`i5-named-by-time`) fails 5; jaw's patch counting in place of the kept chat's value (`i5-skip-kept`) fails 1.
  - Mine: the move giving a wait the kept chat's index (`r4-merging-named-moved`) fails 5: D0 and D1's, E1's, T4's, the two waits', and what's held back under the named index; a wait naming its key rather than the patch's index (`r4-queue-named-key`) fails 1, the group star the phone names by its sender's LID; the raw value under the named index counting beside jaw's patch (`r4-named-raw-counts`) fails 2, the test of what's held back under the named index and round two's mute weighed against jaw's time; an echo known by its time under any index (`r4-echo-time-only`) fails none (T6).
- **H10**, closed with I5: the migration's comments say a wait is kept for a setting, by the JID jaw keeps its chat by, and a star's without its sender, and what `named` holds. Its test says "keyed by an index as JSON".
- **O2** (`7b83fdac`), closed: `newest` compares `(time, index)` in its loop, and `newest_held` the same against what came, jaw's waiting patch under the index it named. A tie in time no longer depends on which value comes first.
  - `o2-time-only` fails C1's test.
  - Mine: the loop alone keeping the first of tied values (`r4-tie-loop-first`) fails none (T6).
- **I4 with X2** (`c57e1c72`), closed: `chat_actions::settled` gives each of the kept chat's four settings the newest held under the two chats' JIDs and their person's, before `merging` moves the waits. Jaw's later wait counts at its time, with the value of the chat it was queued for, and neither wait's index counts its raw value. `start_merge` applies it after main's combination, and so does `merge_settings_again`. A setting nothing is held for keeps main's rule.
  - `i4-combined` fails 6 tests (A1 to A5, and the earlier wait's index); `i4-again-combined` 1; `i4-no-waiting` 6; `i4-earlier-wait-counts-raw` 1; `i4-gone-wait-stays` 3; `i4-kept-wait-stays` 1.
  - Mine: the staying wait taking the kept chat's value (`r4-back-kept-state`) fails 7, or the gone chat's (`r4-back-gone-state`) 7, A1 to A4 among them; WhatsApp's newest held not applied (`r4-settled-no-instead`) fails 1, A5; only the two chats' JIDs looked under (`r4-settled-no-person`) fails none (T6).
  - The converter gives each of the four settings a `Change::Chat`, a read giving a `Change::Read` first (`convert/app_state.rs:36-72`), so `settled`'s use of it holds for all four.
- **T4** (`6362c9b0`) and **T5** (`efe268c7`), closed: `r3-move-no-star` fails T4's test, and `r3-move-no-echo-by` T5's.
- **The narrow case the plan leaves:** it can't make a setting end wrong, and nothing is held longer, but it can lose a request (I6), in the plan's case and in two more. It blocks, by the standard round three held I5 to, unless the owner accepts it as a caveat.

### The migration's new column

- **A schema step, declared:** `named TEXT NOT NULL CHECK (json_valid(named))` in the migration's one new table (`store/schema.rs:1421`), which costs the same however big jaw.db is. Its line is `26 schema` and its comment says so. NOT NULL needs no default: the table is new in this migration, so there are no rows to fill.
- **Its class:** the table is jaw's own, its index derived from its own column, each declared beside its `CREATE`.
- **`kept.rs`:** main's, plus `app_state_awaiting`'s class and seed, the seed now with `named` (`store/schema/kept.rs:421`). `kept`'s test passes, seeding version 25.
- **Nothing of jaw's own is dropped:** the migration only creates; `kept`'s test passes.
- **The lock line:** `26 schema 3e745162…` is the text's hash: `each_merged_migration_is_still_the_text_it_was_merged_with` passes at the head. The renumbering comment sits outside the text, so the hash stays as the number changes.

### Durability and the rest of the standard

- **Nothing is acknowledged before a synced commit.** `act` still answers only after `durably`; with it committing on the live lane (`r4-unsynced`), the synced-commit test fails (1 test).
  - `settled` and its `upsert_chat` run inside `start_merge`'s transaction, and `merge_settings_again`'s inside `merge_step`'s, so each commits or rolls back with the merge it belongs to. A kill before the commit has the merge begin again from the same inbox row; after it, `merge_started` keeps `merge_settings_again` from running for it.
  - Nothing in `send/`, the engine, the acks or the protocol changed this round.
- **Idempotent:** `settled` reads only `app_state` and `app_state_awaiting`, each committed with what it reflects, so a replay reads the same and writes the same. Applying the same chat again changes nothing.
- **Counters:** none changed.
- **Queues:** none new. `app_state_awaiting` stays bounded by the outbox (`outbox` is UNIQUE) and counted in `jawd status`.
- **Plans:** the four statements I5 changed only add `named`, and keep their plans. I4 reuses `AWAITED` and `HELD_UNDER`, and O2 adds no statement. The three plan tests pass at the head.

### The sixth rebase, onto the library

- `git range-diff` from round three's range (`e12d8d34..9bce63e7`) to this one: 64 commits unchanged, 6 new (the five fixes and the docs), and 13 changed:
  - the five stops, each keeping both sides: the fixtures' arm and imports, the demo engine's `SPEECH` and `SATURDAY` beside `collection()`, `migrations.lock` with `schema.rs` (main's `25 data` before the branch's, now 26), the migration's own line, and the protocol README;
  - the renumbering's two commits (the plan's lines, and H8's test as `migration_26_…`);
  - six whose only change is context: the demo engine's `SPEECH` in one, and the protocol README's `## Library (proposed)` becoming `## Library` in five.
- What the library brings that the branch meets:
  - its migration 25 is a data step before the branch's schema step, which `migrate` runs in order, as main's other schema steps after its data steps;
  - its seven tables are all derived, so `kept.rs` needed no seed of theirs; `kept`'s test now seeds version 25 for the branch's 26, and passes;
  - its changes to the merge's writer (`rehome`, `merge_step`, `fold`) don't touch `start_merge`, where `settled` and `merging` run. `merge_step` now also shows the gone chat's newest message, which touches no setting or wait.

### Landing

Not this round, since I6 stands. Worked out here for the round that's approved. Trial merges with `git merge-tree` (no working tree touched) of the head with main (`7b1ad56e`), with media out on main (`d53934f0`), and with communities (`60f83020`, still on `77d44257`) show what its lander meets.

- **The number:** the next free one when it lands. Main holds 26 (contact cards, `26 schema 3732b60a…`). Media out lands as 27 (`27 schema 0c7b0505…`), and communities as 28 after it. So this branch's migration takes 29 if it lands after both, or 28 if it lands before communities, which then takes 29.
  - Its line in `migrations.lock` goes after the last, as `<n> schema <hash>`, with the hash of the migration's text as the fix round leaves it (`3e745162…` now; I6's fix changes the text, so a new hash).
  - `schema.rs`'s comment becomes `// <n>:`, after the migrations before it in landing order.
  - The test becomes `migration_<n>_…`. It finds its start by the migration's text, so only its name changes.
  - The plan's lines that say 26 change, as do the messages of the renumbering commit, H8's and I5's.
- **`kept.rs`:** main's, plus media out's two seeds (`outgoing`, `outgoing_drop`), communities' classes and seeds, and `app_state_awaiting`'s class and seed with `named`. Its test then seeds the version before this branch's migration for the first time, so it's run again.
- **The trailer:** every commit of the branch that `scripts/check-data-audit.py` names, 62 of the 83 now (all but the plan's, the protocol's and `jaw-proto`'s), and the next fix round's, take `Data-Audit: docs/audits/2026-10-29-chat-actions.md`; the rule's `git rebase origin/main -x '… --trailer …'` gives it to every commit, which does no harm. The note itself is committed in the branch.
- **Conflicts with main and media out** (thirteen files), mostly two additions at one place, kept both:
  - `jaw-proto/src/send.rs`, `jaw-proto/tests/fixtures.rs`, `engine/demo.rs` (its module doc), `engine/internal/wa/send_test.go`, `protocol/README.md`, `migrations.lock`, `schema.rs`, `kept.rs`, `plans.txt` (written again with `JAW_PLANS=write`, then read: only both sides' statements may change), and `store/write.rs` (`pub mod contacts;` beside `pub mod chat_actions;`).
  - `scripts/check-data-audit.py`: main's `crates/jawd/src/convert/` covers the branch's `convert/app_state.rs`, so main's lines stand.
- **Where it takes more than keeping both sides:**
  - **The outbox** (`store/write/outbox.rs`, five regions):
    - media out turns `SHOWN_DELETED` into `shown_deleted!()` and adds `HD_SHOWN_DELETED`. The branch only made the old text `pub(super)`, for `star`, so take media out's form, make it `pub(super)`, and pin its plan again;
    - `Step` takes the branch's `Refused` inside the enum beside media out's `Attach` and `Unsent`, before media out's `Attached` and `Kept`;
    - the plan test's rows take media out's plans for `ACT` and `APP_STATE`, which media out changed and the branch didn't, beside the branch's `REFUSED_ROW`, `ECHO_BY` and `RELEASED`;
    - both sides' tests are kept.
    `send/outbox.rs` merges by itself; re-check that media out's HD copies count in `waiting`, which `full` refuses a chat action by, and that app-state rows still wait for their turn and go in each chat's order.
  - **Presence** (`send/mod.rs`, six regions): the branch turned `Sending::start`'s `presence::connected` into its own `sync::reachable`; main kept `connected` and added `presence::reachable`. All three are the same test (online, catching up or phone offline), so keep one and point the branch's callers at it. Media out's `media` and `copies` fields of `Inner` go beside the branch's `live`. `sync::run`'s arguments are the same on main and media out, so the H4 test's call stands.
  - **Communities** (`migrations.lock`, `schema.rs`, `README.md`): its migration drops `chats_community_followed` and sorts a community by its own messages, so re-run the pin tests that count `chats_by_sort`'s pinned range (`PINNED_ELSEWHERE`), and check a community's own row is still refused.
- **What the lander re-checks:**
  - the gate (`make check-generated lint build test`), judging timing failures against the landers' list;
  - the migration's test, `kept`'s, the lock and number tests, the fingerprint test and both plan tests;
  - `tests/upgrading.rs` from version 1 through the new number, with `--engine demo`;
  - the demo engine's tests (`engine::demo`), the outbox's (`store::write::outbox`, `send::outbox`) and the chat list's pin tests;
  - the writer's tests with I4's, I5's, I6's and O2's switches;
  - the trailers, then `make check-audit`.

## Measurements

All in a clone of the worktree under `/tmp/jaw-audits/chat-actions-4`, built with `CARGO_INCREMENTAL=0`, on this Mac (20 cores) at a load average of 16 to 56 from other agents' work, everything run with `nice`.

- **jawd's unit tests at the head** (the test binary of `cargo test -p jawd --bin jawd`, 10 threads): 1,004 passed, 5 failed and 16 ignored, in 3,060 s, with the three plan tests run first on their own, passing. Each of the five passed alone 3 of 3. Four are on the landers' list: the feed's lane test, media in's starts-check test, the socket's connection-full test, and the checkpointer's idle test. The fifth, `media::tests::downloads_wait_while_the_disk_is_nearly_full_and_go_on_once_there_is_room` ("3EB0WAITING never came to downloaded once there's room"), isn't; it's main's code, which the branch doesn't change.
- **The engine's tests** (`go test -race -count=1 ./...`, Go's build cache in the audit folder): ok.
- **The workspace's other tests at the head**, each test binary run from its package's folder, as cargo runs them: jaw 334 passed (2 ignored), with its composing, failures, settings and typing files (3, 10, 3, 1); jaw-client 24, and its jawd file 3; jaw-media 23, and its pipeline 18 (1 ignored); jaw-proto 16, and its fixtures 3; jaw-voice 36 (2 ignored); wa-proto's roundtrip 3; and jawd's installed 1, starting 1, stopping 4 and upgrading 1 (a version 1 jaw.db upgraded through 26 with `--engine demo`). None failed. The doc tests: exit 0.
- **Probes**, through the writer, as the converter and the writer take each change (`arrives`, `acted` and `written` in the chat actions' tests). Ana's chats are under her number and her LID, separate until her LID is joined to her.
  - **N1**, with her number's chat pinned first (and the same with her LID's first): after the merge, pinned at `AT + 1`, with one wait. The phone's unpin at `AT + 5000` under the first pin's index: unpinned. Unpin asked then: `Already`. The first pin's echo: still unpinned. The second's: pinned at `AT + 1`, no wait left. A full fetch after: pinned at `AT + 1`.
  - **N2:** her LID's chat pinned at `AT`, merged; the merged chat's unpin at `AT + 1000` `Queued`, unpinned, one wait. The phone's pin at `AT + 5000` under her LID: pinned. Pin asked then: `Already`. jaw's pin's echo under her LID: still pinned. The unpin's echo under her number: unpinned, no wait left. A full fetch after: unpinned.
  - **N3:** her LID's chat pinned at `AT`; the phone's unpin there at `AT + 5000`, held back; her number's chat pinned at `AT + 1`; merged: pinned at `AT + 1`, one wait. The later pin's echo: unpinned, no wait left. Unpin asked then: `Already`. The earlier pin's echo: pinned at `AT + 1`.
- **T1, I4's cost at a merge's start.** Method: an in-memory jaw.db at the head's schema with 500 people, each with a chat under their number and one under their LID, a pin held under each LID and jaw's archive waiting under each number. Their JIDs are then joined 50 people to a transaction, as the drain applies contacts, timed around the transactions. Each case ran alternately with `settled` and without it (`i4-combined`).
  - Release build, 4 runs each: 1,851 to 1,902 µs a merge with `settled`, 1,320 to 1,348 µs without.
  - Debug build, 3 runs each: 2,066 to 2,078 µs with, 1,414 to 1,444 µs without.
  - The difference is about the same in both builds, so it's mostly SQLite's work: `settled`'s lookups for the four settings, and here `upsert_chat` writing the archive I4 keeps.
- **Switches**, applied in the audit's clone, built once, run one at a time with `JAW_MUTANT` against the 153 tests (151 after the first two, as above). None fail with no switch.

  | Switch | What it changes | Tests failing |
  | --- | --- | --- |
  | `i5-hold-kept` | held back by order under the kept chat's JID, whatever index the patch named (round three's rule) | 4: D0 and D1's, E1's, F1's, and what's held back under the named index |
  | `i5-named-by-time` | the named index weighed by time too | 5 |
  | `i5-skip-kept` | jaw's patch counts in place of the kept chat's JID's value (round three's rule) | 1 |
  | `o2-time-only` | a tie isn't newer, as before O2 | 1: C1 |
  | `i4-combined` | `start_merge` leaves main's combination | 6: A1 to A5, the earlier wait's index |
  | `i4-again-combined` | `merge_settings_again` leaves it | 1: a merge an earlier jawd began |
  | `i4-no-waiting` | at a merge, jaw's waits don't count | 6 |
  | `i4-earlier-wait-counts-raw` | at a merge, the earlier wait's index counts its raw value | 1 |
  | `i4-gone-wait-stays` | at a merge, the gone chat's wait counts | 3 |
  | `i4-kept-wait-stays` | at a merge, the kept chat's wait counts | 1 |
  | `r3-move-no-star` | a merge moves no star's wait | 1: T4 |
  | `r3-move-no-echo-by` | a moved wait loses its `echo_by` | 1: T5 |
  | `r4-echo-time-only` | an echo known by its time alone, under any index | none (T6) |
  | `r4-named-raw-counts` | the raw value under the named index counts beside jaw's patch | 2 |
  | `r4-tie-loop-first` | of tied values held, the first found stands (the loop only) | none (T6) |
  | `r4-merging-named-moved` | a moved wait takes the kept chat's index as `named` | 5 |
  | `r4-queue-named-key` | a wait's `named` is its key, not the patch's index | 1: the group star by his LID |
  | `r4-back-kept-state` | at a merge, jaw's staying wait takes the kept chat's value | 7 |
  | `r4-back-gone-state` | at a merge, jaw's staying wait takes the gone chat's value | 7 |
  | `r4-settled-no-instead` | at a merge, WhatsApp's newest held isn't applied | 1: A5 |
  | `r4-settled-no-person` | at a merge, only the two chats' JIDs are looked under | none (T6) |
  | `r4-unsynced` | a chat action commits on the live lane, not durably | 1: the synced-commit test |

- **Trial merges** (`git merge-tree --write-tree`, nothing checked out):
  - with main `7b1ad56e`: 11 files conflict;
  - with media out `d53934f0` (on main): 13, adding `kept.rs` (one region, 26 lines) and `store/write/outbox.rs` (five regions; four in the code, of 1 to 27 lines a side, and one of both sides' tests);
  - with communities `60f83020`: 3 (`migrations.lock`, `schema.rs`, `protocol/README.md`).

# Round three: `9bce63e7`, round two's fixes, O1 and the fifth rebase

Range: `e12d8d34..9bce63e7` in `chat-actions` (77 commits on main's `e12d8d34`, which is still main on GitHub), 2026-11-01. Auditor: database-expert. The library (migration 25, a data step) and presence (no migration) are approved and land before this branch, in that order.

Rounds two and one are kept as written below this round. This round's verdict comes at the end of the note, after theirs.

## In short

- **Verdict: changes needed.** Round two's asks are done, and each fails a test when it's switched off: I3 on every path round two found, T2, T3 and H8. O1's rule holds in any order of arrivals that has no tie, repeated and across restarts. But a merge in the middle of a wait still shows a value that won't last when the two chats disagree on the setting (I4), and the plan's two narrow cases lose a request the same way (I5).
- **I4, inconsistency:** a merge combines the two chats' setting by main's rule: pinned and muted if either chat is, archived only if both are, marked if either is. It does this whatever jaw's change is waiting for.
  - So while jaw's unarchive, unpin, mark read, or a later but shorter mute waits, the merged chat shows the other chat's value until jaw's echo.
  - A request for that value asked then is answered "already so", and the echo then sets jaw's value: the request is lost on every device (probes A1 to A4).
  - The fix round's merge tests only take the directions main's rule agrees with.
- **I5, inconsistency:** a wait doesn't remember the index jaw's patch named, so the hold-back works the wrong way round in two cases. The design's rule is the reverse in both.
  - After a merge has moved a wait, what comes under the JID jaw's patch named is weighed by time, and what comes under the kept chat's JID is held back by order.
  - For a star of someone else's message in a group, what names the sender by another JID is held back by order.
  - The phone's later change either shows until jaw's echo undoes it, or is held back until the echo applies it. A request judged against what shows meanwhile is lost (D1, E1, F1).
  - It's reached at an engine start whose full fetch joins a person's JIDs while jaw's patch waits, since contacts come before the settings. The plan leaves these cases by name.
- **T4 and T5, test gaps:** two of I3's merge rules fail nothing when they're switched off: moving a star's wait, and moving where a wait's echo has to be.
- **O2, other:** on an exact tie between two different values under a chat's two JIDs, each full fetch ends with whichever JID comes first (C1). A tie needs two devices' changes stamped within the same millisecond. Comparing `(time, key)` would make the end the same in any order.
- **X2, main's code:** once jaw's echo is in, a merge still sets the chat by main's rule, and the next engine start's full fetch puts the newest back (A5). I4's better fix closes this too.
- **H10, hygiene:** migration 25's comments still say a wait is keyed as `app_state` keys its index. Since I3 it isn't. The text can still change before it's merged, and I5's column changes it anyway.
- **What holds:**
  - Durability: nothing is answered before its synced commit.
  - Every new statement's plan is pinned.
  - O1 costs about 3 µs a mutation in a release build.
  - The rebase onto `e12d8d34` changed only context, and dropped `7b5fc48e` as round two said.
  - jawd's 963 unit tests and the engine's tests pass at the head.
- **Landing:** after the library and presence, the migration takes 26. The library's lines conflict in four files and presence's in one, each resolved by keeping both sides. What the lander re-checks is under "Landing", below.

## Findings

### Data loss

None.

### Inconsistency

- **I4. A merge sets a setting jaw's change is waiting for by main's combination of the two chats, not by jaw's change. So the merged chat can show a value that won't last, and a request judged against it is lost on every device, as in I1 and I3.**
  - **Where:**
    - `start_merge` moves the gone chat's waits (`chat_actions::merging`, `store/write.rs:1714`). It then sets the kept chat's `pinned` to the larger of the two, `archived` to the smaller, `muted_until` to the larger and `marked_unread` to the larger (`store/write.rs:1741-1749`), whatever waits.
    - `merge_settings_again` does the same at the end of a merge that an earlier jawd started for a person with three JIDs or more (`store/write.rs:1948-1965`).
  - **What happens** (probes A1 to A4, under Measurements):
    - **A1:** jaw archives Ana's chat under her number. Her LID's chat, a chat of its own that isn't archived, merges into it while the archive waits, and the chat shows unarchived. An unarchive asked then is `Already`. The archive's echo then archives the chat, so the unarchive is lost on every device.
    - **A2:** the phone had pinned both chats, and jaw unpins her number's. Merged while the unpin waits, the chat is pinned again. A pin asked then is `Already`, and the unpin's echo unpins it: the pin is lost. Meanwhile the pin that won't last counts toward the three, so another chat's pin can be refused.
    - **A3:** the phone marked her LID's chat unread, and jaw marks her number's read. Merged, the chat shows marked. A mark unread asked then is `Already`, and the mark read's echo reads it: lost.
    - **A4:** jaw mutes her LID's chat for two hours, then her number's for one. Merged, the chat shows two hours, though the wait that stays is the later patch's hour. Two hours asked then is `Already`, and the later echo sets the hour: lost.
    - The fix round's merge tests (P7, P7b, P8, and the two waits) take the directions main's rule happens to agree with: a pin, a mark unread, and the later mute being the longer. So nothing fails without a fix.
  - **Why it's reached:** a person's two chats merge when jaw first learns that their LID and number go together.
    - An engine start's full fetch brings contacts (`critical_unblock_low`) before the settings (`regular_high`, `regular_low`; `sync/refresh.rs:16`), and jaw's queued patches wait for their turn behind the fetch.
    - A newly linked account's history joins people the same way, while the owner is setting up chats.
  - **The window:** from the merge to the echo. That's under a second while WhatsApp can be reached, and the length of the full fetch at an engine start.
  - **Fix:** as a merge starts, a setting that still waits (after `merging`) takes the value of the chat whose wait stays, not main's combination. There's a better fix that closes X2 too: give each setting the newest value held for it under the two chats' JIDs, counting jaw's waiting change at its time and otherwise WhatsApp's latest under each JID in `app_state`, weighed as `newest_held` weighs them. Main's rule then applies only where nothing is held. Do the same in `merge_settings_again`.
  - **Tests:** A1 to A4's sequences, each failing without the fix.

- **I5. A wait doesn't keep the index jaw's patch named. After a merge has moved a wait, what comes under the JID jaw's patch named is weighed by time, and what comes under the kept chat's JID is held back by order. For a star of someone else's message in a group, what names the sender by another JID is held back by order. The design's rule is the reverse in both. The phone's later change shows until jaw's echo undoes it, or is held back until the echo applies it, and a request judged against what shows meanwhile is lost.** These are the two cases the plan's decision "Across a chat's JIDs, the newest setting stands" says it leaves.
  - **Where:**
    - `held_back` holds back by order what comes under `kept`, the JID jaw keeps the chat by (`store/write/chat_actions.rs:360`).
    - `newest_held` counts jaw's patch in place of every value held under `kept` (`chat_actions.rs:393`).
    - `kept` is the JID jaw's patch named only while the chat hasn't merged: `merging` moves a wait to the kept chat's JID (`chat_actions.rs:417-434`) when jaw's patch named the gone chat's.
    - A wait's key is the index jaw's patch named for every star but one of someone else's message in a group: `wait_key` keeps a star's wait without its sender (`chat_actions.rs:307-309`).
  - **What happens:**
    - **D1:** jaw pins her LID's chat, a chat of its own, which then merges into her number's. The phone's unpin comes under her LID. It's later than jaw's pin by its time, but WhatsApp took it first, while jaw's patch waited behind the full fetch. It's applied, and the chat shows unpinned. An unpin asked then is `Already`. jaw's echo, under her LID, pins the chat again, which is what WhatsApp now holds there, so the unpin is lost on every device. Without the merge (D0), the same unpin is held back, as the design says.
    - **E1:** the same, but the phone's later unpin names her number. It's held back, and the chat shows pinned. A pin asked then is `Already`. At jaw's echo, her number's later unpin stands, as O1 says, so the pin is lost.
    - **F1:** jaw stars Rui's message in a group, naming him by his number. The phone's later unstar names him by his LID, and is held back, so the message shows starred. A star asked then is `Already`. At jaw's echo the later unstar stands, so the star is lost.
  - **Why it's reached:**
    - D1 and E1 need a merge between jaw's patch and its echo, and a change from the phone to that setting in the same window. An engine start's full fetch brings both: contacts first, then the settings, while jaw's patch waits for the fetch to finish.
    - F1 needs a group whose messages the phone names by LID while jaw names the sender another way.
  - **Fix:** keep, with each wait, the index jaw's patch named: its key as `app_state` keys it, in a column of `app_state_awaiting`.
    - The migration isn't merged yet, so its text can still change; update its line in `migrations.lock`, the seed in `kept.rs` and migration 25's test with it.
    - `merging` keeps the column as it is when it moves the wait.
    - `held_back` then holds back by order only what comes under that index.
    - `newest_held` counts jaw's patch in place of that index's raw value only, and weighs every other index by time, the kept chat's included.
  - **Tests:** D1 with D0 beside it, E1, and F1, each failing without the fix.

### Test gaps

- **T4. Nothing fails when a merge leaves a star's wait behind.**
  - **Where:** `merging` moves the waits of all five settings, a star's among them (`SETTINGS`, `chat_actions.rs:63`, and `chat_actions.rs:418`).
  - **What happens:** with stars left out of the move (switch `r3-move-no-star`), none of the 267 tests fails. A star's wait would then stay under the gone chat's JID. Since `wait_key` finds the kept chat for either JID, nothing would look there, and the phone's earlier unstar would be applied, which is P9's loss through a merge.
  - **Fix:** a test of jaw's star in her LID's chat, the chats merging, then the phone's earlier unstar, held back, failing with the switch.
- **T5. Nothing fails when a moved wait loses where its echo has to be.**
  - **Where:** `MOVED` takes the wait's `echo_by` with it (`chat_actions.rs:56-58`, `chat_actions.rs:431`).
  - **What happens:** with `echo_by` dropped as the wait moves (switch `r3-move-no-echo-by`), none of the 267 tests fails but a timing test, which passed alone twice. A wait whose patch WhatsApp had taken before the merge would then end at its setting's next mutation, its outbox row being gone (`chat_actions.rs:354-357`), and that mutation is weighed as if nothing of jaw's waited. If it's the phone's earlier change, it's applied, which is I3's loss again.
  - **Fix:** a test of a merge after `Step::Sent`, then the phone's earlier change under the kept chat's JID, held back, then the converter passing `echo_by` ending the wait, failing with the switch.

### Other

- **O2. On a tie, each full fetch ends with the value of whichever JID it hands over first.**
  - **Where:** `newest_held` lets what's held stand when its time is as late as the incoming value's (`*time >= at`, `chat_actions.rs:410`). So each of two tied values gives way to the other when it arrives.
  - **What happens (C1):** her number says unmuted and her LID says muted for ever, both at the same millisecond. Applied as they came, the chat ends unmuted. A full fetch that hands over her LID first ends muted, and one that hands over her number first ends unmuted.
  - **Reach:** two devices' changes to one setting of one chat stamped within the same millisecond, or two values with no time. A full fetch's order is the server's, and usually the same each time.
  - **Fix, cheap with I5:** break a tie by the key, comparing `(time, key)`, so the end is the same in any order. jaw's waiting change counts under the index it named.
- **X2 (main's code). Once jaw's echo is in, a merge still sets the merged chat by main's rule, and the next full fetch puts the newest back.**
  - **A5:** jaw archives Ana's chat and its echo comes. Her LID's chat, not archived, then merges in, and the chat shows unarchived. The next engine start's full fetch archives it again.
  - On main the same happens to the phone's own changes, and the next full fetch applies whichever value comes last. With O1 it applies the newest.
  - It's main's code, so it doesn't hold this verdict on its own. I4's better fix closes it.
- **O3. The design's caveats stand as the owner chose them.** Each device stamps its changes by its own clock, so across JIDs one whose clock runs ahead wins. And a full fetch can hand a snapshot's values over before the patches that follow. Neither is new in this round.

### Hygiene (follow-ups)

- **H10.** Migration 25's comments say a wait is keyed "as app_state keys it: serde_json of the index's strings", for each app-state index (`store/schema.rs:1210-1217`), and its test says "keyed as app_state is" (`store/schema.rs:2383`). Since I3, a wait is keyed by its chat as jaw keeps it, and a star's wait without its sender. The migration isn't merged, so its text can still say so, and I5's column changes it anyway.
- H9 stands, now with `feed::tests::a_watch_result_waiting_for_room_comes_before_anything_its_window_sends` on the landers' list. Round one's H1 to H3, H5 and H6 and P1 to P3 stand as follow-ups, as the plan's items 14 and 15 say.

## What was asked, checked

### Round two's fixes, each switched off

I applied the fix round's 27 switches and four of mine in the audit's clone, all behind `JAW_MUTANT` in one build. Each ran against the chat actions' writer tests, the converter's, migration 25's, `kept`'s and the writer's own (129 at the head, the probes among them, which assert nothing). A switch that failed none of them then ran against the rules', the merges' and sync's tests too (138 more). The counts are under Measurements, and they're the fix round's to the test.

- **I3, closed on every path round two found:**
  - **Pins, marks and stars (P3, P4, P9):** with `held_back` looking under the index's own text (`i3-held-back-text`), 11 tests fail: P3's, P4's, P9's, P11's, the merges' and O1's.
  - **A group star the phone names by its sender's LID:** with the wait kept under its patch's own text (`i3-queue-text`), or a star's wait keeping its sender (`i3-star-sender`), its test fails.
  - **A merge in the middle of a wait (P7, P7b, P8, and two waits for one setting):** with no move (`i3-no-move`), 4 tests fail. With the earlier or the kept chat's wait winning (`i3-move-earlier-wins`, `i3-move-kept-wins`), the two waits' test fails.
  - **History (P10):** with `unsaid` leaving out the person's JIDs (`i3-unsaid-no-person`), P10's test fails.
  - What the move doesn't reach is I4. What the wait doesn't keep is I5.
- **T2, closed** (`7f932b1b`): with `set_read` asking `marking` under the read's own JID (`t2-read-jid`, round two's `i1-read-jid`), P5's test fails.
- **T3, closed** (`45ed59e9`): with `unsaid` searching the blob's JIDs alone (`t3-blob-only`), P6's and P10's tests fail.
  - The narrower switch, the kept chat's JID left out (`t3-no-kept`, round two's `x1-no-kept`), fails none of the 267. That's because it's equivalent now, not because a test is missing.
  - The blob's JIDs always hold the chat's own JID, whether that's the conversation's ID or, for a LID, its phone number (`convert/history.rs:266-299`). `chat_jid` returns that JID itself, a chat it's merging into, or a one-to-one chat of the same person.
  - A one-to-one chat's own JID is in `person_jids` under its person: `upsert_chat` sets a chat's person only through `person_of` on its own JID, and a merge of people moves `person_jids` and `chats` together (`store/write.rs:1642-1644`). Nothing deletes from `person_jids`.
  - So the JID jaw keeps the chat by is always among the blob's JIDs or its person's, and leaving it out changes nothing that any test, or any jaw.db jawd writes, could see.
- **H8, closed** (`0558e2de`): with the test starting one version late (`h8-start-after`), migration 25's test fails. It also checks that `app_state_awaiting` isn't there before the migration.
- **Round one's I1 and X1** at the head: `i1-set-read` fails 4 tests, `i1-settle` 2, `i1-marking-always` 2, `x1-off` 5, `x1-no-app-state` 4, `x1-no-awaiting` 2 and `x1-kept-only` 2.
- **Mine:**
  - With an echo ending its wait only under the kept chat's JID (`r3-echo-kept-only`), the two waits' test fails.
  - With `act` committing at `NORMAL` (`r3-unsynced`, round one's T1), the synced-commit test fails.
  - Two fail nothing (T4, T5):
    - a merge that moves no star's wait (`r3-move-no-star`) fails none of the 267;
    - a moved wait that loses where its echo has to be (`r3-move-no-echo-by`) fails none but a timing test of sync's, which passed alone twice with the switch on.

### O1, the owner's `fix_blind`

- **The rule as built** (`3c233a3f`): what WhatsApp sends for a chat's setting applies only when it's newer, by the time it carries, than everything held for that setting under the chat's other JIDs. A group message's star counts its sender's JIDs too.
  - What's held is WhatsApp's latest under each JID, kept raw in `app_state`, and jaw's waiting patch at its time under the JID jaw keeps the chat by.
  - A tie isn't newer.
  - When the incoming value isn't the newest, the newest held is applied through the converter (`Held::Instead`). Under one JID, WhatsApp's order decides.
- **Each part is tested:** each of O1's nine switches fails its tests.
  - `o1-no-times`: 12, P11's in either order among them.
  - `o1-no-waiting-time`: 7.
  - `o1-order-across-jids` and `o1-time-within-index`: 2 each.
  - `o1-no-person`: 3.
  - `o1-waiting-counts-raw`, `o1-star-sender`, `o1-tie-applies` and `o1-no-instead`: 1 each.
- **In any order, repeated, across restarts:**
  - The rule keeps nothing but `app_state`, a raw table written in the transaction that converts each mutation, and `app_state_awaiting`, jaw's own. So a restart changes nothing it weighs, and a replay of a mutation writes the same row again and applies nothing new.
  - P11's test covers either order. G1 hands the same two values, with no tie, over six times in alternating order, then twice more with jaw's patch waiting between: it ends archived each time, then as jaw's later change has it.
  - Where the values held under a chat's JIDs differ by time, the end is the newest of them in any order, since each arrival applies the newest of what's held and itself. Under one JID, the last to come replaces what was held there, as WhatsApp's order says.
  - It can still flip on an exact tie (O2). In I5's two cases it shows a value that won't last until the echo.
- **The two narrow cases the plan leaves:** they can lose a request, as I5 shows. The merge case comes together at an engine start whose full fetch joins her JIDs while jaw's patch waits. The star case needs only the phone and jaw naming a group message's sender differently. Keeping the named index closes both.
- **Cost:** about 3 µs more for each settings mutation in a release build (Measurements). A live transaction of 64 of them takes about 0.2 ms more, well within the budgets.

### Durability and the rest of the standard

- **Nothing is acknowledged before a synced commit.** `act` still commits each request through `durably` before it answers, which `r3-unsynced` shows the synced-commit test pins.
  - The fix round's changes are all inside the writer's transactions: the converter's hold-back, `merging` in `start_merge`, and `unsaid`.
  - So each commits or rolls back with what caused it (the inbox row it drains, the person joined, the history blob), and a crash before the commit has it done again.
  - Nothing in `send/`, the engine or the acks changed this round.
- **Plans:** the four new statements are pinned in `plans.txt` and the chat actions' plan test:
  - `WAITING_UNDER` and `HELD_UNDER`, each a range of a primary key;
  - `MOVED`, an upsert;
  - `THEIR_JIDS`, an index search of `chats` and `person_jids_by_person`.
  
  Both plan tests pass at the head. Each settings mutation now makes four or five more searches than in round two, each of a primary key or an index: `chat_jid`'s one or two, `THEIR_JIDS`, and a range of `app_state` for each of the person's JIDs.
- **Schema:** no new table or column this round.
- **Migration 25** is round one's text, unchanged (`25 schema 77f19573…`, the hash round one audited as 19). It's a schema step of one table of jaw's own, after main's 21 to 24.
  - `kept.rs` is main's plus `app_state_awaiting`'s class and seed, and nothing else: `7b5fc48e` is dropped, as round two asked once places and events' L1 was on main.
  - `kept`'s test seeds version 24 and passes.
- **The fifth rebase** changed only context. `git range-diff` from round two's trial on `3878dd66` to the head finds 53 commits unchanged, 17 changed and 2 dropped (`7b5fc48e` and the trial's by-hand commit), and the fix round's 7 new.
  - File by file, the branch's change over main differs from the trial's over its base only where the fix round wrote, and by the number and `kept.rs`.
  - Elsewhere the only differences are the context where places and events wrote beside the branch: the fixtures' and `send/mod.rs`'s imports and module doc, and `demo.rs`'s module doc.

### Landing

The library lands first, as migration 25, a data step (`25 data d3e337e2…`). Then presence lands, with no migration. This branch then takes 26, the next free number. A trial three-way merge of the head with the library's branch (`77d44257`) and with presence's (`a637b061`), each on `e12d8d34`, shows what its lander meets.

- **The number:**
  - `migrations.lock` gets `26 schema …`, after the library's line, with the hash of the migration as I5's fix leaves it;
  - `schema.rs`'s comment becomes `// 26:`, after the library's migration;
  - the test becomes `migration_26_keeps_one_wait_an_index_for_one_outbox_row_each`, which finds its start by the migration's text, so only its name changes;
  - the plan's lines that say 25 change, as do the messages of the renumbering commit and H8's.
- **Conflicts:**
  - With the library: `migrations.lock`, `schema.rs`, `jaw-proto/tests/fixtures.rs` (imports) and `protocol/README.md`. Each is two additions at one place, so keep both sides.
  - With presence: `engine/demo.rs`, again keeping both sides.
  - `kept.rs`, `plans.txt`, `store/write.rs`, `store/mod.rs`, `sync/` and `send/tests.rs` merge by themselves.
  - The library's changes to `write.rs` (`remove_row`, `index`, `rehome`, `merge_step`, `fold`) don't touch `start_merge`, where `merging` runs. Presence doesn't change `sync::run`'s arguments, so the H4 test's call stands.
- **`kept.rs`:** the library's seven classes are all derived and seed nothing. So `kept`'s test, which will seed version 25 for this branch's 26 for the first time, needs no new seed. `app_state_awaiting` keeps its class and seed, with its new column's value once I5 adds one.
- **What the lander re-checks:**
  - the gate (`make check-generated lint build test`);
  - migration 26's test, `kept`'s test, the lock and number tests, the fingerprint test and both plan tests (`JAW_PLANS=write` leaves `plans.txt` as merged);
  - `tests/upgrading.rs` from version 1 through 26, with `--engine demo`;
  - the writer's tests with I3's, I4's, I5's and O1's switches;
  - the trailer on every data-layer commit, then `make check-audit`.

  A schema step after a data step (the library's 25) is nothing new: main has schema steps after each of its data steps 6, 11 and 19, and `migrate` runs each run of schema steps together and each data step alone, in order, before it returns.

## Measurements

All in a clone of the worktree under `/tmp/jaw-audits/chat-actions-4`, built with `CARGO_INCREMENTAL=0`, on this Mac (20 cores) at a load average of 52 to 70 from other agents' work.

- **jawd's tests at the head** (the test binary of `cargo test -p jawd --bin jawd`, 10 threads, `nice`): 963 passed, 0 failed and 15 ignored, in 1,727 s. No timing test failed.
- **The engine's tests at the head** (`go test -race -count=1 ./internal/wa/`, Go's cache in the audit folder): ok, 63.5 s.
- **Probes**, through the writer, as the converter and the writer take each change (`arrives`, `acted` and `written` in the chat actions' tests). Ana's chat is kept under her number, with her LID joined to her unless a probe says otherwise.
  - **A1:** archive `Queued`. Archived before the merge; unarchived after it. Unarchive asked then: `Already`. After the archive's echo: archived.
  - **A2:** the phone pinned both chats; jaw's unpin `Queued`. Unpinned before the merge; pinned at the LID chat's time after it. Pin asked then: `Already`. After the unpin's echo: unpinned.
  - **A3:** the phone marked her LID's chat; jaw's mark read `Queued`. Not marked before the merge; marked after it. Mark unread asked then: `Already`. After the echo: not marked.
  - **A4:** two hours under her LID at `AT`, one hour under her number at `AT + 1`. After the merge: two hours, with the one wait left at `AT + 1`. Two hours asked then: `Already`. After the earlier echo: two hours. After the later: one hour.
  - **A5:** archived after the echo; unarchived after the merge; archived after a full fetch.
  - **C1:** unmuted as they came; muted for ever after a fetch with her LID first; unmuted after one with her number first.
  - **D0:** her LID's chat alone, pinned from jaw; the phone's later unpin under her LID is held back: pinned.
  - **D1:** after the merge, pinned, the wait moved to her number. The phone's later unpin under her LID: unpinned. Unpin asked then: `Already`. After jaw's echo: pinned.
  - **E1:** the phone's later unpin under her number: pinned. Pin asked then: `Already`. After jaw's echo: unpinned.
  - **F1:** the phone's later unstar naming Rui by his LID: starred. Star asked then: `Already`. After jaw's echo: unstarred.
  - **G1:** archived after each of six alternating full fetches. Then not archived through two more and jaw's echo, with jaw's unarchive waiting between, and no wait left.
- **Switches.** Applied in the audit's clone, never the worktree, built once, and run one at a time with `JAW_MUTANT`. 129 tests pass with none.

  | Switch | What it changes | Tests failing |
  | --- | --- | --- |
  | `i3-held-back-text` | the hold-back looks under the index's own text | 11 |
  | `i3-queue-text` | a wait is kept under its patch's own text | 1: the group star |
  | `i3-no-move` | a merge moves no wait | 4: P7, P7b, P8, the two waits |
  | `i3-move-earlier-wins`, `i3-move-kept-wins` | the moved wait, or the kept chat's, always stays | 1 each: the two waits |
  | `i3-star-sender` | a star's wait keeps its sender | 1: the group star |
  | `i3-unsaid-no-person` | history leaves out the person's JIDs | 1: P10 |
  | `t3-blob-only` | history searches the blob's JIDs alone | 2: P6, P10 |
  | `t3-no-kept` | history leaves out the kept chat's JID only | none of 267 (equivalent) |
  | `t2-read-jid` | `set_read` asks under the read's JID | 1: P5 |
  | `h8-start-after` | the migration's test starts one version late | 1: migration 25's |
  | `i1-set-read`, `i1-settle`, `i1-marking-always` | round one's I1 rules out | 4, 2, 2 |
  | `x1-off`, `x1-no-app-state`, `x1-no-awaiting`, `x1-kept-only` | round one's X1 rules out | 5, 4, 2, 2 |
  | `o1-no-times` | applied as they come, as before O1 | 12, P11's among them |
  | `o1-no-waiting-time` | jaw's waiting patch isn't counted at its time | 7 |
  | `o1-order-across-jids` | held back by order under any JID, as before O1 | 2 |
  | `o1-time-within-index` | weighed by time under the named JID too | 2 |
  | `o1-waiting-counts-raw` | the raw value under the kept JID counts while jaw's patch waits | 1 |
  | `o1-star-sender` | a star's setting includes its sender | 1 |
  | `o1-no-person` | only the kept chat's JID is looked under | 3 |
  | `o1-tie-applies` | a tie applies | 1 |
  | `o1-no-instead` | nothing is applied where something held is newer | 1 |
  | `r3-echo-kept-only` | an echo ends its wait only under the kept JID | 1: the two waits |
  | `r3-move-no-star` | a merge moves no star's wait | none of 267 (T4) |
  | `r3-move-no-echo-by` | a moved wait loses its `echo_by` | none of 267 but a timing test, which passed alone 2 of 2 (T5) |
  | `r3-unsynced` | `act` commits at `NORMAL` | 1: the synced-commit test |

- **O1's cost.** Method: an in-memory jaw.db at the head's schema, with 4,000 people, each with a chat under their number and their LID joined, and a pin held under each LID. Then 4,000 unpins under the numbers, all newer, applied in one transaction as the drain applies them, timed around the writer's `apply`. Each case ran 8 times, alternating with and without O1 (`o1-no-times`).
  - Release build, µs a mutation: with O1, 17.9 to 32.1 (median 18.3); without, 13.6 to 72.1 (median 15.5).
  - With the numbers' unpins older, so the LIDs' pins stand: with O1, 13.0 to 15.7 (median 13.4, nothing written); without, 13.6 to 30.0 (median 14.4).
  - Debug build, 2,000 of each, 6 runs: medians 57 and 26 µs.
  - The drain applies at most 64 changes in a live transaction (`LIVE_CHANGES`), so O1 adds about 0.2 ms to a full one.

# Round two: `f414e65c`, round one's fixes, the third rebase and migration 21

Range: `9b3fbdf4..f414e65c` in `chat-actions` (71 commits on main's `9b3fbdf4`), 2026-10-31. Auditor: database-expert. I took over from round two's first auditor, which stopped at about 02:16 with this section begun; I ran its checks again rather than take them, and finished them. Main is now `3878dd66`: typing, and read receipts after a merge (the merge follow-ups), since `9b3fbdf4`.

Round one is kept as written below this round, and this round's verdict comes at the end of the note, after round one's.

## In short

- **Verdict: changes needed.** Round one's asks are done and tested, but this round finds one inconsistency the hold-back has had since task 8 (I3), and two rules of round one's fixes that no test pins (T2, T3).
- **Closed, each failing a test with its part switched off:** I1 on its three paths (a receipt, history, one of yours from another device), I2, T1, X1's two halves and each of its four settings, and H4. The previous auditor's switches all gave what it found, and I1's and X1's give the same on today's main.
- **I3, inconsistency:** one chat's setting is matched by the text of one of its JIDs.
  - What the phone sends under the person's other JID while jaw's patch waits isn't held back.
  - A merge during a wait leaves the wait under the gone chat's JID, where I1's rule doesn't look.
  - History's settings miss what app state said under another JID.

  A request judged against what then shows is lost on every device, as in I1. Probes P3, P4 and P7 to P10 show it, at the head and on today's main.
- **T2 and T3, test gaps:** `set_read` searching under the JID the read names, and X1 leaving out the JID jaw keeps the chat by, each fail nothing among jawd's 846 unit tests outside the schema's. Probes P5 and P6 are the tests they need.
- **The rebases** changed only context. Folders', the keepalive fix's and media in's code is unchanged.
- **Migration 21** is round one's 19, unchanged: a schema step of one table of jaw's own, seeded in `kept.rs`. It's still 21 on today's main, and becomes 25 after places and events. `7b5fc48e` is needed now, and is dropped once places and events' L1 is on main.
- **On today's main** the rebase isn't clean apart from renumbering: 10 commits conflict, and two more lines need a hand. Resolved, the trial builds; its 888 unit tests pass but three timing tests of main's, which pass alone, and so does the upgrade from version 1.
- **O1, other:** a full fetch applies a setting from each of the chat's JIDs in the order it comes, so where WhatsApp holds jaw's value under one JID and the phone's under another, the last wins (P11). Whether that happens depends on which JID the phone's app state names one-to-one chats by, which the owner can see in jaw.db.

## Findings

### Data loss

None.

### Inconsistency

- **I3. A setting of one chat is matched by the text of one of its JIDs. What WhatsApp sends for it under another isn't held back, a merge during a wait leaves the wait behind, and history misses what app state said under another JID. A request judged against what then shows is lost on every device, as in I1.**
  - **Where:**
    - `held_back` looks for a wait under the index exactly as WhatsApp sent it (`store/write.rs:715-721`, `store/write/chat_actions.rs:292-305`).
    - A wait is keyed by the index jaw's patch names (`queue`, `chat_actions.rs:263-274`), whose JID is the one jaw keeps the chat by (`CHAT_TO_ACT_ON`, `send/chat_actions.rs:38-39`).
    - I1's rule looks under that JID only: `marking` (`chat_actions.rs:283-285`), from `set_read` (`write.rs:1033`) and `settle_each` (`write.rs:932-937`).
    - A merge keeps a person's chat under their phone number (`merge_direct_chats`, `write.rs:1536-1553`) and moves the gone chat's settings into it (`start_merge`, `write.rs:1600-1608`), but its waits stay under the gone chat's JID.
    - History's `unsaid` searches the blob's JIDs and the kept chat's (`write.rs:1386-1421`), not the person's others.
  - **Why it's reached:** one chat's setting comes to jaw under any of its person's JIDs.
    - App state names a one-to-one chat by her LID or by her number. The branch's own X1 test has it so (`history_leaves_a_setting_app_state_said_under_another_of_the_jids_…`), the sync audits found it ("the number unmarked through app state, by either JID"), and WhatsApp's app state has an action giving a LID chat its phone number (`pnForLidChat` in whatsmeow).
    - jaw keeps the chat under her number, so its patches name her number while the phone's changes may name her LID.
    - After a merge, the gone chat's echo comes under the JID jaw sent it with.
  - **What happens** (probes P3, P4 and P7 to P10, under Measurements):
    - The phone's change to the same setting, from before jaw's patch reached WhatsApp, names her LID while jaw's patch names her number. It isn't held back, so the chat shows the phone's value, which won't last: WhatsApp took jaw's after it.
    - A request asked then is judged against that value. jaw's unpin, mark read or unstar is `Already` and queues nothing. The echo then sets jaw's earlier value again, here and on every device (P3, P4, P9).
    - After a merge during a wait, the same happens with the JIDs the other way round (P8). I1's rule looks under the kept chat's JID and misses the wait, so a read on the phone (P7) or one of yours from another device (P7b) unmarks the chat, and the mark read asked then is lost: I1 again, through a merge.
    - History archives a chat app state brought back under her LID, when the blob names her number alone (P10): X1 again, under another JID.
    - The window is jaw's patch's flight. While WhatsApp can be reached it's under a second. While it can't, or behind an engine start's full fetch, which hands over every setting the phone holds (round one's P2), it lasts as long as that does.
  - **Fix:** match a wait by its chat and setting, not by its JID's text. For instance:
    - keep keying waits by the JID jaw keeps the chat by, and resolve the JID of what WhatsApp sends with `chat_jid` before `held_back` looks;
    - when a merge starts, move the gone chat's waits to the kept chat's JID, the later of two by its time, so `marking` needs no change;
    - have `unsaid` search under every JID of the chat's person as well as the blob's.

    Or search, in `held_back`, `marking` and `unsaid`, the index under each JID of the chat (its person's, for a one-to-one chat): a few primary-key searches.
  - **Tests:** P3, P4, P7, P7b, P8, P9 and P10's sequences, through the writer, each failing without the fix.

### Test gaps

- **T2. Nothing fails when I1's `set_read` rule searches under the JID the read names rather than the chat jaw keeps.**
  - **Where:** `set_read` asks `marking(db, &jid)`, `jid` being `chat_jid`'s (`store/write.rs:1033`).
  - **What happens:** with `marking(db, &user_jid(&read.chat))` instead (mutant `i1-read-jid`), jawd's 846 unit tests outside the schema's give what they give at the head: only timing tests fail. P5 shows what it breaks: a read-self receipt naming her LID unmarks the chat while jaw's mark unread under her number waits, and a mark read is then `Already`.
  - **Fix:** P5 as a test, failing with that mutant.
- **T3. Nothing fails when X1 leaves out the JID jaw keeps the chat by.**
  - **Where:** `unsaid`'s `named` adds `kept` to the blob's JIDs (`store/write.rs:1394`).
  - **What happens:** without it (mutant `x1-no-kept`), the same 846 tests give what they give at the head. P6 shows what it breaks: a blob naming her conversation by her LID alone pins the chat again after app state unpinned it by her number.
  - **Fix:** P6 as a test, failing with that mutant. I3's fix, searching the person's JIDs, can take both tests as its own.

### Other

- **O1. A full fetch applies a chat's setting from each of its JIDs' values in the order they come, so where WhatsApp holds two, the last wins, whatever their times** (main's converter, which this branch can now reach).
  - **Where:** `convert/app_state.rs:34-72` turns each mutation into the chat's value, and `store/write.rs:715-726` applies it unless it's held back; nothing compares the times of values under different JIDs.
  - **What happens (P11):** a full fetch hands over jaw's unpin under her number, then the phone's older pin under her LID, and the chat ends pinned at the older time.
  - **Why this branch reaches it:** if the phone's app state names her chat by her LID, each patch of jaw's, naming her number, adds a second value for that setting beside the phone's, rather than replacing it. Each engine start's full fetch then sets the chat by whichever comes last. That would be a lasting inconsistency with the phone at every start, for every chat jaw changed.
  - **What decides it:** which JIDs the phone's app state uses for one-to-one chats. The owner can see it in jaw.db: the second element of `app_state`'s keys for `pin_v1`, `archive`, `mute` and `markChatAsRead`, by `@lid` and `@s.whatsapp.net`. I didn't open the owner's data.
  - **Fix, if they're LIDs:** have each patch name the JID WhatsApp already keeps that chat's setting under (its row in `app_state`, else the chat's own), so jaw's value replaces the phone's. Or have the converter apply a chat's setting only when its time is later than the last applied under any of the chat's JIDs.
- **L1. The rebase onto today's main isn't clean apart from renumbering** (for whoever lands it; the numbering itself needs nothing today). What it takes is under "On today's main", below.

### Hygiene (other; follow-ups)

- **H8.** Migration 21's test starts from a hard-coded version, `at(None, 20)` (`store/schema.rs:2305`), so each renumbering moves it. Places and events' tests find the version before theirs by the migration's text; this one could find `CREATE TABLE app_state_awaiting` the same way.
- **H9.** Four of main's tests failed by their timing under a load of 50 to 70, passed alone, and aren't on the gate's list:
  - `feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`;
  - `feed::tests::no_window_starves_while_the_others_keep_falling_due`;
  - `send::tests::a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left`;
  - `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean`, which also failed 1 of 3 alone.

  None is the branch's.
- Round one's H1 to H3, H5, H6 and P1 to P3 stand as follow-ups, as the plan's item 14 says.

## What was asked, checked

### Round one's fixes, each switched off

Each fix was taken out in the audit's clone, jawd's test binary built again with `CARGO_INCREMENTAL=0`, the tests named run, and the clone put back (the mutants, under Measurements).

- **I1, closed on its three paths, each tested both ways,** at the head and on today's main.
  - `set_read`'s rule (`bfa6b4de`, `store/write.rs:1033`), for a receipt and for history: taken out, the receipt test and the history test fail.
  - `settle`'s rule (`cf27b3c9`, `store/write.rs:932-937`), for one of yours from another device: taken out, the test of your answer from the phone fails.
  - `marking` forced true: the receipt test and your-answer test fail at their last step, where nothing of jaw's waits and the read must take the mark off.
  - History's path needs both halves: without `set_read`'s rule, or without X1 counting a change of jaw's on its way, its test fails.
  - What I1's rule doesn't reach is I3's merge, and what isn't pinned is T2.
- **I2, closed** (`19312a69`). The engine's tests pass at the head. `appStateCode` reads the code from both of whatsmeow's wordings (`appstate.go:603-605`). `TestASendThatMightWorkLaterIsTriedAgainAndOneThatCantIsNot` fails:
  - with the `ErrAppStateUpdate` case taken out, on the five refusals (400, 403, 404 and 405 in `regular_low`, 400 in `regular_high`), each tried again;
  - with a 409 given up on, on the two 409s;
  - with every app-state refusal given up on, on the seven to try again (both 409s, 408, 429, 500, 503, and a refusal quoting no error node).
- **T1, closed** (`d8c41a10`): with `act` committing through `store.apply(Lane::Live, …)`, `each_sending_call_that_keeps_something_is_one_synced_commit` fails at `chat.pin`.
- **X1, closed for the JIDs the blob gives and the kept chat's** (`49de1483`); the person's other JIDs are I3's, and the kept chat's isn't pinned (T3).
  - Switched off whole, the two X1 tests and I1's history test fail, at the head and on today's main.
  - Its two halves: with `SAID` reading only `app_state_awaiting`, the first X1 test and the LID test fail; with it reading only `app_state`, the first X1 test (at the mute) and I1's history test fail.
  - Its four settings, each check taken out alone: the pin's and the mute's fail the first X1 test, the archive's the LID test, and the mark's I1's history test.
  - Searching only the JID jaw keeps the chat by fails the LID test.
  - `Change::HistoryChat` goes everywhere `Change::Chat` from history went: the writer applies it, quarantine names it, and `touches_fts` leaves both alone.
- **H4, closed** (`af56fdd4`): with `resync_again` trying whether or not WhatsApp can be reached, or giving up while it can't, `a_full_app_state_refetch_that_fails_spends_no_try_while_whatsapp_cant_be_reached` fails. Each try still ends when the engine restarts.
- **H7, done** (`fd5c32af`, docs); H9 names four more.

### The rebases

- **Round one's 60 commits:** `git range-diff 7262a15..0f4a1eb 9b3fbdf4..d41ffc33` finds 48 unchanged. The other 12 differ only by keeping both sides where main had written beside them, by the migration's number, and by `app_state_awaiting`'s class and seed in `kept.rs`. Main's writing included the fixtures' arm, `pub mod`s and re-exports, `demo.rs`'s `World` and arms, the README's `status` row, `engine.go`'s struct, and `sync::run`'s and the harnesses' arguments.
- **`sync::run`'s arguments:** presence's `Following` sits where main put it, and the H4 test passes it. The app-state turn is the supervisor's, as round one found it, with `Sending::start` back to main's arguments.
- **`engine.go`:** the struct keeps main's keepalive `told` beside the branch's `echoes`, and `keep` notes an echo only after `durable` returns.
- **The fixtures:** the arm for requests answered with nothing holds main's `folder.add`, `favourite.set`, `media.fetch` and `media.cancel` beside the five chat actions.
- **`plans.txt`:** both plan tests pass at the head and on the trial.
- **Folders, the keepalive fix and media in:** the branch changes none of their files (`store/write/folders.rs`, `store/write/copies.rs`, `media/`, `keepalive.go`, `feed/`, `store/read.rs`). Where it shares a file, what it takes out is its own: the demo's app-state store, `connected` moving from send's presence to `sync::reachable` with the same rule, and the enqueue lock carrying the last chat action's time.

### The migration, as 21

- **Kind:** a schema step, in its comment (`store/schema.rs:1146-1148`) and its lock line (`21 schema 77f19573…`), the hash round one audited as 19, so its text is unchanged. It's two `CREATE`s on a new table, which cost the same however big jaw.db is.
- **Class:** jaw's own, in its comment and in `kept.rs` (`("app_state_awaiting", Own, &[])`). Its seed row (`["mute", Ana]`, a time, outbox 1, `echo_by` 7) has every later migration checked to keep it. Its partial index says it's derived from the table.
- **Its number on today's main** is still 21: main's last is media in's 20.
- **Renumbering after places and events,** which lands as 21 to 24: this becomes 25.
  - `migrations.lock`: `25 schema 77f19573545653f3c9e4a4c538cb44b54201b62522e1249f358e1d39000a6d80`, after places' four lines, the hash unchanged;
  - `schema.rs`: the comment `// 25:` on the migration after places' four;
  - its test: `migration_25_keeps_one_wait_an_index_for_one_outbox_row_each`, starting at `at(None, 24)` (or found by its text, H8);
  - the plan's lines 32, 3726 and 3738: migration 25, with main's 13 to 24 before it.

### `7b5fc48e`, `kept.rs`'s seeds for version 20

- **It's needed on today's main, and it's tests only.** The test seeds the version before each migration, so the branch's 21 makes it seed version 20 for the first time. Each part fails `kept`'s test without it:
  - `kept.rs` as before it: "a row of media at version 20 wasn't written … `length(sha256) = 32`";
  - without `SEEDS_SINCE`'s values for media: the same;
  - without `MADE`: "a row of media_made at version 20 wasn't written … UNIQUE constraint failed".

  With it, every migration from 2 to 21 keeps each row of jaw's own and raw, and the test still sees migration 20 take a copy that said "here" to `remote`.
- **Once places and events is on main,** its L1 has made the same fix in its places commit, its own way: media's seed row valid from 20 on, a seed for each of media's three tables, and `seed()` upserting every row. Then:
  - `7b5fc48e` conflicts with it (both add `media_files`, `media_made` and `media_drop` seeds, and both change `seed()`'s insert), and is redundant: drop it in that rebase and take main's `kept.rs`;
  - `app_state_awaiting`'s class and seed stay, in the migration's commit, beside places' tables;
  - `kept`'s test then seeds version 24 for the branch's 25, and must pass.
- **If chat actions lands first,** places' lander takes one of the two seedings, not both: with L1's upsert, a second seed row for the same file would be merged silently, and the test would no longer see a collision.

### On today's main

A trial rebase of the 71 commits onto `3878dd66`, in the audit's clone, built and tested.

- **The rebase isn't clean apart from renumbering** (L1). The numbering itself needs nothing.
- **10 commits stop, with 41 conflict blocks:**
  - nine where typing (M6) wrote beside the branch: `engine/demo.rs` in commits 12, 13, 32 and 36; `send/tests.rs` in 16 and 18; the README's `status` row in 18 and 49; and `sync::run`'s typing argument beside the branch's turn in 22 and 50, in `main.rs`, the send bench and the five harnesses;
  - each of those is resolved by keeping both sides, but 50, which moves the turn into the supervisor: it takes typing's form less the turn, with `sync::run`'s doc as main has it;
  - and `bfa6b4de`, I1's `set_read`, against the merge follow-ups' rewrite of `set_read`: main's doc with I1's clause, and I1's line, `now.marked = now.marked && chat_actions::marking(db, &jid)?;`, in place of main's `now.marked = false;`, before main's `if speaks`.
- **Two more need a hand, which no conflict shows:**
  - the H4 test's `sync::run` call takes typing's `crate::feed::Typing::default()`;
  - `send/tests.rs`'s imports keep only the branch's last (`{ContactAction, PinAction, SyncActionValue}`), where the conflict at 16 leaves an earlier `#[expect(unused_imports)]` one beside it.
- **What the trial carries:** its diff over today's main is the branch's over `9b3fbdf4` in 46 of its 48 files, line for line, and in the other two but for the lines where both wrote. `git range-diff` finds 59 commits unchanged, the ten above, and two whose context moved: 43, where main's hello now names `jaw_proto::PROTOCOL`, and 68, `settle`'s I1 path beside main's new `mine_by_another`.
- **The tests:**
  - jawd's unit tests: 888 passed, 3 failed and 14 ignored, in 1,259 s, at a load of 65 to 70. The three are timing tests of main's: feed's and sock's as at the head, and `send::tests::a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left` ("still waiting", with 400 ms to wait), which passed alone 3 of 3 on the trial and on the head. The plan tests, `kept`'s, the schema's and the chat actions' all pass.
  - `tests/upgrading.rs` (the owner's first run upgrading a version-1 jaw.db, `--engine demo`): ok.
  - I1's three switches and X1's, against the 338 tests of the writer, the converter, sync and the merges: each fails the same tests as at the head.
  - The probes give what they give at the head.

## Measurements

All in a clone of the worktree under `/tmp/jaw-audits/chat-actions-3`, built with `CARGO_INCREMENTAL=0`, on this Mac (20 cores) at a load average of 20 to 70 from other agents' work.

- **jawd's tests at the head** (`cargo test -p jawd --bin jawd`): 847 passed, 3 failed and 14 ignored, in 1,257 s. The three, `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce`, `feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` and `sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`, are timing tests outside the range, and passed alone twice (8.1 s and 9.3 s).
- **The engine's tests at the head** (`go test -race -count=1 ./internal/wa/`, Go's cache in the audit folder): ok, 27.3 s.
- **Probes**, through the writer, as the converter and the writer take each change (`arrives` and `written` in the chat actions' tests), at the head and on today's main alike. Ana's chat is kept under her number, and her LID is joined to her, unless a probe says otherwise:
  - **P3:** jaw pins the chat. The phone's unpin from before jaw's pin reached WhatsApp names her LID, and is applied: unpinned. jaw's unpin, asked then, is `Already`. The pin's echo pins the chat again, with one patch queued and no wait left: the unpin is lost on every device.
  - **P4:** jaw marks the chat unread, two messages unread. The phone's read from before it names her LID, and is applied: not marked, nothing unread. jaw's mark read is `Already`. The mark unread's echo marks the chat again: the mark read is lost, as in I1.
  - **P5:** a read-self receipt naming her LID, while jaw's mark unread waits, reads the chat and leaves the mark, and a mark read is then queued. I1's `set_read` rule finds the chat through `chat_jid`, so this path is right, though nothing pins it (T2).
  - **P6:** a blob naming her conversation by her LID alone, after app state unpinned the chat by her number, leaves it unpinned, through X1's search of the JID jaw keeps the chat by, which nothing pins (T3).
  - **P7:** jaw marks her LID's chat unread while it's a chat of its own. Her LID and number are joined, so it merges into her number's, which takes the mark. The phone's receipt then reads the chat by her number: not marked, nothing unread. jaw's mark read is `Already`. The mark unread's echo, under her LID as jaw sent it, marks the chat again: lost.
  - **P7b:** the same, with your answer to Ana from your phone after the merge in place of the receipt: not marked, the mark read `Already`, and the echo marks the chat again.
  - **P8:** the same with a pin: after the merge, the phone's unpin names her number and is applied, jaw's unpin is `Already`, and the pin's echo, under her LID, pins the chat again.
  - **P9:** jaw stars her message; the phone's unstar names her LID and is applied; jaw's unstar is `Already`; the star's echo stars it again.
  - **P10:** app state brings the chat back by her LID, and a blob then names her conversation by her number alone, archived: the chat is archived.
  - **P11:** a full fetch hands over jaw's unpin under her number and the phone's older pin under her LID, in that order: the chat ends pinned at the older time.
- **Kept for the builder and the lander,** in the agent store's `reports/`: the probes' code (`chat-actions-audit-r2-probes.rs`, for the end of `store/write/chat_actions.rs`'s tests), the mutants (`chat-actions-audit-r2-mutants.py`) and the trial (`chat-actions-audit-r2-trial.bundle`, `3878dd66..10b6903a`, its last commit the two lines by hand).
- **Mutants.** Each was applied in the clone, built, run against the tests named, and put back. The writer tests are the 189 of `store::write::chat_actions`, `store::rules`, `convert::`, `sync::tests` and the synced-commit test, the probes among them, which assert nothing.

  | Mutant | What it changes | Tests failing |
  | --- | --- | --- |
  | `i1-set-read` | `set_read` unmarks whatever waits | 2 of the writer tests: the receipt test, I1's history test |
  | `i1-settle` | `settle` unmarks for one of yours whatever waits | 1: your answer from the phone |
  | `i1-marking-always` | `marking` is always true | 2: the receipt test, your-answer test |
  | `i1-read-jid` | `set_read` searches under the read's own JID | none of 846 but timing tests (T2) |
  | `x1-off` | history's settings over app state's | 3: both X1 tests, I1's history test |
  | `x1-no-app-state` | `SAID` reads only `app_state_awaiting` | 2: the first X1 test, the LID test |
  | `x1-no-awaiting` | `SAID` reads only `app_state` | 2: the first X1 test, I1's history test |
  | `x1-kept-only` | only the kept chat's JID searched | 1: the LID test |
  | `x1-no-kept` | the kept chat's JID left out | none of 846 but timing tests (T3) |
  | `x1-no-pin`, `x1-no-mute` | that setting's check out | 1 each: the first X1 test |
  | `x1-no-archive` | the archive's check out | 1: the LID test |
  | `x1-no-mark` | the mark's check out | 1: I1's history test |
  | `t1-unsynced` | `act` commits at `NORMAL` | 1: the synced-commit test, at `chat.pin` |
  | `h4-no-wait` | the refetch tries while out of reach | 1: the H4 test |
  | `h4-give-up` | the refetch gives up while out of reach | 1: the H4 test |
  | `kept-main` | `kept.rs` as before `7b5fc48e` | 1: `kept`'s test |
  | `kept-no-since` | no `SEEDS_SINCE` values for media | 1: `kept`'s test |
  | `kept-no-made` | no `MADE` | 1: `kept`'s test |
  | `i2-off` | `ErrAppStateUpdate` falls through to try again | the engine's test, 5 cases |
  | `i2-409-refused` | a 409 is a refusal | the engine's test, 2 cases |
  | `i2-all-refused` | every app-state refusal is one | the engine's test, 7 cases |

  The 846 are jawd's unit tests but the schema's 18, run with the probes. On today's main, `i1-set-read`, `i1-settle`, `i1-marking-always` and `x1-off`, run against 338 tests (all of `store::write`, `store::rules`, `convert::`, `sync::tests`, `sync::merge::tests` and the synced-commit test), fail the same tests as at the head and nothing else. Timing tests also failed beside some mutants and are left out of the counts: `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean` ("within jawd's bound to stop", and 1 of 3 alone on the head's binary), and those of H9.

# Round one: `0f4a1eb`, kept as written

# Audit: chat actions from jaw

Range: `7262a15..0f4a1eb` in `chat-actions` (`origin/main..HEAD`, 60 commits on main's `7262a15`), 2026-10-29. Auditor: database-expert.

# Round one: `0f4a1eb`

## In short

- **Verdict: changes needed.** Two inconsistencies in the range (I1, I2) and one rule with no test that fails without it (T1). One inconsistency in main's code (X1) is now reachable from jaw's own requests, and I recommend fixing it here too.
- **What holds.** Each request commits its row, its new `v` and event, its outbox rows and its wait in one transaction through `durably` before it answers. "Already so" writes nothing. The pin limit counts through `chats_by_sort` with merges counted once. The echo finds each row as jawd wrote it, and the hold-back's four exits are right. A refusal is quarantined in the store, put back only while its patch is its index's latest, and fetched again under the turn. Migration 19 is a frozen, unique schema step of one jaw-own table. Every new statement's plan is pinned.
- **Thirteen of fourteen mutants are caught** by a test that fails. The fourteenth is T1: the five requests answering after a commit at `NORMAL` pass every test.
- **I1:** while jaw's mark unread waits for its echo, a read on another of your devices unmarks the chat outside app state. A mark read asked then is answered as so already, and is lost on every device once the echo marks the chat again. A probe shows it.
- **I2:** jaw-engine takes every server refusal of an app-state patch as one to try again (400 included). A refused patch is sent again for about 25 hours of connected time before it's given up, and meanwhile its chat's later messages wait behind it and the phone's changes to that setting are held back. The demo engine's refusals don't do this, so the refusal tests don't show it.
- **X1 (main's code):** history's conversation settings overwrite newer app state. A blob converted after jaw unpins, unarchives, unmutes or marks read a chat puts the phone's link-time value back for good, until the next engine start's full fetch. A probe shows it.

## Findings

### Data loss

None.

### Inconsistency

- **I1. A read on another of your devices unmarks a chat while jaw's mark unread waits for its echo, so a mark read asked then is so already, and is lost on every device.**
  - **Where:** `set_read` unmarks a chat on any WhatsApp-side read that moves its position forward (`crates/jawd/src/store/write.rs:974`). That is reached from a read-self receipt (`crates/jawd/src/sync/drain.rs:145`) and from history's read positions, neither of which goes through `held_back` (`crates/jawd/src/store/write/chat_actions.rs:282`). The mark read's "already so" is `!chat.marked && chat.unread == 0` (`chat_actions.rs:237`).
  - **What happens (probe P1, below):**
    - jaw marks Ana's chat unread with two messages unread, and the patch is queued;
    - the phone reads the chat before jaw's patch reaches WhatsApp, and its read-self receipt reads the chat here and unmarks it (marked false, unread 0);
    - jaw's mark read, asked now, is `Acted::Already`, with nothing queued;
    - the mark unread's echo then marks the chat again.
    WhatsApp took jaw's mark unread last, so every device ends marked unread, and the mark read is lost on every device. That is the outcome the hold-back was added to prevent ("What WhatsApp sends for an index while jaw's own patch for it is on its way is held back", in the plan's decisions). Here it comes through a path that isn't app state. Until the echo, the row also shows read and unmarked, a state that won't last.
  - **The window:** from the receipt to the echo. While WhatsApp can be reached, that's under a second. At a reconnection, the patch waits for its turn behind the engine start's full fetch, for seconds, while the offline receipts arrive.
  - **Fix, either of:**
    - (preferred) `set_read` leaves the mark as it is while its chat's `markChatAsRead` index waits for a patch of jaw's (one search of `app_state_awaiting`'s primary key); the read position still moves. The row then shows what will last, as the hold-back intends.
    - The writer doesn't take a mark read as so already while that index waits. The plan already accepts a second patch in that case for a chat jaw holds none of the messages of.
  - **Test:** the probe's sequence, through the writer, failing without the fix.

- **I2. jaw-engine never refuses an app-state patch for good. A patch WhatsApp refuses is sent again for about 25 hours, its chat's later sends wait behind it, and what WhatsApp sends for its setting is held back meanwhile.**
  - **Where:** `retryable` (`engine/internal/wa/send.go:130-168`) has no case for whatsmeow's `ErrAppStateUpdate`, which `sendAppState` returns when WhatsApp's server refuses a patch, with the server's error node and its code (`go.mau.fi/whatsmeow@v0.0.0-20261015105041-df33a485cc46/appstate.go:603-605`). So it falls through to `return true` (`send.go:167`). `attempt` then gives an app-state patch up only once `row.attempts + 1 >= ATTEMPTS_MOST`, as one that ran out of tries (`crates/jawd/src/send/outbox.rs:502-507`).
  - **What happens:**
    - A scratch test of the engine finds `retryable` true for such a refusal with code 400, 403, 404, 405, 409 and 500 alike (Measurements).
    - With jaw-engine, a patch WhatsApp refuses is therefore sent again 1,500 times. At `retry_most`'s 60 seconds (`crates/jawd/src/send/mod.rs:80-81`) that's about 25 hours of connected time. It's then given up as one that ran out of tries, with the notice "jaw couldn't confirm … with WhatsApp" rather than "WhatsApp refused …".
    - Meanwhile its chat's later sends, messages included, wait behind it, since a job waiting to try again stays ahead of its chat's later jobs (`crates/jawd/src/work/serial.rs:1-5`). That breaks "a change that fails is quarantined, never dropped, and never blocks the changes after it", which the design's refusal path promises for app state.
    - Its index's wait holds back what WhatsApp sends for that setting (`held_back`), so for those hours jaw shows the refused value against what the phone shows.
    - The demo engine refuses with `retry: false` (`crates/jawd/src/engine/demo.rs:928`). So every refusal test (quarantine, put-back, fetch again, notice) runs a path jaw-engine reaches only after 1,500 tries.
    - Main's deletion-for-you patches go the same way.
  - **Fix:** classify `ErrAppStateUpdate` by its error node's code with `transient`. A 4xx other than 408 and 429 is a refusal. A 409 stays one to try again, since whatsmeow has already applied the conflicting changes and sent once more before it gives up. Add the cases to `TestASendThatMightWorkLaterIsTriedAgainAndOneThatCantIsNot` (`engine/internal/wa/send_test.go:71`).

### Tests (missing)

- **T1. Nothing fails when the five chat actions answer after a commit at `NORMAL`.**
  - **Where:** `act` commits each request through `durably` (`crates/jawd/src/send/chat_actions.rs:444`), which is right, but no test pins it.
  - **What happens:**
    - With `act` making its change through `store.apply(Lane::Live, …)` instead, all 84 tests of the chat actions pass, the two crash scenes included (mutant `unsynced`, below).
    - The crash scenes kill jawd with SIGKILL, which loses nothing the operating system already has, so they can't tell a synced commit from one that isn't.
    - Main's `each_sending_call_that_keeps_something_is_one_synced_commit` (`crates/jawd/src/send/tests.rs:1251`) counts the synced commits of every other sending call; the five chat actions aren't in it.
  - **Fix:** add `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star` to it, each one synced commit.

### Unpredictable performance (follow-ups; none breaks a budget)

- **P1. Every patch waiting for the app-state turn tries again each time the turn frees, so n waiting make about n²/2 attempts** (the whole-branch review's point, measured).
  - **Where:** a patch that finds the turn taken waits for `free()` and runs its whole attempt again (`crates/jawd/src/send/outbox.rs:473-474`). Each attempt reads its outbox row on a reader at interactive priority.
  - **Measured:** 26, 51, 101 and 201 patches in as many chats, held behind one at the demo engine's gate. Once it opened they made 326, 1,323, 5,388 and 21,646 attempts, and the 201 patches drained in 1.16 s.
  - **Bound:** with real WhatsApp each patch holds the turn for its round trip, so the attempts spread out. They're bounded by the outbox (1,000 rows, about 500,000 attempts).
  - **Fix:** wake one waiter at a time, from a queue of the rows waiting for the turn.
- **P2. At an engine start, a chat's sends queued after one of its chat actions wait for the start's full fetch** (the plan's own; 951 mutations took 6.4 to 11.4 s on the demo account, and a real account holds more). A patch waiting for the turn could leave its place to its chat's later sends, whose order against it doesn't matter.
- **P3. A burst of patches given up on queues as many full fetches of their collection** (item 11), each holding the turn. They're bounded by the outbox and `FETCH_TRIES`, and nothing is lost. One fetch for each collection would do.

### Hygiene (follow-ups)

- **H1.** The fetches again after a patch given up on (`fetch_again`, `crates/jawd/src/send/chat_actions.rs:398`) are a queue that `jawd status` doesn't count. The rule is that every queue is bounded and reported; this one is bounded only. Main's `resync_again` is the same.
- **H2.** A refused patch's put-back goes to how a chat is when nothing set it (`undo`, `crates/jawd/src/store/write/chat_actions.rs:350`). `app_state`'s latest raw value for that index is what WhatsApp last sent for it, often the phone's value held back meanwhile. Applying it, with the default only when there's none, would be right at once in most cases, and the fetch would then only correct staleness.
- **H3.** A patch that doesn't decode is quarantined and fetches every collection, but puts nothing back (item 11). Its wait's key in `app_state_awaiting` names the index, which `undo_of` could take instead of the undecodable patch. Only corruption gets here.
- **H4.** `resync_again` spends its ten tries whether or not WhatsApp can be reached (`crates/jawd/src/sync/refresh.rs:69`, main's). About nine minutes out of reach after an engine start, and the start's full fetch waits for the next engine. This branch leans on that fetch to heal what keeping WhatsApp's order can't, and to cover a crash between a refusal's commit and its fetch. It should wait for the connection before each try, as `fetch_again` now does.
- **H5.** `Step::Refused`, `REFUSED` and `chat_actions::refused` also cover patches that ran out of tries or don't decode (item 13).
- **H6.** A chat action's time is later than the last only within one run: `next(last, now)` starts from 0 (`crates/jawd/src/send/mod.rs:170`, `send/chat_actions.rs:469`). After a restart with the clock gone back, a patch can carry, to the millisecond, the time its index still waits for. The writer then refuses it, and the request fails as an internal error. Starting `last` from `max(at)` in `app_state_awaiting` would close it.
- **H7.** In my full run under a load average near 62, two of main's tests failed by their timing and passed alone (Measurements). They aren't on the gate's list of tests known to: `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push` and `the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it`. Neither file is in this range.

### Outside this range (main's code)

- **X1, inconsistency. History's chat settings overwrite newer app state, which now undoes jaw's own chat actions.**
  - **Where:** `convert/history.rs:286-298` gives each conversation's chat its `pinned`, `archived`, `muted_until` and `marked_unread` as the phone had them when it built the blob. `upsert_chat` sets them over whatever the row holds (`crates/jawd/src/store/write.rs:1240-1241`).
  - **What happens (probe P2, below):**
    - Ana's chat is pinned by app state at link time;
    - jaw unpins it, and its echo comes: the chat is unpinned, and `app_state`'s latest for the index is unpinned;
    - a history blob the phone built at link time is converted afterwards, through the real converter: the chat is pinned again, and archived as the blob had it, while `app_state`'s latest still says unpinned, as WhatsApp has it.
    It lasts until that setting next changes or the next engine start's full fetch, and the run that linked the account skips that fetch (`crates/jawd/src/sync/mod.rs:416`). A newly linked account's history comes over minutes to hours, the time someone is likeliest to set their chats up from jaw, so this branch makes jaw undo its own requests. On main, the phone's changes in that time are undone the same way.
  - **Fix:** history fills in a chat's setting only where app state has said nothing for it: no row in `app_state` for its index under the conversation's JIDs, and none in `app_state_awaiting`. That's a primary-key search per conversation. Test it with the probe's sequence. It isn't this branch's code, so it doesn't hold this verdict on its own, but I'd fix it here, where it now undoes jaw's own changes.

### The edges the reviews left (item 11), weighed

- **A revoke between a star's check and its write:** closed by `0f4a1eb`. The star's step reads `SHOWN_DELETED` for the message it found, and the mutant `no-star-step-check` fails two tests. A reaction was already checked in main's step.
- **A no-message mark read in the instant a merge into its chat starts:** the gone chat's moved messages are read by the echo and their receipts never go. These are receipts owed to their senders, not jaw's data, in a window of one merge step. Follow-up.
- **Receipts a merge moves wait for the next `want` or start:** as before this branch. Stands.
- **A message reaching jaw between a no-message mark read and its echo, at or before the patch's second,** gets no receipt from jaw; whether the phone sends one is WhatsApp's behaviour. Stands.
- **A burst of patches given up on, each fetching again:** P3.
- **A patch that doesn't decode puts nothing back:** H3.
- **`resync_again` spending its tries out of reach:** H4.
- **A refused archive of a pinned chat still sends its unpin:** the chat ends unarchived and unpinned here and on every device, which is consistent, and the protocol says so. Stands.

### What keeping WhatsApp's order leaves (item 10), weighed

The owner chose on 29 October to land without patching whatsmeow. What remains comes from the app state whatsmeow hands over by itself beside jaw's echo, when the phone changes the same setting while jaw's change is on its way:

- a notification's fetch;
- a 409's conflicting changes;
- the fetch when keys arrive;
- recovery snapshots;
- the rest of a send's fetch after the echo, and the 30-second timeout.

Each leaves jaw showing the other value until that setting next changes or the next engine start's full fetch, and needs the phone to act on the same setting of the same chat in that window. It is written in the plan, the design and the protocol. H4 weakens the healer.

A cheaper close, if wanted later: when a wait ends by its echo after something was held back for its index, fetch that collection again, as a refusal does. Those are exactly the cases where the order mattered, and they're rare. It would need a flag in `app_state_awaiting`, whose migration isn't merged yet.

## What was asked, checked

1. **One transaction, synced, before the answer.**
   - `act` holds `inner.enqueue` from taking the action's time through `durably`'s synced commit, and queues the outbox rows only after it (`send/chat_actions.rs:432-465`).
   - The writer's step writes the row with a new `v` and its event (`upsert_chat`, `apply_amendment`), `set_read` for a mark read, each outbox row (`app_state_row`) and its wait (`AWAIT`), in the one transaction `apply_durable` refuses to commit unsynced.
   - A step that fails is rolled back with its savepoint, quarantined, and answered as an internal error, with no outcome put.
   - What no test pins: T1.
2. **"Already so" writes nothing:**
   - pin by `pinned > 0`;
   - archive by `archived`;
   - mute by `muted_until`, -1 and every time from `FOREVER` on taken as `FOREVER`;
   - mark unread by the mark, and mark read by no mark and nothing unread;
   - star by `starred`.
   The mutant `read-already-by-mark` fails five tests. The exception is I1.
3. **The pin limit:**
   - It counts unarchived pinned chats through `chats_by_sort` (`sort >= '1~'` exactly when `pinned > 0`, the generated column being `iif(pinned > 0, '1~'…, '0~'…)`), less a chat being merged away.
   - It's read in the writer's transaction, and the enqueue lock also serialises the requests.
   - The mutant `merge-counts` fails two tests of its own, and the plan tests.
4. **The echo:**
   - whatsmeow's `EncodePatch` writes `PatchInfo.Timestamp.UnixMilli()` into every mutation (`appstate/encode.go:269-275`), and `patchInfo` sets it from jawd's value whenever that's positive.
   - The demo engine keeps and echoes the value as sent.
   - The converter's write finds each field as jawd wrote it: a pin at its time, a mute's -1 as `FOREVER`, a star by `starred IS NOT ?`, which reads neither its time nor whose it is.
5. **A read on any device unmarks** (`convert/app_state.rs:68-69`); the mutant `no-unmark` fails two tests.
6. **A read-marking patch's receipts go first.** `tell_now` sends what's owed and keeps "told" before the patch takes its turn, and a failure there is the patch's, tried again. The chat is found by name as the writer would find it, so a merge in between finds the kept chat. The mutant `no-receipts-first` fails three tests.
7. **A refusal:**
   - `Step::Refused` quarantines the row with its patch and error and deletes it;
   - it releases the wait, and puts the field back only if that wait was its own;
   - a row already gone, failed or not app state is left alone, so it's done once.
   A refused deletion for you is quarantined where it was dropped, and fetches nothing. `fetch_again` waits for the connection, then the turn, and stops when the engine that gave up restarts, whose start fetches every collection in full. The mutants `no-quarantine`, `no-fetch-again`, `undo-not-latest` and `undo-no-wait-release` each fail tests. What production does instead: I2.
8. **Archive and unpin go as two patches in the chat's order:** queued in that order in the chat's lane, and in `n` order on resume. Either refusal is consistent with what WhatsApp ends with (weighed in item 11's last edge).
9. **Plans and schema:**
   - Every new statement is pinned in `plans.txt` and its module's plan test: each reads a primary key or an index, and `PASSED` a range of the partial index.
   - `count(*)` of `app_state_awaiting` for `status` reads its covering index, one row per index at most.
   - There's one new table and no new column. `app_state_awaiting` is `STRICT` and `WITHOUT ROWID`, with `json_valid(key)`, `NOT NULL` where meant and `outbox` `UNIQUE`, and has no foreign key, since it outlives its outbox row until the echo. `outbox.n` and `inbox.seq` are `AUTOINCREMENT`, so the keys it holds are never reused.
   - Migration 19 is a schema step whose cost doesn't depend on jaw.db's size: two `CREATE`s on an empty table. Its line in `migrations.lock` is the next number. The upgrade from every earlier version, the fault at every statement of every migration and the frozen-text test all pass.
10. **The hold-back:**
    - What WhatsApp sends for an index while it waits is kept raw (`app_state`) and not applied.
    - The wait goes with its echo (same index and time), once the drain passes `echo_by`, when its outbox row has gone without going, or with its refusal; and a refusal undoes only its index's latest patch.
    - `echo_by` is `max(seq)` of the inbox when `Step::Sent` commits, after both engines have the echo in the inbox; an empty inbox gives 0, and there the echo is already converted.
    - The drain is one loop in `seq` order (`sync/drain.rs`), so `passed(seq)` holds.
    - jawd waits for the engine's answer with no timeout of its own, so the turn lasts until it's answered or the engine goes. jaw-engine fails every call in flight when its link ends (`Pending::fail_all`).
    - The mutants `no-hold-back` (15 tests), `no-echo-by` (3), `no-passed` (2) and `no-turn` (3) each fail. What it leaves is weighed above.
11. **The edges:** weighed above.
12. **What main brought:**
    - main's migrations 13 to 18 run before 19;
    - `ECHO_BY` and `RELEASED` run for vote rows too and match nothing, each a search of an index;
    - `touches_fts` looks inside `Change::AppState`, whose own test fails without it, beside `Settings`, `Backfill` and `Thumbnail` as main has them;
    - a chat action asked while a text waits for its preview is queued before it, which changes no data;
    - a starred copy a merge deletes for you loses its star, and its star patch's echo then finds a tombstone and changes nothing;
    - the turn belongs to the supervisor, which sync and sending both hold;
    - the demo `Failing` engine never answers what its crash catches, which holds the turn in demo runs only.
13. **The whole-branch review's leftovers:** H1, H2, P1 and H5.

## Measurements

All at `0f4a1eb` in a clone of the worktree under `/tmp/jaw-audits/chat-actions`, built with `CARGO_INCREMENTAL=0`, on this Mac (20 cores) at a load average of 51 to 67 from other agents' builds.

- **jawd's tests** (`cargo test -p jawd --bin jawd`): 707 passed, 2 failed and 12 ignored, in 430.65 s. The two failures, `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push` (a timeout) and `the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it` ("a live write waited 1.07 s"), passed alone in 5.09 s. Neither file is in this range (H7).
- **The engine's tests** (`go test -race -count=1 ./internal/wa/`, with Go's cache in the audit folder): ok, 2.264 s.
- **Mutants.** Each changed one thing in the clone, was built and ran the same 84 tests (the chat actions' request tests, their store, converter, migration and plan tests, and main's synced-commit test), then was reverted:

  | Mutant | What it changes | Tests failing |
  | --- | --- | --- |
  | `unsynced` | `act` commits at `NORMAL` | 0 (T1) |
  | `no-hold-back` | `held_back` applies everything | 15 |
  | `no-echo-by` | `Step::Sent` sets no `echo_by` | 3 |
  | `no-passed` | draining ends no wait | 2 |
  | `no-receipts-first` | a mark read's receipts don't go first | 3 |
  | `no-star-step-check` | the star's step doesn't check `SHOWN_DELETED` | 2 |
  | `undo-not-latest` | a refusal puts back a superseded patch | 1 |
  | `no-unmark` | a read on any device leaves the mark | 2 |
  | `no-turn` | a patch takes no turn | 3 |
  | `no-quarantine` | a refusal keeps nothing in quarantine | 7 |
  | `no-fetch-again` | a refusal fetches nothing | 5 |
  | `merge-counts` | a chat being merged away counts toward three | 4 |
  | `read-already-by-mark` | mark read is so already with no mark | 5 |
  | `undo-no-wait-release` | a refusal leaves its index's wait | 4 |

- **Probe P1 (I1)**, through the writer:
  - after jaw's mark unread: marked, 2 unread, 1 patch queued;
  - after a read-self receipt up to the newest message: not marked, 0 unread;
  - jaw's mark read: `Already`, still 1 patch queued;
  - after the mark unread's echo: marked, 0 unread.
- **Probe P2 (X1)**, through the writer and `convert::history::walk`:
  - pinned by app state at 1791999940000;
  - after jaw's unpin and its echo: 0;
  - after a blob with that conversation pinned at link time: pinned 1791999940000, archived, with `app_state`'s latest for the pin still `pinned: false`.
- **`retryable` for a server refusal (I2):** `server returned error updating app state (regular_low): <error code="N" …/>` is retryable for N = 400, 403, 404, 405, 409 and 500.
- **Patches waiting for the turn (P1):** for 26, 51, 101 and 201 patches, the attempts after the gate opened were 326, 1,323, 5,388 and 21,646, and the drain took 0.14, 0.27, 0.57 and 1.16 s (debug build, demo engine).
- **The writer's cost of each chat action.**
  - Method: a jaw.db file in WAL at `NORMAL`, 2,000 chats of 100 messages each (200,000 messages). 400 rounds of each action, each in its own transaction with its commit, the outbox and waits emptied between rounds.
  - Release build, p50 / p99 / max in ms:

    | Action | p50 | p99 | max |
    | --- | --- | --- | --- |
    | pin | 0.325 | 1.418 | 4.291 |
    | unpin | 0.161 | 2.630 | 6.474 |
    | archive (with its unpin) | 0.156 | 1.278 | 5.423 |
    | unarchive | 0.155 | 1.544 | 6.718 |
    | mute | 0.151 | 2.005 | 3.517 |
    | mark unread | 0.236 | 2.179 | 4.160 |
    | mark read (with its read) | 0.307 | 1.121 | 2.056 |
    | star | 0.110 | 1.651 | 2.899 |

  - The debug build gave p50 0.17 to 0.37 ms and p99 0.47 to 2.18 ms. The tails are the load's scheduling more than SQLite's work: each action is a few primary-key and index searches and a few row writes.
  - p99 is within the 5 ms live budget. The synced request adds its full flush, as every durable request does.

## Verdict

What must change before landing:

- I1: keep the mark while a mark unread of jaw's waits, or don't take a mark read as so already then, with a test that fails without it.
- I2: classify jaw-engine's `ErrAppStateUpdate` by its code, with the engine test's cases.
- T1: count the five chat actions' synced commits in `each_sending_call_that_keeps_something_is_one_synced_commit`.

Recommended here too: X1. The rest stand as follow-ups.

Verdict: changes needed

# Verdict of round two (`f414e65c`)

What must change before landing:

- **I3:** match each wait, `marking` and history's settings by the chat and its setting rather than by one JID's text: the person's JIDs for a one-to-one chat, and a merged-away chat's waits moved to the kept chat or found there. Test it with P3, P4, P7, P7b, P8, P9 and P10's sequences, each failing without the fix.
- **T2:** P5 as a test, failing with `set_read` searching under the read's own JID.
- **T3:** P6 as a test, failing with X1 leaving out the JID jaw keeps the chat by.

Recommended with I3: settle O1 by looking at which JIDs the phone's app state names one-to-one chats by, and if they're LIDs, have jaw's patches name the JID WhatsApp keeps that setting under.

For the landing, whenever it comes: L1's ten conflicting commits and two lines by hand on today's main; migration 25 after places and events; `7b5fc48e` dropped once places and events' L1 is on main. H8, H9, and round one's H1 to H3, H5, H6 and P1 to P3 stand as follow-ups.

Verdict: changes needed

# Verdict of round three (`9bce63e7`)

What must change before landing:

- **I4:** as a merge starts, a setting that jaw's change still waits for takes that change's value, not main's combination. That's in `start_merge`, and in `merge_settings_again` at the end of a merge an earlier jawd started. Better, and closing X2: each setting takes the newest value held under the two chats' JIDs, with main's rule only where nothing is held. Test it with A1 to A4's sequences, each failing without the fix.
- **I5:** keep with each wait the index jaw's patch named, as a column of `app_state_awaiting`, with its lock line, `kept.rs`'s seed and migration 25's test. Hold back by order only what comes under that index. Weigh everything else by time, counting jaw's patch in place of that index's raw value. Test it with D1 (with D0 beside it), E1 and F1, each failing without the fix.
- **T4 and T5:** tests of a merge moving a star's wait, and of a moved wait keeping its `echo_by`, each failing with its switch.

Recommended with them: break O2's ties by the key, and fix H10's words in the migration, whose text changes with I5 anyway.

For the landing, whenever it comes: migration 26 after the library's 25, the five conflicts resolved by keeping both sides, and the re-checks under "Landing". H9, and round one's H1 to H3, H5, H6 and P1 to P3, stand as follow-ups.

Verdict: changes needed

# Verdict of round four (`678462e8`)

What must change before landing:

- **I6:** keep a wait for each index jaw's waiting patches for a setting named, rather than one a setting (`app_state_awaiting`'s primary key `(key, named)`). What comes under any of those indexes is then held back until that patch's own echo, and each wait counts at its time in place of its index's raw value, at a merge too. The migration's text changes, with its lock line and its test. Test it with N1 in both orders, N2 and N3, each failing without the fix. If the owner instead accepts the window as one of the design's caveats, the plan's decision and the design name all three cases, and it stands as "other".
- **T6:** tests of an echo known by its index, of O2's tie inside `newest`'s loop, and of a merge looking under a third JID of the person, each failing with its switch.

P4 stands as a follow-up for main, as do H9 and round one's H1 to H3, H5, H6 and P1 to P3. O4 goes on the landers' watch list.

For the landing, whenever it comes: the next free number (29 after media out and communities), `kept.rs` with media out's and communities' seeds, the outbox's and presence's resolutions beyond keeping both sides, and the re-checks under "Landing".

Verdict: changes needed

# Verdict of round five (`d42da1c1`)

What must change before landing:

- **T7:** tests of a refusal while a later patch for another setting or another chat waits, and of a pinned chat's refused archive while its unpin waits, each failing with `r5-later-wait-any-key`; and a test that a later patch's wait for an index starts with no `echo_by` after WhatsApp took the earlier one, failing with `r5-await-keeps-echo-by`. The audit's probes P1 to P3 (`chat-actions-audit-r5-probes.rs` in the reports folder) are those tests.

I6 and T6 are closed. P4 stands as a follow-up for main, as do H9 and round one's H1 to H3, H5, H6 and P1 to P3. O5's two guards stay as safeguards, and O7's test goes on the landers' watch list.

For the landing, whenever it comes: the next free number (29 after media out and communities, 30 if disappearing lands first too), `kept.rs` with their seeds, the outbox's, presence's and disappearing's resolutions beyond keeping both sides, and the re-checks under round five's "Landing".

Verdict: changes needed

# Verdict of round six (`2f6ad6f4`)

T7 is closed: P1 and P2 fail with `r5-later-wait-any-key`, P3 with `r5-await-keeps-echo-by`, and nothing else fails with either. Nothing but those tests changed since round five. Nothing marked data loss or inconsistency remains, and every rule the branch changes has a test that fails without it. P4 stands as a follow-up for main, as do H9 and round one's H1 to H3, H5, H6 and P1 to P3. O5's two guards stay as safeguards.

**Landing.** The migration takes the next free number when it lands: 29 after communities' 28, or 30 if disappearing lands first. Its lock line goes last as `<n> schema 18e69a4d683f05b66b98cc4fb66633823ff6ea8cd8660d172cf09b2dfe669d7b`, keeping its hash. `schema.rs`'s comment, the migration test's name, the plan's lines and the six commit messages that name 26 say the new number. The 65 data-layer commits `scripts/check-data-audit.py` names (round five's 64 and `2f6ad6f4`) take `Data-Audit: docs/audits/2026-10-29-chat-actions.md`, and this note is committed in the branch. `kept.rs` is main's, with media out's seeds, plus communities' classes and seeds, disappearing's if it lands first, and `app_state_awaiting`'s class and seed with `named`. Where the branch meets main, the lander re-checks four things beyond keeping both sides:
- the outbox's writer against media out's `typed` draft rule: `Step` with every side's variants, `SHOWN_DELETED` in its `shown_deleted!` macro form made `pub(super)`, the plan rows written again and read, and media out's draft tests re-run with the branch's outbox and refusal tests;
- one of presence's three identical "reachable" predicates kept, with the branch's callers pointed at it;
- the pin tests re-run against communities' sort;
- if disappearing lands first, its four conflict points: `Step::Disappearing`, `send/outbox.rs`'s attempt, `touches_fts` and `kept.rs`.

Then the gate, the upgrade from version 1 through the new number with `--engine demo`, and `make check-audit`. Round six's "Landing" has each of them in full.

Verdict: approved
