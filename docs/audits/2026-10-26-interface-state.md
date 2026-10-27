# Audit: interface state, each interface's layout kept in jaw.db

# Round two: dd348e0, the rebase onto sock-flood and T1

Range: `4126db2..dd348e0` in `interface-state` (worktree `~/src/jaw-istate`), 27 October 2026. Auditor: database-expert. Round one is kept below as written, with its verdict. This round's verdict is the section at the end, and its last line is the note's. Line numbers are at `dd348e0`, and paths are under `crates/` unless they say otherwise.

The range is three commits on main at `4126db2`, which has `sock-flood`'s lane. Nothing is uncommitted but this note.
- `f3c16eb`, jawd: round one's `aee84c8`, rebased, with its socket half done again over the lane.
- `70f7ebe`, the terminal: round one's `3de2d04`, which `git range-diff` finds unchanged.
- `dd348e0`: T1's test, round one's scene as printed, in `jawd/src/layout.rs`'s tests.

Outside `jawd/src/sock.rs` and `protocol/README.md`, `f3c16eb` makes the same change to its base as `aee84c8` made to its own, file by file, but for one comment in jaw-proto's `Interfaces`. Main changed nothing under `jawd/src/store/` between the two bases. So `layout.rs`, the store's half, migration 12, its plans and the upgrade test are as round one audited them.

What I did, in one clone at `dd348e0` with one build folder built with `CARGO_INCREMENTAL=0`:
- read the socket half whole, beside main's loop, and T1's test;
- ran the gate's Rust half on the head as it is;
- built 17 switches into one test binary, each off unless named, the builder's nine and eight of mine, and ran each against the socket, layout and half-close tests;
- wrote five probes over real sockets: a half-closed connection with a push waiting, in two shapes; an answer and a push in either order, left to itself and widened; and a connection that says hello again under another name;
- tried H1's scene and H2's inserts against their survivors;
- killed a real jawd of the head 300 times around a `layout.set`;
- renumbered migration 12 to 13 behind a dummy step, and ran the schema, plan, layout and upgrade tests.

Everything ran with `--engine demo` or in the tests' own folders, under `/tmp/jaw-audits/interface-state-2`, deleted once this note was written. That's why the probes to adopt are printed at the end of this round.

## In short

Approved. Nothing in this range loses data or leaves jaw.db inconsistent, T1 is closed, and the socket half keeps `sock-flood`'s rules with the layout push added. Three follow-ups are new (H12 to H14), none blocking, and round one's stand.

- **T1 is closed.** Its test is round one's scene. It passes 3 of 3, and with either of the person fallback's guards off it fails at "never the chat being merged away", the only one of the 20 layout tests to fail:
  - without the `NOT EXISTS`, the pane that named Rui by a device JID opened his LID chat, the one being merged away;
  - without `user_jid`, that pane was left out, as if his chat were gone.
- **The socket half is as the builder says.**
  - `layout.get` and `layout.set` go down the lane, each with the connection's number and the hello name it had when the request was read (`sock.rs:295`, `:379`, `:385`).
  - The `layout` push is read only while no line waits (`:326`). It goes through main's `send` (`:340`), so with no room it waits in a reply's place and holds the windows. Its channel keeps only the newest layout.
  - Each of the builder's eight switches fails one of their four tests, as they said, and `sock-flood`'s drain switch fails its own test.
- **Never lost, never stale, never ahead of a reply.**
  - A push is never dropped, and a connection that stays connected ends on the newest layout. The builder's tests and my P1 fail with the push dropped or awaited.
  - A push never goes ahead of a reply that waits, nor takes its place, and never ahead of the answer to the hello that subscribed the connection.
  - But an answer read before a newer layout was kept can reach its connection after that layout's push (H13). Left to itself, it didn't happen in 62,222 tries; with the lane's hand-over held 300 ms, it does. The terminal takes the larger version either way, so this is for the README to say.
- **A half-closed connection with a push waiting,** which the builder left untested: my P1 covers it.
  - A connection owed a reply, its writing side closed, with a push waiting, gets that push, then the newest layout, then its reply, and is closed, 3 of 3.
  - It can't lose that push even with the drain not waiting for a line that waits. Once a connection has closed its writing side, a push can only wait while a reply is owed, and that reply can't be sent until the push has gone.
  - Owed nothing, the same connection gets the push that waited and is closed. It's never told the layouts kept while that push waited, though they were kept before it closed its side, and the README promises it "never the last" (H12).
- **Round one's results hold on the rebased code.**
  - 300 kills of a real jawd around a set: the layout kept was never older than the newest answered as kept, never newer than the newest asked for, and always the one asked for; the integrity check was ok and nothing was quarantined.
  - The builder's tests of versions, bounds, quarantine, aliases and merges, the plan test and the upgrade test pass in the gate, and H1's scene passes as printed.
- **Migration 12 holds as 12 today, and renumbers.** Main still has 11. Seven other branches add a 12th, so this one may land as 13 or later. Behind a dummy step it becomes 13 by its lock line and its comment, and the schema, plan, layout and upgrade tests pass.
- **H1 and H2 needn't come before landing.** Both are cheap and checked here, so they're worth adding if the branch is touched for its trailers anyway.
- **The gate's Rust half passes,** but for one feed timing test outside the branch, which passes alone 3 of 3 (X4).

## What was asked, checked

### T1

- **The test** (`jawd/src/layout.rs:679`) is round one's scene with its helpers, as printed.
- **At the head,** it passed 3 of 3 together with the four socket tests, at load 62 to 68.
- **With each guard off,** in the switched binary, against the 20 tests of `layout::` and `store::write::layout::`:

| Switch | What it does | Fails |
| --- | --- | --- |
| `t1-no-not-exists` | the fallback without its `NOT EXISTS` (`store/read.rs:687`) | T1 alone, at `layout.rs:708`: the pane named by the device JID opened `100000000003@lid`, the chat being merged away |
| `t1-no-user-jid` | the fallback asked with the JID as given (`store/read.rs:689`) | T1 alone, at `layout.rs:708`: that pane was left out, its JID in `dropped` |

So T1's test closes round one's T1. After the merge, the test checks each pane's chat with `exists`, which reads jaw.db, so it sees the row the merge's last step deletes.

### The socket half, against `sock-flood`'s rules

Main's connection loop is as `sock-flood` left it, with four changes: the connection's number, taken when it connects (`sock.rs:220`), and its hello name and subscription, taken at hello (`:551-552`); a `Laned` that carries a request's number and name down the lane (`:361`); `send` taking any line (`:340`); and the layout arm, last in the `biased` select (`:326`).

