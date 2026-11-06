# Audit: a download's retry that waits too long (the Linux CI audit's L1)

Range: 25ea7275..3016b57d in `retry-fix` (`~/src/jaw-retry-fix`), 6 November 2026, round one. Auditor: database-expert.

## The change

One commit, 3016b57d, +57 −4, all under `crates/jawd/src/media/`, which `scripts/check-data-audit.py` counts as the data layer:

- `schedule.rs:165-170`: `Schedule::next_try(looked)` keeps the waits that end after `looked`, when the look began (`*at > looked`), where main kept only those still to come once the look was over (`*at > Instant::now()`). `run` passes its own `looked`, taken just before `take()` (`schedule.rs:283`, `:288`).
- `schedule.rs:117-119`, `:324-325`, `:369-372` and `mod.rs:356-362`: a hook in test builds only. `Media::look_slowly(pause)` has the next look sleep `pause` once it has read what's wanted.
- `tests.rs:2403-2435`: the test `a_copy_whose_wait_ends_while_the_scheduler_looks_is_tried_again_with_nothing_else_to_wake_it`.

No SQL, write, transaction, ack, file, schema, migration or fixture changes.

## Why it holds

- **Nothing a look skips is forgotten.** `take()` skips a copy as waiting only if `waiting()` finds its time after `Instant::now()` at that row (`schedule.rs:161-163`, `:347`). Every such moment comes after `looked`, which is taken before `take()`, and `Instant` never goes backwards. So a skipped copy's time is after `looked`, and `next_try(looked)` keeps it.
  - The only exception is a time something removed meanwhile, and each of those wakes the scheduler: a job's success (its end), `media.cancel` (its synced commit) and a reconnect's clear (its `wake`).
  - A time still ahead: the scheduler sleeps until it, as on main.
  - A time that passed during the look: `sleep_until` returns at once, and the next look takes the copy. If that look finds it busy or without room, a job's end wakes the scheduler for it, as on main.
- **Nothing spins.** A kept time that has passed brings one look, and that look's `looked` is after it, so it's dropped there. Times change only when a job fails (`try_later`), so the extra looks are bounded by failures: one per look at most, whatever the next look finds (no room, a failing read, many copies at once). P1, P2 and P3 below measure it.
- **Clock changes move nothing here.** Every time compared is a `std::time::Instant`, the monotonic clock, and tokio's timer runs on the same clock, so a wall-clock step changes neither. A forward jump makes every waiting time pass at once, which brings one look, as any passed time does. `wanted`'s milliseconds only order rows.
- **The design's download rules hold.**
  - Photos, stickers, GIFs and voice still download as they arrive.
  - A file gone from WhatsApp's server is still asked of the phone, at most 20 asks a minute, and waits for it, saying so.
  - Retries still back off from 2 s, doubling to 300 s.
  - Idle, with nothing waiting to be tried again, the scheduler still sleeps (counted below).
  - The row was always kept wanted. Only when its download is tried again changes.
- **The one other changed behaviour** is the builder's: after a look whose read fails, waits that ended during it bring one more look at once, where main dropped them. That look fails or succeeds as reads do, and P2 shows it's one look.

## Findings

None is data loss or inconsistency, and none must be fixed before landing.

**F1. Media out's sender has the same race. Unpredictable performance (a late upload), on main, not changed here. Can wait for after the release, with its own test and audit.** `media/outgoing.rs:167-173`, `run` at `:421-438`, `take` at `:443-503`.

- `Sending::waiting` and `Sending::next_try` compare with `Instant::now()`, as media in's did. So an upload or a copy whose retry time passes between `take()`'s check of it and `next_try()` loses its time.
- The runner has no commit wake, so it then sleeps until the next send, a job's end, a copy made, a reconnect or a restart. Nothing is lost: the outbox row and its kept file stay, and it goes then. But on a quiet account, a message of yours could sit unsent until then.
- It's far rarer than L1:
  - Its look is one read, then a loop with no await. So the window is that loop's in-memory work and `next_try`, microseconds, longer only if the OS preempts the thread, and the time must fall in it while a look runs for some other reason. Media in's window was its whole look, up to three awaited reads.
  - Its usual cause, WhatsApp out of reach, is tried again at the reconnect anyway (`online` calls `retry_now`).
