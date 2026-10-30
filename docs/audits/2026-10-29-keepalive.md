# Audit: the keepalive fix

# Round three: `5230b9f..36cd3ce`

Range: `5230b9f..36cd3ce` in `keepalive-fix` (worktree `~/src/jaw-keepalive`), 30 October 2026, round three. Auditor: database-expert. Line numbers are at `36cd3ce`; paths are under `crates/jawd/src/` unless they say otherwise.

The ten commits, on main at `5230b9f`, which has `folders`:
- `5b12c7d` to `9e84468` are round two's nine, `502b429` to `478b590`, rebased. Seven are unchanged, with the same patch IDs; the other two differ only in context (below).
- `36cd3ce`: T5's test, `TestAnOldTimeoutRacingANewConnectedNeverHasTheLastWord` (`engine/internal/wa/keepalive_test.go:250`). It's my probe of round two, renamed, noting when the new connection began with the branch's `connects`. Its helpers, `words` and `nextWord` (`:62`, `:79`), now also serve the restore race test (`:210`).

No product code changed since round two. `keepalive.go` is as it was at `478b590`, and the lines the branch changes in `engine/internal/wa/engine.go`, `sync/mod.rs` and `engine/failing.rs` are round two's. GitHub's `main` is still `5230b9f`, so there is nothing to rebase.

## In short

Approved. T5 is closed, the rebase brought in only main's context, and nothing in this range loses data or leaves jaw.db inconsistent.
- **T5's test holds the lock `tellConnected` takes.** At the head it passed 3 of 3 under `-race`, in 3 to 4 s. With that lock taken out it failed 6 of 6 runs under `-race`, as `make test` runs it: the old timeout had the last word in 14 to 21 of 200 rounds, with 3 data races each time, and no other test in the `wa` package failed. Without `-race` it failed 3 of 6 runs at a load of 70.
- **Every place that takes told's lock now has a test that fails without it.** The restore race test, on the shared helpers, still fails with `tell`'s lock taken out, and alone: 6 of 6 runs, in 11 to 22 of 200 rounds. Round two's results for `timedOut` and `restored` stand, since neither their code nor their tests changed.
- **The rebase is clean.** `engine.pb.go`'s descriptor was regenerated from the merged `engine.proto`: the lines it adds and removes are the original commit's, line for line, and `make check-generated` passes. The fifth commit differs only by main's `ChatPresence` case beside `Connected` in `say`.
- **The engine's states and `folders`' presence work together.** I ran `jawd demo --fail keepalive` with an interface in front watching the roster.
  - The timeout's `connecting` stopped presence within half a second, with nobody followed in memory or in jaw.db.
  - The restore's `online` started it again, within the 20-a-minute limit.
  - The same held during a catch-up of 10,000, where the restore put back `catching_up`.
  - A message sent in each timeout waited at `('queued', 0)` and went within 0.5 s of the restore, and jaw.db passed every check after each run.
- **One follow-up (O10):** `folders`' presence takes the restore for a new connection. It subscribes to the same people again, within the minute's limit, and starts its count of 200 again. It writes nothing of jaw's own, and it doesn't block.

## What was asked, checked

### T5's test

- **It's sound.** Each round says a first timeout, then races the same run's second timeout against the new connection's `Connected`, each on a goroutine of its own. The new connection began at least 5 ms after the last ping that got through, and the second failure's bound is 2 ms after it.
  - **With the lock, either order leaves `Connected` last.** If `Connected` comes first, the timeout is older than when its connection began, and is dropped. If the timeout comes first, `Connected` follows it.
  - **Without the lock,** the timeout can check the state said last, then have `Connected` noted and said, and then be said itself.
  - **The round's end is sound.** The `Disconnected` that ends each round is handed over after both have returned, so every word of theirs comes before it.
- **It is my probe, as round two had it.** `connects` sets `LastSuccessfulConnect` on the test's goroutine before the two goroutines start, as my probe did, so nothing races on it. The helpers are round two's, with the channel now receive-only.
- **At the head,** with `-race`: T5 passed 3 of 3 alone, in 3 to 4 s, and the whole `wa` package passed in 19 s with no race reported.
- **With `tellConnected` not taking told's lock (G13b),** with `-race`: T5 failed 6 of 6 runs, in 19, 18, 17, 19, 21 and 14 of 200 rounds, with 3 data races each time. The races were `timedOut` reading told while `tellConnected`'s `tellLocked` wrote it. In the whole package, T5 alone failed (18 of 200).
- **Without `-race`** it failed 3 of 6 runs (2, 1 and 3 of 200 rounds) at a load of 70, as the builder found at 60. `make test` runs Go's tests with `-race` (`Makefile:45`), and CI runs `make test`, so there it fails every time.

### The restore race test on the shared helpers

- **Only its helpers changed.** Its own copy of the word collector and its `next` became `words` and `nextWord`; the rest is round two's.
- **With `tell` not taking told's lock (G13a),** with `-race`: it failed 6 of 6 runs, in 13, 11, 14, 15, 22 and 20 of 200 rounds, with 3 data races each time. In the whole package, it alone failed (24 of 200). At the head it passed 3 of 3.
- **The other two places that take the lock,** `timedOut` and `restored`, are as round two left them, with the tests that failed without their locks (G13c, G13d).

### The rebase onto `folders`

- **The range-diff** (`git range-diff 7262a15..478b590 5230b9f..36cd3ce`) shows seven patches unchanged, with the same patch IDs, and two with new context only:
  - `502b429`, now `5b12c7d`: in `engine.pb.go`'s raw descriptor, the lines around the change are now main's `Event.presence = 15` and `Event.typing = 16`. The change itself is the same: `Connection` grows from 396 to 420 bytes, the 24 of `KEEPALIVE_RESTORED = 11`.
  - `3514e04`, now `12d3558`: main's `ChatPresence` case now sits before `Connected` in `say` (`engine/internal/wa/engine.go:317-320`).
  - `36cd3ce` has the patch ID of `893df59`, T5's commit before the rebase, so the builder's "eight of the ten unchanged" is right.
- **The regenerated descriptor.** The lines the first commit adds and removes in `enginepb` and `proto/engine` are the original commit's, all 35 identical. `make check-generated` passes in my clone: `buf` regenerated `enginepb` from the merged proto with nothing to differ, and `sync-protos.py --check` was clean. No later commit touches either.
- **Nothing in `folders` reads the engine's connection states.** On jawd's side it matches only the interface's `online` and `catching_up` (`main.rs:552`, `presence.rs:149`). In the engine, presence and typing go to jawd through `send`, as the app state's versions do, outside told's lock, and its new commands (`subscribePresence`, `userInfo`) don't touch told.
- **On the merged result:**
  - jawd's sync, send, engine and presence tests: 223 passed and 2 ignored, in 80 s, with every keepalive test of the branch's and `folders`' four presence tests among them.
  - The end-to-end keepalive test through jaw (`crates/jaw/tests/failures.rs:285`) passed, in 154 s.

### The engine's states and `folders`' presence, end to end

- **How presence follows the connection.**
  - **Others' presence:** `presence.rs` reads the `connection` pushes (`:141-156`). `online` and `catching_up` mean WhatsApp can be reached, and anything else means it can't. Each change between the two is a new connection to it (`:227-235`): it forgets whom it follows and who's around (`Step::Unknown`). Then, while WhatsApp can be reached, it follows the windows' people again, at most 20 newly a minute and 200 a connection.
  - **Your own:** `send/presence.rs:55-63` tells WhatsApp again each time the connection comes back.
  - So a timeout pauses both, and its restore resumes them, as the builder said.
- **My runs.** I made an account of 20 chats, 20 people and 2,006 messages with `jawd demo --exit`. Then I ran `jawd demo --fail keepalive` on it twice, with a script of mine at the socket. The script:
  - said hello with focus and watched the roster's top 50, 19 people;
  - read `status` and jaw.db, read-only, every half second;
  - sent `SIGUSR1` to the jawd it had started, and a message during each timeout.

  The demo engine answers presence subscriptions, and has one of the people followed come or go every 5 s.
- **Online,** with times from when the script attached:

  | At | What happened |
  | --- | --- |
  | 0.26 s | `online`; the 19 shown all followed, in `status` and in jaw.db |
  | 20.44 s | `SIGUSR1`: `connecting` pushed, reason "1 keepalives failed", with no `done` or `total` |
  | 20.88 s | nobody followed, in memory or in jaw.db, and nobody shown online |
  | 22.03 s | a message sent; its row `('queued', 0)` |
  | 25.7 to 30.4 s | presence heard meanwhile is still written: 2 people online in jaw.db, with nobody followed |
  | 35.44 s | `SIGUSR1`: `online` pushed |
  | 35.92 s | the outbox empty; 1 followed, 18 waiting for the minute |
  | 60.46 s | all 19 followed, a minute after the first 19 |
  | 85.59 s | `SIGTERM`: gone in 0.04 s, exit 0 |

  The only connection pushes were that `connecting` and that `online`.
- **During a catch-up,** with `--catch-up 10000 --in-flight 1 --after 6`:

  | At | What happened |
  | --- | --- |
  | 0.22 s | `connecting`; nobody followed |
  | 6.16 s | `online`, then `catching_up` 0 of 10,000; all 19 followed by 6.25 s |
  | 20.34 s | `SIGUSR1`: `connecting`, the run's only one, with no count; nobody followed by 20.47 s |
  | 22.31 s | a message sent; its row `('queued', 0)` |
  | 35.72 s | `SIGUSR1`: `catching_up` 4,731 of 10,000; by 35.83 s the outbox empty, 1 followed and 18 waiting |
  | 66.65 s | all 19 followed, still catching up |
  | 67.71 s | `catching_up` 10,000 of 10,000, then `online` and the run's one notice, where the summary comes; who's followed unchanged |
  | 135.64 s | `SIGTERM`: gone in 0.04 s, exit 0 |

- **jaw.db after each run,** on a copy with jawd stopped:
  - `quick_check` and `integrity_check` were ok, `foreign_key_check` was empty, and FTS5's `integrity-check` was ok for `messages_fts` and `links_fts`.
  - The inbox, outbox and quarantine were empty, and no `(chat, id)` was there twice.
  - It held 2,007 messages after the first run and 12,008 after the second: the 2,006 of history, the 10,000 of the catch-up and the 2 sent, each sent message one row.
- **What jaw.db said was followed matched `status`** in every one of the 355 samples.

### The builder's gate

- **What its report gives:** check-generated 0, lint 0, build 0, test 2 and check-audit 2.
- **The tests that failed** were main's load-sensitive ones: the feed's long-request test, sync's stop bound, and the socket's full-connection test. The branch touches neither `feed/` nor `sock.rs`, and the stop bound is the flake rounds one and two saw. Each passed once alone in my clone at a load of 60, and the stop bound passed in my sync run as well.
- **check-audit fails as it must** before this note is committed, and names the nine commits below.
- My verdict rests on my own runs, under Measurements.