- **Lane order.** `layout.get` and `layout.set` fall to `Taken::Lane` (`:616`), and the lane answers them in turn (`:385`). With them answered on the connection's own task (`s4`), or each on a task of its own (`s5`), `layout_requests_take_their_turn…` fails.
- **The name and the number** are taken when the request is read (`:295`), not when it's answered. With the name read when it's answered (`s6`), the same test fails. With no number (`s7`), the setter is pushed its own set and `a_layout_kept_is_pushed…` fails, as it does with the setter's check removed (`s8`).
- **At most one line waits.** The layout arm is `if open` (`:326`), as every arm that makes a line is. With it read while a line waits (`s1`), a push takes a waiting answer's place, and `under_the_lane…` fails.
- **The hold.** The push goes through `send`, which keeps it as the line waiting and holds the windows (`:348-352`). The reserve arm sends it and lets them go (`:247-251`). So while a push waits, the windows send nothing but still take their changes, and `Behind` fires after `behind`, as for a reply. No window's result depends on a layout push, so for a push the hold only keeps "one line waits, and nothing else goes" true. It's main's code, which `sock-flood`'s tests pin.
- **`Behind`** is unchanged: the windows' code isn't touched.
- **The drain at a half-close** (`:238`) is unchanged. With it not waiting for a line that waits (`s9`), `sock-flood`'s `a_half_closed_connections_last_reply_waiting_for_room_still_comes_before_it_ends` fails, and my P1 passes, as it should (below).
- **Never lost.** With the push dropped when the queue is full (`s2`), the builder's two slow-connection tests and my P1 fail. With the loop awaiting room (`s3`), `under_the_lane…` and P1 fail: no push ever waits, and the loop reads nothing meanwhile.
- **Never stale, for a connection that stays connected.** The channel holds the newest layout published, never an older one after it (`layout.rs:220-230`, round one's tests), and `layout_kept` reads it with `borrow_and_update` (`sock.rs:418-426`). So once a push that waited has gone, the loop's next turn tells the newest. `a_connection_slow_to_read_ends_on_the_latest_layout` and `under_the_lane…` show it, and P1 does for a half-closed connection owed a reply. For one owed nothing, see H12.
- **Never ahead of a reply it depends on.**
  - A push never goes ahead of a reply that waits (`if open`), nor takes its place (`s1`).
  - The first push for a name comes after the answer to the hello that subscribed the connection to it. The subscription is made in `take` (`:551`), whose answer is sent, or waits, in the same turn of the loop, and the layout arm can only run in a later one.
  - A connection is never pushed its own sets (`:327`, `s8`), so nothing it asks causes a push to it. The order between one of its answers and another connection's push isn't kept (H13).
- **A half-closed connection with a push waiting.** The builder relied on the drain's check that nothing waits. The rule holds without it:
  - Once a connection has closed its writing side, the loop takes another turn only while something is owed (`:238`).
  - So after the end of input, the layout arm runs only while a reply is owed, down the lane or on a task of its own.
  - While the push waits, the arms that take those replies are off (`:254`, `:261`), so what's owed can't fall to nothing until the push has gone.
  - A push that started waiting before the end of input was read keeps the lines arm off (`:265`), so the end is read only after the push has gone.
  - So a push that waits is never lost at a half-close, whatever `settled` says of a waiting line. P1 shows it over a real socket: it passes 3 of 3 at the head and with `s9`, and fails with `s2` and `s3`.

I judged a test worth adding (adopt P1, below), as the only test of the README's sentence about half-closed connections and pushes. Its twin owed nothing, P1b, found H12.

### Round one's results, on the rebased code

- **Durability.** 300 SIGKILLs of the head's jawd, `--engine demo`, at 0 to 25 ms and 0 to 10 ms after a `layout.set` over its socket (Measurements).
  - After every restart, the layout kept was never older than the newest answered as kept, nor newer than the newest asked for, and was the layout asked for at its version.
  - At the end of each series the integrity check was ok and nothing was quarantined.
  - `a_layout_is_answered_only_once_a_synced_commit_has_it` and the two crash tests pass in the gate.
- **Newer wins, and stale sets:** the builder's tests pass in the gate, and the kills' checks held.
- **Bounds:** the builder's limit tests pass. H1's scene passes as printed: exactly 16 panes and 16,384 bytes are kept, one past each is error 5, and nothing is quarantined.
- **Quarantine:** the builder's two tests pass.
- **Aliases and merges:** the builder's alias test and T1 pass.
- **Plans:** the plan test passes, so the pinned plans still match the statements.
- **The migration's tests:** the upgrade from version 1, the fault at every statement and the fingerprints pass in the gate.

Round one's costs (a set's 5.6 ms, migration 12's, the plans', the first run's) were measured on the same store code and the same migration, and I didn't measure them again. The lane adds to a set what it adds to every request that goes down it, a channel and a task's wake-up, beside a synced commit of milliseconds.

### Migration 12, and its number

- **Main** still has 11 migrations, and 12 is this branch's, with round one's hash (`e79615bc…`).
- **Seven branches in flight add a 12th of their own:** `folders`, `library`, `m2-content`, `m2-deleted`, `m3-media-in`, `media-out` and `settings`. Whichever lands first takes 12.
- **Renumbered:** with a dummy schema step put before it, the layouts' migration became 13 by its lock line and its `// 13:` comment; its text and hash didn't change. The schema, plan, layout and store-layout tests (32, 2 ignored) and the upgrade test passed, at load 48.

### H1 and H2: before landing?

They needn't come first. Neither can lose data or leave jaw.db inconsistent: a layout of 16,385 bytes let past jawd's check is refused by the table's `CHECK`, quarantined (one row per interface) and answered with an error, and both limits are tested past their edges. But both are cheap, and both are checked on the rebased code:
- **H1:** round one's scene passes at `dd348e0`, and fails with each of its three switches (16 panes refused, 16,384 bytes refused, 16,385 let through). It's the only one of the 20 layout tests to fail.
- **H2:** the inserts printed below pass at the head. With migration 12's JSON `CHECK` at 16,385, or with no `CHECK` on the key, and the lock line changed to match, they fail ("16,385 bytes", "no key"), while the builder's table test passes.

If the builder touches the branch for the trailers anyway, I'd add them then. Added as printed, as tests alone, they're covered by this round: they're what I ran.

## Findings

### Data loss

None.

### Inconsistency

None in this range. Round one's X1, in main's merge code, stands outside it.

### Tests (missing)

None that blocks. T1 is closed, and every rule the socket half adds fails a test without it, but for one corner of the push's routing, a second hello's subscription (H14), which touches nothing jaw.db keeps.

### Unpredictable performance

None. The socket half adds no statement, and the store's are round one's.

### Hygiene (follow-ups; none blocks)

- **H12. A half-closed connection owed nothing, with a push waiting, isn't told the newest layout.**
  - **Where:** `sock.rs:238` and `:265`, against the README's Layouts (`protocol/README.md:415`).
  - **What happens:** in P1b, a connection read nothing while another kept layouts, until the push of 66 found the queue to it full and waited. Five newer layouts were kept, and then it closed its writing side, owed nothing. When it read, it got the pushes up to 66, the one that waited last, and jawd closed it. 71, kept before it closed its side, never came, 3 of 3.
    - Once the push that waited has gone, the loop's next turn takes the lines arm before the layout arm, reads the end of input, and ends the connection, since nothing is owed.
    - The README says a connection slow to read "may miss a layout, but never the last", and that a half-closed one is closed "whatever is kept after". 67 to 71 weren't kept after.
    - Nothing on disk is wrong, and the interface had said it would send no more. A connection that stays connected, or one owed a reply (P1), ends on the newest.
  - **Fix:** either say so in the README: "…still gets a push that waits; a layout kept while that push waited, or after, may not reach it". Or, at `:238`, before ending a half-closed connection, send the newest layout when its channel holds one the connection hasn't been told, waiting for room as a push does. Adopt P1 either way, and P1b with its last line as the fix makes it.
- **H13. An answer can come after the push of a newer layout, and the README doesn't tell interfaces to go by `v`.**
  - **Where:** between a layout's read (`layout.rs:199`) and the lane handing its answer over (`sock.rs:388`); the README's Layouts (`protocol/README.md:401-415`).
  - **What happens:**
    - In P2, a `layout.get` read version 1, and then another connection kept version 2. With the lane's hand-over held 300 ms (`p2-delay`), the reader was pushed 2, then answered 1. Without the delay, the answer came first, 3 of 3.
    - Left to itself (P2n), 62,222 gets raced against 1,504 sets over 12 s, at load 57, and no answer came after a push newer than it. The window is the read's own chat lookups and two tasks' wake-ups.
    - The broadcast push's drain of the lane's replies (`:309-314`) wouldn't help here: the answer is handed over after the push.
    - The terminal takes the larger version from every answer and push (`jaw/src/view/panes/kept.rs:44`, `:58`) and restores only from its answer at start, so it's unaffected. An interface that showed the last layout it heard could show an older one until the next change.
    - The README tells interfaces to apply a reply's `row` by its `v` (`protocol/README.md:20`), but says nothing like it for layouts.
  - **Fix:** in the README's Layouts: "a `layout` push can come before the answer to a `layout.get` or `layout.set` read before it, so an interface goes by `v`, as it does for rows".
- **H14. Nothing tests that a second hello moves a connection's pushes to its new name.**
  - **Where:** `sock.rs:551`.
  - **What happens:** with the subscription kept from the first hello (`s10`), the socket and half-close tests all pass, the builder's test with a second hello among them. P3 fails: the connection renamed `jaw-mac` was pushed `jaw`'s layout and not its own. The README says the push goes "to no other interface". jaw never says hello twice on one connection, and every push names its interface.
  - **Fix:** adopt P3.
- **Round one's H1 to H11 stand.** H1 and H2 are above. H4, a connection that comes back isn't told what was kept while it was away, is unchanged by the rebase.

### Outside this range

- **X1 stands:** main's merge code is unchanged at `4126db2`.
- **X2:** jaw-client's falls-behind test passed in this round's gate, at load 57 to 60.
- **X4. A feed timing test failed once under load.** In the whole suite, `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce` had one lone message of ten arrive 54 ms after its commit, against its 50 ms bound (load 57 to 60). It passed alone 3 of 3 at load 60 to 61. The branch doesn't touch `feed/`.

## Measurements

The machine is a Mac Studio (M1 Ultra, 20 cores, macOS 26.6.2), with rustc 1.99.1, shared with other builders and audits at load 39 to 78 throughout; the load is beside each number.
- **The clone:** `/tmp/jaw-audits/interface-state-2/jaw`, at `dd348e0`, with one build folder, built with `CARGO_INCREMENTAL=0` and switched with `git checkout`. The head's jawd (`4f87f455…`) was copied out before any change.
- **What I touched:** nothing in `~/src/jaw` or the builder's worktree but this note. No Go engine ran: I didn't build `target/engine/jaw-engine`, so no test could start one, and every jawd I ran was the demo engine's, in a folder of mine. I stopped only jawds I'd started, by their PIDs.
- **What's kept:** copies of my switches and probes (`audit-probes.diff`, which applies to `dd348e0`), my scripts and my logs are in this auditor's store, `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/interface-state-round2/`.

**The gate's Rust half, on the head as it is:**
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (3 min, load 65), `scripts/sync-protos.py --check`, and `engine/internal/link/enginepb` unchanged: all clean.
- `cargo test --workspace --no-fail-fast` (7.5 min, load 57 to 60):
  - passed: jaw's 240 (2 ignored), jaw-client's 18 and its 3 against a real jawd, jaw-media's 22 and 17 (1 ignored), jaw-proto's fixtures, jaw-voice's 36 (2 ignored), wa_proto's 3, and jawd's integration tests (starting, stopping, upgrading);
  - jawd's: 391 passed and 4 ignored, and X4's timing test failed.
- `scripts/check-data-audit.py origin/main HEAD` fails only for want of the trailer on `f3c16eb` and `dd348e0`. `70f7ebe` touches none of its paths.
- **Not run:** `go vet`, `gofmt` and the engine's tests, since the range touches neither `engine/` nor `proto/`. The builder ran `make lint` and `make test` at 0.

**Switches.** All in one test binary, each on when `JAW_AUDIT_MUT` names it (`crate::audit`, only under `cfg(test)`), each run once at load 45 to 78, against one of two sets:
- the socket's 21: the 14 of `sock::tests`, the builder's four layout tests among them; my probes but P1b and P2n; and `sock-flood`'s four half-close tests;
- the layouts' 20: `layout::tests` with H1's scene, and `store::write::layout`.

With none on, all 21 passed.

| Switch | What it does | Fails |
| --- | --- | --- |
| `s1-unguarded` | the layout arm read while a line waits | `under_the_lane…` |
| `s2-dropped` | the push dropped when the queue is full | `a_connection_slow_to_read…`, `under_the_lane…`, P1 |
| `s3-blocking` | the loop awaits room for the push | `under_the_lane…`, P1 |
| `s4-at-once` | layout requests answered on the connection's own task | `layout_requests_take_their_turn…` |
| `s5-apart` | each on a task of its own | `layout_requests_take_their_turn…`, `under_the_lane…` |
| `s6-name-when-answered` | the hello name read when the request is answered | `layout_requests_take_their_turn…` |
| `s7-no-number` | no connection number down the lane | `a_layout_kept_is_pushed…` |
| `s8-setter-told` | the setter pushed its own set | `a_layout_kept_is_pushed…` |
| `s9-settled-ignores-waiting` | the drain not waiting for a line that waits | `sock-flood`'s `a_half_closed_connections_last_reply_waiting_for_room…`; P1 passes |
| `s10-no-resubscribe` | a second hello keeps the first's subscription | P3 alone (H14) |
| `p2-delay` | the lane hands a layout answer over 300 ms late | P2; `under_the_lane…` and P1 too, which can't fill a queue in time with each layout request 300 ms slower |
| `t1-no-not-exists`, `t1-no-user-jid` | T1's two guards | T1 alone |
| `h1-panes-at-16`, `h1-bytes-at-16384`, `h1-bytes-past-16385` | H1's three survivors | H1's scene alone |

`s1` to `s9` are the builder's, written again in this form; `s10`, `p2-delay` and the H1 switches are mine.

**The probes,** over real sockets on the builder's `serving_layouts`, 3 runs each at the head, at load 39 to 43:
- **P1:** answered `[2]`, its sleep; 67 pushes, 1 to 66 and then 71, the newest; then closed by jawd. 3 of 3, 20 s each, since a probe-only request holds its lane 20 s.
- **P1b:** 66 pushes, the last 66; closed; the newest kept, 71, never told. It fails at its last line 3 of 3 (H12).
- **P2:** answer 1, then push 2, 3 of 3. With `p2-delay`: push 2, then answer 1.
- **P2n:** 62,222 answers, none older than a push before it, against 1,504 sets in 12 s (load 57).
- **P3:** pushed `jaw-mac`'s 7 and nothing else, 3 of 3.
- **H1's scene** passes. **H2's inserts** pass, and fail with each of the two changed migration texts (locked as `f0663457…` and `bc2956fd…`), which I then put back to `e79615bc…`.

**Kills** (`kills.py`). The head's jawd, `--engine demo --no-notify`, with `JAW_HOME` in my folder. Each run starts it, reads the layout and checks it, sends a `layout.set` one version newer, SIGKILLs it at a random time after, and reads whatever came. Each series had a fresh folder.

| Kills | Seed | Killed after | Answered | Unanswered, there after the restart | Unanswered, not there | Last answered, last asked, kept at the end | Load |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 150 | 7 | 0 to 25 ms | 114 | 31 | 5 | 150, 150, 150 | 53 to 58 |
| 150 | 8 | 0 to 10 ms | 99 | 31 | 20 | 143, 150, 148 | 57 to 64 |

Each series ended with a start that checked its last kill, then a SIGTERM: `integrity_check` ok, nothing quarantined, `user_version` 12.

**The branches with a 12th migration,** from `git for-each-ref` in `~/src/jaw` at 06:58: `folders` (a data step), `library` (12 and 13), `m2-content` (12 and 13), `m2-deleted`, `m3-media-in`, `media-out` (12 and 13) and `settings`, besides this one, and three older refs, `library-audited-1`, `library-before-a4e0e47` and `media-out-pre-rebase`.

**Renumbering** (`renumber.py`): `CREATE TABLE audit_dummy (x INTEGER) STRICT` went before it as 12 (`2a930de8…`), making the layouts' migration 13. Two files changed. The 32 tests of `store::schema`, `store::plans`, `layout` and `store::write::layout` passed (2 ignored), and so did `upgrading`'s first run from version 1, at load 48.

## The probes to adopt

P1, P1b and P3, with their helpers, for the tests in `jawd/src/sock.rs`, beside the builder's `Lines`, `wide` and `halves`. They're as I ran them, but for their names, the request's name and my prints. P1 needs a test-only request down the lane that waits, in `answer`, beside `test.panic`:

```rust
        #[cfg(test)]
        "test.sleep" => {
            let ms = request.params.get("ms").and_then(Value::as_u64).unwrap_or(1_000);
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            succeeded(id, &serde_json::json!({}))
        }
```

```rust
    /// A line read within `wait`: the line, the end of the connection, or nothing in time.
    enum Heard {
        Line(Value),
        Closed,
        Nothing,
    }

    fn heard(lines: &mut Lines, wait: Duration) -> Heard {
        // macOS refuses a timeout once both sides are shut; the one set before stays.
        let _ = lines.reader.get_ref().set_read_timeout(Some(wait));
        let mut line = String::new();
        match lines.reader.read_line(&mut line) {
            Ok(0) => Heard::Closed,
            Ok(_) => Heard::Line(serde_json::from_str(&line).unwrap()),
            Err(_) => Heard::Nothing,
        }
    }

    /// Everything the connection hears until jawd closes it, or nothing comes for `wait`: the
    /// answers' ids, the pushes' versions, and whether it was closed.
    fn to_the_end(lines: &mut Lines, wait: Duration) -> (Vec<u64>, Vec<u64>, bool) {
        let (mut answers, mut pushes) = (Vec::new(), Vec::new());
        loop {
            match heard(lines, wait) {
                Heard::Line(line) => match line["id"].as_u64() {
                    Some(id) => answers.push(id),
                    None => pushes.push(line["params"]["v"].as_u64().unwrap_or_else(|| panic!("{line}"))),
                },
                Heard::Closed => return (answers, pushes, true),
                Heard::Nothing => return (answers, pushes, false),
            }
        }
    }

    /// Keeps wide layouts from `setter` until a push to another of its interface's connections
    /// waits for room, then `more` after it; says the last version kept.
    fn fill_then(setter: &mut Lines, more: u64) -> u64 {
        let mut v = 0;
        while setter.waiting() == 0 {
            v += 1;
            assert!(v <= 400, "no push waited for room");
            let answer = setter.call("layout.set", json!({ "v": v, "layout": wide() }));
            assert_eq!(answer["result"]["kept"], true, "{answer}");
        }
        for _ in 0..more {
            v += 1;
            let answer = setter.call("layout.set", json!({ "v": v, "layout": wide() }));
            assert_eq!(answer["result"]["kept"], true, "{answer}");
        }
        v
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_half_closed_connection_owed_a_reply_gets_the_push_that_waits_then_the_latest_then_its_reply() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut slow = Lines::open(&socket, "jaw");
            let mut setter = Lines::open(&socket, "jaw");
            let slept = slow.send("test.sleep", json!({ "ms": 20_000 }));
            slow.reader.get_ref().set_read_timeout(Some(Duration::from_secs(40))).unwrap();
            slow.writer.shutdown(std::net::Shutdown::Write).unwrap();
            std::thread::sleep(Duration::from_millis(300));
            let latest = fill_then(&mut setter, 5);
            let (answers, pushes, closed) = to_the_end(&mut slow, Duration::from_secs(40));
            assert_eq!(answers, [slept], "its reply, once");
            assert!(pushes.is_sorted_by(|a, b| a < b), "a push older than one before it: {pushes:?}");
            assert_eq!(pushes.last(), Some(&latest), "it ends on the latest: {pushes:?}");
            assert!(closed, "jawd closed it once it had its reply");
            assert_eq!(setter.waiting(), 0, "nothing waits once it's gone");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_half_closed_connection_owed_nothing_with_a_push_waiting_is_told_the_latest_before_it_ends() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut slow = Lines::open(&socket, "jaw");
            let mut setter = Lines::open(&socket, "jaw");
            let latest = fill_then(&mut setter, 5);
            slow.reader.get_ref().set_read_timeout(Some(Duration::from_secs(20))).unwrap();
            slow.writer.shutdown(std::net::Shutdown::Write).unwrap();
            let (answers, pushes, closed) = to_the_end(&mut slow, Duration::from_secs(20));
            assert!(answers.is_empty());
            assert!(pushes.is_sorted_by(|a, b| a < b), "{pushes:?}");
            assert!(closed, "jawd closed it");
            assert_eq!(setter.waiting(), 0);
            assert_eq!(pushes.last(), Some(&latest), "it's told the latest kept before it closed its side");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_named_anew_is_pushed_its_new_interfaces_layouts_and_not_its_old_ones() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut renamed = Lines::open(&socket, "jaw");
            let hello = renamed.call("hello", json!({ "protocol": PROTOCOL, "client": "jaw-mac", "version": "0" }));
            assert!(hello.get("result").is_some(), "{hello}");
            let mut jaw = Lines::open(&socket, "jaw");
            let mut mac = Lines::open(&socket, "jaw-mac");
            assert_eq!(jaw.call("layout.set", json!({ "v": 5, "layout": halves(5) }))["result"]["kept"], true);
            assert_eq!(mac.call("layout.set", json!({ "v": 7, "layout": halves(7) }))["result"]["kept"], true);
            let mut pushes = Vec::new();
            while let Some(line) = renamed.within(Duration::from_secs(1)) {
                pushes.push((line["params"]["interface"].clone(), line["params"]["v"].clone()));
            }
            assert_eq!(pushes, [(json!("jaw-mac"), json!(7))], "the new name's layout, and not the old one's");
        })
        .await
        .unwrap();
    }
```

The second fails at `dd348e0`, at its last line (H12). With the README's fix, that line goes; with the code's, it passes as written.

H2's inserts, as a test of their own among the tests in `jawd/src/store/write/layout.rs`, or as lines in `the_table_holds_json_of_16_kib_at_most_a_version_from_1_and_names_of_1_to_64_bytes`:

```rust
    #[test]
    fn the_tables_checks_hold_at_their_edges() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let insert = |interface: &str, key: &str, value: &str| {
            db.execute(
                "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES (?1, ?2, ?3, 1, 0)",
                params![interface, key, value],
            )
        };
        assert!(insert("a", "layout", &format!("\"{}\"", "x".repeat(16_382))).is_ok(), "16,384 bytes");
        assert!(insert("b", "layout", &format!("\"{}\"", "x".repeat(16_383))).is_err(), "16,385 bytes");
        assert!(insert("c", "", "{}").is_err(), "no key");
        assert!(insert("d", &"k".repeat(65), "{}").is_err(), "a key of 65 bytes");
        assert!(insert("e", &"k".repeat(64), "{}").is_ok(), "a key of 64 bytes");
    }
```

H1's scene is printed in round one, and passes as written.

# Round one: 3de2d04, kept as written

# Audit: interface state, each interface's layout kept in jaw.db

Range: `4ca6774..3de2d04` in `interface-state` (worktree `~/src/jaw-istate`), 26 October 2026. Auditor: database-expert. Line numbers are at `3de2d04`, and paths are under `crates/` unless they say otherwise.

The range is two commits, with nothing uncommitted:
- `aee84c8`, jawd: migration 12 (`interface_state`), `layout.get`, `layout.set` and the `layout` push, with the protocol's text and fixtures.
- `3de2d04`, the terminal: with panes on, it keeps its panes once they've stayed as they are for 2 s, and opens with them.

`origin/main` moved during the audit, from `4ca6774` to `4126db2`, which brings `sock-flood`: the connection's lane (`79f6211`) and its approved audit. Main added no migration and changed nothing under `jawd/src/store/`. But a trial rebase in my clone stops at `aee84c8`, with four conflicts in `jawd/src/sock.rs` and two in `protocol/README.md`. This branch now lands second (see "Landing second, after sock-flood").

What I did:
- read both commits whole, with the code around them;
- ran the gate in an unmodified clone at `3de2d04`;
- made 44 switches, each turning off one thing the branch does, and ran each against the tests around it; nine survived, and six of those were run again against the whole suite;
- wrote scenes for what no test reached: a merge run to its end, the limits' exact edges, the quarantine, a version at the top of the range, sets in any order from three connections, a connection coming back;
- killed a real jawd 250 times around a `layout.set`, and 24 times while it upgraded a million-message account from 11 to 12;
- ran the owner's first run from version 1 on the generated 90 MB file, three times a side against main;
- measured migration 12, the new statements and their plans on the million-message account.

Everything ran with `--engine demo` on generated accounts, in scratch clones of my own under `/tmp/jaw-istate-audit*`, now deleted as the brief asks. That's why the scenes to adopt are printed at the end of this note.

## In short

Changes needed: one test is missing (T1), and the branch must be rebased onto main, which now has `sock-flood`, then audited again for its socket half. Nothing in this range loses data or leaves jaw.db inconsistent.

- **Durable before answered.** `layout.set` answers only after a synced commit. In 250 kills around a set, every layout answered as kept was there after the restart, and never an older one. Nothing was quarantined, and the integrity check was ok. On the million-message account a set takes 5.6 ms at p50 against 0.40 ms for a get, at load 68, and `synced_commits` moves by one a set: one `F_FULLFSYNC` each.
- **Newest wins, and asking again changes nothing.** Only a newer version replaces what's kept, and a stale set is answered with the newer layout and `kept: false`. In my scene, 60 sets in scrambled order from three connections ended on the newest; a listener was told only newer layouts, only ones kept.
- **Publish-by-writer holds for connections that stay connected, however slow.** A connection that comes back isn't told what was kept while it was away (H4).
- **The bounds hold:** past 16 panes or 16 KiB a layout is refused before the writer, and a 17th interface by the writer, each with error 5 and counted in `jawd status`. The table is `STRICT` and `WITHOUT ROWID`, with `CHECK`s. The exact edges have no test (H1, H2).
- **Chat references:** a pane is read through merges and the person's other JIDs. In my scene, which runs a merge to its end, no pane ever named a chat row that was gone. But the two guards that make this so for a person's other JID have no test (T1).
- **Another interface's layout** can be read and never changed, and reading it writes nothing.
- **Migration 12** is a schema step creating a table of jaw's own data.
  - A fault at every statement, and 24 kills during the upgrade, leave the file wholly at 11 or wholly at 12.
  - It costs a synced commit and nothing measurable beyond that.
  - It renumbers to 13, 14 or later by its lock line and one comment.
- **The owner's first run from version 1** gives the same rows, and the same history byte for byte, as main's.
- **Plans** are pinned. The only scans are of `interface_state`, which holds 16 rows at most, and the plan test allows them for that reason. No temporary b-tree.
- **The terminal** keeps a layout once it has stayed as it is for 2 s, never once a key. It restores only what jawd answers, and jawd leaves out every pane whose chat it doesn't know.
- **Outside this range, an inconsistency in main's merge code (X1):** a chat kept by one merge can be merged away by another. Finishing the second then silently cancels the first, and the person is left with two one-to-one chats. It needs its own fix and audit.

## What was asked, checked

- **A synced commit before every answer.**
  - `keep` writes through `apply_durable` (`jawd/src/layout.rs:107`), which answers after the writer's synced commit (`FULL`, `fullfsync`, `checkpoint_fullfsync` on every connection, as on main). Without the sync, `a_layout_is_answered_only_once_a_synced_commit_has_it` fails.
  - When a shared commit fails and the group is retried alone, each attempt overwrites the write's result slot (`Kept`, `jawd/src/store/write/layout.rs:27`). `keep` looks at `quarantined` before it reads that slot (`layout.rs:110`), so a failed attempt's stale `Written` is never answered.
- **A kill at every point.**
  - 250 kills of a real jawd at 0 to 25 ms after a `layout.set`. Of those, 164 sets were answered and 86 weren't; of the unanswered, 58 were there after the restart and 26 weren't. Both are allowed, since neither was acknowledged.
  - After every restart, the version kept was never below the newest answered as kept, nor above the newest asked for. At the end the integrity check was ok and nothing was quarantined.
  - A kill is a process crash, not a power cut. For a power cut the evidence is the synced commit and what a set costs (Measurements).
- **Replays, and stale or concurrent writes.**
  - The upsert (`store/write/layout.rs:52`) updates only `WHERE excluded.v > interface_state.v`, and `apply` checks for room only for an interface with no row (`:58`).
  - The same version with a different layout is `Unchanged`. It's answered with what's kept and `kept: false`, since `own` compares the value as well as the version (`layout.rs:131`).
  - The builder's tests cover asking again, a late set, several connections at once, and asking again after a restart.
  - My scenes add 60 sets in scrambled order from three connections, and two connections setting at once over real sockets with a third listening. Each ends on the newest, and none is ever told an older layout.
- **Publish-by-writer.**
  - `answer` reads the row back after the commit, and publishes only when this set wrote it and it's still what's kept (`layout.rs:129`).
  - `publish` replaces the channel's value only with a newer version (`:224`), so the channel never goes back, however reads and publishes interleave.
  - The push goes only to the interface's other connections (`sock.rs:133`). It waits for room rather than being dropped (`:136`), and the channel holds only the latest, so a slow connection ends on the latest.
  - After a restart the channels start empty; a connection then says hello and reads with `layout.get`, and ends on what's on disk.
  - Each switch that breaks one of these fails a builder test.
  - Across a reconnect, see H4.
- **Bounds, `CHECK`s, quarantine and `jawd status`.**
  - `checked` (`layout.rs:152`) checks the format, at most 16 panes, that each pane names a JID, that no chat is in two panes, that each split's first side takes 1 to 999 thousandths, that the pane with the keys exists, and at most 16,384 bytes of compact JSON. A layout past a limit is error 5 and counted in `refused`, before the writer, so it never reaches quarantine. A 17th interface is `Keeping::Full`: error 5, counted (`:114`). An interface that has a row can always change it.
  - Migration 12's table (`store/schema.rs:721`) holds names of 1 to 64 bytes, JSON that's `json_valid` and at most 16,384 bytes, and a version from 1, and it's `STRICT` and `WITHOUT ROWID`.
  - A write that fails is quarantined as `interface_state`, `"<interface> layout"`, with its JSON (`store/write/layout.rs:75`), and the set is answered with an error. Failures for one interface replace one row (`ON CONFLICT (source, ref)`, `store/write.rs:529`).
  - `jawd status` shows kept, most, largest_bytes, bytes_most and refused (`layout.rs:140`), and the quarantine as before. After the kills it said 1 kept, 16 most, 152 bytes the largest, 16,384 bytes most, none refused.
- **Chat references, with `start_merge` and `merge_step`.**
  - `chat_named` (`store/read.rs:672`) reads a JID as the writer's `chat_jid` does (`store/write.rs:1054`). A chat's own JID gives it, or the chat it's being merged into. A person's JID with no chat of its own gives their one-to-one chat under another of their JIDs that isn't being merged away, the oldest.
  - A pane whose chat isn't known, or that shows a chat a pane before it shows, is left out and named in `dropped`, and the keys go to the pane before it.
  - Layouts hold JIDs, not row keys, so a chat that goes leaves nothing in jaw.db pointing at it; a pane's chat is read again at each `layout.get`.
  - The builder's test covers aliases and a merge just begun. My scene runs `merge_step` to its end, where the gone chat's row is deleted (`store/write.rs:1375`). Every pane of two interfaces' layouts names a chat row that exists, during the merge and after it. It passes at `3de2d04`.
- **Another interface's layout.** `layout.get` takes an `interface` (`layout.rs:72`). `layout.set` always writes the connection's own hello name (`:81`), whatever its params say, and its answer names that interface.
  - In my scene, a set from `jaw-mac` naming `jaw` kept `jaw-mac`'s layout and left the terminal's as it was.
  - 40 gets of interfaces with nothing kept left two rows.
  - Nothing gets past the limits this way: a get writes nothing, and a set's checks don't depend on the interface.
- **Migration 12.**
  - **Kind and class:** it's a schema step, with its class, jaw's own, declared in its text (never dropped or rebuilt; a rebuild copies it as it is).
  - **Its number:** its text holds none, so its hash (`e79615bc…`) is the same whatever number it takes.
  - **Faults:** consecutive schema steps share one `IMMEDIATE` transaction. `a_fault_at_any_statement_of_any_migration_leaves_the_database_as_it_was_before_that_step` walks every migration, 12 too.
  - **Kills:** of the 24 during the upgrade, 2 left the file at 11 with no `interface_state`, and 22 at 12 with the table exactly as migration 12 makes it, `quick_check` ok each time. A start after them finished at 12 with no foreign-key violations. The migration's own window is milliseconds wide, so the kills mostly land on either side of it; the fault test covers its middle.
  - **First run and renumbering:** covered below.
- **Plans:** each new statement's plan is pinned in `store/plans.txt` and matches `EXPLAIN QUERY PLAN` on the million-message account (Measurements).
- **The terminal.**
  - **Keeping:** `keep_layout_when_due` (`jaw/src/view/panes/kept.rs`) restarts a 2 s timer at each change and keeps the layout once it's due. A layout the same as what's kept keeps nothing.
  - **Restoring:** jaw opens only what jawd answers, so every pane it opens is on a chat in jaw.db. The chat list hides only archived chats, and those are opened from the archive, so every such chat is one the user can see. A layout of one pane or more than six opens nothing, and so does one that comes once the view is split.
  - **Switches:** each switch that breaks the timer, the version or the asking again fails a builder test. The two that don't are under H3 and in Measurements.

## Findings

### Data loss

None.

### Inconsistency

None in this range. One in main's merge code, which this range reads, is under "Outside this range" (X1).

### Tests (missing)

This blocks approval: it's a rule the change touches that no test fails without.

- **T1. Nothing tests that a pane named by a person's other JID never lands on a chat being merged away.**
  - **Where:** `store/read.rs:686` to `:689`, the person fallback in `chat_named`. Its `NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = c.n)` at `:687` leaves out a chat being merged away, and `user_jid(jid)` at `:689` strips a device from the JID.
  - **What happens:** with either removed, every test of the whole jawd suite passes but one notification timing test that never reaches this code (367 tests, at load 65 to 78; see Measurements).
    - **Without the `NOT EXISTS`:** while a person's two chats merge, a pane naming them by a JID with no chat of its own is answered with the oldest of their chats. Whenever their LID's chat is older than their number's, that's the one being merged away. The interface then opens a pane on a chat whose row the merge's last step deletes, which breaks the "never a chat row that's gone" this range promises.
    - **Without `user_jid`:** such a pane is left out as if its chat were gone.
    - **Why no test catches either:** the builder's merge test never asks for a layout through this fallback while a merge runs, and never runs `merge_step` to its end.
  - **Fix:** adopt `a_merge_run_to_its_end_leaves_each_pane_on_a_chat_row_that_exists`, with its three helpers (printed below), or a test doing the same. It fails at "never the chat being merged away" with either guard removed, and passes as written.

### Unpredictable performance

None breaks a budget.
- The new statements cost 2.7 to 17 µs at p50, with 208 µs at p99 for the person fallback's cold pages. A layout of 16 panes reads in 32 statements at most, under 0.5 ms.
- A set is an upsert of 3 µs in a synced commit of about 5 ms on this Mac, like every durable request. The terminal's 2 s wait makes that one commit for each layout settled, never one a key.

### Hygiene (follow-ups; none blocks)

- **H1. The limits' exact edges have no test.**
  - **Where:** `layout.rs:162` (panes) and `:184` (bytes).
  - **What happens:** each of these passes the whole suite:
    - refusing at 16 panes rather than past 16;
    - refusing at exactly 16,384 bytes;
    - letting 16,385 bytes through, which the table's `CHECK` then refuses. That layout is quarantined, 16 KiB of it, and answered -32603 rather than error 5.
  - **Fix:** adopt `exactly_16_panes_and_16_kib_are_kept_and_one_past_each_is_error_5_with_nothing_quarantined` (printed below). It's cheap to add with T1.
- **H2. Two of the table's `CHECK`s have no test at their edges.**
  - **What happens:** with the JSON's `CHECK` at 16,385 bytes, or with no `CHECK` on the key's length, `the_table_holds_json_of_16_kib_at_most_a_version_from_1_and_names_of_1_to_64_bytes` still passes. The other `CHECK`s, `STRICT` and `WITHOUT ROWID` each fail a test.
  - **Fix:** in that test, insert exactly 16,384 bytes (taken) and 16,385 (refused), and keys of 0 and 65 bytes (refused).
- **H3. A restore over panes already split has no test.**
  - **Where:** `layout_came`'s `!self.split()`, in `jaw/src/view/panes/kept.rs`.
  - **What happens:** with it removed, the whole terminal suite passes (239 tests, at load 77 to 80). A late answer, such as one asked again after a reconnect, would then replace panes the user has opened since.
  - **Fix:** a test that splits, then delivers the layout's answer, and checks the panes are as the user left them.
- **H4. A connection that comes back isn't told what was kept while it was away.**
  - **Where:** `subscribe` at hello (`sock.rs:257`, `layout.rs:64`) marks what's in the channel as seen. jaw asks for its layout once at start, and on a reconnect asks again only for requests whose answers were lost.
  - **What happens:** after the socket drops or jawd restarts, jaw keeps the version it knew. Its next set is usually newer anyway, since it takes the clock. But if what was kept meanwhile has a version past this jaw's clock, its set is answered `kept: false`. It moves its version on and keeps nothing until the layout changes again. In my scene over real sockets, a connection that came back after another connection's set heard nothing.
  - **Fix:** at hello, mark the new receiver changed when the channel holds a layout; or have jaw ask `layout.get` after each reconnect, for the version only.
- **H5. A version at the top of the range wedges an interface's layout.**
  - **Where:** `layout.rs:84`. The protocol takes `v` as a `u64`; jawd keeps up to `i64::MAX` and refuses past it with -32602.
  - **What happens:** once a set at `i64::MAX` is kept, no version can replace it, and every clock is behind it (my scene). Versions past 2^53 aren't exact in a JavaScript client's JSON either.
    - Only the interface's own connections can do this, since a set always writes its own interface.
    - It stays wedged until the row is deleted by hand.
  - **Fix:** refuse versions past 2^53 − 1 as a version of 0 is refused, with -32602 naming `layout.v`.
- **H6. A failed write's quarantine row stays once a newer layout is kept, and doesn't hold its version.**
  - **Where:** `store/write/layout.rs:75`.
  - **What happens:** in my scene, three failures left one row with the JSON, and a set kept afterwards left it there. The row's `raw` is the layout without `v`, so whoever reads the quarantine can't tell whether it's older than what's kept.
  - **Fix:** quarantine the params, `{"v": …, "layout": …}`, and say in the README that a newer layout kept supersedes it.
- **H7. A failed read-back after the commit is answered as an error, and the layout is published to no one.**
  - **Where:** `layout.rs:130`.
  - **What happens:** the layout is on disk, but the set is answered with an error and the interface's other connections aren't told. Asked again, the set is `Unchanged`, so it's still pushed to no one. Other connections only move their version on, so nothing is lost.
  - **Fix:** read back again once before answering, or answer `kept: true` with the version written and publish that.
- **H8. Quitting within 2 s of a change loses it.**
  - **Where:** `Does::Quit` in `jaw/src/palette.rs` quits without sending what's pending.
  - **Fix:** on quit, send the pending layout and wait briefly for its answer.
- **H9. The 16 slots can't be freed.**
  - **What happens:** nothing forgets an interface's layout. Once 16 names have kept one, a 17th interface is refused for good. A script or test client that greets the owner's jawd with a name of its own takes a slot the first time it sets. `jawd status` gives the count but not the names.
  - **Fix:** a command listing the names, and a way to forget one.
- **H10. `jawd status` reports a failed read as no layouts kept.**
  - **Where:** `layout.rs:141`, `unwrap_or_default`.
  - **Fix:** leave the field out, or say it couldn't be read.
- **H11, from before this range. Nothing checks that a later migration never drops or rebuilds a table of jaw's own without copying it.**
  - **What happens:** `interface_state`'s class is a comment, as other tables' are.
  - **Fix:** a test over `MIGRATIONS` that refuses a `DROP TABLE` of a table declared jaw's own unless the same step recreates it and copies its rows.

### Outside this range

- **X1, inconsistency in main's merge code: a chat kept by one merge can be merged away by another, and the first merge is then lost.**
  - **Where:**
    - `merge_direct_chats` (`store/write.rs:1283`) picks among a person's one-to-one chats that aren't being merged away, so it can pick as the one to go a chat that another merge is going into.
    - `chat_merges.keep` is `ON DELETE CASCADE` (`store/schema.rs:394`).
    - When the second merge's last step deletes that chat (`store/write.rs:1375`), the first merge's row goes with it.
  - **What happens,** in my scene:
    - A person has three chats: their new number's (the oldest), their old number's, then their LID's.
    - The LID is joined to the old number, so the LID's chat starts merging into the old number's chat.
    - Before a step runs, the LID is found to be the new number's too. The old number's chat then starts merging into the new number's, which is older.
    - `chat_merges` holds both merges, the old number's chat kept by one and gone in the other.
    - The steps take the old number's chat first, and its last step deletes it and, by the cascade, the LID's merge.
    - The person is left with two one-to-one chats, the new number's and the LID's. Nothing is quarantined and nothing is said.
    - Messages aren't lost, but the conversation stays split.
    - While both merges run, this range's `chat_named` answers a pane on the LID with the old number's chat, the one about to go.
  - **How likely:** it needs a person with two numbers' chats, the newer number's chat older, while their LID's merge is still running. That's rare, but a merge's rows outlive a restart, so the window is as long as the merge takes.
  - **Fix:** in `start_merge`, point merges going into the chat that goes at the chat that stays, `UPDATE chat_merges SET keep = ?1 WHERE keep = ?2`, so no chat is both kept and going. Or have `merge_direct_chats` keep the chat others are merging into. Adopt the scene (printed below) as the test. This is main's code, so it's its own change, with its own audit.
- **X2. jaw-client's `an_interface_that_falls_behind_is_told_to_watch_again_and_ends_as_a_fresh_one` fails under load, on main as on the branch.**
  - It's the gate's one failure. Alone, it failed 1 of 3 on the branch (load 71 to 96) and 1 of 3 on main at `4ca6774` (load 77 to 100); it doesn't always pass alone.
  - `sock-flood` changed that file on main, so the gate should be run again after the rebase.
- **X3. The builder's artifacts are gone.** `/tmp/jaw-istate`, which held the builder's scripts, logs, mutant runs and million-message account, was deleted during the audit, along with my first clone, by an `rm -rf` over `/tmp/jaw-*` from a process that isn't mine.
  - I rebuilt what I needed.
  - I couldn't rerun the builder's 41 mutants; my own 44 switches stand in for them.

## Landing second, after sock-flood

`sock-flood` is on main now, so the builder's two requirements for whichever lands second apply to this branch:
- **The lane:** `layout.get` and `layout.set` go down the connection's lane, in the order sent, with the hello name and the connection's number they were read under.
- **The push:** it stays in the connection's loop and is read only while no reply waits (`owed.waiting` on main). When the queue to the interface is full, it waits.
  - The push must not be dropped, and it must not block the loop on `out.send(…).await` as it does here at `sock.rs:136`. Main's loop keeps a reply waiting instead of awaiting room.
  - The channel holds only the latest layout, so leaving it unread until there's room loses nothing.

**What conflicts:**
- In `sock.rs`, `sock-flood` rewrote the push branch, added `send` and `Owed` for a reply waiting, and added `answer_apart` beside where this branch adds `layouts`.
- `protocol/README.md` conflicts twice.
- Everything under `jawd/src/store/` rebases cleanly.

**After the rebase:**
- these must pass, along with the two-setters scene if adopted:
  - `a_connection_slow_to_read_ends_on_the_latest_layout`, which fails with the push sent by `try_send`;
  - `a_layout_kept_is_pushed_to_its_interfaces_other_connections_and_another_interface_can_read_it`;
- the socket half needs a fresh audit round: publish-by-writer, a slow connection, and `sock-flood`'s drain at a half-close with a layout push waiting.

## Renumbering

Migration 12 holds as 12 today, since main added none. If another branch takes 12 first, renumbering needs three things:
- the number in its line of `migrations.lock`;
- the `// 12:` comment above its text in `store/schema.rs`;
- its text kept last in `MIGRATIONS`.

The text holds no number, so its hash is unchanged. `expected(version)` builds each version's fingerprint at runtime, and the upgrade test's `latest()` counts the lock's lines, so neither needs a change.

I measured it with dummy migrations put before it:
- one schema step, making it 13;
- two schema steps, making it 14;
- one data step, making it 13;
- six mixed, making it 18.

Each built, and the schema, layout, plan and audit tests and the upgrade test passed (load 65 to 89). Each diff was two files: the lock and `schema.rs`.

## Measurements

Clones and binaries:
- `/tmp/jaw-istate-audit-a`: `3de2d04` unmodified, for the gate and the branch's jawd (`3f780cae…`).
- `/tmp/jaw-istate-audit-main`: main at `4ca6774`, for main's jawd (`3f3902ec…`), the renumbering and the trial rebase.
- `/tmp/jaw-istate-audit-m`: `3de2d04` with my switches and scenes, each switch on when `JAW_AUDIT_MUT` names it.

The machine was shared and loaded throughout, so the load is beside every number.

- **The gate at `3de2d04`.**
  - **Round 1 (load 70 to 90):**
    - `check-generated` passed.
    - `lint` and `build` failed on my copied CMake cache for `opusic-sys`, which I fixed by removing that build folder.
    - `make test` stopped at jaw-client's falls-behind test (X2).
    - `check-audit` failed, as it must before this audit: `aee84c8` touches the data layer without a trailer. `3de2d04` doesn't touch it.
  - **Round 2 (load 67 to 83):**
    - `lint` (`go vet`, `cargo fmt --check`, `clippy -D warnings`) and `build` passed.
    - All the Rust tests passed but the same jaw-client test: jawd's 368 (4 ignored), the upgrade test, and jaw's 239.
- **Switches: 44, of which 35 fail a builder test.** The focused runs took the layout, schema, socket and upgrade tests, or the terminal's panes and layout tests, at load 55 to 78.

| Area | Switches | Caught | Survived |
| --- | --- | --- | --- |
| Synced commit, quarantine | 3 | 3 | |
| Publishing and pushes | 7 | 7 | |
| Versions and room | 6 | 6 | |
| The layout's checks | 5 | 2 | panes at 16, bytes at 16,384, bytes past 16,385 (H1) |
| Reading chats | 6 | 4 | the fallback's `NOT EXISTS`, its `user_jid` (T1) |
| Migration 12's text | 10 | 8 | the JSON's `CHECK` at 16,385, no key `CHECK` (H2) |
| The terminal | 7 | 5 | a restore over a split (H3); a restore with panes off |

  - **What the caught switches broke:**
    - **Synced commit, quarantine:** an unsynced commit, a quarantine not answered as an error, and no JSON in quarantine.
    - **Publishing and pushes:** publishing a set that wrote nothing, or one not its own; the channel taking any version or an equal one; pushing to the setter, never pushing, or `try_send`.
    - **Versions and room:** `>=` or any version in the upsert; room always full, never full, or room for a 17th; a get reading only the connection's own layout.
    - **The layout's checks:** a chat in two panes allowed; the keys' pane allowed past the last.
    - **Reading chats:** no merge, no person fallback, no dedupe, the keys moved before the left-out pane.
    - **Migration 12's text:** no `json_valid`, no size `CHECK`, the size at 16,386, the version from 0, names of 0 or 65 bytes, no `STRICT`, no `WITHOUT ROWID`.
    - **The terminal:** no wait, no restart of the wait, the version from the clock alone, jawd's versions ignored, and a lost set not asked again.
  - **Two counting rules:**
    - With a migration-text switch on, two schema tests fail whatever the switch: my hook changes the text where `Runner::run` and `expected()` read it, not where those two tests read it. I didn't count those failures.
    - `WITHOUT ROWID` is caught by the plan test, which can't run in the switched clone (it reads my switches' SQL), so I checked it by the plan's diff.
  - **The restore with panes off** is close to unreachable: with panes off jaw never asks, so an answer can only come if panes are turned off while one is on its way.
