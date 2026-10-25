# Audit: nothing is converted until jaw.db keeps the account's JIDs

Range: `96332e2..e59ed75` in `first-run-me` (`0d9774d`, the fix; `e59ed75`, the first-run guide), 25 October 2026. Auditor: database-expert. Line numbers are at `e59ed75` unless they say otherwise.

`origin/main` moved during the audit, from `96332e2` to `c2c60f6`, with `linked-at-hello` (`0fe88ec`, `405e6ed`, `e0953d0`, its audit `a9cea4d`) and two commits that touch only `jaw` and `jaw-client`. So this branch is the second to land. The rebase isn't clean, and its resolution is judged below with the change.

The machine was busy throughout: one-minute load averages of 20 to 75 on its 20 cores. Every timing says the load it was taken at.

## Summary

The fix is sound, and it closes M1.
- jawd now converts nothing until the account's JIDs are committed in jaw.db. The JIDs reach the converters only from `sync_state` at start, or after the commit of the `Remember` that a `CONNECTED` brings.
- Measured on a version-1 `jaw.db` of the spike's shape, main loses every one of the account's own reactions, finds no mention of it, and leaves its group messages at "sent". The branch keeps and finds them all.
- The rows wait whole and in order, and they survive a clean stop and a crash while waiting.

A database that already has `me` behaves as before: the JIDs are restored before the converters start, so nothing waits.

The new test fails on main, and every part of the fix that can be reached in practice fails a test when removed. One part has no test: `remember`'s failure branch (retrying, counting a quarantine as a failure, publishing only after the commit). It's reachable only through a non-transient failure of an `INSERT OR REPLACE` into `sync_state`, which the linked-at-hello audit's H1 already called theoretical. I injected that failure in a scratch test, and the branch behaves as designed. The test is given as follow-up H1.

Findings:
- No data loss, no inconsistency, and nothing that breaks a performance budget.
- Six hygiene items, H1 to H6, may stand as follow-ups.
- The rebase onto `c2c60f6` needs the resolution below. Tried in a scratch clone, it passes every jawd test and the end-to-end run.

## The questions asked

### Can a row be lost, converted with the wrong account, or stuck forever?

**The commit against the converters.**
- `remember` (`sync/mod.rs:105-122`) applies the `Remember` in the live lane. It calls `send_replace` only after `apply` has returned `Ok` with nothing quarantined, so only once the commit is done.
- The converters' writes go through the same single writer after that commit, so `group_reached` (`store/write/outbox.rs:472`, reading `sync_state.me` at `:462`) sees the JIDs the converters used.
- Even with the JIDs published before their commit, the writer's live queue would order the `me` write first in practice: that mutation passes the new test 10 of 10. The ordering in `remember` is what makes it hold when the commit fails.

**A crash between them.**
- A crash after `CONNECTED` but before the `me` commit: nothing has been converted, since conversion starts only after the commit, and the next start waits again.
- A crash after the commit: the next start restores the JIDs and goes on at once. Measured in run C below: `kill -9` with 49,013 history rows left, then a restart with no network finished them, ending identical to the uninterrupted run.
- A power cut after the commit, which is `NORMAL`: the WAL recovers only a prefix of commits, and checkpoints copy only synced frames. So the conversions that followed the `me` row survive only if that row does, and nothing converted with the JIDs is left without them in the file.

**An engine restart.** A restarted engine's `CONNECTED` writes the same JIDs again, which changes nothing. During run A the supervisor restarted the silent engine every 10 s. The converters kept waiting, and the log said why only once each.

**An account change.**
- No row is converted with the wrong account as long as the stored JIDs are the linked account's.
- After a logout and a different account linked into the same folder, `me` still holds the previous account's JIDs until the new account's `CONNECTED` commits. A row of the new account that arrives in that window is converted with them. That can happen: whatsmeow dispatches `Connected` from a goroutine of its own after the post-connect passive IQ, while messages flow (`connectionevents.go:187-205` in the pinned whatsmeow).
- It's the same on main, where `live.me` also held the previous account until `CONNECTED` was handled. The change widens the window by one live commit. It needs a second WhatsApp account in the same jaw folder, which the owner's first run doesn't involve. H3.

