# Audit: jawd ends its engine however it stops (`engine-stop`, M8)

# Round two: `b4d9624d..39461d70`, round one's findings, `session.lock`, and the landing onto today's main

Range: `b4d9624d..39461d70` in `engine-stop`, eleven commits, 4 November 2026. Auditor: database-expert.

- Round one's four, rebased unchanged: `9e48c3ce`, `3d01f843`, `880f637a`, `bb8282c3`.
- Round two's four: `579a1a1f` (I1), `20a39d83` (T1 to T4), `d7c2006a` (`session.lock`, O1) and `85e1f13b` (O2 and O4).
- Round 2b's three: `72e76d96` (pauses that tests can lengthen), `ea7e4746` (the end in order worded by exit status) and `39461d70` (`main.rs` on the data-audit list, and the design's Supervision line).

Paths are under `crates/jawd/src/` unless they say otherwise, with line numbers at `39461d70`. The worktree stayed at `39461d70`, with nothing uncommitted but this note. Everything was measured in one clone of it at `/tmp/jaw-audits/engine-stop-r2/jaw`, built with `CARGO_INCREMENTAL=0` and run under `nice -n 10`, on a shared Mac with 20 cores. Other branches were landing at the same time: the load was 17 to 100, and from 13:20 to 14:40 it stayed between 60 and 100. The real jaw-engine ran only on scratch homes that were never linked, so it never connected: nothing was sent to WhatsApp, and no process but the audit's own was signalled.

## In short

- **Approved.** There's no data loss or inconsistency, and every rule the change touches has a test that fails without it. Follow-ups O5 to O12 stand; none blocks.
- **I1 is fixed.** `run` now takes `jaw.lock` before it makes the runtime and before the socket is listened on (`main.rs:528`). `stopped` lets it go only once the engine's end and the runtime's shutdown are over (`main.rs:557-564`). A probe took the lock every half millisecond through 46 stops of every kind round one named and the rest, a panic included, and never found it free while the engine ran.
- **`session.lock` keeps two engines off one `session.db`.** The engine takes it after its watch on jawd and before its hello, holds it until it exits, and is refused at once if another process holds it, with its own words in `jawd status`.
  - Never two engines ran on one home across 13 restarts after jawd was killed (in 3 of them the old engine was kept alive 3 s on purpose) and 5 engine kills.
  - A lock held by another process kept the engine back only until it was let go, and the file a dead engine leaves behind keeps nothing back.
- **T1 to T4** exist, and each fails with its rule off. T1's, T3's and T4's tests passed in every run. T2's test failed 2 of 43 times alone at a load of about 20, and 3 of 13 whole runs near 90 (twice on its log line, once with its engine not started in time), never on its rule (O5).
- **2b's words** describe each step after the first by the engine's exit status, and no interface reads them. **Production's pauses** are 1 s and 500 ms, measured. `--engine-quitting` does nothing unless given, and neither an interface nor a config can give it.
- **The data-audit list** now has `main.rs` and `engine/cmd/jaw-engine/`. `home.rs` belongs on it too, with `jaw-proto`'s `local.rs` (O9).
- **2b's `jaw.lock` flake** can't bite a real jawd beyond refusing, for a moment, a jawd started just as the first exits: the lock can only outlive its drop, never go early. Most of it comes from `jaw-media`'s fork, but `posix_spawn` has the same window, shorter. It can be a follow-up (O7).
- **The suite at the head** fails only listed tests at the brief's load. Near a load of 90, stopping tests fail on their own timeouts, never on their rules. The fixed never-answers test failed that way twice (O6 explains why), and passed in every run at a load of 17 to 61.
- **Landing.** One conflict onto today's main, in `run()`, resolved by keeping both sides. If the disk watchdog lands first, it's more than that. No migration is needed, and the nine commits `check-audit` names take the trailer.

## What was asked, checked

### I1: `jaw.lock` held until the engine has gone and the runtime has shut down

- **The shape at the head.**
  - `run` takes the lock (`main.rs:528`) before it makes the runtime, so before `serve` listens on the socket (`572`). A refused jawd touches neither.
  - It hands the lock to `stopped` (`550-566`). That runs the serving future under `catch_unwind`, then `Ending::end` (`636-649`), then `shutdown_timeout(100 ms)`, and only then `drop(lock)` (`564`), before it lets a panic go on. No profile sets `panic = "abort"`, so `catch_unwind` does catch.
  - `serve`, `start_on`, `start` and `Started` no longer carry the lock.
  - The two `exit(1)` paths, on a second signal in `run`'s select (`540`) and in `serve`'s (`597`), first kill the engine and wait up to 250 ms for it (`Ending::at_once`, `652-656`). The lock goes as the process does.