- **The survivors against the whole suite.**
  - **jawd's whole suite:** 367 tests, run at 6 threads two at a time, 212 to 220 s a run (load 65 to 80). It left out my scenes and the plan test, and 4 tests are ignored.
    - The three limit switches passed all 367.
    - The two fallback switches failed only `notify::tests::a_burst_in_a_chat_is_one_notification_with_its_newest_message`. That's a timing test: it passed 3 of 3 alone under each (load 67), and it can't reach `chat_named`, whose one caller is `layout.rs:209`.
  - **The terminal's whole suite:** 239 tests, which passed with the split guard removed (40 s, load 77 to 80).
  - **My scenes as written:** eight pass at `3de2d04` (4 s, load 51 to 75). The ninth is X1's, which fails, showing main's bug.
- **Kills around a set:** `kills.py`, a real jawd with `--engine demo` in a folder of its own, SIGKILLed at a random time after a `layout.set`, then started again.

| Runs | Seed | Kill after | Answered | Unanswered, there after restart | Unanswered, not there | Load |
| --- | --- | --- | --- | --- | --- | --- |
| 100 | 7 | 0 to 25 ms | 82 | 12 | 5 | 75 to 80 |
| 150 | 8 | 0 to 10 ms | 82 | 46 | 21 | 72 to 81 |

  Each series' last run is checked only by the final start, which found the newest answered, a whole file and no quarantine.
