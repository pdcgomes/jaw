# Audit: a panicking conversion can't stop the inbox's drain (`drain-panic`, M8)

Range: `ea0f4134..6405f92e` in `drain-panic`, two commits (the plan, `d5109709`, and the change with its three tests, `6405f92e`), 2 November 2026. Auditor: database-expert. Round one. Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `6405f92e`.

The worktree stayed at `6405f92e`, with nothing uncommitted but this note. Everything was measured in one clone of it at `/tmp/jaw-audits/drain-panic/jaw`, built with `CARGO_INCREMENTAL=0` under `nice`, at a load of 36 to 86 on a shared Mac. Every rule's switch and the audit's own mutants were built into one copy of the head, each chosen at run time by `JAW_SWITCH` (one per test process), so each fails or passes with the rest of the code as it is; two probes were built into the head and into the base, `ea0f4134`.

## In short

- **Changes needed.** One change loses data (D1), and three rules the change touches have no test that fails without them (T1 to T3).
- **The fix itself is right.** Each inbox row and each row of history kept raw converts under its own `catch_unwind` into an output of its own, which joins the batch only once the row has converted. A row that panics leaves nothing of itself, stays in the inbox or in `history_raw`, and is kept in quarantine once, under the name its later `Drained` clears. The rows after it convert, in order, once each. At each start it's tried once more, never in a loop.
- **Every builder switch fails its test**, applied by the audit: all ten, at the assertion that names the rule. So does a catch around the whole batch instead of each row.
- **D1, data loss.** A history notification whose conversion panics makes no offer, so the fetcher's first sweep deletes its blob from the spool while its row is kept in the inbox. On main the converter never got that far.
- **T1 to T3, test gaps.** The tests pass when the panic's own message is lost (T1); when the batch drops a row's pictures or name, and they only hang when it drops its offers (T2); and when a panicking row is tried again at once, for ever, while the rows after it still convert (T3).
- **The full suite at the head** passes but for five of jawd's timing tests at a load of 55 to 86, three on the landers' list and two first sightings; all five pass alone, 15 of 15.
- **Landing.** No migration. It merges onto main `0dd286c9` without a conflict, and main adds no `Change::Quarantined`. The trailer goes on the change's commit.

## What holds

- **Nothing of a panicking row reaches the writer.** `convert_row` and `history_row` build their row's output in values of their own (`sync/drain.rs:109-182`, `596-605`); a panic drops it whole, offer, picture, name and thumbnail included. Switch 5, main's conversion in place, keeps both panicking messages and fails.
- **A conversion's panic can't happen mid-transaction.** Conversion runs on a blocking thread before its batch is handed to the writer (`drain.rs:429`, `530`), so no transaction, savepoint, counter or FTS row is open when it unwinds, and the writer never sees it. Conversion holds no shared state: no `Mutex`, `RwLock`, `RefCell`, thread-local, `OnceLock` or `LazyLock` in `convert/` (its `people` and `app_state` among it), `sync/drain.rs` or `store/thumbnails.rs`, whose `sha256` is all a conversion calls there. So `AssertUnwindSafe` is sound, and nothing is left poisoned for the rows after. Release builds unwind: `[profile.release]` doesn't set `panic = "abort"`, and nothing in `.cargo` does. An abort isn't caught (a stack overflow, memory running out), but nothing in conversion is known to do either, as prost caps a message's nesting at 100.
- **Kept, once, and cleared.** The quarantine's unique `(source, ref)` (`store/schema.rs:351-359`) and its upsert (`store/write.rs:660-667`) keep one row per panicking row, refreshing `at` and `error`. Its `raw` is NULL, as the schema says for those sources: the raw is the inbox or `history_raw` row it names. Only a `Drained` deletes that row (`store/write.rs:725-736`), and the same `Drained` deletes the quarantine row once a later jawd converts it. Merges rename only `answer`, `edit` and `vote` references (`store/write/events.rs:110-115`, `store/write/polls.rs:50`). `inbox.seq` is `AUTOINCREMENT` (`store/schema.rs:337-345`), so no reference is ever reused. `jawd status` counts the row in `stored.quarantined` and its raw row in `stored.inbox` or `stored.history` (`state.rs:253-259`).
- **Tried once a start.** The converter reads past the last seq of each batch it stores (`drain.rs:441`), and history past the last `(chat, id)` (`drain.rs:539`). A panicking row is tried again only at the next start, which reads from the beginning: the control run counts 4 panics for the inbox test's two rows over two starts, and 2 for history's one.
- **`groups`' new split splits no row.** Besides the two the branch adds, each a row's only change, `Change::Quarantined` is made only by `sync/fetch.rs:120`, alone in its batch, and by a test (`store/rules.rs:714`); main adds none. A row kept in quarantine is then a savepoint of its own (switch 7). `apply_live` still cuts only after a `Drained` (`drain.rs:486`), which keeps a `Quarantined` with what follows it: harmless.
- **`join` takes in all five of `Converted`'s fields**, the later row's name winning as before (`drain.rs:63-72`), though nothing tests three of them (T2).
- **Durability, plans and schema don't change.** The branch adds no statement, index, trigger, PRAGMA, commit or ack. A quarantine row commits in its batch's transaction at `NORMAL`, as every converted row does; a power cut before a later synced commit brings the batch back, to be converted again, idempotently. The engine's acks still follow the synced inbox commit. No migration; the fingerprint and the pinned plans are main's.
- **The other drains.** The backfills already catch a panic per message, in a savepoint, and keep it in quarantine with its raw protobuf (`store/write/kinds.rs:71-110`, `contacts.rs`, `copies.rs`). The send paths run the converter on your own composed messages, not as a drain. History blobs don't catch a panic (O1).

## Findings

### D1. Data loss: the spool's sweep deletes the blob of a notification kept for a panic

**Where:** `sync/drain.rs:80-93` with `sync/fetch.rs:87-99` and `142-156`.

**What happens:**
- A history notification's offer is made inside `message()` (`drain.rs:266-272`). So a notification whose conversion panics leaves no offer, by design, and is kept in the inbox and in quarantine. Only `convert::web_message` (`drain.rs:262`) runs before the offer, so that's where the panic would be.
- The converter now goes on past it. The first time it reaches the inbox's end, it sends `Work::Offered` (`drain.rs:409-412`). The fetcher then removes every spool file that no offer of this run has named (`fetch.rs:95-99`, `142-156`).
- So if that notification's blob is already in the spool, it's deleted while the inbox row that names it is kept. A run that stopped, crashed or lost power in the middle of history leaves the blob there: the case `a_restart_before_a_blobs_drain_uses_the_blob_in_the_spool_again_without_storing_anything_twice` is for.
- The next jawd that converts the notification has to download the blob again. If WhatsApp's server no longer keeps it, the notification goes to quarantine as one that can't be had, and the history it carried is lost.
- On main the converter never got past the panic, so it never reached the sweep: the blob stayed, though nothing after it converted.
- The design says: "A file is deleted only after a synced commit stops pointing to it."