- **Every stop, measured** (`lockprobe.py` and `panicprobe.py`; the method is under Measurements). In 46 stops the lock was never free while the engine ran:
  - a stop while starting (`--hold-start`) with an engine that ignores its standard input closing and SIGTERM, 5 runs: SIGKILL ended it 1,509 to 1,517 ms after the SIGTERM, and the lock came free 0.1 to 1.2 ms after its exit;
  - the same with an engine that SIGTERM ends, 5 runs: SIGTERM ended it at 1,004 to 1,164 ms;
  - a start that fails once the engine runs (media's folder made a file), 6 runs, the engine started in all 6: the lock came free 0 to 1.0 ms after its exit;
  - an orderly stop that runs out of its 2.75 s (an engine that says hello, never answers and ignores SIGTERM), 3 runs: killed at once at 2,755 to 2,759 ms;
  - a second SIGTERM 300 ms after the first, in `run`'s select 5 runs and in `serve`'s 3: exit 1 at 306 to 325 ms, the engine gone first;
  - the real engine, SIGTERM once serving 3 runs and while starting 3: it exited on its standard input's end 4 to 19 ms in;
  - a panic in `start` once the engine runs (a probe build), 13 runs: exit 101 each time, a stubborn engine ended in order by SIGKILL, the lock free within 1 ms of its exit, and jawd gone 1 to 3 ms later.
- **The supervisor's restarts** happen while jawd holds the lock. The next engine starts only once the keeper says the last has exited (`engine/go.rs:168-172`, T4), after the backoff, and `session.lock` refuses one whose predecessor still runs (below).
- **The one release with the engine perhaps not yet reaped.** `Ending::end` waits for the end at once until the stop's deadline. Should the keeper not have said by then that the engine exited, jawd logs "its engine was sent SIGKILL, and wasn't seen to exit by the end of the stop" and goes on (`644-648`). By then `close` has sent SIGKILL to the engine's process group itself (`engine/go.rs:252`, T1), so the engine runs no more code. Should it still hold `session.lock` while the kernel tears it down, a next engine is refused and tried again. So there's no window with two engines on `session.db`.
- **On `jaw.db`'s side** there's a smaller residue: the lock goes a moment before the process exits (O11).
- **Two probe runs** at a load of about 90 found `jaw.lock` held 366 and 445 ms *after* jawd had exited. I couldn't explain them, and 8 later runs timed from jawd's spawn never showed it. A loop with no fork that took, let go and took again fresh lock files was never refused in 3,000 tries, so nothing on this Mac takes locks on new files. It's the safe direction: a lock held longer only refuses a second jawd.

### `session.lock` (O1, `d7c2006a`)

- **What it locks.** An exclusive, non-blocking `flock` on `<home>/session.lock` (`engine/internal/wa/session.go:31-51`), created with mode 0600. The file is kept in a package-level slice, so Go's garbage collector never closes it (the O1gc switch fails its test).
- **When it's taken and let go.**
  - `main` takes it after the watch on jawd and before `wa.Run` (`engine/cmd/jaw-engine/main.go:40-44`), so before the hello and before whatsmeow opens `session.db`. An engine whose jawd has already gone exits before it takes it.
  - The kernel lets it go when its last descriptor closes, which the engine's exit does however it exits.
  - Nothing else can hold it: jaw-engine starts no process, Go opens files close-on-exec, and jawd never opens it, so jawd's own children can't hold it either.
- **Refused,** the engine exits 1 at once, before it writes a byte of `session.db`, saying "another jaw-engine is running on …: it holds session.lock, so this one leaves session.db alone". jawd's failed start adds the engine's last line (`engine/go.rs:151-160`), and the supervisor tries again after its backoff, 500 ms doubling to 30 s. `jawd status` shows the refusal as the engine's last stop.
- **Never two engines on one `session.db`** (`restartprobe.py`, the real engine):
  - jawd SIGKILLed once serving, and a second jawd started at once, 10 runs: the old engine had gone before the first sample (its watch ended it), and the next one was first seen 316 to 1,500 ms later;
  - the same with the old engine kept alive 3 s past its jawd's death, 3 runs: a wrapper was its parent and held its standard input open, as a leaked descriptor would. The new engine was refused 3 times in each run, and ran 1.7 to 2.1 s after the old one was last seen;
  - the engine SIGKILLed 5 times while jawd served: each next one was first seen 1.3 to 8.2 s later, after the backoff;
  - `integrity_check` on `session.db` and `quick_check` on `jaw.db` passed on every home.
- **A stale lock can't keep jaw from starting for good.**
  - With `session.lock` held by another process, the engine was refused 4 times in 3 s, and ran 3.2 s after the lock was let go (its backoff had reached 4 s).
  - A lock left by an engine that died is free, as the Go and jawd tests and the crash runs above show. Only the file stays, empty and harmless.
  - Only a running process can hold the lock, and `jawd status` says so while one does.
- **On the data-audit list:** `engine/internal/wa/` was already, and `engine/cmd/jaw-engine/` now is (`scripts/check-data-audit.py:36`).

### T1 to T4

- **Each exists:**
  - T1, `engine::go::tests::ended_at_once_an_engine_is_killed_with_what_it_started_though_its_keeper_never_runs_again`;
  - T2, stopping's `a_second_sigterm_while_jawd_ends_its_engine_in_order_kills_it_at_once`;
  - T3, the reproduction's check that SIGTERM ended its engine;
  - T4, `engine::go::tests::an_instance_ends_once_its_engine_has_exited_not_once_its_standard_output_closes`.
- **Each fails with its rule off** (the T1 to T4 switches, below): "what the engine started outlived it"; "the engine outlived jawd"; the reproduction's words; and the instance's end coming before the engine's exit.
- **Under load.** T1's, T3's and T4's tests passed in every run: 3 baseline rounds at a load of 44 to 61, the full suite near 90, 5 whole stopping runs at 86 to 100, and 3 at 17 to 25. T2's test passed 41 of 43 times alone at a load of about 20, and 10 of 13 whole runs. Its failures, near 90 but for the two alone, were its log line twice and once its engine not started within the test's 10 s, with jawd's exit code, its time and the engine's end always as they should be (O5).

### O4 and 2b's words

- `after` (`engine/go.rs:344-358`) words the SIGTERM and SIGKILL steps, and the end at once, by the engine's exit status. SIGTERM's status gives "…so SIGTERM ended it" at any step; SIGKILL's gives "…so SIGKILL ended it", or "killed at once"; any other gives "exited by itself just as …". The W1 and O4 switches fail their tests.
- The first step still says "exited once its standard input closed", whatever the status. So a refused engine is logged "…exited once its standard input closed (exit status: 1)" (O8).
- **Nothing a test or an interface reads changed meaning.** The words reach interfaces only as `last_exit`, which `jawd status` prints as it is (`crates/jaw-client/src/lib.rs:187`). The terminal and the Mac app don't read it. Only `go.rs`'s own tests and `tests/stopping.rs` read the words, and each reads what the engine's status now gives.

### The pauses and the hidden options

- **Production's pauses are 1 s and 500 ms.**
  - `QUITTING` and `TERMINATING` (`engine/go.rs:32`, `34`) are `Pauses::default()`.
  - `run` builds its engine with `options.engine_pauses` (`main.rs:530`), which are the defaults unless `--engine-quitting` is given (`320-323`). Every other path takes `engine()`, so the defaults: `bench link` among them.
  - So production calls `with_pauses` only with those values; other values come only from tests (`fake::PATIENT`).
  - Measured: SIGTERM at 1.00 s and SIGKILL at 1.51 s after a stop while starting. No test pins the values (O10).
- **`--engine-quitting`** (`main.rs:74-75`) is a hidden clap argument, like `--hold-start` and `--busy-at-exit`, with no environment variable or config behind it.
  - jaw starts jawd with no arguments (`crates/jaw/src/wire.rs:212-219`), the LaunchAgent and the systemd unit give its path alone (`crates/jaw/src/agent.rs:107-110`, `143`), and Jaw.app's demo gives `--engine demo --no-notify` (`mac/JawApp/Sources/Agent.swift:76`). Only `tests/stopping.rs:358` passes it.
  - A negative value would panic in `Duration::from_secs_f64` before anything is opened, as `--busy-at-exit`'s and `--phone-patience`'s would.
- **The Supervision line** (the design's line 113) says what the code does. It leaves out the stop's own 3 s, which the design doesn't state anywhere, and the kill in its last 250 ms.

### The data-audit list

- `crates/jawd/src/main.rs` and `engine/cmd/jaw-engine/` are on `DATA_PATHS` (`scripts/check-data-audit.py:24`, `36`). At the head, `check-audit` asks trailers for exactly the nine the brief names. It doesn't ask for `9e48c3ce`, which touches only `tests/stopping.rs` (not on the list), or `39461d70` (docs and the script).
- **`home.rs` belongs there.** It's what makes the design's "left out of Time Machine and Spotlight" true, which decides what a backup can bring back. A `session.db` restored from a backup holds keys WhatsApp has moved on from, and a `jaw.db` and its WAL copied at different moments can come back torn. That's "One process" in Data integrity.
- **`jaw-proto/src/local.rs` belongs there too.** It decides where the folder is: a change there would start jawd on an empty folder beside the owner's data.
- No commit in this range touches either, so listing them now asks for no new trailers (O9).

### 2b's `jaw.lock` flake

- **Reproduced.** I ran `main.rs`'s 12 tests with the supervisor's 7 at a load of 40 to 70. A `jaw.lock` test failed in 8 of 22 rounds with the head's binary.
  - Four tests take the lock again in-process just after letting it go (`main.rs:1492`, `1518`, `1550` and `1583`). The last is the panic test's final check, which 2b didn't name.
  - In the full suite, none of the four failed.
- **The mechanism, measured.** A child being spawned at that moment holds a reference to every open descriptor until its exec closes the close-on-exec ones.
  - std forks for `jaw-media`'s `run()`, which sets a `pre_exec`. It uses `posix_spawn` for `GoEngine`'s command and for `Tools::find`'s (`spawntest.rs`, counting `pthread_atfork` calls with rustc 1.99.1).
  - With the `pre_exec` removed (a probe build), the lock tests failed in 3 of 12 rounds, against 7 of 12 with the head's binary, interleaved. So most of it is the fork, and the rest is `posix_spawn`'s own window inside the kernel.
- **It can't bite a real jawd beyond a refused start.**
  - A jawd opens `jaw.lock` once and lets it go once, a moment before it exits. A child being spawned then can only hold it a little longer, never let it go early, so it can't put two jawds on one `jaw.db`.
  - A jawd started in that instant exits saying "another jawd is running". launchd and systemd start the next jawd only after the old one has exited, and try again after 10 s; jaw shows the refusal.
  - jawd never opens `session.lock`, so its children can't hold up an engine's restart.
  - So it can be a follow-up (O7), and those four tests belong on the flakes list until it's done.

### Each rule's switch

I ran `switch.py` from 2b's files, pointed at the clone. Each switch turns one rule off by a text edit, runs its tests, writes the file back from a copy checked by hash, and rebuilds the engine after a Go switch. 41 switches ran from 14:28 to 15:23, with no compile error, and `git status` clean after each.

- **These fail their tests, as they should:**
  - round two's 31: S1 to S12 and S4serve; W ("still running 5.01s after jawd was killed"); O6; I1 ("jaw.lock was free 35.9ms after SIGTERM, while the engine still ran"); I1panic and I1b ("jaw.lock went before the runtime had shut down"); T1 to T4; O1none, O1hello, O1early, O1gc, O1shared, O1wait, O1file and O1words; and O4;
  - 2b's W1, P1 and P2;
  - three of mine: A1, `jaw.lock` taken after the socket is listened on (`a_jawd_refused_while_another_stops_leaves_no_socket_behind`); A2, `QUITTING` set to 3 s (five stopping tests); and A12, `stopped` without its own `Ending::end` (the panic test).
- **These pass every test** (O10): A3 and A4, `QUITTING` at 2 s and `TERMINATING` at 1 s; A6, `jaw.lock` let go before the engine's end on a panic's path only; and A7, the demo dropping its `jaw.lock` at once.

### The suite at the head

- **The baseline,** 3 rounds at a load of 44 to 61: Go's `go test -race -count=1 ./...` passed; so did jawd's 60 (the engine's 44, `main.rs`'s 12, sync's three stop tests and send's restart test) and stopping's 11. At 17 to 25 the stopping binary passed 3 more times.
- **The full suite,** `cargo test --workspace --no-fail-fast`, ran from 13:16 to 14:13 at a load of 17 to 100.
  - jawd's unit binary: 1,203 passed and 5 failed, all listed: the feed's lane test, media in's starts-check, the socket's connection-full, the checkpointer's flush test and the saved layout's lane test.
  - Every other target passed but stopping, which failed 3 near a load of 90. T2's test and `a_second_sigterm_stops_jawd_at_once_and_its_engine_with_it` failed on their log line (O5). The fixed never-answers test failed with "exit status: 0 after 5.259538209s", its stop noticed about 4 s late while jawd started (O6).
  - The watched tests that ran all passed: sync's stop bound, the restart test, the feed's starvation and coalescing tests, the WAL and blob tests, jaw-voice's recording-end and playhead tests, and the terminal's update-needed and engine-crash tests.
- **Sync's stop bound** runs an engine in jawd's own process, whose `end` is the trait's default. So the supervisor's new end in order is ready at once, and the test behaves as on main: its quickest of three stops is under 2.5 s, and the stop stays unclean.
- **The fixed never-answers test** passed alone 13 of 13 times (at loads of 17 to 100), and in every whole run at 17 to 61 (7 of 7). It failed twice near 90: in the full suite as above, and in one of 5 whole runs at 86 to 100 with "the engine never started", because jawd's start took longer than the test's 10 s. Its engine never outlived jawd.
- **At 86 to 100,** 5 whole stopping runs had 8 failures between them. Every one was a timeout of the test's own (an engine not started within 10 s, a failing start not failed within 6 s) or the log line of O5. None was an engine outliving jawd or `jaw.lock` free while an engine ran. Each test that failed then passed alone 3 of 3, except T2's, which passed 2 of 3 and then 39 of 40.
- **The branch's own** `tests::an_engine_still_running_when_the_stop_has_a_moment_left_is_killed_by_its_end` failed in 2 of 22 rounds of `main.rs`'s and the supervisor's tests together, and passed alone 23 of 23 (O5).

## Findings

### Data loss

None.

### Inconsistency

None. Round one's I1 is fixed and tested.

### Test gaps

None. Round one's T1 to T4 each have a test that fails without its rule. O5 and O10 are about how well tests hold up and what they pin, not about rules left untested.

### Unpredictable performance

None. The range adds no SQL, statement, index or PRAGMA, and nothing on the live path but round one's writer `select!`, whose `bench link` numbers stand.

### Other (follow-ups; none blocks)

- **O5. Tests that wait for the keeper's word on a SIGKILLed engine sometimes get it too late.**
  - **Where:** `tests/stopping.rs:308` (T2's test) and `:387` (round one's second-signal test), against `Ending::at_once` (`main.rs:652-656`); and `main.rs:1598`, against `Ending::end` (`636-649`).
  - **What happens:** on a second signal, jawd kills its engine (with `close`'s own SIGKILL) and waits up to 250 ms for the keeper to say how it ended, then exits 1. Under `nice` at a load of about 20, and more often near 90, the keeper's word came later, so jawd exited without the line those tests look for. The S3 test's check takes a killed engine that its keeper hasn't reaped yet for one still running, since `kill(pid, 0)` succeeds on a zombie. In every failure the rule held: jawd exited 1 in time with its engine gone, or the stop's end had sent the engine SIGKILL. The only thing lost is the log line.
  - **How often:** T2's test 2 of 43 times alone at a load of about 20; the two second-signal tests 2 times each in 13 whole runs, near 90; the S3 test in 2 of 22 rounds beside `main.rs`'s and the supervisor's tests, and 0 of 23 alone.
  - **Fix:** have `at_once`, and `Ending::end` when it gives up, log that the engine was sent SIGKILL as they stop waiting, and have the tests accept either line; and have the S3 test count an exited but unreaped engine as gone, as `go.rs`'s `gone()` does.
- **O6. A stop that comes while jawd starts waits for media's synchronous start.**
  - **Where:** `start` calls `media::Media::start` (`main.rs:766-767`), which calls `jaw_media::Media::new`, so `Tools::find` (`crates/jaw-media/src/tools.rs:50-75`). That runs `ffmpeg -version` and `ffmpeg -hide_banner -encoders` to the end on the thread that polls `serve`'s select (`main.rs:573-580`). A signal that comes then is seen only when `start` next yields.
  - **Measured** with a probe build that stamps `start`'s steps, 8 jawds started at once: in 24 of 24 stops the signal waited there. It waited 16 to 86 ms at a load of about 20, about 330 ms at 45, and once about 4 s near 90, where the fixed never-answers test took 5.26 s against its 3 s.
  - **The branch's part:** it adds the end in order, 1 to 1.5 s, to such a stop. jawd's own bound counts from when the stop is seen (`Ending::begin`).
  - **No data risk:** `jaw.lock` is held throughout, and the engine goes before jawd does. launchd and systemd wait 10 s. After a panic, nothing listens for a second signal while the engine ends, which takes at most 3 s.
  - **Fix:** find the tools on a blocking thread, or once in `run` after `jaw.lock` is taken and before the runtime; and count the bound from the signal.
- **O7. Four tests take `jaw.lock` again just after letting it go (2b's flake).**
  - **Where:** `main.rs:1492`, `1518`, `1550` and `1583`.
  - **What happens:** a child that another test spawns at that moment holds the lock's descriptor until its exec. A fork (`crates/jaw-media/src/run.rs:49`) does most of it, and `posix_spawn` the rest. It failed in 8 of 22 rounds with the head's binary in the set above, and never in the full suite. In a real jawd it can only refuse a start, as above.
  - **Fix:** the four tests retry the lock for up to a second. Dropping the `pre_exec` (for example, lowering the child's priority by its PID once it has started) narrows the window but doesn't close it.
- **O8. The end in order's first step isn't worded by status.**
  - **Where:** `engine/go.rs:313-315`.
  - **What happens:** an engine that exits by itself as its standard input closes is said to have "exited once its standard input closed", in the log and in `jawd status`, whatever its status. One refused by `session.lock` reads "(exit status: 1)", as in the held-lock runs. The status in brackets tells the truth.
  - **Fix:** as `after` does, keep those words for status 0 and say "exited" with the status otherwise, or look at `try_wait` before telling the engine to end.
- **O9. `home.rs` and `jaw-proto/src/local.rs` belong on the data-audit list,** as above: one line each in `DATA_PATHS`, in this branch or the next.
- **O10. Four of my switches pass every test.**
  - A3 and A4, production's `QUITTING` at 2 s or `TERMINATING` at 1 s. The stopping tests pin only that the end in order fits in the 2.75 s. A test of `Pauses::default()`, and of the pauses `Options` gives without `--engine-quitting`, would pin the design's 1 s and 500 ms.
  - A6, `jaw.lock` let go early on a panic's path only. The panic test checks the engine's end, and the lock only afterwards. Today the panic's path runs the same code that the shutdown test pins; a witness like that test's, in a task the panic leaves behind, would pin the lock through a panic's end too.
  - A7, the demo dropping `jaw.lock` at once. The demo's engine runs inside jawd's own process, so no engine can outlive the lock. The demo holds the lock for `serve`'s scope (`demo/mod.rs:237`), so it goes before the demo's runtime shuts down, as I1's did, and nothing tests it.
- **O11. `jaw.lock` goes a moment before the process exits.** `stopped` drops it (`main.rs:564`) and returns through `run` to the exit, while the store's own threads, and any blocking work the shutdown left behind, end only at the exit. A second jawd would have to take the lock in those microseconds, and SQLite keeps `jaw.db` whole between two processes. Letting the descriptor close at the exit would make "for as long as it runs" literal; the tests that take the lock again in-process would then drop it themselves.
- **O12. The stopping tests' guard can miss a fake engine that starts late.** At a load near 100, three tests failed with "the engine never started" within their 10 s. Their fakes wrote their PIDs only after the guard had SIGKILLed jawd, so no `Fake` killed them, and three sleeps ran on until I killed them by their PIDs. The guard could send SIGTERM and wait for jawd's own end before SIGKILL, or a `Fake` could wait a moment for its PID.
- Round one's **O3** stands: the watch's "jawd had already gone" checks are untested.

## Measurements

**Method.**
- One clone of the worktree at `39461d70`, built there with `CARGO_INCREMENTAL=0`.
- Binaries kept: jawd and jaw-engine in `bin/head`; a probe build that panics in `start` when `JAW_AUDIT_PANIC` is set, in `bin/panic`; one that stamps `start`'s steps, in `bin/steps`; and the unit binary without `jaw-media`'s `pre_exec`, in `bin/nopre`.
- Scratch homes lived under the audit's folder, were never linked, and gave jawd `JAW_HOME`, `JAW_ENGINE` and `JAW_NOTIFY=off`.
- Exits were timed by kqueue's `NOTE_EXIT`. `jaw.lock` was probed every 0.5 ms with `flock(LOCK_EX | LOCK_NB)` on a descriptor of the probe's own, let go at once. A violation is the lock found free while the engine hadn't exited and `ps` said it ran. Engines on a home were counted with `ps`.
- The load was 17 to 100 on 20 cores, with everything under `nice -n 10`.

**Runs.**
- `lockprobe.py`: the 33 stops listed under I1. `panicprobe.py`: the 13 panics.
- `restartprobe.py`: the runs listed under `session.lock`. A first set of three lingering-engine runs is left out: it had the second jawd on the wrapper too, whose shell kept a refused engine's output open until the hello timeout. The crash, engine-kill and held-lock runs were started twice at once by a doubled shell call. Each copy had homes of its own, so neither touched the other's engines or locks, and each line counted above is one home's result. The copies wrote over each other's lines in their shared log, so the counts are the lines that could be read, not every run.
- `noticeprobe.py`: 8 jawds at once, each with a silent engine, SIGTERM once the engine runs, and the log stamped as lines arrive. The engine was either the stopping tests' shell, which every SIGTERM came after jawd served (noticed within 34 ms; exit at 1.02 to 1.07 s), or a compiled one that starts at once, which every SIGTERM came while jawd started (exit at 1.04 to 1.36 s at a load of 45). With the step stamps, at 17 to 22, the numbers are as under O6.
- `spawntest.rs`: std's spawn paths, as under the flake.
- `flake.sh`: `main.rs`'s 12 tests and the supervisor's 7 from one binary per round; 10 and then 12 rounds with the head's binary, 12 interleaved with the build without `pre_exec`.
- `switch.py`, `base.sh` and the full suite: as above. `jawd bench link`'s numbers are round one's: this range changes nothing on its path.

## Landing, once approved

- **Rebase onto `origin/main` (`1708094e` or later), don't merge.** `check-audit` reads a merge commit against each of its parents (`git diff-tree -m`), so a merge commit would itself touch `main.rs` and need a trailer.
- **Onto `1708094e`,** done in the clone (branch `trial-landing`, `rerere` off):
  - The first four commits apply cleanly. `579a1a1f` conflicts in `run()`, one hunk: main's `#[cfg(target_os = "macos")] home::keep_out(home);` against the branch's `let lock = lock(home)?;`, both just after `set_permissions`. Keep both, the lock first:

    ```rust
    // Taken before the socket is listened on, so a jawd refused never touches it.
    let lock = lock(home)?;
    #[cfg(target_os = "macos")]
    home::keep_out(home);
    ```

    That way a refused jawd leaves the folder's attributes as it found them, as it leaves the socket. The other order is safe too, since `keep_out` only sets what's missing.
  - The other six commits apply cleanly. The result is main's 14 files and the branch's 17, and nothing else.
  - On it, all pass: `make engine` and `cargo build --workspace`, `cargo clippy -p jawd --all-targets -- -D warnings`, `cargo fmt --all --check`, the 60 unit tests, stopping's 11 and `starting.rs`'s 3. It's no more than keeping both sides.
- **If the disappearing branch (`7d6499c9`, migration 31) lands first:** main with it merges cleanly, and this branch on top has the same one hunk, while `check-data-audit.py` and the design merge by themselves. There's nothing more to check than above.
- **If the disk watchdog (`ad3402e5`) lands first, it's more than keeping both sides.** It splits `start` into `start` and `start_watching(home, lock, engine, options, disk)`, which still takes the lock this branch moved into `run`, and it creates `pushes` before the store opens. That makes four hunks in `main.rs`, resolved like this:
  - `start(home, engine, options, supervised)` calls `start_watching(home, engine, options, supervised, disk::Disk::watch(...))`, which takes no lock;
  - `start_watching` keeps the watchdog's early `pushes`, drops the branch's second one, and sets `supervised` straight after `Supervisor::spawn`, as the branch does;
  - the watchdog's new tests in `main.rs` take `let (_lock, listener) = open(...)`, and call `start(…, &OnceLock::new())` and `start_watching(…, &OnceLock::new(), disk.clone())`.

  The lander then checks `cargo build`, `make lint` (a `lock` parameter left unused fails clippy), `main.rs`'s tests (the watchdog's two and `jaw_lock_goes_only_once_the_runtime_has_shut_down` among them), stopping's 11 and `starting.rs`. The watchdog's tasks run on the runtime, so they end before `jaw.lock` goes, and its store opens after `run` has taken the lock.
- **Trailers:** `Data-Audit: docs/audits/2026-11-03-engine-stop.md` on the nine commits `check-audit` names, as rebased: `3d01f843`, `880f637a`, `bb8282c3`, `579a1a1f`, `20a39d83`, `d7c2006a`, `85e1f13b`, `72e76d96` and `ea7e4746` (on the trial, `2a2eb653`, `82008bef`, `8dc189c3`, `6ea49fa8`, `dc54f9d7`, `890823dd`, `c7d7017a`, `cb8d660e` and `72d66923`). Not `9e48c3ce` or `39461d70`. Commit this note in the branch.
- **No migration.** The range adds no SQL, schema, index, trigger or PRAGMA, and changes neither `jaw.db`'s nor `session.db`'s format nor the link protocol. `session.lock` is a new empty file beside `session.db`, which no version reads.
- **The gate.** At the brief's load the stopping tests pass. Above about 80 they fail on their own timeouts and on O5's log line. The fixed never-answers test is best judged at a load like the brief's. Until O5 and O7 are done, these belong on the flakes list: the four lock tests, `tests::an_engine_still_running_when_the_stop_has_a_moment_left_is_killed_by_its_end`, and stopping's two second-signal tests.

# Round one: `bab95fc2..d0e00176`, kept as written

# Audit: jawd ends its engine however it stops (`engine-stop`, M8)

Range: `bab95fc2..d0e00176` in `engine-stop`, four commits, 3 November 2026. Auditor: database-expert. Round one, by two auditors in turn: the first measured the real engine's stops, ran the full suite and most of the switches; the second, from 15:00, finished the switches, measured `jaw.lock` against the engine's end, judged the suite's failures, checked `session.db` across each kind of stop and the hidden options, and made the trial merge. Each found I1 on its own.

- `df1e5d7e`: the stopping tests' guard. Tests only.
- `601085c4`: a deterministic reproduction, and two hidden test-only options, `--hold-start` and `--busy-at-exit`.
- `6fe180e2`: jawd ends its engine in order however it stops (its standard input closed, then SIGTERM to its process group after 1 s, then SIGKILL 500 ms later), and waits for it within its 3 s.
- `d0e00176`: jaw-engine exits as soon as jawd has gone (a kqueue on macOS, a pidfd on Linux).

Paths are under `crates/jawd/src/` unless they say otherwise, with line numbers at `d0e00176`. The worktree stayed at `d0e00176`, with nothing uncommitted but this note. Everything was measured in one clone of it at `/tmp/jaw-audits/engine-stop/jaw`, built with `CARGO_INCREMENTAL=0` under `nice`, at a load of 20 to 80 on a shared Mac with 20 cores. The real jaw-engine ran only on scratch homes with no account linked, where it never connects: nothing was sent to WhatsApp, and no process but the audit's own was signalled.

## In short

- **Changes needed.** One inconsistency (I1), and four rules the change touches that no test fails without (T1 to T4).
- **The fix does what it says.** However jawd stops (SIGTERM or Ctrl-C while serving or while starting, a second signal, a start that fails, a panic, an orderly stop that runs out of time), its engine has gone by the time jawd exits, ended in order when there's time, and the supervisor notes a clean stop only once it has. jaw-engine exits within milliseconds of jawd dying outright, even with its standard input held open by another process. Every builder switch fails its test (S1 to S12, G and W), or, for S13 and S14, their measurement by hand.
- **`session.db` comes through every kind of stop whole.** Its settings make any exit safe, whatsmeow commits each step with what depends on it, and nothing is acknowledged to WhatsApp that jawd didn't store. 60 kills inside the engine's opening of `session.db`, and every home of every run, pass `integrity_check`.
- **I1, inconsistency: `jaw.lock` comes free before the engine has gone,** on four of the stops this change is about: a stop while starting, a start that fails, a panic, and an orderly stop that runs out of its 2.75 s. A jawd started then, as `jaw` starts one when nothing answers on the socket, opens `jaw.db` beside the first and starts a second engine on `session.db`. Measured: two real engines on one `session.db` at once in 5 of 5 runs, with the first engine slow to go within jawd's own 1 s pause.
- **T1 to T4, test gaps:** `close`'s own SIGKILL (T1), a second signal while `run` ends the engine (T2), a stop while starting ending the engine in order (T3), and an instance's end waiting for its engine's exit (T4).
- **The full suite at the head** fails only five timing tests, all on the landers' list, each passing alone 3 of 3.
- **`session.lock`** isn't implemented. With I1 fixed, it still leaves open an engine run by hand, an engine that can't run its watch, and a restart under long scheduling delays. It should come before the first release, but needn't block this branch.
- **Landing.** No migration. The branch merges onto main `b4d9624d` without a conflict, builds, and the change's tests pass on the merge. `6fe180e2` takes the trailer, and so does `d0e00176` once `engine/cmd/jaw-engine/` is on the data-audit list, where it belongs.

## What holds

### `session.db`, however jawd and its engine stop

- **Its settings make any exit safe.** The engine opens `session.db` in WAL mode with `synchronous=FULL`, foreign keys, a 5 s busy timeout and immediate transactions. Its connect hook sets `checkpoint_fullfsync` and `journal_size_limit`, and each setting is read back and refused if wrong (`engine/internal/wa/session.go:25-38`).
  - A process that dies at any instruction, by SIGKILL, by SIGTERM (the engine installs no handler, so SIGTERM is its default action), or by `os.Exit` from the watch or at its standard input's end, loses only a transaction it hadn't committed. SQLite rolls that back at the next open.
  - A power cut is no different from before this change.
- **whatsmeow writes nothing half,** at the pinned `df33a485cc46`:
  - Each schema upgrade runs in one transaction with its version: dbutil's default is `txn = "on"`, and no upgrade file turns it off.
  - A message's decryption, which moves its Signal session on, commits in one `DoTxn` with its plaintext in the event buffer (`message.go:559-568`). The plaintext is cleared only once the handler has returned.
- **Nothing is acknowledged that jawd hasn't kept.** The handler returns only on jawd's ack, which jawd sends after the inbox's synced commit.
  - Once jawd has gone, `keep` never returns (`engine/internal/wa/engine.go:206-209`), so whatsmeow never tells WhatsApp a message arrived that jawd didn't store.
  - At its standard input's end the engine disconnects and exits at once, without waiting for its handlers (`engine/cmd/jaw-engine/main.go:40-46`); so does the watch.
  - A step of whatsmeow's that spans transactions can be cut between them. SIGKILL could cut it the same way before this change, when every stop killed the engine, and so can a crash.
- **An ack lost at the end costs a redelivery, not a message.** Told to end, the writer drops what it hasn't written (`engine/go.rs:372-382`). An ack still in its buffer never reaches the engine, so WhatsApp sends that message again, and the inbox takes it once. A frame cut in half is never applied: the engine reads a frame whole or fails, and exits.
- **Measured.** Each home below was checked afterwards with `integrity_check` on `session.db` and `quick_check` on `jaw.db`, and every one passed:
  - the engine's own stop, on SIGTERM to jawd once serving (5 runs, and 5 more in the lock runs);
  - SIGTERM at a random moment after the engine's spawn (20);
  - a stop while starting (10, and 15 more with a second jawd started after);
  - a start that fails (6);
  - a second SIGTERM 2 ms after a stop while starting, which kills the engine at once (10);
  - SIGKILL of jawd, the engine exiting through its watch (10, and 10 restarts);
  - two engines on one `session.db` at once (5);
  - 60 fresh homes killed a random 0 to 80 ms into the engine's opening of `session.db`: 20 by SIGKILL, 20 by SIGTERM, and 20 by SIGKILL of their stand-in jawd, so that the watch exited the engine mid-open. After each, a clean open gave a schema identical to a never-killed one's.

### One engine to a jawd, and a clean stop noted only once it has gone

- **Within one jawd, never two.**
  - `Running::start` spawns under the lock `close` takes, so an engine is either refused or among those `close` ends (`engine/go.rs:194-231`).
  - Once ended, `GoEngine` starts no other (S10).
  - A start whose handshake fails, or which is told to end, has its engine gone before it fails (`engine/go.rs:129-136`, S11).
  - The supervisor starts the next engine only once the last has exited, since `exited` resolves on the keeper's word (`engine/go.rs:143-147`, untested: T4), and only after its 500 ms backoff.
- **A clean stop is noted only once the engine has gone.** The supervisor ends the engine in order before it says `Stopped` (`engine/supervisor.rs:182`, S5), and `noted_clean` writes `clean = 1` only once it's `Stopped` (`sync/mod.rs:667-679`). `clean` gates only the next start's "didn't stop cleanly" line.
- **However jawd stops, the engine goes before jawd does.**
  - `stopped` runs the serving future under `catch_unwind`, then `Ending::end`, which stops the supervisor and kills whatever still runs 250 ms before the 3 s bound. Only then does the runtime shut down, and only then does a panic go on (`main.rs:533-547`; S1 to S3).
  - The supervisor is recorded as soon as `start` spawns it (`main.rs:733-735`).
  - A second signal kills the engine before `exit(1)` in both selects (`main.rs:521-525`, `575-579`; S4).
  - `bench link` ends its engine before it removes its folder (`main.rs:928`; S14).
- **The 3 s bound holds.** With an engine that said hello, never answers, and ignores its standard input closing and SIGTERM, jawd exits 2,773 to 2,820 ms after SIGTERM over 6 runs, the engine killed at once at 2.75 s. With S13 off it takes 3,010 to 3,108 ms.
- **After a crash, the engine goes within milliseconds.** launchd kills a dead job's own process group, and the engine has a group of its own, so its watch is what ends it.
  - The engine outlived jawd's SIGKILL by 0 to 19 ms over 10 runs.
  - With the watch off (W) it was 11 to 124 ms, and for good with its standard input held open by another process (the Go test).
- **At a restart after a crash, no overlap.** I SIGKILLed jawd once its engine served, and at once started another jawd on the same home.
  - Over 10 runs, the old engine was gone 4 to 11 ms after the kill, and the new jawd's engine spawned 118 to 155 ms after it.
  - With W off, over 20 runs: 10 to 158 ms, and 135 to 455 ms.
  - launchd and systemd start a new jawd only once the old one has exited. launchd's `ThrottleInterval` counts from spawn to spawn, so a jawd that ran over 10 s is started again as soon as it exits; jawd now exits only once its engine has gone.
- **The process group.**
  - The engine starts in a group of its own (`engine/go.rs:73`), so signals reach it and whatever it starts (S12).
  - jaw-engine starts nothing today. jawd's own children, ffmpeg and the like, stay in jawd's group, and none of the owner's processes can be in the engine's.
  - Signals go to the group only while the engine is unreaped (`engine/go.rs:325-331`), or under the registry lock before its keeper forgets it (`engine/go.rs:222-228`), so the group's ID is still the engine's.

### The hidden options

- **They do nothing unless given, and no interface can reach them.**
  - `--hold-start` (`main.rs:66`, used at `750-752`) and `--busy-at-exit` (`main.rs:70`, used at `541-543`) are hidden clap arguments. Neither has an environment variable (no argument of jawd's has one) or a config behind it.
  - `jaw` starts jawd with no arguments (`crates/jaw/src/wire.rs:212-219`). The LaunchAgent and the systemd unit give its path alone (`crates/jaw/src/agent.rs:107-110`, `143`), and Jaw.app's demo `--engine demo --no-notify` (`mac/JawApp/Sources/Agent.swift:76`).
  - Only `tests/stopping.rs:252` passes them.
- **They stay in the binary,** as `--fail` and `--without` already do. The stopping tests run the jawd that ships. A test-only build couldn't do that: `cfg(test)` doesn't reach integration tests, and a cargo feature would mean testing a second binary.

### The suite at the head

- **Go:** `go test -race -count=1 ./...` passed.
- **Rust:** `cargo test --workspace --no-fail-fast` ran 27 targets at a load of 22 to 71: 1,687 passed, 5 failed, 25 ignored. All five failures are in jawd's unit binary, every one is on the landers' list, and each passed alone 3 of 3 with the suite's own binary (15 of 15, the second time at a load of about 70):
  - feed's `a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends` (the feed's half-closed tests);
  - feed's `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most` (listed by name);
  - sock's `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push` (the socket's connection-full test: "nothing went missing: Elapsed");
  - store's `the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it` and `the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end` (the checkpointer's flush tests: "never flushed jaw.db" within their wait).
- **None of the five runs through this change's code.** Their engines are in jawd's own process, or absent, and `Engine::end`'s default does nothing.
- **The watched tests all passed in the suite.**
  - Sync's stop bound, `an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean`, also passed alone 3 of 3. Its engine is in jawd's own process, so the supervisor's new `engine.end(End::InOrder)` before `Stopped` is ready at once, and the stop stays unclean as before.
  - So did the restart tests, the feed's starvation and coalescing tests, the WAL tests, the blob tests, and jaw-voice's recording-end and playhead tests.
  - The stopping tests passed 9 of 9 in the suite and 3 more times whole. The fixed test, `jawd_whose_engine_never_answers_stops_on_sigterm_and_takes_its_engine_with_it`, passed 10 of 10 alone, with no fake engine left after any.

## Findings

### Data loss

None.

### I1. Inconsistency: `jaw.lock` comes free before the engine has gone

**Where:** `main.rs:553-559`, `705`, `716`, `539` and `571`, against `520` and `540`.

**What happens:**
- `serve` gives `jaw.lock` to `start_on` (`main.rs:553-555`), which gives it to `start` (`705`, `716`). `Started` then keeps it (`_lock`, `659-660`) only as long as `stop`'s future lives. So the lock goes as soon as the serving future does:
  - a signal while starting drops the `start_on` future in `serve`'s first select (`556-559`);
  - a start step that fails after the engine's spawn (media's, sending's, and on main voice's) drops it with `start`'s locals;
  - a panic drops it as the serving future unwinds (`539`);
  - an orderly stop that runs out of its 2.75 s drops `stop`'s future, and `Started` with it (`571`).
- Only then do `run` and `stopped` end the engine (`main.rs:520`, `540`), in order, which can take `QUITTING` and `TERMINATING`, 1.5 s, of the 3 s bound. Meanwhile jawd's own tasks (`sync::run`, the feed, the store's threads) run on until the runtime shuts down (`545`).
- The design's "One process" says jawd holds `jaw.lock` for as long as it runs. With `session.lock` not implemented, `jaw.lock` is all that keeps a second jawd, and so a second engine, off the folder. Jaw.app's agent counts on it: "jawd's `jaw.lock` refuses a second jawd whoever starts it" (`mac/JawApp/Sources/Agent.swift:5-7`).
- Something does start one in that moment. `jaw` starts a jawd of its own whenever nothing answers on the socket as it starts (`crates/jaw/src/wire.rs:285`), and a jawd stopped while starting has removed its socket first (`main.rs:558`). So would the owner running `jawd` by hand. launchd and systemd don't: they wait for jawd's exit.
- So a second jawd can open `jaw.db` while the first's tasks still write it, which `jaw.lock` is there to prevent ("two would race the backup an upgrade takes, and each other's writes", `main.rs:663-664`). It can also start a second engine on `session.db` while the first's still runs. Two engines on one device's keys each move its Signal sessions and pre-keys, which can leave messages that can't be decrypted.
- The orderly stop is right: `_lock` goes only when `stop` returns, after `sync::stop` has waited for `Stopped`, so after the engine has gone (measured: 10 to 14 ms after).
- The shape is older than the branch: the lock went at the same points before, and the engine was left to the runtime. But these are the stops this change makes end the engine, and its own in-order end is the window.

