# Audit: jaw-voice's steadied timing tests (voice-flakes)

Range: `bab95fc2..b02458b7` in `voice-flakes` (worktree `~/src/jaw-voice-flakes`), 3 November 2026. Auditor: database-expert. `bab95fc2` is `origin/main`, and was still main on GitHub at 11:45. Line numbers are at `b02458b7`.

The range is three commits, all in `crates/jaw-voice/src/tests.rs` (36 lines in, 6 out), and nothing is uncommitted but this note:
- `9025426e`, the playhead test, `a_message_plays_to_its_end_reporting_the_playhead_ten_times_a_second`. It now plays three times. Each play is timed from its first sound, as the fake speaker heard it, and held to one report a tick of its whole time. How long the play took and how many reports came are judged at the best of the three.
- `6969ffbe`, the recording-end test, `a_recordings_end_is_held_for_a_caller_that_falls_behind_and_never_dropped`. It reads nothing until the audio thread has taken every command (`taken`, tests.rs:343-351), rather than after a fixed 300 ms.
- `b02458b7`, its sibling, `refusals_held_while_the_caller_is_behind_never_push_out_a_recordings_end`, given the same wait.

What I did, in one clone at `b02458b7` with one build folder, built with `CARGO_INCREMENTAL=0`, everything under `nice -n 10`, and through jaw-voice's fake devices only. Nothing played, no microphone opened, nothing was sent to WhatsApp, no jawd or engine was started, and nothing of the owner's was opened.
- I read the range against `bab95fc2`, and the library code each test guards: `lib.rs`, `play.rs`, `fake.rs` and `record/`. I also read crossbeam-channel 0.5.17's bounded channel, which `taken` reads.
- I built jaw-voice's library without its tests at both revisions and compared the two.
- I applied the builder's switches (`voice-flakes-fix-switches.diff`, which applied cleanly) and ten of my own, chosen at run time by environment variables. I ran every case 3 times alone, against the steadied test and against main's copy of it beside it (the builder's `main_…`).
- I ran the head's jaw-voice tests 130 times whole (alone, four at a time and eight at a time) and each steadied test 20 times four at a time. Every run's load (the 1-minute average, on 20 cores) was recorded.

The clone, its build folder and the logs were in `/tmp/jaw-audits/voice-flakes`, deleted once this note was written. My report, with the log of each step, is `voice-flakes-audit.md` in this auditor's store (`~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/reports/`). Beside it, `voice-flakes-audit-files/` holds my switches (`switches.diff`, the builder's and mine, against `b02458b7`), the scripts, and a line for every run (`results.tsv`).

## In short

Approved. The range changes test code only. Built without its tests, jaw-voice's library is bit-identical at main and at the head, so nothing it writes, syncs, holds or tells its caller changes.
- **A recording's end and its refusal:** every switch of either rule, the builder's 4 and my 5, fails the recording-end test 3 of 3. Every one of those 9 that the sibling guards fails it 3 of 3 too. The four that drop a held end also fail it 3 of 3 with the sync 1 s slower, where main's sibling passed each of them 3 of 3: with slow syncs, main's sibling didn't test its rule.
- **The waits:** `taken` returns only once the audio thread has taken every command. So every command but the last has been handled while the caller is still behind, however long the syncs take. If the audio thread dies, `taken` fails in 20 s rather than returning. The 20 s in `until` now only stops a hang.
- **The playhead:** each of these fails the steadied test 3 of 3, as it fails main's: the playhead reported five or twenty times a second, twice a tick, or only for the first second; the end not reported; the end reported half a second early or 0.7 s late. With the play's start held up 0.8 s, or the speaker's thread starting 0.8 s late, the steadied test passes 3 of 3 and main's fails 3 of 3.
- **Steadiness:** none of the three, and no other jaw-voice test, failed in the head's 130 whole runs at a load of 31 to 68, alone, four at a time and eight at a time. Side by side, main's recording-end test failed 11 of 20 whole runs four at a time, at 64 to 71.

One test gap stands as a follow-up (T1). At the band's lower edge, eight reports a second, the floor judged at the best of three plays is looser than main's floor on every play. Main caught that rate only because of the library's skipped ticks. Those skipped ticks, the builder's finding, are a display defect and not a data one (O1). Their fix is in `lib.rs`, so it needs an audit of its own, but this landing doesn't wait for it.

## Findings

### Data loss