- **Sets and gets over the socket:** 50 each, on the million-message account at 12 (load 68).
  - `layout.set` took 5.63 ms at p50 and 8.54 ms at p99; `layout.get` took 0.40 and 0.95 ms.
  - `synced_commits` moved by one a set.
- **Kills during the upgrade:** `kill_migration.py`, 24 SIGKILLs at 0 to 1.5 s (seed 11) into starts on copies of the million-message account at 11 (load 69 to 83). Results are under "What was asked, checked".
- **Migration 12's cost:** five rounds alternating, on copies of the million-message account at 11, already started once by main's jawd (load 66 to 73).
  - **The SQL alone:** run with Python's SQLite, from `BEGIN IMMEDIATE` to the end of a `FULL`, `fullfsync` commit, it took 16 to 114 ms (median 32). The same transaction setting only `user_version` took 16 to 41 ms (median 21).
  - **jawd's start, to its socket answering hello:**

| jawd | Its file | To run | Took (median) |
| --- | --- | --- | --- |
| The branch's | at 11 | migration 12 | 1.05 to 1.34 s (1.21) |
| The branch's | at 12 | nothing | 1.00 to 4.31 s (1.14) |
| Main's | at 11 | nothing | 0.90 to 1.78 s (1.17) |

  - The builder measured 0.08 to 0.16 s against 0.15 to 0.23 s at load 80, the same conclusion.
