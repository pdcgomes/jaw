# Audit: the landing integration of media in (M3) and typing (M6)

Range: two landings' integration changes, 30 October 2026. Auditor: database-expert.
- **A, media in onto main:** `abfa15a4..42a31eaf` in `m3-media-in` (worktree `~/src/jaw-media-in`), 22 commits on main's `abfa15a4` (folders and the keepalive fix), against the approved `7262a15..a73b659` (`docs/audits/2026-10-26-media-in.md`, six rounds). `origin/main` is now `c50a2ba3`, 23 Mac harness commits on `abfa15a4`.
- **B, typing on top of media in:** `5230b9f4..76208624` in `presence-typing` (worktree `~/src/jaw-presence`), 11 commits approved in `docs/audits/2026-10-29-typing.md` and `docs/audits/2026-10-30-typing-rebase.md`. I rebased it onto `42a31eaf` in my clone and made the three changes its lander found it needs.

Only the integration is in scope: what each landing changes beyond keeping both sides of its rebase. Everything else in either branch stands as its own audits approved it. Paths are under `crates/jawd/src/` unless they say otherwise. Line numbers are at `42a31eaf` for A, and at the patched head for B. In the worktrees I wrote only this note.

## In short

Approved. Nothing is marked data loss or inconsistency.

**A, media in:**
- **The rebase holds what the lander's report lists, and nothing more:** the renumbering to migration 20 with its text unchanged, the engine fields after main's last, `kept.rs`'s patch exactly as the media note prints it, and the three `sync::run` arguments. A mechanical merge of the approved head onto the new base differs from `42a31eaf` only there, and in the 15 conflicted files both sides' lines are kept whole.
- **The three calls are in test and bench code only,** and a default `Following` starts nothing, subscribes no one and writes nothing.

**Media's longer start, and `stopping`'s test:**
- **The cost is needed and bounded.** Media's start makes and syncs its cache's folders once per home, and runs migration 20 once on a fresh jaw.db. The folders' syncs are durability the design asks for, and are about a third of the extra cost.
- **The failure is a load flake that media makes likelier, not a regression.** At a load of 52 to 58, main's jawd answered its first hello a median 0.30 s after listening on a fresh home, and never past 0.76 s. Media in's took a median 0.74 s, past `stopping`'s 1.5 s in 3 of 18 starts.
- **The stop itself is never late:** every stop took at most 0.26 s of its 3 s.
- **What fails is the test's fixed 1.5 s wait for the start,** which main's own jawd misses at a load of about 70. It should wait for hello's answer, as the test beside it does (O1).
- **Nothing in media needs to change before landing.** Cutting the folders' syncs is a follow-up (P1).

**B, typing:**
- **The rebase onto `42a31eaf` conflicts only in its first commit,** in the four files its lander foresaw. Both sides keep cleanly, as described below.
- **The three changes are exactly what's missing, and enough:**
  - without them, jawd's binary and its tests fail to compile in exactly the four places they change;
  - with them, everything I ran passes but three of main's timing tests, which pass alone 3 of 3: clippy, rustfmt, the build, the typing, folders, sync, media and demo tests, and the end-to-end typing test.
- **The patch** is `integration/typing-on-media-in.diff` in the agent store. `git apply` takes it on the rebased head, and on typing over media in over main's harness.
- **Folding the patch into typing's first commit,** rather than adding it as a commit on top, keeps every commit building (O2).

## A. Media in onto main

### The range-diff, and nothing else in it

`git range-diff 7262a15..a73b659 abfa15a4..42a31eaf` pairs all 20 approved commits with their rebased ones (`66f7b18` is `=`, the other 19 `!`) and adds two: `9ce3df60`, `kept.rs`'s patch, and `42a31eaf`, the media-in note. The range-diff is 3,242 lines, most of them regenerated `engine.pb.go` and messages, so I checked what it holds a second way. I merged the approved head onto the new base mechanically (`git merge-tree --write-tree abfa15a4 a73b659`, whose merge base is `7262a15`) and compared that tree with `42a31eaf`:
- **The merge conflicts in the same 15 files the lander lists, and no others.**
- **Seven files merge by themselves but differ at the head,** each as the lander's report says:
  - `media/bench.rs`: one line, the bench's `Default::default()`.
  - `media/tests.rs`: the two rigs' `Default::default()`; `a_media_row_from_before_migration_20_is_kept`, built at version 19 (`[..19]`, `user_version` 19); and K8's two comments saying migration 20.
  - `store/schema/kept.rs`: the media note's patch. Its 34 added lines and 1 removed line are, line for line, those the note prints under "The `kept.rs` patch to adopt".
  - `store/write/copies.rs` and `store/write/copies/tests.rs`: docs saying migration 20; `measure_the_copies_backfill` builds its jaw.db at 19 and prints "migrated to 20".
  - `docs/plans/2026-10-16-jaw-design.md`: "migration 20's `copies`".
  - `docs/audits/2026-10-26-media-in.md`: the note, added. Its last line is `Verdict: approved`.
