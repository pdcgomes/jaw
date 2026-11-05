# Audit: jawd quiet when idle (the idle fix)

Range: 3c2d1101..28c34d9f in idle-fix (297f9cc2 media's settle, 28c34d9f the disk watchdog's look), 5 November 2026. Auditor: database-expert. Round one. origin/main is 4dfdfd09, as GitHub's main is: 39 commits past the branch's base, da2bfced among them, none touching `crates/` or any file of the branch.

## What the change is

- **297f9cc2, media's settle.** The task that sends new previews and thumbnails to the drive and notes them settled (`media_made.settled`) used to wake every `settle_every` (5 s) for as long as jawd ran. Now it waits on a `Notify`, told by `previews::thumbnail` and `previews::previews` once their commit naming a thumbnail or previews returns, or on the weekly look. Then it waits `settle_every` as before. It goes on every `settle_every` while a settle fails, and it starts with something to settle when the start found suspects. The clean stop's settle is unchanged. The design's Data integrity line says "a few seconds after they're written" where it said "every few seconds".
- **28c34d9f, the watchdog's look.** The watchdog used to look every 10 s. Now `tick` says when to look again:
  - every 10 s while the disk is short of room, the ballast isn't kept, or the free space can't be read now;
  - otherwise as long as a fill at 2 GB a second takes to bring the disk down to the floor and margin, clamped to 10 s .. 10 minutes;
  - at once when `full()` gives up the ballast.

  `room()` is unchanged: every caller still reads the disk. tokio's `test-util` is in jawd's dev-dependencies only (resolver 3; it isn't in jawd's normal feature tree).
- **Not changed:** no SQL, schema, migration, PRAGMA, transaction boundary, ack, or file written for the data. The plan test is untouched.

## What was asked, checked

- **Every number wakes the settle.** Only `Step::Previews` and `Step::Thumbnail` give `media_made` numbers (store/write/media.rs:442-460). They're built only in `draw_previews` and `draw_thumbnail`, and applied only in `previews::previews` and `previews::thumbnail` (previews.rs:55, 123), which now tell the settle after the commit (58, 126). `mend` forgets in a synced commit and wants the copy again, so what it remakes is named through the same two.
- **No wake-up can be lost.** In tokio 1.53.3 (Cargo.lock), `notify_one` with no task waiting stores a permit (`notify_locked`: `EMPTY | NOTIFIED => NOTIFIED`). A `Notified` that `select!` drops after it was told passes the notification on, or stores it again (`drop_notified`). So a naming while the task settles, sleeps out `settle_every` or looks weekly is kept for its next wait, and so is a ballast given up while the watchdog looks.
- **Every path, by reading:**
  - **Named while the loop waits:** it wakes, and settles `settle_every` later.
  - **Named during the `settle_every` sleep:** the settle reads `made` when it starts, so it takes it. The permit left makes one more settle, which finds nothing.
  - **Named while a settle runs,** after it read `made`: the permit makes the next settle take it, `settle_every` later.
  - **A settle that fails** (the drive, a read, or the mark): `made` stays set, so it's tried again every `settle_every`. The mark can't be refused for a constraint: `min(made, max(settled, ?1))` always meets `CHECK (settled <= made)`. A full disk's failure goes through the writer's retries.
  - **Named just before a clean stop:** the stop's `settle_within(500 ms)` takes it, unchanged. One named after that settle read `made`, or a settle not done within 500 ms, is past `settled` at the next start and checked there, as on main.
  - **A crash or a power cut:** the next start's suspects are exactly the rows past `settled`. They're checked before they're shown, then settled `settle_every` after the check, since `made` starts set.
  - **The weekly look:** if it comes due while something waits to settle, it goes first, and the settle follows, since `made` stays set.