**Measured** (method under Measurements; times are from the SIGTERM):
- A stop while starting let the lock go at 1 ms, while the engine exited at 3 to 128 ms (10 runs).
- A start that fails let it go 1.6 to 4.4 ms before its engine had gone (5 runs), and a stop that overran 0.4 to 1.0 ms before (3 runs).
- A second jawd, started the moment the lock came free, ran beside the first in 15 of 15 runs, while the first still had `jaw.db` open and its tasks running, for up to 228 ms. With a healthy engine their engines didn't overlap: the first had gone at 4 to 226 ms, and the second spawned at 208 to 702 ms.
- With the first engine slow to go within jawd's own 1 s pause, they overlapped in 5 of 5 runs. The second engine had opened `session.db` and answered a ping at 243 to 294 ms, so its jawd had opened `jaw.db` before then, while the first engine and its jawd ran until about 1,010 ms.
- The first auditor's runs, with a wedged fake engine, had two engines at once in 3 of 5, for about 1.1 s each.

**Fix:** hold `jaw.lock` until the engine has gone and the runtime has shut down. For example, take it in `run` before `stopped` and drop it only when `run` returns; the `exit(1)` paths let it go at exit. Or give `Ending` a `try_clone()` of the lock's file, dropped after `stopped`'s `shutdown_timeout`: a `flock` lasts while any descriptor of its open file does.