## Findings

### Data loss

None. No product code changed since round two, and the rebase brought in only main's context. In both runs a message sent during the timeout waited, with no try counted, and went once after the restore.

### Inconsistency

None. The restore writes nothing to jaw.db, as before. The presence rows that `folders` writes at the timeout and the restore are fetched from WhatsApp and kept only until the next start, and they matched what jawd followed throughout. Every check passed after both runs.

### Test gaps

None left. T5 is closed: without the lock `tellConnected` takes, T5 fails, under `-race` every time. Each of the four places that take told's lock now has a test that fails without it.

### Unpredictable performance

None. The range adds a test and no code.

### Other (a follow-up; it doesn't block)

- **O10. `folders`' presence takes a keepalive restore for a new connection.**
  - **What happens.** To presence, a timeout's `connecting` and its restore's `online` are two changes of connection (`presence.rs:227-235`). At each, it forgets whom it follows and who's around. After the restore it subscribes again to the people the windows show, on a connection to WhatsApp that may still have their subscriptions, and starts its count of 200 a connection again.
  - **What it costs.** The 20 a minute holds across the restore, since the minute's subscriptions are kept (`:220`, `:250`). So all but one of the people waited until a minute after their first subscription: 25 and 31 s after the restore in my runs. Until then they show as not known, which is the safe side. Over several restores on one connection, jawd could follow more than 200 different people, if the windows showed that many.
  - **Whose it is.** It's `folders`', meeting the branch's restore. Before the branch, `connecting` lasted until whatsmeow connected again, where starting again is right. None of it is jaw's own data.
  - **Fix, if wanted:** sync tells presence when a connection ends, where `cut_short` runs (`sync/mod.rs:149`), and presence starts its count again only then. Whether to keep the subscriptions across a timeout depends on whether WhatsApp keeps them, which is to be checked on a real account.
- Round two's O5 to O9 and X2 stand as written.

## Measurements

The machine's load average was 50 to 80 throughout.
- **Where:** one clone, `/tmp/jaw-audits/keepalive-3/jaw`, at `36cd3ce`. Go's cache, `TMPDIR` and every jawd folder were in the same folder. Rust built debug with `CARGO_INCREMENTAL=0`, and everything ran under `nice -n 10`.
- **The engines:** the Go engine ran only in its own tests, on temporary `session.db`s with made-up keys that never connect. Every jawd I ran was `jawd demo`, on a folder of the audit's, with no `JAW_` variable passed from my shell.
- **Go:**
  - **The binaries:** one test binary each for the head, G13b and G13a, built with `-race` through `go test -c` and `-overlay`, so no source was edited, and one of G13b without `-race`.
  - **The runs:** each binary ran directly under `timeout`, from the package's folder. Each run's log was kept apart, with a line for it in a summary: 27 runs, 27 lines, none doubled.

  | Variant | Test | Runs | Result |
  | --- | --- | --- | --- |
  | head | T5 | 3 | 3 passed, in 3 to 4 s |
  | head | the restore race | 3 | 3 passed, in 2 to 3 s |
  | head | the whole `wa` package | 1 | passed, in 19 s, no race |
  | G13b, `-race` | T5 | 6 | 6 failed: 19, 18, 17, 19, 21 and 14 of 200; 3 races each |
  | G13b, `-race` | the whole package | 1 | T5 alone failed: 18 of 200 |
  | G13b, no `-race` | T5 | 6 | 3 failed: 2, 1 and 3 of 200; 3 passed |
  | G13a, `-race` | the restore race | 6 | 6 failed: 13, 11, 14, 15, 22 and 20 of 200; 3 races each |
  | G13a, `-race` | the whole package | 1 | the restore race alone failed: 24 of 200 |

  `gofmt -l engine` listed nothing, and `go vet ./...` passed.
- **The rebase:** `git range-diff` as above, `git patch-id --stable` for each commit before and after, and the lines each commit adds and removes in `enginepb` and `proto/engine`, compared. `make check-generated` passed, in 106 s.
- **Rust:**
  - `cargo test -p jawd --bin jawd -- sync:: send:: engine:: presence::`: 223 passed and 2 ignored, in 80 s.
  - The end-to-end keepalive test: passed, in 154 s.
  - The gate's three load-sensitive tests, once each, alone: passed, in 3.6, 4.9 and 2.2 s.
- **End to end:** as above. At the signals, jawd's logs said "the demo engine's failure (keepalive) is on" and "off". At the ends they said "SIGTERM, so it stops", then "stopped in 20 ms" and "stopped in 16 ms".

# Round two: `7262a15..478b590` (kept as written)

Range: `7262a15..478b590` in `keepalive-fix` (worktree `~/src/jaw-keepalive`), 29 October 2026, round two. Auditor: database-expert. Line numbers are at `478b590`; paths are under `crates/jawd/src/` unless they say otherwise.

The nine commits, on main at `7262a15`:
- `502b429`, `3f9be3a` and `4fe4a2e` are round one's `186b67a`, `2326f7d` and `fd7aab7`, rebased with no conflicts. Their patch IDs are unchanged (`084c4cac…`, `24894eb3…`, `afc9d96a…`), as in round one's trial rebase.
- `995dc8a`: T1's tests, and T3's comment at `connected` (`engine/internal/wa/keepalive.go:34-37`).
- `3514e04`: O1. A `Connected` is noted as of when its connection began, whatsmeow's `LastSuccessfulConnect` (`engine/internal/wa/engine.go:316`, `keepalive.go:53-58`).
- `2201795`: T2's and T4's tests.
- `003ef09`: O2. sync notes whether the connection running now has said `Connected` (`sync/mod.rs:43`), and forgets it at every connection's end (`cut_short`, `:150`). A `Connected` after a timeout is taken for a new connection only once this one has said one (`:730-732`), and the connection's own puts back `catching_up` (`:745`).
- `7c7ccdb`: O3. `apply` takes a restore only while sync says `connecting` (`:691`), and the loop pushes nothing for one it doesn't take (`:380`).
- `478b590`: X1. A signal while the demo's phases run stops `jawd demo` (`demo/mod.rs:256-263`), with a test (`crates/jawd/tests/stopping.rs:132`).

Main hasn't moved: GitHub's `main` is still `7262a15`, so there is nothing to rebase.

## In short

Changes needed: one test (T5), and no code. Nothing in this range loses data or leaves jaw.db inconsistent, and round one's findings are done as it asked.
- **Round one's T1, T2 and T4 are closed.** Each of round one's surviving switches now fails a branch test: the lock in `tell`, the ping spacing, the timeout's bound off by one either way, the first state's time kept, the restore's bound off by one, the restore's `total`, and `connecting` counted toward the phone's patience. T3's comment is in, and the login check's switch still survives, as round one said it could.
- **O1, O2 and O3 work as built.** Every one of their switches fails a branch test, but for two that change nothing jawd can reach (O2f, O2g) and T5's. None of them writes to jaw.db: a restore writes nothing, whether sync takes it or not.
- **This range adds no state where jawd says online with no socket, or connecting with a good one,** killed and restarted included. The builder's three notes hold as written, and none blocks (O5 to O7). The one way left to say online with no socket is whatsmeow's, and main's (X2).
- **End to end, the fix holds at the head.** Through a catch-up of 10,000 messages, the timeout pushed `connecting` alone. A message sent then waited at `('queued', 0)` and went 0.12 s after the restore put back `catching_up` 500 of 10,000, and the summary came at the end. A message sent in a second timeout survived a `SIGKILL` and went once after the restart. jaw.db then passed every check, with 12,026 messages, each once.
- **The demo's signal fix holds.** One `SIGTERM` at 2,000 of 10,000 stopped `jawd demo` in 0.09 s, exit 0. jaw.db passed every check, and the restart found the stop noted clean and nothing left to convert.
- **What must change (T5):** the lock `tellConnected` takes is the one part of the order rule that no branch test holds. O1 moved `Connected` out of `tell`, whose lock the race test holds, into a function of its own. With that lock taken out, the branch's tests passed 3 of 3, and my probe, an old connection's timeout racing the new connection's `Connected`, had the timeout as jawd's last word in 6 to 22 of 200 rounds. The fix is to adopt the probe, under "The test to adopt".

## What was asked, checked

### Round one's switches, again

I built the engine's tests once with every switch read from `JAW_MUTANT`, and ran the whole `wa` package with `-race` for each; sync's tests likewise for the Rust switches (Measurements says how).

| Round one's finding | Switch | At the head |
| --- | --- | --- |
| T1 | `tell` without told's lock (G13a) | `TestARestoreRacingADisconnectNeverHasTheLastWord` fails |
| T1 | pings counted 30 s apart (G14) | `TestTheEngineCountsPingsWhatsmeowsLeastIntervalApart` fails |
| T1 | the nth failure counted n − 1 pings after (G18a) | `TestATimeoutIsSaidAfterAStateOnlyOnceItsFailureCantHaveComeBeforeIt` and the late-`Connected` test fail |
| T1 | counted n + 1 pings after (G18b) | the same bound test and `TestATimeoutIsJudgedAgainstTheStateSaidLast` fail |
| T1 | the first state's time kept (G16) | `TestATimeoutIsJudgedAgainstTheStateSaidLast` fails |
| T1 | the restore's next ping n after the last that got through, not n + 1 (G17) | `TestARestoreIsSaidOnlyOnceTheNextPingCouldHaveGotThrough` fails |
| T2 | the restore's `total` from everything waiting (R13) | `pings_that_get_through_again_during_a_catch_up_put_back_the_messages_waiting_as_its_total_not_all_that_waits` fails |
| T3 | `connected` without the login check (G12) | survives, as round one said it could; the comment is at `keepalive.go:34-37` |
| T4 | `connecting` counted toward the phone's patience (P1) | `the_phones_patience_doesnt_count_while_a_keepalive_timeout_has_jawd_connecting_and_starts_again_whole_once_the_pings_get_through` fails |

- **The tests are round one's,** adopted with the changes the builder describes, and I read each. The race test now ends each round on a `Connected` rather than a 50 ms wait for more words. That is sound: the `Connected` is handed over after both racing words have returned, so every word of theirs comes before it.
- **The other two places that take told's lock** are held too. `timedOut` without it (G13c) fails eight branch tests under `-race`, and `restored` without it (G13d) fails the race test and four more. `tellConnected` is the exception (T5).
- **Round one's sync switches whose code this round changed fail as before.** No catch-up put back (R2) fails 3 tests. The account written for a restore (R3) fails the nothing-written test. A timeout ending a catch-up (R4) fails 4, and the restore ending one (R5) fails 2. No `Connected` after a timeout taken as new (R6) fails the reconnect test. The catch-up put back with no counts (R7) fails 3, and `4fe4a2e`'s rule out (R10) fails the restore's catch-up test.

