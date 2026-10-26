# Audit: the socket's lane

# Round three: b6841e8, round two's T7 and H7

Range: `49d6a89..b6841e8` in `sock-flood` (worktree `~/src/jaw-sock`), 26 October 2026. Auditor: database-expert. Rounds two and one are kept below as written. Round two's verdict is the section before this round's, at the end, and this round's verdict is the note's last line. Line numbers are at `b6841e8`, and paths are under `crates/jawd/src/` unless they say otherwise.

Main hasn't moved from `4ca6774`, and `b6841e8` sits on `49d6a89`, so the branch's first four commits are as round two audited them. `b6841e8` adds:
- **T7:** round two's probes H and H2 as tests of the drain at a half-close (`feed/tests.rs:2850`, `:2885`). The second now confirms through `status` that its reply waits for room before the interface reads.
- **H7:**
  - a connection whose lane's task has died ends with an error, so its interface connects again (`sock.rs:239-245`);
  - a find and a long request each run on an inner task, through `on_own_task` (`:365-374`, called at `:362` and `:408`), so if what they run panics, the reply is -32603 and the count goes back;
  - two test-only requests that panic, `test.panic` down the lane (`:571-572`) and `test.panic.apart` on a task of its own (`:469`, `:617-618`), with a test each (`feed/tests.rs:2916`, `:2947`).
- **`jawd status` says "1 request"** (`jaw-client/src/lib.rs:215-218`).

There's still no SQL, schema, migration, PRAGMA, index, trigger or plan change. No answer changed: the lane, `answer`, `answer_apart` and the logout are as at `49d6a89` but for the test-only arms. What changed is how a connection ends after a panic, and that a find or a long request whose task panics is still answered.

## Summary

- **Nothing is lost and nothing is inconsistent.** Everything round two found holding still holds at the new head. I checked it with round two's probes, two new ones, 12 mutations, the two kill tests, the gone and half-close tests on the head's release build, and three SIGTERMs with connections open (Measurements).
- **T7 is closed.** Each of the drain's three conditions (`sock.rs:107`) has a test that fails without it, 3 of 3:
  - without waiting for finds and long requests, the builder's H test gets 1 of its 5 long requests' replies;
  - without waiting for a reply waiting for room, the builder's H2 test never sees its reply wait, and the H test loses a long request's reply too;
  - without waiting for the lane, round two's half-close test gets its find's reply and none of its ten drafts'.
- **H7 is done, and pinned for the lane and for long requests.**
  - With a dead lane ignored, as at `49d6a89`, the builder's lane test fails 3 of 3. It also fails 3 of 3 when a connection's counts aren't taken back as it ends.
  - With a panicking long request's task sending no reply, the builder's long-request test fails 3 of 3.
  - Round two's P1 and P2, which failed at `49d6a89`, pass 3 of 3. A gone interface whose lane died is forgotten 9 to 151 ms after it goes, and a half-closed one whose long request died gets both replies and is closed.
- **Two parts of H7 have no test in the branch (hygiene, H12 and H13).**
  - A find whose task panics: with the find back on a plain task, every test of the branch passes, the find, socket and feed suites included. My probe P5, with a probe-only find that panics, fails 3 of 3.
  - The replies a lane hands over before it dies: with the connection ending as soon as it sees the lane dead, the branch's tests pass. My probe P3 fails 3 of 3, getting 1 of 5. The code is right by construction, since `recv()` returns `None` only once every reply handed over has been taken.
- **H7's change can't end a connection whose lane is healthy, and it drops, duplicates and reorders no reply.**
  - The lane's channel closes only when its task has gone, which is once the connection has ended, when jawd stops, or when the task panics.
  - A stopping jawd's connections take their stopping arm first and end without an error. Three SIGTERMs with seven connections open logged no connection error. Among the seven were one with a send waiting on the writer, a half-closed one, and one with a reply waiting for room.
  - Every find and long request gets exactly one reply, and a dying lane's replies come once each, in order, before its connection ends.
- **The test-only requests can't be reached in a release build.** They're compiled only under `cfg(test)`. The head's release jawd holds none of their strings, where the test binary holds each. On a real `jawd demo`, each is error -32601, "jawd doesn't know test.panic", answered in turn, and the connection goes on.
- **The gate passes, apart from two timing tests outside the branch.** jaw's frame-rate bench and jawd's bound on how long a stop takes each failed once in the full suite at load 51 to 62, and passed 5 of 5 alone at 47 to 53. `check-audit` fails only for want of the trailers on the four data-layer commits.

## Round two's T7 and H7, each against its rule

The tests are the builder's, at their lines in `feed/tests.rs`. Each passes 3 of 3 as the branch is, at loads of 51 to 71, and fails without its rule as the last column says (Measurements, Mutations). Their names:
- "H" is `a_half_closed_connection_with_long_requests_and_replies_waiting_gets_every_reply_then_ends`;
- "H2" is `a_half_closed_connections_last_reply_waiting_for_room_still_comes_before_it_ends`;
- "half-close" is round two's `an_interface_that_closes_its_writing_side_still_gets_every_reply_and_then_the_connection_ends`;
- "lane dies" is `a_connection_whose_lane_dies_ends_and_nothing_of_it_is_counted`;
- "request dies" is `a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`.

| Item | Rule | The test | Without the rule |
| --- | --- | --- | --- |
| T7 | the drain waits for finds and long requests (`sock.rs:107`) | H (`:2850`) | fails 3 of 3: of the five long requests, only the first was answered; my H fails 3 of 3 |
| T7 | the drain waits for a reply waiting for room (`:107`) | H2 (`:2885`) | fails 3 of 3: `status` never saw the reply wait. H fails 3 of 3 too, a long request's reply lost; my H2 fails 3 of 3 |
| (a) H1 | the drain waits for the lane (`:107`) | half-close (`:2741`) | fails 3 of 3: the find was answered, none of the ten drafts; H2 fails 3 of 3 too |
| H7 | a connection whose lane's task has died ends, and nothing of it is counted (`:239-245`) | lane dies (`:2916`) | fails 3 of 3 with the dead lane ignored, and 3 of 3 with a connection's counts not taken back (`in_lanes` 2); my P1 and P3 fail 3 of 3 with the dead lane ignored |
| H7 | a long request whose task panics is answered with -32603 (`:365-374`) | request dies (`:2947`) | fails 3 of 3 with no reply from a panicking task: only the `status` was answered. My P2 and P5 fail 3 of 3, the connection kept open |
| H7 | a find whose task panics is answered the same way (`:408`) | none | with the find on a plain task, lane dies and request dies pass 3 of 3, and so do the find, socket and feed suites, 72 tests; my P5 fails 3 of 3 (H12) |
| H7 | the replies a lane handed over before it died all come, once and in order (`:239-245`) | none | with the connection ending as soon as it sees the lane dead, lane dies and my P1 pass 3 of 3; my P3 fails 3 of 3, getting 20 alone of 20 to 24 (H13) |
| round two | "1 requests" | | `jawd status` says "lanes 1 request, this one among them, …" |

With the end of the lines ending the connection, as at `c768143`, H, H2 and half-close each fail 3 of 3.

## Findings

### Data loss: none

- **Every acknowledgement still follows a synced commit.** `b6841e8` changes no answer. A lane reply still goes only once `answer` has returned, an apart reply once `answer_apart` has, and the logout's once `apply_durable` has forgotten the account (`sync/mod.rs:140-151`).
- **A dying lane loses no reply it has handed over** (`sock.rs:239-245`). `answers.recv()` returns `None` only once the lane's task has dropped its sender and every reply in the channel has been taken. So the connection sends them all, in order, and only then ends.
  - In P3, the queue to the interface was full when five drafts and a `test.panic` went down the lane. The first draft's reply waited for room, the other four were handed over, and the lane's task died. Once the interface read, it got 20 to 24, once each and in order, and the connection closed with nothing of it counted, 3 of 3.
  - A connection that ended as soon as it saw the lane dead would lose the four (P3 got 20 alone). Nothing in the branch tests it (H13).
- **A find or a long request gets exactly one reply** (`sock.rs:367-374`). `on_own_task` awaits the inner task and sends its reply, or -32603, "jawd failed while answering: …", if it panicked. Only a panic or a cancellation makes that an error, and nothing cancels the inner task. In P2 and P5 the replies were 5 and 6, each once, 3 of 3.
- **A dead lane ends its connection as a disconnect does.** The lane's requests after the one that panicked are never started, and the replies of finds and long requests still under way go to nobody.
  - README:21's promise to a half-closed interface can't hold for a request whose answering panicked.
  - An interface connects again and asks again, and a send asked again by key writes nothing twice. Nothing it was told is lost.
  - The README could say so (H14).
- **Kills and interfaces that go still leave a prefix of the lane done, each once** (release, Measurements).
  - (b)'s kill test: of 40 kills, 14 kept none, 9 the first send and 17 all three.
  - (a)'s: 6 kills kept 3, 2, 12, 0, 0 and 8, never fewer than the replies read.
  - An interface that went with 20 sends in its lane had 3 carried out in each of 10 rounds, a prefix. Sent again by key, each was there once.
- **A half-closed connection** got 10 of 10 replies from the head's release jawd, 3 of 3.

### Inconsistency: none

- **H7's change can't end a connection whose lane is healthy.** The lane's sender lives only in its task (`sock.rs:331-357`). The task returns only:
  - when the connection's sender to it, or its receiver, is gone, which is once the connection has ended;
  - when jawd stops;
  - or when it panics.

  At a stop, the connection's biased `select!` polls `state.stopping()` before the lane's arm. That's a watch read with `wait_for`, which holds once set (`state.rs:105-108`). So the connection ends with `Ok` before it can see the lane gone. I SIGTERMed the release jawd three times, each with seven connections open, and it exited 0 with no connection ending in an error. The seven were:
  - one idle and focused, one that had used its lane, and one that had sent a find;
  - one with a send waiting on the writer, held from outside;
  - one half-closed;
  - one that read nothing while it sent 300 `status`, with a reply waiting and 65 or 66 requests in lanes in all;
  - the one that asked `status`.

  No connection ended early anywhere else either: not in the 63 runs on the head, the gate's suite, jaw-client's three tests against a real jawd, the kill and gone tests, or ten pipelined rounds on one release connection.
- **Order and counts are as before.**
  - The lane's order is untouched: probe E passes, the lane-order test passes in the gate, and P3's five replies came in order.
  - Apart replies come "in no order with the rest" (README:17), so the inner task breaks no promise of order.
  - When a connection whose lane died ends, its part of `in_lanes`, `apart` and `replies_waiting` goes back. Without that, lane dies fails 3 of 3.
- **Round two's other findings of consistency hold.** Probes A, B, E, R and U pass 3 of 3:
  - no window goes back in seq;
  - a chat list watched while a forward is kept shows none or all of it: 15 or 16 results of 24 showed none, the rest all three, none part;
  - a window dropped while a reply waits is resynced after it, then fresh;
  - the 16 MiB bound on kept reads holds.

### Must change before landing

None.

### Unpredictable performance (follow-ups; no budget broken)

- **One more task for each find and each long request.** `on_own_task` spawns the inner task from the outer one. A find goes at every keystroke, and spawning a task costs microseconds, so I didn't measure it.
- **Round two's P4, and round one's P1 to P3 in both notes, stand as written below.** `b6841e8` doesn't touch `status`, the lane's bound or what a flood goes through, so I didn't run the flood and `status` benches again.

### Hygiene (follow-ups; none blocks)

- **H12. Nothing in the branch tests that a find whose task panics is answered** (`sock.rs:408`).
  - The two H7 tests pin `on_own_task`, but not that `Finding::start` uses it, and nothing in the branch's tests can make a find panic.
  - With the find back on a plain task, as at `49d6a89`, lane dies and request dies pass 3 of 3, and so do the find, socket and feed suites.
  - Without `on_own_task`, a find whose task panics leaves `apart` up, and H7 is back for finds: a half-closed connection never ends, and a gone interface stays counted, in front, until jawd restarts.
  - **Fix:** a test-only find that panics, as my probe-only hook does: a find for `audit4.panic` panics in `Finding::start`'s future under `cfg(test)`. Then adopt P5, `audit4_a_half_closed_connection_whose_find_dies_is_told_and_still_ends`.
- **H13. Nothing in the branch tests that a dying lane's handed-over replies still come.**
  - The code is right (above), but a rewrite that ended the connection once it saw the lane dead would lose them with no test failing.
  - They're the replies to requests already carried out, a send's among them. The connection still ends, so the interface asks again.
  - **Fix:** adopt P3, `audit4_a_lane_that_dies_still_sends_each_reply_it_handed_over_once_in_order_then_ends`.
- **H14. The README doesn't say what a failure inside jawd does to a connection.** Suggested, after `protocol/README.md:21`:

  > If jawd fails inside while answering a request that waits its turn, it closes the connection; an interface connects again and asks again what it had no reply to. A find or a long request that fails that way is answered with -32603.

- **H15. The lanes line's other counts are still plural at one** (`jaw-client/src/lib.rs:220`): "1 lanes full", "1 replies waiting" and "1 finds and saves".
- **Round two's H8 to H11 stand,** and so does its note for the media branch, with one addition: a dead lane drops the replies of saves still under way, as a disconnect does, so a save asked again must do no harm.

## Measurements

All on the same Mac Studio (M1 Ultra, 20 cores, macOS 26.6.2), with rustc 1.99.1 and rusqlite 0.40.2 with its bundled SQLite, while other builders and auditors kept the load at 43 to 96. The copies:
- **`/tmp/jaw-sock4-audit`** is `b6841e8` as it is: the gate, and the head's release build.
- **`/tmp/jaw-sock4-audit-mut`** is `b6841e8` with my probes (`/tmp/jaw-sock4-audit-probes.diff`, which applies to `b6841e8`): the probes and the mutations (`/tmp/jaw-sock4-audit-mutate.py`, `-run.py`, `-queue.sh`).

