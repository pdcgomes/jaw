# Audit: voice wired into jawd (M5), round one

Range: `ea0f4134..f8832659` in `voice-wiring` (worktree `~/src/jaw-voice-wiring`), 2 November 2026: 9 commits on media out, with main at `0dd286c9` (communities, migration 28). Auditor: database-expert. Two auditors did this round: the first stopped at 14:20 when Cursor's window reloaded, and the second took over from its report, logs and scratch clone, reused its results, and ran what was left.

Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `f8832659`. Every run used a clone in `/tmp/jaw-audits/voice-wiring` with `CARGO_INCREMENTAL=0`, the demo engine, and jaw-voice's fake speaker and microphone. No owner data, no real microphone or speaker, and nothing sent to WhatsApp.

## In short

Changes needed: no test of the branch's fails without three of the rules it touches. Those are `voice.send`'s synced commit before it answers (T1), the backfill telling each row it changes (T2), and `voice.send`'s bound on kept bytes (T3). The audit's own tests cover all three and can be taken as they are. Nothing is lost and nothing is left inconsistent.

- **`voice.send` follows `send.media` step for step.** It checks the chat, keeps jaw's copy of the recording, writes one synced commit holding the message, its outbox row and the copy to make, takes the copy back on every refusal, and removes the recording's own file only after the commit.
  - Probed: it answers after a synced commit.
  - A SIGKILL right after the answer, then a start: the recordings folder is empty, and the voice note goes once (PTT, Ogg Opus, its 64 points). Its key answers with it, and jaw's copy goes.
- **The backfill is a schema step leaving one `backfills` row.** It reads 64 messages a step, through the guarded `read_again` that places and events share.
  - It's idempotent: taken again, it changes nothing and tells nothing.
  - Stopped after each of its 29 committed steps and started again, it ends as one run does.
  - At a million messages, one in 20 voice, it takes 3.03 ms p99 a transaction beside live writes, against main's polls backfill's 3.21 ms in the same load window. Live messages stay within their 5 ms.
  - At one in five it overshoots the 3 ms cap under load, though live messages still keep theirs (P1, a follow-up).
- **The draft is kept, rightly** (`typed: false`; S20 fails a test).
- **A recording whose chat's merge finishes before it's sent is refused** (O1). Nothing is left behind, but the person is still there: during the merge the same send goes to the kept chat. A small fix.
- **Played receipts' plan matches disappearing's design** except that it names no durable played mark, which is the once-only guard (O2).
- **35 switches:** 24 fail a test of the branch's, and 11 don't. Of those 11, S7, S15 and S19 fail one of the audit's tests.
- **The full suite:** 1,529 passed, 7 failed, 23 ignored. Each of the seven passed alone 3 of 3, and none is the range's doing.

## What was asked, checked

### The backfill

- **Its migration** (`store/schema.rs:1482-1489`): `INSERT INTO backfills (name, after, upto) SELECT 'voices', 0, seq FROM sqlite_sequence WHERE name = 'messages' AND seq > 0`. That's one row covering the messages held at the migration, so newer ones are the converter's, and nothing on a jaw.db without messages.
- **Its steps** (`store/write/links.rs:45-47`, `:90`): `NEXT_VOICES` takes the next 64 messages by `n` within `after..=upto` (a rowid range search). Each voice message still there goes through `kinds::guarded` with `CONVERTING`, so a panic is kept in quarantine and the rest go on (S5).
- **What it writes** (`store/write/again.rs:90-103`, shared with places and events):
  - The row is read again from its raw protobuf as the converter reads it now.
  - It's written at a new version with its event only when its text, spans, summary or detail change (S6). The voices test checks a text message, a voice message deleted for everyone, and one whose raw reads as another kind all left alone.
  - The converter's `voiced` (`convert/kinds.rs`) gives `voice` its seconds when above 0. It gives the waveform only when it has 64 points, each clamped to 100 (S1, S2, S3).
- **Idempotent and resumable:**
  - Taken again from the start, it reads nothing twice and tells nothing again (the branch's test).
  - The audit's `audit_the_voices_backfill_stopped_after_every_committed_step_ends_as_one_run_does` closed the store after each of 29 committed steps and opened it again. The backfill ended as one run did: 297 voice messages read again, the deleted, the edited and the unreadable left alone.
  - The cursor is the shared `backfill()`'s, which copies' kill test also covers.
- **Its cost** is under Measurements: 0.8 to 1.5 s at the owner's shape, and 7.9 to 40 s at a million messages depending on the share of voice messages and the load.

### The migration

- **Kind and class:** a schema step, as its comment and lock line declare, costing the same however many messages there are. `backfills` is jaw's own, already in `kept.rs`. There's no new table or column, so `kept.rs`, the seeds and the fingerprint are unchanged.
- **The lock line:** `28 schema ca276a3d…`. The lock hashes the migration's text alone (`store/schema.rs` `sha256`), and the "28" is in a Rust comment outside it.
- **Plans:** every new statement is pinned in `store/plans.txt`, and none scans a large table or sorts in a temporary b-tree.
  - The migration's `SCAN sqlite_sequence` is that one-row-per-table table.
  - `NEXT_VOICES` searches by rowid range.
  - Media's kind and seconds of a message, and the answer's chat of a message, are primary-key searches.
  - The copies to do read `iif(o.kind = 'voice', 'voice', g.kind)`, with the plan it had.
  - The plan test passed at the head. It fails on the audit's patched build with no switch on, because a switch rewrites a statement, so that failure is not a finding.
- **Renumbering at landing:** main has communities as 28, and chat actions lands as 29. So this step becomes 30, or the next free number then:
  - move it after the others in `MIGRATIONS`, its SQL byte for byte;
  - make its comment say 30;
  - make its lock line `30 schema ca276a3d…`, with the same hash.
  - Nothing else names it by number: the tests find it by `SELECT 'voices'`.
  - A jaw.db made by this unlanded branch's jawd isn't upgradable, as for any unlanded branch. The owner's is made by main's.

### `voice.send` on the data layer's terms

- **The outbox row is in a synced commit before the answer** (`send/voice.rs:89-92`, `durably`).
  - The audit's `audit_voice_send_answers_after_a_synced_commit` counts the store's synced commits around it: 1, then 2.
  - No test of the branch's fails with the commit made without waiting for its sync (S19): T1.
- **Idempotent with its key, through retries, restarts and a kill at any commit it adds:**
  - With a key that named a message within a day, `voice.send` answers with that message (`by_key`, `send/voice.rs:39-45`). That holds once the recording is gone too (`voice/mod.rs:615-627`), and when the outbox's own key check finds an earlier message: the new recording is left stopped (S22).
  - A kill after the Attach commit leaves the message to the outbox, its copy to media, and its recording to the start's emptying. Probed after a SIGKILL right after the answer: it went once and its key answered with it.
  - A kill before that commit leaves jaw's copy for the start's sweep of files no row names (`media/outgoing.rs:354`), the recording to the start, and nothing sent. That's by design: a recording lasts while jawd runs.
  - The copy's later commits are media out's, approved in its audits.
- **Media out's rules, `typed` included:**
  - The copy is kept through `media.keep` with the kept bytes read in the same query as the chat (`send/voice.rs:59-72`), and taken back on every refusal (S24; S25 and S26 are T8).
  - Its copy's row is written as audio (`goes_as`, S30), and the copies to do read it back as voice (S29). So it's probed with jaw-voice rather than ffprobe and goes as it is (S32), uploaded with audio keys (S31).
  - The chat's timer goes in its `ContextInfo` (S28).
  - **The draft:** `typed: false`, so a voice note leaves the chat's draft (S20 fails a test). It should. The draft is what was typed in the box, and the voice note isn't it: media out's D1 showed that clearing a draft the message doesn't carry loses words that are neither sent nor kept.
- **A recording cancelled, or whose chat is gone or merged, leaves nothing behind:**
  - **Cancelled:** the file and its `.part` go (S13). A connection's end throws its recording away (S14); jawd's stop does too (S15, T5), and the start empties the folder in any case (S12).
  - **A recording being sent is past throwing away** (`voice/mod.rs:771-776`): a cancel, a connection's end or jawd's stop leaves it to finish, and its file goes once the commit is made (S21). So the copy is never read from a file being removed.
  - **Merged:** while the merge is under way, `CHAT_TO_SEND` (`send/outbox.rs:38-39`) follows `chat_merges` to the kept chat, and it went there (probed). Once the merge has finished, the gone chat's row is deleted, and `voice.send` is refused with 3. Nothing is left: 0 messages, outbox rows, copies to make or kept files, and the recording stays stopped; a cancel leaves nothing (probed). That's O1, and the branch has no test of it (T9).
  - A chat's row is deleted nowhere else (`store/write.rs:1899` is the merge's fold), so gone means merged.