### O1: a timeout after a late `Connected`, judged by when its connection began

- **What whatsmeow does.** In the pinned `v0.0.0-20261015105041-df33a485cc46`, `unlockedConnect` starts the keepalive loop once the noise handshake is done (`client.go:598`). `handleConnectSuccess` writes `LastSuccessfulConnect = time.Now()` when `<success>` comes, about a round trip later, and only then starts the goroutine that sends the prekey and passive IQs and dispatches `Connected` (`connectionevents.go:164`, `:187-206`). `time.Now()` carries a monotonic reading, as the loop's `lastSuccess` does, so both bounds still compare Go's monotonic readings, and a change of the clock can't move them.
- **An older connection's timeouts are still dropped.** The nth failure since a ping that got through comes at least n pings after it. An older connection's failures all came before it ended, so before this connection began. So after the new `Connected`, however late it's said, an older connection's timeout has `LastSuccess + n × 20 s` before `told.at` and is dropped. That's what keeps jawd from saying connecting with a good socket and no restore to come: the new connection's loop has had no failure to restore.
- **This connection's own timeouts are said** once the loop's start plus n × 20 s passes `<success>`. That is the first failure whenever `<success>` comes within 20 s of the handshake. O1's case, with `Connected` up to two 75 s IQs late, is gone. If `<success>` itself came more than 20 s late, that connection's first failures would still be dropped until n × 20 s passed it (O8).
- **What it trades.** The time noted for `Connected` is no longer later than every word older than it. A timeout of the same connection from before its late `Connected`, handled after it, is now said (the builder's second note, below).
- **The read of `LastSuccessfulConnect`** isn't synchronised (the builder's third note, below).
- **Switches.** `Connected` timed when it's said (O1a) fails the branch's late-`Connected` test. With no time (O1b), that test and two older ones fail. Half a ping before its connection began (O1c) fails the bound test. `tellConnected` without told's lock (G13b) passes every branch test (T5).

### O2: a connection's own late `Connected` keeps its catch-up

- **How it works.** `said_connected` is set by `Connected` and cleared by `cut_short`, which every connection's end calls: an ending word (`:733-736`), the engine stopping (`restarting`, `:492`) and jawd's own logout (`logged_out`, `:234`). A `Connected` while sync says `connecting` is a new connection only if this one has said `Connected` (`:730-732`). Otherwise it's the connection's own, and it puts back `catching_up` with how far the catch-up has got, as a restore does (`:744-751`).
- **Whether jawd can send is unchanged.** `catching_up` and `online` both count as connected (`send/presence.rs:18-20`). So O2 changes only whether a catch-up goes on, never whether sends go.
- **Switches.** Four fail the branch's O2 test: any `Connected` after a timeout taken for a new connection (O2a), the catch-up put back only by a restore (O2b), `cut_short` not forgetting the `Connected` (O2c), and the flag set before it's read (O2e). Never noting it (O2d) fails the reconnect test.
- **Two switches survive, and can stand,** since the next connection clears the flag before it matters:
  - `restarting` keeping the flag (O2f): an engine started again says `CONNECTING` first (`engine/internal/wa/engine.go:150`), whose `cut_short` clears it.
  - `logged_out` keeping it (O2g): sync then says `logged_out`. Only a timeout of the device linked again, before its own `Connected`, could make that `Connected` look like a reconnect, and it would matter only if that connection were catching up.
- **What it can't recognise** is the builder's first note, below.

### O3: a restore taken only while sync says connecting

- **It's sound.** sync says `connecting` from a timeout until its next word, and the engine says a restore only after its timeout, in order on the link. So the only restores sync now drops are ones heard after it changed state on its own: a preview that came during the timeout (`catching_up`, `:453`), jawd's own logout (`logged_out`), or any later word if the engine's order failed. Dropping one then says nothing wrong, since what sync says is already the later state.
- **Nothing is written either way.** A dropped restore returns before the account is touched (`:691-693`), and the loop goes on past the push and the account write (`:380-382`). The restore sync takes writes nothing to jaw.db either, as round one found. The branch's test that holds jaw.db's writer still passes, and R3 still fails it.
- **My probe of a preview during a timeout** pushed `online`, `connecting`, then `catching_up 0 of 10` at the preview, nothing for the restore, and `online` and the summary at `OfflineDone`.
- **Switches.** Each of these fails the branch's O3 test: a restore taken in any state (O3a), one dropped but pushed anyway (O3b), and one dropped only after a logout (O3c).
- **What it doesn't cover** is a timeout and its restore both heard after jawd's own logout (O9).

### The builder's three notes

1. **"O2 still can't recognise a forced reconnect whose new connection also says its `Connected` late."** True, and it doesn't block.
   - **What happens.** A forced reconnect says no `Disconnected`, so the flag is still the old connection's, and the new connection's late `Connected` after its own timeout is taken for a reconnect. It cuts that connection's catch-up short.
   - **My probe.** The old connection said `Connected`, a preview of 10, a message and a timeout. The new one said a preview of 3, a message, a timeout, a message, its late `Connected`, a message and `OfflineDone`. jawd pushed `online`, `catching_up 0 of 10`, `connecting`, `catching_up 0 of 3`, `connecting`, `online` and `online`, with no summary, and the last message came as live. jaw.db held all 4 messages, each once.
   - **What it costs.** What jawd says of the connection is right throughout: the new connection is up when it says `online`. What's lost is that catch-up's summary and its held order, as round one's head lost them for every late `Connected`.
   - **How likely.** It needs 3 minutes of failed pings, then a new connection whose first ping fails before its IQs are answered. The fix is the builder's: the engine says which connection each word is from (O5).
2. **"With O1, a timeout from before a late `Connected` of the same connection that is handled after it is now said, so jawd says connecting until the restore comes, the safe direction."** True, and it is the safe direction.
   - **My probe** (`TestAuditAnOwnTimeoutHandledAfterALateConnectedIsFollowedByItsRestore`): after the late `Connected`, the timeout is said, then the restore at the next ping that gets through is said too.
   - **How long.** jawd says connecting until that ping, less than 40 s (the next ping within 30 s of the failure, and its answer within 10 s). Sends wait meanwhile, with no try counted.
   - **When it would last longer.** Only if the timeout were also handled after its own run's restore, which the late `Connected` had the engine drop. Then nothing would follow it, and jawd would say connecting with a good socket until the connection next changed. My `TestAuditAnOwnTimeoutHandledAfterItsRunsRestoreIsSaid` shows the engine would say it; round one's rule dropped it.
   - **Why that's unreachable.** The timeout's goroutine would have to be held back by at least a ping, 20 s, behind two later ones. Nothing between whatsmeow's dispatch and told's lock can do that. `dispatchEvent` takes the handlers' read lock, which only adding or removing a handler waits on, and the engine adds one only to a new client. Once a goroutine has waited 1 ms for told's lock, Go's mutex hands the lock over in the order its waiters came (O6).
3. **"whatsmeow doesn't lock `LastSuccessfulConnect`."** True, and harmless.
   - **For a connection's own `Connected`,** the write comes before the goroutine that dispatches it starts, so the read is ordered after it.
   - **Only the next connection's write can race with the read,** when the socket drops during the IQs and the reconnect's `<success>` is handled before the delayed handler reads the field. whatsmeow's IQs wait up to 5 s for that reconnect (`retryFrame`, `request.go:185-202`). Then `told.at` is the next connection's start. That drops more of the dead connection's timeouts and none of the next one's, whose failures come at least a ping after its loop's start, a round trip before its `<success>`.
   - **A torn read** of the `time.Time` would give one of the two monotonic readings: both values have one, and `Before` compares only those.
   - **The race detector** would report the race in a test that raced the two writes, and no test does (O7).

### Killed and restarted

- **Every state these rules keep is in memory:** the engine's `told`, and sync's `said_connected` and catch-up. A `SIGKILL` of jawd ends the engine too, once its link closes. The next start begins from `starting`, `connecting` for a linked account, and then the new engine's first word. Nothing of the builder's three notes survives a restart.
- **End to end,** a message sent during a second timeout sat at `('queued', 0)` through a `SIGKILL`, and the restarted jawd sent it, once (below).

### The demo's signal fix (`478b590`)

- **What changed.** The `select!` between the phases and the signals now records which ended it (`demo/mod.rs:256-262`), and only phases that finish wait for a second signal (`:263`). The stop is jawd's own (`main.rs:572`): the socket removed, the engine disconnected and stopped, the clean stop noted, the WAL emptied.
- **Nothing is half-written.** Dropping the phases' future cancels only what the demo would do next. Every write it caused went through the store's transactions, each inbox commit comes before its ack, and the clean stop is noted only once the engine has stopped. That was so before the fix too; the fix removes only the wait for a second signal.
- **My run.** On an account of 20 chats and 2,000 messages, I ran `jawd demo --catch-up 10000 --in-flight 1 --after 4` and sent one `SIGTERM` once the catch-up had stored 2,000.
  - It stopped in 0.09 s with exit 0. Its log said "SIGTERM, so it stops" and "stopped in 28 ms", and never "serving until Ctrl-C".
  - jaw.db then passed `quick_check`, `integrity_check` and FTS5's `integrity-check`, with `foreign_key_check` empty. It held 4,013 messages with no `(chat, id)` twice, and the inbox, outbox and quarantine were empty.
  - jawd again on the folder didn't say the last run stopped uncleanly. It said `online` and held the same 4,013.
- **The test.** `jawd_demo_stops_on_one_sigterm_while_its_phases_still_run` passes at the head. With the signal taken for the phases' end (X1), it fails: jawd was still running 6.1 s after `SIGTERM`.

### End to end

I ran the head's jawd as round one's timeline had it: `jawd demo --fail keepalive --catch-up 10000 --in-flight 1 --after 4`, on a demo account of 20 chats, 20 people and 2,000 messages of history, driven through the socket by a script of mine.

| At | What happened |
| --- | --- |
| 5.17 s | hello: `connecting` |
| 13.37 s | `SIGUSR1` once the catch-up had passed 300: pushed `connecting`, with the timeout's reason and no `done` or `total` |
| 13.40 s | a message sent: `pending` |
| 16.49 s | 3 s later: `connecting`, outbox 1, its row `('queued', 0)`; the one push since the timeout is that `connecting` |
| 16.49 s | `SIGUSR1`: pushed `catching_up`, 500 of 10,000 |
| 16.61 s | the outbox empty, 0.12 s after the restore |
| 112.58 s | `catching_up` 9,900 and 10,000, then `online`; notice "While you were away: 10000 messages in 20 chats" |
| 113.69 s | a second timeout; a message sent then, its row `('queued', 0)` |
| 113.72 s | `SIGKILL` |
| later | the restarted jawd: `online`, linked, the outbox and inbox empty; one `SIGTERM`: gone in 0.09 s, its log saying the last run didn't stop cleanly, then "stopped in 26 ms" |