- **The owner's first run from version 1:** my copy of the generated 90 MB file, three runs a side alternating, main's jawd and the branch's, `--engine demo` (load 63 to 72).
  - **The file:** both reached their version (11 and 12), with the integrity check ok and no foreign-key violations, and the same rows in every table: 83,486 messages, 452 chats, 404 people, 629 reactions, 42 receipts, 1,021 tombstones and 20 quarantined. The branch adds my one layout.
  - **The content:** these digests were the same in all six runs:
    - the history's 83,483 messages (`85c42efe7244`);
    - the chats' names, kinds and flags (`150842c1bb23`);
    - reactions (`00b7b849301c`) and receipts (`009040764318`);
    - the search index over history, 72,181 rows (`4fba60cbb13d`);
    - people's JIDs (`7ba011383126`) and the quarantine (`c0f802855c4b`).
  - The demo engine's three live messages carry each start's time, so they're left out of the digests.
  - **Times:** to the socket, main took 0.71 to 1.31 s and the branch 1.10 to 4.71 s. The 4.71 s run was slow throughout: its history took 15.0 s, against 9.3 to 11.8 s for the other five. Indexing took 3.8 to 4.6 s on main and 2.9 to 3.5 s on the branch.
  - **A set afterwards:** it took 6.1 to 8.7 ms (load 64 to 68). After a second start it was there, with a pane on a chat jaw doesn't have left out and named in `dropped`.