**Quarantine of the `Remember`.**
- A quarantined or failed `Remember` is retried, at 10 ms doubling to 1 s, and the JIDs aren't published meanwhile.
- The quarantine row is upserted by `(source, ref)`, so retries don't grow it.
- Measured with a trigger that refuses the `me` row: nothing converts while it's refused, and everything converts once it's dropped (H1).
- If the write could never land, the rows would wait, counted in `jaw status`, with the failure logged on each try. Nothing is lost. Converting without the JIDs is what loses data (M1), so waiting is the safe way to fail.

**Stuck forever?** Only while no engine ever says `CONNECTED` on a jaw.db without `me`, or while the `me` write can never land (theoretical). In both cases the rows stay whole and counted.

**Nothing else can make a waiting converter fail.** `stored_me`'s `expect` (`sync/mod.rs:139`) would panic only if the watch's sender were dropped. Nothing replaces `Live` or `live.me` after it's built: I checked every assignment, on the branch and after the rebase.

### A database that already has `me`

- `run` restores the JIDs before it spawns the converters (`sync/mod.rs:152-154`). After the rebase, `starting` restores them, as `main.rs` builds the live state with it. Either way `stored_me` returns at once, and every later start behaves as before.
- Measured: run C's offline restart, and the rebased build's third run, converted with no engine at all.
- The one difference: a `CONNECTED` whose JIDs differ from the stored ones (a LID learned, say) reaches the converters one live commit later than on main. Meanwhile they use the stored JIDs, which are also what the store holds, so the two agree.

### `linked-at-hello`: what the second to land must keep

This branch is now second. A trial rebase onto `c2c60f6` conflicts in three places in `sync/mod.rs`, and needs two edits beyond them.
1. **`starting` restores the JIDs through the watch:** `live.me.send_replace(convert::Me::new(…))`, not `live.me = …`, which no longer compiles.
2. **`run` restores nothing,** as on main, since `starting` does.
3. **The `CONNECTED` arm computes the JIDs without touching `live.me`.** Keep `let me = connected.then(|| convert::Me::new([connection.jid.clone(), connection.lid.clone()]));` beside main's `account`. Git already puts `remember(&store, &live, me)` after main's awaited account write, and `apply_soon` goes.
4. **`sync/tests.rs`** (`an_account_linked_at_the_last_connection_starts_linked_and_connecting`) reads `live.me.borrow().jids`.
5. **`send/tests.rs`'s `Running::timed`** builds its live state with `crate::sync::starting(&store, engine)`. With `Arc::default()` and no restore in `run`, the account linked earlier has no JIDs, the converter waits forever, and `a_message_waits_pending_for_as_long_as_whatsapp_cant_be_reached_and_goes_once_it_can` fails (measured).

With that resolution:
- jawd's 255 tests, `starting.rs` and `stopping.rs` all pass.
- `starting.rs` now pins the restore through the binary: without it, "the message left in the inbox was never converted".
- linked-at-hello's guarantees hold: hello says linked from `account`, an `UNLINKED` after linked is `logged_out`, and `link.logout` answers after a synced commit. Its tests, the write-lock one included, pass, and none of those lines changed.
- The trial is `c90a6d7` in `/tmp/audit-frm` (branch `trial-rebase`), and its `crates/jawd` tree is `91cc6d2`.

**What must not happen in the rebase:**
- taking main's side of the `CONNECTED` arm and turning `live.me = …` into `live.me.send_replace(…)`. That compiles, passes every test but H1's, and publishes the JIDs before their commit;
- assigning `live.me` anywhere after `run` starts.

A resolution that differs only in how the send harness gets the JIDs (a restore kept in `run` instead of edit 5) is equally sound. Anything else in those lines should come back for a look.

### The owner's first run, end to end

