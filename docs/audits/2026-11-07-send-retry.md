# Audit: the upload sender's late retry (the download-retry audit's F1 and F2)

Range: fff9e612..071e7827 in `send-retry` (`~/src/jaw-send-retry`), 7 November 2026, round one. Auditor: database-expert. GitHub's main is 501f16c3, 34 commits past the branch's base, none of them in the data layer.

## The change

Two commits, +144 −9, all under `crates/jawd/src/media/`, which `scripts/check-data-audit.py` counts as the data layer:

- **11f9fd35, media out's sender (the download audit's F1),** `outgoing.rs` +29 −3 and `outgoing/tests.rs` +56.
  - `run` takes `looked` each time round its loop, just before `take()` (`outgoing.rs:441`), and `Sending::next_try(looked)` keeps the waits that end after it (`:188-190`), where main kept only those still to come once the look was over. It's media in's `next_try` since b6240027, line for line.
  - In test builds only: `next_look_pause` and `looks` (`:94-99`), `look_slowly` (`:103-109`), the count (`:442-443`), and the pause, taken as `take()` starts (`:464-465`) and slept once it has gone through what's to do (`:524-527`).
  - The tests (`outgoing/tests.rs:858-912`): the helper `upload_failed_once`; the race test, `an_upload_whose_wait_ends_while_the_sender_looks_is_tried_again_with_nothing_else_to_wake_it`; and the one-look test, `a_wait_that_ends_while_the_sender_looks_brings_one_more_look_not_a_spin`.
- **071e7827, media in's one-look test (the download audit's F2),** `schedule.rs` +15 and `tests.rs` +44 −6.
  - In test builds only: `looks` and `refuse_reads` (`schedule.rs:120-125`), the count (`:290-291`), and the refusal (`:334-340`), which takes the slow look's pause and then fails the read.
  - The tests (`tests.rs:2403-2473`): the race test's setup, moved into `waiting_to_be_tried_again`, and the one-look test, `a_wait_that_ends_while_the_scheduler_looks_brings_one_more_look_not_a_spin`.

No SQL, write, transaction, ack, file, schema, migration, `plans.txt` or fixture changes. Outside test builds, media in's code is main's.

## Why it holds