- **Plans and costs:** on a copy of the million-message account at 12, with no `sqlite_stat1`, 2,000 runs each (load 61). These were measured with Python's SQLite 3.54.0; jawd bundles 3.53.2. The plans match `store/plans.txt`.

| Statement | Plan | p50 | p99 |
| --- | --- | --- | --- |
| A chat's own JID (`read.rs:675`) | `chats` by its `id` index, `chat_merges` and `chats` by rowid | 11.8 µs | 16.3 µs |
| A person's other JID (`read.rs:686`) | `person_jids` by `jid`, `chats_by_person`, `chat_merges` by rowid; no sort | 17.1 µs | 207.7 µs |
| The layout kept, or its version | `interface_state` by its primary key | 3.3 µs | 4.3 µs |
| Rows, for room (`store/write/layout.rs:51`) | `SCAN interface_state`, 16 rows at most | 2.7 µs | 3.7 µs |
| `jawd status` | `SCAN interface_state`, 16 rows at most | 3.8 µs | 4.9 µs |
| The upsert, uncommitted | | 2.8 µs | 3.7 µs |

  The two scans are on the plan test's list of allowed scans (`store/plans.rs:67`, `:71`), each with its reason.

## The scenes to adopt

The T1 scene and its helpers, for the test module of `jawd/src/layout.rs`. Its imports are already there.