**The fixture:** a scratch test wrote a version-1 `jaw.db` with migration 1's text, of the spike's shape (90 MB; the owner's is 73 MB).
- **224 inbox rows:** 20 history notifications, 4 groups, push names and app state, the account's own reactions (10 of them before their message), mentions of the account, and the account's group messages with receipts from all members or all but one.
- **84,394 history rows in 450 chats:** 997 edits, 28 empty shells, 15% images with 2 to 6 KB thumbnails, 526 own reactions and 9 mentions of the account.
- **The account** is the demo engine's (`351910000001@s.whatsapp.net`). No `session.db` existed anywhere, and no Go engine ran.

The runs, read with `jawd status` and `sqlite3 -readonly`:

| Run | jawd and engine | What happened |
| --- | --- | --- |
| A | branch; a silent engine, as with no network | Upgraded to version 8 and took `jaw.db.before-v1` (88.4 MB). Nothing converted. The log said why once per converter, and status said "waiting 224 events to convert, 84394 messages of history to convert". My harness then killed jawd while it waited: every row intact. |
| A2 | branch; silent | A clean stop in 23 ms while both converters waited: `clean=1`, rows intact. |
| B | branch; `--engine demo` | Converted the 224 inbox rows, and the 84,394 history rows in 8.0 s (10,592 a second, debug build, load 21 to 28), with every expected value below. |
| C | branch; demo, `kill -9` mid-history, then silent | 49,013 history rows were left at the crash. The restart with no engine finished them in 4.8 s, with the same values as B. |
| M | main (`96332e2`); silent | Converted everything with no JIDs: 0 of 550 own reactions (none pending or quarantined), 0 of 15 mentions, all 8 group messages "sent". This is M1. |
| R1-R3 | rebased (`c90a6d7`); silent, demo, silent | As A2, then as B (8.8 s), then a later start with no network that says "linked as Pedro", connecting, and has nothing left to wait for. |

What B (and C, and R2) left, against what the fixture planted:
- 83,486 messages in 452 chats; `history_raw` empty; `history_kept` 1,025 (997 edits and 28 shells).
- All 550 own reactions, under the account's JID. Others' reactions: 79.
- `mentions_me` on all 15. Chats: 95 unread and 2 mentions, the two mentions that came after the account's last message.
- Each "all read" group message "read", and each "all but one" "delivered". All 10 direct messages "read".
- Nothing pending. 20 inbox rows left: the history notifications, quarantined with their rows kept, since the demo engine can't download.
- `sync_state.me` written (and `account`, on the rebased build).

### A start with no network converts nothing until the engine connects: acceptable?

Yes.
- It happens only while jaw.db has no `me`:
  - the one-off upgrade from version 1;
  - a fresh folder, which has nothing to convert before linking;
  - a jaw.db whose only `me` write a power cut took;
  - a restore of the backup taken before the upgrade.
- Every later start converts at once, as runs C and R3 show.
- Nothing is lost while waiting, the log says why, and status counts the rows.
- Holding everything rather than only what needs the JIDs is right. The store reads the JIDs for group ticks, and a partial hold would break the order the merge rules see.
- For the owner, it means an empty chat list until the engine connects. His first run will be online.

### Should the JIDs from the Go engine's `CONNECTING` be taken too?

As a follow-up, only when `CONNECTING` carries both the phone JID and the LID.
- `CONNECTING` is sent before `Connect()`, with `Store.ID` and `Store.LID` from session.db (`engine/internal/wa/engine.go:112`, `:236-240`). So it comes even offline, and names the linked device.
- But `Store.LID` is empty in a session.db that hasn't learned it yet; whatsmeow fills it from `<success>` (`connectionevents.go:176-184`).
- Converting with the phone JID alone would miss mentions by LID, which LID-addressed groups use, and `mentions_me` is decided at conversion, not revisited.
- A device logged out while jawd was off still names the old account then. That's right for the rows it left.
- Taken through the same `remember`, the order would hold. It isn't needed for the owner's first run, and I didn't open his session.db to see whether it has a LID.

## Findings

### Data loss

None in the change. M1 is closed: measured above, and pinned by `what_a_version_1_jaw_db_holds_waits_for_the_accounts_jids_and_is_converted_with_them` (`sync/tests.rs:344`), which fails on main.

### Inconsistency

None.

### Unpredictable performance

None that breaks a budget.
- The change adds no SQL statement and touches no schema, so there are no plans to pin. `store/`, the fingerprint and `migrations.lock` are untouched.
- Each converter batch adds a mutex lock, a watch subscription and a clone of one or two JIDs.
- At the upgrade, the conversion now starts once the engine connects, so it overlaps the catch-up and the app-state refetch. The writer's priorities already protect live writes from background batches, and in run B the WAL peaked at 12,365 frames (about 51 MB), under the 64 MiB bound. The background lane waited for the checkpointer 12 times, 0.3 s in all.
- On the owner's real rows the sync audit measured 25.6 s for this conversion. Expect about that after the engine connects.

### Tests: rules the change touches without a test that fails without them

None that blocks.
- The change's rule, that nothing is converted before the JIDs are committed and everything after with them, is pinned by the new test. Mutations of each reachable part fail it, or a test that existed; see Measurements.
- The unreachable parts are H1.

### Hygiene (may stand as follow-ups)

- **H1. `remember`'s failure branch has no test** (`sync/mod.rs:108-118`).
  - With one attempt that publishes whatever happened, or with a quarantine counted as kept, all 247 + 2 tests pass.
  - It's reachable only through a non-transient failure of the `Remember`, since the writer retries every transient one itself (`store/mod.rs:854-871`).
  - **Fix:** a test that, after `Store::open`, creates `CREATE TRIGGER t BEFORE INSERT ON sync_state WHEN NEW.key = 'me' BEGIN SELECT RAISE(ABORT, 'refused'); END` from an outside connection; puts a group, an own group message with every member's read receipt and an own reaction in the inbox; sends `CONNECTED`; and checks that after 1.5 s nothing has converted, `me` isn't kept and `("remember", "me")` is quarantined once. Then it drops the trigger, and checks everything converts with the account's JID and the message is "read".
  - Written in the scratch clone, that test passed 3 of 3 on the branch and failed under each mutation: 3 of 3 with one attempt, 2 of 2 with a quarantine counted as kept, 1 of 1 with the JIDs published before their commit.
- **H2. Two `remember` tasks in flight publish in the order they finish, not the order they committed,** and a retrying earlier one can land after a later one, writing the earlier JIDs back.
  - It needs two `CONNECTED`s within one live commit, or H1's theoretical failure across an account change. Then `me` would be stale, even another account's, until the next `CONNECTED`.
  - **Fix:** a generation number checked before each write and the publish, or one long-lived task that always writes the newest JIDs it was given.
- **H3. After an account change, rows of the new account that arrive before its JIDs are committed are converted with the previous account's.**
  - The same on main, since `me` is never cleared; above.
  - **Fix**, if jaw is to hold two accounts' rows: tell which account each row came for, since clearing `me` at a logout would convert the old account's waiting rows with the new account's JIDs instead.
- **H4. A `me` refused and then kept leaves its quarantine row,** counted in status. This is the sync audit's H2, standing.
- **H5. `jaw status` counts the waiting rows but doesn't say why;** only the log does. "waiting for the engine to say which account is linked" beside the counts would tell the owner what he's looking at.
- **H6. The test's `VERSION_1`** (`sync/tests.rs:327`) is a hand copy of migration 1. `schema::MIGRATIONS[0]` (exposed to tests), or the schema tests' `at(…, 1)`, would keep it exact.

**Also checked:**
- **The fetcher's wait** (`sync/fetch.rs:174`) can't be reached with no JIDs: offers come only from the inbox converter, after its own wait. It's harmless, and keeps the rule local.
- **`docs/first-run.md`'s new sentence** is accurate: the log says why, once per converter, and status counts the rows.

**Not this change:** with `JAWD` set, `jaw`'s wire tests fail two tests on main with main's own jawd, as with the rebased one: `on_a_real_jawd_each_message_action_goes_with_its_chat_and_a_key_of_its_own_and_is_done` and `on_a_real_jawd_killed_between_forwarding_and_answering_jaw_asks_again_with_its_key_and_there_is_one_copy`, both new in `c16fc31`.

## Measurements

**Method:**
- A scratch clone, `/tmp/audit-frm`, of the worktree at `e59ed75`, with `upstream` = `~/src/jaw`.
- `CARGO_TARGET_DIR=/tmp/audit-frm-target`, an APFS clone of an earlier audit's debug target. Debug builds.
- Mutations and scratch tests were made in the clone, one at a time, and undone with `git checkout` after each.
- Homes were `/tmp/audit-frm-home-*`. The silent engine was `/tmp/audit-frm-engine/silent`, a shell loop given as `JAW_ENGINE`.
- Each jawd was stopped by its PID. The harness's first jawd was killed when its tool call ended, which served as a crash while waiting. No process I didn't start was touched, and none of mine was left running.
- Nothing in `~/src/jaw-firstrun` was written but this note. Nothing under `~/Library/Application Support/jaw` was opened, no `session.db` existed in any folder used, and no Go engine ran.

**The suites:**
- At `e59ed75`: 247 passed and 2 ignored in 105 s (load 47 to 59), and `stopping.rs` 2 of 2.
- The trial rebase: 255 passed and 2 ignored in 94 s, `starting.rs` 1 and `stopping.rs` 2 (load 22 to 38).
- `jaw` (`cargo test -p jaw` with `JAWD`): 198 of 200 with the rebased jawd, and the same 198 with main's own jawd at `c2c60f6`, twice each (load 51 to 74). A first run at about 50 had 10 failures, 8 of them in scripted tests that don't use jawd, and they didn't come back.

**The new test:**
- On main's sync code it fails 3 of 3 at its "nothing is converted before" check.
- With that check removed, it fails on content: no reaction kept, none pending or quarantined, no mention, and the group message "sent".
- 24 of 24 as 8 copies at once, at load 54.

**Mutations at `e59ed75`,** each alone:

| Mutation | Result |
| --- | --- |
| the history converter doesn't wait | the new test fails, 3 of 3 |
| the inbox converter doesn't wait | the new test fails, 3 of 3 |
| `remember` never publishes | the new test fails, 2 of 2 ("waited too long") |
| `run` doesn't restore `me` | `a_message_waits_pending_…_goes_once_it_can` fails; the rest pass |
| one attempt, published whatever happened | all 247 + 2 pass (H1) |
| a quarantine counted as kept | the scratch refused-`me` test fails (H1) |
| the JIDs published before their commit | the new test passes 10 of 10; the scratch test fails |

**Mutation of the trial rebase:** with `starting` not restoring `me`, three tests fail: `an_account_linked_at_the_last_connection_starts_linked_and_connecting`, the send test above, and `tests/starting.rs`.

**End to end:** the table above. History converted at 10,592 a second on the branch (load 21 to 28) and 9,542 on the rebased build (load 33). The WAL peaked at 12,365 frames.

**The audit check:**
- `scripts/check-data-audit.py` names only `0d9774d` (`5b8db7b` once rebased), for `send/tests.rs`, `sync/drain.rs`, `sync/fetch.rs` and `sync/mod.rs`.
- `e59ed75` touches only `docs/first-run.md`.

## Verdict

Approved. M1 is closed, as the owner's first run on a version-1 `jaw.db` shows end to end. Nothing marked data loss or inconsistency remains, and every rule the change touches that can be reached has a test that fails without it.

To land:
1. Rebase onto `origin/main` (`c2c60f6`) with the resolution under "what the second to land must keep": `starting` restoring through `send_replace`, no restore in `run`, the `CONNECTED` arm leaving `live.me` alone with `remember` after the account write, `live.me.borrow()` in the test, and the send harness built with `starting`. Run jawd's tests, with `starting.rs` and `stopping.rs`.
2. Give every data-layer commit the trailer `Data-Audit: docs/audits/2026-10-25-first-run-me.md`, which the rule's `git rebase … -x` does.
3. Commit this note in the branch, and run `make check-audit`.

A resolution of the `CONNECTED` arm or `starting` other than this, beyond how the send harness gets the JIDs, comes back for a look.

H1 to H6 may stand as follow-ups, with H1's test the first worth adding. `CONNECTING`'s JIDs, taken only when both are present, is an optional follow-up.

Verdict: approved