**Test:** in `stopping.rs`, jawd with `--hold-start` and an engine that ignores its standard input closing and SIGTERM. After SIGTERM, while the engine still runs, `flock(jaw.lock, LOCK_EX | LOCK_NB)` must fail with `EWOULDBLOCK`. Do the same in `jawd_that_fails_to_start_takes_its_engine_with_it` once its engine runs. At the head the lock is free while the engine runs (measured), so both fail there.

### T1. Test gap: an end at once sends SIGKILL itself

**Where:** `engine/go.rs:222-228`, in `close`.

**What happens:** told to end at once, `close` sends SIGKILL to each engine's group itself, under the registry lock, so the engine is killed even if its keeper's task never runs again before jawd exits. The builder added it while measuring S13, after 2 of 3 runs without it left the engine running. Switched off (O1), go.rs's 8 unit tests, `tests::an_engine_still_running_when_the_stop_has_a_moment_left_is_killed_by_its_end` and stopping's 9 all pass, because the keeper's own SIGKILL in `ended` kills the engine a poll later.

**Test:** a `GoEngine` test whose fake starts a child of its own, in its group, and ignores its standard input closing and SIGTERM. Call `close(End::AtOnce)` and drop the runtime before the keeper runs again: neither process may be left. Without `close`'s SIGKILL only `kill_on_drop` acts, which kills the fake's PID alone, so its child outlives it.