```rust
    fn exists(home: &Path, id: &str) -> bool {
        let db = Connection::open(home.join("jaw.db")).unwrap();
        db.query_row("SELECT count(*) FROM chats WHERE id = ?1", [id], |r| r.get::<_, i64>(0)).unwrap() == 1
    }

    fn panes_of(saved: &Saved) -> Vec<String> {
        saved.layout.as_ref().map(|l| l.chats().into_iter().map(str::to_string).collect()).unwrap_or_default()
    }

    async fn merges_run(store: &Store, home: &Path) {
        for _ in 0..50 {
            if count(home, "chat_merges") == 0 {
                return;
            }
            store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
        }
        panic!("the merge never finished");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_merge_run_to_its_end_leaves_each_pane_on_a_chat_row_that_exists() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        // Rui's LID chat first, so it has the lower n, then his number's chat and the group.
        let changes = vec![
            Change::Chat(Chat { id: RUI_LID.into(), kind: Some("direct"), ..Chat::default() }),
            Change::Chat(Chat { id: RUI.into(), kind: Some("direct"), ..Chat::default() }),
            Change::Chat(Chat { id: CLIMBING.into(), ..Chat::default() }),
        ];
        store.apply(Lane::Live, changes).await.unwrap();
        let layouts = Layouts::new(store.clone());
        let three = layout(2, split(How::Across, 500, chat(RUI_LID), split(How::Down, 500, chat(CLIMBING), chat(RUI))));
        let saved = layouts.set("jaw", 1, asked(1, &three)).await.unwrap();
        assert_eq!((saved.kept, saved.dropped.len()), (Some(true), 0), "two chats, before they're one person's");
        store
            .apply(Lane::Live, vec![Change::Person(Person { jids: vec![RUI.into(), RUI_LID.into()], ..Person::default() })])
            .await
            .unwrap();
        assert_eq!(count(home.path(), "chat_merges"), 1, "the merge has begun");
        let during = layouts.get("jaw", Value::Null).await.unwrap();
        assert_eq!(panes_of(&during), [RUI, CLIMBING], "during the merge: {during:?}");
        assert_eq!(during.dropped, [RUI.to_string()], "the second pane on Rui, as it named him");
        assert_eq!(during.layout.as_ref().unwrap().focused, 1, "the keys to the pane before it");
        // A pane naming Rui by a device JID while his LID chat, the older, is being merged away.
        let device = layout(0, split(How::Across, 500, chat("351913333333:7@s.whatsapp.net"), chat(CLIMBING)));
        let by_device = layouts.set("jaw-mac", 1, asked(1, &device)).await.unwrap();
        assert_eq!(panes_of(&by_device), [RUI, CLIMBING], "never the chat being merged away: {by_device:?}");
        merges_run(&store, home.path()).await;
        assert!(!exists(home.path(), RUI_LID), "the merged-away chat's row is gone");
        for interface in ["jaw", "jaw-mac"] {
            let after = layouts.get(interface, Value::Null).await.unwrap();
            for pane in panes_of(&after) {
                assert!(exists(home.path(), &pane), "{interface}'s pane on {pane}, which has no row: {after:?}");
            }
        }
        let after = layouts.get("jaw", Value::Null).await.unwrap();
        assert_eq!((panes_of(&after), after.dropped.clone()), (vec![RUI.to_string(), CLIMBING.into()], vec![RUI.into()]));
        // A layout naming only his LID, once its chat is gone, opens his number's.
        let lid = layouts.set("jaw", 1, asked(2, &layout(0, chat(RUI_LID)))).await.unwrap();
        assert_eq!((lid.kept, panes_of(&lid)), (Some(true), vec![RUI.to_string()]));
    }
```

