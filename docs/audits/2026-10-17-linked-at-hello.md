# Audit: jawd knows at start that its account is linked

# Third audit: 050a9c4, T3's test

Range: `ba1c922..050a9c4` in `linked-at-hello` as rebased (`d42e4c4`, `8e67d9b` and the new `050a9c4`), 25 October 2026. Auditor: database-expert. The two audits before are kept below as written, with their verdicts at the end, and this audit's verdict is the note's last line. Line numbers are at `050a9c4`.

`origin/main` has moved once more, to `96332e2`, which adds `crates/jawd/src/sock.rs` to the audit's paths (the second audit's H13). The machine was busy again, with load averages of 36 to 52 on its 20 cores throughout.

## Summary

T3 is closed. The logout test, `a_logout_jawd_asks_for_is_answered_only_once_the_account_is_forgotten_with_a_synced_commit` (`sock.rs:504-557`), now fails every time for each way the second audit found it passing:
- the forgetting at `NORMAL`: 0 synced commits, where it expects 1;
- the answer first and the forgetting after it: answered while jaw.db's write lock is held;
- jawd's own account left linked: a later hello says linked.

The rebase changed nothing judged here. Both commits are patch-identical to the ones audited, and the tree is byte-identical to the trial rebase the second audit measured. The new commit changes only that test, one attribute and two lines of docs.

One new hygiene item, H16: the test counts synced commits after the answer has reached its own task, so a synced commit made just after an answer can be counted. It takes a contrived mutation to show, and a small change to the test fixes it.

H14 needn't be done before landing; why is below. Nothing marked data loss or inconsistency remains in the change, and every rule it touches has a test that fails without it.

## T3, checked

**What the test does now:**
- It holds jaw.db's write lock from a connection of its own (`BEGIN IMMEDIATE`, `sock.rs:532-533`), and reads `synced_commits` (`:534`) before it asks.
- It checks that no answer comes within 400 ms (`:543-544`), then lets the lock go (`:545`) and expects the answer, `{}`, within 5 s.
- It then expects exactly one more synced commit (`:548`), the `logged_out` push with its reason, the key empty, `starting` unlinked, and a new connection's hello unlinked with no account (`:555-556`).

**Why the lock pins the order:**
- `logout` (`sock.rs:338-349`) answers only once `sync::logged_out` returns, and that waits on `apply_durable` (`sync/mod.rs:144`).
- While the lock is held, the writer's `BEGIN IMMEDIATE` waits in its busy handler. So an answer can come then only if something answers without that commit.
- The count comes from the PRAGMAs the commit ran under (`commits_synced`, `store/mod.rs:122`, counted at `:1252-1260`), not from what was asked for.

**My mutations,** each alone:

| Mutation | The logout test |
| --- | --- |
| `logged_out` forgets with `apply(Lane::Live, …)`, at `NORMAL` | fails 3 of 3: 0 synced commits, expected 1 |
| `logout` answers, and forgets 50 ms later on a task of its own | fails 3 of 3: answered while the lock was held |
| `logout` answers, with the forgetting spawned and no delay | fails 5 of 5, the same |
| `logged_out` leaves jawd's own account linked | fails 3 of 3: the later hello says linked as Pedro |
| the writer commits a durable live write at `NORMAL` and answers it | fails 3 of 3: 0 synced commits |
| `logged_out` commits at `NORMAL`, then spawns a synced write of the same key | passes 6 of 20 (H16) |
| `logged_out` sets jawd's account and pushes before its commit | passes 5 of 5, and needn't fail (below) |
| the sync loop's forgetting at `NORMAL` | passes, as all 254 + 1 + 2 jawd tests do (H14) |

**The push before the commit needn't fail it:**
- No rule orders a push after its commit, and the sync loop pushes first by design (`sync/mod.rs:238`, then the write at `:246-258`).
- The answer still waits for the synced commit. And the connection that asked still gets the answer before the push, since `one()` writes the reply before it reads pushes again (`sock.rs:99-111`).

**Under load:** the test alone passed 20 of 20 in a row, in 0.56 to 0.62 s each (0.4 s of it the held lock), at load averages of 45 to 48. It passed 24 of 24 as 8 copies at once, at about 49, and in the whole suite's run, at 36 to 39.

## The rebase

- `git range-diff` gives `=` for both: `d42e4c4` is `f739cb2`, and `8e67d9b` is `62476a0`, patch for patch.
- `8e67d9b`'s tree (`f00899c`) is, byte for byte, the tree of the second audit's trial rebase onto `ba1c922`. That's where the second audit measured the suites, T1's lock test and P1 on main's writer, so those judgements stand.
- `050a9c4` adds the test's changes, `#[value(hide = true)]` (`main.rs:56`), the README's row (`protocol/README.md:229`) and the guide's line (`docs/first-run.md:43`), and nothing else.
- **Onto `96332e2`:** a trial rebase is clean, with every patch identical and a Rust tree identical to `050a9c4`'s.
  - With H13 on main, the audit check counts all three commits as the data layer, `050a9c4` too, through `sock.rs`. Without trailers it names all three.
  - So each needs the `Data-Audit` trailer, which the rule's `git rebase origin/main -x …` gives every commit.

## Findings

### Data loss

None in the change. M1, on main, is still open (`ba1c922..96332e2` changes only the audit script), and still bears on the owner's first run.

### Inconsistency

None.

### Unpredictable performance

None. The change adds no statement, and the writer is main's, which the second audit measured (P1).

### Tests: rules the change touches without a test that fails without them

None. T3 is closed, as above.

### Hygiene (may stand as follow-ups)

- **H16. The logout test counts synced commits after the answer has crossed to its own task.**
  - **Where:** `sock.rs:548` reads `store.storage().synced_commits` once the answer has gone over the socket to the client's thread, and from there over a oneshot to the test's task.
  - **What passes:** with the forgetting at `NORMAL` and a synced write spawned after it, the answer follows an unsynced commit, but the spawned commit can land before the count is read. That passed 6 of 20 runs.
  - It's contrived. The change's plausible regressions (at `NORMAL`, not awaited, answered first, left linked) all fail every time.
  - **Fix:** read the count on the client's thread as soon as `call` returns, and send it with the answer. Tried in the scratch clone, that mutation failed 20 of 20, and the real code passed 20 of 20.