- So it doesn't block this fix. Its fix is the same line, `looked` taken before `take()`. But it needs its own hook, between `take()`'s loop and `next_try()`, a test that fails without it, and its own audit, and landing it untested before Friday would be the wrong trade. It needs no known-limit line.

**F2. Nothing in the suite fails if the scheduler keeps looking. Test gap, on main as here. Can wait.** `schedule.rs:168-170`, `:283`, `:288`.

- With the fix, a kept time may already have passed. Only `looked` being taken afresh each time round the loop keeps that to one look.
- A mutant that keeps every time since the first look (`hoisted`, the same as `looked` hoisted out of the loop or the filter dropped) makes 19,496 to 47,252 looks in 8 s in P1, and 915,084 to 983,811 in 5 s in P2, each look a read of jaw.db. jawd's `media::` suite still passes 139 of 139 under it, the branch's test included.
- Main's filter was just as unguarded: drop it, and no test fails there either.
- The fix as written makes exactly one more look (2 against main's 1 in P1 and P2), and its idle wake-ups are main's (below), so this only guards later changes.
- Fix: a test-only look counter, and a test shaped as P1 or P2 (a passed time brings one look, then quiet), from `retry-fix-audit-probes.diff` beside the audit's report.

**F3. A look whose read fails schedules no look of its own. Other (a late download), on main, can wait.** `schedule.rs:285-288`.

- After a failed read, the copies still wanted wait for the next commit, job end, reconnect or restart. P2 shows it, on main and here: once reads work again, the photo stays put until a wake comes. The fix's extra look covers only the waits that ended during the failed look.
- A read fails only if the readers have stopped, a reader panicked, or SQLite errs after its 5 s busy timeout (a lock held that long, an I/O error, corruption), all of which come with bigger trouble.
- Fix, later: after a failed look, look again after a bounded pause, such as `disk::POLL`. That wakes only while reads fail.

**F4. The branch's test can lose its edge, but never fails falsely. Other, can wait.** `tests.rs:2424-2431`.

- It assumes its slow look begins before the 1 s wait ends, which needs the test's 300 ms sleep and the scheduler's wake to take under 700 ms together. Past that, the look takes the copy, and the test passes on main too.
- As measured, the switch failed 15 of 15 up to load 39, so it holds here.
- Fix, optional: note when the job ended and assert that the look began within the wait, or make the wait longer.

## Measurements

All in one clone (`/tmp/jaw-audits/retry-fix/jaw`, deleted afterwards), built with `CARGO_INCREMENTAL=0`. jawd's unit tests (`cargo test -p jawd --bin jawd`) ran from copies of each binary, one test per process, on a 20-core Mac.

- **The switch:** the commit with only `next_try`'s comparison put back to `*at > Instant::now()`.
  - At light load (5.7 to 8.1, with five switch streams, two fix streams and the whole `media::` suite at once): the switch failed 10 of 10, each at `until`'s 60 s bound (61.5 s), with the row still `remote`, wanted, "the server answered 404". The fix passed 30 of 30, in 3.52 to 3.67 s.
  - At heavy load (three `media::` suites at 8 threads beside them; load 17.7 rising to 40.3): the fix passed 30 of 30, in 4.06 to 5.90 s. The switch failed 5 of 5 (63.1 s), with load 17.7 at their start, rising to 39 while they ran.
  - In all, the switch failed 15 of 15 and the fix passed 60 of 60.
  - The load suites passed 140 of 140 once. Twice they had 139, with media in's listed start-check test failing (`tests.rs:2555`, the idle audit's O5), as in the builder's runs.
- **Spin probes** (`retry-fix-audit-probes.diff`): a look counter, every look's read refused while a flag is set, and two mutants chosen at run time, all in test builds only. Three runs each, at load 10 to 45.
  - P1: a 5 s wait ends during a 5 s look, and 16 photos with held downloads leave no room for it; looks counted for 8 s. The fix made 2, 2 and 2, and the photo downloaded once the room freed. Main's comparison made 1, 1 and 1. `hoisted` made 19,496, 47,252 and 46,517.
  - P2: a 1 s wait ends during a 2 s look whose read fails, as does every read after it; looks counted for 5 s. The fix made 2, 2 and 2, and the photo downloaded once reads worked and a wake came. Main's comparison made 1, 1 and 1. `hoisted` made 915,084 to 983,811.
  - P3: six 2 s waits end during one 4 s look. With the fix, all six downloaded in 4.15 s, with 9 looks and 12 asks, 3 of 3. With main's comparison, all six stayed stuck, and it failed 3 of 3 at the 60 s bound. `hoisted` passed, with 11 to 16 looks.