- **Every file it makes is bounded and counted:**
  - **The recordings folder** (`voice/folder.rs`) holds one recording, and its `.part` while it records. It's mode 0700, capped at 15 minutes (`jaw_voice::RECORDING_LIMIT`), and refused under 64 MB free (S16).
  - The start empties it only after `jaw.lock` is held (`main.rs`'s `start` is handed the lock before `Voice::start`), so a second jawd can't empty the first's.
  - `jawd status` says its files and bytes (S33), what the start removed, and whether hard links work. Probed: a stopped recording was counted as 1 file and 3,122 bytes, and jawd's stop removed it.
  - **jaw's copy** is one of media's kept files, under the 8 GB bound. Probed: past that bound `voice.send` is refused with 5 (limit media), the recording kept and nothing kept to send. No test of the branch's covers it: T3.
  - **The backfill's** remaining count is `stored.voices_backfill` (T6).

### Playing and recording write nothing they shouldn't

- **Playing** writes only what media in's cache rules write for a file someone wants:
  - It refuses what isn't a voice message (S9).
  - A voice message not in the cache is wanted at interactive priority in a synced commit (`media/mod.rs:438-441`).
  - Its file is marked used as it plays (`:456-458`).
  - Both are untested (S10, S11): T7.
- **Recording** writes nothing to jaw.db. WhatsApp is told you're recording through typing's queue.
- Its temporary file is jaw-voice's `.part` beside the recording's name, put in place by a hard link once synced. It is bounded and removed as above.

### The plan's tests not written

The builder's report lists four. Two block landing, as T1 and T3; the audit's tests can be taken for them. The other two can follow:

- the answer after the synced commit (T1);
- the kept bytes past their bound (T3);
- a recording's chat gone at send: with O1's fix, once its behaviour is decided (T9);
- a recording with nothing in it: refused with 4, as the builder's report has it. That's outside the data layer (T9).

There's also one the plan lists that the report doesn't: the backfill stopped and started again midway (T9). It doesn't block: the audit's test passes, and the shared cursor is tested by copies' kill test.

### Played receipts' design

The plan's task 6 matches disappearing's (`~/src/jaw-disappearing` at `d9acfb02`, read only) in three ways:

- **The outbox row:** kind `played`, one per target (`send/once.rs`'s `PLAYED_ROW`), its raw the engine's `MarkRead` with `type` `played`.
- **Settling:** settled by its `Done`, never "may have gone" (`send/outbox.rs:437`).
- **Your phone's receipt:** the drain already takes `played-self`, and `played` from you (`sync/drain.rs:148`), as a view-once message opened elsewhere.

What it doesn't say is where "it was played" is kept. That mark is the once-only guard: view-once gets it from its `opened` table, whose insert is `ON CONFLICT (message) DO NOTHING`. The `played` row is written only when that insert inserts (`store/write/expiry.rs:217-224`). The follow-up will be small if the plan names four things (O2):

- that mark, its migration's kind and its table's class;
- the drain's `played-self` marking voice messages too;
- the receipt written in the same commit as the mark;
- `send/actions.rs`'s text for a `played` row ("telling its sender you opened their view-once message") made to fit a voice message.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gap

The first three block landing; the audit's tests can be taken as they are. They're in `voice/tests/audit.rs` and `store/write/again/tests/audit.rs` of the audit's clone, each module added with a `mod audit;` line. They use only the head's own rig and helpers. Copies are kept in the coordinator's store beside the audit's report, under `reports/voice-wiring-audit-probes/`.

- **T1 (blocks). No test of the branch's checks that `voice.send` answers only after its synced commit** (`send/voice.rs:89-92`).
  - With the commit applied without waiting for its sync (S19), every voice test passes, and they're the only tests that send a recording.
  - The builder's report says `send.media`'s test covers `durably()`. But this call is `voice.send`'s own, and switching it fails nothing.
  - **Test:** `audit_voice_send_answers_after_a_synced_commit`. It holds the copy's making, counts `storage().synced_commits` around `voice.send`, and expects one more by the answer. It passes at the head and fails with S19.
- **T2 (blocks). No test checks that the backfill tells each row it changes** (`store/write/again.rs:99`).
  - With the message's event left out (S7), 279 of the store's tests passed, the events backfill's 34 among them. The plan test failed as it does on the patched build with no switch on, 10 are ignored, and the run was stopped with only the migration-fault test still going.
  - The voices test checks the versions and that a second pass tells nothing new, but not that the first pass tells anything.
  - The plan's "done when" asks for "a version and an event each". `read_again` is shared with places and events, so this gap is main's too.
  - **Test:** `audit_each_voice_message_the_backfill_reads_again_is_told`, or its two lines in the branch's test: count the events around the backfill, and expect one for each voice message read again. At the head it's 29 events for 29; with S7 it's 0.
- **T3 (blocks). No test of the branch's checks `voice.send` against the kept bytes' bound.** The plan lists it, and the builder's report says it isn't written.
  - **Test:** `audit_kept_bytes_past_their_bound_refuse_it_and_keep_the_recording`. A failed send's copy of 8,000 MB is kept, then `voice.send` is refused with 5 and `limit: media`. The recording stays stopped, and no message or kept file is left.
  - The bound is checked twice: before the copy is kept, and again inside the commit (`store/write/outbox.rs:929`). Either refuses it, so the test stands for the rule as a whole. S25 covers the second check's clean-up (T8).
- **T4. A connection's end reaching the voice task is untested where it can be lost.** That's through the socket (`sock.rs:806-811`, S36), and while the task's requests are full (`voice/mod.rs:194-200`, S18, which is what `f8832659` fixed).
  - The branch's test calls `ended` directly (S14).
  - Lost, a recording goes on until its 15-minute cap, then stays stopped until jawd stops or starts again. WhatsApp is told you're recording meanwhile, and other recordings are refused (4).
  - **Tests:** a connection that recorded, closed through the socket; and one ended while the task's requests are held full.
- **T5. jawd's stop throwing the recording away** (`voice/mod.rs:328-333`, S15) fails no test of the branch's. The start's emptying, whose switch S12 fails a test, removes what it would leave. The audit's `audit_jawd_stopping_throws_a_stopped_recording_away_and_status_counts_it_meanwhile` fails with S15.
- **T6. `stored.voices_backfill`** (`state.rs:296`, S8) is never checked while the backfill has work left.
  - The branch's test reads `backfill_left` itself ("as jawd status says"), and `main.rs`'s test checks it's 0 at the end.
  - Main's places and events counts are the same.
- **T7. Media in's rules for what plays are untested for `voice.play`.** These are the want made durable before it waits for the file (`media/mod.rs:441`, S10) and the file marked used (`:456-458`, S11).
  - Without the first, a crash loses the want, so the file isn't fetched at the next start. Nothing plays it then, so that's minor.
  - Without the second, a voice message played often goes from the cache before one never played.
- **T8. jaw's copy kept after two rare refusals** stays until the next start's sweep. One is the refusal inside the commit, past the kept bytes' bound (`send/voice.rs:103-106`, S25). The other is a key that an earlier message took between `by_key` and the commit (`:119-122`, S26).
  - No test reaches either: the audit's kept-bytes test is refused before the copy is kept.
  - `send.media`'s refusal inside the commit is untested on main as well.
- **T9. Three of the plan's tests aren't written:**
  - a recording whose chat is gone at send (with O1);
  - a recording with nothing in it (4);
  - the backfill stopped and started again midway (the audit's test passes, and the cursor is the shared one copies' kill test covers).

### Unpredictable performance

- **P1. At one voice message in five, a 64-message step can overshoot the background lane's 3 ms cap under load.**
  - Beside live writes it reached 5.37 and 5.71 ms p99 at load about 70; without them, 12.76 ms at the owner's shape.
  - Live messages beside it stayed within budget, at 3.98 to 4.51 ms p99. At one in 20 it's 3.03 ms p99, in line with main's polls backfill's 3.21 ms in the same window.
  - Each step reads every voice message among its 64 again, where places' steps rarely find one.
  - **A follow-up:** end a step once it has read some number again (16, say), as copies has its own step.
  - This doesn't break a budget: the live one held, and a transaction no one waits behind holds nobody up.

### Other

- **O1. A recording whose chat's merge finishes before it's sent is refused with 3, "jawd has no chat …; chat.start finds or makes one".** The person is still there, under the kept chat (`send/voice.rs:59-67`).
  - The outcome hangs on whether the background fold has run: during the merge, the same send goes to the kept chat.
  - Nothing is lost or left behind. But the recording can only be cancelled: `voice.send` takes no chat, and its chat is fixed when recording starts.
  - With the check skipped (S35), the writer put the voice note in the phone number's chat, as it places any message under a LID it can fold. So the fix is small: resolve the recording's chat through the merge at send (`person_jids` to the kept chat), reading the timer there. Test it before and after the fold.
  - The owner's call whether before landing. It's small enough to do now.
- **O2. Played receipts' plan names no durable played mark** (above). It should, so the follow-up stays as small as the plan expects.

## Switches

One runtime switch per rule (`JAW_AUDIT_SWITCH`), each run against the modules whose tests cover it with the audit's tests skipped. S7 was also run against all of the store's tests, and S7, S15, S19, S25, S26 and S35 against the audit's tests. There are no S17 or S27.

| Switch | Turns off | Fails |
|---|---|---|
| S0 | migration 28 | 2 of the voices backfill's tests |
| S1 | the converter's `voice` | 6, convert, backfill and voice |
| S2, S3 | the waveform's 64 points, each at most 100 | the converter's test |
| S4 | the step choosing voice messages | 2 backfill tests |
| S5 | the step's guard | the quarantine test |
| S6 | leaving a row that's right alone | 3, voices' and places' |
| S7 | the row's event | none of the store's 279 that ran; the audit's T2 test |
| S8 | status's `voices_backfill` | none (T6) |
| S9 | refusing what isn't a voice message | the refusals test |
| S10, S11 | the want made durable; the file marked used | none (T7) |
| S12 | the start emptying the folder | its test |
| S13 | a thrown-away stopped recording's file | 3 |
| S14 | a connection's end throwing its recording away | its test |
| S15 | jawd's stop throwing it away | none; the audit's stop test (T5) |
| S16 | the 64 MB free check | the refusals test |
| S18 | a connection's end while the task's requests are full | none (T4) |
| S19 | `voice.send`'s synced commit | none; the audit's T1 test |
| S20 | leaving the draft | the send test |
| S21 | the recording's file going once sent | 2 |
| S22 | a key's earlier message answering | the send test |
| S23 | refusing `at` and `repeat` | its test |
| S24 | the full outbox's refusal taking the copy back | its test |
| S25 | the commit's kept-bytes refusal taking the copy back | none, the audit's included (T8) |
| S26 | a key raced to an earlier message taking the copy back | none, the audit's included (T8) |
| S28 | the chat's timer | its test |
| S29, S30 | the copies to do reading voice; the copy's row as audio | 2 and 4 |
| S31 | audio upload keys | the send test |
| S32 | probing the voice copy with jaw-voice, not ffprobe | 2 |
| S33 | status's folder counts | the recording test |
| S34 | a failed send leaving the recording stopped | the full-outbox test |
| S35 | the chat check | none; the audit's merge test passes too, as the writer places it (O1) |
| S36 | the socket's end reaching the voice task | none of the socket's or voice's 49 (T4) |

## Measurements

The backfill was measured on the release build by the audit's `audit_measure_the_voices_backfill`.

- **The made-up jaw.db:**
  - The owner's shape is 84,465 messages (the owner's size, as `2026-10-26-library.md` has it) in 450 chats; the large one is 1,000,000 messages in 2,000 chats.
  - Every 20th or every 5th message is a voice message with a waveform. Their rows are as an earlier jawd wrote them, without `voice`.
- **The run:**
  - The migration runs, then the backfill runs as `sync::backfill` sends it, 64 steps to an apply. Optionally, live messages are written beside it.
  - The writer's per-transaction times are its own, commit included.
  - Everything ran under `nice` on a machine shared with other agents, at load 55 to 75.

| jaw.db | Voice | Live | Backfilled | Transactions | p50 / p99 | Live p50 / p99 | WAL most |
|---|---|---|---|---|---|---|---|
| 84,465 | 1 in 20 | no | 0.78 s | 83 | 2.50 / 5.23 ms | | 14.6 MB |
| 84,465 | 1 in 5 | no | 1.50 s | 182 | 3.75 / 12.76 ms | | 27.0 MB |
| 84,465 | 1 in 5 | yes | 0.90 s | 199 | 2.65 / 5.71 ms | 1.69 / 4.51 ms | 33.6 MB |
| 1,000,000 | 1 in 20 | no | 7.92 s | 937 | 2.66 / 4.84 ms | | 50.8 MB |
| 1,000,000 | 1 in 20 | yes | 40.26 s | 1,052 | 0.26 / 3.03 ms | 0.68 / 3.44 ms | 50.8 MB |
| 1,000,000 | 1 in 5 | no | 18.70 s | 2,027 | 3.72 / 6.12 ms | | 50.8 MB |
| 1,000,000 | 1 in 5 | yes | 21.05 s | 2,156 | 1.29 / 5.37 ms | 0.90 / 3.98 ms | 50.9 MB |

- **The baseline:** main's `measure_the_polls_backfill_and_what_votes_cost` at 1,000,000 messages, in the same window as the second million-message row (load 70 to 75). It took 254 transactions at 0.11 ms p50 and 3.21 ms p99.
- **The migration** took 84 to 854 ms with the store's opening, whatever the size: it's one insert.
- **jaw.db** grew by 1.8 to 5.6 MB at the owner's shape, and by 29 to 68 MB at a million, from the new detail and its events.
- **Events:** without live writes, each run wrote one event for each voice message read again, for example 4,223 for 4,223. With live writes, each live message adds two, its own and its chat's: 60,520 for 50,000 voice messages and 5,260 live ones.
- **An apply** took 10 to 39 ms p50 and 92 ms at most. That's the background lane's request, not a transaction.
- **Stopped after every step:** `audit_the_voices_backfill_stopped_after_every_committed_step_ends_as_one_run_does` works on 300 voice messages, one deleted for everyone, one edited and one unreadable.
  - For each of the 29 committed steps, it closes the store after that step, opens it again and finishes the backfill.
  - Every row and each row's events ended as in an uninterrupted run.
  - A committed step is all a kill can leave, so this stands for a kill after each commit.
- **`voice.send`'s synced commit:** the store's synced commits went from 1 to 2 around it, with the copy's making held.

## The full suite

- **The run:** `cargo test --workspace --no-fail-fast` at the head, debug, under `nice`, at load 40 to 80. 1,529 passed, 7 failed, 23 ignored.
- **The Go engine's tests pass.** The range touches neither `engine/` nor `proto/`.
- **The seven each passed alone 3 of 3** from the head's binaries:
  - feed's `a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends` and media's `a_save_waiting_for_its_file_holds_up_nothing_else_on_its_connection`, both on the landers' list;
  - feed's `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`, media's `the_starts_check_reads_a_page_again_that_it_couldnt_and_shows_nothing_of_it_meanwhile`, the socket's `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push` and the store's `the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end`, each failing under load in earlier audits;
  - `tests/stopping.rs`'s `jawd_whose_engine_never_answers_stops_on_sigterm_and_takes_its_engine_with_it`, a new sighting.
- **The new sighting isn't the range's.** It failed on "the engine outlived jawd": its engine's `sleep` was still found a second after jawd had exited within its bound.
  - The range adds `voice.stop()` to jawd's stop, before the engine is stopped. It waits only for the voice task to take its turn, and nothing there waits on the audio thread.
  - So it can't keep the engine alive after jawd's exit.
- None of jaw-voice's recording-end or playhead tests failed.

## Verdict

Before landing:

- **T1:** a test that `voice.send` answers only after its synced commit, failing with that commit not waited for.
- **T2:** a test that the backfill tells each row it changes, failing with the event left out.
- **T3:** a test that `voice.send` is refused past the kept bytes' bound, the recording kept and nothing kept to send.

The audit's tests for these can be taken as they are. O1's fix is the owner's call, small enough to do now, with its test (T9). T4 to T9, P1 and O2 can stand as follow-ups.

Verdict: changes needed

# Round two: `f8832659..cf406cc5`, round one's findings, and the landing on main

Range: `ea0f4134..cf406cc5` in `voice-wiring` (worktree `~/src/jaw-voice-wiring`): round one's 9 commits and the fix round's 10 (`9611d57d` to `cf406cc5`, one for each item). Audited 2 and 3 November 2026, with main at `9e1b8188`. Auditor: database-expert. Two auditors did this round. The first stopped at 21:30 on 2 November, when Cursor's extension host crashed. The second took over on 3 November from its report, logs, scratch clone and draft, checked and reused its results, and ran what was left: the rebase onto `9e1b8188` with the change's tests, its switches and the full suite there, D1 as it now is on disappearing's branch, and the follow-ups below.

Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `cf406cc5`. Every run used one clone in `/tmp/jaw-audits/voice-wiring-r2` with `CARGO_INCREMENTAL=0`, the demo engine, and jaw-voice's fake speaker and microphone. No owner data, no real microphone or speaker, and nothing sent to WhatsApp.

## In short

Approved. Nothing is lost or left inconsistent. Every rule round one found untested and blocking (T1 to T3), and T4 to T6 and T9 with them, now has a test that fails with its rule switched off, at the head and rebased onto today's main. T7, T8 and T10 stand as test-gap follow-ups, P1 as a follow-up in the writer, and O3 to O7 as others. None of them can lose data or leave jaw.db inconsistent.

- **T1 to T6 and T9 are fixed as round one meant them.** Each new test passes at the head and fails with its rule switched off, and nothing else does: S19 (T1), S7 (T2), F1 (T3), S36 and S18 (T4), S15 and this round's G2 (T5), S8 (T6), and F2, F3 and F4 (T9). The same holds rebased onto today's main, for every switch. F5 fails the stop test only with the other backfill tests, as the fix round said.
- **O1 is fixed.** `voice.send` finds the recording's chat as the writer finds a message's. A probe records in her LID's chat, sends during the merge and after its fold, each with a key, and SIGKILLs jawd right after the second answer. After a start, each voice note goes once, to her number, with the kept chat's timer, both keys answer with their messages, and nothing is left behind. F2 and F3 each fail it.
- **The timer it takes agrees with disappearing's D1,** which has landed on disappearing's branch since the brief. D1 reads the timer a message went with from its raw protobuf, `AudioMessage` included, and `voice.send` puts the kept chat's timer there. Nothing in `voice.send` has to change when the two meet; the migration's number does (O5).
- **P1: the step of 16 is right, and what still passes 3 ms is the writer's rule.** With steps of 64, almost no transaction reached the 16 steps the writer needs before it allows for a commit, so 89% passed 3 ms. With steps of 16, 85 to 96% do, and their work stays under 2.92 ms at p99. What's left is how the writer aims: at 3 ms with an average commit, so commits' tails make the p99. With only the writer's allowance changed, the same steps put 1.7% past 3 ms at a million messages, where the head puts 38%. Live writes keep their 5 ms beside it as well as beside a backfill that reads nothing. It can stand, as a follow-up in the writer.
- **O2: the plan's `played` mark stops a second receipt across a restart, a replay and your phone's own receipt.** But it misses a merge's fold, which deletes a duplicate copy and would take its mark with it (O3), and it marks a play still waiting for its file (O4).
- **Nothing new.** The fix round's three changes to the product (the chat found, the step's size, the test-only hold) add no commit, acknowledgement or file, and lose nothing. The hold is compiled into jawd's unit tests only.
- **The full suite at the head** failed only eight tests on the landers' list, each passing alone 3 of 3, and the plan test, which the first auditor broke by editing the clone's sources while it ran. jaw-voice's recording-end test passed there; alone, under load, it fails about one run in four, from the same binary main builds: the branch changes nothing of jaw-voice's.
- **Landing on main:** four files conflict, and each resolves by keeping both sides. The step takes 30 (31 if disappearing lands first), with its hash. Rebased onto `9e1b8188`, every switch fails what it failed at the head, and the full suite fails only five tests on the landers' list, each passing alone 3 of 3. Main's media out tests pass there. 15 of the 19 commits take the trailer.