**How likely:** It needs a jawd whose `web_message` panics on a notification that an earlier jawd took. That's a regression arriving with an update in the middle of history: the case this change is for. Main deletes the blob the same way when `web_message` returns an error for such a notification (`undecodable`, `drain.rs:251-255`). The fix below closes both.

**Measured:** probe S, in Measurements. At the head: "jawd: removed 1 files from the spool whose history is already in", the row after it converted, inbox 1, quarantine 1, and the blob gone. At the base, the converter spun on the notification: 1,203 panics, nothing after it converted, and the blob kept.

**Fix:**
- Make the sweep delete only what it knows is done:
  - a `.pb` file whose ID has its `history_kept` row, which the durable commit that drains its notification writes (`fetch.rs:214-220`), and which nothing deletes;
  - `.part` files, since the engine renames a download into place only once it's whole and synced (`engine/internal/wa/sync.go:246-264`).
- Anything else stays.
- A lesser fix is to skip the sweep in a run whose converter kept an inbox row for a panic, or left a message row it couldn't decode. A blob whose rows are in then waits for a later sweep. The spool isn't in `jawd status`, so that wait should be bounded or counted.

**Test:** keep probe S's shape, with the panic hook made a test's:
- a notification whose conversion panics before its offer, its blob in the spool as a run stopped mid-blob leaves it, and a row after it;
- the row after it converts, and the notification stays in the inbox and in quarantine;
- the blob is still in the spool after the sweep;
- then a jawd that converts the notification walks the blob from the spool without asking the engine.

### T1. Test gap: the panic's own message

**Where:** `drain.rs:100-107` (`panicked`), and the tests' `error LIKE 'converting it panicked: %'` (`sync/tests.rs:220`).

**What happens:** The quarantine's error is the only record of why a row is kept, and the tests check only its prefix. With `panicked` losing the panic's message, so that it always says "for no reason it gave", both tests pass (mutant `why`). The hook's payload is always a `String`, from `assert_ne!`, so the `&str` branch is never run either.

**Fix:** In both tests, check the whole error: "converting it panicked: " and the hook's message. Have the hook panic once with a literal message too.

### T2. Test gap: what a row converts to besides its changes

**Where:** `drain.rs:63-72` (`Converted::join`).

**What happens:** Every output of a row that converts now reaches the batch only through `join`.
- Dropping the row's `pictures` or its `my_name`, or letting the earlier row's name win, passes every test of `sync::` (120), `demo::`, `presence::` and `main.rs` (32). No test sends a picture row or a name through the converter at all.
- Dropping `offers` passes all of `sync::` too. It's caught only by the demo's two history tests hanging: their wait for blobs has no deadline (`demo/mod.rs:606-622`). They said "0 of 7 blobs in after 734 s" until the audit stopped them.
- Dropping `thumbnails` is caught, by three of sync's link-preview tests.

**Fix:**
- Add a test of `convert_rows` on one batch holding a notification, a picture row, two rows that set your name, a link preview and a row that panics. Check that the batch keeps the offer, the picture's JID, the later name and the thumbnail.
- Have `join` destructure what it takes in (`let Converted { changes, offers, my_name, pictures, thumbnails } = row;`). Then the compiler says so if a field added later isn't taken in.

### T3. Test gap: tried once a start, not again at once

**Where:** `drain.rs:439-441`, and the inbox test (`sync/tests.rs:248-306`).

**What happens:** The inbox test checks that the rows after a panicking one convert and that the quarantine holds one row for each. It doesn't check how often a panicking row is tried. Mutant `spin` is a converter that doesn't read past a batch holding a panicking row. It tries that row again at once, for ever, while still converting what arrives after it, and the test passes: 137 panics in 1.15 s, where the head makes 4. The brief's rule is "isn't tried again for ever". History's twin, `spin-h`, is caught, but only by its test hanging (143,019 panics in 150 s).

**Fix:** Count the hook's panics with an `AtomicUsize` beside `PANICS`. Check that each panicking row panicked once in each start: two in the first, two more after the restart, and one each start for history.

### O1. Other, a follow-up: a history blob whose walk panics

This is main's behaviour, and the plan's follow-up.

**Where:** `sync/fetch.rs:182-209`.

**What happens:**
- The walk runs the converter on every message of a blob without catching a panic. So a message that panics ends the walk.
- The chunks handed over before it stay, idempotently. The blob fails as `Failed::Later("converting it stopped: …")` and waits behind the others. It's walked again after a pause doubling to a minute (`RETRY_MOST`, `fetch.rs:47`), for as long as jawd runs, and again at each start.
- Nothing is lost: the notification stays in the inbox, and the blob stays in the spool, named by its offer, so the sweep keeps it. Other blobs and the inbox aren't held up.
- But every conversation after the panic in that blob waits for a jawd that converts it. The blob is never in quarantine, and nothing counts it. Each minute, everything before the panic is walked and stored again.
- History comes through blobs, so a converter regression would show there first.

**Fix:** Have the walk catch a panic per message, keep that message raw in `history_raw` (whose drain this branch makes safe) or in quarantine, and go on.

### O2. Other, hygiene: a panic outside the row's catch still spins

**Where:** `drain.rs:429` (`else { continue }`) and `drain.rs:530` (`else { return }`).

**What happens:** Only the batch's own bookkeeping is left outside the catch: `join` and an `eprintln!`. So a `JoinError` should now be unreachable. If one came anyway (an `eprintln!` to a closed stderr panics), the inbox's converter would read the same rows again at once, with no pause: X1's spin. History kept raw would stop for that run.

**Fix:** On a `JoinError`, wait with the store error's back-off (10 ms doubling to 1 s) before reading again.

### O3. Other: first sightings for the flakes list

Each failed at a load of 55 to 86, is a timing test of code this branch doesn't change, and passed alone 3 of 3:
- the store's `store::tests::the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end`: "the checkpointer never flushed jaw.db while the writer appended", its listed sibling's message;
- `tests::jawd_stops_well_within_its_bound_while_messages_pour_into_a_watched_chat`: "stopping took 1.437368042s". It failed again in a mutant run that can't reach it;
- `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean`: "within jawd's bound to stop", in a mutant run that can't reach it.

## Measurements

**The three new tests at the head:** pass, in 4.9 s.

**Each rule's switch**, applied by the audit (the builder's diffs, chosen at run time), against its test:

| Switch | Test | Fails at |
| --- | --- | --- |
| 1. No `catch_unwind` in `convert_rows` | inbox | "waited too long for the row after it", with 31,382 panics in 6.8 s: the spin |
| 2. None in `history_rows` | history | "the row after it is converted, and nothing of it is kept": `[]` for `["h1", "h3"]` |
| 3. The inbox row that panicked drained | inbox | "each stays in the inbox": `[]` for `[2, 4]` |
| 3b. The history row that panicked drained | history | "it stays raw": `[]` for `["3EB0PANICS"]` |
| 4. The inbox row not kept in quarantine | inbox | "each is kept in quarantine, saying why": `[]` |
| 4b. The history row not kept in quarantine | history | "at start 0": `[]` |
| 5. Its partial changes kept (main's conversion in place) | inbox | "nothing of either panicking message is kept": both `3EB0PANICS` stored |
| 6. An inbox reference that changes at each try | inbox | "each is kept in quarantine, saying why": `"2 at 1793619791581"` for `"2"` |
| 6b. A history reference its `Drained` doesn't clear | history | "at start 0": `"3EB0PANICS"` for `"<chat> 3EB0PANICS"` |
| 7. `groups` not ending at a `Quarantined` | store | "each under its own name, in the Live lane": only `("inbox", "2")` |

With no switch, that build passes all three tests.

**The audit's own mutants:**

| Mutant | Test | Result |
| --- | --- | --- |
| w. One `catch_unwind` around the whole batch | inbox | fails: "waited too long for the row after it" |
| why. The panic's message lost | inbox, history | both pass (T1) |
| j-offers. `join` dropping offers | sync, demo, presence, main | sync passes; demo's two history tests hang, "0 of 7 blobs in after 734 s" (T2) |
| j-pictures. `join` dropping pictures | the same | all pass (T2) |
| j-name. `join` dropping the name | the same | all pass (T2) |
| j-name-first. The earlier row's name winning | the same | all pass but the stop-bound flake (T2) |
| j-thumbnails. `join` dropping thumbnails | sync | fails: three link-preview tests |
| spin. The inbox's converter not reading past a batch with a panicking row | inbox | passes: 137 panics in 1.15 s (T3) |
| spin-h. The same for history kept raw | history | hangs: 143,019 panics in 150 s, stopped |

**Probe S, a notification whose conversion panics, its blob in the spool:**
- A hook in `message()` panics for the notification `3EB0NOTEPANICS` before its offer.
- The inbox holds that notification and a message after it, and the spool holds `3EB0NOTEPANICS.pb`, as a run that stopped mid-blob leaves it.
- The converter and the fetcher run as jawd runs them, with a scripted engine. The probe reads the state after the row after it converts and 1.5 s more.
- At the head: "jawd: removed 1 files from the spool whose history is already in". The row after it converted, inbox 1, quarantine 1, blob in the spool false (D1).
- At the base: the converter spun on the notification, as X1 says: 1,203 panics in about 4.5 s. The row after it converted 0, inbox 2, quarantine 0, blob in the spool true.

**Probe T, `convert_rows` on 20,000 live text messages, best of 7, in the debug build the gate's tests use:**
- At the head: 34.2 µs and 40.6 µs a row, in two runs.
- At the base: 35.1 µs and 35.0 µs a row.
- So the per-row catch and `join` cost nothing the load lets one measure.
- The head's build held the switchboard. Its seven run-time checks a row are a few nanoseconds each.

**The full suite at the head** (`cargo test --workspace --no-fail-fast`, 48 min at a load of 55 to 86, 42 of them jawd's unit tests):
- Every target passes but jawd's unit tests: 993 passed, 5 failed, 17 ignored.
- Three failures are on the landers' list: the long-request lane, media in's starts-check and the socket's connection-full.
- Two are O3's first sightings.
- All five pass alone, 3 of 3 each.
- The Go engine's tests weren't run: the range doesn't touch `engine/`.

**Landing:** `git merge-tree --write-tree --merge-base ea0f4134 0dd286c9 6405f92e` is clean. `git grep 'Change::Quarantined' 0dd286c9` finds only the producers above.

## Landing, once a round approves

- Rebase onto main: `0dd286c9`, or later with chat actions.
- The lander re-checks the following, then runs the three tests (and D1's and T1 to T3's), switches 1, 3, 5 and 7, and probe S's test:
  - every `Change::Quarantined` on main is still its row's last change, now that `groups` ends a group there;
  - `join` takes in any field added to `Converted`;
  - an arm main adds to the old loop of `convert_rows` moves into `convert_row`, writing into the row's own `out`.
- The trailer `Data-Audit: docs/audits/2026-11-02-drain-panic.md` goes on the change's commit, which touches `crates/jawd/src/sync` and `crates/jawd/src/store/`, and on any fix commit under those paths. The plan's commit and this note's are docs only.
- No migration: no schema, index, trigger or statement changes.

## Verdict

Before approval:
- D1: the sweep keeps the blob of a notification kept in the inbox, with its test.
- T1 to T3: their tests, each failing with its rule switched off.

O1 and O2 may stand as follow-ups, written down; O3 goes to the flakes list.

Verdict: changes needed

# Round two: `9027f786`, round one's fixes

Range: `ea0f4134..9027f786` in `drain-panic`, seven commits: round one's two, and the fix round's five, one for each finding (T1 `467132d9`, T3 `3a9100de`, T2 `ebb3ca5a`, O2 `e1e27117`, D1 `9027f786`), 2 November 2026. Auditor: database-expert. Round two. Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `9027f786`.

Round one is kept as it was written, above. The worktree stayed at `9027f786`, with nothing uncommitted but this note. Everything was built and measured in one clone of it at `/tmp/jaw-audits/drain-panic-r2/jaw`, with `CARGO_INCREMENTAL=0` under `nice`, at a load of 35 to 73 on a shared Mac. Three builds were measured: the head as it is; a copy of the head with every switch built in, the fix round's and the audit's own, each chosen at run time by `JAW_SWITCH`; and the branch rebased onto main `1669937a`, as it is and with the same switches.

## In short

- **Changes needed, for one test gap (T4).** Nothing is lost or inconsistent. D1 is closed, and so are T1, T2, T3 and O2. But when the sweep's new read fails, the sweep keeps every blob, and no test checks that it does. A sweep that deleted them instead, as main's did, passes every test of the branch's. The audit's probe R is the missing test.
- **D1 is closed.**
  - The sweep deletes a `.pb` blob only when its ID has a `history_kept` row. Only the synced commit that drains a blob's notification writes one, after all the blob's rows, and nothing ever deletes one.
  - So no order of events can delete a blob whose rows aren't in: not across a restart, nor for a quarantined notification, a notification converted while the sweep runs, or a blob an offer of this run names.
  - Blobs whose rows are in, and `.part` files, still go. `jawd status` counts what stays.
  - The new statement searches the table's unique index, and its plan is pinned.
- **T1, T2, T3 and O2 are closed.**
  - Each of the fix round's 27 switches fails its tests, at the assertion that names its rule, both at the head and rebased.
  - So do the audit's two switches on the same rules: a row never tried again after its first panic, and a pause that doesn't double.
- **Nothing new.** The fix round adds one read, and no write, commit, ack or schema. O2's back-off is in the converter's own loop: no ack and no inbox commit waits on it.
- **The full suite at the head passes under the flakes rule:** 1,509 passed, 5 failed, 23 ignored. All five failures are on the landers' list, and each passes alone 3 of 3. The watched library window test passed.
- **Landing on main `1669937a`.**
  - `drain.rs` resolves by carrying main's new `app_state` arm into `convert_row`. That's more than keeping both sides: main's edit has to move with the code. The README keeps both sides.
  - Rebased, these pass: the change's tests, both plans tests, and the whole of `sync::`, `store::` and `convert::`, but for one listed timing test. Every switch fails its tests again.
  - The data-layer commits take the trailer. There's no migration.

## What holds

### D1, as round one meant it

- **What the sweep deletes** (`sync/fetch.rs:148-190`).
  - It skips a file an offer of this run names (`:154`).
  - It deletes `.part` files (`:157`).
  - It deletes a `.pb` only when its ID, the file's name without `.pb`, has a `history_kept` row, read in one read transaction (`:163-177`).
  - Anything else stays, and every blob stays when that read fails (`:179`).
- **Why that's safe.**
  - The fetcher writes a blob's `history_kept` row only in the synced commit that drains its notification, after the blob's last chunk is stored (`fetch.rs:258-262`). So the row means the blob's rows are in.
  - Nothing deletes one: `history_kept` has no `DELETE` anywhere, and its class is raw (`store/schema.rs:370-378`).
  - So the fact the sweep acts on stays true once it's true.
- **A row from a crashed run.** A run that crashed can leave its last commit in the WAL, recovered at the next open before it reached the disk. But every start makes a synced commit, `sync::running` (`sync/mod.rs:218`), before the converter and the fetcher start (`main.rs:595-611`). That flushes the recovered frames, so any `history_kept` row a sweep reads is durable.
- **Across a restart:** the D1 test's second half. A jawd whose conversion of the notification doesn't panic walks the blob from the spool without asking the engine, and its drain takes the quarantine row with it.
- **A quarantined notification:**
  - one kept for a panic keeps its blob, as the D1 test checks;
  - one kept because its blob can't be had has no blob, as `Failed::Never` comes only when the spool has none (`fetch.rs:205-216`).
- **A notification converted while the sweep runs.**
  - The fetcher sweeps inside its own loop, so it asks the engine for nothing meanwhile.
  - The engine writes a blob only when asked (`engine/internal/wa/engine.go:616`), through a `.part` file that it flushes and renames (`:652`).
  - An engine call has no deadline (`engine/mod.rs:71-75`), so no download the fetcher gave up on is still being written.
  - An offer sent meanwhile waits in the channel, and its blob isn't fetched yet.
  - A blob the sweep has listed goes only if its row is there when the sweep reads it.
- **A blob an offer of this run names** is skipped by name. Without that rule it would still stay unless its rows are in (O5).
- **No orphans.** A notification leaves the inbox only in the commit that writes its blob's `history_kept` row, since `convert_row` never drains one (`sync/drain.rs:269-277`). So every `.pb` without a row has its notification in the inbox, waiting for a jawd that converts it.
- **The spool is bounded and counted.**
  - It holds the blob being fetched, those that offers of this run name and that wait to be tried again, one blob for each notification kept in the inbox (which `stored.inbox` counts), and `.part` files until the next start's sweep.
  - `stored.spool_files` and `stored.spool_bytes` count every file in the folder the fetcher uses (`state.rs:252`, `sync/mod.rs:337`).
  - The D1 test checks the count after the sweep: 1 file, of the blob's bytes. Nothing checks it before (O6).
- **The statement.**
  - `SELECT EXISTS (SELECT 1 FROM history_kept WHERE id = ?1)` plans as `SCAN CONSTANT ROW` and `SEARCH history_kept USING COVERING INDEX sqlite_autoindex_history_kept_1 (id=?)`.
  - That scans no table and sorts in no temporary b-tree. The plan is pinned (`store/plans.txt:4215-4219`), and both plans tests pass at the head and rebased.
  - It runs once a start, as one search for each blob in the spool, on a reader. `jawd status` lists one small folder.

### T1, T2, T3 and O2

- **T1.** Both tests compare each quarantine row's whole error, and one hook panics with a literal. The switches `why`, `no-str` and `no-string` each fail them.
- **T2.**
  - `join` destructures `Converted` (`sync/drain.rs:65-72`), and so does the batch test. A field added later that `join` doesn't take in fails to compile.
  - The batch test fails with each field dropped (`j-changes`, `j-offers`, `j-pictures`, `j-name`, `j-thumbnails`), and with the earlier name winning (`j-name-first`).
- **T3.**
  - The hook counts each message's panics.
  - `spin` fails the inbox test, with 345 and 308 panics in its first start where the test wants 1 each. `spin-h` reaches the history test's 10 s bound after 27,746 panics.
  - The audit's `a-once`, which never tries a row again after its first panic, fails both tests. So "once more at the next start" is checked too.
- **O2.**
  - When the blocking task fails, the converter now waits the store error's pause before it reads the inbox again (`sync/drain.rs:434-444`): 10 ms doubling to 1 s, and back to 10 ms once a batch is stored.
  - `o2` fails its test with 6,818 tries in a second. The audit's `a-o2-flat`, a pause that doesn't double, fails it with 79.

### Nothing new

- **One read, no writes.** The fix round adds one read and a folder listing. It adds no write, commit, ack, index, trigger, PRAGMA or migration, and its hooks are `#[cfg(test)]`.
- **The sweep deletes less than main's did,** and only once the synced commit that stops pointing at a file is in.
- **O2's back-off holds no ack.** The inbox's commits and the engine's acks are made by the writer (`store.inbox`), which waits on no conversion. The back-off pauses only the converter, at most 1 s a try.
- **A failure that persists** holds back the rows after it, as a store error does. Outside each row's own catch, only the batch's bookkeeping can fail: `join`, or an `eprintln!` that can't write.
- **History kept raw** still ends its pass when its task fails, as before, and now says so (`:545-553`).

### The gate's 16:02 rebuild

It doesn't matter.

- **The deciding run used its own binaries.** The `make test` that decided the gate, from 15:00 to 16:01, ran the binaries built from `9027f786` before it started.
- **The rebuild changed no code.** The 16:02 cargo call rebuilt ring, rustls, rustls-webpki, tokio-rustls and jawd under the same file names, from the same unchanged source. Cargo does that when an input a build script watches, such as an environment variable, differs.
- **The later runs used make's binaries.** Under make, cargo then compiled nothing more. So the three re-runs, and the targets the deciding run didn't reach, ran the binaries `make test` uses, with the code that run tested.
- **The audit's own run agrees.** From a fresh clone, its failures are all on the list, and each passes alone.

## Findings

### T4. Test gap: nothing checks that a sweep which can't read which blobs are in keeps them all

**Where:** `sync/fetch.rs:175-180`.

**What happens:**

- When the read of `history_kept` fails, the sweep deletes only the `.part` files and keeps every blob. That's right: it can't know which blobs are done with.
- Nothing tests it. The audit's `a-read-err` deletes the blobs when the read fails, as main's sweep did. It passes every test of the branch's: the whole of `sync::` (122 tests) and `demo::` (15). Only probe R fails with it, at "a blob whose rows aren't in stays".
- The read runs once a start, on a reader, and an I/O error, a damaged page or a busy recovery can make it fail.
- A later change that deleted on a failed read would bring D1 back in just that case: a blob deleted while its notification is still kept in the inbox.

**Fix:** add a test like the audit's probe R, whose text is beside the audit's report, ready to use:

- the spool holds a blob whose `history_kept` row is in, a blob whose row isn't, and a `.part`;
- the test makes the read fail by renaming `history_kept` in the test's `jaw.db`;
- the fetcher is sent `Offered`, and then an offer whose blob, walked only after the sweep, says the sweep is done;
- the `.part` has gone, and both blobs are still there.

The probe passes at the head and rebased, and fails with `a-read-err`.

### O4. Other, a follow-up for main's merge rules: a row tried again at the next start lands after newer rows

**Where:** `sync/drain.rs:78-100`, with main's `Change::AppState` in `store/write.rs`, `store/write/chat_actions.rs:404-414` and `sync/refresh.rs`.

**What happens:**

- A row that panics now waits for the next start while the rows after it go in, as the design asks: a failure "never blocks the changes after it". A mended jawd then converts it after newer rows for the same thing.
- For messages, the merge rules handle that. Where the last row applied wins, it brings back the older value:
  - an app-state index, where main's rule is "under one index WhatsApp's own order does";
  - a push name;
  - a group's info or members;
  - a removed picture.
- Something else usually puts the newer value back:
  - App state is fetched again in full each time an engine connects. That comes in through the inbox after the old row, so the setting is right again in the same start.
  - Contacts and groups are fetched again at each connect too, but go straight to the writer (`refresh.rs`, `store_changes`). So they can race the old row, and lose until the next connect or change.
- Main already does the same for a row that doesn't decode (`undecodable` keeps it and moves on) and for a change the writer quarantines. So this isn't the branch's to fix.

**Fix, as a follow-up:** merge rules by time for those kinds, or have the connect's refresh come in through the inbox.

### O5. Other: the sweep's rule for a blob an offer of this run names is untested, and now only saves a download

**Where:** `sync/fetch.rs:154`.

**What happens:**

- The audit's `a-named` drops the rule. It passes everything it could reach: the D1 test, the restart test, and the whole of `sync::` (123) and `demo::` (15).
- Since D1, the rule matters only for a blob whose rows are already in, such as one for a notification sent again, which would be fetched again. A blob whose rows aren't in is kept by the `history_kept` rule anyway.
- In the same way, the sweep looks a blob up by its file name. `spool_name` makes that from the ID with `/` and `\` replaced, so a blob whose ID had either would never be swept. That's safe, and WhatsApp's IDs have neither.

**Fix:** none needed. If the download it saves matters, add a test of an offered blob whose rows are in.

### O6. Other: `jawd status`'s spool count is checked only after the sweep

**Where:** `sync/fetch.rs:192-200`, and the D1 test (`sync/tests.rs:2057-2058`).

**What happens:** The test reads the count once only one `.pb` is left. So the audit's `a-status-pb`, which counts only `.pb` files, passes, and a download cut short would go uncounted.

**Fix:** have the test read the count before the sweep too, when the spool also holds the `.part` and the done blob.

### O1 and O3

- **O1**, a history blob whose walk panics, is main's behaviour and stands as the plan's follow-up.
- **O3's three first sightings are now on the landers' list:** the checkpointer's flush tests, jawd's stopping tests, and sync's stop-bound and engine-disconnect tests.

## Measurements

**The full suite at the head:**

- `cargo test --workspace --no-fail-fast` ran from 16:26 to 17:12, at a load of 35 to 73, and exited 101: 1,509 passed, 5 failed, 23 ignored.
- Every target ran: jaw, jaw-client, jaw-media, jaw-proto, jaw-version, jaw-voice, jawd's unit tests and its installed, starting, stopping and upgrading tests, wa-proto, and the doc tests.
- Only jawd's unit tests failed: 996 passed, 5 failed, 17 ignored, in 2,510 s.
- All five failures are on the landers' list, and each passed alone 3 of 3:
  - the feed's `a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`;
  - the feed's long-request lane test;
  - media in's starts-check test;
  - the socket's connection-full test ("nothing went missing: Elapsed(())");
  - the checkpointer's idle test ("the checkpointer woke 2 times with nothing to do").
- The watched library window test passed.
- The Go engine's tests weren't run: the range doesn't touch `engine/`.

**Each switch, at the head and rebased.** Each ran as one test process, from `crates/jawd`. With no switch, eight tests pass: the change's six, the restart test, and probe R. The results are the same at the head and rebased unless a cell says otherwise:

| Switch | Rule | Fails at |
| --- | --- | --- |
| 1, 2 | the rows' `catch_unwind` | "waited too long for the row after it"; "the row after it is converted, and nothing of it is kept" |
| 3, 3b | the row that panicked stays | "each stays in the inbox"; "it stays raw" |
| 4, 4b, 6, 6b | kept in quarantine, under the name its `Drained` clears | "each is kept in quarantine, saying why"; "at start 0" |
| 5 | nothing kept of a row that panics | "nothing of either panicking message is kept"; "the notification's offer, and not the one's that panicked" |
| 7 | `groups` ends at a `Quarantined` | "each under its own name, in the Live lane" |
| why, no-str, no-string | T1 | "each is kept in quarantine, saying why"; "at start 0" |
| j-changes, j-offers, j-pictures, j-name, j-name-first, j-thumbnails | T2 | the batch test, each at its field's own assertion |
| spin, spin-h | T3 | "each is tried once in this start": 345 and 308 panics, rebased 612 and 543; the 10 s bound, after 27,746 panics, rebased 42,965 |
| a-once (the audit's) | T3, once more at the next start | "and once more at the next start": 1 and 1; "tried once in each start": 1 |
| o2, a-o2-flat (the audit's) | O2 | "tried 6818 times in a second", rebased 10,585; 79 |
| d1-old, d1-parts, d1-done, d1-status | D1 | "the blob it names stays in the spool"; "what's done with goes" for the next two; "jawd status counts it" |
| a-read-err (the audit's) | D1, when the read fails | only probe R fails: "a blob whose rows aren't in stays" (T4) |
| a-named (the audit's) | a blob an offer names | none fails (O5) |
| a-status-pb (the audit's) | the spool count | none fails (O6) |

**Rebased onto main `1669937a`.** The rebased head, `a7274302`, builds with no warnings, and `cargo fmt --check` passes.

- The change's six tests, the restart test and both plans tests pass alone.
- The whole of `sync::` passes, 123 tests in 184 s, and so does `convert::`, 62 tests.
- `store::`, without the fault-at-every-statement test, ran 366 passed, 1 failed, 11 ignored, in 651 s. Among them are main's 77 chat-actions tests and 8 app-state tests. The one failure, the checkpointer's flush test after a pass the writer outran, is on the list and passes alone 3 of 3.

## Landing, once a round approves

**Rebase onto main** `1669937a` or later. There are two conflicts, as the brief says.

**`drain.rs`, in the change's own commit.**

- Main rewrote the `app_state` arm of `convert_rows`' loop: the mutation's changes, its row's seq and its time now go inside `Change::AppState`, for `held_back`. The branch had moved every arm into `convert_row`.
- Take the branch's loop, with its per-row catch. In `convert_row`, replace the branch's `app_state` arm with main's. Main's arm sets `my_name`, then pushes `Change::AppState { index, value, seq: row.seq, at: applied.at, changes: applied.changes }`, and doesn't extend `out.changes`.
- That's more than keeping both sides, because main's edit has to move with the code.
- Keeping the branch's arm doesn't compile, since main's `Change::AppState` has three more fields. But a resolution that also kept `out.changes.extend(...)`, with a clone, would compile. It would apply the mutation's changes outside `held_back` and undo chat actions' waits.

**What a short landing audit checks:**

- `convert_row`'s `match` is main's loop body exactly. The audit's diff found the two identical, all 72 lines.
- `git range-diff ea0f4134..<the old head> origin/main..HEAD` shows the change's commit differing only by that arm and D1's only by the README, with the other five unchanged.
- Main still adds no `Change::Quarantined` producer and no field to `Converted`.
- These pass: the change's tests, T4's test, switches 1, 3, 5, 7, d1-old and spin, and main's `store::write::chat_actions::` and app-state tests.

**`protocol/README.md`, in D1's commit.** Both sides append to `stored`'s list. Keep main's `quarantined` and `awaiting`, then the branch's `spool_files` and `spool_bytes`.

**The trailer** `Data-Audit: docs/audits/2026-11-02-drain-panic.md` goes on each commit that touches `crates/jawd/src/sync` or `store/`.

- Today that's six: the change and the five fixes. T4's fix will be a seventh.
- The plan's commit, `d5109709`, and this note's are docs only. A `rebase -x` that amends every commit would put the trailer on the plan's too.

**No migration.** No schema, index or trigger changes, and the one new statement reads an existing table by its unique index.

## Verdict of round two

Before approval:

- T4: a test like probe R, which fails when the sweep deletes the blobs it couldn't read about.

O4 to O6 may stand as follow-ups, written down. O1 stands as before.

Verdict: changes needed

# Round three: `11304aa7`, rebased onto main, with round two's fixes

Range: `73c986bb..11304aa7` in `drain-panic`, nine commits, rebased since round two onto main `73c986bb`: the plan `9907b531`, the change `89cf60e2`, round one's fixes (T1 `2fdb83d3`, T3 `b9eafe84`, T2 `0ea3fdcf`, O2 `25f57c03`, D1 `7ba2390d`) and round two's (T4 `af5d8ee0`, O6 `11304aa7`). Main is now `b33dcba1`, one commit further, in `crates/jaw` only. 2 November 2026. Auditor: database-expert. Round three, in place of round two's landing audit. Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `11304aa7`.

Rounds one and two are kept as they were written, above. The worktree stayed at `11304aa7`, with nothing uncommitted but this note. Everything was built and measured in one clone of it at `/tmp/jaw-audits/drain-panic-r3/jaw`, with `CARGO_INCREMENTAL=0` under `nice`, at a load of 38 to 73 on a shared Mac, and about 7 for the last runs. Three builds were measured: the head as it is; a copy of it with every switch built in, the fix rounds' and the audit's, each chosen at run time by `JAW_SWITCH`, with this round's two probes; and the head merged onto main, for jaw's tests.

## In short

- **Approved.** Nothing is lost or inconsistent, and every rule the change touches has a test that fails without it.
- **The landing resolution is right.**
  - `convert_row`'s body is main's loop body exactly: 75 lines each, and the diff is empty.
  - Main's app-state arm hands the mutation's changes to the writer only inside `Change::AppState`, so they go through `held_back`, once.
  - A resolution that also kept the branch's old `extend`, the one round two warned of, fails eight of main's chat-action tests.
  - An app-state row whose conversion panics is kept in quarantine as any other row, and the rows after it go on (probe P-AS).
- **T4 and O6 are closed.** T4's test fails when the sweep deletes the blobs it couldn't read about. The D1 test fails at O6's new count when `jawd status` counts only the `.pb` files.
- **Round one's and two's rules hold on the rebased head.** Each of their 30 switches fails its test at the assertion that names its rule. a-named still passes, as O5 says.
- **Nothing new is lost or inconsistent.** One interplay with main's chat actions wasn't weighed before. An echo kept for a panic leaves what follows it under its index held back until the next start (O7). Main does the same for an echo that doesn't decode.
- **The full suite at the head passes but for six timing tests, of the feed, media, the socket and jaw-voice.** Five are on the landers' flakes list, and each passes alone three times of three. The sixth, the feed's coalescing test, is off the list and watched, so this round looked into it (O8): the branch isn't on its path, and it has failed in six audits before this branch.
- **Landing.** It merges onto main `b33dcba1` without a conflict, and jaw's tests pass on the merged tree. Eight commits take the trailer, all but the plan's. There's no migration. Beyond the gate, the lander re-checks nothing unless main moves in the data layer first.

## What holds

### The landing resolution, as round two meant it

- **The arms are main's.**
  - `convert_row`'s body (`sync/drain.rs:114-188`) is main `b33dcba1`'s loop body (`drain.rs:65-139` there), one level less indented. That's 75 lines each, the `match` and the `if drained` block, and `diff` finds nothing.
  - `git range-diff ea0f4134..9027f786 73c986bb..11304aa7` shows the plan, T1, T3, T2 and O2 unchanged. The change's commit differs only by main's arm, and D1's only by the README's `status` row, which keeps main's `quarantined` and `awaiting` and then the branch's `spool_files` and `spool_bytes`. T4 and O6 are new.
  - Main still adds no `Change::Quarantined` producer (only `sync/fetch.rs:121`, alone in its batch) and no field to `Converted`. Since round one's base it hasn't changed `groups`, `record_quarantine` or `fetch.rs`.
- **App-state changes go through `held_back`, once.**
  - `applied.changes` appears once in `drain.rs`, inside `Change::AppState { index, value, seq: row.seq, at: applied.at, changes: applied.changes }` (`:120-126`). Nothing adds it to the row's changes besides.
  - The writer applies those changes only in its `AppState` arm, after `chat_actions::held_back` (`store/write.rs:790-808`), which the branch doesn't change.
  - The audit's a-as-twice is the resolution round two warned of: main's arm, with the branch's old `out.changes.extend(...)` kept beside it. It fails main's `send::tests::a_change_from_the_phone_while_a_pin_waits_is_held_back_and_the_pin_goes_after_it` at "held back: jaw's pin goes after it, so it's how every device ends", and seven more of `send::`'s pin, mute, archive and mark tests.
- **Inside the row's own catch.** The arm runs in `convert_row`, under the row's `catch_unwind` (`:83`), and writes only into the row's own output. The branch's batch test sends two app-state rows through it, and checks that the later name wins.
- **A panicking app-state row is kept as any other** (probe P-AS):
  - two messages, then a pin whose conversion panics once it has made its changes, then a pin of another chat;
  - the panicking row stays in the inbox, and in quarantine as `("inbox", "3")` with its whole error. Nothing of it is kept raw, and the pin after it applies;
  - a jawd that converts it at the next start drains it, with its quarantine row;
  - switches 1, 3, 4, 5 and 6 each fail the probe.

### T4 and O6, as round two meant them

- **T4** (`af5d8ee0`): `a_sweep_that_cant_read_which_blobs_are_in_keeps_every_blob` (`sync/tests.rs:2114`) is probe R word for word. It passes at the head, and fails with a-read-err at "a blob whose rows aren't in stays".
- **O6** (`11304aa7`): the D1 test also reads `jawd status`'s spool count before the sweep (`sync/tests.rs:2072-2078`), through `State::status`. It reads it before the test hands the fetcher anything, so no download can race it. It fails with a-status-pb, (2, 204) for (3, 215), and with d1-status, (0, 0).

### Round one's and two's rules, at the rebased head

- With no switch, the 13 tests the switches use pass: the change's six, the restart test, T4's, the store's `groups` test, both probes, and main's three converter-level app-state tests.
- Each of the fix rounds' 26 switches and round two's four fails its tests at the assertion that names its rule, as in round two. The table is in Measurements.
- a-named still passes every test it reaches (O5).

### Nothing new

- **No new write, commit or ack.** The rebase adds no statement, index, trigger, PRAGMA or migration. D1's one read is pinned, and both plans tests pass in the full suite. The engine's acks still follow the synced inbox commit, which the branch doesn't touch.
- **`AssertUnwindSafe` is still sound.**
  - Main's only change to conversion since round one's base is `convert/history.rs`: a blob's chats become `Change::HistoryChat`, in the blob walk.
  - Conversion still holds no `Mutex`, `RwLock`, `RefCell`, thread-local, `OnceLock` or `LazyLock`: only `store/thumbnails.rs`'s atomics, which conversion doesn't touch.
  - Nothing in a `Cargo.toml` or `.cargo` sets `panic`.
- **A row tried again at the next start meets main's chat actions as any row does.** If jaw has queued a later patch for its index since, `held_back` holds the old row back, as it came before that patch. What a kept row doesn't do is end its own patch's wait: O7.

## Findings

### O7. Other, a follow-up for main's chat actions: an echo kept for a panic leaves what follows it under its index held back

**Where:** `store/write.rs:751-756`, with `store/write/chat_actions.rs:365-394` (`held_back`) and `:579-582` (`passed`), and `store/write/outbox.rs:299-300` (`ECHO_BY`).

**What happens:**

- While jaw's patch for an index waits for its echo, what WhatsApp sends under that index is held back: kept raw in `app_state`, but not applied.
- The echo ends the wait. So does the drain of an inbox row at or past `echo_by`, the inbox's last seq when WhatsApp took the patch: `passed` takes it that the converter has then gone past every row before it.
- A row kept in the inbox for a panic is gone past without being converted. If it's the echo, what WhatsApp sends under its index after it, up to `echo_by`, came after jaw's patch but is held back. Then `passed` ends the wait, and nothing applies what's held. So the chat shows jaw's setting while `app_state` holds the phone's newer one.
- At the next start, a jawd that converts the echo writes its older value over the newer one in `app_state`: O4's case.
- Main does the same today for an echo that doesn't decode, and for one the writer quarantines: each is kept and gone past. On main, an echo whose conversion panicked stopped the whole drain instead.
- The next start puts it right. Each engine start fetches every app-state collection again from version 0 (`sync/mod.rs:466-480`, `refresh.rs:44-60`, `engine/internal/wa/sync.go:196-210`). That comes in through the inbox after the kept rows, so WhatsApp's newest value under each index is applied last.
- It takes an echo whose conversion panics, and a change under the same index in the moments between that echo and WhatsApp's answer.

**Measured:** probe P-ECHO, in Measurements.

**Fix, as a follow-up for main:** when `passed` ends a wait whose echo never came, apply what `app_state` holds for its index, through `held_back` as if it had just come. O4's merge rules by time would cover the next start.

### O8. Other, for the landers' week-long watch: the feed's coalescing test failed in the suite at the head

**What happened:**

- `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce` failed in the full suite, at a load of about 70.
- One push of its ten came 51.9 ms after its commit, and the test allows 50 ms. The ten were 22.4, 51.9, 46.2, 16.3, 7.2, 17.4, 15.6, 12.1, 20.6 and 4.9 ms.
- The landers' flakes list of 2 November, 21:07, took it off the list to watch it for a week: a failure is looked into, not retried.

**Looked into:**

- The branch isn't on its path. The test's bed is a store, the feed and its watchers: `Store::apply` in the Live lane, with no converter, sweep or `status`. The branch's one change there is `groups`' pattern, `Drained(_) | Quarantined { .. }`, and a lone `Change::Message` is a group of its own with or without it.
- It didn't wait to coalesce. The test sets the coalescing waits to 100 ms so that a wait can't hide in a debug build's slowness, and 51.9 ms is about half of that.
- It has failed in six audits before this branch:
  - feed-names (26 October), with one push at 72.7 ms at a load of 67;
  - folders (26 October);
  - failure-states (28 October), with an outlier of 273 ms;
  - typing, keepalive and chat-actions (29 October).
- It passed in the media-out landing (2 November).
- Alone, it passes three times of three, at a load of about 7.

**Fix:** nothing in this branch. It's a sighting for the watch: a loaded debug build, over the bound by 1.9 ms.

### O1, O4 and O5

- O1, a history blob whose walk panics, is main's behaviour and stands as the plan's follow-up.
- O4 stands as a follow-up for main's merge rules, and O7 adds a case to it.
- O5 stands: a-named passes every test it reaches.

## Measurements

**Probe P-AS** (in `sync/tests.rs`, with an audit-only hook in `convert_row`'s app-state arm that panics, once the row's changes are made, for an index naming a chat whose JID starts with `3EB0PANICS`):

- passes at the head: inbox `[3]`, quarantine `[("inbox", "3", "converting it panicked: the conversion of 3EB0PANICSAPPSTATE@s.whatsapp.net panics once it has made its changes, as a test asks")]`, `app_state` holding only the later pin's key, the older chat pinned on top, one panic; then, mended, the inbox and the quarantine empty and both keys kept;
- fails with 1 and 3 at its wait for the rows after it (the spin, and the row drained); with 4 and 6 at "kept in quarantine, as any row"; with 5 at "nothing of it kept raw", the panicking pin's key kept.

**Probe P-ECHO** (in `store/write/chat_actions.rs`'s tests, with their helpers). jaw pins a chat at `AT`. Its echo is inbox row 6, and the phone's unpin, at `AT + 5 s` and taken by WhatsApp after the pin, is row 7. Both are in when WhatsApp answers, so `echo_by` is 7. The state after each case, as (pinned at, what `app_state` keeps for the index, a wait on):

| Case | State |
| --- | --- |
| The echo converts, then row 7 | (0, the phone's unpin, no) |
| The echo kept for a panic, then row 7 | (`AT`, the phone's unpin, no) |
| Then the echo converts at the next start | (`AT`, jaw's pin's echo, no) |
| Main: an echo that doesn't decode, then row 7 | (`AT`, the phone's unpin, no) |

**Each switch at the rebased head**, one test process each from `crates/jawd`:

| Switch | Rule | Fails at |
| --- | --- | --- |
| 1, 2 | the rows' `catch_unwind` | "waited too long for the row after it"; "the row after it is converted, and nothing of it is kept" |
| 3, 3b | the row that panicked stays | "each stays in the inbox"; "it stays raw" |
| 4, 4b, 6, 6b | kept in quarantine, under the name its `Drained` clears | "each is kept in quarantine, saying why"; "at start 0" |
| 5 | nothing kept of a row that panics | "nothing of either panicking message is kept"; "the notification's offer, and not the one's that panicked" |
| 7 | `groups` ends at a `Quarantined` | "each under its own name, in the Live lane" |
| why, no-str, no-string | T1 | "each is kept in quarantine, saying why"; "at start 0" |
| j-changes, j-offers, j-pictures, j-name, j-name-first, j-thumbnails | T2 | the batch test, each at its field's own assertion |
| spin, spin-h | T3 | "each is tried once in this start": 444 and 412 panics for 1 each; "the pass over the history kept raw didn't end", after 23,515 panics |
| a-once | T3, once more at the next start | "and once more at the next start": (1, 1) for (2, 2); "tried once in each start": 1 for 2 |
| o2, a-o2-flat | O2 | "tried 10504 times in a second"; "tried 80 times in a second" |
| d1-old, d1-parts, d1-done, d1-status | D1 | "the blob it names stays in the spool"; "what's done with goes" for the next two; O6's count, (0, 0) for (3, 215) |
| a-read-err | T4 | T4's test: "a blob whose rows aren't in stays" |
| a-status-pb | O6 | the D1 test, at O6's count: (2, 204) for (3, 215) |
| a-named | a blob an offer names | none fails (O5) |
| a-as-twice (this round's) | the landing: the mutation's changes only through `held_back` | `send::tests`' held-back pin test, "held back: jaw's pin goes after it, so it's how every device ends"; its pin-toggling test, "as asked last, the earlier echoes changing nothing", (true, 12) for (true, 7); the whole of `send::`, 160 passed and 8 failed. It passes `sync::tests`' app-state test and P-AS, which hold no wait |

**The suite at the head** (`cargo test --workspace --no-fail-fast`), in 57 minutes. Its failures came at a load of about 70. `cargo fmt --all --check` passes.

- Every crate passes but jaw-voice and jawd's unit tests. jawd's installed, starting, stopping (four) and upgrading tests pass, and no fake engine was left running.
- jaw-voice: 35 passed and 1 failed, its recording-end test (on the list), which "waited in vain" with a `PlaybackFailed` among what it saw. Alone, it passes three times of three, in 1.2 to 1.9 s.
- jawd's unit tests: 1,149 passed, 5 failed and 19 ignored, in 2,861 s. Each failure passes alone three times of three, at a load of about 7:

| Test | The list | In the suite | Alone |
| --- | --- | --- | --- |
| `feed::tests::a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends` | on it | its `status` unanswered when its read ended | 0 to 1 s |
| `feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` | on it, a lane test | "the lane and the watch waited for the long requests" | 4 s |
| `media::tests::a_save_waiting_for_its_file_holds_up_nothing_else_on_its_connection` | on it | "status came first: None" | 0 to 1 s |
| `sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push` | on it | "nothing went missing: Elapsed" | 6 to 7 s |
| `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce` | off it, watched (O8) | one push of ten at 51.9 ms, for 50 | 2 to 3 s |

- Two of them wait on a `status` answer, to which the branch adds its spool count (`sync/fetch.rs:194`, called at `state.rs:252`). That's one `read_dir` of the home's spool, which in these tests is empty or missing.

**Landing:**

- `git merge-tree --write-tree b33dcba1 11304aa7` is clean. A merge commit of its tree, made in the audit's clone, differs from main by exactly the branch's diff, and from the head by exactly main's four files under `crates/jaw`. No commit of the branch touches `crates/jaw`.
- jaw's tests on the merged tree (`cargo test -p jaw`, at a load of about 9): 354 passed and 2 ignored, main's four new view tests among them, and its 19 integration tests, in 2 minutes 22 seconds.

## Landing

- **Merge or rebase onto main `b33dcba1`.** Nothing conflicts: main moved only in `crates/jaw`, which the branch doesn't touch, and jaw's tests pass on the merged tree.
- **The trailer** `Data-Audit: docs/audits/2026-11-02-drain-panic.md` goes on the eight commits that touch the data layer: the change and every fix, `89cf60e2` to `11304aa7`, each under `crates/jawd/src/sync` or `store/`.
  - Not the plan's, `9907b531`, nor this note's: both are docs only.
  - A `rebase -x` that amends every commit would put it on the plan's too, so amend only those eight.
- **Commit this note** in the branch. `make check-audit` reads its last `Verdict:` line, which is this round's.
- **No migration.** No schema, index or trigger changes. The one new statement reads `history_kept` by its unique index.
- **Beyond the gate, the lander re-checks nothing,** unless main moves under `crates/jawd/src/sync`, `store/` or `convert/` before the push. Then round two's short list again: `convert_row`'s arms against main's loop, the `Change::Quarantined` producers, `Converted`'s fields, and the change's tests, T4's and main's chat-action tests.
- **If the gate's suite fails the feed's coalescing test again,** the landers' rule is to look into it rather than retry it. O8 is this round's look: the branch isn't on the test's path.

## Verdict of round three

O1, O4, O5 and O7 stand as follow-ups, written down, and O8 goes to the coalescing test's watch.

Verdict: approved