- **H14. The sync loop's forgetting is synced but still unpinned. It needn't be done before landing.**
  - With `apply` in place of `apply_durable` at `sync/mod.rs:250`, all 254 + 1 + 2 jawd tests pass on the rebased head, as at `62476a0`.
  - **Nothing is acknowledged after that commit.** The engine sends `LOGGED_OUT` and `UNLINKED` with `e.send`, fire and forget (`engine/internal/wa/engine.go:202-203`, `:244-246`); only `keep` waits for an ack (`:163-167`). And interfaces are pushed the state before the write (`sync/mod.rs:238`).
  - **The commit narrows a window that can't be closed,** since jawd can die before it takes the event. If a power cut loses it, jaw.db says linked with no device: the next start says linked and connecting until the engine's `UNLINKED`, which F1's tested path turns into `logged_out` and a synced emptying (`sync/tests.rs:1307-1335`).
  - **It becomes required** if the logout's ending ever goes through the loop (one fix for H10), since the answer would then follow that commit.
  - **Fix, when next in that file:** in `an_account_logged_out_while_jawd_was_off_ends_up_logged_out_and_the_next_start_unlinked`, read `store.storage().synced_commits` before the second `after(…)` and expect exactly one more after it.
    - In both of its cases the forgetting is the only synced commit in that `after(…)`: `CONNECTING` writes nothing, and `run` writes no clean marker.
    - Tried in the scratch clone: it passed 10 of 10, and failed 5 of 5 with the forgetting at `NORMAL`.
- **H7, done.**
  - The README says the connection that asked gets its push just after the answer, which is what `one()` does (`sock.rs:99-111`), and the test no longer names an order.
  - The test doesn't pin the order, since `call` keeps pushes. That's fine: nothing is acknowledged by it.
- **H12, half done.**
  - `jawd --help` lists only `go` and `demo`, and so does the error for a value it doesn't know. `jawd --engine demo-unlinked status` still parses: on an empty scratch folder it said jawd isn't running.
  - Refusing a demo engine on a folder that holds `session.db` stands as a follow-up.
- **H13, done on main** (`96332e2`, `scripts/check-data-audit.py:22`). What it means for landing is above.
- **`docs/first-run.md:43`, corrected.** It now matches the second audit's measured first run: "not linked" at the first start until the engine reports, since a version-1 `jaw.db` has no `account` key, and linked at later starts.
- H8, H9, H10, H11 and H15, and the first audit's F3 and H5, stand as written.

## Measurements

**Method:**
- A scratch clone, `/tmp/audit-lah3`, of the worktree at `050a9c4`, with `CARGO_TARGET_DIR=/tmp/audit-lah2-target` and `JAW_HOME=/tmp/audit-lah3-home`. Debug builds, on the M1 Ultra.
- Mutations and scratch tests were made in the clone, one at a time, and undone with `git checkout -- .` after each.
  - A first try of the spawned-synced-write mutation landed on top of the linked-account one, since the edit ran beside the undo. Its runs were discarded, and it was run again alone for the table.
- Every database was a tempdir. Nothing in `~/src/jaw-sync` was written but this note, and nothing under `~/Library/Application Support/jaw` was opened.
- No Go engine was built or found, beside jawd or on `PATH`, and none ran. No jawd daemon was started by hand (only `jawd --help` and `jawd status` ran), and no process was stopped.

**The suites at `050a9c4`:**
- `cargo test -p jawd`: 254 passed and 2 ignored, in 101 s, at load averages of 36 to 39. `starting.rs` 1 passed, `stopping.rs` 2.
- `JAWD=<its jawd> cargo test -p jaw`: 191 passed, at about 43.

**The logout test:** 20 of 20 alone and 24 of 24 as 8 copies at once, with the mutations in the table above.

**H16's fix, tried:** with the count read on the client's thread as `call` returns, the real code passed 20 of 20 and the spawned-synced-write mutation failed 20 of 20, at about 52.

**H14's check, tried:** 10 of 10 on the real code, and 5 of 5 failing with the loop's forgetting at `NORMAL` ("on the phone: forgotten with a synced commit", 0).

**H12:** `jawd --help`; `jawd --engine nonsense` ("possible values: go, demo"); and `jawd --engine demo-unlinked status` on the empty scratch folder, which parsed, said jawd isn't running, and left the folder empty.

**The audit check:** on the trial rebase onto `96332e2`, without trailers, `scripts/check-data-audit.py upstream/main trial` names all three commits, the third for `crates/jawd/src/sock.rs`. With this note committed on top and every commit given the trailer by the rule's `git rebase … -x`, it passes.

**Plans:** the change adds and changes no statement. There's no schema change, so the fingerprint and `migrations.lock` are untouched.

# Second audit: 62476a0, the first audit's fixes (kept as written)

Range: `2822c59..62476a0` (`origin/main..HEAD` as the branch stands; `origin/main` has since moved to `ba1c922`) in `linked-at-hello`, 25 October 2026. Auditor: database-expert. The first audit is kept below as written, and this audit's verdict is the note's last line. Line numbers are at `62476a0` unless they say otherwise.

The machine had just been restarted and was busy throughout, with load averages from 36 to 91 on its 20 cores. So every timing below is high, and says the load it was taken at.

## Summary

The first audit's four blockers are fixed, and each fix fails a test when it's removed:
- **F1:** an engine with no device after jawd said linked is now reported as `logged_out`, with the reason "this device isn't linked any more". An interface never told linked still hears `offline`. Pinned in jawd and, against a real jawd running `--engine demo-unlinked`, in jaw.
- **F2:** `link.logout`, once the engine has logged out, empties the key with a synced commit, sets the account unlinked and pushes `logged_out`, then answers.
- **T1, T2:** the awaited write, and `start()` building the live state with `starting` (and `me`), are pinned.