## What was asked, checked

### T1 to T6 and T9, each with its rule switched off

Each switch is one runtime flag (`JAW_AUDIT_SWITCH`) in one patched build. The fix round's are round one's S0 to S36 and its own F1 to F6 (`voice-wiring-fix-r1-switches.diff`); G2, G4 and G6 are this round's. Each was run against the voice and backfill tests, the probe among them, at the head (on its base, `ea0f4134`) and again on the branch rebased onto `9e1b8188`. With none on, 40 passed in both.

| Item | Switch | Turns off | Fails, at the head and rebased |
|---|---|---|---|
| T1 | S19 | `voice.send`'s synced commit | `voice_send_answers_after_a_synced_commit` alone |
| T2 | S7 | the row's event in `read_again` | the voices test alone |
| T3 | F1 | the kept bytes' bound, before the copy is kept and in the commit | `kept_bytes_past_their_bound_refuse_it_and_keep_the_recording` alone |
| T4 | S36 | the socket's end reaching the voice task | `a_connection_that_recorded_and_closes_its_socket_throws_its_recording_away` alone |
| T4 | S18 | an end that finds the requests full | `a_connection_ending_while_the_voice_tasks_requests_are_full_still_throws_its_recording_away` alone |
| T5 | S15 | jawd's stop throwing the recording away | `jawd_stopping_throws_a_stopped_recording_away_and_status_counts_it_meanwhile` alone |
| T5 | G2 (this round's) | jawd's stop throwing away a stopped recording, not one going | the same test alone |
| T6 | S8 | status's `voices_backfill` | the voices test alone |
| T9, O1 | F2 | the chat found by the JID it was recorded in | the O1 test and the probe |
| T9, O1 | F3 | the timer read in the chat it was recorded in | the O1 test and the probe |
| T9 | F4 | an empty recording's waiting `voice.send` answered 3 | `a_recording_with_nothing_in_it_is_refused_and_leaves_nothing` alone |
| T9 | F5 | a step's cursor saved where it looked last | the stop test, with six other backfill tests (and in one run at the head a seventh, below) |
| P1 | F6 | steps of 64 | `a_step_of_the_voices_backfill_looks_at_16_messages_so_it_reads_16_again_at_most` alone |
| | G4 (this round's) | the row naming the chat recorded in, its place and timer the chat found | none of O1's: the writer puts the row in the kept chat either way. At the head one playback test failed beside it, a race of its own (O7) |
| | G6 (this round's) | jawd removing a failed recording's file | none: jaw-voice's `PartFile` removes its part on a failed finish and on drop (`jaw-voice/src/record/file.rs:72-74`, `:94`), so jawd's removal is a second guard |

- **T1** holds the copy's making and counts the store's synced commits around `voice.send`, so nothing else syncs in between.
- **T3:** F1 has to switch off both checks, before the copy is kept and in the commit (`store/write/outbox.rs:928`), since either refuses it. The commit's own refusal taking the copy back is still T8.
- **T4's socket test** also closes a connection that's still recording: it's cancelled there and then, its part removed, and WhatsApp told you've stopped.
- **T4's requests-full test** fills the voice task's requests behind its test-only hold. The fix round replaced a lock on jaw.db with that hold, since the lock also timed out the writer's inbox write.
- **T9's stop test** fails only with F5, which every backfill test that ends a step early catches. A cursor that lags is hidden by idempotence, so no switch fails the stop test alone; the fix round's follow-up stands.
- **F5's seventh,** in the first auditor's run at the head, was a places test, `a_location_the_converter_has_written_again_since_and_a_window_on_one_read_again`. It catches F5 only when a step happens to be cut short by its time bound. The voices step test and the 64-locations step test cut a step short every time, and caught F5 in every run.
- **Every switch, round one's others included, fails on the rebased tree exactly the tests it failed at `cf406cc5`** in the fix round's last run, the probe left out (45 switches, 3 November 07:04 to 07:34, load 47 to 77). S10, S11, S25, S26 and S35 fail none, as at the head: T7, T8, and O1's old check, which is no longer reached.

### O1: a recording sent after its chat's merge

- **The fix** (`send/voice.rs:58-73`): `send_voice` finds the chat with `read::chat_named` (`store/read.rs:1148`). That's the writer's own rule (`chat_jid`, `store/write.rs:1356`): the chat itself, the one it's merging into (`chat_merges`), or, once no chat has its JID, the same person's one-to-one chat under another of their JIDs (`person_jids`), never one merging away. Where `chat_jid` would make a chat, `chat_named` finds none, so the LID's chat is never made again. `voice.send` takes that chat's composing place, reads its timer and the kept bytes there (`CHAT_TO_SEND`), and writes the row there.
- **Gone still means merged.** Only the merge's fold deletes a chat's row, at the head (`store/write.rs:1899`) and on main (`:2007` there). So `voice.send` still refuses with 3 only when the recording's chat and its person are both gone, and the recording stays stopped.
- **The probe** (`audit_r2_a_recording_sent_after_its_chats_merge_goes_once_to_the_chat_kept_across_a_kill`, kept beside the report):
  - In a process it kills, with the copies held, a recording made in her LID's chat is sent with a key while the merge goes. Another is recorded there, and the fold runs to its end. The first key asked again answers with the first message and leaves the new recording stopped. That one is then sent with a second key, and the process is killed with SIGKILL.
  - Before the kill, each message is in her number's chat with one outbox row and one copy row.
  - After a start on the same folder: the recordings folder is empty. Each voice note goes once, to her number, a PTT with the kept chat's 86,400 s timer. Both keys answer with their messages, and nothing more goes. The LID's chat isn't made again. jaw's copies leave `media/outgoing` once made, and neither message has an outbox or copy row left.
  - It passes at the head and rebased, and fails with F2 or F3.
- **The writer places the row again at the commit,** by `chat_jid`, so a merge that moves between `voice.send`'s reads and its commit still puts the message in the kept chat. Only the timer is read before the commit, so a second merge in those milliseconds is the one way it could be stale.
- **A race, harmless:** a merge can begin and fold between `chat_named`'s read and `CHAT_TO_SEND`'s. Then the second finds nothing, and `voice.send` is refused with 3, the recording kept; sent again, it goes.

### O1's timer, and disappearing's D1

- **What it takes:** the kept chat's timer, read in the chat its row is written to.
  - On this base, the merge's start gives the kept chat the longer of the two rows' timers (`max(k.ephemeral, g.ephemeral)`, `store/write.rs:1714`). Disappearing makes it the newer, by `ephemeral_at`.
  - It goes in the `AudioMessage`'s own `contextInfo.expiration` (`send/compose.rs:127-139`), with no wrapper, and the upload keeps it: `compose::uploaded` fills only the file's location and keys. That's the timer your phone and the recipient's apply to the message, so it's the one WhatsApp would apply, as far as jaw knows the chat's.
  - The O1 test's expectations hold under both merge rules.
- **D1 has landed on disappearing's branch since the brief** (`~/src/jaw-disappearing` at `6bc83d63`, read only: `9803ef42`, then `cdb6f34f`). When the server has a message of yours, `outbox::received` takes the timer it went with from the outbox row's raw protobuf (`convert::timer`), counted from then, and none if it went without one.
  - `convert::timer` reads `context_of`, which takes `audio_message`'s `contextInfo` among every kind's. So it finds a voice note's timer where `voice.send` puts it.
  - So `voice.send` agrees with D1, and nothing in it has to change when the two meet.
- **What does have to change at landing** is the migration's number: disappearing has its own migration 30, so whichever lands second takes the next (O5).
- **The landing audit** should run D1's two cases with a voice note too: one recorded in a chat with no timer, the timer set to a day before the server's answer, gets no time; one recorded with a day's timer, turned off before the answer, gets a day. D1's own tests send text only.

### P1: the step of 16, and the 3 ms

The design caps a background transaction at 3 ms, commit included. The writer meets that by estimate (`store/mod.rs:1091-1140`): it ends a background transaction once its work has had 3 ms, or, once it holds 16 rows (`PACE_ROWS`), once its work plus its commit, as commits have lately cost for each row, reaches 3 ms (`Pace::enough`). It learns that cost only from transactions of 16 rows or more (`Pace::learn`). A backfill's step is one row to the writer.

- **Steps of 64 (`f8832659`):** at one voice message in five, a step reads about 13 again, so a transaction ends on its work alone after about 8 steps. Only 1% reached 16, so the pace never learned a commit to allow for, and 89% passed 3 ms with theirs (p99 11.6 ms at the owner's shape).
- **Steps of 16 (the head):** 85 to 96% of transactions hold 16 steps or more, so the writer allows for their commit. Their work takes 2.69 to 2.92 ms at p99, and the median, commit included, is 2.91 to 2.98 ms: where the writer aims.
- **What passes 3 ms is the writer's aim.** It ends a transaction when its work and an average commit reach 3 ms, so the third or more whose commit costs more than average end just past it, and commits' own tails (1.3 to 3.9 ms at p99) make the p99: 3.6 to 6.2 ms at the owner's shape, 5.2 ms at a million.
- **Changing only the writer's allowance shows it.** Allowing for the commit from the first step, with the same steps, put 1.7% past 3 ms at a million messages (p99 3.51 ms), against the head's 38% (5.17 ms). It took twice the transactions, since shorter ones don't teach the pace, so the estimate it learned early is kept. At the owner's shape it changed nothing (3.64 and 3.81 ms).
- **Smaller steps don't help much.** Steps of 8 took 0.3 to 2.6 ms off the owner's shape's p99 (3.64 ms) for a quarter more transactions, and with the allowance from the first step they made the p99 18 ms (more, smaller transactions, and slower commits).
- **Live writes keep their budget beside it.** At a million messages, in one window at load 67 to 78, live writes' p99 was 4.85 ms beside the voices backfill. Beside a backfill that reads nothing again (no voice messages) it was 3.43 ms before and 5.37 ms after. The fix round measured 2.74 to 3.15 ms at load 30 to 36.
- **So it can stand.** The step is right, and the overshoot is the writer's for any background work whose commits vary this much. This one's are larger than most: at a million messages it writes 200,000 new versions and as many events, where the polls backfill rewrites 2,000 rows. The follow-up is in the writer: aim below 3 ms by the commit's spread (allowing for a commit at its p90 rather than its average, say), or learn the commit from shorter transactions too, measured across every background kind with `make perf`'s background row.
- **Every P1 number** was taken at load 32 to 82 on 20 cores. A quieter set wasn't possible: the load went from 8 to 37 and then to 71 within minutes on 3 November.

### O2: the played mark

The plan's task 6 (`docs/plans/2026-11-02-voice-wiring-plan.md:100-114`) now names the mark: a table of jaw's own, `played`, its insert `ON CONFLICT (message) DO NOTHING`, and the `played` outbox row written in the same synced commit only when that insert inserts.

- **Across a restart:** the mark and its receipt's row go in the synced commit `voice.play` answers after.
  - A crash before that commit leaves neither, and one after it leaves both.
  - The row is resumed and settled by its `Done`, never "may have gone". So a receipt WhatsApp had before the crash can go again, which is harmless for a receipt.
- **A replay, or a second interface's play:** the insert inserts nothing, so no row is written.
- **Your phone's own receipt:** disappearing's drain makes `played-self`, and `played` from you, an `Opened` amendment. The plan takes that as the mark, with no row, and one that comes before its message waits in `pending`. A play here and on your phone at once can each send one, as two of your devices can.
- **What it misses: a merge's fold (O3).** The fold (`store/write.rs` on main, its duplicate-copy fold ending at `:2312`) moves a duplicate copy's votes, answers, media and outbox rows to the copy kept, then deletes the duplicate. `played`'s `ON DELETE CASCADE` would take the duplicate's mark with it. Then the kept copy says it isn't played, and the next play sends a second receipt. Disappearing carries view-once's `opened` across the same fold (`OPENED_FOLDED`).
- **And when it marks (O4).** J7 (`docs/audits/2026-10-16-voice.md:94`) sends the receipt off jawd's accepted `voice.play` rather than the audio thread's `Playing` event. But a play of a message still waiting for its file is accepted too. If that download then fails, its sender is told it was played, though nothing played it.

### Nothing new

- **The fix round's product changes are three:** `send_voice`'s chat (O1), `VOICES_STEP` (P1, `store/write/again.rs:37`, `store/write/links.rs:90`), and the test-only hold. The rest is tests and docs.
  - None adds a commit, an acknowledgement or a file.
  - `voice.send` still answers after its synced commit (`durably`, T1's test), and still takes the copy back on each refusal it did before.
- **The step's cursor** is the shared `backfill()`'s (`store/write/links.rs:99-121`). It saves the last message a step took or looked at, for any step size: a step that reads none again saves its last look, and one cut short saves where it stopped. F5 tests it.
- **The hold** is `#[cfg(test)]` in `Inner`, in `Task`, in their constructors, in the task's loop and in `Voice::hold` (`voice/mod.rs:102`, `:132`, `:143`, `:173`, `:181`, `:336`, `:343`). jawd's integration tests build jawd without it.
  - In the head's build, `nm` finds `Voice::hold` once in jawd's test binary and not in `target/debug/jawd`, with `Voice::call` six times in each.
  - So nothing outside jawd's unit tests can hold the voice task.

### The full suite

- **At the head** (`cargo test --workspace --no-fail-fast`, debug, `nice`, 2 November 19:47 to 20:41 at load 47 to 81), every target passed but jawd's unit tests: 1,536 passed, 9 failed and 24 ignored in all, of which jawd's unit tests were 1,022, 9 and 18. Go's tests (`go test -race ./...`) passed.
- **Eight of the nine are on the landers' list** as the head has them, and each passed alone 3 of 3 from the head's binary at load 38 to 46:
  - the feed's half-closed connection and long-request lane tests;
  - media's save-waiting test;
  - the socket's connection-full test and the saved layout's lane test;
  - the WAL test with history pouring in, in the form the head's base has;
  - sync's engine-disconnect test;
  - jawd's stop bound with messages pouring in.
- **The ninth, the plan test,** was the first auditor's doing. It edited the clone's sources while the suite ran, and the plan test reads `src/` at runtime (`store/plans.rs`). Alone at the clean head it passed 3 of 3. No other jawd test reads `src/` at runtime.
- **jaw-voice's recording-end and playhead tests passed** in the head's suite (36 passed, 2 ignored).
  - Alone on 3 November at load 62 to 66, the recording-end test failed 3 of 13, each "waited in vain" past its 20 s wait for the events behind a channel it fills on purpose. The playhead test passed 5 of 5.
  - Nothing in the branch can be why. It changes no file of jaw-voice's. It adds jaw-voice to jawd's dependencies with no features, and jaw-voice has none. jawd and jaw-voice were already built together in a workspace run, so jaw-voice's test binary is the one main builds.
  - That's main's to steady, as the fix round's follow-up says.
- **Rebased onto `9e1b8188`** (the same command, 3 November 07:37 to 08:33 at load 48 to 80): 1,703 passed, 5 failed and 26 ignored in all, of which jawd's unit tests were 1,180, 4 and 20.
  - **The five are on the landers' list,** and each passed alone 3 of 3 at load 74 to 80: the feed's half-closed connection and long-request lane tests, media in's starts-check test, the socket's connection-full test, and `jawd_whose_engine_never_answers_stops_on_sigterm_and_takes_its_engine_with_it`.
  - That last one left its fake engine's `sleep` running ("the engine outlived jawd"), as the landers' notes say it does. I stopped it by its PID. Alone, it left none.
  - **What the rebase could have broken passed.** That's the plan test (`plans.txt` merged), the protocol's fixtures test (the resolved arm) and the store's migration tests: the lock, a fault at any statement of any migration, every earlier version keeping its rows, and `kept.rs`'s check that no migration drops jaw's own data. The upgrade from every fixture (`tests/upgrading.rs`) and jaw-voice's 36 passed too.
  - **Main's media out tests pass:** every test in `media::outgoing`, `send` and `store::write`, the kept bytes, the outbox and drafts among them.
  - **The WAL test with history pouring in** passed. The flaky tests' data half took it off the list, and it's watched in its steadied form, which the rebased tree has.

### Landing on today's main

Main is `9e1b8188`. Since the branch's base `ea0f4134` it has gained communities (migration 28), chat actions (29), the Mac library, the terminal's `until:` and the flaky tests' data half. In the clone, the branch was rebased onto `b33dcba1` on 2 November, then onto `9e1b8188` on 3 November.

- **Four files conflict, in two commits, as the brief lists; nothing else does** (`git merge-tree` finds the same four against both). Each resolves by keeping both sides:
  - `store/schema.rs` (in `de7c4177`): the voices step goes after main's 29 (`app_state_awaiting`'s index), its SQL byte for byte, its comment saying 30.
  - `store/migrations.lock`: main's `28 schema ddc73e7f…` and `29 schema 18e69a4d…`, then `30 schema ca276a3d…`, the hash unchanged. The lock hashes the step's text alone, and the number is in a Rust comment outside it.
  - `jaw-proto/tests/fixtures.rs` (in `ab305686`): the arm for methods with an empty result holds main's `chat.pin` to `star` and the branch's `voice.play` and `voice.record`.
  - `protocol/README.md`: the status row holds main's `quarantined` and `awaiting` and the branch's `voices_backfill` and `voice`, two separate insertions.
- **The only thing more than keeping both sides is the number,** 28 to 30, in the step's comment and its lock line. Nothing else names the step by number: its tests find it by `SELECT 'voices'`.
- **Five more files change on both sides and merge cleanly:** `jaw-proto/src/lib.rs`, `send/mod.rs`, `state.rs`, `store/plans.txt` and `store/write/outbox.rs`. The one that needed reading is `send/mod.rs`'s `enqueue`, now main's `Mutex<i64>`, which holds the time of the last chat action and is read and set only by chat actions (`send/chat_actions.rs:440-442`). `voice.send` takes only its guard, as every message send does, `send.media` among them. The plan test and the protocol's fixtures test check the other two that could go wrong, `plans.txt` and the protocol's types, in the rebased suite.
- **The result:** 17 commits identical to the branch's and two differing only by those resolutions (`git range-diff`). The change is byte for byte the same on `9e1b8188` as on `b33dcba1` (the diffs from each base, without index lines and line numbers, compare equal).
- **Main since `b33dcba1`** (the flaky tests' data half) shares one file with the branch, `store/write/outbox.rs`, and merges cleanly there.
- **On the rebased tree:** every switch fails what it failed at the head (above), and the full suite is under "The full suite".
- **The trailer:** the rebased tree's `scripts/check-data-audit.py`, which the branch extends with `crates/jawd/src/voice`, names 15 of the 19 commits as touching the data layer, the same 15 as at the head: `de7c4177`, `b6823877`, `a151a096`, `1929ef1a`, `9d7c321c`, `f8832659`, `9611d57d`, `a0720ab8`, `c80317c3`, `89009553`, `504c8b06`, `de6a4e10`, `9a083c2b`, `9c10e917` and `cf406cc5`. The other four are the plan (`d9580c0e`), the Info.plist build (`d8175c36`), the protocol (`ab305686`) and the played receipts' plan (`6e404e82`).
- **If disappearing lands first** (O5), voices takes 31 with the same three edits, and the landing audit checks D1's two cases with a voice note.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gap

None blocks; each can stand as a follow-up, as round one and the brief have them.

- **T7 and T8 (round one's) stand.** S10, S11, S25 and S26 still fail no test, at the head or rebased. The rules are in the code: the want made durable, the file marked used, and the copy taken back on the two rare refusals. Only their tests are missing, and neither rule's loss could lose data or leave jaw.db inconsistent. A want lost in a crash leaves a file unfetched after the start, when nothing is waiting to play it. A copy not taken back would wait for the next start's sweep.
- **T10. The stop-after-every-step test catches no rule alone.** F5 fails it with six other backfill tests, and a cursor that lags would be hidden by idempotence. Main's places and events backfill tests still check the versions they move, not that each row is told (T2's shared gap). Both are the fix round's follow-ups.

### Unpredictable performance

- **P1 stands as a follow-up, in the writer** (above). The voices backfill's transactions pass the background's 3 ms at p99 (3.6 to 6.2 ms) by commits' tails, because the writer aims at 3 ms with an average commit. Live writes keep their 5 ms beside it as well as beside a backfill that reads nothing. Fix in the writer: allow for a commit at its spread, or learn the commit from shorter transactions too, and measure every background kind with `make perf`.

### Other

- **O3. The played receipts' plan misses a merge's fold** (`docs/plans/2026-11-02-voice-wiring-plan.md:106-107`). The fold deletes a duplicate copy after moving what hangs off it, and `played`'s `ON DELETE CASCADE` would take its mark too, so the next play of the kept copy sends a second receipt and the row says it isn't played. Fix in the plan: carry the mark across the fold as disappearing carries `opened` (`INSERT OR IGNORE INTO played (message, at) SELECT ?1, at FROM played WHERE message = ?2` before the duplicate goes), with a test of a play, a merge that folds a duplicate, and a second play sending nothing.
- **O4. The plan marks a play still waiting for its file** (`:102`). If that file's download then fails, its sender is told it was played. Fix in the plan, the owner's call against J7: mark when jawd starts the play, its file in, still jawd's own decision rather than the thread's event, and not at the acceptance of one that waits.
- **O5. Voice and disappearing both take migration 30.** Whichever lands second moves its step after the other's, its SQL byte for byte, its comment and lock line taking the next number with the same hash. The landing audit runs D1's two cases with a voice note.
- **O6. During a merge, the other sends and `voice.send` disagree on the chat's place and timer.** The other sends are named by the JID the interface gives, so a send to the chat merging away takes that chat's composing place and its own timer (`send/outbox.rs:216`, `CHAT_TO_SEND`'s `c.ephemeral`). `voice.send` now takes the kept chat's. So a text to her LID that waits for its link preview can be overtaken by a voice note recorded there, and the two can carry different timers when the two rows' differed. Nothing is lost or doubled. Fix (a follow-up): `voice.send` takes both places (`composing` sorts and dedups them), and the other sends read the timer where `chat_named` finds the chat, as `voice.send` does.
- **O7. A playback test races the status it reads** (`voice/tests.rs:411`). On `PlaybackFailed` the voice task clears what plays and pushes `failed`, and only after that turn of its loop leaves the status (`voice/mod.rs:871-879`, `:367`). So a status read right after the push can still say it's playing. It failed once beside G4, which doesn't touch playback, and passed 10 of 10 alone. Fix: the test waits for the status, or the task publishes before it pushes.

## Measurements

**P1** was measured by the fix round's `measure_the_voices_backfill` on a release build of the head, with three knobs of the audit's own, not for landing:

- `JAW_P1_STEP`, the voices step's size;
- `JAW_P1_GATE`, how many rows a transaction holds before the writer allows for its commit (16, `PACE_ROWS`, at the head; 1 allows for it from the first step);
- `JAW_P1_TRACE`, each background transaction's rows, work and commit, by the writer's clock.

The made-up jaw.db is round one's: the owner's shape is 84,465 messages in 450 chats, and the large one 1,000,000 in 2,000. Every fifth message (or every twentieth) is a voice message with a waveform, written as an earlier jawd wrote them. Each row below is one run, under `nice`, on 2 November between 20:48 and 21:28. Times are a transaction's work and commit together.

At the owner's shape, one voice message in five unless it says otherwise, without live writes, at load 32 to 58:

| Steps | Commit allowed for from | Transactions | p50 / p99 | Past 3 ms | Holding 16 steps or more |
|---|---|---|---|---|---|
| 16 (the head) | 16 steps (the head) | 266 | 2.91 / 6.20 ms | 36% | 91% |
| 16 | 16 steps | 252 | 2.95 / 3.96 ms | 40% | 85% |
| 16 | the first step | 251 | 2.96 / 3.64 ms | 43% | 93% |
| 16 | the first step | 251 | 2.98 / 3.81 ms | 46% | 96% |
| 8 | 16 steps | 331 | 2.72 / 3.64 ms | 26% | 100% |
| 8 | the first step | 831 | 1.08 / 18.18 ms | 11% | 2% |
| 64 (`f8832659`) | 16 steps | 167 | 3.74 / 11.60 ms | 89% | 1% |
| 64 | the first step | 167 | 3.76 / 15.78 ms | 93% | 1% |
| 16, one in 20 | 16 steps | 171 | 1.78 / 13.98 ms | 17% | 73% |
| 16, one in 20 | the first step | 171 | 1.85 / 8.93 ms | 23% | 71% |

- With steps of 16, the transactions holding 16 steps or more did their work in 2.37 to 2.47 ms at p50 and 2.69 to 2.83 ms at p99. Their commits took 0.51 to 0.55 ms at p50 and 1.27 to 3.94 ms at p99.
- At one in 20, the p99 is commits' (7 to 12 ms at p99, in 171 transactions).

At a million messages, one voice message in five unless it says otherwise, steps of 16, at load 63 to 82:

| Commit allowed for from | Live writes beside | Transactions | p50 / p99 | Past 3 ms | Holding 16 steps or more | Live writes' p99 |
|---|---|---|---|---|---|---|
| 16 steps (the head) | none | 3,062 | 2.94 / 5.17 ms | 38% | 88% | |
| the first step | none | 5,948 | 1.48 / 3.51 ms | 1.7% | 1% | |
| 16 steps | 9,506, beside the audit's own switch runs | 3,963 | 2.83 / 11.75 ms | 34% | 57% | 14.03 ms |
| the first step | 12,946, beside them too | 14,899 | 0.64 / 3.86 ms | 2.4% | 2% | 6.79 ms |
| 16 steps, no voice messages | 7,224 | 1,015 | 0.84 / 3.02 ms | 1.2% | 98% | 3.43 ms |
| 16 steps | 2,972 | 3,556 | 2.84 / 4.26 ms | 30% | 62% | 4.85 ms |
| 16 steps, no voice messages | 2,319 | 1,014 | 0.85 / 2.70 ms | 0.6% | 99% | 5.37 ms |

- The last three rows are one window at load 67 to 78, with nothing else of the audit's running: a backfill that reads nothing again, the head's, and the first again.
- At the head without live writes, the transactions holding 16 steps or more did their work in 2.50 ms at p50 and 2.92 ms at p99, and their commits took 0.44 ms at p50 and 2.72 ms at p99.
- The backfill took 25 to 90 s at a million, depending on the load and the live writes. It wrote one event for each voice message read again (200,000 for 200,000), and each live write adds its own and its chat's. The WAL stayed at its bound, 50.7 to 50.9 MB at most. The migration, one insert, took 114 to 1,869 ms with the store's opening, under load.
- No quieter set was possible on 3 November: the load went from 8 to 37 and then to 71 within minutes.

**The full suites** are `cargo test --workspace --no-fail-fast`, debug, under `nice`:

- at the head, 2 November 19:47 to 20:41, at load 47 to 81: 1,536 passed, 9 failed and 24 ignored, jawd's unit tests 1,022, 9 and 18 in 2,935 s; Go's `go test -race ./...` passed;
- rebased onto `9e1b8188`, 3 November 07:37 to 08:33, at load 48 to 80: 1,703 passed, 5 failed and 26 ignored, jawd's unit tests 1,180, 4 and 20 in 2,799 s.

**Alone**, each from a copied test binary, with its own timeout:

- the head's eight listed failures, 3 times each at load 38 to 46: 24 of 24 passed;
- the rebased suite's five, 3 times each at load 74 to 80: 15 of 15 passed, and the stopping test left no fake engine;
- jaw-voice's recording-end test, 13 times at load 62 to 66: 10 passed, and 3 failed, each "waited in vain" after its 20 s;
- jaw-voice's playhead test, 5 times at load 62: 5 passed;
- the playback test O7 names, 10 times at the head with no switch, on 2 November at about load 70: 10 passed.

**The switches,** at the head (2 November 21:00 to 21:15) and rebased (3 November 07:04 to 07:34), each against the modules whose tests cover it, with a timeout of 900 s each: none on, 40 passed, and each switch failed what the table and its line above say.

## Landing

- **Trailers:** the 15 commits named under "Landing on today's main" take `Data-Audit: docs/audits/2026-11-02-voice-wiring.md`, with this note committed in the branch. The script reads a note's last `Verdict:` line, which is round two's. The other four (the plan, the Info.plist build, the protocol and the played receipts' plan) touch none of its paths.
- **The migration** takes the next free number as it lands: 30 on today's main, 31 if disappearing lands first. Move the step after the last in `MIGRATIONS`, its SQL byte for byte. Its comment says the number, and its lock line is `<number> schema ca276a3d…`, the same hash.
- **The other three conflicts** keep both sides, as above.
- **A short landing audit** checks four things on the rebased head:
  - the step's comment and its lock line name the same number, the next free one;
  - the plan test, the lock test, the protocol's fixtures test and the upgrade from every fixture pass;
  - the README's status row holds both sides' fields;
  - if disappearing has landed, D1's two cases with a voice note (O5).
- **Follow-ups for the pending notes, none blocking:**
  - the test gaps T7, T8 and T10;
  - P1, in the writer;
  - O3 and O4, in task 6's plan, before it's built;
  - O6 and O7;
  - jaw-voice's recording-end test, steadied in main.
- **The flakes list:** nothing in this branch adds to it or takes from it. jaw-voice's recording-end test stays on it.

## Verdict

Verdict: approved