- **In the 14 conflicted files other than the generated `engine.pb.go`, both sides are kept whole.** I compared each side's added and removed lines before and after the merge:
  - **Media's lines** are at the head as approved, except for the renumbering and two lines it shares with main. The renumbering is the lock's line moving from 19 to 20 with the same hash, `cc0f2d3a`, so the migration's text is unchanged; the migration's comment saying 20; and the six engine fields. The shared lines are `jaw-proto/tests/fixtures.rs`'s list of methods, now two lines with both sides' methods, and the README's `status` row.
  - **Main's lines,** everything `7262a15..abfa15a4` adds or removes (folders and the keepalive fix), are at the head too, except for the same two shared lines. Read whole, the row has main's `presence` and `folders` clauses, then media's.
- **The numbers follow main's last.** The lock reads `18 schema`, `19 data d6e58f43` (folders' step), `20 schema cc0f2d3a` (media's). In `engine.proto`, main ends at `Event` 16 (`typing`), `Command` 21 (`user_info`) and `Reply` 12 (`user_info_list`). Media's fields are `Event` 17 and 18, `Command` 22 to 24 and `Reply` 13, and `engine.pb.go`'s tags say the same. I didn't regenerate `engine.pb.go`. The lander's `make check-generated` did, and CI runs it again.

### The three `sync::run` calls

Folders gave `sync::run` a seventh parameter, `following: presence::Following` (`sync/mod.rs:295`). Media's three calls each pass `Default::default()`:
- **The first rig,** `rig_on` (`media/tests.rs:90`), and **the second,** `answer_left_in_the_inbox` (`media/tests.rs:2521`). Both are in `media/tests.rs`, which is `#[cfg(test)] mod tests;` (`media/mod.rs:38`).
- **The bench,** `serve` in `jawd bench media` (`media/bench.rs:78`). `media/bench.rs` is compiled into jawd, but only `jawd bench media` reaches `serve`, through `bench_media` (`main.rs:358`), on a scratch folder under the temporary folder that it removes afterwards. The owner's jawd never runs it.

Main's own callers do the same: `send/bench.rs:54`, `send/tests.rs:90`, `send/poll/tests.rs:39` and `send/link/tests.rs:344`. jawd's one real caller, `start` (`main.rs:567`), passes the `Following` it starts and serves, as on main.

**A default `Following` starts, subscribes and writes nothing:**
- **Making one spawns nothing.** `Following::default()` (`presence.rs:83`) makes a channel of 1,024 (`HEARD_MOST`), keeps its receiver, and zeroes its counts.
- **Only `start()` (`presence.rs:94`) spawns anything:**
  - `write_heard`, presence's only writer to jaw.db;
  - `connection_of`;
  - `follow`, the only sender of `SubscribePresence` (`presence.rs:260`), and the writer of the `Following` and `Unknown` steps;
  - `about`, which sends `UserInfo` and writes `About`.
  Only jawd's `start` (`main.rs:595`) and presence's and sync's own tests call it, so neither media's rigs nor its bench do.
- **sync uses it in one place.** A presence event from the engine goes to `following.heard(presence)` (`sync/mod.rs:381`), which counts it and `try_send`s it, never waiting. Past 1,024 waiting, it's counted as dropped.

So in the rigs and the bench, presence is at most queued in memory, up to 1,024, and nothing subscribes and nothing is written. Media's demo engine says no presence anyway.

### The start, and `stopping`'s test

**What media's start does** (`media/mod.rs:196`) after jaw.db is open:
- **The cache's folders, in `cache::prepare` (`media/cache.rs:114`).** It makes `media`, `media/files` and `media/previews` if missing, syncing each one it makes and the folder it was made in with `File::sync_all`, which is `F_FULLFSYNC` on macOS. It then removes partial files from them.
  - On a fresh home that's six full flushes, `media` among them three times.
  - On a home where link previews already made `media/`, as the owner's has, it's five, `media` and jaw's folder among them. That happens once, at the first start with media.
  - On every later start it's none.
- **A live commit at every start,** `Restarted` (`media/mod.rs:253`).
- **On a fresh jaw.db, migration 20,** in a transaction of its own, since folders' 19 before it is a data step.

**Needed.** A file is durable only if the folders holding it are. If `media/files`' entry in `media`, or `media`'s in jaw's folder, wasn't synced, a power cut can leave the folder gone, with every file in it, while a synced jaw.db names those files. Syncing each folder once as it's made, before any job writes into it, is what the design's rule asks for: files synced and renamed into place before rows point to them.

**Bounded.** At most seven syncs, and only while a folder is missing, so once per home. Migration 20 also runs once per jaw.db. The design budgets jaw's first frame with jawd already running, and has no budget for jawd's own start.

**Measured,** at a load of 52 to 58 (Measurements says how), on debug builds as the test runs them:

| | Main (`abfa15a4`) | Media in (`42a31eaf`) |
| --- | --- | --- |
| Fresh home: socket to the first answer to hello, p50 (min to max), 18 starts | 0.30 s (0.18 to 0.76) | 0.74 s (0.36 to 1.75) |
| Of them, within the test's 1.5 s | 18 of 18 | 15 of 18 |
| The test replayed, 10 each, alternating | 10 pass | 8 pass; 2 "SIGTERM while starting" |
| The test itself, alone, `cargo test`, 6 each | 6 pass | 5 pass; 1 "SIGTERM while starting" |
| Second start on the same home, p50, 8 | 0.12 s | 0.23 s |
| The same with `media/` taken away first, p50, 8 | 0.15 s | 0.36 s |
| Longest stop after SIGTERM, of its 3 s | 0.17 s | 0.26 s |

So on a fresh home media in adds about 0.44 s at p50. That's three parts, from differences of medians of separate samples, so rough:
- about 0.14 s for the folders' syncs, the start with `media/` taken away against the plain second start;
- about 0.11 s for what media does at every start, mostly `Restarted`'s commit, the two second starts against each other;
- the remaining 0.19 s or so for migration 20's own transaction and its checks on a fresh jaw.db.

The folders' six full flushes alone, made as `prepare` makes them in a separate timing (20 runs, load 52 to 54), took p50 290 ms, 104 to 636 ms. The lander's 0.5 to 0.75 s at a load of about 70 fits these numbers, with the folders as one part of it.

**A load flake, not a regression:**
- **jawd stops within its bound every time.** What fails is the test's precondition, its `std::thread::sleep(Duration::from_millis(1_500))` (`crates/jawd/tests/stopping.rs:68`). Its log check needs a jawd that has finished starting, since only a started jawd says "jawd: stopped in", and one signalled while starting says "SIGTERM while starting, so it stops" and returns at once (`main.rs:445`).
- **Both builds miss the wait under enough load.** Main's own jawd missed it in 5 of 10 starts at a load of about 70, as the lander measured. Media's longer fresh-home start moves that point to lower loads.
- **Nothing in media needs to change.** The test should wait for jawd to answer hello, as `jawd_started_with_sigterm_blocked_…` beside it already does (`stopping.rs:121`), before it signals (O1). Cutting the folders' syncs is a small follow-up (P1).

## B. Typing on top of media in

### The rebase, and how its conflicts were resolved

In my clone, `git rebase 42a31eaf` on `presence-typing` at `76208624`. Only the first commit, `c876d9f5` (typing in chats), conflicted, in the four files its lander foresaw. The other ten applied by themselves. Every conflict kept both sides, each side's lines taken from the conflict markers by a script, not retyped:

1. **`crates/jaw-client/src/lib.rs`** (`jaw status`'s lines): media's block, its `media`, `paused`, `previews` and `jobs` lines, then a `    }` closing it, then typing's `typing` block. The `    }` after the conflict closes typing's block.
2. **`main.rs`** (`start`): typing's two lines, then media's `let mut pools`, which media extends with its lanes:
   ```rust
   let me = live.lock().expect("live state").me();
   typing.start(store.clone(), me, sending.hearing());
   let mut pools: Vec<Arc<dyn work::Report>> =
   ```
   Git merged the rest of `start` by itself, as typing has it: `Feed::start` and `hear_typing` before the supervisor, and `typing.clone()` as `sync::run`'s eighth argument, after `following.clone()`. The feed starting before `Media::start` reads nothing early. `Feed::start` only spawns the follower of committed events, rows are read only for windows, and no window can exist before `start` returns and the socket is served, so media's suspect previews (`media/mod.rs:242`) are still known before anything reads a row.
3. **`engine/demo.rs`**, two conflicts:
   - **`World`'s fields:** media's server fields, `files` to `when_asked`, then typing's field and its doc line, then the struct's `}`, then media's `WhenAsked` and `Phone`:
     ```rust
     /// Where its people's typing goes, once jawd has said.
     typing: Mutex<Option<crate::feed::Typing>>,
     ```
   - **`DemoEngine::new()`:** media's body, which makes its world first so it can serve the lake photo, ending in typing's `.typing()`, then typing's `fn typing(self)`:
     ```rust
     ];
     DemoEngine { script, world, pause: Duration::ZERO }.typing()
     }

     /// The same, its people typing and recording once its script has played (`typists`).
     fn typing(self) -> DemoEngine {
         DemoEngine { typists: true, ..self }
     ```
     The literal has no `typists` until the patch adds it. The other three literals (`with`, `for_account`, `sharing`) merged by themselves, with typing's `typists: false`.
4. **`protocol/README.md`** (the `status` row): media's row, with typing's clause where typing put it, between `layouts`' clause and `settings`':
   > `typing`: the `chats` with someone typing now, how many changes WhatsApp sent (`heard`) and of them `dropped` for coming too fast, those `waiting` to be taken now, of the `waiting_most` that can, the `typing` pushes `sent`, and the changes `merged` into a newer one before they went (see Presence and typing in chats);

   Taking that clause out of the merged row gives media's row byte for byte, and taking it out of typing's row gives its base's.

**To check a resolution against mine,** compare each file's blob at the rebased head, before the patch, with `git rev-parse --short=12 HEAD:<path>`:

| File | Rebased, before the patch | With the patch |
| --- | --- | --- |
| `crates/jaw-client/src/lib.rs` | `78674f4827d3` | same |
| `crates/jawd/src/main.rs` | `c8f2f7014f05` | same |
| `crates/jawd/src/engine/demo.rs` | `7a0e36ba4f2f` | `4f4181663517` |
| `protocol/README.md` | `ec0bc3ce15ad` | same |
| `crates/jawd/src/media/bench.rs` | `8ab4d0c6757f`, media in's own | `bff3f90fc222` |
| `crates/jawd/src/media/tests.rs` | `4cc5e9ded7bb`, media in's own | `96a897177674` |

Main's harness touches none of these files, so the blobs are the same on media in rebased onto `c50a2ba3`.

### The three changes, as one patch

`integration/typing-on-media-in.diff` in the agent store: 3 files, 5 lines added and 2 changed.
1. **Typing's argument, the eighth, at media's three `sync::run` calls:** `crate::feed::Typing::default()`, after the seventh, at `media/bench.rs:79`, `media/tests.rs:91` and `media/tests.rs:2523`. Typing's own test and bench callers pass the same (`send/bench.rs`, `send/link/tests.rs`, `send/poll/tests.rs`). sync uses it in one place, `typing.message(…)` for each message from someone else (`sync/mod.rs:375`). On a `Typing` nothing starts, that returns at once while nobody's typing, and otherwise queues up to 1,024 (`INTAKE_MOST`) or notes the overflow, never waiting. `Typing` writes nothing at all, as typing's audits found, so nothing reaches jaw.db.
2. **`typists: false` in media's literal in `DemoEngine::new()`** (`engine/demo.rs:484`): `DemoEngine { script, world, pause: Duration::ZERO, typists: false }.typing()`. Every literal then sets `typists`, as typing has them, and `new()`'s people type, as typing's `new()` has them do.
3. **Media's bench hello says `PROTOCOL`** (`media/bench.rs:182`): `"protocol": jaw_proto::PROTOCOL`, as the bench's other hello already does (`media/bench.rs:271`). Typing makes the protocol 2, and jawd answers a hello saying 1 with error 6, "this interface is older" (`sock.rs:614`).

**Checked:**
- **`git apply --check` takes the patch on the rebased head,** and applying it gives the tree I tested (`7753a955`). It also takes it on typing rebased onto media in after main's harness, giving the tree of that merge (`68c93b8f`).
- **Without the patch, nothing else is missing.** `cargo check -p jawd --all-targets` on the rebased head fails in exactly four places, all the patch's:
  - E0063, `typists` missing, at `engine/demo.rs:484`;
  - E0061, 7 arguments of 8, at `media/bench.rs:71`, `media/tests.rs:83` and `media/tests.rs:2514`.
  The first two are in jawd's binary itself, so without the patch jawd doesn't build. The third change can't be caught at compile time.
- **With the patch,** everything I ran passed but three of main's timing tests, which pass alone (Measurements). That's clippy for the workspace with `-D warnings`, rustfmt's check, the build, the typing, folders, sync, media and demo tests, and the end-to-end typing test.
- **The third change is needed, and no test catches it.** With the patch, `jawd bench media --count 3` runs through. With the third change alone reverted and jawd rebuilt, it fails: "a photo's preview never came: timed out waiting on channel". No test runs the bench's `watch` (O3).
- **Folded into typing's first commit,** the patch also works. Put in with `git commit --fixup` and `git rebase -i --autosquash`, it goes in without a conflict and gives the same tree. `cargo clippy --workspace --all-targets -- -D warnings` passes at each of the 11 commits (O2).

### Main's harness, `c50a2ba3`

`git merge-tree` merges `c50a2ba3` with media in's `42a31eaf`, and with typing on it, without conflicts. The harness touches only `mac/` and `docs/plans/` (32 files), and none of them is a file media in or typing touches. So media in rebases onto it as the lander expects, typing's conflicts on it are the four above, and the patch applies unchanged.

The one place they meet is on the build Mac, which I can't build on. Typing raises the protocol to 2 in JawKit (`Envelope.swift`, `FakeJawd.swift`) and takes chat rows' `typing` out of the fixtures. By reading, the harness is unaffected:
- it names no protocol number;
- its chat list scene draws made-up rows (`Samples.chats`), not the fixtures;
- JawKit decodes a missing `typing` as empty (`Rows.swift:55`).
JawKit's and the harness's tests should still run on the build Mac once typing lands on main with the harness (O4).

## Findings

### Data loss

None found.

### Inconsistency

None found.

### Test gap

None in the data layer. The calls' arguments are in test and bench code, and the rules they touch are tested where they live: an unstarted `Following` or `Typing` never waits and writes nothing (presence's and typing's own tests, typing's O5 among them). The third change's gap is in bench code (O3).

### Unpredictable performance

- **P1. Media's start syncs its folders by full flushes, `media` up to three times.**
  - **Where:** `media/cache.rs:114` (`prepare_syncing`).
  - **What happens:** on a fresh home, six `F_FULLFSYNC`s before media starts, whose time depends on what else the drive holds. Made as `prepare` makes them, they took p50 290 ms, 104 to 636 ms, at a load of about 53. Inside jawd they were about 0.14 s of media's 0.44 s more than main's fresh start.
  - **Bounded:** once per home, and no budget covers jawd's start.
  - **Fix (follow-up):** sync each folder once, after making them: `media/files`, `media/previews`, `media`, jaw's folder. That's four full flushes, which took p50 237 ms. On the Mac's own drive, barriers would also do, as for downloads' files (round one's P1), since a commit flushed in full precedes every row naming a file in these folders. Barriers took p50 175 to 179 ms. That needs `drive::barriers_hold` decided before the folders exist, since it looks at them.

### Other (follow-ups; none blocks)

- **O1. `stopping`'s missing-engine test waits a fixed 1.5 s for jawd to start.**
  - **Where:** `crates/jawd/tests/stopping.rs:68`.
  - **What happens:** the stop bound always holds, but a jawd still starting 1.5 s after it listens says "SIGTERM while starting", and the test's log check fails. Main's jawd misses the wait at a load of about 70; media in's fresh-home start misses it in 3 of 18 starts at a load of 52 to 58.
  - **Fix:** before signalling, wait up to 30 s for jawd to answer hello, as the test beside it does with `jaw_client::Connection::open` (`stopping.rs:121`), and for its log to say "starting it again". It's a test-only change outside the data layer and needs no audit. Landing it before or with media in keeps the gate's rule of passing alone 3 of 3 meaningful for this test.
- **O2. If the patch lands as a commit on top, typing's 11 commits before it don't build on media in.**
  - **What happens:** without the patch, jawd's binary fails at `engine/demo.rs:484` and `media/bench.rs:71`, so `git bisect` would meet 11 commits that don't build.
  - **Fix (either way is approved):** folding the patch into typing's first commit, as media's lander folded its arguments into `abae4de` and `d69e277`, goes in by autosquash without a conflict. It gives the same tree, and clippy passes at each of the 11 commits. No later typing commit changes `sync::run`'s parameters or `DemoEngine`'s fields.
- **O3. No test runs `jawd bench media`'s `watch`,** so nothing checks the protocol its first hello says.
  - **Where:** `media/bench.rs:182`.
  - **What happens:** with the patch's third change reverted, every test passes, and the bench fails only when run.
  - **Fix (follow-up):** one hello for both of the bench's connections, built from `jaw_proto::PROTOCOL`; or a test that runs `media::bench::run` with a plan of one small photo.
- **O4. Typing and main's harness meet only in Swift.** By reading, the harness names no protocol number and draws made-up rows, and JawKit decodes a missing `typing` as empty. JawKit's and the harness's tests build only on the build Mac, so typing's lander should have them run there once on main.
- **The three timing tests that failed in my run** are main's, and on both landers' lists: the long-request lane, the socket's connection-full test and sync's stop bound. Each passed alone 3 of 3.

## Measurements

**Method:**
- **Scratch:** one folder, `/tmp/jaw-audits/integration`, with one clone of `~/src/jaw-media-in`, which has every branch, and main's `c50a2ba3` fetched into it. Its `target` was the only build folder, with `CARGO_INCREMENTAL=0`, and `TMPDIR` in the folder.
- **Revisions:** switched with `git checkout`. jawd's binaries for `abfa15a4`, `42a31eaf` and the patched head were copied out before the tree changed. The third change's mutant was applied by a script refusing a text it didn't find exactly once, and put back with `git checkout`.
- **Bounds:** every step ran under `nice -n 10` and `gtimeout --kill-after`, which signals the whole process group, test binaries included.
- **Doubled commands:** each step took a lock named for it, so a doubled command would have been refused. None was, and each of the 48 logs has one start and one end.
- **Machine:** Apple M1 Ultra, 20 cores, macOS 26.6.2, rustc 1.99.1. Other agents kept the load at 43 to 101.
- **What I didn't touch:**
  - I opened nothing under `~/Library/Application Support/jaw`, copied no `session.db`, and ran no Go engine. The probes' jawd had a missing engine, and the tests their demo engines.
  - Nothing was sent to WhatsApp.
  - I signalled only the jawd processes the probe started, by their PIDs.

**The start probe** (`startprobe.py`) starts jawd as `stopping.rs` does: `JAW_HOME` a fresh folder, `JAW_ENGINE` an engine that isn't there, `JAW_NOTIFY=off`, output to `jawd.log`, and the socket polled every 20 ms. Builds took turns run by run.
- **"Socket to the first answer":** it then connects and says hello, which jawd answers only once `start` is over, since serving begins after it.
- **"Replayed":** the test itself. SIGTERM 1.5 s after the socket shows, then exit 0 within 3 s and a log saying "starting it again" and "jawd: stopped in".
- **Second starts:** on the same home, then again after taking `media/` away.
- **Load:** 52 to 58, 56 runs, 72 starts.

**The folders' syncs** (`syncs.py`): `mkdir` with mode 0700 and `fcntl` on each folder's descriptor, as `prepare` does, on fresh folders in the scratch folder, on the same volume as the tests' homes. Four variants took turns, 20 runs each, at a load of 51 to 54:

| Variant | p50 | min | p90 | max |
| --- | --- | --- | --- | --- |
| `F_FULLFSYNC`, as `prepare` (6) | 290 ms | 104 | 495 | 636 |
| `F_FULLFSYNC`, each folder once (4) | 237 ms | 125 | 485 | 559 |
| `F_BARRIERFSYNC`, as `prepare` (6) | 175 ms | 93 | 351 | 456 |
| `F_BARRIERFSYNC`, each folder once (4) | 179 ms | 76 | 471 | 1,375 |

**B's runs,** at the patched head:

| Step | Result | Took | Load |
| --- | --- | --- | --- |
| `cargo check -p jawd --all-targets`, without the patch | exit 101, exactly the four errors above | 62 s | 51 to 58 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 43 s | 52 to 54 |
| `cargo fmt --all --check` | 0 | 3 s | 53 |
| `cargo build --workspace` | 0 | 62 s | 43 to 50 |
| jawd's unit tests: `feed::`, `sock::`, `store::write::folders`, `presence::`, `sync::`, `media::`, `store::write::copies`, `demo::`, `engine::`, `store::plans` and the palette's folders test | 330 passed, 3 failed (the timing tests above), 5 ignored | 391 s | 48 to 100 |
| The three alone, 3 runs each | 9 of 9 pass, in 2.2 to 5.9 s | | 52 to 59 |
| `cargo test -p jaw`: 328 unit, `failures`, `settings`, and `typing` end to end | all pass; the end-to-end test in 38 s | 538 s | 63 to 100 |
| `cargo test -p jaw-client -p jaw-proto`, fixtures among them | all pass | 289 s | 66 to 68 |
| `jawd bench media --count 3` | runs through: 256 px preview p50 1.78 s (debug) | 20 s | 47 to 63 |
| The same, the third change reverted | exit 1: "a photo's preview never came" | 75 s | 51 to 62 |
| Clippy at each of the 11 commits with the patch folded in | 11 of 11 exit 0 | 26 to 352 s each | 47 to 68 |

## Landing

**Media in,** from `~/src/jaw-media-in`, after rebasing onto `origin/main`:
- **This note:** commit it as a docs-only commit of its own, after the media-in note's.
- **The trailer:** give `Data-Audit: docs/audits/2026-10-30-landing-integration.md`, beside the `docs/audits/2026-10-26-media-in.md` they carry, to the three commits holding the calls:
  - commit 2 of 22, now `5a029230` (from `abae4de`, "Media in: the engine downloads…"): the bench's and the first rig's argument;
  - commit 7, now `59ee1c38` (from `9c34283`, "The media-in audit's changes: …"): the first rig's call, written one argument a line around it;
  - commit 14, now `f46a3b13` (from `d69e277`, "The phone's "no file" ends the copy's want…"): the second rig's.
  Their hashes change with the rebase onto `c50a2ba3`, but their places and subjects don't.
- **`make check-audit`** passes only once this note is committed, since each trailer's note must be at the head and end in an approved verdict.
- **If main moves again** with more than `mac/` and `docs/plans/`, re-check:
  - main's last migration is still 19 (`migrations.lock`), and `engine.proto`'s oneofs still end at `Event` 16, `Command` 21 and `Reply` 12;
  - `sync::run` gained no parameter, and every caller still has as many arguments as it takes (the build says so);
  - `git range-diff` against `abfa15a4..42a31eaf` shows only main's context and the trailers;
  - `JAW_PLANS=write` regenerates `plans.txt` byte for byte, and `make check-generated` passes.
  Anything beyond keeping both sides in a data-layer path needs another look.

**Typing,** from `~/src/jaw-presence`, once media in is on main:
- **The rebase:** `git rebase origin/main`. Expect conflicts only in `c876d9f5`'s four files, and resolve them as "The rebase, and how its conflicts were resolved" says. The blobs in its table check each resolution.
- **The patch:** `git apply "<agent store>/integration/typing-on-media-in.diff"`, then either:
  - commit it as typing's new commit, which takes the trailer `Data-Audit: docs/audits/2026-10-30-landing-integration.md`; or
  - better, by O2: `git commit --fixup` it onto the rebased `c876d9f5` and `git rebase -i --autosquash`. That commit then takes the trailer, beside the two it carries.
- **The gate:** as usual, with clippy at every commit if folded. Have JawKit's and the harness's tests run on the build Mac (O4).
- **If main moves again,** re-check that it brings no new place needing the same changes:
  - media's calls of `sync::run` are still these three, each with both arguments;
  - every `DemoEngine {` literal in `engine/demo.rs` sets `typists`;
  - `git grep -nE '"protocol": ?1\b' -- crates scripts mac` finds only `jaw-proto`'s settings samples;
  - `git apply --check` still takes the patch, and `make check-audit` passes.
  A new caller, literal or hello needs the same change and another look.

## Verdict

Nothing is marked data loss or inconsistency, and every rule the integration touches is tested where it lives. P1 and O1 to O4 are follow-ups.

Verdict: approved