One rule that F2's fix touches still has no test:
- **T3:** that `link.logout` answers only after its commit is synced. With that commit at `NORMAL`, or made 50 ms after the reply, all 251 jawd tests pass. Two checks in the existing test would close it.

Nothing marked data loss or inconsistency remains in the change. The new hygiene items are below, with one finding on main (M1) that bears on the owner's first run.

The questions asked:
- **Does each fix have a test that fails without it?** Yes for F1, for F2's ending, and for T1 and T2. No for F2's synced commit before the reply (T3).
- **Can `demo-unlinked` be picked by accident?** Only by typing `--engine demo-unlinked` on jawd's command line. jaw, the login agent and `jaw demo` never pass `--engine`, and no setting or variable chooses it. It is listed in `jawd --help` (H12). Even on the owner's folder it would only empty the `account` key and say logged out, and the next run with the Go engine writes the key again.
- **Does the owner's first run misbehave?** Not through this change: measured end to end on a version-1 `jaw.db` with no `account` key, it behaves as main does, then as designed. But on main and here alike, the rows the spike left are converted before the account's own JIDs are known, which loses the account's own reactions left in the inbox (M1).
- **Rebase?** Needed to land, and clean. It changes no judgement here; P1's numbers on main's writer are below.

## The first audit's items

- **F1, fixed.** `apply` reports an `UNLINKED` that follows a linked account as `logged_out`, with `NOT_LINKED` unless the engine gave a reason (`sync/mod.rs:419-424`). The key is emptied with a synced commit (`:246-258`). Tests:
  - `an_account_logged_out_while_jawd_was_off_ends_up_logged_out_and_the_next_start_unlinked` now checks the last push and its reason (`sync/tests.rs:1306-1335`).
  - `an_engine_with_no_device_is_offline_to_an_interface_never_told_the_account_was_linked` (`:1337-1347`).
  - jaw's `on_a_real_jawd_that_said_linked_and_whose_engine_then_finds_no_device_jaw_offers_to_link_again` (`crates/jaw/src/wire/tests.rs:885-908`), which ran here against the branch's jawd.
- **F2, fixed** as the first audit's second option: jawd ends the account on the engine's `Done` (`sock.rs:338-349`, `sync/mod.rs:135-154`). Test: `a_logout_jawd_asks_for_forgets_the_account_and_says_so_before_it_answers` (`sock.rs:503-543`), but see T3.
- **T1, fixed:** `the_engine_next_word_waits_until_the_account_it_said_before_is_stored` holds jaw.db's write lock from outside (`sync/tests.rs:1375-1396`), and `the_account_is_kept_in_the_order_the_engine_said_it` ends with the account linked after the logout, not the one before it (`:1349-1362`).
- **T2, fixed:** `tests/starting.rs` plants `account`, `me` and an inbox row that mentions the account, then checks the binary's hello and the row's conversion.
- **H1, H2, H3, H4 and H6, done:** the quarantine count is checked (`sync/mod.rs:251-254`), a kept value that doesn't read is logged (`:120-127`), the comment is beside the keys (`:87-94`), the README says it (`protocol/README.md:59`), and the stopping test's `deadline` is renamed.
- **F3** is written beside the await as a follow-up (`sync/mod.rs:241-245`). **H5** stands.

## Findings

### Data loss

None in the change.

### Inconsistency

None in the change.

### Unpredictable performance

None that breaks a budget.
- **P1, measured again on main's writer.**
  - Since `3e44fb5`, a live write waits behind the inbox group being committed, and a synced one goes with the next group's synced commit. So the sync loop's awaited account write costs more while rows are streaming in.
  - Measured with one task handing over 500 rows a second, as the sync loop does: a live write took 4.2 to 5.4 ms at p50 and 20.8 ms at most, and a synced one 11.7 to 12.5 ms at p50 and 38.5 ms at most (load averages 39 to 51). With the inbox idle, 68 to 153 µs at p50.
  - In practice the inbox is quiet at these moments. A `CONNECTED` follows a disconnection, and a logout or an `UNLINKED` ends the stream. While the loop waits it hands over no rows, so the wait is bounded by one group's commit.
  - It's once per connection or logout, and nothing is lost. Accept, as before.

### Tests: rules the change touches without a test that fails without them

These block under rule 7.

**T3. That `link.logout` answers only after its commit is synced isn't tested.**
- **Where:** `sock.rs:503-543`.
  - `jaw_client::Connection::call` keeps the pushes that arrive while it waits for its reply (`crates/jaw-client/src/lib.rs:51-70`). So `told` is the same push whichever came first.
  - The key is read only after that push, which `logged_out` sends after its commit.
- **What passes that shouldn't**, each alone:
  - `sync::logged_out` with `apply(Lane::Live, …)` in place of `apply_durable`: all 248 + 1 + 2 jawd tests pass.
  - `logout` answering first and forgetting 50 ms later, on a task of its own: all pass.
  - `logged_out` leaving jawd's own account linked: the socket tests pass, and no other test sends `link.logout`.
- **Why it's a rule:** "Durable before acknowledged" lists "any action before its reply". The README now says `link.logout` is "answered once the account is logged out and jawd has forgotten it" (`protocol/README.md:229`).
- **Fix:** in that test,
  - count `store.storage().synced_commits` across the call and expect exactly one more, as `each_sending_call_that_keeps_something_is_one_synced_commit` does (`send/tests.rs:933-972`);
  - hold jaw.db's write lock from outside, as T1's test does, and check the reply doesn't come until it's released;
  - check that a hello after the logout says unlinked.

### Hygiene (may stand as follow-ups)