Each copy's build folder was seeded from round two's by copy-on-write. As I finished, a cleanup of `/tmp` that I didn't start was deleting every round's folders, mine among them. Copies of my probes, scripts and logs are in this auditor's store, `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/sock-flood-round3/`.

Nothing in `~/src/jaw` or the builder's worktree was changed apart from this file. Nothing touched the owner's folder: every jaw and jawd I ran had `JAW_HOME` and `TMPDIR` in my own folders. No Go engine ran: the tests and `jawd demo` make their own made-up accounts, and `target/engine/jaw-engine` wasn't built in my copies, so the test that would run it skipped. Every jawd I stopped was one I'd started.

**The gate at `b6841e8` (debug), at load 51 to 62:**
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `scripts/sync-protos.py --check` and the generated engine code: unchanged.
- `cargo test --workspace --no-fail-fast`:
  - passed: jaw-client's 17 and its 3 against a real jawd, jaw-media's 22 and 17 (1 ignored), jaw-proto's 1, jaw-voice's 36 (2 ignored), wa_proto's 3, and jawd's 3 integration tests;
  - jaw: 231 passed and 2 ignored, but `bench::tests::a_flood_alone_is_drawn_at_most_60_times_a_second` drew 16 frames a second against its floor of 20. That's jaw's own bench with no jawd (`jawd: false`), and the branch doesn't touch it;
  - jawd: 368 passed and 3 ignored, but `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean` took longer than its bound on a stop. `stop` is main's: the branch changed only what `logged_out` returns;
  - each of the two passed 5 of 5 alone, at loads of 47 to 53.
- The engine's Go tests, `go vet` and `gofmt` weren't run: the branch doesn't touch `engine/` or `proto/`.
- `scripts/check-data-audit.py 4ca6774 HEAD` fails, as it should for now: `1c1f96c`, `a7df8a5`, `49d6a89` and `b6841e8` touch the data layer and have no trailer. `1f06f0c` touches none of its paths.

**Mutations.** Each is one change in the mutation copy. The tests were rebuilt, and each was run alone (`--exact`) under a 400 s alarm, at loads of 43 to 74; the find, socket and feed suites under `find_plain_spawn` ran once, at 68 to 80. The builder's tests are named as in the table above. "Long requests" is round two's cap test, `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`, and T5, the find test and S1 are as in round two's table.

| Mutation | What it does | The builder's tests | My probes |
| --- | --- | --- | --- |
| `settled_ignores_apart` | the drain doesn't wait for finds and long requests | H fails 3/3; H2 and half-close pass 3/3 | H fails 3/3; H2 passes 3/3 |
| `settled_ignores_waiting` | the drain doesn't wait for a reply waiting for room | H and H2 fail 3/3; half-close passes 3/3 | H fails 1/3; H2 fails 3/3 |
| `settled_ignores_lane` | the drain doesn't wait for the lane | half-close and H2 fail 3/3; H passes 3/3 | |
| `eof_ends` | the end of the lines ends the connection, as at `c768143` | H, H2 and half-close fail 3/3 | |
| `dead_lane_ignored` | a dead lane ignored, as at `49d6a89` | lane dies fails 3/3 | P1 and P3 fail 3/3 |
| `dead_lane_drops_handed_over` | a dead lane ends the connection as soon as it's seen, its handed-over replies with it | lane dies passes 3/3 | P1 passes 3/3; P3 fails 3/3 |
| `apart_panic_unanswered` | a task of its own that panics sends no reply, as at `49d6a89` | request dies fails 3/3 | P2 and P5 fail 3/3 |
| `find_plain_spawn` | a find on a plain task, as at `49d6a89` | lane dies and request dies pass 3/3; the find, socket and feed suites pass, 72 tests | P5 fails 3/3 |
| `answers_while_waiting` | the lane's replies taken while one waits, in the arm `b6841e8` changed | T5 fails 3/3 | E fails 1/3; P3 fails 3/3 |
| `finished_while_waiting` | finds' and long requests' replies taken while one waits | the find test fails 3/3 | E fails 3/3 |
| `lanes_not_taken_back` | a connection's counts not taken back when it ends | lane dies and S1 fail 3/3 | P3 fails 3/3 |
| `apart_unbounded` | no cap on finds and long requests | long requests fails 3/3 ("the 65th, refused") | C fails 3/3 |

**My probes on the head**, in `/tmp/jaw-sock4-audit-probes.diff`, 3 runs each at loads of 51 to 71, all passing, beside the builder's five tests of the drain and H7:
- **Round two's A, B, C, E, F, G1 to G3, H, H2, P1, P2, R and U,** unchanged but for leaving out my own `test.panic` hooks, which the branch has now.
  - G1, G2 and G3: the gone interface was forgotten 150 to 156 ms, 10 to 99 ms and 120 to 211 ms after its requests could go on.
  - P1: forgotten 9 to 151 ms after its interface went; at `49d6a89`, never. P2: replies 5 and 6, then the connection closed.
- **P3** (new): a lane that dies with one reply waiting for room and four handed over behind it. Replies 20 to 24, then closed, nothing counted.
- **P5** (new): a find that panics, through a probe-only hook in `Finding::start` (under `cfg(test)`, for the query `audit4.panic`), on a half-closed connection. -32603 and the `status`'s reply, each once, then closed.

**The test-only requests in a release build** (`/tmp/jaw-sock4-audit-release.py`):
- `strings` finds none of "the test made", "test.panic" and "test.wait" in the head's release jawd, and each 3 times in the mutation copy's test binary.
- On a release `jawd demo` of 2,000 messages:
  - `test.panic`, `test.panic.apart` and `test.wait` were each error -32601 ("jawd doesn't know test.panic", and so on), answered in turn before a `status`, 3 of 3.
  - In ten rounds where they were pipelined among two keyed sends, a draft, a find and a `status`, each was -32601, and every other request was answered once, the lane's in order. The 20 sends were each kept once, and the connection went on.
- **SIGTERM with connections open:** the seven connections above, then SIGTERM, with the writer let go 0.5 s later. jawd exited 0 each of 3 times, with no connection ending in an error, no panic, and no "lane stopped" anywhere in its log.

**Round two's release scripts, on the head** (`/tmp/jaw-sock4-audit-halfclose.py`, `-gone.py`, `-kill.py` and `-kill-a.py`, round two's with my folders), at loads of 44 to 62:
- **A half-closed connection:** 10 of 10 replies, 3 of 3.
- **Interfaces that go**, 10 rounds: 3 sends carried out each round. The interface was forgotten a median 328 ms after the writer was let go, the slowest 844 ms. Each send sent again by key was there once. Round two measured 145 and 879 ms at `49d6a89`. `b6841e8` doesn't change how a gone interface's connection ends, so I take the difference for load (H8).
- **(b)'s kill test**, 40 kills: none kept 14 times, the first send 9, all three 17, each a prefix.
- **(a)'s kill test**, 6 kills: 3, 2, 12, 0, 0 and 8 kept, with about 2, 1, 5, 0, 0 and 8 replies read. Sent again by key, there were exactly 40, each once.

# Round two: 49d6a89, the round-one fixes and this round's commit

Range: `4ca6774..49d6a89` in `sock-flood` (worktree `~/src/jaw-sock`), 26 October 2026. Auditor: database-expert. Round one was two independent audits of `cc7fad7..c768143`, (a) and (b). Both are kept below as written, each with its verdict, and this round's verdict is the note's last line. Line numbers are at `49d6a89`, and paths are under `crates/jawd/src/` unless they say otherwise.

The branch is four commits on main `4ca6774`:
- **`1c1f96c` and `1f06f0c`** are round one's `fffb143` and `c768143`.
- **`a7df8a5`** is the builder's answer to round one (`e3ae66c`): the lanes in `jawd status`, and a test for each of (a)'s T1 to T5.
- **`49d6a89`** is this round's commit:
  - a half-closed connection is drained;
  - the logout's push follows its answer by construction;
  - finds and `media.save` are answered on tasks of their own, the "apart" tasks, at most 64 a connection;
  - the README, and jaw-client's falls-behind test.