- **Nothing a look skips is forgotten.** `take()` passes over a copy as waiting only when `waiting()` finds its wait after `Instant::now()` at that row (`outgoing.rs:181-183`, `:467`). That moment is after `looked`, and `Instant` never goes backwards, so the wait ends after `looked` and `next_try(looked)` keeps it.
  - A wait is set only by `try_later` (`:192-196`), when a job fails, and the job's end then wakes the sender. One set during a look ends after `looked` too, so it's kept.
  - A wait is forgotten only when its upload succeeds (`:959`, and the job's end wakes the sender, `:488-489`), in `stop()` (`:1043`: a cancel or a deletion, which leave nothing to do), and at a reconnect (`retry_now`, `:198-202`, which wakes the sender).
  - A kept wait still ahead: the sender sleeps until it, as on main. One that passed during the look: `sleep_until` returns at once, and the next look takes the copy. If that look finds it busy, the job's end wakes the sender, as on main.
  - The other copies a look leaves are running, and their jobs' ends wake the sender, or weren't read (past `AT_ONCE`, or a failed read, F4), as on main.
- **Nothing spins.** A kept wait that has passed brings one look and isn't kept by it: that look's `looked` is read after the timer fired or the wake came, and tokio's timer never fires before its deadline. Waits are set only when jobs fail, so the extra looks are bounded by failures, one per look at most, whatever the look finds. Measured: one look with every read failing (P1), one for six copies whose waits all end in one look (P2), and none at all for a wait left behind once it has passed (P3).
- **The backoff and the rest of the sender are main's.** `try_later`, `waiting`, `retry_now`, the defaults (2 s, doubling to 300 s, `mod.rs:121-122`) and every caller are unchanged. The design's rule, that a failure is tried again with backoff and then stays marked for a manual retry, holds. Only when a waiting copy is looked at again changes.
- **Clock changes move nothing.** Every time compared is a `std::time::Instant`, on the monotonic clock tokio's timer also runs on, so a wall-clock step changes neither.
- **Performance.** No new or changed statement. An extra look is one `TO_DO` read (`store/write/outgoing.rs:117-122`, pinned at `plans.txt:3845`), at most one per failure. Idle, jawd wakes as main does (Measurements).
- **The hooks are in test builds only.** Everything new but the fix itself (`looked`, and `next_try`'s argument and comparison) is under `#[cfg(test)]`. jawd built without tests compiles with no warning, and has no `look_slowly` symbol and no text of the refused read, where the test binary has two and one.
- **Media in's race test is as it was.** `waiting_to_be_tried_again` is its setup word for word, with the message's ID as its one parameter, and the test goes on from it unchanged. It still fails 10 of 10 with main's comparison, and passes 10 of 10 at the head.

## Findings

None is data loss or inconsistency, and none must be fixed before landing.

**F1. "At most two looks" also passes a wait kept for two looks. Test gap, can wait.** `outgoing/tests.rs:907-910`, `tests.rs:2466-2469`.

- The `lag` mutant, whose `next_try` goes by the previous look's start, keeps a passed wait for two looks where the fix keeps it for one. It passed the sender's one-look test 10 of 10, P1 5 of 5 and media in's one-look test 5 of 5, each with exactly 2 looks.
- With the fix, each of the 30 one-look runs with the count printed made exactly 1 look, as did the builder's 40.
- So the tests catch a spin by a wide margin, 13,599 looks at the fewest against their bound of 2, but not one more look per failure. That's one more `TO_DO` read and breaks no budget, so it can wait.
- Fix: count wakes in test builds too, and assert exactly one look when the slow look began within the wait and no wake came while it ran; at most two otherwise. The probes read the first from the copy's wait in `later`, against when the slow look was seen to begin.

**F2. The sender's race test can lose its edge, but never fails falsely. Other, can wait.** `outgoing/tests.rs:871-883`.

- As the download audit's F4 says of media in's: it assumes the slow look begins before the 1 s wait ends, so the 300 ms sleep and the sender's wake must take under about 1 s together. Past that, the look takes the upload itself, and the test passes on main too.
- Measured: in all 58 sender runs that print it, the slow look was seen to begin 654 to 694 ms before the wait ended, at load 60 to 69. Main's comparison failed the race test 20 of 20.
- Fix, optional: assert that the slow look began within the wait.

**F3. A copy's wait outlives it, and a manual retry's backoff starts from the old pause. Other, on main, not changed here, can wait.** `outgoing.rs:192-196`, `:959`, `:1043`, `:198-202`.

- A copy that fails for good (`unsent`) keeps its wait and its doubled pause until a reconnect clears them all, and so does one whose make failed and then went through. `send.retry` tries the first at once, but if that fails for a reason that may pass, it waits the kept pause (4 s, 8 s, up to 300 s) rather than 2 s. An upload after a failed make starts from the make's pause the same way.
- The waits kept are harmless to the loop: each has passed, so every look that begins after it drops it. P3 measures no look at all in 10 s.
- Fix, later: forget a copy's wait when it's unsent or made.

**F4. After a look whose read fails, the sender looks again only when woken. Other (a late upload), on main, can wait.** `outgoing.rs:444-447`.

- The twin of the download audit's F3, as the builder's report says. Copies still to do after a failed read wait for the next send, job's end, copy made, reconnect or restart; the fix's one more look covers only the waits that ended during the failed look. P1 shows it: once reads work again, its photo goes only when the probe wakes the sender.
- A read fails only if the readers have stopped, a reader panicked, or SQLite errs past its busy timeout, each with bigger trouble.
- Fix, later, with media in's: after a failed look, look again after a bounded pause.

## Measurements

- **Method.** One clone of the worktree in `/tmp/jaw-audits/send-retry/jaw` at 071e7827, with its one build folder, `CARGO_INCREMENTAL=0`, each binary copied out before the next build. jawd's unit tests ran from those copies, one test per process (`--exact`), from the clone's `crates/jawd` with `TMPDIR` in the scratch, each killed past 300 s, eight at once, in two rounds (12:31 to 12:36 and 12:39 to 12:44 UTC). The Mac has 20 cores, and the other database expert's unlink audit ran beside this one; each result gives the 1-minute load at its runs' starts.
- **The probes** (`probes.py`: exact replacements, each found once, made for one build and removed after):
  - Two switches read at run time, `AUDIT_SENDER` for media out's `next_try` and `AUDIT_SCHED` for media in's: `now` is main's comparison (`*at > Instant::now()`), `hoisted` keeps the first look's start for good (the download audit's spin mutant), and `lag` goes by the previous look's start.
  - In test builds only, a hook that fails the sender's read as `refuse_reads` fails media in's. Both one-look tests print their count, and the sender's how long before the wait's end its slow look was seen to begin.
  - P1: the sender's one-look test with every read failing from the slow look on, looks counted for 4 s from its start; then reads work, a wake comes, and the photo is sent.
  - P2: six photos whose first uploads fail wait 3 s each. A 5 s look begins within all six waits (1.6 to 1.9 s before the first ends; the six spread over 0.8 to 1.0 s) and ends past them all; the retried uploads are held, and looks counted until 2 s after the first is held. Then all six are sent, 12 uploads in all.
  - P3: a photo whose first upload fails and whose second is refused for good leaves its wait behind, passed; looks counted for 10 s of idle.
- **The branch's four tests, from the head's own test binary:** 70 of 70.

  | Test | Passed | Seconds | Load |
  | --- | --- | --- | --- |
  | the sender's race test | 20 of 20 | 3.3 to 6.7 | 63 to 69 |
  | the sender's one-look test | 20 of 20 | 5.3 to 7.8 | 62 to 69 |
  | media in's race test | 10 of 10 | 3.9 to 5.6 | 64 to 69 |
  | media in's one-look test | 20 of 20 | 5.8 to 8.6 | 60 to 69 |

- **The switches and probes**, from the probes' binary, at load 45 to 69 (looks counted after the slow one):

  | Test | The fix | `now`, main's comparison | `hoisted`, the spin | `lag` |
  | --- | --- | --- | --- | --- |
  | the sender's race test | the head's 20 of 20 | failed 20 of 20, at the 60 s bound (61.0 to 63.5 s) | passed 3 of 3 | passed 3 of 3 |
  | the sender's one-look test | passed 10 of 10, 1 look each | failed 6 of 6, "its upload was never tried again" | failed 20 of 20, 13,599 to 17,792 looks | passed 10 of 10, 2 looks each |
  | P1, reads failing | passed 5 of 5, 1 look each | passed 3 of 3, 0 looks | failed 5 of 5, 446,810 to 541,005 looks | passed 5 of 5, 2 looks each |
  | P2, six waits | passed 5 of 5, 1 look each, all six sent | failed 3 of 3, 0 looks, no upload tried again | failed 3 of 3, 13,657 to 15,207 looks | |
  | P3, idle | passed 5 of 5, 0 looks | passed 3 of 3, 0 looks | failed 3 of 3, 75,082 to 81,792 looks | passed 3 of 3, 0 looks |
  | media in's race test | the head's 10 of 10 | failed 10 of 10, at the 60 s bound (61.5 to 63.1 s) | | |
  | media in's one-look test | passed 10 of 10, 1 look each | passed 3 of 3, 0 looks | failed 20 of 20, 373,651 to 495,615 looks | passed 5 of 5, 2 looks each |

- **Media's module** (`media::`, the head's test binary, once, load 15 rising to 48): 142 passed and 1 ignored in 116 s, the sender's 57 among them. Media in's start-check test failed ("the page it couldn't read is still to check"), a listed timing test (the idle audit's O5), as in both of the builder's gates; alone it passed 3 of 3 (1.1 to 1.2 s, load 29).
- **Builds:** the head's test binary in 3 min 13 s (load 53 to 65), the probes' in 1 min 27 s and 1 min 34 s; jawd without tests in 1 min 4 s, with no warning; release builds, silenced, in 4 min 11 s (the head, load 63) and 3 min 16 s (the base, load 10).
- **The hooks:** `nm` finds `next_try` (48 symbols) and `try_later` (6) in both the test binary and jawd built without tests, and `look_slowly` (2) only in the test binary; `strings` finds the refused read's text only there.
- **Idle wake-ups,** by the idle audit's method: release builds with the demo engine silenced (its `silence.py`), each on a fresh demo account of 5,000 messages, 50 chats and 200 people, its about texts asked after 39 s, then `jawd demo --idle 120` through its `count.py`, with `JAW_HOME` and `TMPDIR` in the scratch. `jawd demo` runs its own demo engine whatever `--engine` says (`demo/mod.rs:228`), so nothing reached WhatsApp.

  | jawd | A minute | In 120 s | CPU | Load at start, end |
  | --- | --- | --- | --- | --- |
  | the head, 071e7827 | 6.0 | 12 | 1.1 ms | 16.5, 10.7 |
  | main, fff9e612 | 5.5 | 11 | 1.2 ms | 12.8, 12.8 |
  | the head, again | 5.0 | 10 | 1.1 ms | 12.8, 14.8 |

  No difference beyond the counts' own noise, and the idle fix's few a minute: the download audit counted 6.0 and 5.5 for its fix, and 4.5 and 5.5 for main.
- **Landing facts:**
  - `git ls-remote` shows GitHub's main at 501f16c3, the worktree's `origin/main`; `send-retry` isn't on GitHub.
  - Its 34 commits past fff9e612 change 140 files, none of the branch's four and none of `check-data-audit.py`'s paths.
  - `git merge-tree --write-tree 501f16c3 071e7827` exits 0, tree 55851f28.
  - In a scratch worktree of the audit's clone, the two commits rebase onto 501f16c3 with no conflict and the trailer added by `git rebase -x`, to tree 55851f28. `check-data-audit.py 501f16c3 HEAD` then exits 0 with an approved note committed after them; without the trailers it flags both commits.
  - b6240027, media in's fix, is in v0.1.0; this one isn't in any release.

## Verdict

Approved. The fix closes the download audit's F1: a copy of yours whose wait ends while media out's sender looks is tried again once the look is over, with nothing else to wake the sender, one copy or six. The race test fails 20 of 20 with main's comparison and passes 20 of 20 at the head, at load 45 to 69. No kept wait brings more than one look, whether reads fail, six waits end at once or a wait is left behind, and idle wake-ups are main's. The download audit's F2 is closed too: both one-look tests fail every time the look's start is kept for good. Nothing touches jaw.db or what's written. F1 to F4 can wait.

**Landing.** GitHub's main is 501f16c3.

- **The trailer:** give `Data-Audit: docs/audits/2026-11-07-send-retry.md` to both commits, by their hashes before the rebase 11f9fd35 and 071e7827, for example `git rebase origin/main -x 'git commit --amend --no-edit --trailer "Data-Audit: docs/audits/2026-11-07-send-retry.md"'`.
- **The note:** commit it in the branch after them. It touches no data-layer path, so it needs no trailer.
- **What the lander checks:**
  - Main is still 501f16c3 when pushing. Rebased onto it, before the note's commit, the head's tree is 55851f28, with or without the trailers. If main has moved, rebase onto it. If that conflicts, or main's new commits change anything under `crates/jawd/src/media/` or what wakes media's sender or scheduler, bring it back to the audit.
  - The gate on the rebased tree: `mise run check-generated`, `mise run lint` and `mise run test`.
    - The branch's four tests must pass, and so must `a_phone_that_cant_be_reached_is_asked_again_and_one_that_cant_be_asked_says_why`.
    - Listed timing tests follow the flakes rule: each may fail once if it then passes alone 3 of 3.
  - `mise run check-audit` exits 0.
  - The push is from `~/src/jaw-send-retry`, through the pre-push hook, never with `--no-verify`. Nothing is published: no tag and no release, as the owner chose while away.
- **The release's notes:** `docs/releases/0.1.1.md` on main says, in its first paragraph and under "What's the same", that jawd is 0.1.0's but for the words of a few messages. With this branch that no longer holds, so those two places need changing, and the notes need a line for the fix in jawd: an upload whose retry came due while media's sender was looking waited for the next send, job or reconnect. For example: "A photo or file of yours whose upload was to be tried again, and whose time came while media's sender was looking at what to do, waited for your next send, another copy's job or a reconnect; now it's tried again as soon as the look is over. Nothing was lost meanwhile."
- **Follow-ups:** F1 to F4.

Verdict: approved