None. Nothing outside `tests.rs` changed (see below). The rules these tests guard are main's code: a recording's end is never dropped for a caller that falls behind, and a refusal never pushes one out (the 16 October voice audit's F2 and F3). Both tests now catch every way I could find of breaking them.

### Inconsistency

None, for the same reason.

### Test gaps

**T1, a follow-up. The playhead test's floor, judged at the best of three plays, passes eight reports a second half the time. Main's floor, on every play, failed that rate** (tests.rs:50-53).
- **Measured:** with ticks every 125 ms, the steadied test passed 2 of 3 runs alone, at a load of 64 to 66, and 6 of 12 four at a time, at 65. The best play in each run brought 15 or 16 reports. Main's copy failed 3 of 3 alone and 11 of 12 four at a time ("15 reports in 2.00 s").
- **Why main caught it:** 125 ms ticks give 17 reports when every tick runs. Main's floor of 16 caught the rate only because the library loses one or two reports near a message's end (O1). At eleven a second (91 ms) it's the other way round: the steadied test failed 2 of 3 ("22 reports in 2.00 s") and main's passed 3 of 3. The steadied ceiling, one report a tick of the play's time, is tighter than main's 24.
- **The same trade elsewhere:** judging at the best of three also misses an end reported late in only one or two of the three plays, as the 30 October flakes audit's O1 found of its quickest-of-three judgements. Every other check holds on every play: the end is reported, at 2.0 s; 2 s is heard; the first event is the play's; at least 1.95 s passes; and there is at most one report a tick.
- **Why it can wait:** the cadence of the playhead is the display's, and no data depends on it.
- **The fix:** do it with O1's fix. Once every due tick runs (20 reports in each of 9 plays with the builder's probe), raise the floor to 19 at the best of three, so eight a second (17 reports) fails outright. Raising it to 17 would work now too: without a switch the head's plays brought 18 to 20, and eight a second's best play was 16. But that changes the tree, so the gate would run again.

### Unpredictable performance

None: nothing outside the tests changed. jaw-voice's suite takes 6.0 s rather than about 2 s, since the playhead test plays three times.

### Other

**O1, display only; its fix waits for an audit of its own. The library can skip a playhead tick near a message's end** (lib.rs:327-357).
- **What happens:** once a message is decoded, the speaker's callback wakes the audio thread every 10 ms (play.rs:257-258), since each refill clears `low` (play.rs:279). When a tick falls due with a wake waiting, `select!` picks one of the two at random. If it picks the wake, the deadline moves on (`Some(t) => Some((t + TICK).max(now))`, lib.rs:355) without the tick having run. So the reports due at 1.8 and 1.9 s of a 2 s message are often lost: plays here brought 15 to 19 reports where 20 are due.
- **Confirmed:** with the builder's probe, which runs a due tick whichever wake `select!` chose, all 9 plays brought 20 reports, at a load of 65 to 67.
- **Not a data concern:**
  - A tick carries only progress: the playhead's report, and the recording's level and its silence warning. The design already drops these while the caller is behind, since the next tick says the same, newer. A tick also runs the microphone's fault check (record/mod.rs:136).
  - Everything else runs on every pass of the loop, whichever arm woke it:
    - the end of a play and the speaker's fault (`refill`, lib.rs:342 and play.rs:118-135);
    - the recording's cap and write errors (`drain`, lib.rs:345, record/mod.rs:118-128 and 201-205);
    - held playback states, refusals and ends (`release`, lib.rs:348).
  - A skipped tick can delay a failed microphone's end by one tick (100 ms), and only while a message plays during a recording. That needs a `play` sent while recording, since `record` stops playback first (lib.rs:381). Either way, what came before the failure is kept (record/mod.rs:215-220).
- **The fix:** it's in `lib.rs`, which is on `check-data-audit.py`'s list, so it gets a short audit of its own: the probe above as its switch, and T1's floor raised as its test. This landing doesn't wait for it.

**O2, a note for whoever changes the audio thread's loop. `taken` holds only while the thread handles each command before it takes the next** (tests.rs:343-351).
- **True today:** the thread is the command channel's only receiver, and each pass of `select!` takes one command and handles it before the next (lib.rs:327-330). The helper's comment says it relies on that ("so all but the last are handled").
- **If that changed:** if `run` ever took commands in batches, `taken` would return before they're handled, and the sibling could again pass without its end being held.
- No change is needed now.

## What was asked, checked

### Nothing but tests

- `git diff --stat bab95fc2 b02458b7` names `crates/jaw-voice/src/tests.rs` alone. That file is compiled only as `#[cfg(test)] mod tests;` (lib.rs:20-21), and at this base no other crate depends on jaw-voice.
- What it adds is test code:
  - `TICK` is imported (a `pub const`);
  - `a_message_played_to_its_end` is new;
  - `taken` reads the private `Voice::commands` from the crate's own child module, so no visibility changed.