- **Nothing inconsistent.** The change moves only when `settle()` runs. `settle()` itself is unchanged (fsync each file and the folder, one barrier, then the mark), so what jaw.db says is settled is on the drive. The task's settle and the stop's can overlap, as on main, and the mark only moves forward within `made`. So nothing is noted twice to any effect, and nothing is skipped: a row is settled with all its paths at its last number, and a write after its page was read keeps it past `settled`.
- **The disk's promises.**
  - **Background work never waited for the watchdog's look.** The writer reads the disk before each background transaction and in `index_step`, and again each `POLL` while it waits (store/mod.rs:1439, 1903-1908). The media scheduler reads it on every pass (media/schedule.rs:279), and the history fetcher each `POLL` (sync/fetch.rs:206). Main's store tests of the floor never start the watchdog.
  - **A fill at up to 2 GB a second** is told within 10 s of going below the floor. From a look finding F free, the next comes no later than (F − floor − margin) / 2 GB/s, so by then the disk can't have come below the floor and margin; within 20 GB of them the look is every 10 s. The branch's fill test shows it, and so does probe C at a literal rate.
  - **Still every 10 s** while the disk is short of room (it's then under the floor and margin, so the wait is 0, clamped to 10 s), while the ballast is unmade or given up, and while the free space can't be read now. At once when `full()` gives the ballast up.
  - **No stale `room()`.** It reads `statvfs` every time, as before, and answers from the last reading only while the disk can't be read, also as before. Only `Disk::status` and media's `paused` (media/mod.rs:381) show the last reading (O1).
- **What the watchdog's audit relied on the 10 s look for:**
  - The ballast made again: now every 10 s while it isn't kept, and at once when given up.
  - The notice when something else fills the disk while jaw is quiet: still within 10 s for fills up to 2 GB a second (O3 for faster ones).
  - The first look at a start, which makes the ballast: unchanged.
  - The loop being a tokio task, dropped at `shutdown_timeout`: unchanged.

  O17's and T8's tests count the writer's own `disk::POLL` reads, not the watchdog's looks. Erasures and the WAL's emptying never waited for the disk, and read it themselves.
- **O7 of the watchdog's audit (a network folder):** judged in O4 below. The builder's call is right.
- **The media-in audit's settle rules hold:** a barrier before the mark; the start's check before suspects are shown; and "the settle's mark goes again 5 s later" when it fails.
- **Performance:** no new or changed statement, so no plan to pin. When idle, the task no longer opens a reader connection (`Store::pair`) and reads a page every 5 s. My count is 4.0 a minute, twice, against main's 53.0 (Measurements).
- **The landing:** a clean merge, both commits take the trailer, and no migration (Measurements).

## Findings

No finding is data loss or inconsistency: the code holds every path a naming or a fill can take, as above. But two of the settle's paths that "nothing lost" rests on have no test of the branch's that fails without them (T1, T2). This round asks for those two tests, which are already written: probes A and B in `probes.patch`, beside the audit's report in `idle-fix-audit-files/`. Everything else can wait for after the release.

### T1. Test gap, to fix before landing: previews named while a settle is under way

- **Where:** media/previews.rs:58 and 126 (`notify_one`); media/settle.rs:59-79 (the loop); the branch's settle tests, media/tests.rs:1059-1140.
- **What happens:** a naming while the task isn't waiting on the `Notify` (a settle under way, after it read `made`) is kept only because `notify_one` stores a permit. Switched to `notify_waiters`, which keeps none, the branch's settle tests passed in 11 of 12 runs. In the twelfth, the retry test failed only because its thumbnail happened to be named before the task's first wait. Such previews would then wait for the next naming or the stop: the tail of a burst of photos. The code is right, but nothing holds it.
- **Fix:** a test in which previews are named while a settle is held, and the next settle takes them with nothing more named. Probe B (`audit_previews_named_while_a_settle_is_under_way_are_settled_by_the_next`) does this. Downloads are gated so that the thumbnail is named first, and a stand-in drive holds that settle's first push. It fails with `notify_waiters` in 12 of 12 runs ("settled through 1 of 2 after 6.0 s"), and passed 72 of 72 at the head, at loads of 67 to 77.

### T2. Test gap, to fix before landing: a thumbnail named alone, and previews named after it is settled

- **Where:** the same.
- **What happens:** the branch's tests name a photo's thumbnail and previews within `settle_every` of each other, so either naming wakes the settle for both. With only the thumbnail's `notify_one` removed, or only the previews', no test in `media::` fails for it (one listed flake aside, see Measurements). Both cases are common. A thumbnail is named while its file is still downloading or waiting: a large video, a file the phone is asked to send again, a download media's free-space check holds back. Previews are named more than 5 s after the thumbnail: a large file or a slow download.
- **Fix:** probe A (`audit_a_thumbnail_named_alone_and_previews_named_later_are_each_settled`). With downloads gated, the thumbnail is named and settled alone (1 of 1); then the previews are named and settled (2 of 2). It fails with either `notify_one` removed ("settled through 0 of 1"; "settled through 1 of 2").

### T3. Test gap, can wait: the plan's 2 GB a second isn't pinned

- **Where:** disk.rs:31 (`FILL`); disk/tests.rs, whose new tests are all written in `FILL`.
- **What happens:** with `FILL` at 1 GB a second, every branch test passes, while a fill at 2 GB a second is then told 99 s after going below the floor, not within 10 s. Background work still waits at once, so no data is at stake; only the notice is late. `LONGEST` is held: at an hour, the wait test fails.
- **Fix:** probe C (a fill at a literal 2 GB a second from 200 GB above the floor and margin; with `FILL` halved, "below the floor at 101 s, told at 200 s"), or the branch's fill test at a literal rate.

### T4. Test gap, can wait: a ballast given up during a look

- **Where:** disk.rs:229 (`full()`'s `notify_one`); the loop, 260-271.
- **What happens:** with `notify_waiters`, every test passes. A ballast given up during the look itself (the `statvfs` and the ballast's check, microseconds) would then wait for the next look. But a disk full enough to refuse a commit reads short of room in that same look, so the next look is within 10 s anyway. This is about the code saying what it means, not about the ballast.
- **Fix:** probe E. A look makes the ballast, the ballast is given up, then the watchdog starts: it looks twice at once, where with `notify_waiters` it looks once. It fails with `notify_waiters`.

### O1. Other, can wait: `jawd status`'s disk line is as last read

- **Where:** disk.rs:306-322 (`status`); media/mod.rs:381 (`paused`).
- **What happens:** on an idle jawd far above the floor, the free space that `jawd status` and `jaw status` show is the last look's, up to 10 minutes old, as the plan says. Nothing decides anything on it.
- **Fix:** optional: `Disk::status` reads the disk itself (one `statvfs`, 2 µs at p50 by the watchdog's audit).

### O2. Other, can wait, and for the release's notes: below about 22 GB free, the watchdog still looks every 10 s

- **Where:** disk.rs:250-253.
- **What happens:** within 20 GB of the floor and margin (under 22.3 GB free on any disk over 40 GB), the wait is 10 s. So an idle jawd there still wakes for the watchdog as main's does: 18 of main's 53 wake-ups a minute, by the bisect. That's by design, since near the floor it should look often. It's worth saying beside the idle limit in the notes: the fix's 4 a minute needs more free space than that.
- **Fix:** none needed.

### O3. Other, can wait: a fill faster than 2 GB a second is told at the next look

- **Where:** disk.rs:250-253.
- **What happens:** a Mac's own SSD can write faster than 2 GB a second. On a jawd with no commits, such a fill is told at the next look, up to 10 minutes after the look before it, where main told it within 10 s. Background work waits at once, since it reads the disk itself; inbox rows and live writes never waited; and the ballast is kept. So nothing is lost, as the plan says.
- **Fix:** none needed.

### O4. Other: the watchdog's O7, judged

- The builder kept the design's answer, refusing a network or synced folder ("One process"), which main doesn't do yet. It didn't take O7's second fix, reading the disk every `POLL` on the watchdog's task, which would be 240 looks a minute for as long as jawd runs, against the idle budget.
- That's right. O7 was "Other", and its first fix is that refusal; on such a folder WAL mode isn't safe anyway, so the refusal is the fix that matters.
- This branch adds no `statvfs` and moves none, so O7 is as it was. The refusal stays a change of its own, after the release.

## Measurements

- **Method:** one clone of the worktree in `/tmp/jaw-audits/idle-wakeups` with its one build folder, `CARGO_INCREMENTAL=0`, and the probes as scratch commits on the head there. Each switch was an exact replacement, checked to match before it was made, then built and run, and the file restored after. The machine has 20 cores and had other work beside the audit; each result gives its load.
- **The branch's tests at the head:** the 17 disk and settle tests passed (6.1 s, load 18.9). All 139 of `media::`, with the probes: passed, 1 ignored (110 s, load 17.5).
- **The switches.** The builder's 15 each failed a test of the branch's, at the assertions its report gives (load 14 to 72):

  | Switch | The branch's tests that fail | Probes |
  | --- | --- | --- |
  | the task wakes every `settle_every`, as before | wakes-only | |
  | naming doesn't wake it | wakes-only, tried-again | A, B |
  | a failed settle not tried again | tried-again | |
  | a start's unsettled previews wait for a naming | start-found | |
  | the stop's settle settles nothing | clean-stop, main's stop-bound | |
  | every 10 s whatever the room | the wait, ballast, unreadable, idle-looks | D |
  | the loop sleeps 10 s whatever the look says | idle-looks | |
  | every 10 minutes whatever the room | the wait, ballast, unreadable, idle-looks, the fill | C |
  | the margin left out | the wait | |
  | no cap | the wait | D |
  | no 10 s at least near the floor (the wait tests only) | the wait | |
  | the last reading taken while unreadable | unreadable | |
  | the ballast's state ignored | ballast | |
  | no look at once when it's given up | given-up | |
  | `room()` answers as the last look found the disk | what-waits, and three of main's | |

  Six of the audit's own:

  | Switch | The branch's tests that fail | Probes |
  | --- | --- | --- |
  | the settle's `notify_one` as `notify_waiters` | tried-again in 1 of 12 runs, none in 11 | B, 12 of 12 |
  | only the thumbnail's `notify_one` removed | none of `media::`'s 139 | A, B |
  | only the previews' `notify_one` removed | none of `media::`'s 139 | A, B |
  | `full()`'s `notify_one` as `notify_waiters` | none | E |
  | `FILL` 1 GB a second | none | C |
  | `LONGEST` an hour | the wait | D |

  In the run of all of `media::` with the thumbnail's `notify_one` removed, media in's starts-check test also failed. It's on the landers' list, it runs before the settle's loop starts, and it passed at the head.
- **Under load,** at the head with the probes:
  - the four new settle tests and probes A and B: 32 of 32 runs with 4 at once (load 69.6 to 77.3), and 40 of 40 with 8 at once (load 67.2 to 74.1);
  - the 13 disk tests and probes C and D: 100 of 100 with 4 at once (load 70.6 to 77.7);
  - with probe E as well: 60 of 60 (load 23.7 to 24.8).
- **The count.** Each count is `jawd demo --idle 120` on an APFS clone of one account, no jaw attached, with release builds silenced as the builder's (`COMINGS` 3600 s, no presence answer), through the builder's `count.py`. The account was smaller than the builder's: 5,000 messages in 50 chats with 200 people, made by the head's jawd, its about asks done after 39 s.

  | jawd | A minute | In 120 s | CPU | Load at start, end |
  | --- | --- | --- | --- | --- |
  | the head, 28c34d9f | 4.0 | 8 | 1.3 ms | 61.6, 40.5 |
  | main, 3c2d1101 | 53.0 | 106 | 50.8 ms | 33.3, 23.0 |
  | the head, again | 4.0 | 8 | 1.2 ms | 23.0, 20.8 |

  These agree with the builder's 4.0 and 4.5 against 52.5, and with the bisect's 53.5.
- **Landing facts:**
  - `git ls-remote` shows GitHub's main at 4dfdfd09, the worktree's `origin/main`.
  - `git merge-tree --write-tree 4dfdfd09 28c34d9f` exits 0, with no conflict.
  - origin/main's 39 commits change none of the branch's paths (`crates/jawd`, the two plans, Cargo.lock), and add no audit note of 4 or 5 November.
  - The branch changes no schema, migration, `migrations.lock` or `plans.txt`.
  - In `scripts/check-data-audit.py`'s paths, 297f9cc2 touches `crates/jawd/src/media/`, and 28c34d9f touches `crates/jawd/src/disk.rs` and `crates/jawd/src/disk/`.

## Verdict

Changes needed, for T1 and T2 only, and neither needs a code change: the code holds every path, and the two tests are written.
- **What must change:** add tests that fail with the settle's `notify_one` as `notify_waiters`, and with either of its two `notify_one`s removed. Probes A and B in `probes.patch` do this (test files only). Put them beside the branch's settle tests in media/tests.rs, in 297f9cc2 or a commit after it, and say so in the commit's message.
- **Round two checks** that they're in the branch, fail with those three switches every time, pass under load, and that the branch still merges onto origin/main with no conflict.
- **Can wait for after the release:** T3, T4 and O1 to O4. Probes C, D and E in the same patch are there for T3 and T4.
- **At landing, once approved:** both commits take the trailer `Data-Audit: docs/audits/2026-11-05-idle-wakeups.md`, as does any commit that adds the tests, since it touches media/. There's no migration.

Verdict: changes needed

# Round two: a521ed70

Range: 3c2d1101..a521ed70 in idle-fix, 5 November 2026: round one's 297f9cc2 and 28c34d9f, then a521ed70, the tests (`media/tests.rs` +54, `disk/tests.rs` +45). Auditor: database-expert. origin/main and GitHub's main are 4dfdfd09.

## In short

- **Verdict: approved.** Round one's two gaps are closed: T1 by B and T2 by A, each failing under its switches in 10 runs of 10 (and B in 12 of 12 in round one). The two that could wait are closed too: T3 by C, T4 by E. Nothing but the tests changed since round one, whose code holds every path. Under load the four passed 150 runs of 150 each, and in every full run they were in.
- **The start-check test fails more often with A and B in the run.** In full `media::` and `disk::` runs under load, it failed in 13 of 23 runs with A and B, and in none of 12 without them. Run one at a time, as a gate runs them, it failed in 6 of 9. A and B share nothing with it but the process, and leave nothing running once they're done. What decides it is when it starts in the run, which A and B move later. It's media in's timing test, on the landers' list, and it fails by looking too late, never by a wrong answer. The lander applies the flakes rule; fixing its window is a follow-up (O5).
- **The landing:** a clean rebase onto 4dfdfd09. All three commits take the trailer. No migration.

## What was asked, checked

### The tests close round one's gaps

- **They're round one's probes,** with the changes the test round's report gives. C is probe C, and E is probe E, each with a comment, E's message counting all three looks. A is probe A on `rig`, with a stand-in drive that does nothing, where the probe had `recording` (A never read what it kept). B is probe B with its hold an `mpsc` channel and a 30 s `recv_timeout` in place of a condvar; a failing test that drops the sender lets the hold go at once. Probe D isn't in, and wasn't asked for: the branch's wait test already fails with `LONGEST` at an hour.
- **Each fails under its switch, every time** (10 runs each, in the audit's clone; the table under Measurements):
  - B with the settle's `notify_one` as `notify_waiters` (T1);
  - A and B with only the thumbnail's `notify_one` removed, or only the previews' (T2);
  - C with `FILL` at 1 GB a second (T3);
  - E with `full()`'s `notify_one` as `notify_waiters` (T4).

  A passes with `notify_waiters`, as round one meant: its namings come while the task waits.
- **And one more switch of mine:** the task heeds only the first naming it hears, as a loop polling one `Notified` made outside it would. A and B fail in 10 runs of 10, while the branch's wakes-only, tried-again, start-found and clean-stop tests all pass. So A and B hold a bug that nothing caught before them.
- **C measures what it says.** On its 500 GB fake disk the floor is 2 GB (`min(2 GB, 5 %)`) and the margin 256 MB. From 200 GB above them at a literal 2 GB a second, the disk goes below the floor at 101 s; the plan's rate has the watchdog look at 100 s and 110 s.
- **Under load:** 150 of 150 each, 6 processes at once (load 15 to 49). A and B passed in all 16 of my full runs that had them, and C and E in all 25. With the test round's 20 at the head and 7 full runs, none of the four has failed.

### Nothing else changed

- 297f9cc2 and 28c34d9f are the objects round one audited: a521ed70's parent is 28c34d9f.
- `git range-diff 3c2d1101..28c34d9f 3c2d1101..a521ed70` shows `1 = 1`, `2 = 2` and a521ed70 new. `git diff 28c34d9f a521ed70` outside the two test files is empty.

### The new tests and the start-check test

- **What they share.** Each rig has its own folder, engine (its download gate and media server), store and runtime. Process-wide, there are only:
  - `jaw-media`'s `CLAIMED`, which holds absolute paths, so different folders never meet;
  - `FINISHING`, held only while a file is synced into place, which the settle's drive never takes (B's held push holds no lock);
  - the demo's cache of painted lakes;
  - the CPU.
- **Its window** (media/tests.rs:2502-2528). `check_after` is zero, so in its second jawd the check starts inside `Media::start_with`. Looks at about 0, 50 and 150 ms fail (`AGAIN` 50 ms, doubling, settle.rs:156), and the fourth reads the page at about 350 ms. The test's first poll comes only after `start_with` and `rig_on` return, then every 5 ms. If its thread doesn't run for that while, it reads the status after the fourth look (line 2521: `checking: false, suspect: 0`). That's the right answer, read too late.
- **Measured** (Measurements). Full `media::` and `disk::` runs with the marks binary, two or three at once so that all arms see the same load: A and B in, A and B skipped, and a control arm that keeps A and B and skips two tests from beside them (106 and 137 in name order). Together with the test round's 10 runs and my two single runs at the head, the start-check test failed in 13 of the 23 runs with A and B, and in none of the 12 without them. An exact test, one-sided and stratified by round, gives about 0.003. The control arm, which has as many tests ahead of it as the arm without A and B, failed 2 of 5. So it isn't only the number of tests ahead.
- **Not through A and B themselves:**
  - A had ended 24 to 40 s before the start-check test started, and B ended 3.5 to 13 s before its window, in every run.
  - A probe test run after each, one test at a time, finds 2 threads and no CPU in 5 s after A, B, either control test, or the start-check test itself. So nothing of theirs outlives them.
- **Through when it starts.** All 7 failures in my paired and three-arm runs came in the arm whose start-check test started latest in its round, 2.4 to 9.7 s after the next arm's. No earlier-starting arm ever failed: about 1 in 1,000 by chance. Every test ahead of it in libtest's name order moves that start, and A and B mostly move it later: the arm with them started it latest in 7 of the 9 rounds, the control arm in the other 2, and the arm without them never. Its 200 ms window then falls in a busier moment of the run's tail.
- **So, for the lander:** the start-check test will likely fail in `make test` under load. Run one at a time, as a gate runs them, `media::` and `disk::` failed it in 6 of 9 runs with A and B, at loads of 17 to 78. It's on the landers' list, so it may fail once if it then passes alone 3 of 3, as it did here (3 of 3 at load 27 to 34, and 5 of 5 in the test round). Don't skip it, mark it ignored, or change it in this branch. Its fix is O5.

### The landing

- GitHub's main is 4dfdfd09, 39 commits past the base. They touch 29 files, under `mac/`, four of the Mac's plans, `docs/releases/`, `README.md`, `Makefile`, `protocol/README.md`, `packaging/homebrew/jaw.rb` and `scripts/release.sh`. None is one of the branch's nine, and none is an audit note.
- In the audit's clone:
  - The three commits rebase onto 4dfdfd09 with no conflict, to tree 981d5bf2, which is `git merge-tree`'s too.
  - `check-data-audit.py` then flags all three as touching the data layer: 297f9cc2 (media's `mod.rs`, `previews.rs`, `settle.rs`, `tests.rs`), 28c34d9f (`disk.rs`, `disk/tests.rs`) and a521ed70 (`disk/tests.rs`, `media/tests.rs`).
  - With the trailer added by `git rebase -x`, the tree is still 981d5bf2. With this note committed after them, `check-data-audit.py upstream/main HEAD` exits 0.
- No migration: the branch changes no schema, migration, `migrations.lock` or `plans.txt`.

## Findings

No finding is data loss or inconsistency, and no test gap is left.

### T1 and T2. Test gaps, closed

- **T1** is closed by B (media/tests.rs:1098), **T2** by A (media/tests.rs:1080), each failing under its switches every time, as above.

### T3 and T4. Test gaps, closed

- **T3** is closed by C (disk/tests.rs:264), **T4** by E (disk/tests.rs:316).

### O5. Other, can wait: the start-check test misses its window more often with A and B in the run

- **Where:** media/tests.rs:2510-2521, media in's `the_starts_check_reads_a_page_again_that_it_couldnt_and_shows_nothing_of_it_meanwhile`; the check's backoff, media/settle.rs:155-157.
- **What happens:** the test must read the status within the about 200 ms between the check's third failed look and its fourth. Under load its thread misses that, and more often when A and B are in the run: 13 of 23 full runs against 0 of 12, as above, and 6 of 9 run one at a time. Until it's fixed, every lander after this one will meet it too. The check itself is right every time: the page is passed at the fourth look.
- **Must it be fixed before landing?** No. It's a listed timing test, and the flakes rule covers it.
- **Fix, after the release, as a change to media in's test with its own audit:** keep the looks failing until the test has read the status. Set `failing` to `u32::MAX`, wait for three looks to have failed, read the status (still checking, one suspect, which is then certain), then set it to 0 and wait for the page to be shown. That leaves no window to miss, and changes no product code.

### O6. Other, nothing to fix here: the flood test, watched, failed with three full sets at once

- **Where:** media/tests.rs:1453-1474, `a_flood_of_photos_never_holds_up_a_message`, which the flakes audit has watched rather than listed, so that a failure is looked into.
- **What happens:** it failed in 6 of the 15 runs with three full sets at once (load 45 to 58): "a message waited 525 ms to 1.06 s behind photos", against 500 ms. 4 of the 6 were without A and B. It failed in none of the 8 runs with two at once.
- **Looked into:** the branch doesn't touch media's schedule or the lanes the test measures, and its settle waits an hour there (`quick()`), as on main. It passed alone 3 of 3 (load 20 to 23). So it's a load flake, at about three times a gate's load. If it fails in the gate, judge it as the watched rule says.

### Round one's O1 to O4 stand

- O1, `jawd status`'s disk line as last read, can wait.
- O2 is for the release's notes: below about 22 GB free, the watchdog still looks every 10 s.
- O3, a fill faster than 2 GB a second, needs no fix.
- O4 is judged.

## Measurements

- **Method.** One clone of the worktree in `/tmp/jaw-audits/idle-wakeups-r2`, with its one build folder, `CARGO_INCREMENTAL=0`. Binaries were copied out before switching.
  - **Switches:** each an exact replacement, checked to match, in a file that matched HEAD; built, its tests run 10 times as separate processes under a timeout, then the file restored from HEAD and checked.
  - **Two scratch probes,** built the same way and restored after. The marks binary appends a wall-clock mark as A, B and the start-check test start and end, and as the start-check test sees the third failed look. The residue binary adds a test, run last, that measures the process's CPU time and threads over 5 s of sleep.
  - **Load:** the machine has 20 cores, with other agents' work and the Linux builder's gate beside the audit. Each result gives its 1-minute load.
- **The switches** (load 11 to 22, each built in 57 to 58 s):

  | Switch | Fails | Passes |
  | --- | --- | --- |
  | the settle's `notify_one` as `notify_waiters` | B 10 of 10, "settled through 1 of 2 after 6.0 s" | A 10 of 10 |
  | only the thumbnail's `notify_one` removed (previews.rs:58) | A 10 of 10, "settled through 0 of 1 after 5.1 s"; B 10 of 10, "the thumbnail's settle never reached the drive" | |
  | only the previews' removed (previews.rs:126) | A 10 of 10, "settled through 1 of 2 after 5.1 s"; B 10 of 10, "settled through 1 of 2 after 6.0 s" | |
  | `FILL` at 1 GB a second (disk.rs:31) | C 10 of 10, "below the floor at 101 s, told at 200 s" | |
  | `full()`'s `notify_one` as `notify_waiters` (disk.rs:229) | E 10 of 10, 2 reads where 3 are due | |
  | the task heeds only the first naming it hears (settle.rs:58-62) | A 10 of 10 and B 10 of 10, "settled through 1 of 2" | the branch's 4 settle tests, 10 of 10 each |

  These agree with the test round's 10 runs of each of the first five.
- **Stress at the head:** A, B, C and E passed 150 of 150 each: 6 processes at once, 25 rounds each, beside the first pair (load 15.1 to 48.8; a run took 1.9 to 17.9 s).
- **The start-check test in full `media::` and `disk::` runs** (155 tests, 1 ignored; failures of it, of the runs):

  | Runs | With A and B | A and B skipped | 106 and 137 skipped |
  | --- | --- | --- | --- |
  | the test round's, one at a time, load 20 to 78 | 4 of 7 | 0 of 3 | |
  | 4 pairs, two at once, load 12 to 59 | 2 of 4 | 0 of 4 | |
  | 5 rounds, three at once, load 25 to 58 | 3 of 5 | 0 of 5 | 2 of 5 |
  | two at the head, one at a time, load 17 to 52 | 2 of 2 | | |

  The other failures:
  - the save-waiting test, listed: in 4 of my runs (2 with A and B, 2 without), and in 3 of the test round's (2 with, 1 without);
  - the flood test, watched: in 6 of my runs, all three at once (O6).

  The marks:
  - the start-check test took 11 to 19 s, and saw the third failed look 11.1 to 16.6 s in;
  - A ended 24 to 40 s before it started;
  - B ended 1.0 to 10.7 s into it, 3.5 to 13.3 s before that look.

  By the logs' birth times, the failing arm's start-check test started latest of its round in all 7 rounds with a failure. The arm with A and B started it latest in 7 of the 9 rounds, the control arm in 2.
- **Residue** (3 rounds, `--test-threads=1`, load 26 to 34): after nothing, A, B, 106, 137 or the start-check test, the process had 2 threads and used 0.000 s of CPU in 5 s.
- **Alone at the head:** the start-check test 3 of 3 (load 27 to 34), the save-waiting test 3 of 3 (load 19), the flood test 3 of 3 (load 20 to 23).
- **Landing facts:**
  - `git ls-remote`: main 4dfdfd09.
  - `git merge-tree --write-tree 4dfdfd09 a521ed70` exits 0, tree 981d5bf2.
  - The simulated landing is as above.

## Verdict

Approved. A and B close T1 and T2, C and E close T3 and T4, and each fails under its switch every time and passes under load. Nothing else changed since round one. The start-check test's flakiness beside them is its own window, met by the flakes rule now and by O5 after the release.

**Landing.** origin/main is 4dfdfd09.
- **The trailer:** give `Data-Audit: docs/audits/2026-11-05-idle-wakeups.md` to all three commits, named by their hashes before the rebase: 297f9cc2, 28c34d9f and a521ed70, for example `git rebase origin/main -x 'git commit --amend --no-edit --trailer "Data-Audit: docs/audits/2026-11-05-idle-wakeups.md"'`.
- **The note:** commit this note in the branch, after them. It touches no data-layer path, so it needs no trailer.
- **What the lander checks:**
  - Main is still 4dfdfd09 when pushing. Rebased onto it, before the note's commit, the head's tree is 981d5bf2, with or without the trailers. If main has moved, rebase onto it. If that conflicts, or main's new commits change any of the branch's nine files or another file under `crates/jawd/src/media/` or `crates/jawd/src/disk*`, bring the move back to the audit.
  - The gate on the rebased tree: `make lint`, `make test` and `make check-generated`.
    - In `make test`, the start-check test is likely to fail under load (O5). It and the save-waiting test may each fail once, if each then passes alone 3 of 3, for example `cargo test -p jawd -- --exact media::tests::the_starts_check_reads_a_page_again_that_it_couldnt_and_shows_nothing_of_it_meanwhile` three times. CI runs `make test` on Linux, with a schedule of its own; a failure of it there is the same case.
    - The flood test is watched: if it fails, compare with O6 before judging it.
    - None of the three is skipped, ignored or changed in this branch.
  - `make check-audit` exits 0.
  - The push is from `~/src/jaw-idle-fix`, through the pre-push hook, never with `--no-verify`.
- **Follow-ups:** O5 and O6, and round one's O1 to O4 (O2 for the release's notes).

Verdict: approved