H1's scene, in the same module:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn exactly_16_panes_and_16_kib_are_kept_and_one_past_each_is_error_5_with_nothing_quarantined() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[]).await;
        let tree = |n: u32| (1..n).fold(chat("0@g.us"), |tree, k| split(How::Down, 500, tree, chat(&format!("{k}@g.us"))));
        let sixteen = layouts.set("jaw", 1, asked(1, &layout(0, tree(16)))).await.unwrap();
        assert_eq!(sixteen.kept, Some(true), "16 panes");
        let e = layouts.set("jaw", 1, asked(2, &layout(0, tree(17)))).await.unwrap_err();
        assert_eq!(e.code, code::LIMIT, "{}", e.message);
        // One pane whose JID makes the compact JSON exactly BYTES_MOST bytes, then one byte more.
        let sized = |bytes: usize| {
            let empty = serde_json::to_string(&layout(0, chat("@s.whatsapp.net"))).unwrap().len();
            layout(0, chat(&format!("{}@s.whatsapp.net", "9".repeat(bytes - empty))))
        };
        assert_eq!(serde_json::to_string(&sized(BYTES_MOST)).unwrap().len(), BYTES_MOST);
        let at_most = layouts.set("jaw", 1, asked(3, &sized(BYTES_MOST))).await.unwrap();
        assert_eq!(at_most.kept, Some(true), "16,384 bytes");
        let e = layouts.set("jaw", 1, asked(4, &sized(BYTES_MOST + 1))).await.unwrap_err();
        assert_eq!(e.code, code::LIMIT, "{}", e.message);
        assert_eq!(count(home.path(), "quarantine"), 0, "refused before the writer, never quarantined");
        assert_eq!(layouts.status().refused, 2);
        assert_eq!(layouts.status().largest_bytes, BYTES_MOST as u64);
    }
```

X1's scene, which fails on main's merge code today. It's written for the same module, with the helpers above, but it belongs with the merge's tests once its fix is made.

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn a_chat_kept_by_one_merge_and_merged_away_by_another_leaves_the_person_one_chat() {
        const NEW: &str = "351910000002@s.whatsapp.net";
        const OLD: &str = "351910000001@s.whatsapp.net";
        const LID: &str = "100000000009@lid";
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        // The new number's chat is the oldest, then the old number's, then the LID's.
        let changes = vec![
            Change::Chat(Chat { id: NEW.into(), kind: Some("direct"), ..Chat::default() }),
            Change::Chat(Chat { id: OLD.into(), kind: Some("direct"), ..Chat::default() }),
            Change::Chat(Chat { id: LID.into(), kind: Some("direct"), ..Chat::default() }),
        ];
        store.apply(Lane::Live, changes).await.unwrap();
        // The LID's chat starts merging into the old number's; before a step runs, the LID turns out
        // to be the new number's too, and the old number's chat starts merging into the new one's.
        let join = |a: &str, b: &str| Change::Person(Person { jids: vec![a.into(), b.into()], ..Person::default() });
        store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
        store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
        assert_eq!(count(home.path(), "chat_merges"), 2, "the old number's chat is kept by one merge and gone in the other");
        merges_run(&store, home.path()).await;
        assert_eq!(count(home.path(), "chats"), 1, "one person, one one-to-one chat");
    }
```

At `3de2d04`, it fails at its last line with 2 chats.

## Verdict

Nothing in this range loses data or leaves jaw.db inconsistent. Before landing:
1. Add T1's test: a pane named by a person's other JID never lands on a chat being merged away, through a merge run to its end.
2. Rebase onto `origin/main` (`4126db2`), redoing `sock.rs` over the lane as "Landing second, after sock-flood" says, and run the gate there.
3. Have a short second audit of the rebased branch check two things: that T1's test fails with each guard switched off, and the socket half again.

H1 to H11 can stand as follow-ups; H1 and H2 are cheap to add with T1. X1 is in main's merge code, and needs its own change and audit. Once approved, commit this note and give `aee84c8` the trailer `Data-Audit: docs/audits/2026-10-26-interface-state.md`; `3de2d04` touches nothing on the data layer's list.

Verdict: changes needed

# Verdict of round two (dd348e0)

Nothing in `4126db2..dd348e0` loses data or leaves jaw.db inconsistent, and the rules it touches are tested:
- T1's, through a merge run to its end, failing with either of the person fallback's guards off;
- the socket half's, each failing a builder's test without it, the drain at a half-close failing `sock-flood`'s, and a second hello's subscription failing only my P3 (H14);
- round one's, on the rebased code, in the gate and through 300 kills.

H12 to H14 are follow-ups, with P1, P1b and P3 to adopt. H14 is a corner of the push's routing, not of what jaw.db keeps. Round one's H1 to H11 stand, and H1 and H2 needn't come first. X1, in main's merge code, needs its own change and audit.

Commit this note, and give `f3c16eb` and `dd348e0` the trailer `Data-Audit: docs/audits/2026-10-26-interface-state.md`; `70f7ebe` touches nothing on the data layer's list. If migration 12 is renumbered when it merges, its lock line and its comment are all that change.

Verdict: approved