- Built without its tests, `libjaw_voice.rlib` is bit-identical at `bab95fc2` and at `b02458b7` (sha256 `c4dc7f9550d3831c81038a0b7bee85aea7e44a0f6816d94887c00fde3cf0ff60`), and neither build's dep-info names `tests.rs`. So everything below is exactly main's, as the 16 October audit approved it:
  - the `.part`, its `F_FULLFSYNC`, the hard link and the folder's sync;
  - the held events;
  - `Recorded`, sent once the file is synced and in place.

### Each test still catches its rule

Each case ran 3 times alone, under `nice`, against the steadied test and main's copy beside it, from one build with the switches chosen at run time. The load range is the 1-minute average over the case's runs.

| Test | Switch or stand-in | Steadied | Main's | Load |
|---|---|---|---|---|
| playhead | none | passes 3 | passes 3 | 47 to 54 |
| playhead | five a second | fails 3 | fails 3 | 57 |
| playhead | twenty a second | fails 3 | fails 3 | 57 to 59 |
| playhead | end not reported | fails 3 | fails 3 | 59 to 66 |
| playhead | end before the last half second | fails 3 | fails 3 | 63 to 64 |
| playhead | no reports past 1 s (mine) | fails 3 | fails 3 | 62 to 64 |
| playhead | two reports a tick (mine) | fails 3 | fails 3 | 61 to 62 |
| playhead | end 0.7 s late (mine) | fails 3 | fails 3 | 61 to 63 |
| playhead | eleven a second (mine) | fails 2 | fails 0 | 61 to 64 |
| playhead | eight a second (mine) | fails 1 | fails 3 | 64 to 66 |
| playhead | start 0.8 s late | passes 3 | fails 3 | 64 to 67 |
| playhead | speaker's thread 0.8 s late | passes 3 | fails 3 | 63 to 66 |
| recording end | none | passes 3 | passes 3 | 64 |
| recording end | end dropped when the caller is behind | fails 3 | fails 3 | 65 to 68 |
| recording end | no refusal while an end is held | fails 3 | fails 3 | 63 to 73 |
| recording end | refusal released before its end | fails 3 | fails 3 | 66 to 67 |
| recording end | a refusal pushes the end out | fails 3 | fails 3 | 63 to 66 |
| recording end | end dropped when offered again (mine) | fails 3 | fails 3 | 47 to 62 |
| recording end | a playback state pushes the end out (mine) | fails 3 | fails 3 | 38 to 68 |
| recording end | refused but started anyway (mine) | fails 3 | fails 3 | 63 to 64 |
| recording end | end sent as progress (mine) | fails 3 | fails 3 | 63 to 67 |
| recording end | refusals among the ends, one kept: round two's F3 (mine) | fails 3 | fails 3 | 60 to 64 |
| recording end | sync 1 s slower | passes 3 | fails 3 | 66 to 70 |
| recording end | sync 1 s slower, with each of 4 end-dropping switches | fails 3 each | fails 3 each | 21 to 61 |
| sibling | none | passes 3 | passes 3 | 64 to 65 |
| sibling | end dropped when the caller is behind | fails 3 | fails 3 | 65 to 69 |
| sibling | a refusal pushes the end out | fails 3 | fails 2 | 62 to 68 |
| sibling | no refusal while an end is held | fails 3 | fails 3 | 65 to 67 |
| sibling | end dropped when offered again (mine) | fails 3 | fails 3 | 37 to 44 |
| sibling | refused but started anyway (mine) | fails 3 | fails 2 | 63 |
| sibling | end sent as progress (mine) | fails 3 | fails 3 | 62 to 69 |
| sibling | refusals among the ends (mine) | fails 3 | fails 2 | 58 to 66 |
| sibling | sync 1 s slower | passes 3 | passes 3 | 62 to 63 |
| sibling | sync 1 s slower, with each of 4 end-dropping switches | fails 3 each | passes 3 each | 21 to 39 |

The 4 end-dropping switches are: the end dropped when the caller is behind, dropped when offered again, pushed out by a refusal, and refusals kept among the ends. I read the dumps of what each test saw, and each failure was for its switch's own reason:
- the end was missing, or the refusal was;
- the second recording's levels came;
- the test said "the second recording didn't start", or "the refusal comes after the end".

The sibling checks neither the order of a refusal and its end nor plays after the end. So a refusal released first, and a playback state pushing the end out, pass both copies of it. The recording-end test fails both 3 of 3.

Main's recording-end test fails with the sync 1 s slower even without a switch. So its failures in the rows with the slow sync show nothing; the steadied test's do.

**The playhead's ceiling under load.** The ceiling allows one report a tick of the play's whole time, so it grows with the start, during which no tick can run. Twenty a second still failed the steadied test 3 of 3 with every play's start held up 1.0 s ("37 to 40 reports in 3.0 s") and 1.6 s ("38 reports in 3.61 s"). One play of the three did pass at 1.6 s, with 36 reports, so every play would need a start of about 2 s for that rate to pass. Four at a time at a load of 65, twenty a second failed 12 of 12.