- **H7. On the connection that asks, the reply to `link.logout` comes before its `logged_out` push.**
  - `one()` awaits `answer` and writes the reply before it reads pushes again (`sock.rs:99-111`). Measured on a raw socket: the reply came first in 20 of 20.
  - The README says the push comes "first" (`protocol/README.md:229`), and the test's name says "before it answers". Other connections get the push whenever their loop takes it.
  - The commit still precedes the reply, so nothing is acknowledged early. Either correct the README and the test's name, or write a connection's waiting pushes before its reply.
- **H8. jaw and `jawd status` ignore the reason.** Both say "logged out from the phone" (`crates/jaw/src/view/draw.rs:196`, `crates/jaw-client/src/lib.rs:119`), and jaw's panel says "Your phone logged jaw out" (`draw.rs:951`). They say it for "this device isn't linked any more" and for "logged out from jaw" too, as step 5 of the first run below shows. Show the reason.
- **H9. jaw, told `logged_out`, goes back to its chats on a later `offline`.**
  - The view leaves `linked` as hello set it (`crates/jaw/src/view.rs:444-463`), and `linking()` needs `linked == false` or `logged_out` (`:919-921`).
  - jawd sends `offline` after a logout when an engine started again says `UNLINKED`, since the account is unlinked by then. jaw then shows the chats, offline, with the message box: F1's symptom.
  - It needs the engine to exit after a logout, and the Go engine stays up after one. The same happens on main.
  - Fix in jaw (`linked = Some(false)` on `logged_out`), or in jawd (keep `logged_out` when the engine says `UNLINKED` after it).
- **H10. A logout and a connection being stored at the same moment.**
  - `logged_out` runs on the socket's task, outside the sync loop's order.
  - If the loop has read a `CONNECTED`'s account under the lock, but not yet queued its write, when the logout's synced commit lands, the key ends linked and the last push says `online`, while jawd's own account says logged out. The next start corrects it, visibly (`UNLINKED`, then `logged_out`).
  - It needs a `CONNECTED` within about a millisecond of a logout.
  - The order would hold if the ending went through the loop: an event of jawd's own queued after the engine's `Done`, or the engine sending `LOGGED_OUT` before it replies (the first audit's first option).
- **H11. A forgetting that fails for good leaves jawd's account linked.** The reply is an error (`sock.rs:342-347`), but the device is already gone. Set the account unlinked and push either way, and say the commit failed. The next start corrects the key.
- **H12. `demo-unlinked` shows in `jawd --help`, and nothing refuses a demo engine on a real folder.**
  - The value is listed "for tests"; `#[value(hide = true)]` keeps it out of the help.
  - Neither demo engine is refused on a folder that holds `session.db`. There, `--engine demo` writes the made-up account's chats into that `jaw.db` (as on main), and `demo-unlinked` empties the `account` key and says logged out, which is harmless.
  - Refusing both there would protect the owner's folder.
- **H13. `scripts/check-data-audit.py` doesn't list `crates/jawd/src/sock.rs`.** Its `link.logout` now answers after a synced commit, which the rule counts as the data layer "wherever it is". Add it.
- **H14. The sync loop's forgetting is synced, but nothing pins it.** With `apply` in place of `apply_durable` (`sync/mod.rs:250`), all tests pass. No acknowledgement follows it, so the rules don't require it. A `synced_commits` check in the logged-out test would keep it.
- **H15. A logout during a catch-up leaves the catch-up open.**
  - Neither `apply` nor `logged_out` clears `catching_up` or `catch_up`.
  - After linking again in the same run, `apply` keeps the connection from going online until an `OfflineDone` (`sync/mod.rs:444-446`), and the feed and notifications wait for a catch-up's end (`feed/mod.rs:74-78`).
  - The same happens on main for WhatsApp's `LOGGED_OUT`. End the catch-up on a logout.
- **`docs/first-run.md`** (on main, line 43) says jaw may show "not linked" at the first start "if `linked-at-hello` hasn't landed yet". Once it has landed, the first start on the owner's version-1 `jaw.db` still says so until the engine connects, since that `jaw.db` doesn't know the account yet; later starts say linked. Correct it in the rebase.

### On main, found while checking the owner's first run (not this change)

**M1 (data loss). Rows left from before are converted before the account's own JIDs are known, and the account's own reactions left in the inbox are lost.**
- **Why `me` is unknown:** a version-1 `jaw.db` has no `me`, since `sync_state` comes with migration 4. So at the first start the converter takes the rows the spike left with `me` empty. `drain::inbox` and `drain::history_raw` read `me` from the live state (`sync/drain.rs:284`, `:380`), and nothing waits for the engine.
- **What happens to an own reaction:** it gets no sender (`convert/mod.rs:359`, `:384-385`), and `apply_amendment` skips it with `Ok(false)` (`store/write.rs:1771`; `:1795` on main). It isn't applied, kept pending or quarantined.
- **In the inbox:** the row is drained in the same transaction, so the reaction is gone.
  - Measured in a scratch test on the rebased tree: Ana's message and the account's own ❤ reaction to it, left in the inbox and converted with `me` unknown, end as 1 message, 0 reactions, 0 pending, 0 quarantined and 0 inbox rows.
  - With `me` known, the reaction lands, as `the_inbox_is_converted_once_even_when_jawd_stops_in_the_middle` shows.
- **In history:** the row's raw stays, in `messages.raw` or `history_kept`, so a rebuild could bring the reaction back. Until one does, jaw doesn't show the account's own reactions, and mentions of the account aren't marked (`convert/mod.rs:287`).
- **When else:** the same happens on any start whose `me` is missing or out of date while rows are left. One case is a first connection's `me` write (`apply_soon`, at `NORMAL`) lost to a power cut.
- **For the owner:** how much it costs depends on what the spike left in its inbox, which this audit didn't open.
- **Fix on main, before the owner's first run**, either:
  - hold the converter's rows from before, and history kept raw, until `me` is known whenever an engine is configured (at the engine's first `CONNECTED`);
  - or send a reaction with no sender to pending rather than skip it.
  - Either way, write `me` awaited at `CONNECTED`, as `account` now is.