### T2. Test gap: a second signal while `run` ends the engine

**Where:** `main.rs:521-525`.

**What happens:** a second signal while `run` ends the engine, after a stop while starting, a failed start or a panic, kills the engine before `exit(1)`. Switched off (S4run: the first `ending.at_once()` only), `a_second_sigterm_stops_jawd_at_once_and_its_engine_with_it` and the reproduction both pass, because the test's second signal comes during the orderly stop, in `serve`'s select. jawd would then exit 1 and leave the engine to its watch. With the real engine the rule holds: 8 runs reached this branch, and the engine had gone first in each.

**Test:** `--hold-start` and an engine that ignores its standard input closing and SIGTERM; SIGTERM, then a second during the 1 s pause. jawd must exit 1 well before 1.5 s, with "killed at once", and its engine gone at its exit.

### T3. Test gap: a stop while starting ends the engine in order

**Where:** `main.rs:733-735`.

**What happens:** `start` records its supervisor in `Ending` as soon as it's made, so a stop while starting stops the supervisor, which ends the engine in order rather than at once. Switched off (O7), both stopping tests on this path pass. The reproduction checks only that the engine has gone, and the failing start checks how it went only when its fake had started before media failed, which it can't force. Main's own tests set the supervisor themselves.

**Test:** the reproduction asserts how its engine ended, "jaw-engine didn't exit once its standard input closed, so SIGTERM ended it", as the other stopping tests do.