- **The hook:** `look_slowly`, `next_look_pause` and the pause in `take()` are all under `#[cfg(test)]`. A release build of the commit compiles (3 min 45 s), and its binary has no `look_slowly` symbol, where the test binary has one.
- **Idle wake-ups,** by the idle audit's method (`count.py`):
  - Release builds, with the demo engine silenced (`silence.py`: its people come and go hourly), on a fresh demo account of 5,000 messages, 50 chats and 200 people. Then `jawd demo --idle 120`, which waits for 15 s without a commit and counts the process's wake-ups for 120 s.
  - Alternating, the fix counted 6.0 and 5.5 a minute (12 and 11 wake-ups; 1.9 and 1.1 ms of CPU), and main 4.5 and 5.5 (9 and 11; 4.3 and 1.1 ms). The second pair is identical: 11 wake-ups, 21 switches, 1.1 ms of CPU.
  - So there's no difference beyond the counts' own noise. The idle audit's 4.0 to 4.5 were on a copied account whose about texts had already been asked.
- **Landing facts:**
  - GitHub's main is `25ea7275` (`git ls-remote`, 04:03 UTC), the branch's base. `retry-fix` isn't on GitHub.
  - `git merge-tree --write-tree` of the branch with `linux-ci` exits 0, both as GitHub has it (`fceabf21`) and as the shared repository has it now (`77c13e9a`, with its audit). `linux-ci` touches `media/drive.rs` and the photo test in `media/tests.rs`.
  - A simulated landing in the clone, with the trailer on 3016b57d and this note committed after it, passes `check-data-audit.py 25ea7275 HEAD`.

## Verdict

Approved. The fix closes L1: a copy whose wait ends while the scheduler looks is tried again once the look is over, with nothing else to wake it. The test fails 15 of 15 without the fix and passes 60 of 60 with it, up to load 40, and six copies at once behave the same. It loses nothing and changes no data, no kept time brings more than one look whatever that look finds, and idle is as on main. F1 to F4 can wait.

**Landing.** GitHub's main is `25ea7275`, the branch's base.

- **The trailer:** give `Data-Audit: docs/audits/2026-11-06-download-retry.md` to 3016b57d, the branch's one commit, for example `git rebase origin/main -x 'git commit --amend --no-edit --trailer "Data-Audit: docs/audits/2026-11-06-download-retry.md"'`.
- **The note:** commit it in the branch after 3016b57d. It touches no data-layer path, so it needs no trailer.
- **What the lander checks:**
  - Main is still `25ea7275` when pushing. If it has moved (for example, `linux-ci` landed first), rebase onto it. `linux-ci` merges with this branch without conflict as it stands. If the rebase conflicts, or main's new commits change `media/schedule.rs`, `media/outgoing.rs` or what wakes the scheduler, bring it back to the audit.
  - The gate on the tree that's pushed: `make check-generated`, `make lint` and `make test`.
    - The new test and `a_phone_that_cant_be_reached_is_asked_again_and_one_that_cant_be_asked_says_why` must pass. With the fix, L1 no longer excuses the phone test.
    - Listed timing tests, such as media in's start-check test, follow the flakes rule: each may fail once if it then passes alone 3 of 3.
  - `make check-audit` exits 0.
  - The push is from `~/src/jaw-retry-fix`, through the pre-push hook, never with `--no-verify`, and only once the owner has said it goes into 0.1.0. Then watch main's own CI run on Linux.
- **The release's notes:** if this lands before the tag, L1 needs no known-limit line, and there's none for it in `docs/releases/0.1.0.md` on this base. If it doesn't land, the Linux CI audit's suggested line applies. F1 needs none.
- **Follow-ups:** F1 (media out's race, with its own hook, test and audit), F2 (a test that a passed time brings one look), F3 and F4.

Verdict: approved