## Rebasing onto origin/main

- **Needed to land.** The branch is on `2822c59`, and `origin/main` is 8 commits on, at `ba1c922`: the writer's fairness (`9b800dd`..`49763fb`), the audit rule's wording (`28a7ac3`) and the first-run guide (`ba1c922`). Landing also adds the `Data-Audit` trailers by `git rebase origin/main -x …`.
- **Clean.** A trial rebase in a scratch clone had no conflicts. On it, jawd's 254 tests (the branch's 248 and main's 6) pass, with `starting.rs` and `stopping.rs`, and jaw's 191 pass against the rebased jawd. T1's lock test passed 10 of 10, and the logout test 3 of 3.
- **Nothing judged here changes with main's writer:**
  - It still takes the live queue in order, alone or inside an inbox commit, so the account's writes stay in the order the engine said them.
  - A synced write is answered only after the synced commit it went in, so `link.logout`'s reply and the loop's forgetting still follow a synced commit.
  - A synced write handed to "the next group" goes alone once no rows are waiting (`commit_inbox`, `store/mod.rs:882-892` on the rebased tree). So the loop, which hands over the rows and is waiting on that write, can't hold it up.
  - Only P1's numbers change, above.

## Measurements

**Method:**
- A scratch clone, `/tmp/audit-lah2`, of the worktree, at `62476a0` and rebased onto `ba1c922`, with `CARGO_TARGET_DIR=/tmp/audit-lah2-target`. Debug builds, with dependencies optimised as `Cargo.toml` sets, on an M1 Ultra.
- Every database was a tempdir or `/tmp/audit-lah2-home`. Mutations and scratch tests were made in the clone, one at a time, and undone after each.
- Nothing in `~/src/jaw-sync` was written but this note, and nothing under `~/Library/Application Support/jaw` was opened.
- No Go engine ran: none was built, and none is beside jawd or on `PATH`. Only processes these runs started were stopped, by their PIDs.
- **Load:** at the start the machine had been up 10 minutes, with load averages of 75, 91 and 58 (over 1, 5 and 15 minutes). While measuring, the one-minute average was 36 to 75.

**The suite at `62476a0`:**
- `cargo test -p jawd`: 248 passed and 2 ignored, in 90 s at load averages 66 to 75. `starting.rs` 1 passed, `stopping.rs` 2.
- `JAWD=<its jawd> cargo test -p jaw`: 191 passed.
- The new wire test alone: 5 of 5, in 2.5 to 3.0 s each, 2 s of them the demo engine's pause.

**Mutations,** each alone at `62476a0`, against the whole jawd suite where none failed:

| Mutation | Tests that fail |
| --- | --- |
| an `UNLINKED` after linked is `offline` again (F1's arm removed) | logged out while jawd was off…; jaw's wire test |
| F1's reason dropped | the same two |
| an `UNLINKED` is `logged_out` even to an interface never told linked | an engine with no device is offline… |
| `link.logout` only forwarded to the engine | a logout jawd asks for… |
| `logged_out`'s forgetting at `NORMAL` | none: 248 + 1 + 2 pass (T3) |
| `logout` answers, then forgets 50 ms later | none: 248 + 1 + 2 pass (T3) |
| `logged_out` leaves jawd's own account linked | none of the socket tests (T3) |
| the account's write spawned rather than awaited, still synced for a logout | the engine's next word waits… (5 runs of 5); kept in order… (2 of 5) |
| `main.rs` builds the live state with `Arc::default()` | `tests/starting.rs`, at hello |
| `starting` doesn't restore `me` | `tests/starting.rs`, at the mention; an account linked at the last connection… |
| the sync loop's forgetting at `NORMAL` | none: 248 + 1 + 2 pass (H14) |

**Under load:** T1's lock test, whose windows are 400 ms, passed 10 of 10 at `62476a0` (load averages 48 to 53) and 10 of 10 rebased (50 to 60).

**The reply and the push on the asking connection** (scratch socket test with the test's obliging engine, 20 runs): the reply came first 20 times.

**The owner's first run, end to end**, with the rebased jawd on `/tmp/audit-lah2-home`, read with `jawd status` and `sqlite3 -readonly`:
- The folder held a version-1 `jaw.db` made with the schema tests' `at(…, 1)`, so with migration 1's exact text. It had one inbox row and one history row, neither of which decodes.
- The six runs:
  1. An engine that never says hello (`JAW_ENGINE`, a script that `exec`s `sleep`): upgraded to version 8, took `jaw.db.before-v1`, and answered in 0.13 s with "not linked yet" and offline, as main does. The two rows were kept, not dropped. `sync_state` then held only `clean=1`.
  2. `--engine demo`: linked as Pedro, online. `account` and `me` were written.
  3. The silent engine: "linked as Pedro (+351910000001)", connecting.
  4. No engine: linked as Pedro, offline, with the engine "go, missing".
  5. `--engine demo-unlinked`: at once linked and connecting; 3 s later "not linked yet" and "logged out from the phone" (H8). `account` was emptied.
  6. The silent engine: "not linked yet", offline.

**P1 on main's writer** (rebased tree, scratch test on the store, 3 runs at load averages 39 to 51):
- 200 awaited live `Remember`s on an idle writer: p50 68 to 153 µs, p99 3.9 to 6.3 ms, max 4.6 to 7.0 ms.
- Then for 8 s, one task handed over a 600-byte inbox row every 2 ms, and every 50 ms awaited a live `Remember`, with a synced one 24 ms after each:
  - live (160 of them): p50 4.2 to 5.4 ms, p99 10.8 to 16.1 ms, max 14.7 to 20.8 ms;
  - synced (160): p50 11.7 to 12.5 ms, p99 21.7 to 28.1 ms, max 22.9 to 38.5 ms;
  - 939 to 995 synced commits in all, for 4,000 rows.
- The first audit's numbers were from release builds on the old writer, so they aren't directly comparable.

**Plans:** the change adds no statement. `remembered`'s `SELECT` and `Remember`'s `INSERT OR REPLACE` are pinned as before. There's no schema change, so the fingerprint and `migrations.lock` are untouched.

# First audit: f739cb2 (kept as written)

Range: `2822c59..f739cb2` (`origin/main..HEAD`) in `linked-at-hello`, 17 October 2026. Auditor: database-expert. Line numbers are at `f739cb2`.

## Summary

The key itself is sound. It's one row in `sync_state`, written through the writer by the existing `Remember` change, in the order the engine's events come. No SQL, migration, plan or fingerprint changes. The owner's first run, on a database from before with no key, behaves exactly as main does, and the key is written at the first connection. The builder's 244 tests pass, and each part the builder named fails a test when it's removed.

What's wrong is around the key:
- **F1:** when the engine says `UNLINKED` after a start that said linked, jawd turns its own account to unlinked but tells interfaces only `connection: offline`. jaw keeps showing the account as linked and offline, and never offers linking until it reconnects. On main, hello said unlinked in that case, so this is new.
- **F2:** a logout through `link.logout` leaves the key, and jawd's account, linked. whatsmeow's `Logout` emits no event, and neither the engine nor jawd says anything. The next start then says linked until the engine says `UNLINKED`, which lands in F1.
- **T1, T2:** two of the change's rules have no test that fails without them: that the write is awaited, and that `start()` builds the live state with `starting` (which now also restores `me` before the converter runs).

## The questions asked

**Can the key say linked for an account that's logged out, once the engine has spoken?**
- Within jawd, no. The sync loop awaits each write before it takes the next event, so writes commit in the order the events came. A logout followed by a new link and connection ends with the new account. An engine started again says `CONNECTING` (nothing written) or `UNLINKED` (emptied). The supervisor injects no events and waits at least 500 ms before a restart, so two runs' events never interleave.
- But the engine doesn't always speak, or speak in order: after `link.logout` it says nothing (F2), and whatsmeow can send `CONNECTED` with a removed device's JID after `LOGGED_OUT` (F3).
- A crash or power cut between whatsmeow deleting the device and jawd's commit also leaves the key linked with no device. jawd can die before it takes the `LOGGED_OUT` event, which isn't acked. Or the power can go before jaw.db's next synced commit or its idle checkpoint 10 s later: `session.db`'s deletion commits at `FULL`, the key's emptying at `NORMAL`, and a logged-out account brings no inbox commits.
- In every case the next start corrects the key once the engine says `UNLINKED`. Interfaces only hear of it once F1 is fixed.

**Is it the right class?** `sync_state` is jaw's own (`store/schema.rs:24`). `account`, like `me` beside it, is a cache of what WhatsApp said through the engine, learned again at each connection, so by nature it's fetched from WhatsApp. Either treatment is safe, because the engine's first word replaces it. A rebuild that copies it (jaw's own) or drops it (fetched) leaves the next start correct once F1 is fixed. Keeping it beside `me` is right; H3 asks for the comment that says so.

**Do its writes go through the writer correctly?** Yes. It's `Change::Remember` on the live lane: an idempotent `INSERT OR REPLACE` by key, in a transaction of its own, with no event row (nothing watches `sync_state`), committed at `NORMAL`. `NORMAL` is right, since no acknowledgement follows from this commit. The `connection` push reports state, and only hello at the next start reads the key. A lost write only rewinds the key to its value before, which the engine corrects. F1's fix lists an optional hardening.

**The owner's first run (a database from before, with no key)?** Measured end to end below. hello and status say "not linked yet" and offline while the engine hasn't spoken, as on main. jaw then asks to link, and the engine refuses because `session.db` has a device (`engine/internal/wa/engine.go:404-406`, before it disconnects anything). The engine's `CONNECTING` and `CONNECTED` make the account linked as before, and the first `CONNECTED` writes the key. The next start says linked as the account and connecting, or offline with no engine. The change brings no migration or backup. Nothing new misbehaves.

## Findings

### Data loss

None.

### Inconsistency

**F1. An engine `UNLINKED` after a start that said linked isn't told to interfaces.**
- **Where:** `sync/mod.rs:200` pushes only `live.connection`. `apply` maps `State::Unlinked` to `ConnectionState::Offline` (`sync/mod.rs:368`) and sets the account unlinked (`:375-376`), but no push carries the account. `protocol/README.md:59` describes only the `logged_out` case. In jaw, hello sets `linked` (`crates/jaw/src/view.rs:427-443`), a `connection` push only ever turns it true (`:444-463`), and `linking()` needs `linked == false` or `logged_out` (`:919-921`).
- **What happens:** hello says linked, from the key. The engine finds no device in `session.db` and says `UNLINKED`. jawd's account goes unlinked, and interfaces get `connection {"state":"offline"}` and nothing else (measured). jaw then shows its chats under "offline: what you send waits until it's back", with the message box, and offers no linking until it reconnects to jawd. What's typed is committed to the outbox and waits, pending, for a connection this account won't get.
- **When:** after F2; after the crash or power-cut windows above; after F3; after a build from before this change has run and seen a logout (it doesn't empty the key); after `session.db` is removed or replaced. The builder's "with session.db's device gone" case (`sync/tests.rs:1295-1319`) is exactly this. It checks `live`, not what interfaces hear.
- **Fix:** tell interfaces. Either:
  - report an `UNLINKED` that follows a linked account as `logged_out`, with a reason such as "this device isn't linked any more". jaw and the README already treat that as the account gone, though jaw's text ("Your phone logged jaw out") would need the reason.
  - or add an `account` push, sent whenever `live.account` changes, which jaw takes in place of hello's. That also carries `me` and the account's name, which interfaces otherwise learn only at hello.
- **Tests:** on both sides. In jawd, after a linked start and `UNLINKED`, the pushes take an interface out of linked. In jaw, the view goes to linking on them.
- **Optional:** empty the key with `apply_durable` on `LOGGED_OUT`, and on `UNLINKED` when the key wasn't already empty. Logouts are rare, and the commit's `F_FULLFSYNC` also flushes `session.db`'s deletion from the drive's cache. That closes the power-cut window, though not the window before jawd takes the event.

**F2. `link.logout` leaves the key, and jawd's account, linked.**
- **Where:** `sock.rs:281` forwards to the engine and answers `{}` (`sock.rs:339-349`). The engine's `Logout` (`engine/internal/wa/engine.go:359-363`) calls whatsmeow's `Logout`, which deletes the device and "will not emit any events" (whatsmeow `client.go:743-744`).
- **What happens:** after an interface's logout, jawd's account stays linked and online, as on main, and now the key does too. The next start says linked and connecting until the engine says `UNLINKED`, which lands in F1. This contradicts the README's new sentence ("hasn't been logged out since") and the builder's account ("emptied when it reports logged out"). No shipped interface sends `link.logout` yet (jaw says "jaw unlink comes later"), but the protocol offers it.
- **Fix:** end a logout jawd asks for the way one WhatsApp reports ends. Either:
  - the engine sends `LOGGED_OUT` (reason "logged out from jaw") once `Logout` succeeds, before it replies, so the sync loop empties the key and pushes `logged_out`. That's acceptable once F1 is fixed, since a lost emptying is then corrected visibly at the next start.
  - or jawd, on the engine's `Done`, empties the key with `apply_durable`, sets the account unlinked and pushes, before answering the interface. That follows "any action before its reply" to the letter.
- **Test:** one that fails without the fix. After a logout jawd asked for, the next `starting` says unlinked and interfaces were told.

### Unpredictable performance

None that breaks a budget.
- **P1. The awaited write holds the sync loop, which takes no engine event meanwhile.** Measured in release: 79 µs p50, 167 µs p99 and 6.4 ms at most on an idle writer; 0.48 ms p50, 5.7 ms p99 and 10.9 ms at most while a 100,000-message background batch is stored (it waits for at most one background transaction). It happens once per `CONNECTED`, `LOGGED_OUT` or `UNLINKED`, so at most one message's ack per connection waits that long. While the writer retries a transient failure (a full disk, a lock), the loop waits too, and `connection` pushes wait with it. Nothing is lost, since the inbox can't commit then either. Accept, and say so beside the await.

### Tests: rules the change touches without a test that fails without them

These block under rule 7.

**T1. That the write is awaited isn't tested.** With the awaited write replaced by `apply_soon`, all 42 sync and socket tests pass, in 5 runs of 5. Add a test that fails without the await:
- Hold the writer with an outside `BEGIN IMMEDIATE`, as `writes_that_meet_a_lock_held_past_the_busy_timeout…` does. Send `CONNECTED`, then `LOGGED_OUT`, and check the second's push doesn't go out until the lock is released.
- And in one run, `CONNECTED` (A), `LOGGED_OUT`, link done, `CONNECTED` (B) ends with B kept, while `CONNECTED`, `LOGGED_OUT` ends with nothing kept.

**T2. That `start()` builds the live state with `starting` isn't tested.**
- With `main.rs:320` back to `Arc::default()`, all 246 tests pass. The socket test builds its own `State` (`sock.rs:426-457`).
- That line now also restores `me`, moved from `run`. The converter uses `me` for inbox rows left from the last run (`sync/drain.rs:285`): for mentions of the account, and for the sender of its own edits, deletions and reactions (`convert/mod.rs:287, 359, 385`). jaw can't rebuild those rows from raw. Restoring `me` had no test on main either: removing it from `run` at `5ab4b5f` passes all 240 tests.
- Add a test through `start`, as `main.rs`'s tests call it, or through the binary, as `tests/stopping.rs` runs it with an engine that never says hello. Put an `account` key in jaw.db and check hello says linked as that account before the engine speaks. Together with `starting`'s own assertion on `me` (`sync/tests.rs:1288`), that pins both.

### Hygiene (may stand as follow-ups)

- **F3. whatsmeow can send `CONNECTED` after `LOGGED_OUT` for the device it removed.**
  - whatsmeow dispatches `LoggedOut` in a goroutine of its own while it deletes the device (`connectionevents.go:40-47`, `:126-132`).
  - It dispatches `Connected` even when the post-connect IQ fails (`:200-204`).
  - The engine reads `Store.ID` as it sends (`engine.go:234-242`), and `Delete` clears it only once it's done (`store/store.go:286-298`).
  - So a device removed in the round trip after `<success>` can arrive as `LOGGED_OUT` then `CONNECTED` with the old JID, and jawd writes the key linked again and shows online.
  - The live state has the same race on main. The key carries it only to the next start, where the engine's `UNLINKED` corrects it once F1 is fixed.
  - Follow-up: in the engine, set a flag before sending `LOGGED_OUT`, and drop a later `Connected` until `PairSuccess`.
- **H1.** `Applied.quarantined` isn't checked after the write (`sync/mod.rs:202-207`), so a quarantined `Remember` would go unsaid apart from the quarantine count. An `INSERT OR REPLACE` into `sync_state` can only fail transiently, so this is theoretical. Say it if it's ever non-zero, as `drain::history_raw` does.
- **H2.** The key holds `jaw_proto::Me`, the wire type, and `starting` reads a value it can't parse as unlinked, silently (`sync/mod.rs:114`). A field added to `Me` without a default would make every start say unlinked until the next connection. Log a non-empty value that doesn't parse, or keep a struct of its own.
- **H3.** Say beside `ACCOUNT` and `ME` that they're caches of what WhatsApp said through the engine, kept in the jaw-own `sync_state`: a rebuild may copy them, a restore may bring an old one back, and the engine's first word replaces them, so nothing durable may be decided from them.
- **H4.** `protocol/README.md:59` should also say that with no engine the connection is `offline`, not `connecting`, and that a folder from before this version says `unlinked` until its engine connects. An interface that asks to link then is refused by the engine, which is harmless.
- **H5.** The account's name from app state (`sync/drain.rs:317`) reaches the key only at the next `CONNECTED`. The next start shows the earlier name until the engine's `CONNECTING` brings whatsmeow's. Harmless.
- **H6.** In `tests/stopping.rs:77`, `started` holds a deadline; a name like `deadline` would read better. The wait itself is right: it polls `pgrep` for the test's own marker every 20 ms for up to 10 s, then asserts. It closes the supervisor audit's ask.

## Measurements

**Method:**
- Scratch clone `/tmp/audit-lah` at `f739cb2`, with `CARGO_TARGET_DIR` in `/tmp`, on an M-series Mac. Every database was a tempdir or `/tmp/audit-lah-home`.
- Scratch tests and mutations were made in the clone only. Nothing in `~/src/jaw-sync` was written but this note, nothing in the owner's folder was opened, and no Go engine ran on a folder holding a `session.db`.

**The suite at `f739cb2`:** `cargo test -p jawd`: 244 passed, 2 ignored. `tests/stopping.rs`: 2 passed.

**Mutations,** each alone, against the four new tests and then the wider suites:

| Mutation | Tests that fail |
| --- | --- |
| `starting` ignores the key | linked…, logged out…, hello… |
| nothing written on `CONNECTED` | linked…, logged out… |
| never emptied | logged out… |
| emptied on `LOGGED_OUT` only, or on `UNLINKED` only | logged out… (each) |
| connecting even with no engine | linked… |
| the write not awaited (`apply_soon`) | none: 42 sync and socket tests pass, 5 runs of 5 (T1) |
| `main.rs` doesn't use `starting` | none: all 246 pass (T2) |
| at `5ab4b5f`, `run` doesn't restore `me` | none: all 240 pass |

**F1, jawd's side** (scratch test): after a `CONNECTED`, the next `starting` says linked and connecting. One `UNLINKED` later, jawd's account is unlinked, and the only push is `connection {"state":"offline"}`.

**F1, jaw's side** (scratch view test): starting from a linked view, that push leaves the header at "offline: what you send waits until it's back", the chats and the message box on screen, and no link screen. A `logged_out` push instead brings "logged out from the phone" and "⏎ link again".

**P1** (release build, scratch test):
- 2,000 awaited `Remember`s on an idle writer: p50 79 µs, p99 167 µs, max 6.4 ms.
- 588 of them, 2 ms apart, during a 100,000-message background batch of 1,130 transactions: p50 484 µs, p99 5.7 ms, max 10.9 ms.

**The first run, end to end,** with debug binaries of `2822c59` and `f739cb2`, `JAW_HOME=/tmp/audit-lah-home`, read with `jawd status`:
1. Base build, demo engine: linked as Pedro, online. `sync_state` then held `clean=1` and `me=351910000001@s.whatsapp.net`, and no `account`; `user_version` 8.
2. The branch, with an engine that never says hello (`JAW_ENGINE` a script that sleeps): "not linked yet", offline, engine starting. Same as main.
3. The branch, demo engine: linked, online, and `account={"id":"351910000001@s.whatsapp.net","name":"Pedro","phone":"+351910000001"}` written.
4. The branch, the silent engine again: "linked as Pedro (+351910000001)", connecting.
5. The branch, no engine anywhere: linked as Pedro, offline, engine missing.

A first try at step 5 found a stray `/tmp/jaw-engine` beside the binary and started it on the scratch folder. That folder had no `session.db`, none was made, and jawd ended the engine within 26 ms. The binary was moved to a folder of its own for the run above.

**Plans:** the change adds no statement. `remembered`'s `SELECT` and `Remember`'s `INSERT OR REPLACE` are pinned (`plans.txt:87-88`, `:418`). There's no schema change, so the fingerprint and `migrations.lock` are untouched.

## Verdict

What must change before this lands:
1. F1: an interface told linked hears when the engine says the account isn't linked, with tests in jawd and jaw.
2. F2: a logout jawd asks for empties the key and is told to interfaces, with a test.
3. T1 and T2: tests that fail without the await, and without `start()` using `starting`.

F3 and H1 to H6 may stand as follow-ups.

Verdict: changes needed

# Verdict of the second audit (62476a0)

Changes needed. F1, F2, T1 and T2 are fixed and pinned, and nothing marked data loss or inconsistency remains in the change. Before this lands:

1. **T3:** a test that fails if `link.logout` answers before its commit is synced: count `synced_commits` across the call, hold jaw.db's write lock and check the reply waits for it, and check that a hello afterwards says unlinked.

Rebase onto `origin/main` before landing, correct `docs/first-run.md`'s line about `linked-at-hello` then, and give every data-layer commit the trailer `Data-Audit: docs/audits/2026-10-17-linked-at-hello.md`. H7 to H15, and the first audit's F3 and H5, may stand as follow-ups. M1 is main's, not this change's, but it bears on the owner's first run and should be fixed before that run.

The next audit only needs to check T3's test, and that nothing else moved.

Verdict: changes needed

# Verdict of the third audit (050a9c4)

Approved. T3 is closed: the logout test fails for each way it passed before, and the rebase changed nothing the earlier audits judged. Nothing marked data loss or inconsistency remains in the change, and every rule it touches has a test that fails without it.

To land:
1. Rebase onto `origin/main` (`96332e2`), giving every commit the trailer `Data-Audit: docs/audits/2026-10-17-linked-at-hello.md`. All three now touch the data layer, `050a9c4` through `sock.rs`.
2. Commit this note in the branch, and run `make check-audit`.

These may stand as follow-ups:
- H16, reading the count where the answer arrives;
- H14, which becomes required if the logout's ending moves into the sync loop;
- H12's refusal of a demo engine on a folder that holds `session.db`;
- H8 to H11 and H15, and the first audit's F3 and H5.

M1 is main's, not this change's, and should still be fixed before the owner's first run.

Verdict: approved