### T4. Test gap: an instance's end waits for its engine's exit

**Where:** `engine/go.rs:143-147`.

**What happens:** an instance's `exited` resolves on the keeper's word that the engine has exited, so the supervisor starts the next engine only once the last has gone. Switched off (O8: `exited` resolving once the engine's standard output closes), go.rs's 8, the supervisor's 7 and stopping's 9 tests all pass. Before the branch, `exited` waited on `child.wait()`. Today the supervisor's 500 ms backoff keeps the gap harmless.

**Test:** a fake that says hello, closes its standard output (`exec >&-`), then sleeps a moment and writes a file before it exits. `instance.exited` must resolve only once the file is there.

### Unpredictable performance

None. The live path's only change is the writer's `select!` around its loop, which polls a watch receiver. `jawd bench link --commit` at the head (debug jawd, real engine, 600-byte messages, 3 runs at a load of 30 to 70) gave p50 5,069 to 5,138 µs and 142 to 180 a second, the flush most of it. The review's `FULL` and `fullfsync` baseline is p50 5,003 µs and 204 a second.

### O1. Other: `session.lock`

The design has the engine take `session.lock` before `sqlstore.New` and hold it for as long as it runs, and write `session.db.before-whatsmeow-vN` before an upgrade. Nothing in `engine/` does either. With I1 fixed, two engines can still share `session.db` in three ways:
- an engine run by hand, or by an agent, on a live folder: there's no jawd, so no `jaw.lock`;
- an engine that can't run its watch when its jawd dies: one stopped by SIGSTOP or a debugger, or one on a system with neither kqueue nor pidfd, which says it can't watch and relies on its standard input closing, which a leaked descriptor can hold open;
- a restart under scheduling delays longer than a new jawd's start. Measured, the old engine had gone 4 to 11 ms after its jawd's SIGKILL, and the new one spawned 118 to 155 ms after.

A `flock` on `session.lock`, taken before `sqlstore.New`, closes all three by construction, since the system lets it go at any exit. Its refusal should reach jawd as a state the supervisor backs off on, so that a restart waits for the old engine rather than failing. It should come before the first release. It needn't block this branch once I1 is fixed.

### O2. Other: `engine/cmd/jaw-engine/` belongs on the data-audit list

Its `main` decides how the engine exits, so how `session.db`'s writes end and that nothing is acknowledged once jawd has gone (`main.go:40-46`, `parent.go`), and the rule counts an ack "wherever it is". Add it to `DATA_PATHS` in `scripts/check-data-audit.py` in this branch; `d0e00176` then takes the trailer too. `main.rs` holds `jaw.lock`, the stop's order and now the engine's end, and I1's fix goes there. Listing it, or moving those into a listed module, is a follow-up, and I1's fix commit should take the trailer either way.

### O3. Other: the watch's "jawd had already gone" checks are untested

Switched off (ESRCH, and `getppid` having changed: `parent_darwin.go:25-38`, `parent_linux.go:15-27`), every Go test passes. This is harmless. An engine whose jawd went first exits on its standard input's end, or on its hello's SIGPIPE, before it opens `session.db`, which it does only after jawd's hello (`engine/internal/wa/engine.go:60-75`).

### O4. Other: an engine killed at once that had already exited is said to be killed

The keeper reads what it was told before the engine's exit (`engine/go.rs:281-285`). So an engine that exited on its own as it was told to end at once is logged "jaw-engine was killed at once, with SIGKILL (exit status: 0)", as in 4 of the second-signal runs. The status in brackets tells the truth; only the wording misleads.

## Measurements

**Method.** One clone of the worktree at `d0e00176`. jawd and jaw-engine were built there as debug builds and kept in `bin/head`; the same with W off in `bin/w`, and with S13 and S14 off in `bin/s13s14`. Scratch homes lived under the audit's folder, never linked, with jawd given `JAW_HOME`, `JAW_ENGINE` and `JAW_NOTIFY=off`. Exits were timed by kqueue's `NOTE_EXIT`, or by polling, and the lock by `flock(LOCK_EX | LOCK_NB)` on a descriptor of the script's own, let go at once. The load was 20 to 80 on 20 cores, with jawd under `nice`.

- **The real engine's stops** (`real.py`):
  - SIGTERM once serving, 5 runs: "jaw-engine exited once its standard input closed (exit status: 0)", jawd stopped in 14 to 54 ms, and the engine had gone at jawd's exit.
  - SIGTERM at a random moment after the engine's spawn, 20 runs (3 while starting): the engine had gone at jawd's exit every time.
  - SIGKILL of jawd once serving, 10 runs: the engine outlived it by 0 to 19 ms; with W off, 11 to 124 ms.
  - A restart, SIGKILL and at once a second jawd: the old engine was gone at 4 to 11 ms and the new one spawned at 118 to 155 ms (10 runs); with W off, 10 to 158 ms and 135 to 455 ms (20 runs).
- **`session.db` killed mid-open** (`sessionkill.py`): 60 fresh homes, each killed a random 0 to 80 ms after the hello, 20 by SIGKILL to the engine, 20 by SIGTERM to it, and 20 by SIGKILL to the stand-in jawd, after which the engine exited through its watch 7 to 17 ms later. All 60 passed `integrity_check`: 3 had no table yet, 4 only an empty version table, and 53 the whole schema at version 16. A clean run after each gave the never-killed schema, `sqlite_master` compared byte for byte.
- **`jaw.lock` against the engine's end** (`lock.py`): as under I1. The control, an orderly stop once serving (5 runs): the lock came free at 19 to 44 ms, after the engine had exited at 6 to 34 ms.
- **A second signal** (`double.py`): 10 fresh homes, `--hold-start`, SIGTERM once the engine had opened `session.db`, and a second 2 ms later. 8 reached `run`'s select: exit 1, the engine gone first, 8 to 625 ms after the first signal (the late ones are this Mac's scheduling at a load of about 70). In 2, tokio took the two signals as one, and jawd stopped in order with exit 0. Both databases were whole in all 10.
- **The Go watch:** `TestTheEngineExitsOnItsOwnSoonAfterJawdIsKilled` passed 5 of 5 at the head (`-race`, load about 70), the engine gone 8 to 79 ms after jawd's SIGKILL. With W off: "still running 5.0s after jawd was killed" with its standard input held open.
- **The bound** (`bound.py`): as above, 2,773 to 2,820 ms at the head over 6 runs, and 3,010 to 3,108 ms with S13 off over 4.
- **`bench link`:** with `--commit`, as above. Acked as they arrived, p50 28 to 45 µs at a load of about 40, and 109 to 116 µs at about 70. It logs its engine's in-order end before its folder goes, 2 of 2; with S14 off it logs nothing, 2 of 2. Its scratch folder was empty after.
- **Switches** (`logs/switches.txt`: each rule switched off alone in the clone, the tests that should catch it run, and the file checked out again):
  - **Fail their tests, as they should:** S1 (the reproduction and the failing start), S2, S3, S4 and S4serve, S5 (its unit and stopping tests), S6, S7, S8, S9, S10, S11, S12, O6 (the handshake's `told` branch) and W (Go). S13 and S14 fail their measurements by hand (above). G, by hand: with S1 on and the guard off, the reproduction's fake (`sleep 7244.622440`) was left running under launchd, and was killed by its PID; with the guard on, nothing was left.
  - **Fail no test:** O1 (T1), S4run (T2), O7 (T3), O8 (T4), and O3 (Go; O3 above).
- **The suite:** as above.
- **The trial merge:** as under Landing.

## Landing, once a round approves

- **The trial merge** onto `b4d9624d`, the remote's main at 15:52 (`git ls-remote`), has no conflict.
  - `main.rs` combines cleanly. Main's voice start comes after the supervisor's spawn and its `supervised.set`. It's one more start step that can fail with the engine running, which `Ending` ends like the others, and which I1's fix covers like the others. Main's `voice.stop()` opens `stop`.
  - On the merge, `cargo build --workspace` passes. So do stopping's 9 tests; GoEngine's 8, the supervisor's 7 and the 31 voice tests in jawd's unit binary; main's own 11; sync's 76 (1 ignored); `go vet ./...`; and `go test -race -count=1 ./...`.
- **Trailers:** `6fe180e2` takes one (`crates/jawd/src/engine/`). `d0e00176` takes one once `engine/cmd/jaw-engine/` is listed (O2). So do the fix round's commits that touch listed paths, and I1's. `df1e5d7e` and `601085c4` touch only `tests/stopping.rs` and `main.rs`, which aren't listed.
- **No migration.** The branch adds no SQL, schema, index, trigger, PRAGMA, file format or link protocol, and touches nothing under `store/`, `sync/`, `engine/internal/` or `proto/`.

## Verdict

Changes needed. I1: `jaw.lock` held until the engine has gone and the runtime has shut down, with its test. T1 to T4: each rule tested, failing with that rule off. O1 to O4 stand as follow-ups.

Verdict: changes needed

# Verdict of round two (`39461d70`)

Approved.

- Round one's I1 is fixed. `jaw.lock` is held until the engine has gone and the runtime has shut down, however jawd stops. That was measured in 46 stops, and tests that fail without it pin it.
- `session.lock` keeps two engines off one `session.db`, at restarts and crashes alike, and can't keep jaw from starting once its holder has gone.
- T1 to T4 each have a test that fails without their rule.
- On no kind of stop is anything lost or left inconsistent.
- O5 to O12, and round one's O3, are follow-ups; none blocks. O5 and O7 are the ones a lander meets in the gate, so their tests belong on the flakes list until they're done.

Land as under round two's Landing: rebased rather than merged, with `run()`'s one hunk keeping both sides (the lock first), the trailer on the nine commits, and this note committed in the branch.

Verdict: approved