### The waits can't hang or pass vacuously

- **`taken` is bounded:** it polls `Sender::is_empty` every 5 ms for at most `PATIENCE`, 20 s, and then fails with "the audio thread never took its commands".
- **It returns only once every command is taken:** in crossbeam-channel 0.5.17, the version the lock pins, a receive claims its slot by moving `head` before it reads, and `is_empty` is `tail & !mark == head`. So `is_empty` is true only once the thread has claimed every command sent. Dropping the receiver only sets the mark, leaving the messages in place, so a dead audio thread fails `taken` in 20 s rather than letting it return.
- **In the recording-end test:** the audio thread is the only receiver and handles each command before it takes the next. So when `taken` returns, every command but the last has been handled, and the test has read nothing yet. That's all 306 plays, the stop and the second `record`; the last command is the `play("last")`. So the stop's `Recorded` met a full channel and was held, and the `record` was refused while it was held, however long the syncs took.
- **In the sibling:** the last command is the third `record`. It's handled before the loop's next `release` (lib.rs:348) can let the end go, so it too is refused while the end is held.
- **The `until` bound:** its 20 s (tests.rs:17-28) now only stops a hang. Once `taken` returns, the end, the refusal and the last play's failure have all been made, or are about to be, and the loop offers held events on every pass, at least once a tick.
- **The playhead test** waits only in `until(Ended)`, at most 20 s a play. `first_sound().expect(...)` fails any play that was never heard.

### The builder's finding: skipped ticks

See O1. It's a display defect, not a data one, and its fix gets an audit of its own.

### The suite

From the head's own test binary, under `nice`, 36 passed and 2 were ignored in every run (the 2 ignored are the child processes of the kill and full-disk tests):

| Runs | At a time | Load | Passed |
|---|---|---|---|
| 10 whole | alone | 35 to 39 | 10 |
| 40 whole | four | 31 to 35 | 40 |
| 40 whole | four | 58 to 61 | 40 |
| 40 whole | eight | 59 to 68 | 40 |
| 20 of the playhead test | four | 66 to 68 | 20 |
| 20 of the recording-end test | four | 67 | 20 |
| 20 of the sibling | four | 68 | 20 |

No jaw-voice test failed in these runs, so none needed judging against the landers' flakes list. Side by side, in whole runs of the switch build with no switch set, four at a time for 5 rounds at 64 to 71, main's recording-end test failed 11 of 20. The steadied three, and every other test, passed all 20.

The builder's gate stands as it reported it, and meets the landers' rule:
- `make check-generated`, `make lint` and `make build` passed.
- `make test` ended at exit 2, with four of jawd's tests failing: `feed::tests::a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`, the lane test `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`, media in's starts-check test, and the socket's connection-full test.
  - Each is on the list, and each passed 3 of 3 alone.
  - The targets cargo didn't reach passed on their own.
  - jaw-voice's 36 passed inside it.
- jawd doesn't depend on jaw-voice at this base, so none of those four can be touched by this range.

## Landing

- **The trailer:** all three commits touch `crates/jaw-voice/src/tests.rs`, which is on `check-data-audit.py`'s list, so each takes `Data-Audit: docs/audits/2026-11-03-voice-flakes.md`, for example with `git rebase origin/main -x 'git commit --amend --no-edit --trailer "Data-Audit: docs/audits/2026-11-03-voice-flakes.md"'`. Then commit this note on top. It touches only `docs/audits/`, so it needs no trailer. The script wants the note at HEAD, with its last verdict line reading approved.
- **No migration:** no SQL, schema, PRAGMA, `jaw.db`, `session.db` or data file changes, and jaw-voice keeps no database.
- **What to run again:** the trailers change only the commit messages. Once the note is committed, the tree differs from the gated tree, `b02458b7`'s, only by this note.
  - If main is still `bab95fc2`, or has moved only under `mac/` and `docs/` (rebase onto it), run `make check-generated` and `make check-audit` again, then push from `~/src/jaw-voice-flakes`. Of the gate's steps, only `check-audit` reads `docs/` or `mac/`.
  - If main has moved anywhere else, rebase and run the whole gate again, judging failures against the landers' flakes list.
- **The flakes list:** once this is on main, take jaw-voice's recording-end and playhead tests off it, and watch them for a week, as the flakes data half's were, so that a failure is looked into rather than run again.

## Verdict

T1 and O1 stand as follow-ups, to be done together in `lib.rs`'s own audited change, and O2 is a note.

Verdict: approved