My script tripped on the killed jawd's socket file, which a `SIGKILL` leaves, so I read the restarted jawd's state a few minutes after it started, when the script had already stopped. Then, with jawd stopped, on a copy of jaw.db: `quick_check`, `integrity_check` and FTS5's `integrity-check` were ok, and `foreign_key_check` was empty. The outbox, inbox and quarantine were empty, and jaw.db held 12,026 messages with no `(chat, id)` twice. Each sent message was one row, read.

### The builder's gate

Its logs were deleted with its scratch, so the gate is its report's word. My verdict rests on these runs of mine at `478b590`:
- Go, `go test -race -count=1 ./...`: ok (`jaw-engine` 2.0 s, `link` 3.0 s, `wa` 47.2 s).
- jawd's sync, send and engine tests: 218 passed and 2 ignored, in 111 s.
- The end-to-end keepalive test (`crates/jaw/tests/failures.rs`): passed, in 185 s.
- `tests/stopping.rs`: the new demo test passed. Main's `jawd_whose_engine_is_missing_stops_on_sigterm_within_its_bound` failed once, with jawd still starting 1.5 s in at a load of 68. It then passed 3 of 3 alone, and the suite 4 of 4. The range doesn't touch it.
- `gofmt -l engine`, `go vet ./...` and `cargo fmt --all --check`: clean. I didn't run clippy.
- jawd's whole unit suite, once: 637 passed and 12 ignored, none failed, in 373 s.

## Findings

### Data loss

None. The inbox path, its acks and the outbox are unchanged. The words this round adds or drops need no ack and write nothing. A message sent during a timeout survived a `SIGKILL` and went once.

### Inconsistency

None. Nothing this round adds writes to jaw.db. The account and its JIDs are written for a `Connected` only, as before. My runs found every message once, the queues empty and every check ok.

### Test gaps

- **T5. The lock `tellConnected` takes has no test that fails without it (must change; one test).**
  - **Where:** `engine/internal/wa/keepalive.go:55-56`. O1 moved `Connected` out of `tell`, whose lock the branch's race test holds, into `tellConnected`, which takes the lock itself.
  - **What survives:** with `tellConnected` saying `Connected` without the lock, the whole `wa` package, my probes left out, passed 3 of 3 with `-race`, and no race was reported.
  - **What it prevents:** a timeout can check that it's newer than the state said last, and then have a `Connected` noted and said before the timeout is sent. That leaves the timeout as jawd's last word. If the timeout is an older connection's, nothing restores it, since the new connection's loop has no failure to restore. jawd then says connecting with a good socket until the connection next changes, and sends wait: the bug this branch fixes.
  - **Why it blocks:** in whatsmeow it needs the old connection's last timeout held back past a whole reconnect, so the lock is a guard rather than a race anyone has seen. But it's a part of the rule that round one's T1 asked to have each part tested, and the three other places that take the lock are.
  - **Fix:** no code. Adopt `TestAuditAnOldTimeoutRacingANewConnectedNeverHasTheLastWord`, under "The test to adopt".
    - It passed 3 of 3 at the head with `-race`.
    - Under the switch it failed in 22 of 200 rounds with `-race` (3 races reported), and in 6, 9 and 7 of 200 without it.
    - It also fails with `timedOut` without the lock, with the first state's time kept, and with `Connected` given no time.
- My other probes needn't be adopted: they pin what's already tested, or what can't be guarded. They're under "Measurements".

### Unpredictable performance

None. There is no SQL, no write and no queue. `tellConnected` reads one field, and `apply` one flag.

### Other (follow-ups; none blocks)

