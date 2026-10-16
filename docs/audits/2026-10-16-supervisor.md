# Audit: the engine's supervisor stops when jawd stops

Range: a0ad151..b0987b8 in `lifecycle-2` (the one commit "The engine's supervisor stops when jawd stops, whatever it's waiting for…", first seen as b3fc525 and rebased onto 004481d with an identical diff), 2026-10-16. Auditor: database-expert. Judged against `origin/main`; the two commits under it are out of scope.

The change gives `Supervisor` a `stop()` that sets a flag every wait in its loop also watches (a start under way, a running engine's `exited`, the pause before the next start), so the loop returns from any of them, clears `current` and reports `stopped`. Returning drops the start future or the `Instance`, which for `GoEngine` drops the `Child`, and `kill_on_drop` sends SIGKILL: the same end jawd's exit already gave the engine, when the runtime dropped the supervisor's task. jawd's `stop()` now calls it after `state.stop()`, the socket's removal and the notifications' abort. Nothing in SQL, the schema, PRAGMAs, the inbox or the ack path changes.

The rules it touches:

- No ack goes before its commit, and nothing the engine handed over is lost when the engine is ended mid-message.
- What WhatsApp sends again is stored once (idempotency).
- A `stopped` supervisor starts nothing more, whatever comes late.
- The order of jawd's stop, which the sync branch's clean-stop step will join.

## How an ack and a kill meet

An event jawd must keep reaches jawd with an `Ack` bound to the instance that sent it (`engine/mod.rs:33-41`, `go.rs:172`). The writer thread sends the ack only after the inbox `INSERT` returns (`store/mod.rs:523-543`). The engine's handler blocks until that ack (`engine.go:153-173`). With `SynchronousAck`, whatsmeow tells WhatsApp only after the handler returns. With `EnableDecryptedEventBuffer`, a message WhatsApp sends again is decrypted from the buffer, not the ratchet (`engine.go:94-97`).

Every point where the new `stop()` can cut it:

- **Before jawd's insert.** Nothing is in jaw.db and nothing is acked. WhatsApp sends the message again after the restart, and it's stored then.
- **After the insert, before the engine reads the ack.** The row is in jaw.db and WhatsApp hasn't been told. WhatsApp sends it again, so a second inbox row is converted. `upsert_message` matches it on (chat, id) and changes nothing (`store/mod.rs` test `each_change_writes_its_event_and_its_version_goes_up`: "the same message again changes nothing").
- **After the engine has the ack, before whatsmeow tells WhatsApp.** Same as the point before.
- **No early ack.** The stop path sends the engine nothing; it only drops. An ack can't reach a later engine either, whose seqs start again at 1 (`link.NewAcks` per run), because each `Ack` holds its own instance's sender.
- **Events still in the pipe.** `go.rs`'s reader task is detached (`go.rs:84`), so after the kill it keeps draining the pipe. Sync commits those events to the inbox, and their acks go to a closed channel. They're sent again by WhatsApp and stored once, as above.

These are the cut points a SIGKILL at jawd's exit already had. The change moves the kill earlier, by up to the 100 ms `shutdown_timeout`, and makes it deterministic. It adds no new kind of cut.

While jawd finishes stopping, background tasks keep running, and the engine now answers them "unavailable". None of them records that durably:

- `fetch` treats it as `Later`, so the history notification stays in the inbox (`sync/fetch.rs:88-92`).
- `refresh` and `avatars` only log.

## Findings

### Data loss

None.

### Inconsistency

None.

### Unpredictable performance

None that breaks a budget. Measured on this Mac:

- `stop()` with a running Go engine takes 0.49 ms p50 and 6.5 ms max.
- The builder measured jawd's whole stop at 2 ms with a missing engine and 13 ms with a silent one.

These are well inside the 3 s `STOPPING` bound.

### Hygiene (stand as follow-ups)

1. **A start can still be polled after the stop flag is set** (`supervisor.rs:136-139`, also 153-156 and 174-177).
   - `tokio::select!` isn't `biased`, so when the flag and `engine.start` are both ready, either can win.
   - `GoEngine::start` spawns jaw-engine on its first poll (`go.rs:54-66`). The process is then killed at once by the next `told`, before its hello. It never opens `session.db`, because `wa.Run` says hello and waits for jawd's before `open` (`engine.go:38-45`). So it costs a spawn and a kill, not data.
   - Measured with zero backoff (method below): 497 and 577 of 2,000 runs polled a start after the flag was set.
   - With `biased;` and `told` first in all three selects: 164 and 149 of 2,000. What remains is a stop raised during the start's own first poll, which is ordinary concurrency, not the loop ignoring a flag it has seen.
   - No start ever came after `Stopped`: none in 10,000 runs.
   - Fix: `biased;` with `told` as the first branch in each select.
2. **`stop()` returns once SIGKILL is sent, not once the engine has been reaped** (`supervisor.rs:78-82`, `go.rs:64, 85-93`).
   - Dropping `exited` drops the `Child`, and `kill_on_drop` only calls `start_kill`. Meanwhile the reader task goes on draining the pipe.
   - Measured: the engine was already dead or a zombie when `stop()` returned in 20 of 20 runs (`proc_pidinfo`). The window is real but very short.
   - No data effect today, since the engine acks WhatsApp only after jawd's ack.
   - The doc comment says "waits until it has", and the sync branch will want "after `engine.stop()`, nothing of the engine runs" (see below). Fix: have `Instance` carry an end that kills and awaits the child (`child.kill().await`), so `Stopped` means reaped.
3. **`call()` after `Stopped` gives a stale reason** (`supervisor.rs:93-97`). If the engine ever exited before, it reports that exit rather than "it's stopped". `jaw-proto`'s `Engine.state` doc still lists only `starting`, `running`, `restarting` and `missing` (`crates/jaw-proto/src/lib.rs:318`).
4. **Tests.**
   - Each of the three waits has a test that fails without its branch (mutations below).
   - Nothing fails if `main.rs`'s `engine.stop()` is removed: `tests/stopping.rs` passes on a0ad151 too (twice), since the runtime's drop also kills the engine.
   - `told_to_stop_while_its_engine_runs…` uses the demo engine, whose serving task outlives the stop, so "a running engine is ended" isn't tested with a real process.
   - Follow-up: a Go-engine test, skipped when jaw-engine isn't built like `go.rs`'s, that no jaw-engine on that home is left once `stop()` returns.
   - Separately, a0ad151's `jawd_whose_engine_never_answers…` failed its "the engine started" precondition once on a cold build. Its fixed 500 ms sleep should become a wait with a deadline.

### Before main: not this change

The inbox ack still follows a commit at `synchronous=NORMAL` (`store/mod.rs:116`, data review D1). The sync branch's durable acks fix that. This change neither adds nor removes an ack.

### What the sync branch must keep when the two meet

1. **The engine's disconnect comes before `engine.stop()`.** `sync::stopped`'s `LinkCancel` goes through `Supervisor::call`, which after `stop()` refuses at once.
2. **Write the clean marker after `engine.stop()`, not between the disconnect and it.**
   - `LinkCancel` is only whatsmeow's `Disconnect()` (jaw-sync `engine.go:362-364`). The process stays alive, handlers already dispatched keep forwarding events and waiting for acks, and the supervisor still restarts an engine that exits.
   - `sync::stopped` (jaw-sync `sync/mod.rs:223-233`) checks only for a timeout. When no engine is running, the call fails at once and it writes `clean = "1"`. A supervisor that hasn't yet been told can then finish a start before `engine.stop()`, so an engine runs after the marker.
   - The order that leaves nothing after the marker: disconnect, then `engine.stop()` (reaping, per hygiene 2), then the durable marker. Write `"1"` only if the disconnect succeeded or no engine was running.
   - Test it: one that fails if the marker can be committed while an engine still runs.
3. **The budget.** `LinkCancel`'s 2 s, the marker's synced commit (about 5-10 ms with `F_FULLFSYNC`) and `stop()` (under 13 ms) fit in `STOPPING`'s 3 s. If the outer timeout fires first, the marker stays `"0"`, so the next start refetches app state in full: the safe direction. Keep it that way.
4. **Acks stay bound to their instance**, also when they're released in groups after a synced commit. An ack for an old run's seq must never reach a new engine.
5. **"Unavailable" and "gone" stay temporary.** Quarantine and `Failed::Never` must not take them as a permanent failure, since every background task now sees them during a stop.
6. **When `send.*` lands, its outbox row is committed durably before the engine is called.** `sock.rs:99` finishes an answer already under way after `state.stop()`, and `engine.stop()` can cut its engine call.

## Measurements

Scratch clone `/tmp/jaw-audit-sup` at b3fc525, then at b0987b8. `git show` of the two is byte-identical apart from the header, and `git diff b3fc525 b0987b8` touches nothing under `crates/jawd`. `JAW_HOME=/tmp/jaw-audit-home`. jaw-engine was copied from the worktree's `target/engine/`. The owner's jawd and folder weren't touched.

- **The commit's tests, at b0987b8.** `cargo test -p jawd --bin jawd -- supervisor jawd_stops`: 7 passed. `--test stopping`: 2 passed. At b3fc525, `stopping` failed once on the cold build (precondition, hygiene 4) and passed 3 of 3 after.
- **Mutations.** Each `told` branch was removed in turn:
  - from the start's select: `…while_a_start_waits_for_its_hello…` fails.
  - from the running engine's: `…while_its_engine_runs…` fails.
  - from the pause's: `…while_it_waits_to_start_an_engine_again…` fails.
- **`main.rs`'s hunk.** `--test stopping` at a0ad151, without the change: 2 passed, twice.
- **Late starts.**
  - Method: a test engine whose start fails at once and, on its first poll, records whether the supervisor's own stop flag is set (read through `supervisor.stop.subscribe()`). Backoff 0. `stop()` at 0-980 µs into each run, 2,000 runs, 4 worker threads. Each run asserts no start after `stop()` returns, 2 ms later.
  - As committed: 497 and 577 of 2,000 runs had a start first polled after the flag was set.
  - With `biased;` and `told` first: 164 and 149.
  - Every run, these four and a first one that counted at call time (934 of 2,000), found no start after `Stopped`.
- **`stop()` with a running Go engine.**
  - Method: `GoEngine` on a fresh home, wait for `Running`, find the child's pid with `ps`, time `stop()`, then read `proc_pidinfo` (`PROC_PIDTBSDINFO`, zombies counted as dead). 20 runs.
  - `stop()` took 0.49 ms p50 and 6.5 ms max; an earlier run gave 0.46 ms and 9.0 ms.
  - Engine alive when `stop()` returned: 0 of 20.

## Verdict

Nothing here loses or garbles data. Stopping the supervisor cuts the engine only at points where WhatsApp hasn't been told, and the store takes what WhatsApp sends again once. A stopped supervisor starts nothing more, and each of its three waits is pinned by a test. The hygiene items stand as follow-ups. The sync branch must keep the list above, above all writing the clean marker only once the engine has been ended.

Verdict: approved