**The rebase is plain.** `git range-diff` shows the three earlier commits as the same patches (`=`). Since `cc7fad7`, main touched three of the branch's files:
- one test line in `sock.rs` (`store.indexed()` before a find, which main's search backlog needs);
- README text about message actions, away from the branch's lines;
- new tests in `crates/jaw/src/wire/tests.rs` for the terminal's quick looks and split panes, some against a real jawd. They pass against the branch's.

The branch's new finds only check that a reply comes, so main's indexing doesn't change them.

There's still no SQL, schema, migration, PRAGMA, index, trigger or plan change. Nothing is acknowledged sooner than before:
- a lane reply goes only once `answer` has returned, and an apart reply once `answer_apart` has;
- the logout's answer still follows its synced commit (`sync/mod.rs:140-151`), and its push now follows the answer.

What changed is which requests wait for which, which replies a connection still sends after its interface stops writing, and what `status` reports.

## Summary

- **Nothing is lost and nothing is inconsistent.** Everything round one found holding still holds at the new head. I checked it with both rounds' probes, 30 mutations, two kill tests and a test of interfaces that disappear (Measurements).
- **Every must-change in both round-one notes is closed** by a test that fails without its rule (the table below). (b)'s T6 is covered by the builder's reworked test, which now confirms through `status` that the reply waits. Not adopting (b)'s probe R2 is fine.
- **One rule this round adds isn't fully pinned (must change).** At a half-close, the connection waits for three things before it ends (`sock.rs:106-108`): its lane, its finds and long requests, and a reply waiting for room. The builder's test pins the lane part only.
  - It passes 3 of 3 with either of the other two conditions removed.
  - My probes H and H2 fail 3 of 3 each without theirs.
- **The apart tasks can't reorder, lose or duplicate a reply, and the cap holds.**
  - Each task sends exactly one reply, and it's counted until the connection takes it, so at most 64 of their replies wait.
  - Past 64, a long request or a find is refused with error 5 and `{"limit": "apart", "most": 64}`, and every place freed is taken again.
- **The drain doesn't hang a connection whose interface has gone, unless a task under it dies (hygiene).**
  - On a real jawd, an interface that went entirely with 20 sends in its lane was forgotten a median of 145 ms after they could go on, the slowest 879 ms. On main: 117 ms, the slowest 2,442 ms.
  - If the lane's task or an apart task panics, the connection never ends. A gone interface stays counted as connected and in front, and a half-closed one is never closed.
  - On main and at `c768143`, the end of the lines ended the connection at once.
- **The falls-behind test's new wait hides nothing.**
  - It still checks that the slow interface gets both resyncs, stays connected, and ends with what the fresh one has.
  - It fails 2 of 2 when `Behind` drops the windows without owing a resync, and 1 of 1 when `Behind` never fires.
- **`status` during a flood costs on the head what it costs on main.** Its p50 was 48 to 299 ms and its slowest 406 to 703 ms, with a draft right behind it waiting as long on both builds. It makes 16 counts, each on a fresh connection and the largest by a scan, on a runtime thread. A follow-up for main's code (P4).
- **The flood figure holds.** During a flood of 2,000 a second, the head opened a chat in 8.7 to 35.4 ms at p99 against main's 55.4 to 306.3 ms, and drew 126 opens in each run against main's 66 to 117. That was at loads of 43 to 56, too loaded to call the 30 ms budget met.
- **The gate passes, apart from one store test outside the branch.** It failed once in the full suite at load 64 to 70 and passed 2 of 2 alone. `check-audit` fails only for want of the trailers.

## Round one's must-changes, each against its rule

Each test is the builder's, at its line in `feed/tests.rs` unless it says otherwise. It passes 3 of 3 as the branch is, and fails without its rule as the last column says (Measurements, Mutations).

| Item | Rule | The test | Without the rule |
| --- | --- | --- | --- |
| (a) T1, (b) T1 | the lane's bound | `with_64_requests_in_its_lane_a_connection_reads_no_more_and_drops_none` (`:2512`) | fails 3 of 3: the watch behind 64 requests is read |
| (a) T2 | requests ahead of the windows' flushes | `requests_sent_during_a_windows_read_all_go_ahead_of_the_windows_still_due` (`:2465`) | fails 3 of 3 without `biased`, only 0 to 15 of the 40 going first, and 3 of 3 with every due window flushed at once |
| (a) T3, (b) T2 | the windows held while a reply waits | `a_push_never_overtakes_a_waiting_watch_result_even_with_room_made_mid_flush` (`:2604`), paused by `Feed::pause_next_flush` (`feed/mod.rs:159`) | fails 3 of 3 without the hold call, and 3 of 3 with the client's hold doing nothing |
| (a) T4, (b) T4 | no window starves | `no_window_starves_while_the_others_keep_falling_due` (`:2640`) | fails 3 of 3 with changes heard before a due flush: three windows got no batch in 2 s |
| (a) T5, (b) T3 | the lane's replies wait while one waits | `replies_coming_down_the_lane_while_one_waits_for_room_each_come_once_in_order` (`:2704`) | fails 3 of 3 |
| (b) T3 | finds' replies wait while one waits | `a_finds_reply_coming_while_another_reply_waits_for_room_neither_takes_its_place_nor_is_lost` (`:2761`) | fails 3 of 3, and so does my probe E, which mixes long requests among the finds |
| (b) T5, (a) H2 | the logout's push after its answer | `the_connection_push_after_a_logout_comes_after_its_answer_on_the_raw_socket` (`sock.rs:861`) | fails 3 of 3 with the lane's old order and 2 ms between push and answer (200 of 200 pushes first); 3 of 3 with both of the connection's guards removed (114 to 124 of 200) |
| (b) T6, (a) H5 | `Behind` while a reply waits | `an_interface_owed_a_reply_it_has_no_room_for_is_still_told_to_watch_again` (`:2336`) | fails 3 of 3 with the windows unpolled while a reply waits, and 3 of 3 with a held flush not counted as blocked |
| (a) S1, (b) 1 | the lanes in `jawd status` | `status_says_how_many_requests_are_in_the_lanes_and_whose_lane_is_full_or_reply_waits` (`:2564`) | fails 2 of 2 each with a waiting reply not counted, a full lane not counted, or a connection's part not taken back when it ends |
| (a) H1 | a half-closed connection's replies | `an_interface_that_closes_its_writing_side_still_gets_every_reply_and_then_the_connection_ends` (`:2730`) | fails 3 of 3 when the end of the lines ends the connection, as at `c768143`; but see T7 |
| (b) 3, (a) H3 and H6 | the README | `protocol/README.md:16-21`, `:42`, `:206`, `:216` | each point asked for is there |

- **`jawd status` prints the lanes** (`jaw-client/src/lib.rs:214-224`): `lanes 1 requests, this one among them, at most 64 a connection; 0 lanes full, 0 replies waiting for their interface to read; 0 finds and saves on tasks of their own, at most 64 a connection`. "1 requests" wants a singular.
- **(b)'s T6 without R2.** Two things make the builder's test enough:
  - It fills the queue with pushes that aren't for windows, then confirms through `status` that the reply waits before any message comes. So its premise no longer depends on timing, and it passes 3 of 3 at load 46 to 48.
  - R2's other check, that windows watched again after their resync equal a fresh query, is about resync in general, not this change. My port of R2 (probe R) passes 3 of 3, and fails 3 of 3 with the windows unpolled.
- **The logout's order now rests on two guards in the connection, on top of the lane's order** (`sock.rs:344-347`):
  - the bias (`:231`);
  - the push arm, which first sends any lane reply handed over before the push (`:290-295`).

  Each guard alone keeps the order: without `biased` the test passes 3 of 3, and without the push arm's sending 2 of 2. The builder's "the old order puts the push first 200 of 200 times" holds only with a moment between the two sends. With the lane's old order and nothing between, the push came after its answer 600 times in 600, since the connection sends a reply already handed over first. With the answer, 2 ms, then the push, the test passes 3 of 3: the order no longer depends on nothing awaiting in between.
- **(a)'s T3 pause belongs to the feed** (`feed/client.rs:1046-1049`, `feed/mod.rs:159`), as asked, so tests running at once can't trip each other's.

## Findings

### Data loss: none

- **Every acknowledgement still follows a synced commit.**
  - A lane request is answered only once `answer` returns. An apart request is answered only once `answer_apart` returns, and nothing it can do yet writes (`media.save` isn't in this branch, and `test.wait` is test-only).
  - The logout forgets the account with `apply_durable`, and only then returns its answer and push (`sock.rs:640-650`, `sync/mod.rs:140-151`).
- **An apart reply is sent once, never lost while the connection lasts, and never duplicated** (`sock.rs:259-274`, `:356-361`, `:243-246`).
  - Each find, and each request `apart_method` names (`:455-457`), is counted (`parted`, `:121`) before its task starts. Its task sends exactly one reply on `found`: an invalid find and a missing finder answer at once, and a cancelled find answers that it was cancelled. The count goes back (`returned`, `:126`) only when the connection takes the reply, so at most 64 replies wait unread.
  - A reply that comes while another waits for room stays in the channel (`if open`), and goes once room comes.
  - In probe E, the queue to the interface was full and the interface sent a find, six long requests, twelve `status`, a watch and a focus. Every reply came once, the lane's in order, 3 of 3.
  - In probe C, 64 long requests waited on the writer. The 65th, the 66th and a find were refused with error 5 and `data` `{"limit": "apart", "most": 64}`, and nothing else came. Once the writer could write, all 64 were answered once, the count went back to 0, and 64 more were taken. That was 3 of 3, two rounds each.
  - Removing the cap fails the builder's test 3 of 3 ("the 65th, refused") and probe C 3 of 3. Sending long requests down the lane fails the builder's test 3 of 3: the `status` and the watch behind them waited.
- **A kill with requests in a lane leaves a prefix of them done, each once.**
  - (b)'s kill test ran the head's release jawd, its writer held from outside, with two keyed sends and a draft. Over 40 kills, it kept none 29 times, the first send 4, both sends 3 and all three 4.
  - (a)'s kill test pipelined 40 keyed sends and killed jawd 5 to 200 ms later. Over 6 kills, it kept 6, 2, 17, 1, 1 and 32, never fewer than the replies read before the kill.
  - Sent again by key, every send was answered with the message kept, and each was there once.
- **An interface that goes entirely still has its lane's requests done or not done, a prefix, each once.** With the drain, its connection lives until a reply can't be written, so a few more of its requests are carried out than before.
  - Probe F held the writer, sent two keyed sends and a draft, and closed both sides. All three were carried out, 3 of 3, where round one found only the first.
  - On a real jawd, 3 or 4 of 20 keyed sends were carried out on the head, and 2 on main, in 10 rounds each.
  - Sent again by key, each was there once.
- **A half-closed connection gets its replies.** On the head's release jawd, `hello` and 10 `status` followed by `shutdown(SHUT_WR)` got 10 of 10, 3 of 3, where round one measured 0 of 10.

### Inconsistency: none

- **Answering watches first still shows a consistent state, then the request's effect, whole.**
  - Probe A sends `send.text` while the writer is held, then `messages.watch`. The result equals a fresh query without the message, and once the writer can write, the window equals a fresh query with it. That held over 5 rounds in each of 3 runs. The window's push of the sent row came before the send's reply in every round, with the same `v`.
  - Probe B forwards to three chats and watches the chat list 0 to 12 ms later. Of 72 results, 50 showed the forward in none of its chats and 22 in all three, none in part. After each result without it, the first push to show it showed all three.
- **No window goes back in seq.** Every probe folds each window's result and pushes and checks seq, and so does the feed tests' `Watcher`.
- **The lane keeps effects and replies in order.** With the lane's requests answered concurrently, the lane-order test, T5's and T1's fail 2 of 2 each.
- **The 16 MiB bound on kept reads holds.** Probe E checks `kept_bytes <= kept_most` with the queue full and replies waiting.
- **A window dropped while a reply waits is resynced, and fresh once watched again.**
  - In probe R, a ported R2, the queue was full and a `status` reply waited, while both windows changed for 2.5 times `behind`. Both were dropped, and once the interface read, it got the reply and then a resync for each.
  - Each, watched again with `since` and `have`, equalled a fresh query, 3 of 3.

### Must change before landing

- **T7. The drain's wait for finds, long requests and a waiting reply isn't pinned** (`sock.rs:106-108`, `:223-225`, `:250-253`).
  - **What the rule is.** At the end of the interface's lines, the connection reads no more but ends only once its lane is empty, no find or long request is under way, and no reply waits for room. The README promises it (`protocol/README.md:21`): "still gets the reply to every request it sent". The builder's commit says "finds included".
  - **What the builder's test pins.** It fails 3 of 3 when the end of the lines ends the connection.
  - **What it doesn't.** It passes 3 of 3 with `self.apart == 0` removed from `settled()`, since its find answers before its ten drafts are done. It also passes 3 of 3 with `self.waiting.is_none()` removed, since nothing waits for room when its lane empties. Without either condition, the reply that was still owed is lost at a half-close: a find's or, once the media branch lands, a save's, or the last reply when the interface's queue was full, which can be a send's acknowledgement.
  - **Fix:** adopt two probes, or tests that fail as reliably without their conditions. Both are in `/tmp/jaw-sock3-audit-probes.diff`, which applies to `49d6a89` (`git apply --check`).
    - Probe H, `audit3_half_closed_with_long_requests_and_replies_waiting_gets_every_reply_then_ends`: five long requests and five drafts wait on the writer, with the queue full, when the interface closes its writing side. It passes 3 of 3, and fails 3 of 3 without the apart condition, the long requests' replies lost.
    - Probe H2, `audit3_half_closed_its_last_reply_waiting_for_room_still_comes_before_the_connection_ends`: one draft, whose reply waits for room after the half-close. It passes 3 of 3, and fails 3 of 3 without the waiting condition, the connection closing with no reply.
    - Or change the builder's test to let the readers go only once the drafts are answered, and add a case whose last reply waits for room.

### Unpredictable performance (follow-ups; no budget broken that main didn't break more)

- **P4. `status` makes 16 counts, each on a fresh connection, on a runtime thread, inside the lane.** This is main's code (`state.rs:191-213`, `store/mod.rs:559-563`).
  - **What it does.** Each count opens a reader connection of its own and runs `SELECT count(*)`. The largest, `messages`, `history_raw` and `inbox`, scan a large table, which the design's rule forbids. The whole of it runs synchronously in `answer` on a tokio worker.
  - **Measured during a flood** (method in Measurements): `status` took p50 48 and 65 ms on main and 74 and 299 ms on the head, and at worst 406 to 703 ms, with the WAL at up to 74 to 161 MB. A draft sent right behind it on the same connection waited as long, on both builds: down the lane on the head, and on main because everything on a connection waited.
  - **Worth a follow-up.** The builder saw seconds with `jawd bench feed`'s larger WAL. The counts grow with jaw.db: these accounts held 50,000 messages, where the budgets' benchmarks use a million. On the head, `status` holds up the sends and reads sent after it on its connection, and a runtime thread meanwhile.
  - **Fix:** read the counts in one read transaction on the reader pool, off the runtime, and keep the large ones derived, or leave them out of `status`.
- **(b)'s P1, a run on a quiet machine, is still owed.** At loads of 43 to 56, a committed message took 55 to 84 ms at p99 to reach the interface on both builds, against a 5 ms budget. So this machine can't show the 30 ms budget for opening a chat met. The head stayed under it at p99 in 3 runs of 4.
- **Round one's other performance findings stand as follow-ups:**
  - (a)'s P1 and (b)'s P3: lanes are bounded in requests, not bytes, and a line's length isn't bounded.
  - (a)'s P2: pushes go last.
  - (b)'s P2: slow requests still hold up their lane. `status` is above; `chat.start`, `history.more` and `link.*` wait on WhatsApp or the engine. Finds and `media.save` no longer do.

### Hygiene (follow-ups; none blocks)

- **H7. A task that dies under a draining connection keeps the connection forever** (`sock.rs:223-225`, `:239-246`). This extends (b)'s H1. Best done in this round, since it's a few lines and the drain made it worse.
  - **What happens.** If the lane's task panics, `answers` closes, and its arm is dropped from each `select!`. The count of the lane's request then never goes back, so `settled()` never holds once the interface stops writing. A long request's task that panics leaves `apart` up the same way, since the connection keeps a sender of `found`.
  - **Measured with a probe-only panic.** In probe P1, a focused interface whose lane's task panicked went away entirely. 3 s later, `status` still counted it: `connected 2, focused true, in_lanes 2`. In probe P2, a half-closed connection whose long request panicked got its other reply and wasn't closed within 3 s, and nothing would ever close it.
  - **What it costs.** A phantom interface "in front" keeps presence online and notifications quiet for the chats it showed, and `status` counts its request, until jawd restarts.
  - **Not a regression in the cause.** With the end of the lines ending the connection, as at `c768143`, both probes pass: the connection ended in 10 to 22 ms.
  - **Fix:** end the connection when `answers` closes (`None` from `recv`). Have an apart task reply with an internal error if what it runs panics, for example by awaiting its `JoinHandle` in the task that sends the reply.
- **H8. A draining connection keeps its viewer, so a gone interface counts as connected and in front until its requests are answered.**
  - While the writer was held, `status` counted an interface that had closed both sides as connected and focused, with its requests in the lanes. That was so on main too.
  - On a real jawd, it was forgotten a median of 145 ms (slowest 879 ms) after the writer was let go; on main, 117 ms (slowest 2,442 ms).
  - Today only finds and lane requests can hold it. Once `media.save` waits for a download on an apart task, a gone or half-closed interface would stay in front for that long.
  - **Fix:** for the media branch, let the viewer stop being in front or reading at the end of the lines, or end the drain once a write to the interface fails.
- **H9. `run_for` still can't time out when the client is always ready** (`feed/tests.rs:195-205`). This is (a)'s H4 and (b)'s H2, not done. With a held flush not counted as blocked, the held-client test would still spin rather than fail, while the builder's new loops check their deadline. The fix is still `while Instant::now() < until && let Ok(ready) = …`.
- **H10. Two guards no test pins, each backed by another.**
  - The push arm's `if open` (`sock.rs:286`): the arm itself sends nothing while a reply waits (`:297`).
  - The held client not waiting for room (`feed/client.rs:964`): the bias and tokio's first-come permits give the reply the room.
  - With either removed, the ordering test and T3's pass 2 of 2. They may stand.
- **H11. (b)'s H5 stands.** `crates/jawd/src/convert/` isn't among `scripts/check-data-audit.py`'s paths, so `1f06f0c` needs no trailer under the script.
- **For the media branch.** `media.save` is apart, "in no order with the rest" (`protocol/README.md:17`), while `media.fetch` and `media.cancel` take the lane, as the rule beside the routing says (`sock.rs:459-463`). A save sent after a fetch can start before the fetch is kept, and a cancel after a save can be carried out while the save waits. The media branch should say what a save does then, and test it.

## Measurements

All on a Mac Studio (M1 Ultra, 20 cores, macOS 26.6.2), with rustc 1.99.1 and rusqlite 0.40.2 with its bundled SQLite, while other builders and auditors kept the load at 33 to 84. The copies:
- **`/tmp/jaw-sock3-audit`** is `49d6a89` as it is: the gate, and the head's release build.
- **`/tmp/jaw-sock3-audit-mut`** is `49d6a89` with my probes (`/tmp/jaw-sock3-audit-probes.diff`): the probes and the mutations (`/tmp/jaw-sock3-audit-mutate.py`, `-run.py`, `-queue.sh`).
- **`/tmp/jaw-sock3-audit-main`** is `4ca6774`: main's release build.

No dependency changed since round one, so each copy's build folder was seeded from round (b)'s by copy-on-write. Nothing in `~/src/jaw` or the builder's worktree was changed apart from this file. Nothing touched the owner's folder: every jaw and jawd I ran had `JAW_HOME` and `TMPDIR` in my own folders. No Go engine ran: the tests, `jawd demo` and `jawd bench feed` make their own made-up accounts, and every jawd I stopped was one I'd started.

**The gate at `49d6a89` (debug), at load 64 to 70:**
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `scripts/sync-protos.py --check` and the generated engine code: unchanged.
- `cargo test --workspace --no-fail-fast` passed:
  - jaw's 232 (2 ignored), jaw-client's 17 and its 3 against a real jawd, jaw-media's 22 and 17 (1 ignored), jaw-proto's 1, and jaw-voice's 36 (2 ignored);
  - jawd's 364 (3 ignored) and its 3 integration tests, but for one failure: `store::tests::indexing_for_search_waiting_for_the_wal_gets_room_with_nothing_else_committing`, "indexing took 33.97s in 202 waits for the checkpointer".
  - That test is main's (`9726312`), in code the branch doesn't touch. It passed 2 of 2 alone, in 3 s each, at load 47.
- The engine's Go tests and vet weren't run: the branch doesn't touch `engine/` or `proto/`.
- `scripts/check-data-audit.py 4ca6774 HEAD` fails, as it should for now: `1c1f96c`, `a7df8a5` and `49d6a89` touch the data layer and have no trailer. `1f06f0c` touches none of its paths.

**Mutations.** Each is one change in the mutation copy. The tests were rebuilt, and each was run alone (`--exact`) under a 400 s alarm, at loads of 37 to 82. The builder's tests are named by the item they pin, as in the table above:
- "ordering" is `a_watch_result_waiting_for_room_comes_before_anything_its_window_sends`;
- "lane order" is `requests_down_the_lane_take_effect_in_the_order_they_were_sent`;
- "watch while writer" is `a_watch_is_answered_while_a_request_sent_before_it_waits_on_the_writer`;
- "long requests" is `a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`;
- "falls behind" is jaw-client's `an_interface_that_falls_behind_is_told_to_watch_again_and_ends_as_a_fresh_one`.

| Mutation | What it does | The builder's tests | My probes |
| --- | --- | --- | --- |
| `lane_unbounded` | lines read whatever the lane holds | T1 fails 3/3 | |
| `no_bias` | the connection's `select!` unbiased | T2 fails 3/3 (9, 0 and 15 of 40 first); logout passes 3/3 | |
| `flush_all` | a flush does every due window | T2 fails 3/3 (0 of 40) | |
| `no_hold_call` | no `windows.hold(true)` (`sock.rs:316`) | T3 fails 3/3 | U fails 3/3 |
| `client_hold_noop` | a held client flushes (`feed/client.rs:1020-1023` emptied) | T3 fails 3/3 | U fails 3/3 |
| `hear_first` | changes heard before a due flush | T4 fails 3/3 | |
| `answers_while_waiting` | lane replies taken while one waits | T5 fails 3/3 | E passes 3/3 |
| `finished_while_waiting` | finds' and long requests' replies taken while one waits | the find test fails 3/3 | E fails 3/3 |
| `lines_while_waiting` | lines read while a reply waits | ordering fails 2/2 | E fails 2/2 |
| `pushes_while_waiting` | the push arm unguarded | ordering and T3 pass 2/2 | E passes 2/2 |
| `windows_wait` | windows unpolled while a reply waits | T6 fails 3/3 | R fails 3/3 |
| `held_not_blocked` | a held flush not counted as blocked | T6 fails 3/3 | U fails 3/3 ("spun") |
| `room_while_held` | a held client waits for room beside the reply | ordering and T3 pass 2/2 | |
| `lane_concurrent` | each lane request on a task of its own | lane order, T5 and T1 fail 2/2 | |
| `watch_in_lane` | watches down the lane | watch while writer fails 2/2 | A fails 2/2 |
| `eof_ends` | the end of the lines ends the connection, as at `c768143` | H1 fails 3/3 | H fails 3/3; P1 and P2 pass 2/2 |
| `settled_ignores_apart` | the drain doesn't wait for finds and long requests | H1 passes 3/3 | H fails 3/3 |
| `settled_ignores_waiting` | the drain doesn't wait for a reply waiting for room | H1 passes 3/3 | H passes 3/3; H2 fails 3/3 |
| `apart_unbounded` | no cap on finds and long requests | long requests fails 3/3 | C fails 3/3 |
| `apart_in_lane` | long requests down the lane | long requests fails 3/3 | |
| `waiting_not_counted` | a waiting reply not counted | S1 fails 2/2; T6 fails 2/2 | |
| `full_not_counted` | a full lane not counted | S1 fails 2/2 | |
| `lanes_not_taken_back` | a connection's part not taken back when it ends | S1 fails 2/2 | |
| `logout_push_first` | the push before the answer is handed over | logout passes 3/3 (0 of 600 first) | |
| `logout_push_first_gap` | the same, with 2 ms between | logout fails 3/3 (200 of 200) | |
| `logout_gap_after_reply` | the answer, 2 ms, then the push | logout passes 3/3 | |
| `no_drain_before_push` | the push arm sends no lane reply first | logout passes 2/2 | |
| `no_drain_before_push,no_bias` | both | logout fails 3/3 (114 to 124 of 200) | |
| `behind_no_resync` | `Behind` drops the windows and owes no resync | falls behind fails 2/2 ("jawd's resync didn't happen") | |
| `never_behind` | `Behind` never drops them | falls behind fails 1/1 (the bench ended first) | |

As the branch is, each of the builder's tests in the first table passed 3 of 3, at loads of 46 to 49. The falls-behind test passed 3 of 3 at 57 to 61.

**My probes on the branch**, in `/tmp/jaw-sock3-audit-probes.diff`: 3 runs each, all passing but P1 and P2.
- **A**, a watch while a send waits on the writer, 5 rounds a run.
- **B**, a forward racing a chat list's watch, 24 rounds a run.
- **E**, (b)'s E2 with six long requests among a find, twelve `status`, a watch and a focus, the queue full. The watch's result came first, then the others, the lane's in order.
- **C**, the cap on long requests, two rounds a run.
- **F**, a connection dropped with requests in its lane, then sent again by key.
- **G1, G2 and G3**, an interface gone while its lane waits on the writer, while its reply waits for room, and while its long requests wait. Each was still counted, connected and focused, while they waited. Each was forgotten once they could go on: in 207 to 570 ms, 43 to 196 ms and 80 to 266 ms.
- **H and H2**, a half-close with long requests and replies waiting, and with the last reply waiting.
- **R**, (b)'s R2, which fills the queue the way the builder's `fill_window` does.
- **U**, (b)'s U, a unit test of `send` with a full queue, now with `Owed`.
- **P1 and P2**, a probe-only panic in the lane's task and in a long request's task: each failed 1 of 1, the connection outliving its interface (H7).

**The kill tests** (the head's release jawd, `jawd demo` of 2,000 messages in 20 chats):
- **(b)'s**, `/tmp/jaw-sock3-audit-kill.py`. Each round held the writer from outside and sent two keyed sends and a draft, let the writer go, and killed jawd 0 to 30 ms later (20 rounds) or 0 to 150 ms later (20 rounds). Kept: none 29 times, the first send 4, both sends 3, all three 4.
- **(a)'s**, `/tmp/jaw-sock3-audit-kill-a.py`. Each run pipelined 40 keyed sends and killed jawd 5 to 200 ms later, over 6 runs. Kept: 6, 2, 17, 1, 1 and 32, with about 5, 1, 14, 0, 0 and 26 replies read. Sent again by key, there were exactly 40, each once.

**Interfaces that go away** (`/tmp/jaw-sock3-audit-gone.py`, release, `jawd demo` of 2,000 messages). A focused interface watching a chat pipelines 20 keyed sends while the writer is held from outside, and closes both sides. 300 ms later, the writer is let go. 10 rounds a build:
- **Head:** 3, 3, 4, 3, 3, 3, 3, 3, 3 and 3 of the sends carried out. Forgotten, `status` no longer counting it, a median 145 ms after the writer was let go, the slowest 879 ms.
- **Main:** 2 carried out every round. Forgotten a median 117 ms after, the slowest 2,442 ms.
- **On both,** `status` counted it as connected and focused while the writer was held. What was kept was a prefix each time, and sent again by key, each was there once.

**A half-closed connection** (`/tmp/jaw-sock3-audit-halfclose.py`, release): `hello` and 10 `status`, then `shutdown(SHUT_WR)`. The head answered 10 of 10, 3 of 3 runs; main 10 of 10, in one run.

**Opening a chat during a flood** (`/tmp/jaw-sock3-audit-flood.sh`, release, in ABBA order).
- **Setup.** One made-up account of 50,000 messages in 300 chats was seeded once, and each run took a fresh copy of it. `jawd demo --flood 2000 --seconds 30 --in-flight 256 --after 8` poured messages in, while `jaw bench --keys typing --warm 10 --seconds 25` typed, sending a message and opening two chats each cycle.
- **Columns.** Opens are chats opened by a key and drawn. "Ack" is the flood's commit-acked p99, and the last column how long a committed message took at p99 to reach the interface. Times are milliseconds.

| Run | Load before | Opens | p50 | p99 | Slowest | Ack p99 | To the interface p99 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| main 1 | 48.2 | 104 | 13.1 | 80.7 | 90.0 | 119 | 60 |
| head 1 | 47.0 | 126 | 2.7 | 14.1 | 18.6 | 39 | 56 |
| head 2 | 46.6 | 126 | 3.7 | 35.4 | 45.7 | 174 | 67 |
| main 2 | 43.3 | 117 | 10.4 | 55.4 | 56.2 | 78 | 62 |
| main 3 | 54.3 | 102 | 23.6 | 105.5 | 106.2 | 120 | 78 |
| head 3 | 55.9 | 126 | 2.6 | 8.7 | 10.7 | 24 | 55 |
| head 4 | 51.6 | 126 | 2.5 | 9.6 | 12.8 | 43 | 58 |
| main 4 | 48.4 | 66 | 78.7 | 306.3 | 346.9 | 427 | 84 |

Keys were drawn in 0.57 to 0.90 ms at p99 on the head, and 0.50 to 1.33 ms on main.

**`status` during a flood** (`/tmp/jaw-sock3-audit-status.py`, the same flood without the typing). Every half second for 30 s, one connection asked for `status` and sent a `draft.set` right behind it. Times are milliseconds, from asking.

| Run | Load before | Statuses | p50 | Slowest | The draft behind it, p50 / slowest | WAL at most |
| --- | --- | --- | --- | --- | --- | --- |
| head 1 | 47.8 | 45 | 74 | 555 | 88 / 617 | 87 MB |
| main 1 | 44.9 | 47 | 48 | 406 | 86 / 528 | 161 MB |
| main 2 | 40.7 | 45 | 65 | 469 | 115 / 569 | 113 MB |
| head 2 | 46.8, 84.5 after | 37 | 299 | 703 | 326 / 723 | 74 MB |

# Round one (a): c768143, kept as written

# Audit: the socket's lane

Range: origin/main (cc7fad7)..c768143 in sock-flood (worktree `~/src/jaw-sock`), 26 October 2026. Auditor: database-expert. Line numbers are at c768143.

The branch's two commits change:
- **`sock.rs`.** A connection answers its watches (`chats.watch`, `messages.watch`, `watch.move`, `unwatch`), `hello` and `focus` on its own task, and a `find` on a task of its own, as before. Every other request goes down a lane of its own, answered one at a time in the order sent, with at most 64 in it (`:30`, `:118`, `:171-188`). Its `select!` is biased (`:105`): room for a waiting reply first, then stopping, the lane's replies, finds' replies, request lines, the windows, and pushes last. A reply that finds the queue to the interface full waits, one at most (`:149-166`). Meanwhile no request is read and no other reply or push goes, and the windows are held but still polled (`:160`, `:109`).
- **`feed/client.rs`.** A held client sends nothing, and a flush due while it's held counts as blocked, so `Behind` still drops its windows (`:1004-1008`). A flush does one window, conversations first (`:1016-1037`). Room in the queue goes to the waiting reply first (`:948-949`).
- **The feed audit's H4 to H7**, and jaw's scripted jawd listening before a test is handed it (`crates/jaw/src/wire/tests.rs:218`).
- **`protocol/README.md`**: one sentence on what's answered at once (`:15`), and the code 5 row (`:36`).

There's no SQL, schema, migration, PRAGMA, plan or acknowledgement change. Each request still writes through the store's synced transactions as before. `sock.rs` is in the data layer because its replies are the interfaces' acknowledgements.

## Summary

- **Nothing is lost and nothing is inconsistent.**
  - Each lane request is one synced transaction, as before, and the lane keeps their order.
  - A watch reads one snapshot, so it sees none or all of an earlier request, never part of one.
  - Killing jawd with 40 keyed sends in a lane always left a prefix of them, each once. Sending all 40 again with their keys left exactly 40 (6 of 6 runs).
  - A connection dropped with requests in its lane leaves the one under way done once and the rest undone.
- **Order holds where the protocol needs it.**
  - No window's seq goes back, and nothing of a window comes ahead of its watch's result.
  - The lane's replies come once each, in order.
  - A sent row's push now often comes before `send.text`'s reply (21 of 30). The protocol's `v` rule makes that harmless, and jaw uses only the reply's `message`.
- **Five rules the change adds have no test that fails reliably without them** (must change): the lane's bound, requests going ahead of the windows, the hold at the socket, no window starving, and the lane's replies waiting while one reply waits. For each, I wrote a probe that fails every time without its rule and passes every time with it.
- **The lane is a queue that can fill, and `jawd status` doesn't show it** (must change). The design asks a number in `jawd status` of everything that grows.
- **A client that closes its write side now loses its lane's replies.** It got 0 of 10, where main gave 10 of 10. Nothing answered is lost, but requests jawd has read are dropped without a word while the client still listens.
- **The flood figure holds in direction.** A chat opened in 13 to 32 ms at p99 on the branch, at loads of 26 to 38, against 103 to 239 ms on main, at loads of 21 to 51.
- **The gate passes, apart from two timing tests outside the branch.** `notify`'s burst test and `work::serial`'s re-run test failed once in the full suite at load 50, and passed 5 of 5 each alone.
- **The feed audit's H4, H5 and H6 are done**, each pinned by a test that fails without it. H7 is done but for one limit.

## Findings

### Data loss and inconsistency: none

- **Every acknowledgement still follows a synced commit.** The lane runs the same `answer` as before (`sock.rs:370`), one request at a time. `send.text` answers once its message, outbox row, recipients and key are in one synced transaction.
- **A watch never sees part of a request.** A window's read is one read transaction (`window::query`), and each lane request one write transaction. So a watch answered while an earlier request waits sees jaw.db as it was before that request.
- **The lane keeps effects and replies in order.** It awaits each answer before taking the next (`sock.rs:183`). Answering each request on a task of its own makes `requests_down_the_lane_take_effect_in_the_order_they_were_sent` fail 3 of 3, and probes B and F fail too.
- **Requests are done once.** A request under way when its connection goes is finished, and its reply dropped. The rest of the lane is left undone, since the lane stops once nobody can take its replies (`:180`). A key sent again, even while the old lane still waits on the writer, answers with the same message, because the key is checked again inside the write transaction (probe F, and the kill test in Measurements).
- **The waiting reply is never lost or reordered.** `waiting` is one slot (`sock.rs:98`), and nothing that could fill it again is taken while it's full: the lane's replies, finds' replies, request lines and pushes each wait on `open`. Probe K sends 20 lane requests that the writer holds back while pushes fill the queue. Once the writer is let go, all 20 replies come once each, in order (2 of 2). Each guard is checked in "What holds", and the lane's is pinned only by K (T5).

### Rules the change adds that no test pins (must change)

- **T1. The lane's bound isn't pinned** (`sock.rs:118`). Removing `in_lane < LANE_MOST` passes every test of the branch's; its two lane tests pass 2 of 2. **Fix:** adopt probe B, `audit_with_64_requests_in_its_lane_a_connection_reads_no_more_and_drops_none`. It puts 64 `draft.set` in a lane that waits on jaw.db's writer, then a `messages.watch` and a `focus`. It checks the watch isn't read while the lane is full, then that all 64 are answered in order, each a success, with the watch and focus after them, and the draft `d63`. It passes 3 of 3, and fails 2 of 2 without the bound.
- **T2. Requests going ahead of the windows' flushes is pinned 1 time in 5** (`sock.rs:105`). Without `biased`, `a_request_waits_for_one_windows_read_at_most_rather_than_the_whole_flush` fails 1 of 5. Its one request races one window's flush, and an unbiased `select!` still takes the request first 6 times in 7: it starts from a random branch, and only a start at the windows' takes them first. **Fix:** adopt probe D, `audit_requests_sent_during_a_windows_read_all_go_before_the_next_window`. During a 300 ms read of a conversation, it writes 40 `focus` lines, and all 40 replies must come before the chat list's push, in each of two rounds. It passes 3 of 3, fails 3 of 3 without the bias, and fails 3 of 3 with every due window flushed at once.
- **T3. The hold at the socket isn't pinned** (`sock.rs:160`). Without `windows.hold(true)`, every test passes. The builder's `a_watch_result_waiting_for_room_comes_before_anything_its_window_sends` passes 3 of 3, and even with the bias gone too it fails only 1 of 3.
  - **Why a black-box test can't pin it:** tokio's queue to the interface hands room to its waiters in turn, and the connection's `reserve()` always registers before the windows' `room()`. So the result gets the first room either way. A push can overtake it only if room comes after the `select!` has polled `reserve()` and before a due flush looks for room. That's a gap of microseconds inside one task, which no test can open from outside.
  - **Fix:** a test-only pause in `flush()`, between its held check and its look for room, and a test that makes room during it. Mine is `audit_pause` and probe J, `audit_a_push_never_overtakes_a_waiting_watch_result_even_with_room_made_mid_flush`. It fills the queue with pushes, so a watch's result waits, then makes the window due, and reads while the flush is paused. It passes 3 of 3, with the flush never reaching the pause, since it's held. Without the hold call it fails 3 of 3, and without the client's hold (`client.rs:1005-1008`) 2 of 2: in both, the window's push comes before the watch's result. Mine is a global, which tests running at once could trip, so the builder's should belong to the feed, as `Feed::slow_reads` does.
- **T4. No window starving isn't pinned** (`client.rs:944-946`). One window per flush can starve the windows sorted last, unless a flushed window can't fall due again before the others are flushed. `ready()` returns `Flush` before it hears more changes, which ensures that, but no test checks it. **Fix:** adopt probe E, `audit_no_window_starves_while_the_others_keep_falling_due`. It runs 6 conversations and the chat list, with reads slowed to 6 ms and a message to every chat every 3 ms for 2 s. No window may go 500 ms without a batch. It passes 3 of 3: each window got 24 to 36 batches, and its longest gap was 87 to 158 ms. When changes are heard before a due flush (`hear_first`), it fails 2 of 2: three windows got 1 batch each in 2 s, with gaps of 2,045 to 2,063 ms, while the other four got about 60.
- **T5. The lane's replies waiting while a reply waits isn't pinned** (`sock.rs:113`). With `if open` taken off the lane's replies, each reply that comes while one waits takes its slot, and the one before is lost. The branch's four socket tests all pass that way (1 of 1 each). The interface never hears those requests were done: with a key it can ask again safely, but an unkeyed `send.text` asked again is a second message. **Fix:** adopt probe K, `audit_replies_coming_down_the_lane_while_one_waits_for_room_each_come_once_in_order`. It sends 20 `draft.set` that wait on the writer, fills the queue with pushes, and lets the writer go. All 20 replies must come once each, in order, and the draft must be `d19`. It passes 2 of 2, and without the guard fails 2 of 2, keeping only `[119]`. Finds' replies have the same guard (`:117`), which nothing probes. A find answered while a reply waits would show it the same way.

The probes are in `/tmp/jaw-sock-audit-probes.patch` (`git apply --check` passes at c768143): A1, A2, B to H, J, K, the pause and the `run_for` fix (H4 below). C is the probe whose premise was wrong (see T3); it passes without the hold, so leave it out.

### A queue `jawd status` doesn't show (must change)

- **S1. The lanes and the waiting replies aren't in `jawd status`.** The design says "nothing grows without a limit and a number in `jawd status`" (design:145), and the data-integrity rule asks it of every queue. A lane holds up to 64 requests, read and not yet answered. A full lane stops the connection reading, and a waiting reply stops it altogether. Probe B's status, taken while a lane was full, shows `interfaces {"connected":2,"focused":false,"notifications":"off"}` and the windows' counts, and nothing of the lane. **Fix:** add to `interfaces` (`state.rs:210-214`, and `Interfaces` in jaw-proto) the requests in all lanes, the connections whose lane is full, and the connections with a reply waiting. Test them as the windows' counts are.

### Unpredictable performance (follow-ups; no budget broken)

- **P1. A lane is bounded in requests, not bytes.** Within the text limit, a `draft.set` holds up to 256 KiB, so 64 of them hold about 16 MiB a connection, where main held one request at a time. A text past the limit is refused only once the lane reaches it, and a line's length isn't bounded at all, as on main (`BufReader::lines`). Interfaces are the owner's own processes (the peer's uid is checked), so this is about a buggy one. A cap on a line's bytes, and on the lane's, would close it.
- **P2. Pushes go last in the biased `select!`** (`sock.rs:137`). On main they had an even chance. A connection whose requests, replies and windows are all ready stays behind on its pushes. Past the 256 the channel keeps, it misses them, as any slow connection did on main ("the next says how things are", `:138`). Nothing counts the pushes a connection misses. A count in `jawd status` would show whether floods cause it.
- **P3. Opening chats with no flood didn't separate from main** (Measurements). In six back-to-back pairs at loads of 25 to 63, p99 was higher on the branch in 3 and lower in 3, with 45 to 63 opens a run.

### Hygiene

- **H1. A half-closed connection loses its lane's replies** (`sock.rs:119`, `:180`). At the end of the interface's lines, the connection ends at once, and its lane stops before its next request. A client that sends `hello` and 10 `status`, then shuts its write side and reads, got 0 of 10 replies on the branch and 10 of 10 on main (3 runs each). Main answered each line before reading the next, so it reached the end only after every reply. No client in the repo half-closes (they all shut both sides), and nothing answered is lost. But a script that shuts its write side once its input ends would see its send neither done nor answered. **Fix:** at the end of the lines, stop reading but keep serving until the lane is empty and no reply waits, then end. Or say in the README that closing the write side ends the connection, and requests not yet answered with it. It's a regression from main, and a few lines, so best done in this round.
- **H2. The logout's `connection` push isn't ordered after its answer by construction** (`sync/mod.rs:153`). README:229 promises the push "on the connection that asked just after the answer". `logged_out` broadcasts the push before the lane hands back its answer, so only the lack of an await between the two keeps the order. In 600 logouts the push never came first (probe G, 3 runs of 200). With a 5 ms pause after the push (`logout_gap`), it came first 200 of 200. On main the connection answered inline, so its pushes waited until the reply had gone. **Fix:** have `link.logout`'s answer carry the push for the connection to send right after the reply, or drop "just after the answer" from the README.
- **H3. The README doesn't say plainly what a watch sees when it overtakes a request** (`README.md:15`). The new sentence says watches don't wait. It doesn't say that a watch then shows jaw.db before the earlier request, whose effect comes as a push. It also doesn't say that a full lane, or a reply waiting for room, holds watches up too, since lines are read in order. Suggested:

  > A watch is answered with what jaw.db holds when it's read, which may be before a request sent ahead of it has taken effect; that request's effect then comes as a push, and a row it makes can't be an anchor yet. jawd holds at most 64 of a connection's other requests; past that, or while a reply waits for the interface to read, it reads nothing more from that connection, watches included.

  README:200, "every window showing the chat gets it at the same moment", could also say the windows' push may come before the reply.
- **H4. `run_for` can't time out when the client is always ready** (`feed/tests.rs:194-204`). `timeout_at` polls the client first, so a `ready()` that never waits never lets the deadline through. With a flush due while held not counting as blocked (`held_not_blocked`), `a_held_client_sends_nothing_and_is_told_to_watch_again_once_held_for_behind` spun at full CPU. Its two runs went 12 minutes and 97 seconds before I stopped them. This is H5 of the feed audit again, in another helper. **Fix:** `while Instant::now() < until && let Ok(ready) = …`. With that, the test fails 2 of 2 under the same mutation ("its window was dropped"), and passes 3 of 3 as the branch is.
- **H5. `an_interface_owed_a_reply_it_has_no_room_for_is_still_told_to_watch_again` doesn't reach the held path.** Its client is blocked by its own flush before the reply waits. So the test passes without the hold call (3 of 3) and without a held flush counting as blocked (2 of 2). It does pin what it's named for: with the windows left unpolled while a reply waits (`windows_wait`), it fails 2 of 2. The held path is pinned by the client test above, once H4 is fixed.
- **H6. H7 left one limit out.** The code 5 row now lists the outbox, a forward's chats and a connection's windows. It leaves out text past 65,536 characters, which `send.text` and `draft.set` answer with error 5 (README:202, :208).

### What holds

- **A watch that overtakes an earlier request shows a consistent state, then that request's effect.**
  - Probe A1 holds jaw.db's writer, then sends `send.text` and a `messages.watch` of its chat. The watch was answered at once without the message, then its row came as a push, then the send's reply (3 of 3).
  - Probe A2 races 40 watches each against a `forward` of two messages sent just before it. Per run of 40, 26 to 31 saw none of the forward, 9 to 14 all of it, and none part of it (3 runs).
  - That's acceptable for the protocol. Replies already "come in any order", windows converge through pushes, and an upserted row replaces the held one only when its `v` is at least as new (README:66-67). `messages.watch` of a chat `chat.start` is still making answers an empty window, which fills through pushes. An anchor an earlier request would make isn't there yet (error 3), but an interface can't know its id before that request's reply.
- **`send.text`'s row and its reply.** On the branch, the windows' push of a sent row came before its reply 21 of 30 times (probe H). On main the reply came first by construction. jaw uses only the reply's `message` (`crates/jaw/src/wire.rs:433-435`) and draws only what jawd sends, so nothing shows twice. An interface that applies rows by `v` keeps the newer one.
- **The 65th request waits in the socket, unread.** Nothing is dropped and nothing errs. The interface's writes back up in the socket, as for any reader that stops (probe B).
- **Each of the waiting reply's guards matters.** With lines read while a reply waits (`read_while_waiting`), the builder's ordering test fails 2 of 2: the request after the watch is answered and takes the slot, so the watch's result is lost. With the lane's replies taken while one waits (`answers_while_waiting`), probe K fails 2 of 2, keeping only `[119]` of 20 replies, while the branch's own tests pass (T5). With the windows unpolled meanwhile (`windows_wait`), as on main, `an_interface_owed_a_reply_it_has_no_room_for_is_still_told_to_watch_again` fails 2 of 2.
- **The hold, and `Behind`.** A held client sends nothing, and the client test fails 2 of 2 without the hold (`client_hold_noop`). A flush due while it's held counts as blocked, so `Behind` drops its windows and owes `resync`, which goes once the reply has. jaw-client's `an_interface_that_falls_behind_is_told_to_watch_again_and_ends_as_a_fresh_one` (`crates/jaw-client/tests/jawd.rs:229`), against a real jawd, passed in the gate.
- **The 16 MiB bound while held.** A held client asks for no read, because its flush returns before reading. The hold is set on the connection's task, which can't be inside a flush at that moment, so no read is under way for it then. Kept reads stay under the shared bound as before.
- **One window per flush keeps a request's wait to one read.** `a_request_waits_for_one_windows_read_at_most_rather_than_the_whole_flush` fails 3 of 3 when a flush does every due window (`flush_all`), and so does probe D.
- **The feed audit's items.** H4: `the_oldest_kept_read_is_let_go_first` fails when the newest is let go first (`newest_first`). H5: T1's test, when any kept read is taken (`any_kept`), fails in 13 s ("the first connection never asked for a read for o3") rather than spinning. H6: `no_kind_of_message_has_a_detail_key_named_like_one_of_its_rows_own_fields` (`convert/tests.rs:575`) fails when a location's detail gets a `kind` (`detail_kind`). One run each.

## Measurements

All on a Mac Studio (M1 Ultra, 20 cores, 128 GiB, macOS 26.6.2), with rustc 1.99.1, rusqlite 0.40.2 and its bundled SQLite. Other builders kept the load at 20 to 68 throughout. The copies, each with `JAW_HOME` and `TMPDIR` in a temp folder of mine:
- `/tmp/jaw-sock-audit` is c768143 as it is: the gate, and the release build;
- `/tmp/jaw-sock-audit-main` is cc7fad7: the release build;
- `/tmp/jaw-sock-audit-mut` is c768143 with my probes: the mutations.

Nothing ran on the owner's folder. Every jawd I ran was `jawd demo` on a folder of mine, which always uses the demo engine (`demo/mod.rs:186`).

**Mutations.** Each was applied in the mutation copy, the tests built, and each test run alone (`--exact`). Load was 21 to 68.

| Mutation | What it does | Result |
|---|---|---|
| `lane_unbounded` | lines read whatever the lane holds | B fails 2/2; the branch's two lane tests pass 2/2 |
| `lane_concurrent` | each lane request on a task of its own | the lane-order test fails 3/3; B and F fail |
| `no_bias` | `select!` unbiased | the one-read test fails 1/5; D fails 3/3; the ordering test passes 2/2 |
| `no_hold_call` | `windows.hold(true)` removed | J fails 3/3; the ordering test passes 3/3; the owed-reply test passes 3/3 |
| `no_hold_call,no_bias` | both | the ordering test fails 1/3 |
| `client_hold_noop` | a held client flushes anyway | the held-client test fails 2/2; J fails 2/2; the ordering test passes 1/1 |
| `held_not_blocked` | a held flush isn't counted as blocked | the held-client test spins (2 runs stopped); with `run_for` fixed, fails 2/2; the owed-reply test passes 2/2 |
| `read_while_waiting` | lines read while a reply waits | the ordering test fails 2/2 |
| `answers_while_waiting` | lane replies taken while one waits | K fails 2/2, keeping 1 of 20; the branch's four socket tests pass 1/1 |
| `windows_wait` | windows unpolled while a reply waits | the owed-reply test fails 2/2 |
| `flush_all` | a flush does every due window | the one-read test fails 3/3; D fails 3/3 |
| `hear_first` | changes heard before a due flush | E fails 2/2 |
| `newest_first` | the newest kept read let go first | H4's test fails 1/1 |
| `any_kept` | any kept read taken | T1's test fails in 13 s |
| `detail_kind` | a location's detail gets `kind` | H6's test fails 1/1 |
| `logout_gap` | 5 ms after the logout's push | G fails: 200 of 200 pushes first |

On the branch as it is, every probe passed 3 of 3 (K 2 of 2): A1, A2, B, D, E, F, G and J.

**Opening a chat during a flood** (the builder's `flood-open.sh`, in a folder of mine). A fresh demo account of 50,000 messages in 300 chats, then `jawd demo --flood 2000 --seconds 30 --in-flight 256 --after 8`, with `jaw bench --keys typing --warm 10 --seconds 25`, in which each cycle sends a message and opens two chats. Release builds, branch and main alternating:

| Run | Load before | Opens | p50 | p99 | Slowest | Keys p99 |
|---|---|---|---|---|---|---|
| branch 1 | 33.9 | 126 | 2.81 ms | 31.76 ms | 34.29 ms | 0.68 ms |
| branch 2 | 26.0 | 126 | 3.51 ms | 13.16 ms | 24.79 ms | 0.93 ms |
| branch 3 | 38.0 | 125 | 3.43 ms | 30.03 ms | 31.70 ms | 0.78 ms |
| main 1 | 20.9 | 85 | 31.53 ms | 103.30 ms | 107.14 ms | 1.15 ms |
| main 2 | 25.5 | 74 | 44.25 ms | 231.37 ms | 252.56 ms | 1.21 ms |
| main 3 | 50.5 | 81 | 24.20 ms | 239.22 ms | 252.97 ms | 1.01 ms |

The builder's 5 to 17 ms against main's 44 to 141 ms was taken at lower load. Here the branch's p99 was 3 to 18 times lower than main's, and its p50 7 to 16 times lower. The 30 ms budget for opening a chat can't be judged at loads past 25. The branch's p99 was over it in 2 of 3 runs, at loads of 34 and 38.

**Opening chats with no flood:** `jaw bench --jawd --keys walk --every 250 --warm 2 --seconds 16`, which runs jaw bench's own jawd at about 1,900 messages a second, in six pairs back to back. p99 on the branch was 27.85, 21.97, 15.27, 25.62, 38.82 and 65.47 ms. On main it was 6.22, 13.74, 18.58, 44.34, 18.91 and 66.96 ms. Loads were 25 to 63, and each run had 45 to 63 opens, so p99 is close to the slowest open. Nothing I can separate from the load.

**Killing jawd with sends in a lane** (`/tmp/jaw-sock-audit-kill.py`). A demo account of 2,000 messages in 20 chats. Each run pipelines 40 keyed `send.text` to one chat, then SIGKILLs the release jawd it started 5, 20, 50, 100 or 200 ms later, and restarts it. Over 6 runs, it kept 8, 3, 14, 1, 1 and 31 messages, each time a prefix of the burst, each once. That was never fewer than the replies read before the kill, counted roughly as 7, 2, 13, 0, 0 and 30. Sending all 40 again with their keys, every one was answered, the kept ones with their own ids, and exactly 40 were there, each once.

**A half-closed connection** (`/tmp/jaw-sock-audit-halfclose.py`). `hello` and 10 `status` requests, then `shutdown(SHUT_WR)`, and reading until jawd closes. On the branch, hello was answered and 0 of 10 status, in 3 of 3 runs. On main, 10 of 10, in 3 of 3.

**The gate at c768143**, run while my mutations ran, at load about 50:
- `cargo fmt --all --check` was clean, and so was `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo test --workspace` passed jaw's 205, jaw-client's 17 and its 3 against a real jawd, jaw-media's 22 and 17 (1 ignored), jaw-proto's fixtures, and jaw-voice's 36 (2 ignored).
- jawd's unit tests: 333 passed, 3 ignored, and 2 failed. `notify::tests::a_burst_in_a_chat_is_one_notification_with_its_newest_message` saw "2 unread" for 3. `work::serial::tests::a_job_waiting_to_run_again_gives_up_its_place_but_not_its_keys_turn` counted 0 for 1. Neither module is in the branch. Each passed 5 of 5 alone at load 23 to 25, so I take them as timing under load.
- Cargo stopped at that failure, so only the library crates' doc tests went unrun.
- The engine's Go tests weren't run, since the branch doesn't touch the engine.

## Verdict

Nothing marked data loss or inconsistency. But five rules the change adds have no test that fails reliably without them, and the lanes are a queue `jawd status` doesn't show.

What must change before landing:
1. **T1 to T5**: adopt probes B, D, J (with a pause that belongs to the feed), E and K, or tests that fail as reliably without their rules.
2. **S1**: the lanes and the waiting replies in `jawd status`, with a test.
3. Then a fresh audit of the new head.

Best done in the same round: H1 (the half-closed connection) and H4 (`run_for`). These may stand as follow-ups: H2, H3, H5, H6, and P1 to P3.

Verdict: changes needed

# Round one (b): c768143, kept as written

# Audit: the socket's lane, and the reply that waits for room

Range: cc7fad7..c768143 in sock-flood (worktree `~/src/jaw-sock`): fffb143 and c768143, 26 October 2026. Auditor: database-expert. Line numbers are at c768143.

**The branch moved during this audit.** At 11:55 the builder committed e3ae66c on top of c768143, answering another audit of this branch. I haven't audited e3ae66c: this note's findings and verdict are for c768143, as I was asked.
- **What e3ae66c appears to address,** by its message, its tests' names and its README diff:
  - must change 1, with `in_lanes`, `lane_most`, `lanes_full` and `replies_waiting` in `status`;
  - T1, T2, T4 and T6 of must change 2, and T3's lane replies;
  - must change 3, in part.
- **What it doesn't mention:** T5 (the logout's push after its answer), T3's finds' replies, the README's `typing` line, and that a reply's `row` goes by its `v`.
- **What it needs:** it touches the data layer (`sock.rs`, `feed/`, `state.rs`), so it needs its own round, by a fresh auditor, checked against this note.

What the branch does. A connection answers its watches (`chats.watch`, `messages.watch`, `watch.move`, `unwatch`), `hello` and `focus` on its own task, and a `find` on a task of its own as before. Every other request goes down a lane of the connection's own: one task that answers them one at a time, in the order sent, holding at most 64 (`sock.rs:30`, `:118-130`, `:171-189`). Lane replies and requests go ahead of the windows' flushes (`biased`, `:105`), and a flush reads one window (`client.rs:1004-1038`). A reply that finds the queue to its interface full waits, one at most (`send`, `sock.rs:149-166`). Meanwhile nothing is read and no other reply or push goes (`if open`, `:113-137`), and the windows are held (`hold`, `client.rs:737`, `:1005-1008`): they take their changes, and a flush due while held counts as blocked, so `Behind` still fires. `chat.opened` changes what the connection reads at once and keeps the open for frecency down the lane (`sock.rs:331-337`). c768143 adds a test of detail keys and has jaw's scripted jawd listen before a test is handed it.

There's no SQL, schema, migration, PRAGMA, index, trigger or plan change. Nothing is acknowledged sooner than before: a lane request is answered only once `answer` has returned, after its synced commit (`send/mod.rs:232-238`). What changes is which requests wait for which, and the order in which replies and pushes reach an interface.

## Summary

I found nothing in the code that loses data or shows an interface something inconsistent, and I checked each property the brief asks about with probes through the real socket, mutations, a kill test and the benches (Measurements). The change can't land yet, for three reasons:

1. **The lane is a new queue that can fill, and `jawd status` doesn't report it.** With the writer held, 64 requests fill a connection's lane, and the connection then reads nothing more, watches included, until one is answered. The rules ask that every queue be bounded and reported.
2. **Six rules the change adds have no test in the branch that fails without them.** The lane's bound, the windows held while a reply waits, at most one waiting reply, no window starving, the logout's push after its answer, and `Behind` firing while a reply waits all pass the branch's suite with the rule removed, or, for the last, fail on main only some of the time. Tests that fail without each are in `/tmp/jaw-sock2-audit-probes.diff`, which applies to c768143.
3. **The README doesn't say plainly what answering watches first means for an interface,** and two of its lines are no longer always true: watches do wait once the lane is full, and `typing` no longer answers at once.

What holds:
- **Watches first can't show a mix.** A watch answered while an earlier send or forward waits on the writer shows the chats as they were, then the request's effect arrives as a push, whole. Over 48 forward rounds, results showed all of the forward or none of it, never part.
- **No window goes back in seq,** and lane requests keep their order in effects and replies.
- **The waiting reply** is at most one, and never lost or reordered. `Behind` fires while it waits, and windows watched again after their `resync` match a fresh query.
- **A disconnect or SIGKILL** leaves each request in the lane done or not done. Across 50 kills, what jawd kept was always a prefix of what was sent, and sends sent again by key changed nothing twice.
- **One window a flush starves no window,** because a due flush goes before the connection hears new changes.
- **The feed audit's items are done:** the reply that waits for room, as it asked, and H4 to H7.
- **The gate passes at c768143:** jawd's 335 tests with 3 ignored, plus its 3 integration tests; jaw's 205; jaw-client's 17 plus 3 against a real jawd; jaw-proto's 1; `cargo fmt --check` and clippy with warnings denied.
- **The flood figure, spot-checked, holds in direction.** During a flood of 2,000 a second, a chat opened in 2.9 to 6.5 ms at p50 on the branch, against main's 12.2 to 35.3 ms. At p99, three branch runs took 18 to 26 ms and two took 67 and 110 ms, against main's 47 to 234 ms. That was at load 51 to 61, too loaded to call the 30 ms budget met (P1).

## Findings

### Data loss: none

- **A reply that waits for room is sent once room comes, and never more than one waits.** `send` keeps a reply only when the queue is full (`sock.rs:155-163`). While one waits, the arms that could make another are off: lane replies (`:113`), finds' replies (`:117`) and lines (`:118`).
  - In probe E2, the queue to the interface was full, and the interface asked for a `find`, twelve `status`es, a watch and a `focus`. Every one got its reply, once, and the lane's came in order, 3 of 3 runs.
  - With any one of the three guards removed, the next reply overwrites the waiting one, and E2 fails 3 of 3 with replies missing. A lost reply is a lost acknowledgement, so these guards matter (T3).
- **Nothing is acknowledged before its commit.** A lane reply is sent only once `answer` returns, and every lane request that writes makes one synced commit first, as on main: `send.text`, `draft.set`, `read`, an action, `chat.opened`'s frecency and a logout. The lane only changes when other requests are read.
- **A disconnect with requests in the lane leaves each done or not done.** The request being answered is carried out and committed. Those waiting behind it are never started, since the lane checks that the connection is still there before each (`:180`).
  - Probe F held the writer, sent two keyed sends and a draft, then dropped the connection. The first send was kept and the second send and the draft weren't, 3 of 3.
  - Sent again by key on a new connection, the first was answered with the message already kept, and there was one row each, in `messages`, the outbox and `send_keys`.
- **A SIGKILL with requests in the lane leaves a prefix of them done.** My kill test ran a real `jawd demo` with the writer held from outside, and sent two keyed sends and a draft. It let the writer go, killed jawd with SIGKILL 0 to 30 ms later (20 rounds) or 0 to 150 ms later (30 rounds), and started it again.
  - Over 50 kills, what jawd kept was always the lane's first n requests: none 30 times, the first send 5, both sends once, all three 14.
  - The draft was never kept without both sends before it. Every key sent again was answered with the message kept and wrote nothing twice.

### Inconsistency: none

- **Answering watches first can't show a mix of before and after.** A watch's result comes from one read transaction (`window::query`), and each lane request commits in one transaction. So a result has all of an earlier request's effect or none of it.
  - A window without the effect gets it as a push, since its connection subscribed to the feed before the watch read (`feed/mod.rs:116-119`, `client.rs:845-850`).
  - Probe A held the writer and sent `send.text`, then `messages.watch` of the same chat. The result came while the send waited, equal to a fresh query of the chat without the message. Once the writer was let go, the sent row came as a push and the window equalled a fresh query: 5 rounds in each of 3 runs.
  - Probe B forwarded a message to three chats and then watched the chat list, the watch staggered 0 to 12 ms after the forward, 24 rounds a run. Of 48 results, 24 showed the forward in none of its chats and 24 in all three, none in part. After each result without it, the first push to show it showed all three.
- **No window goes back in seq.** Watches, moves and flushes still all run on the connection's task, so T2's rule holds as on main. Every probe through the socket checks each window's seq through its result and pushes, and the branch's `Watcher` checks it in every feed test.
- **A send's push can now reach its connection before the send's reply.** In every round of probe A, the window's push of the sent row came ahead of the send's reply, with the same `v`. On main the reply always came first, since the connection did nothing else while it waited.
  - Rows converge by `v`, and jaw reads only `message` from the reply (`jaw/src/wire.rs:433-436`), so jaw draws nothing wrong.
  - An interface that applied a reply's `row` over a newer push would go back, until the next push. So the README must say it (must change 3).
- **Lane order holds.** One task takes the requests from a FIFO channel one at a time, so effects and replies go in the order sent.
  - With the lane's requests answered concurrently instead, the builder's send-then-draft test, my 20 drafts (C) and the lane order in E all fail 3 of 3.
  - The kill test found what was kept to be a prefix of what was sent, every time.
- **The logout's push still comes after its answer, but by an accident of structure.** `logged_out` broadcasts the `connection` push (`sync/mod.rs:153`) before `logout` returns its reply. The lane then hands the reply to the connection's task without awaiting anything in between, and the `biased` select takes a lane reply before a push.
  - In probe H, 200 logouts on a raw socket, the push came after the answer 600 times in 600.
  - Without `biased`, it came first 106 to 119 times in 200. With a 2 ms pause after the broadcast, it came first 200 times in 200.
  - The branch's logout test (`sock.rs:599`) can't see the order, since jaw-client keeps the pushes that arrive while a call waits (`jaw-client/src/lib.rs:53-83`). So the README's promise (`protocol/README.md:229`) is pinned by nothing (T5).
- **The windows are held while a reply waits.** No push goes ahead of a watch's result waiting for room (`client.rs:1005-1008`), and their changes are still taken. A due flush counts as blocked, so `Behind` fires after `behind`, and then `resync` waits for the hold too.
  - In probe R2, the queue was full, a `status` reply waited, and both windows changed for twice `behind`. Both were dropped while the reply waited. Once the interface read again it got the reply and a `resync` for each, and each window, watched again with `since` and `have`, equalled a fresh query, 3 of 3. On main R2 fails 3 of 3: the windows are still watched.
- **The 16 MiB bound on kept reads holds while windows are held.** A held client asks for no read, and `trim` runs on every read kept (`client.rs:282-307`), so holding adds nothing and lets nothing escape the bound.

### Must change before landing

1. **The lane in `jawd status`** (`sock.rs:30`, `:118`).
   - **What happens:** the lane fills. In probe D, with the writer held, 65 drafts and then a watch: after 64 the connection read nothing more, so neither the 65th nor the watch was read. Once the writer was let go, every draft was answered once, in order, and the watch after the first two replies freed room. Nothing was dropped.
   - **What's missing:** `status` has `interfaces` (`connected`, `focused`, `notifications`) and `windows`, and nothing about lanes. A full lane looks from outside like a jawd that has stopped answering.
   - **Fix:** report the requests waiting in lanes, at least the most any one connection holds now and the bound, under `interfaces`, with a test, as `windows` has.
2. **Tests for the rules the change adds that no branch test pins.** For each, the branch's tests pass with the rule removed, or, for T6, fail on main only some of the time (Measurements, Mutations), and the probe named fails without it. All of them are in `/tmp/jaw-sock2-audit-probes.diff`, formatted and clean under clippy.
   - **T1. The lane's bound.** Without `in_lane < LANE_MOST` (`sock.rs:118`), probe D (`audit2_a_full_lane_reads_no_more_and_drops_nothing`) fails 2 of 2: the watch and the focus behind 65 requests are answered while the writer is held.
   - **T2. The windows held while a reply waits.** Without `windows.hold(true)` (`sock.rs:160`), the builder's ordering test passes 5 of 5 and the stalled-reply test 2 of 2, because the bias wins that race nearly always.
     - Probe U (`audit2_a_reply_with_no_room_holds_the_windows_until_it_goes`) is a unit test of `send` with a full queue. It fails 3 of 3 without the call, and 1 of 1 with the client's `hold` doing nothing.
     - U also fails, rather than hanging, when a held flush doesn't count as blocked ("the held windows spun"). In jawd, that mutation would have the connection's task spin a core.
   - **T3. At most one reply waiting.** Without `if open` on lane replies (`:113`), the builder's ordering test passes 3 of 3, and nothing tests finds' replies (`:117`).
     - Probe E2 (`audit2_with_the_queue_full_replies_come_once_each_and_the_lanes_in_order`) fails 3 of 3 without either guard.
     - The guard on lines (`:118`) is pinned: the builder's ordering test fails 3 of 3 without it.
   - **T4. No window starves.** A flush that's due is returned before the connection hears new changes (`client.rs:944-946`, and the order at `:950-956`). So a window flushed can't be marked again until every window due has been flushed, and one window a flush can't starve the rest.
     - With changes heard first, the builder's one-window test still passes.
     - Probe S (`audit2_no_window_starves_when_each_read_takes_longer_than_a_burst`) fails 3 of 3: with reads slower than a burst, three of five windows got one batch in 2 s, while two got about 50.
   - **T5. The logout's push after its answer.** Probe H (`audit2_the_connection_push_after_a_logout_comes_after_its_answer_on_the_raw_socket`) fails without `biased`, and with any await between the broadcast and the reply.
   - **T6. `Behind` while a reply waits, every time.** The builder's `an_interface_owed_a_reply_it_has_no_room_for_is_still_told_to_watch_again` (`feed/tests.rs:2249`) is this change's main test, and here it passed on main in 1 of 3 runs.
     - It asks for `status` at a fixed 1.8 s, and under load the queue hadn't filled by then, so the reply never waited.
     - Probe R2 (`audit2_with_the_queue_full_a_reply_waiting_past_behind_resyncs_every_window_and_watched_again_each_is_fresh`) floods until the feed's read count stops rising, which happens only once the queue is full (`audit2_fill`), and only then asks. It fails on main 3 of 3 and passes on the branch 3 of 3.
     - `a_watch_result_waiting_for_room_comes_before_anything_its_window_sends` rests on the same premise and should fill the queue the same way.
3. **The README** (`protocol/README.md:15`, `:210`). Answering watches first is right for the protocol: it is principle 1, every window still converges to a fresh query, and no interleaving shows part of a request. But an interface written from the README must know what it means.
   - **Name the requests answered at once:** `hello`, `focus`, `chats.watch`, `messages.watch`, `watch.move` and `unwatch`, with `find` on its own as before.
   - **Say they wait once 64 of the connection's other requests are waiting:** jawd then reads nothing more from that connection until one of them is answered.
   - **Say a watch's result can be from before the effect of a request sent ahead of it,** a send, a read position or a deletion, and that the effect then comes as a push. For example, `around: unread` just after `read` anchors on the first unread message before that read.
   - **Say a window's push can come before the reply of the request that caused it,** and that a reply's `row` is an upsert like any other, applied by its `v`.
   - **Fix the `typing` line:** "answers at once" (`:210`) is no longer true, since `typing` waits behind the lane's earlier requests, a send waiting for the disk among them. It touches no disk (`send/typing.rs:72-107`), so it could be answered on the connection's task like `focus`. Otherwise the line should say it answers after the requests sent before it.

### Unpredictable performance (follow-ups; no budget broken that main didn't break more)

- **P1. Opening a chat during a flood, at load 51 to 61** (`/tmp/jaw-sock2-audit-flood.sh`, five runs of each build, alternating).
  - **Branch:** p50 2.9 to 6.5 ms. p99 was 18.1, 25.6 and 26.3 ms in three runs, and 66.9 and 110.5 ms in two.
  - **Main:** p50 12.2 to 35.3 ms; p99 47.0 to 233.7 ms.
  - **Why it can't count as a pass:** both branch runs over 30 ms had the engine's commit-acked p99 over 300 ms. In every run of both builds, a committed message took 57 to 73 ms at p99 to reach the interface, against a 5 ms budget. So this machine, at three times its cores, can't show the 30 ms budget met. The builder's 5.3 to 16.6 ms was at load 15 to 47.
  - **Owed:** a run on a quiet machine (load under 2) to call it met, as the feed audit's P2 is still owed. It isn't a regression: main broke the budget in all five runs.
- **P2. Slow requests hold up the rest of their lane.** `status` waits up to 500 ms for the engine's ping (`state.rs:221-229`). `chat.start` waits on WhatsApp's servers to check a number, and `history.more` and `link.*` wait on the engine. In the lane, each of these holds up the sends, drafts and reads sent after it. On main they held up everything, watches too, so this is better than main, but not yet what principle 1 asks for. Requests that only read, or that wait on WhatsApp, could have a task of their own.
- **P3. The lane is bounded in requests, not bytes.** 64 parsed requests can each be as large as a line, and jawd doesn't enforce the protocol's 16 MiB line (`BufReader::lines` has no limit). That gap is main's, but the lane holds 64 such requests where main held one. The socket is the owner's alone, so it's a follow-up: bound lines as the protocol says, or count the lane in bytes.

### Hygiene (follow-ups; none blocks)

- **H1. A lane whose task dies leaves its connection up.** If `answer` panicked, the lane's requests would stay unanswered, and the connection would end only when it next sent one down the lane (`sock.rs:128`). On main a panic ended the connection at once. Ending the connection when `answers` closes would have the interface reconnect and ask again.
- **H2. The held-client test hangs instead of failing without its rule.** With a held flush not counting as blocked, `a_held_client_sends_nothing_and_is_told_to_watch_again_once_held_for_behind` (`feed/tests.rs:2212`) ran until my 300 s alarm. `ready` returns at once every time, so `run_for`'s `timeout_at` (`:194-204`) never fires. Counting turns or yielding in `run_for` would make it fail, as T1's test now does (the feed audit's H5, below).
- **H3. The two tests the builder found failing only some of the time without their rule.**
  - **The one-window test** (`feed/tests.rs:2385`) failed 5 of 5 here with every due window flushed, as main does, where the builder saw 2 of 5. I found no interleaving in which both windows aren't due when the flush starts, so I take it as pinned.
  - **The ordering test** (`:2284`) can't be made reliable through the socket. Its two rules overlap: the bias alone gives the waiting reply the first room in every interleaving but one microseconds wide, and the hold alone does too, so it fails only with both removed. Pinning each on its own is what T2 (the hold) and T5 (the bias) do.
- **H4. The logout's order could be made structural.** Have `logout` put its reply on the lane's channel before `logged_out` broadcasts the push, so the order no longer depends on nothing awaiting in between. T5's test is required either way; this may stand.
- **H5. `crates/jawd/src/convert/` isn't among `check-data-audit.py`'s paths,** though the converters decide what jaw.db keeps. So c768143 doesn't need the trailer under the script, while fffb143 does.

### The feed audit's items

- **The reply that waits for room** (its "socket's next change"): done as it asked. At most one waits, nothing is read meanwhile, and the windows are still polled, so `Behind` fires. R2 shows it, failing on main 3 of 3.
- **H4, oldest kept read let go first:** pinned. With the newest let go first, `the_oldest_kept_read_is_let_go_first` fails 3 of 3.
- **H5, T1's test:** it now fails instead of spinning. With any kept read taken, it failed 2 of 2 in 11 to 13 s ("the first connection never asked for a read for o3").
- **H6, detail keys:** pinned. With a location's detail given a `kind` key, `no_kind_of_message_has_a_detail_key_named_like_one_of_its_rows_own_fields` fails 1 of 1.
- **H7, the README's table of errors:** done.
- **jaw's scripted jawd:** listens before a test is handed it, and its new test passes. It's outside the data layer, and I didn't examine it further.

### For whoever lands second: which lane each request takes

The rule I'd follow:
- **Down the lane:** a request that writes jaw.db or asks the engine to do something, or that must see the effect of requests sent before it.
- **On the connection's task:** a request that is quick and works on memory, or reads without waiting on the writer or the engine, and needs no order with the lane. Only these.
- **On a task of its own:** a request whose answer can take long, with care that any request cancelling it can't overtake it.

- **`media.fetch`:** down the lane. It makes the copy wanted with one synced commit and answers (`m3-media-in`, `media/mod.rs:332-338`).
- **`media.cancel`:** down the same lane. It's a synced commit and an engine call, and a cancel must never be carried out before a fetch sent ahead of it.
- **`media.save`:** not down the lane. It waits for the file to be downloaded first, which can take as long as the phone is away, then copies and syncs a file of any size (`media/save.rs:1-3`, `:28-46`). Down the lane, it would hold up every send, draft and read the connection sent after it. Give it a task of its own, answered when it's done, as `find` has, but not cancelled by the next one. On main as it is, the media branch's seven lines block the whole connection, watches too, for a save's whole length.
- **`settings.set`:** down the lane, though the settings design puts it "beside `focus`". It answers only once the change is on disk, synced, so on the connection's task it would make watches wait for the disk, which this branch removes. If it broadcasts its `settings` push before its reply, as `logged_out` does, the push can reach the asking connection first. Both carry every setting, so that's harmless, but the README should say which comes first, and a test should pin it.
- **`themes`:** on the connection's task. It answers from palettes built into jaw-proto, with no disk and no order to keep.

## Measurements

All in scratch copies on a Mac Studio (M1 Ultra, 20 cores, macOS 26.6.2), with rustc 1.99.1 and rusqlite 0.40.2 with its bundled SQLite, while other builders and auditors kept the load at 47 to 69. The copies:
- `/tmp/jaw-sock2-audit` is c768143 as it is: the gate, and the branch's release build.
- `/tmp/jaw-sock2-audit-mut` is c768143 with my probes, for the probes and the mutations (`/tmp/jaw-sock2-audit-mutate.py`, `/tmp/jaw-sock2-audit-run.sh`).
- `/tmp/jaw-sock2-audit-main` is cc7fad7 with the branch's tests (but the one that calls `hold`, which main lacks) and my probes, only in test code: main's release build, and the tests on main.

Nothing in `~/src/jaw` or the builder's worktree was changed apart from this file. Nothing touched the owner's folder: every jaw and jawd I ran had `JAW_HOME` and `TMPDIR` in my own folders. No Go engine ran: the benches and the kill test ran `jawd demo` and `jawd bench feed`, which make their own made-up accounts. I left `/tmp/jaw-sock-audit*` alone, since another agent was running the earlier auditor's mutations in them throughout.

**The gate at c768143 (debug):**
- jawd: 335 passed and 3 ignored, and its 3 integration tests, in 190 s at load 59.
- jaw 205, jaw-client 17 and the 3 against a real jawd, and jaw-proto 1, at load 50 to 60.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.

**The branch's new tests on main** (cc7fad7 with the branch's tests), 3 runs each:

| Test | On main |
| --- | --- |
| `an_interface_owed_a_reply_it_has_no_room_for_is_still_told_to_watch_again` | fails 2 of 3, passes 1 (T6) |
| `a_watch_is_answered_while_a_request_sent_before_it_waits_on_the_writer` | fails 3 of 3 |
| `a_request_waits_for_one_windows_read_at_most_rather_than_the_whole_flush` | fails 3 of 3 |
| `a_watch_result_waiting_for_room_comes_before_anything_its_window_sends` | passes 3 of 3 (main's blocking send orders it too) |
| `requests_down_the_lane_take_effect_in_the_order_they_were_sent` | passes 3 of 3 (main does everything in order) |
| `the_oldest_kept_read_is_let_go_first` | passes 3 of 3 (main has the rule; the test is new) |

**My probes on the branch**, 3 runs each unless noted, all passing:
- **A**, a watch while a send waits on the writer, 5 rounds a run: the reply came after a push of its row in every round, with the same `v`.
- **B**, a forward racing a chat list watch, 2 runs of 24 rounds: 24 results without it, 24 with all of it, none with part.
- **C**, 20 drafts in order.
- **D**, a full lane.
- **E** and **E2**, replies waiting for room. E2's order was always the watch first, then the lane's replies, the find's and the focus's among them.
- **F**, a disconnect with requests in the lane.
- **S**, no starvation: each of five windows got 17 to 37 batches in 2 s, the longest wait 90 to 167 ms.
- **R** and **R2**, resync after a reply waited.
- **H**, 200 logouts: no push came before its answer.
- **U**, the hold call.

On main, A and R2 fail 3 of 3 and E 2 of 3. B, C, D, E2, F, S, H and R pass: main does everything in order, and R's reply never waited.

**Mutations**, each one change in the mutation copy, rebuilt and run 3 times unless noted:

| Mutation | Branch tests | Probes |
| --- | --- | --- |
| No `windows.hold(true)` (`sock.rs:160`) | ordering test passes 5 of 5; stalled-reply test 2 of 2 | U fails 3 of 3; E, R and E2 pass |
| A held client flushes (`client.rs:1005-1008` emptied) | held-client test fails 3 of 3; ordering test passes 3 of 3 | U fails 1 of 1 |
| A held flush not counted as blocked (`client.rs:1006`) | held-client test hangs until killed at 300 s (H2) | U fails 1 of 1 ("spun"); R2 passes 1 of 1 |
| The windows wait for room beside the reply (`client.rs:949`) | ordering test passes 3 of 3 | E passes 3 of 3 |
| No `biased` (`sock.rs:105`) | one-window test fails 1 of 3; ordering test passes 3 of 3 | H fails 3 of 3 (106 to 119 pushes first in 200); E passes |
| Lane replies taken while one waits (`sock.rs:113`) | ordering test passes 3 of 3 | E fails 1 of 3; E2 fails 3 of 3 |
| Finds' replies taken while one waits (`sock.rs:117`) | none tests it | E2 fails 3 of 3 |
| Lines read while a reply waits (`sock.rs:118`) | ordering test fails 3 of 3 | E fails 3 of 3; E2 fails 1 of 1 |
| No lane bound (`sock.rs:118`) | watch-while-writer test passes 2 of 2 | D fails 2 of 2 |
| Lane requests answered concurrently (`sock.rs:183`) | send-then-draft test fails 3 of 3 | C and E fail 3 of 3; D fails 1 of 1; F passes 1 of 1 |
| Watches down the lane (`sock.rs:339`) | watch-while-writer and one-window tests fail 2 of 2 | A fails 2 of 2 |
| Every due window in one flush (`client.rs:1031-1036`, as main) | one-window test fails 5 of 5 | S passes 1 of 1 |
| Changes heard before a due flush (`client.rs:944-956`) | one-window test passes 1 of 1 | S fails 3 of 3 (three windows one batch in 2 s) |
| A 2 ms pause after the logout's push (`sync/mod.rs:153`) | logout test passes 1 of 1 | H fails 3 of 3 (200 of 200 pushes first) |
| Newest kept read let go first (`client.rs:296`) | `the_oldest_kept_read_is_let_go_first` fails 3 of 3 | |
| Any kept read taken (`client.rs:859`) | T1's test fails 2 of 2, in 11 to 13 s | |
| A location's detail with a `kind` key (`convert/kinds.rs:249`) | the detail-key test fails 1 of 1 | |

**The kill test** (`/tmp/jaw-sock2-audit-kill.py`, the branch's release jawd, 2,000 messages in 20 chats). Each round held jaw.db's write lock from outside and sent two keyed sends and a draft on one connection. It let the lock go, sent SIGKILL 0 to 30 ms later (20 rounds) or 0 to 150 ms later (30 rounds), and started jawd again.
- **Kept after each kill:** none of the three 30 times, the first send 5, both sends 1, all three 14.
- **Each time,** what was kept was a prefix of what was sent, and the draft never came without both sends.
- **Sent again by key,** each send was answered with the message kept. Each text was there once.

**Opening a chat during a flood** (`/tmp/jaw-sock2-audit-flood.sh`, release, alternating). One made-up account of 50,000 messages in 300 chats was seeded once, and each run took a fresh copy of it. `jawd demo --flood 2000 --seconds 30 --in-flight 256 --after 8` poured messages in, while `jaw bench --keys typing --warm 10 --seconds 25` typed, sending a message and opening two chats each cycle. Opens are chats opened by a key and drawn. Ack is the flood's commit-acked p99, and the last column is how long a committed message took at p99 to reach the interface. Times are milliseconds.

| Run | Load before | Opens | p50 | p99 | Slowest | Ack p99 | To the interface p99 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| main 1 | 51.6 | 115 | 14.6 | 81.8 | 149.8 | 345 | 57 |
| branch 1 | 50.8 | 126 | 3.1 | 26.3 | 29.4 | 207 | 59 |
| main 2 | 54.2 | 94 | 23.1 | 233.7 | 255.7 | 298 | 64 |
| branch 2 | 60.5 | 126 | 4.5 | 25.6 | 38.3 | 113 | 66 |
| main 3 | 60.6 | 77 | 35.3 | 223.2 | 244.2 | 475 | 69 |
| branch 3 | 55.6 | 125 | 3.5 | 110.5 | 113.9 | 314 | 62 |
| branch 4 | 56.1 | 126 | 2.9 | 18.1 | 23.2 | 46 | 57 |
| main 4 | 54.4 | 122 | 12.2 | 47.1 | 48.5 | 52 | 58 |
| branch 5 | 55.5 | 124 | 6.5 | 66.9 | 139.0 | 339 | 73 |
| main 5 | 56.3 | 120 | 13.7 | 47.0 | 58.4 | 261 | 67 |

The branch also drew more opens in the same 25 s (124 to 126, against main's 77 to 122), since fewer of them waited.

**`jaw bench --jawd --keys walk --rate 2000 --seconds 20 --every 300`** (release, alternating), opening each chat while walking the chat list, at load 47 to 56:

| Build | p99 of each run | Median p99 | p50 | Slowest |
| --- | --- | --- | --- | --- |
| branch | 20.9, 11.2, 10.7, 17.9, 15.5, 57.2 | 16.7 | 2.7 to 3.3 | 11.6 to 120.7 |
| main | 36.4, 25.8, 24.5, 15.9, 21.5, 11.5 | 23.0 | 2.8 to 6.9 | 11.5 to 113.2 |

A single open over 110 ms came once in each build, so I take those to be the machine. The builder's 20.0 to 41.1 ms against 23.6 to 45.5 ms is borne out: the walk sends no messages, so the lane helps less there than during a flood.

## Verdict

Nothing marked data loss or inconsistency is in the code. Answering watches first is right for the protocol: every watch shows a consistent state, then the effect of a request sent before it arrives whole as a push, and no window goes back. Lane requests keep their order through disconnects and kills. The reply waiting for room is never lost or reordered and no longer stops `Behind`, and a window watched again after its resync equals a fresh query. But the change can't land yet:

1. The lane is a queue that fills and isn't in `jawd status`: report it, with a test.
2. Six rules it adds have no test that fails without them: T1 to T6 above, with tests in `/tmp/jaw-sock2-audit-probes.diff`.
3. The README must say plainly which requests jump the lane and when they don't, that a watch can come before an earlier request's effect, and that a push can come before a reply, whose `row` goes by `v`. It must also fix or make true its line that `typing` answers at once.

These may stand as follow-ups once those are done:
- P1, a quiet-machine run for the 30 ms budget during a flood;
- P2 and P3;
- H1 to H5;
- the lane each later request takes, as above.

Verdict: changes needed

# Verdict of round two (49d6a89)

Nothing marked data loss or inconsistency remains. Every must-change in both round-one notes is closed by a test that fails without its rule. Everything round one found holding still holds:
- no window's seq goes back;
- effects and replies stay in order;
- kills and disconnects leave a prefix of a lane done, each once;
- sends sent again by key change nothing twice;
- the 16 MiB bound on kept reads holds.

The apart tasks lose, reorder and duplicate no reply, and their cap and error 5 hold. But one rule this round adds isn't pinned, so the branch can't land yet.

What must change before landing:
1. **T7.** Tests that the drain at a half-close waits for finds and long requests, and for a reply waiting for room. Adopt probes H and H2 from `/tmp/jaw-sock3-audit-probes.diff`, or tests that fail as reliably without `self.apart == 0` and `self.waiting.is_none()` in `settled()`.
2. **Then a fresh audit of the new head.** If nothing else changes, it only needs to check those two tests against their mutations.

Best done in the same round: H7. End a connection whose lane's task has died, and have a long request's task reply even if what it runs panics. It's a few lines, and the drain made the failure last until jawd restarts.

To land, once approved:
1. Give every commit that touches the data layer (`1c1f96c`, `a7df8a5`, `49d6a89` and the one with the tests) the trailer `Data-Audit: docs/audits/2026-10-26-sock-flood.md`.
2. Commit this note in the branch, and run `make check-audit`.

These may stand as follow-ups:
- P4, `status`'s cost, which is main's;
- H8 to H11, and the note for the media branch;
- round one's leftovers:
  - (a)'s P1 to P3, and H4 (H9 here);
  - (b)'s P1 to P3, H1 (H7 here), H2 (H9 here) and H5 (H11 here).

Verdict: changes needed

# Verdict of round three (b6841e8)

Nothing marked data loss or inconsistency remains.
- **T7 is closed:** each of the drain's three conditions has a test that fails without it.
- **H7 is done:** a dead lane ends its connection with nothing of it counted, and a long request whose task panics is still answered, each pinned by a test that fails without it.
- **H7's change is safe:** it ends no connection whose lane is healthy, and it drops, duplicates and reorders no reply.
- **The test-only requests can't be reached in a release build.**
- **Everything round two found holding still holds.**

To land:
1. Give the four commits that touch the data layer, `1c1f96c`, `a7df8a5`, `49d6a89` and `b6841e8`, the trailer `Data-Audit: docs/audits/2026-10-26-sock-flood.md`.
2. Commit this note in the branch, and run `make check-audit`.

These may stand as follow-ups:
- H12 and H13: tests for H7's find and for a dying lane's handed-over replies, as my probes P5 and P3;
- H14, the README's sentence on failures inside jawd, and H15, the lanes line's plurals;
- round two's P4, H8 to H11 and its note for the media branch;
- round one's leftovers, as round two lists them.

Verdict: approved