- **O5. O2 can't tell a forced reconnect's own late `Connected`** (the builder's first note). It costs that catch-up its summary and held order, and nothing else. Fix: the engine numbers each connection and says the number with each word. sync then takes a `Connected` as new when its number is, and both could drop an older connection's words by number rather than by time.
- **O6. A connection's own timeout, handled after its run's restore, would be said with nothing to follow it** (the edge of the builder's second note). It's unreachable in practice, as above. Fix, if wanted: `restored` notes when a restore was heard, said or not, and `timedOut` drops a timeout whose nth failure could have come before it.
- **O7. `LastSuccessfulConnect` is read without a lock** (the builder's third note). It's harmless, as above; there's nothing to do unless a test comes to race it.
- **O8. If `<success>` comes more than 20 s after the handshake,** its connection's first failures are still dropped after its `Connected`, until n × 20 s passes it. The bound is now how late `<success>` is, where before O1 it was the IQs, up to 150 s. The cost is O1's.
- **O9. A timeout and its restore, both heard after jawd's own logout, say `connecting` then `online`, and link the account in memory again.**
  - **My probe** found that. jaw.db's account stays empty, so a restart starts unlinked.
  - **Whose it is.** The timeout's part is main's, as round one's O3 said for every state, and O3 covers a restore heard alone.
  - **Why it's unreachable.** The restore comes at least a ping after its timeout, so both would have to wait 20 s in sync's queue behind the logout's synced commit.
  - **Fix:** take the logout's end through sync's loop, so words queued before it are applied before it.

### Outside this range

- **X2. whatsmeow says `Connected` for a connection whose post-connect IQs gave up, and the engine passes it on.**
  - **Why it happens.** When the socket drops during those IQs, `onDisconnect` answers every waiter with a stream end (`client.go:614`) and dispatches `Disconnected` (`:617`). Each IQ then waits up to 5 s for a reconnect (`retryFrame`, `request.go:199`). With none by then, `handleConnectSuccess`'s goroutine logs the failures and dispatches `Connected` anyway (`connectionevents.go:187-206`).
  - **What it does.** By then the `Disconnected` has been said, so jawd says `online` with no socket until the reconnect's own `Connected`. That lasts as long as whatsmeow fails to reconnect, since a failed reconnect dispatches nothing (`autoReconnect`, `client.go:643-676`). Sends are tried meanwhile, and a forced reconnect during the IQs does the same.
  - **Whose it is.** It's main's: before the branch, `Connected` was said the same way.
  - **Fix:** the engine says `Connected` only while `connected()` holds, asked before told's lock as `restored` asks it. It's worth the engine's next change.

## Measurements

The machine's load average was 57 to 70 throughout.
- **Where:** one clone, `/tmp/jaw-audits/keepalive-2/jaw`, at `478b590`. Debug builds used `CARGO_INCREMENTAL=0` under `nice -n 10`, with `TMPDIR`, `GOCACHE` and every jawd home in the same folder.
- **The engines:** the Go engine ran only in its own tests, on temporary `session.db`s with made-up keys that never connect. Every jawd I ran was `jawd demo`, with `JAW_ENGINE` naming nothing.

- **At the head:** as under "The builder's gate".
- **Mutation tests:** one build for each language read every switch from `JAW_MUTANT`, so no switch needed a build of its own. Go ran the whole `wa` package with `-race -count=1`, my probes beside the branch's tests. Rust ran sync's tests for each switch, sync's, send's and engine's for no switch, and `tests/stopping.rs`'s demo test for X1. With no switch, Go passed, and Rust passed 221: the head's 218 and my 3 probes.

  | Switch (Go) | Result |
  | --- | --- |
  | G12 `connected` without the login check | survives (T3, as round one said it could) |
  | G13a `tell` without the lock | the branch's race test and my timeout-against-`Disconnected` race fail, with 6 races reported |
  | G13b `tellConnected` without the lock | only my old-timeout race fails: 22 of 200 rounds, 3 races (T5) |
  | G13c `timedOut` without the lock | 8 branch tests and both my races fail, with 26 races reported |
  | G13d `restored` without the lock | the branch's race test, 4 more of its tests and one of my probes fail, with 15 races reported |
  | G14 pings counted 30 s apart | the spacing test fails |
  | G16 the first state's time kept | the state-time test and both my races fail |
  | G17 the restore's next ping n after | the restore-bound test fails |
  | G18a the nth failure counted n − 1 pings after | the bound test, the late-`Connected` test and my two note-2 probes fail |
  | G18b counted n + 1 pings after | the bound and state-time tests fail |
  | O1a `Connected` timed when it's said | the late-`Connected` test and my two note-2 probes fail |
  | O1b `Connected` with no time | the late-`Connected` test, `TestAKeepalivesWordOlder…`, the bound test and my old-timeout race fail |
  | O1c `Connected` half a ping before its connection began | the bound test fails |

  | Switch (Rust) | Result |
  | --- | --- |
  | R13 the restore's `total` from everything waiting | T2's test fails |
  | P1 `connecting` counted toward the phone's patience | T4's test fails |
  | O2a any `Connected` after a timeout taken as new | the O2 test fails |
  | O2b the catch-up put back only by a restore | the O2 test fails |
  | O2c `cut_short` not forgetting the `Connected` | the O2 test fails |
  | O2d the `Connected` never noted | the reconnect test fails |
  | O2e the flag set before it's read | the O2 test fails |
  | O2f `restarting` keeping the flag | survives (can stand) |
  | O2g `logged_out` keeping the flag | survives (can stand) |
  | O3a a restore taken in any state | the O3 test and my preview probe fail |
  | O3b a dropped restore pushed anyway | the same fail |
  | O3c a restore dropped only after a logout | the same fail |
  | R2 no catch-up put back | 3 fail: the O2 test and both restore catch-up tests |
  | R3 the restore writes the account | the nothing-written test fails |
  | R4 a timeout ends a catch-up | 4 fail, the failure-states audit's T3 test among them |
  | R5 the restore ends a catch-up | both restore catch-up tests fail |
  | R6 no `Connected` after a timeout taken as new | the reconnect test fails |
  | R7 the catch-up put back with no counts | 3 fail |
  | R10 `4fe4a2e`'s rule out | the restore's catch-up test fails |
  | X1 the signal taken for the phases' end | the demo test fails: still running 6.1 s after `SIGTERM` |

  - **One load flake:** `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean`, which no switch reaches, failed beside R6's test. It's the flake round one and the builder saw.
- **My probes,** each passing at the head:
  - **Go:** the old timeout racing a new `Connected` (T5's test) and a timeout racing a `Disconnected` passed 3 of 3 with `-race`. The two of the builder's second note, the timeout followed by its restore and the one said after its run's restore, both fail under round one's rule (O1a), which drops the timeout.
  - **Rust:** the forced reconnect's late `Connected` (the first note), a restore after a preview during its timeout, and a timeout and its restore after jawd's own logout (O9), each as above.
- **End to end:** as above.

## The test to adopt

**T5, Go,** in `engine/internal/wa/keepalive_test.go`, as I ran it against the branch's helpers. Rename it as the builder likes; the `Audit` prefix only kept mine apart. It passes at the head with `-race`, 3 of 3, in about 3 s under a load of 60, and fails with `tellConnected` not taking told's lock.

```go
// words collects the connection states the engine says to jawd.
func words(jawd *link.Link) chan enginepb.Connection_State {
	said := make(chan enginepb.Connection_State, 64)
	go func() {
		for {
			frame, err := jawd.Recv()
			if err != nil {
				return
			}
			if c := frame.GetEvent().GetConnection(); c != nil {
				said <- c.GetState()
			}
		}
	}()
	return said
}

func nextWord(t *testing.T, said chan enginepb.Connection_State) enginepb.Connection_State {
	t.Helper()
	select {
	case state := <-said:
		return state
	case <-time.After(10 * time.Second):
		t.Fatal("the engine said nothing more")
		return 0
	}
}

// The old connection's last timeout, dispatched as whatsmeow connects again (its forced reconnect
// says no Disconnected), races the new connection's Connected: whichever is said first, jawd's last
// word is the Connected, or it says connecting with a good socket and no restore to come.
func TestAuditAnOldTimeoutRacingANewConnectedNeverHasTheLastWord(t *testing.T) {
	const apart = time.Millisecond
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	said := words(jawd)
	timedOutLast := 0
	const rounds = 200
	for round := range rounds {
		lastPing := time.Now()
		go e.handleFrom(client, failedPings(1, lastPing))
		if state := nextWord(t, said); state != enginepb.Connection_KEEPALIVE_TIMEOUT {
			t.Fatalf("round %d: the first timeout said %v", round, state)
		}
		time.Sleep(5 * apart)
		// The new connection began after the old one's second failure.
		client.LastSuccessfulConnect = time.Now()
		start := make(chan struct{})
		var both sync.WaitGroup
		both.Add(2)
		go func() { defer both.Done(); <-start; e.handleFrom(client, failedPings(2, lastPing)) }()
		go func() { defer both.Done(); <-start; e.handleFrom(client, &events.Connected{}) }()
		close(start)
		both.Wait()
		go e.handleFrom(client, &events.Disconnected{})
		var last enginepb.Connection_State
		for state := nextWord(t, said); state != enginepb.Connection_DISCONNECTED; state = nextWord(t, said) {
			last = state
		}
		if last != enginepb.Connection_CONNECTED {
			timedOutLast++
		}
		time.Sleep(2 * apart)
	}
	if timedOutLast > 0 {
		t.Fatalf("in %d of %d rounds jawd's last word was the old timeout, after the new Connected", timedOutLast, rounds)
	}
}
```

It needs `sync` among the test file's imports, which `keepalive_test.go` has.

# Round one: `907246c..fd7aab7` (kept as written)

Range: `907246c..fd7aab7` in `keepalive-fix` (worktree `~/src/jaw-keepalive`), 29 October 2026, round one. Auditor: database-expert. Line numbers are at `fd7aab7`; paths are under `crates/jawd/src/` unless they say otherwise.

The three commits:
- `186b67a`: after a keepalive timeout jawd says online again once whatsmeow's pings get through, and waiting sends go. A new `KEEPALIVE_RESTORED = 11` in `engine.proto`'s `Connection.State`; the engine says every connection state under one lock (`engine/internal/wa/keepalive.go`); sync maps the restore to online, or back to `catching_up` during a catch-up.
- `2326f7d`: the failing demo engine's `--fail keepalive`, with a send test and an end-to-end test.
- `fd7aab7`: while a keepalive timeout has a catch-up's connection say connecting, its progress isn't pushed as connecting with a count.

The bug it fixes is the failure-states audit's (`docs/audits/2026-10-28-failure-states.md`, "Outside this range", and its T3).

## In short

Changes needed: tests, and no code. Nothing in this range loses data or leaves jaw.db inconsistent, and the fix does what it says.
- **The fix holds end to end.** At the rebased head, I ran `jawd demo --fail keepalive` through the socket during a catch-up of 10,000 messages:
  - while the pings failed, `connecting` was pushed alone, with no count, and a message sent then waited at `('queued', 0)`;
  - the restore put back `catching_up 796 of 10000`, and the message went 0.10 s later, once;
  - the summary still came at the catch-up's end;
  - a message sent in a second timeout survived a `SIGKILL` and went once after the restart. jaw.db then held all 12,009 messages, each once, with `integrity_check` ok.
- **The engine's two time rules are sound against the pinned whatsmeow.** Both only ever drop a word. `connected()` also drops the restores whatsmeow dispatches when a socket drops, or a stream error comes, while a ping waits.
- **The restore writes nothing to jaw.db.** During a catch-up it puts back `catching_up` with `done` and `total`. T3 of the failure-states audit is closed. The phone's patience starts again whole after a restore.
- **The trial rebase onto `origin/main` (`7262a15`) has no conflicts.** The patches are unchanged, and the sync, send, engine and Go tests pass on the result.
- **What must change (T1, T2):** seven of my switches pass the branch's tests. These are the lock that orders the engine's words, the ping spacing the engine takes from whatsmeow, the timeout's bound off by one either way, the restore's bound off by one, the time of the state said last, and the restore's `total`. The builder was asked for each rule, switched off, to fail a test. Each of the tests under "The tests to adopt" passes at the head and fails under its switch. Two more switches survive and can stand (T3, T4).
- **Follow-ups (O1, O2):** both come from whatsmeow saying `Connected` late, after its IQs. Timeouts after a late `Connected` are dropped, and its connection's own catch-up ends at it. Neither touches jaw.db.

## The builder's gate

The builder's report was lost in a stall at about 11:15. Its logs were in `/tmp/jaw-keepalive/gate/` (deleted once this note was written, as the brief asked), from `gate.sh`, which ran each step with `nice -n 5 make <step>` in the worktree and wrote its exit code to `codes.txt`.

- **What they say:**
  - `codes.txt`: check-generated 0 (10 s), lint 0 (353 s), build 0 (57 s), test 0 (922 s), check-audit 2 (1 s).
  - `build.log` stamps the engine `fd7aab7`, and the worktree's reflog has `fd7aab7` committed at 11:03:24 and nothing after it. The logs were written from 11:06 to 11:28.
  - `lint.log`: `go vet`, `cargo fmt --check` and `clippy -D warnings`, all clean.
  - `check-generated.log`: `buf generate`, `sync-protos.py --check` and `git diff --exit-code` on `enginepb`, all clean.
  - `test.log`: Go's three packages ok, all three "(cached)"; jaw 324 passed and 2 ignored; `failures.rs` 10 of 10, the new keepalive test among them; jawd 613 passed and 12 ignored; the other suites passed. All six new tests are in it and passed.
  - `check-audit.log` fails as it must before this audit: all three commits touch the data layer with no `Data-Audit` trailer.
- **An earlier gate**, `/tmp/jaw-keepalive/gate1/`, ran at `2326f7d`, before `fd7aab7`. Its tests failed in two of main's load-sensitive tests, `send::tests::a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left` and `sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`. A `--no-fail-fast` rerun failed the sock test again, `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce`, and jaw's first-frame test, which main has since reworked (`48c635e`, `8136143`, `43323b7`). The branch touches none of them.
- **What I couldn't confirm:**
  - That the final gate ran on an unmodified tree. `scripts/version` runs `git describe --always` without `--dirty`, so the stamp can't say. The worktree is clean now.
  - That Go's tests ran at all in that gate. They were cached, so they are an earlier run's results; Go's cache is keyed on the sources, so they are the results for these sources, but not a fresh run. I ran them again uncached (below).
  - The builder's mutation results. `mutants-go.txt` and `mutants-rs.txt` list 13 Go and 10 Rust switches, each failing a test, but they were run at about 09:30 to 10:00, before `fd7aab7` and before the 10:03 rebase. Its `--fail keepalive` probe (`probe.out`, 10:55) is the run that found `fd7aab7`'s bug: `connecting done=400 total=None` was pushed during the timeout. I ran my own mutants instead (below).
  - The probe's jawd didn't stop within 15 s of `SIGTERM` and was killed (`jawd stopped: -9`). That is `jawd demo`'s own signal handling, outside this range (X1).

## The engine's rules, against whatsmeow

I read them against the pinned whatsmeow (`v0.0.0-20261015105041-df33a485cc46`): `keepalive.go`, `client.go` (`unlockedConnect`, `onDisconnect`, `Disconnect`, `autoReconnect`), `connectionevents.go` (`handleConnectSuccess`, `handleStreamError`) and `request.go` (`clearResponseWaiters`).

- **The pings' spacing holds.** `keepAliveLoop` waits a random 20 to 30 s before each ping and sets `lastSuccess` only after one gets through. So the nth failure since `LastSuccess` comes at least n × 20 s after it, and the next ping after n failures gets through at least (n + 1) × 20 s after it. Both bounds the engine uses are right, and both compare Go's monotonic readings, so a change of the clock can't move them.
- **Each event is dispatched on a goroutine of its own:** `KeepAliveTimeout`, `KeepAliveRestored`, `Disconnected` (from `onDisconnect`) and `Connected` (from a goroutine in `handleConnectSuccess`). That is the reason the order needs guarding, as the builder says.
- **The forced reconnect says no `Disconnected`.** After 3 minutes of failed pings, the loop calls `Disconnect()`, which marks the disconnect expected, so `onDisconnect` says nothing. Then `autoReconnect` connects again. So jawd stays `connecting` from the last timeout to the new `Connected`, and the branch's rule that a `Connected` after a timeout is a new connection is needed for it.
- **whatsmeow dispatches restores that aren't real, and `connected()` drops them.** `sendKeepAlive` takes any answer on the ping's waiter as success, and two places answer every waiter with a node that isn't a pong:
  - `onDisconnect` and `unlockedDisconnect` answer with a stream end. Both set `cli.socket = nil` first, under `socketLock`, so `IsConnected()` is already false.
  - `handleStreamError` answers with the stream error. It sets `isLoggedIn` false first, so `IsLoggedIn()` is false.

  So after timeouts, a socket that drops or a stream error while a ping is waiting makes whatsmeow dispatch `KeepAliveRestored`. The engine's `connected()`, `IsConnected() && IsLoggedIn()`, drops both cases. The login check matters for the stream error, and no test holds it (T3).
- **The lock orders every word.** Every connection state goes through `tell` (`connection`, `loggedOut`, `TemporaryBan`, and `Run`'s outdated `session.db`), and nothing else sends one. Under the lock there is only `link.Send`, never a call into whatsmeow, so the lock can't deadlock with `socketLock`. `restored` asks `connected()` before taking the lock, which avoids waiting the up-to-5 s that `Disconnect` holds `socketLock`. Without the lock, a restore racing a `Disconnected` can have the last word, and none of the branch's tests notices (T1).
- **What the time rules can't see.** Both rules only drop words, never pass on an older one, so the order is safe. They can drop a true timeout, though, when the state said last was told well after the loop's `LastSuccess`. `Connected` can be: the keepalive loop starts at the handshake, and `handleConnectSuccess` dispatches `Connected` only after the prekey-count IQ, perhaps an upload, and the passive IQ, each with whatsmeow's 75 s default. On a slow network, timeouts after such a late `Connected` are dropped until n × 20 s since the handshake passes the time `Connected` was told. Until then jawd says online while pings fail, at most until the forced reconnect at 3 minutes (O1).

## jawd's side

- **The mapping.** `apply` (`sync/mod.rs:693`) maps `KEEPALIVE_RESTORED` to online. Its match on `pb::connection::State` has no `_` arm, so a state added later can't be left out silently. The only other matches are `matches!` lists: the catch-up rule (`:718`) and the account write (`:376`, `:380`). Neither the interface protocol's `ConnectionState` nor its fixtures change, so jaw, the Mac app and `jaw-proto` see nothing new.
- **Nothing is written to jaw.db.** sync writes the account only for a `Connected` with a JID, or an `Unlinked` or `LoggedOut` (`:375-387`). It remembers the JIDs and starts the app-state refresh only for `Connected` (`:409-424`). The restore is none of these. It does update the account jawd holds in memory from its JID and push name (`:701-710`), as the timeout already did; nothing of that reaches jaw.db. The restore test holds jaw.db's writer with `BEGIN IMMEDIATE`, so a write would hold sync's loop: the store retries a busy lock as transient (`store/write.rs:500`), the third push wouldn't come, and the rows are compared after.
- **During a catch-up**, the restore puts back `catching_up`, with `done` the messages stored so far and `total` the preview's message count (`:724-730`), as `OfflinePreview` set them (`:445-447`). With `fd7aab7`, `caught` goes on counting during a timeout but neither sets `done` nor pushes, so `connecting` never carries a count (`:638-642`). A timeout and a restore don't end a catch-up, and a `Connected` after a timeout does (`:716-721`), for whatsmeow's forced reconnect; but see O2.
- **Sends.** `presence::connected` counts online, `catching_up` and `phone_offline` as able to send (`send/presence.rs:18-20`), so both of the restore's states let sends go. `attempt` waits on the `inner.connected` watch with `wait_for` (`send/outbox.rs:399-403`), so a send that saw "not connected" can't miss the change. A send held by the timeout counts no try while it waits, since `attempt` returns before reading the row; the branch's send test checks `('queued', 0)` while waiting, one send once restored, and one counted try for a later refusal.
- **The phone's patience.** `online_since` is reset whenever the connection isn't online or `phone_offline` (`sync/mod.rs:323-327`). So a timeout stops the patience, and the restore starts it again whole. My probe asks the phone, lets 0.5 s pass online, times out for 2 s, and restores, with a patience of 1 s. `phone_offline` came 1.002 s and 1.003 s after the restore in two runs, and nothing came before it (T4). During a catch-up, the restore's `catching_up` isn't online, so the patience doesn't count, as in any catch-up.
- **T3 is closed.** `a_keepalive_timeout_during_a_catch_up_holds_it_until_whatsapp_says_it_is_done_and_its_summary_comes_after` is the test the failure-states audit asked for. It fails with a timeout ending the catch-up (my R4).

## The trial rebase onto main

`origin/main` is now `7262a15`, ten commits past `dec0fe1`: tests and the terminal's loop probe only (`crates/jaw`, `crates/jaw-client/tests`, `notify/tests.rs`, and a test helper in `work/serial.rs`). I rebased onto `7262a15`, in a branch of my clone.

- **No conflicts.** Since `907246c`, main and the branch both change only `send/tests.rs` and `protocol/README.md`, and git merged both on its own.
- **The patches are unchanged.** `git range-diff` shows all three as `=`, and each commit's `git patch-id --stable` is the same before and after: `084c4cac…`, `24894eb3…` and `afc9d96a…`. The rebased head was `76ba919` in my clone. Nothing was applied twice: three commits, three patch IDs.
- **Where the two shared files landed:**
  - `protocol/README.md`: the keepalive paragraph still comes right after the `phone_offline` paragraph, at the end of "Other pushes" and before "Voice (proposed)". Main's additions (scheduled messages, M2's deletions) are elsewhere in the file.
  - `send/tests.rs`: the keepalive send test still comes right after `a_message_sent_while_the_engine_restarts_waits_with_no_attempt_counted_and_goes_once_it_is_back`. Main's m2-deleted tests are elsewhere.
- **On the rebased result** (debug, `CARGO_INCREMENTAL=0`, under `nice`, at a load of 35 to 47): jawd's sync, send and engine tests, 214 passed and 2 ignored, where the head has 194 (main's m2-deleted adds 20 send tests); Go's `go test -race -count=1 ./...` ok (`wa` 8.4 s); the end-to-end keepalive test passed in 44 s.
- **Nothing for the lander to resolve.** The branch adds no migration, so migration 18 (main's) is untouched and no number moves. The only code of main's in the branch's path is `send/outbox.rs`'s `attempt`, which m2-deleted changed only with a test-only pause and a new `Enqueued::Deleted` arm. The gating the keepalive send test relies on, waiting on `inner.connected` before a try, is unchanged.

## Findings

### Data loss

None. The inbox path, its acks and the outbox are unchanged. The restore and the failing engine's words need no ack and write nothing. A message sent during a timeout survived a `SIGKILL` and went once after the restart (Measurements).

### Inconsistency

None. With or without the restore, jaw.db's rows are the same: the branch's test compares them, and my end-to-end run found every message once and nothing in the inbox or quarantine. O1 and O2 below are about what jawd says of the connection and when, not about jaw.db.

### Test gaps

- **T1. The order guard's parts have no test that fails without them (must change).**
  - **What survives the branch's tests.** I switched each part off in turn and ran the engine's tests with `-race`. Each of these passed:
    - the lock: `tell` says a state without taking it (`engine/internal/wa/keepalive.go:39`);
    - the ping spacing: `newEngine` counts pings whatsmeow's longest interval apart, 30 s, rather than its shortest (`engine.go:74`);
    - the timeout's bound: the nth failure counted (n − 1) or (n + 1) pings after the last ping that got through (`keepalive.go:61`);
    - the time of the state said last: `tellLocked` notes the state but keeps the first state's time (`keepalive.go:46-48`);
    - the restore's bound: the next ping counted n pings after the last that got through, rather than n + 1 (`keepalive.go:86`).
  - **What each part prevents.**
    - **The lock:** a restore that checked the state said last before a racing `Disconnected` was said gets said after it, so jawd says online with no socket until whatsmeow's reconnect. My test of it failed without the lock in 11 and 13 of 200 rounds, with 3 data races reported each time.
    - **The spacing:** at 0, every timeout of a connection's first run after its `Connected` would be dropped, since `LastSuccess` stays at the loop's start, which is before the `Connected`. jawd would say online while pings fail, until the forced reconnect.
    - **The bounds and the time:** each, off by one ping or keeping the wrong time, lets an older word through after a later state, or drops a true one.
  - **Why it blocks.** These parts are the rule this branch adds so that jawd never says online with no socket, nor connecting with a good one. That decides when a send is tried, which the outbox records: a try counted while the socket is gone marks the message as one that may have gone, and a later refusal then holds it a day rather than failing it. The builder was asked for each rule, switched off, to fail a test.
  - **Fix:** no code, five Go tests. Adopt those under "The tests to adopt", or ones like them. Each passes at the head and fails under its switch (Measurements).
- **T2. The restore's `total` is the preview's message count, untested (must change; trivial).**
  - Putting back the preview's total of everything waiting (`c.total`) rather than its messages (`c.messages`, `sync/mod.rs:726-728`) passes every test. The restore test's preview, `offline_preview(10)`, gives both the same number.
  - It is what interfaces are shown, not jaw.db, but it is a rule this range adds.
  - **Fix:** a preview whose total and message count differ. The variant under "The tests to adopt" uses 25 and 10; it passes at the head 3 of 3 and fails 3 of 3 with `c.total` put back.
- **T3. The login half of `connected()` has no test (can stand).**
  - It drops the restore whatsmeow dispatches when a stream error answers a waiting ping. By then `handleStreamError` has set `isLoggedIn` false, but the socket can still be open.
  - The tests replace `connected` with their own, so `IsConnected()` alone passes them. A test would need whatsmeow's socket, which is unexported, and the reading of whatsmeow is recorded above.
  - **Fix, cheap:** a comment at `connected` (`keepalive.go:33-36`) naming the stream-error case, so a later edit doesn't take the login check for redundant.
- **T4. The phone's patience across a timeout and a restore has no test (can stand; main's rule).**
  - The rule holds: `phone_offline` came a whole patience after the restore. But counting a keepalive timeout's `connecting` toward the patience passes the branch's suite. The failure-states tests break the connection with `Disconnected`, which says offline, never with a timeout.
  - The rule is `online_since`'s, which this range doesn't change, so it can stand.
  - **Fix:** adopt my probe, under "The tests to adopt". It fails with `connecting` counted.

### Unpredictable performance

None.
- There is no SQL, no new write, and no new queue.
- Under `told`'s lock there is only `link.Send`, which every word took before through the link's own lock. A connection word now also costs one more lock and a comparison or two.
- `restored` asks whatsmeow whether it's connected before taking the lock, so no word waits on whatsmeow's socket lock.

### Other (follow-ups; none blocks)

- **O1. After a late `Connected`, true timeouts are dropped, and jawd says online while pings fail, at most until whatsmeow's forced reconnect.**
  - **Why:** `timedOut` (`keepalive.go:61`) compares a timeout with when the state said last was told. whatsmeow starts the keepalive loop at the handshake (`client.go:598`), but dispatches `Connected` only after the prekey-count IQ, perhaps an upload, and the passive IQ, each with a 75 s default (`connectionevents.go:187-205`).
  - **Example:** with `Connected` told 75 s after the handshake, a failing run's timeouts after it are dropped until n × 20 s passes 75 s, which is the fourth failure. With pings timing out that's at least 120 s after the handshake, so jawd says online for at least 45 s while they fail. With `Connected` later still (two IQs timing out, 150 s), the forced reconnect at 3 minutes comes first. Before this branch every timeout was said.
  - **Cost:** sends are tried while pings fail. A try counted on a socket that's gone marks a message as one that may have gone, and a later refusal holds it a day rather than failing it. Nothing is lost or sent twice, since every try carries its one ID.
  - **Fix:** judge a timeout after `Connected` by when that connection began, not by when whatsmeow said so. For instance, the engine could take told's time for `Connected` from `client.LastSuccessfulConnect`. `handleConnectSuccess` writes it before starting the goroutine that dispatches `Connected`, so the handler reads it in order. The loop's start comes before it by a round trip, and every older connection's failures come before it too.
- **O2. A late `Connected` on its own connection, after its first keepalive timeout, ends that connection's catch-up.**
  - **What happens:** whatsmeow can say, on one connection, the offline preview, then a keepalive timeout, then `Connected` (O1's cause). The branch's new rule (`sync/mod.rs:716`) takes any `Connected` after a timeout for a new connection, and cuts the catch-up short.
  - **My probe:** with that order, jawd pushed `connecting`, `catching_up 0 of 10`, `connecting`, `online` at the `Connected`, and `online` at `OfflineDone`. No summary came, and the catch-up counted 2 of the 3 messages; the third was taken as live.
  - **Cost:** as the failure-states audit's T3 had it. The summary never comes, the rest is notified one by one, and the chat list stops holding its order. Before this branch, the same order kept the catch-up but said `connecting`, holding sends, until `OfflineDone`.
  - **Fix:** tell the two apart. sync could take a `Connected` after a timeout as a new connection only once this connection has already said `Connected` since it last ended. whatsmeow's forced reconnect follows its connection's `Connected`, and a late one is its connection's first. Or the engine could say which connection a word is from. In both cases the restore's rule of putting back `catching_up` applies to the late one.
- **O3. sync takes a restore whatever it says at the time.**
  - It relies on the engine alone for the order. A true restore can only follow a timeout that sync turned into `connecting`. So taking a restore only while sync says `connecting` costs nothing.
  - It would also cover a word handled after jawd's own logout. `logout` (`sock.rs:802-815`) sets `logged_out` outside sync's loop, so a connection word queued before the logout's answer can be applied after it. That is main's, for every state, and unreachable in practice, since sync drains its queue long before `logged_out`'s synced commit.
- **O4. A new engine with an older jawd.**
  - The link's version stays 1, as it has through every additive change since `40c6551`, so they still shake hands. An older jawd reads 11 as unspecified: it says offline and cuts a catch-up short, and sends wait as they did before the fix.
  - `make build` builds both, and the hello says to.

### Outside this range

- **X1. One signal doesn't stop `jawd demo` during its phases** (`demo/mod.rs:255-266`).
  - A signal that comes while the phases run ends them as if they had finished. Then, without `--exit`, the demo prints "serving until Ctrl-C" and waits for another.
  - That is why the builder's probe's jawd was still running 15 s after `SIGTERM` and was killed. A demo started with nothing to do stops at once: mine stopped in 60 ms.
  - It affects demo folders only.

## Measurements

The machine's load average was 21 to 71 throughout. Everything was in one clone, `/tmp/jaw-audits/keepalive/jaw`: debug builds with `CARGO_INCREMENTAL=0` under `nice -n 10`, with `TMPDIR`, `GOCACHE` and every jawd home in the same folder. The Go engine ran only in its own tests, on a temporary `session.db` with made-up keys that never connects. Every jawd I ran was `jawd demo`, which has only the demo engine, with `JAW_ENGINE` naming nothing.

- **At the head (`fd7aab7`):**
  - Go, `go test -race -count=1 ./...`: ok (`jaw-engine` 1.5 s, `link` 1.8 s, `wa` 6.9 s).
  - jawd's sync, send and engine tests: 194 passed and 2 ignored, in 47 s.
  - The end-to-end keepalive test (`crates/jaw/tests/failures.rs`): passed in 33 s.
- **On the rebased result (`76ba919` in my clone):** the same 214 passed and 2 ignored, in 55 s; Go ok (`wa` 8.4 s); the end-to-end keepalive test in 44 s; `make check-generated` ok.
- **Mutation tests**, on the rebased result. For each language, one build read the switch from `JAW_MUTANT`. Go ran the `wa` package with `-race`, and Rust jawd's sync, send and engine tests, without my probes.

  | Switch (Go) | Result |
  | --- | --- |
  | G1 the restore not passed on | 4 fail: the running client's restore, the replaced client's, with no connection, and sooner than the next ping |
  | G2 a restore from any client | `TestARestoreFromAClientReplacedAfterALogout…` fails |
  | G3 no check that the client is connected | `TestARestoreWhileTheClientIsntConnected…` fails |
  | G4 a restore after any state | `TestAKeepalivesWordOlderThanALaterConnectionState…` and the replaced client's fail |
  | G5 a restore before the next ping | `TestARestoreSoonerThanTheNextPing…` fails |
  | G6 a timeout never older than a state | `TestAKeepalivesWordOlder…` fails |
  | G7 a timeout never older than a timeout | `TestATimeoutOlderThanTheTimeoutSaidLast…` fails |
  | G7b compared by time after a timeout | the same fails |
  | G8 the logout not noted | the replaced client's test fails |
  | G9 the ban not noted | `TestAKeepalivesWordOlder…` fails |
  | G10 the other states not noted | the same fails |
  | G11 a timeout's ping not noted | 2 fail |
  | G12 `connected` without the login check | survives (T3) |
  | G13 `tell` without the lock | survives (T1); my lock test fails |
  | G14 pings counted 30 s apart | survives (T1); my spacing test fails |
  | G16 the first state's time kept | survives (T1); my "judged against the state said last" fails |
  | G17 the restore's next ping n after, not n + 1 | survives (T1); my restore-bound test fails |
  | G18a the nth failure counted n − 1 pings after | survives (T1); my timeout-bound test fails |
  | G18b counted n + 1 pings after | survives (T1); my timeout-bound and state-time tests fail |

  | Switch (Rust) | Result |
  | --- | --- |
  | R1 the restore not online | the send test and both restore tests fail |
  | R2 the restore puts no catch-up back | `pings_that_get_through_again_during_a_catch_up…` fails |
  | R3 the restore writes the account | `pings_that_get_through_again_after_a_keepalive_timeout_say_online_with_nothing_written…` fails |
  | R4 a timeout ends a catch-up | T3's test and the restore's catch-up test fail |
  | R5 the restore ends a catch-up | the restore's catch-up test fails |
  | R6 a `Connected` after a timeout isn't new | `a_connection_after_a_keepalive_timeout_during_a_catch_up_is_a_new_one…` fails |
  | R7 the catch-up back with no counts | the restore's catch-up test fails |
  | R7b the catch-up back with `done` 0 | the same fails |
  | R8a the failing engine's timeout unsaid | the failing engine's test and the send test fail |
  | R8b the failing engine's restore unsaid | the same fail |
  | R9 `connecting` counted as connected | the send test fails |
  | R10 `fd7aab7`'s rule out | the restore's catch-up test fails |
  | R11 nothing counted during a timeout | T3's test and the restore's catch-up test fail |
  | R12 the restore says `catching_up` with no catch-up | the nothing-written test and the send test fail |
  | R13 the restore's `total` from everything waiting | survives (T2); my variant fails, 3 of 3 |
  | P1 `connecting` counted toward the phone's patience | survives (T4); my patience probe fails |

  - **Load flakes:** three tests that the switches don't touch failed once each: `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean` (also in the run with no switch), `send::tests::a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left`, and `send::poll::tests::a_vote_in_a_poll_deleted_for_everyone_before_it_goes_is_withdrawn`. Alone, each passed 3 of 3 with its switch off and with it on.
- **My tests and probes:**
  - **The lock test**, on one engine, 200 rounds: at the head 3 of 3 passed, in 11 to 12 s. Without the lock 2 of 2 failed, in 13 and 11 of 200 rounds, each with 3 data races reported. An earlier version, with an engine for each run, failed in 7 and 12 of 60 without the lock and passed 2 of 2 at the head.
  - **The spacing test** passes at the head, and fails with pings counted 30 s apart.
  - **The bounds and state-time tests** passed 2 of 2 at the head, and each failed under its switch.
  - **The patience probe:** `phone_offline` came 1.0017 s and 1.0031 s after the restore, with a patience of 1 s. It fails with P1, "not offline at once".
  - **The late-`Connected` probe:** jawd pushed `connecting`, `catching_up 0 of 10`, `connecting`, `online` (at the `Connected`) and `online` (at `OfflineDone`). No summary came, and the catch-up counted 2 of the 3 messages.
  - The whole `wa` package, with my tests beside the branch's, passes at the head.
- **End to end**, with the rebased head's jawd: `jawd demo --fail keepalive --catch-up 10000 --in-flight 1 --after 4`, on a demo account of 20 chats and 2,000 messages of history, driven through the socket by a script of mine:

  | At | What happened |
  | --- | --- |
  | 7.56 s | `SIGUSR1`: the keepalive times out, the catch-up at 300 of 10,000 |
  | 7.62 s | a message sent: `pending` |
  | 10.67 s | 3 s later: `connecting`, outbox 1, its row `('queued', 0)`. The one push since the timeout is `connecting`, with no `done` or `total` |
  | 10.68 s | `SIGUSR1`: the pings get through again |
  | 10.73 s | pushed `catching_up`, 796 of 10,000 |
  | 10.77 s | the outbox empty, 0.10 s after the restore |
  | 74.40 s | `catching_up` 9,900 and 10,000, then `online`; notice "While you were away: 10000 messages in 20 chats" |
  | 76.10 s | a second timeout; a message sent then, `('queued', 0)` |
  | 76.17 s | `SIGKILL` |
  | 78.57 s | jawd again on the folder: `online`, the outbox empty |
  | 81.08 s | one `SIGTERM`: stopped, exit 0 |

  Then, with jawd stopped: `quick_check` and `integrity_check` ok and `foreign_key_check` empty; outbox, inbox and quarantine empty; each sent message one row, read; no `(chat, id)` twice. jaw.db held 12,009 messages: the 2,007 of history, the 10,000 of the catch-up, and the 2 sent.

## The tests to adopt

As I ran them, against the branch's own helpers. Rename them as the builder likes; the `Audit` and `audit_` prefixes only kept mine apart from the branch's.

**T1, Go:** `engine/internal/wa/keepalive_audit_test.go`. All five pass at the head. Each fails under its switch, as the table shows. The lock test takes about 12 s under a load of 60.

```go
package wa

// The audit's probes (database-expert, 29 October 2026), not the branch's.

import (
	"sync"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types/events"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// A restore and a Disconnected dispatched at once, each on a goroutine of its own as whatsmeow
// does: whatever their order, jawd's last word is the Disconnected.
func TestAuditARestoreRacingADisconnectNeverHasTheLastWord(t *testing.T) {
	const apart = time.Millisecond
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	said := make(chan enginepb.Connection_State, 64)
	go func() {
		for {
			frame, err := jawd.Recv()
			if err != nil {
				return
			}
			if c := frame.GetEvent().GetConnection(); c != nil {
				said <- c.GetState()
			}
		}
	}()
	next := func(within time.Duration) (enginepb.Connection_State, bool) {
		select {
		case state := <-said:
			return state, true
		case <-time.After(within):
			return 0, false
		}
	}
	restoredLast := 0
	for round := 0; round < 200; round++ {
		go e.handleFrom(client, failedPings(1, time.Now()))
		if state, _ := next(10 * time.Second); state != enginepb.Connection_KEEPALIVE_TIMEOUT {
			t.Fatalf("round %d: the timeout said %v", round, state)
		}
		time.Sleep(3 * apart)
		start := make(chan struct{})
		var both sync.WaitGroup
		both.Add(2)
		go func() { defer both.Done(); <-start; e.handleFrom(client, &events.KeepAliveRestored{}) }()
		go func() { defer both.Done(); <-start; e.handleFrom(client, &events.Disconnected{}) }()
		close(start)
		both.Wait()
		var last enginepb.Connection_State
		for state, ok := next(10 * time.Second); ok; state, ok = next(50 * time.Millisecond) {
			last = state
		}
		if last != enginepb.Connection_DISCONNECTED {
			restoredLast++
		}
		go e.handleFrom(client, &events.Connected{})
		if state, _ := next(10 * time.Second); state != enginepb.Connection_CONNECTED {
			t.Fatalf("round %d: the Connected said %v", round, state)
		}
	}
	if restoredLast > 0 {
		t.Fatalf("in %d of 200 rounds jawd's last word was the restore, after the Disconnected", restoredLast)
	}
}

// The engine whatsmeow runs in keeps pings whatsmeow's least interval apart, which the timeout's and
// the restore's bounds count on.
func TestAuditTheEngineCountsPingsWhatsmeowsLeastIntervalApart(t *testing.T) {
	e, _, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	if e.told.apart != whatsmeow.KeepAliveIntervalMin {
		t.Fatalf("pings counted %v apart, where whatsmeow's least is %v", e.told.apart, whatsmeow.KeepAliveIntervalMin)
	}
}

// A timeout is said after a state only when its failure can't have come before it: the nth failure
// since the last ping that got through comes at least n pings after that ping.
func TestAuditATimeoutIsSaidAfterAStateOnlyOnceItsFailureCantHaveComeBefore(t *testing.T) {
	const apart = time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	lastPing := time.Now().Add(-1500 * time.Millisecond)
	saysNothing(t, e, client, failedPings(1, lastPing), "a first failure that could have come before the Connected")
	go e.handleFrom(client, failedPings(2, lastPing))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
}

// A timeout is judged against the state said last, not an earlier one.
func TestAuditATimeoutIsJudgedAgainstTheStateSaidLast(t *testing.T) {
	const apart = time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	time.Sleep(2 * apart)
	go e.handleFrom(client, &events.Disconnected{})
	saidNext(t, jawd, enginepb.Connection_DISCONNECTED)
	lastPing := time.Now().Add(-1500 * time.Millisecond)
	saysNothing(t, e, client, failedPings(1, lastPing), "a timeout that could have come before the Disconnected")
}

// After n failures since the last ping that got through, a restore can't come sooner than the
// (n+1)th ping after it.
func TestAuditARestoreIsSaidOnlyOnceTheNextPingCouldHaveGotThrough(t *testing.T) {
	const apart = time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	lastPing := time.Now()
	go e.handleFrom(client, failedPings(1, lastPing))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
	time.Sleep(time.Until(lastPing.Add(1500 * time.Millisecond)))
	saysNothing(t, e, client, &events.KeepAliveRestored{}, "a restore between the first and second pings after the last that got through")
	time.Sleep(time.Until(lastPing.Add(2500 * time.Millisecond)))
	go e.handleFrom(client, &events.KeepAliveRestored{})
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_RESTORED)
}
```

**T2, Rust,** in `sync/tests.rs`: the variant of the restore's catch-up test, with a preview whose total and message count differ. It passes at the head, and fails with the preview's total put back.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn audit_pings_that_get_through_again_put_back_the_preview_s_message_count_as_the_total() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (_live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    let preview = pb::OfflinePreview { total: 25, messages: 10, receipts: 15, ..Default::default() };
    let preview = pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflinePreview(preview)) };
    tx.send(crate::engine::Incoming { event: preview, ack: None }).await.unwrap();
    tx.send(arriving(ANA, "3EB0AWAY1")).await.unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    tx.send(connection_event(State::KeepaliveRestored)).await.unwrap();
    let pushed: Vec<_> = next_told(&mut told, 4).await.iter().map(connection_of).collect();
    let expected = [
        said("online", None, None),
        said("catching_up", Some(0), Some(10)),
        said("connecting", None, None),
        said("catching_up", Some(1), Some(10)),
    ];
    assert_eq!(pushed, expected, "the total the catch-up said before the timeout");
    drop(tx);
    running.await.unwrap();
}
```

**T4, Rust,** in `sync/tests.rs`: the phone's patience across a timeout and a restore. It passes at the head, and fails with `connecting` counted toward the patience.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn audit_the_phones_patience_starts_again_once_the_pings_get_through_again() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let patience = Duration::from_secs(1);
    let (live, tx, mut told, running) = phoning(&store, home.path(), patience, PHONE_GIVE_UP);
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    asked_phone(&live);
    tokio::time::sleep(patience / 2).await;
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Connecting));
    assert_eq!(next_connection_within(&mut told, patience * 2).await, None, "nothing of the phone while connecting");
    let restored = Instant::now();
    tx.send(connection_event(State::KeepaliveRestored)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    assert_eq!(next_connection_within(&mut told, patience / 2).await, None, "not offline at once");
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    let waited = restored.elapsed();
    eprintln!("audit: phone_offline {waited:?} after the restore, with a patience of {patience:?}");
    assert!(waited >= patience, "a whole patience again after the restore: {waited:?}");
    drop(tx);
    running.await.unwrap();
}
```

**O2, Rust:** the late `Connected`. It fails at the head, as O2 says, and belongs with O2's fix. Its last line asserts what O2's fix would give.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn audit_a_late_connected_after_its_own_connections_keepalive_timeout_during_a_catch_up() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    // One connection, as whatsmeow can order it: the engine's CONNECTING, the offline preview, the
    // first keepalive timeout (its loop starts at the handshake), and only then Connected, which
    // whatsmeow dispatches once its prekey and passive IQs are answered.
    tx.send(connection_event(State::Connecting)).await.unwrap();
    tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
    tx.send(arriving(ANA, "3EB0LATE1")).await.unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    tx.send(arriving(GROUP, "3EB0LATE2")).await.unwrap();
    until("two counted", || live.lock().unwrap().catch_up.as_ref().is_some_and(|c| c.stored == 2)).await;
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(arriving(ANA, "3EB0LATE3")).await.unwrap();
    tx.send(offline_done(3)).await.unwrap();
    let pushed = next_told(&mut told, 7).await;
    let said: Vec<String> = pushed
        .iter()
        .map(|p| if p.method == "notice" { format!("notice {}", p.params["text"]) } else { format!("{:?}", connection_of(p)) })
        .collect();
    eprintln!("audit: {said:#?}");
    let summary = pushed.iter().any(|p| p.method == "notice");
    let stored = live.lock().unwrap().catch_up.as_ref().map(|c| c.stored);
    eprintln!("audit: summary {summary}, catch-up stored {stored:?}");
    assert!(summary, "the catch-up's summary after OfflineDone: {said:#?}");
    drop(tx);
    running.await.unwrap();
}
```

## Verdict

Changes needed, in tests only:
1. **T1:** tests that fail without the engine's lock, without whatsmeow's ping spacing, with either bound off by one, and with the state said last's time not kept. The five Go tests above do it.
2. **T2:** a restore test whose preview's total and message count differ, as above.

Nothing marked data loss or inconsistency was found, and no code needs to change. T3, T4, O1 to O4 and X1 can stand as follow-ups. O1 and O2 are worth a branch of their own, as both come from whatsmeow saying `Connected` late. Once T1 and T2 are in, the next round need only check those tests against their switches, and the rebase if main has moved.

Verdict: changes needed

# Verdict of round two (`478b590`)

Round one's T1, T2 and T4 are closed, each test failing with its rule switched off, and T3's comment is in. O1, O2, O3 and the demo's signal fix work as built: each of their switches fails a branch test, except two that change nothing jawd can reach (O2f, O2g) and the lock T5 asks a test for. Nothing marked data loss or inconsistency was found, and no code needs to change.

One thing must change before landing:
1. **T5:** a test that fails when `tellConnected` says `Connected` without told's lock. The Go test under "The test to adopt" does it.

O5 to O9 and X2 can stand as follow-ups; X2, whatsmeow's `Connected` for a connection whose IQs gave up, is worth the engine's next change. Once T5 is in, the next round need only check that test against its switch, and the rebase if main has moved.

Verdict: changes needed

# Verdict of round three (`36cd3ce`)

T5 is closed. Without the lock `tellConnected` takes, a test now fails, under `-race` every time, and so does every other place that takes told's lock without it. The rebase onto `folders` brought in only main's context and a descriptor regenerated from the merged proto. The engine's states and `folders`' presence work together through a timeout and its restore, a catch-up's included. Nothing marked data loss or inconsistency remains, no code needs to change, and every rule the branch touches is tested. Round one's T3, round two's O5 to O9 and X2, and this round's O10 stand as follow-ups.

**Landing.** Commit this note in the branch. Give the trailer `Data-Audit: docs/audits/2026-10-29-keepalive.md` to the nine commits `scripts/check-data-audit.py` names: `5b12c7d`, `e2ec927`, `08aed6c`, `779b28e`, `12d3558`, `c7ae2bc`, `d0fca42`, `24021cd` and `36cd3ce`, as they are now. That's every commit but the demo's signal fix, `9e84468`, which touches only `demo/mod.rs` and `tests/stopping.rs`; a `git rebase -x` that gives all ten the trailer does no harm. Then `make check-audit` must pass. If main has moved past `5230b9f`, rebase, and check that `git range-diff` against `5230b9f..36cd3ce` shows only context. If main changed `engine.proto`, regenerate `engine.pb.go` from the merged proto with `make proto`, rather than merging the descriptor by hand, and run `make check-generated`. Then run the engine's tests with `-race`, T5 and the restore race test among them, and jawd's sync, send, engine and presence tests. Have it audited again if anything but context changed in a patch, or if main changed `engine/internal/wa/keepalive.go`, the engine's `say`, sync's connection handling, or anything that reads the `connection` pushes, as `folders`' presence does.

Verdict: approved
