# Audit: M8's failure states

# Round two: `817bc02..d40c721`, round one's I1, T1 and T2, and the phone-offline rule rebuilt

Range: `817bc02..d40c721` in `failure-states` (worktree `~/src/jaw-failures`), 28 October 2026, round two. Auditor: database-expert. Line numbers are at `d40c721`; paths are under `crates/jawd/src/` unless they say otherwise. Round one, further down, is kept as written; its hashes are from before the branch was rebased onto `817bc02`.

The 17 commits are round one's five data-layer commits rebased (`125e40a`, `3c46f84`, `56d14de`, `00754c2`, `035b266`), the terminal's and the tests' (`3b96f2a`, `0cf08fd`, `f76642a`, `518d7bf`, `5712a0c`, `d92f9ce`), and this round's six:
- `3115df6`: T2. One list, `kept`, of what jawd must store, read by the running client's path and a replaced client's, with a table test of the seven kinds.
- `7bfb786`: O3. `retire` acts only for the client running now, and jawd's own logout goes through it.
- `6f35023`: I1. Every connection state but connected and a keepalive timeout cuts a catch-up short, and so does jawd's own logout.
- `8af6705`: T1. A send test with the failing engine crashing.
- `4f1879d`: O1. Sync counts the stops it has said, and the supervisor starts the next engine only once sync has said the last.
- `d40c721`: O2 and O5. The phone-offline rule rebuilt.

## In short

Approved. Round one's I1, T1 and T2 are closed, each with a test that fails with its rule taken out, and nothing this round adds loses data or leaves jaw.db inconsistent. Three small rules are left without tests, and none of them guards data (T3 to T5). The keepalive bug the builder found is real, is on main, and should get a branch and an audit of its own before release.

- **I1 is closed, end to end.** In `jawd demo --fail logout`, the phone logs jaw out halfway through a catch-up of 20,000 messages:
  - a message sent while logged out waits, pending, with 0 attempts;
  - linking again, jawd says online at once, and that message and a new one are sent within 0.24 s;
  - killed with `SIGKILL` while catching up, while logged out, and after linking again, each run again sends every message once, loses and duplicates nothing, and `quick_check` is ok.
  - With the I1 rule taken out, jawd still says `logged_out` 40 s after linking again, when the run ends, and both sends stay queued.
- **T1 and T2 are closed.** T1's test fails with `engine_restarting` counted as connected. T2's table test fails with any of the seven kinds taken out of `kept`, with a replaced client handing nothing over, or with it handing everything over.
- **The supervisor waiting on sync can't deadlock or be held for good.** A restart waits for as long as sync is held and no longer: 0.29, 1.56 and 4.02 s for jaw.db's writer held 0, 1.5 and 4 s past the stop, with a 0.3 s pause. Only the writer can hold sync, and the writer never waits on sync or an engine.
- **The send in the moment between an engine's stop and sync's word** still counts an attempt if it lands there, but I couldn't make one land: 0 attempts in 10 tries with the writer held across the stop. It's acceptable as a follow-up (O11).
- **The late-logout guard never keeps a device WhatsApp logged out.** It ignores only a client already replaced, whose device the replacing already deleted. Raced with the phone's logout 40 times under `-race`, jawd's own logout renews exactly once.
- **The phone-offline rule can't stick or flap.** A question is given up after 2 minutes, the patience starts again at every reconnect, restart and catch-up's end, and a connection that drops every 0.4 s never says offline. It can be wrong for at most 2 minutes, and only in edge cases (O13). If real phones don't send the "more remains" field, it's never said at all, which fails safe; I judge that acceptable for release on conditions (O12).
- **A `session.db` from a newer whatsmeow** is left byte for byte, with no file beside it, whatever jawd asks.
- **The one changed statement** is pinned, searches `messages_by_time`, sorts nothing, and costs 0.015 ms, or 1.1 ms with 10,000 failed messages of yours before the oldest WhatsApp has.
- **The keepalive bug is confirmed.** After a keepalive timeout jawd says "connecting", the engine never passes on whatsmeow's `KeepAliveRestored`, and a send waits, with nothing tried, until the connection drops and comes back, however long that is. Nothing is lost. "Outside this range" says what a fix needs.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gaps (none blocks)

- **T3. A keepalive timeout during a catch-up leaves it running: no test.**
  - **The rule:** `6f35023`'s exception at `sync/mod.rs:708`, since whatsmeow's connection goes on after a keepalive timeout.
  - **The switch:** cutting the catch-up short on `KeepaliveTimeout` too passes the 142 tests of sync, send and the engine.
  - **Why it doesn't block:** taken out, a keepalive timeout ends the catch-up early. The rest arrives as live messages, notified one by one rather than in one summary, and the chat list stops holding its order. The account, the connection and jaw.db are the same either way.
  - **Fix:** a sync test: a preview, a keepalive timeout, messages, then `OfflineDone`. It expects `catching_up` held until `OfflineDone` and the summary notice after it. Take it with the keepalive fix, which changes this same path.
- **T4. A logout forgetting which chats the phone keeps more of: no test.**
  - **Where:** `forget_phone` (`sync/mod.rs:118-121`) clears `more_on_phone`.
  - **The switch:** leaving it uncleared passes every test; `a_question_put_to_the_phone_before_a_logout_is_forgotten…` checks only the question.
  - **Why it doesn't block:** it matters only when another account is linked in the same run with a chat in common. A question there would be counted on the old phone's word, and `phone_offline` could show wrongly for up to 2 minutes.
  - **Fix:** in that test, assert `more_on_phone` is empty after the logout.
- **T5. The supervisor ending when sync has gone: no test.**
  - **Where:** `engine/supervisor.rs:219`, the `events.closed()` branch of the wait for sync.
  - **The switch:** taking it out passes every test.
  - **Why it doesn't block:** sync gone means jawd stores nothing anyway. Without the branch, the supervisor would wait in `restarting` rather than stop. jawd's own stop still ends it at once (`told`).
  - **Fix:** a supervisor test that follows, drops the receiver, stops the engine and expects `Stopped`.

### Unpredictable performance

None.
- **The changed statement:** `history_more`'s oldest-message query (`sync/mod.rs:755-757`, pinned at `store/plans.txt:1683`) plans as `SEARCH c USING COVERING INDEX sqlite_autoindex_chats_1 (id=?)` then `SEARCH m USING INDEX messages_by_time (chat=?)`, with no temporary sort.
  - The new filter costs a row lookup for each message of yours, pending or failed, older than the oldest WhatsApp has, about 0.13 µs each (Measurements).
  - It's an interactive read, run once each time you move past a chat's oldest message.
- **`heard_from`** adds one lock and two string comparisons per message and receipt in sync's loop.
- **The supervisor's wait** adds nothing to the writer.

### Other (follow-ups; none blocks)

- **O11. A send in the moment between an engine's stop and sync's word of it counts an attempt.**
  - **Where:** the supervisor drops the engine's commands when it exits (`engine/supervisor.rs:194`), but the connection says online until sync has drained the stopped engine's last events and said `engine_restarting`. A send whose attempt starts then gets `Unavailable`, which `attempt` (`send/outbox.rs:474`) counts as a failed try.
  - **How long:** at most the drain of the engine's leftover events (the channel holds 1,024) and one account commit, plus the push reaching the presence task. Normally that's well under a millisecond.
    - I held jaw.db's writer across the stop to widen it and made the send then: 0 attempts counted in 10 of 10, since sync's commit is ahead of the send's in the writer's queue, so the connection says restarting before the send's attempt looks.
  - **Cost if hit:** one counted attempt marks the message as one that may have gone (`:434`). A later refusal then holds it unconfirmed for a day rather than failing it at once. It's never lost or sent twice, since every try carries its one ID. Before `125e40a` every send during a restart did this; the branch narrowed it to this moment.
  - **Judgement:** acceptable.
  - **Fix, cheap and exact:** `Unavailable` is made only when no engine took the command (`engine/supervisor.rs:127`, `send/mod.rs:281`). `attempt` could wait for the connection to change without counting it, rather than `Step::Failed`.
- **O12. If phones never set the "more remains" field, `phone_offline` is never said.**
  - `more_on_phone` is filled only from a conversation's `end_of_history_transfer_type` (`convert/history.rs:126`). Without it, no question counts, and the rule is silent: never wrong, and nothing lost, since the request goes either way and its answer is stored when it comes.
  - See "The questions asked" for the judgement and what the spare-number session should check.
- **O13. The rule can still say offline wrongly, for at most 2 minutes, in three edge cases.** Each ends early if the phone sends anything.
  - A request the phone drops or ignores in a chat it said it keeps more of.
  - An anchor the phone no longer has. jawd doesn't remove expired disappearing messages yet, so in a chat with them on, its oldest row can be one the phone dropped long ago. Deleted-for-you rows leave `messages` (`store/write.rs:1027`), and revoked ones stay as placeholders the phone keeps too, so neither is a bad anchor.
  - A chat the phone names differently in two blobs (its LID in one, its number in another). A "more" under one name then isn't undone by a "none" under the other.
  - **Fix, when it matters:** leave out of the anchor rows with `expires_at` in the past, once expiry lands.
- **Round one's follow-ups:**
  - Closed: O1 (`4f1879d`), O2 and O5 (`d40c721`), O3 (`7bfb786`).
  - Standing as written:
    - O4, the newer-`session.db` mode answering commands with `retry: false` (`engine/internal/wa/engine.go:466`);
    - O6;
    - O7;
    - O8, votes and reactions decrypted with the current client (`:378`, `:384`);
    - O9;
    - O10, `--phone-patience` taking `Duration::from_secs_f64` (`main.rs:245`).

### Outside this range

- **The keepalive bug, on main: sends wait for as long as a keepalive timeout's connection lives on.**
  - **What happens:**
    - The engine says `KEEPALIVE_TIMEOUT` (`engine/internal/wa/engine.go:314`), which jawd shows as `connecting` (`sync/mod.rs:687`). The outbox counts `connecting` as not connected (`send/presence.rs:18-20`).
    - whatsmeow keeps the connection after a keepalive fails, and dispatches `KeepAliveRestored` once a ping gets through again (`keepalive.go` at the pinned whatsmeow). It reconnects only if pings fail for 3 minutes.
    - Nothing in the engine handles `KeepAliveRestored`, on main (`817bc02`) or here, so only a `Disconnected` and a reconnect bring `online` back. On a connection that recovered, that can take hours.
  - **What I saw:**
    - In jawd: after a keepalive timeout, a message sent was still pending 3 s later, its outbox row at `('queued', 0)`, with nothing tried. Only a `Connected` sent it.
    - In the engine: a probe handing `KeepAliveRestored` to the running client found nothing sent to jawd.
  - **Cost:** nothing is lost. Arrivals are stored and acked as usual, and sends are durable in the outbox and go at the next reconnect. But the header says connecting and sends wait while WhatsApp is reachable, for as long as the connection lives.
  - **What a fix needs:**
    - **The engine passes the restore on**, in `say`, so it comes from the running client only, and T2's table lists it among what a replaced client doesn't hand over. It can go as `CONNECTED`, which jawd already maps to online but which also rewrites the account and the JIDs, idempotently. Or it can go as a new `KEEPALIVE_RESTORED` state in `engine.proto`, which jawd maps to online with no writes.
    - **jawd goes back to `catching_up` during a catch-up.** `apply` returns early for online while catching up (`sync/mod.rs:712-714`). Since `6f35023` a keepalive timeout no longer ends a catch-up, so after the fix it would still leave `connecting`, and sends waiting, until the catch-up ends, unless the restore puts back `catching_up` with its `done` and `total`.
    - **The order holds.** whatsmeow dispatches `KeepAliveTimeout` and `KeepAliveRestored` each on a goroutine of its own, so either can reach the engine after a later `Disconnected` or `Connected`. The engine should say restored only while the client is connected, and drop a timeout older than the last connection state it said. Otherwise jawd can say online with no socket, or connecting with a good one.
    - **Tests:**
      - Go: a restore from the running client says connected; from a replaced client, nothing.
      - Rust: a send waiting after a timeout goes once restored; during a catch-up the state is `catching_up` again and its summary still comes, which also closes T3.
      - End to end: a `--fail keepalive` in the failing engine.
    - It touches `engine/internal/wa`, and with a new state `proto/engine`, so it needs its own audit.
- **The app-state refresh is keyed on the supervisor's count, which can already include a stop sync hasn't said.**
  - **Where:** `sync/mod.rs:411-413` reads `engine.status().restarts` when a `Connected` is handled.
  - **What happens:** an engine that connects and stops before sync has handled its `Connected` (a crash loop just after connecting) marks the next engine as refreshed. That engine's contacts, groups, LIDs and avatars aren't fetched; its app state is, since `resync_again` keeps trying while it runs.
  - **Fix:** with `4f1879d`, sync's own count of the stops it has said (`said`) names exactly the engine an event came from, so `refreshed != Some(said)` makes it exact.
- **Round one's "Outside this range" items stand:** no `session.lock` or backup of `session.db`, no guard on `--engine demo`, and other refusals of `session.db` still exit.

## The questions asked

- **I1 closed:**
  - **In sync:** every connection state but connected and a keepalive timeout ends a catch-up (`sync/mod.rs:708-710`, through `cut_short` at `:144`), and so does jawd's own logout (`:228`). Two sync tests and one send test fail with the first rule out, and one sync test with the second.
  - **End to end:** with a real catch-up through `jawd demo` and `SIGKILL` at each step (Measurements):
    - a message sent while logged out waits with 0 attempts, survives a kill, and goes once after;
    - linking again says online at once;
    - nothing is held, lost or duplicated.
  - **Notifications** can't be watched from a real binary without reaching the owner's desktop. The builder's send test, which a message arriving after linking again must notify, fails with the rule out.
- **T1 and T2 closed:**
  - T1's test (`send/tests.rs:847`) fails with `engine_restarting` counted as connected, the only test that does.
  - T2's (`engine/internal/wa/engine_test.go:371`) fails for each of the seven kinds taken out of `kept` alone, and for a replaced client handing over nothing or everything.
  - `kept` is now the one list both paths read (`engine.go:211-217`, `:221-227`, `:276-299`).
- **The supervisor waiting on sync:**
  - **No deadlock:**
    - Sync's loop waits on nothing but jaw.db's writer, for one account commit at a `Connection` event (`sync/mod.rs:396-408`).
    - The inbox never blocks it (`store/mod.rs:655-657`).
    - The writer never waits on sync or an engine: acks are unbounded sends (`engine/mod.rs:46-48`), and a durable write handed back between inbox groups is written alone once no more rows come (`store/mod.rs:1184-1194`).
    - A logout's synced commit, then a stop, with the writer held a second: the next engine ran 0.11 s after the writer let go, and the logout was kept.
  - **Not held for good:** sync is held only while the writer can't commit. That's a checkpoint wait, which the design bounds at 1 s, or a jaw.db that can't be written (a full disk), when an engine couldn't have anything acked anyway. The restart follows as soon as the writer does, and a stop ends the supervisor at once whatever it waits for.
  - **The delay:** a restart starts after the longer of the pause and the time sync is held, and no later: measured 0.29, 1.56 and 4.02 s for 0, 1.5 and 4 s held. Normally sync is held for the drain of at most 1,024 leftover events and one live commit.
  - **The send in the moment:** O11, acceptable.
  - **Taking out the wait** fails the supervisor's test and sync's held-writer test. Going back to acting on `Restarting` alone passes them, as the builder says, since the supervisor now holds that state until sync has said it; the count is belt and braces.
- **The late-logout guard (`engine.go:245-247`):**
  - **It never keeps a device WhatsApp logged out.** It ignores a `LoggedOut` only from a client that isn't running now, or a second one from the same client.
    - A client is replaced only by `renew`, which first deletes every device in `session.db`, so an ignored client's device is already gone.
    - If `renew` can't delete one, the client stays running and retired. Its device, if whatsmeow's own delete failed too, is logged out again by WhatsApp at the next start, as before this branch.
  - **jawd's own logout racing the phone's:** 40 runs under `-race` each renewed exactly once, said `LOGGED_OUT` at most once, and left no device.
  - Taking out the running-client check fails the builder's test.
- **The phone-offline rule:**
  - **It asks only before a message WhatsApp has:** the query leaves out yours still pending or failed (`sync/mod.rs:755-757`); O13 is the edge. It waits only in a chat a blob said, in this run, the phone keeps more of (`:770-771`, `:125-133`).
  - **Anything the phone sends answers it:** a message or receipt whose sender is the account's own JID with no device (`:103-109`), since the phone is device 0, so history notifications and peer answers count as the messages they are.
  - **The 20 s counts only online,** from the later of the question and the connection's last coming online (`:505-507`, `:321-327`).
  - **A question is given up 2 minutes after it was asked** (`:524`, `:559-561`). It's noted before its request and dropped if the request fails (`:771-777`).
  - **It can't stick:** a question lasts at most 2 minutes, and a new one needs you to move past a chat's oldest again.
  - **It can't flap:** offline comes only after a whole patience online, and every reconnect, restart and catch-up's end starts that again. My probes found no `phone_offline` on a connection dropping every 0.4 s with 0.8 s of patience. After a catch-up, offline came 0.70 s after its end with 0.7 s of patience.
  - **When nothing older exists:** the phone says so in the blob ("none remain"), the chat leaves `more_on_phone`, and its silence counts for nothing.
  - **Every part has a test that fails without it** but `forget_phone`'s clearing: 12 of 13 switches fail a test, and the 13th is T4 (Measurements).
  - **The "more remains" limit, judged:** acceptable for release on three conditions.
    - The spare-number session checks it before release notes say older history waits for the phone. It should look at:
      - whether `INITIAL_BOOTSTRAP`, `RECENT`, `FULL` and `ON_DEMAND` conversations carry `end_of_history_transfer_type`;
      - what the phone sends when nothing older is left;
      - whether it answers before a message it no longer has.
    - The design's "The phone is offline" line is read as best effort until then.
    - If phones don't send the field, the fallback should keep the rule's property, that only a question known to have an answer counts. For example, count a question in a chat whose last on-demand answer in this run came back full (as many messages as asked), which says more remain without the field. Don't go back to counting every question.
- **The newer `session.db`:** never written. After link start, logout, a history request, contacts, an app-state resync and link cancel, each refused, the file was 167,936 bytes with the same SHA-256 as before, with no `-wal` or `-shm` left. `035b266`'s path is untouched by this round: `do` refuses before the logout branch when no session is open.
- **Plans:** one changed statement, pinned, and the plan test passes on the head (see "Unpredictable performance").

## Measurements

The machine's load average was 28 to 71 throughout. Everything was in one clone, `/tmp/jaw-audits/failure-states-2/jaw`, built with `CARGO_INCREMENTAL=0` under `nice`, with `TMPDIR`, `GOCACHE` and every jawd home in the same folder. The Go engine ran only in its tests, and `JAW_ENGINE` named nothing.

- **The gate, again, at the head:**
  - Go: `go test -race ./...` ok (`link` 2.1 s, `wa` 2.4 s); `go vet` ok; `gofmt -l` empty.
  - jawd's unit tests: 573 passed, 2 failed under load, and each of those passed 3 of 3 alone:
    - `feed::tests::a_lone_change_after_quiet…`, as in round one, with a 53 ms outlier;
    - `sock::tests::a_connection_full_when_its_change_is_answered…`, a timeout. The range touches `sock.rs` only in `history_more`'s arguments.
  - jawd's integration tests: 4 of 4. `failures.rs`, end to end: 9 of 9 in 55 s. The plan test passes.
  - `check-parity` ok. `check-audit` names the 11 data-layer commits, round one's five and this round's six, as missing the trailer.
  - The branch has 17 distinct patches, and the reflog shows one rebase onto main and one reword, nothing doubled.
- **I1 end to end:**
  - **Setup:** `jawd demo --fail logout`, with 20 chats and 2,000 messages of history, then a catch-up of 20,000 messages with one in flight.
  - **The driver:** a socket client that sends with `send.text`. `SIGUSR1` has the phone log jaw out 0.5 s into the catch-up. `link.start` and a second `SIGUSR1` link it again.
  - **The checks:** after a `SIGKILL`, `jawd demo` ran again on the folder. jaw.db was then read with jawd stopped: `quick_check`, `(chat, id)` duplicates, the inbox, the quarantine and the outbox.

  | Run | Before the kill | After running again |
  | --- | --- | --- |
  | No kill | online 0.01 s after linking again; the waiting message and a new one sent within 0.05 s | all 3 read, each once; outbox empty; `quick_check` ok |
  | Killed while catching up | the message sent then `('queued', 0)`; 2,103 messages | sent once; 2,105 messages; outbox empty; ok |
  | Killed while logged out | the message sent then `('queued', 0)` | it went, and was read, once; 2,101 of 2,101; ok |
  | Killed after linking again | all 3 sent | each once; 2,144 of at least 2,140; ok |
  | I1's rule out, no kill | `logged_out` said for 40 s after linking again; both sends `('queued', 0)` | both still queued at the stop, not lost |

- **Probes** (tests of my own in the clone):
  - **The restart:**
    - With jaw.db's writer held 0, 1.5 and 4 s past a stop and a 0.3 s pause, the next engine ran 0.29, 1.56 and 4.02 s after the stop.
    - With a logout, then a stop, behind a writer held 1 s, the next engine ran 0.11 s after it let go. The logout was kept, and nothing said reconnecting over `logged_out`.
  - **The send in the moment:** writer held across a failing engine's crash, with a send made then. Over 10 tries, `attempts` was `[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]`, and each message went once.
  - **The keepalive:**
    - 3 s after a keepalive timeout: `pending`, `('queued', 0)`, 0 sends tried; then sent once a `Connected` came.
    - Sync stays `connecting` with nothing else to move it.
    - In Go, the engine sends jawd nothing for `KeepAliveRestored`.
  - **The phone:**
    - With 0.8 s of patience and a 3 s give-up, a connection dropping every 0.4 s never said `phone_offline`; the question was given up, and nothing was said in the 1.6 s after.
    - A question asked while catching up went offline 0.70 s after the catch-up ended, with 0.7 s of patience.
  - **The logout race:** 40 runs under `-race`, each renewing once, with `LOGGED_OUT` said at most once and no device left.
  - **A newer `session.db`:**
    - Set up as a newer whatsmeow leaves it: version 1000/1000 and a table of the newer one's.
    - Before and after: `session.db` 167,936 bytes, SHA-256 `40543e2f41e4c1f2…`, and no other file.
  - **The changed statement**, on a copy of a demo jaw.db (best of 20): 0.015 ms; 0.13, 1.08 and 13.4 ms with 1,000, 10,000 and 100,000 failed messages of yours before the oldest WhatsApp has.
- **Mutation tests.**
  - **How they ran:** each switch was read from an environment variable in one build, and run against the 142 tests of sync, send and the engine (Rust) or the package's tests (Go).
  - **Load flakes:** three tests failed once each under switches that don't touch them, and each passed alone with its switch on and off:
    - `send::tests::a_restart_gives_a_message_waiting_for_a_receipt…`;
    - `send::tests::reading_moves_the_read_position…`;
    - the builder's I1 send test, `linked_again_after_a_logout…`, once in the 19 runs that didn't take its rule out.

  | Switch | Result |
  | --- | --- |
  | M1 no catch-up ended by a connection's end | 3 fail: the logout, the dropped connection, and the send test linking again |
  | M2 jawd's own logout ends no catch-up | `a_logout_jawd_asks_for_ends_a_catch_up…` fails |
  | M3 a keepalive timeout ends one | survives (T3) |
  | M4 `Connected` ends one | sync's held-writer test fails |
  | M5 `engine_restarting` counted as connected | T1's test fails |
  | M6 the next engine doesn't wait for sync | the supervisor's test and sync's held-writer test fail |
  | M9 no end when sync's receiver goes | survives (T5) |
  | P1 the anchor may be pending or failed | `older_history_is_asked_for…` fails |
  | P2 every chat counted as keeping more | the same fails |
  | P3 noted after the request | the same fails |
  | P4 not dropped when the request fails | the same fails |
  | P5 "none remain" never heard | `a_history_blob_says_which_chats…` fails |
  | P6 a logout keeps `more_on_phone` | survives (T4) |
  | P7 any of your devices counts as the phone | `anything_the_phone_itself_sends…` fails |
  | P8 receipts don't count | the same fails |
  | P9 messages don't count | 3 fail |
  | P10 time offline counts | 2 fail |
  | P11 no giving up | `a_question_is_given_up…` fails |
  | P12 the patience never starts again | 2 fail |
  | P14 every end-of-history type counts as more | `a_history_blob_says_which_chats…` fails |
  | G1 to G7 each kind out of `kept` | T2's table test fails each time, with the kind's own test |
  | G8 a replaced client hands nothing over | T2's table test and the logout test fail |
  | G9 a replaced client hands everything over | the same fail |
  | G10 `retire` for any client | `TestALateLogoutFromAClientReplacedTwice…` fails |

# Round one: `99d125f..6b95b6b`, kept as written

# Audit: M8's failure states

Range: `99d125f..6b95b6b` in `failure-states` (worktree `~/src/jaw-failures`), 28 October 2026, round one. Auditor: database-expert. Line numbers are at `6b95b6b`; paths are under `crates/jawd/src/` unless they say otherwise. Nothing in the worktree was uncommitted but this note.

The five data-layer commits, which `make check-audit` names on the rebased branch too:
- `8646519`: the connection is `engine_restarting` from an engine's stop until the next engine speaks, and a catch-up the stop cut short ends.
- `6b0d84a`: the failing demo engine (`engine/failing.rs`, `--fail`).
- `86f4685`: `phone_offline` after a question to the phone goes 20 s unanswered; the outbox counts it as connected.
- `477b6a2`: after a logout the engine deletes any device left in `session.db` and swaps in a client on a fresh device; a retired client's connection events are dropped.
- `b6993f7`: a `session.db` from a newer whatsmeow is said as `CLIENT_OUTDATED`, and the engine serves jawd instead of exiting.

The other six (`1e849f2`, `1194d79`, `357135f`, `29fb49b`, `37ceafc`, `6b95b6b`) are the terminal's and the tests'; I read them where they touch the data (jaw's `history.more`, the end-to-end tests).

## In short

Changes needed: one inconsistency and two missing tests.

- **I1, inconsistency.** A logout doesn't end a catch-up.
  - Link again in the same run, as `477b6a2` now allows, and the fresh device's `Connected` is swallowed by `apply`'s catch-up rule.
  - jawd then pushes `logged_out` for an account that is linked and online.
  - Meanwhile the outbox holds every send, the chat list reorders only every 2 s, and no notification goes out.
  - This lasts until some later `OfflineDone` or a restart. Reproduced.
- **T1, a rule with no test.** Nothing tests that a send made while the engine restarts waits without counting an attempt.
  - With `engine_restarting` counted as connected, the send tests and sync's restart tests pass, and the end-to-end crash test sends nothing during the restart.
  - My kill test shows the difference: 0 attempts at `6b95b6b`, 1 with the mutant.
- **T2, a rule with no test.** The engine still hands over what jawd must store from a retired client, but only messages are tested.
  - Taking `Receipt`, `AppState`, `PushName`, `JoinedGroup`, `GroupInfo` or `Picture` out of `mustKeep` fails nothing.
- **What holds:**
  - Nothing new is acknowledged, and `restarting()` writes nothing.
  - Killed at three points of the restart window, jawd loses, duplicates and counts nothing: a send made during the restart stays queued with 0 attempts through `SIGKILL` and goes after.
  - A `session.db` from a newer whatsmeow is left byte for byte as it was, whatever jawd asks.
  - A logout keeps every message, and `session.db` keeps nothing of the old device that whatsmeow's own logout wouldn't.
  - The failing engine can only wrap the demo engine.
  - No new statement; the one this path runs more often is pinned and searches an index.
  - The trial rebase onto `fa9265c` is clean.
- **The phone-offline rule** can't lose anything or stop a send, but as built it can say "phone offline" wrongly, and then keep saying it. Judged below; follow-ups O2 and O5.

## What I checked

- **The diff and the code around it:**
  - sync's loop, `apply`, `caught_up`, `restarting`, `phone_due` and `phone_said`;
  - the supervisor and `GoEngine`'s `exited`, which waits for the reader to drain, so a stopped engine's last events are in the channel before `Restarting`;
  - the outbox's `attempt` and `retryable`;
  - the feed's and the notifier's catch-up hold, which reads `catch_up.seconds`;
  - the history fetcher and its quarantine;
  - `jaw`'s `ask_for_older`;
  - the Go engine's `handleFrom`, `mustKeep`, `loggedOut`, `renew`, `Run` and `do`.
- **whatsmeow and `go.mau.fi/util` at the pinned versions:**
  - `connectionevents.go`: `LoggedOut` is dispatched on its own goroutine while `Store.Delete` runs, for 401, 403 and 406.
  - `client.go`: `Logout`, `Disconnect`'s 5 s wait for the handler queue, and a deleted store refusing to connect.
  - `sqlstore`: `DeleteDevice`, `NewDevice` storing nothing until pairing, and the schema's cascades.
  - `dbutil.Upgrade`: the refusal comes before any upgrade step; the owner check only reads when no owner is set; the version table is written only when it lacks `compat`.
  - `BuildHistorySyncRequest`'s documentation.
- **Probes of my own** in my clone (`sync/tests.rs`, `engine/internal/wa/audit_probe_test.go`, a socket driver for kill tests), described where they're used.
- **Mutation tests:** 11 switches in Go and 11 in Rust, each against the tests named in Measurements.
- **The gate, again, in my clone:**
  - Go tests with `-race`;
  - jawd's unit tests;
  - the end-to-end `crates/jaw/tests/failures.rs`;
  - the plan test;
  - `check-parity` at both heads;
  - `check-audit` on the rebased branch.
- **The trial rebase**, and the branch's history for anything doubled.

## Findings

### Data loss

None.

### Inconsistency

- **I1. A logout during a catch-up leaves the catch-up running, so linking again in the same run leaves jawd saying "logged out".**
  - **Where:** `sync/mod.rs:594-597` (`apply`'s logout branch) and `:174-186` (`logged_out`, for a logout jawd asked for) end nothing. `:607-609` returns before setting the connection while `catching_up` is set and the new state is `Online`. Only `caught_up` (`OfflineDone`) and `restarting` end a catch-up.
  - **What happens:** reproduced with sync's own harness.
    - Online, then `OfflinePreview`, then `LoggedOut`, then the fresh device's `Connected`.
    - Pushed: `logged_out`, with the account `linked` and `feed::catching_up` still true.
    - So `presence::connected` is false and every send waits, `pending`.
    - The feed's clients reorder the chat list only every 2 s.
    - The notifier delivers nothing until a catch-up ends.
    - The terminal stays on its logged-out screen (`jaw/src/view.rs:508-518` leaves it only for `online` or `catching_up`). Its "link again" sends a `link.start`, which the engine refuses ("an account is already linked").
  - **Why it's reachable now:** before `477b6a2`, linking again needed the engine started again. A logged-out connection never sends the `OfflineDone` that would end its catch-up. whatsmeow documents `OfflineSyncCompleted` only as "after the server has finished sending missed events", and a freshly linked device has missed nothing. The demo's `--fail logout` sends no `OfflineDone` after linking again either.
  - **Nothing is lost:** the sends are in the outbox, synced, and go once the state moves.
  - **Fix:** end the catch-up whenever its connection ends, as `restarting` does: take `catching_up`, set `catch_up.seconds`, clear `caught_chats`.
    - At least on `LoggedOut` and `Unlinked` in `apply`, and in `logged_out`.
    - Better on any connection state that ends the connection (see "Outside this range").
  - **Test:** the sequence above, expecting `online` after the fresh `Connected` and `feed::catching_up` false. My probe `probe_a_logout_during_a_catch_up_then_linked_again` does that and fails at `6b95b6b`.

### Test gap

- **T1. A send made while the engine restarts waits without counting an attempt: no test.**
  - **The rule:** `8646519`'s, and the brief's: sends wait, `pending`, rather than failing against a dead engine.
  - **The switch:** with `EngineRestarting` added to `presence::connected` (`send/presence.rs:18-20`), the send tests and sync's restart tests pass, 68 in all. The end-to-end crash test (`crates/jaw/tests/failures.rs:227`) sends nothing during the restart, so it can't catch it either.
  - **It's observable:** my kill test, with jawd built at that switch, finds the outbox row at `('queued', 1)` where `6b95b6b` leaves `('queued', 0)`. A counted attempt also marks the message "may have gone", so a later refusal waits a day as unconfirmed instead of failing.
  - **Fix:** a send test.
    - `Running::unconnected`, `connection(EngineRestarting)`, send.
    - Assert `pending` with `attempts = 0` after a moment.
    - Then `Online`, and assert it goes.
- **T2. What jawd must store from a retired client is tested only for messages.**
  - **Where:** `engine/internal/wa/engine.go:217-223` (`mustKeep`) must list every kind `handle` keeps (`:263-287`).
  - **Today's list is right:** it covers every kind `handle` keeps. What it drops (`Connected`, `Disconnected`, `KeepAliveTimeout`, `StreamReplaced`, `ClientOutdated`, `ConnectFailure`, `TemporaryBan`, `PairSuccess`, `OfflineSyncPreview`, `OfflineSyncCompleted` and `AppStateSyncComplete`) carries nothing jawd stores.
  - **But nothing pins it:** taking out `Receipt`, `AppState`, or `PushName`, `JoinedGroup`, `GroupInfo` and `Picture` together passes every Go test.
  - **Why it matters:** with `SynchronousAck`, whatsmeow acks an event once the handler returns. A kind dropped by mistake would be acked to WhatsApp and lost for good.
  - **Fix:** a table test handing each kept kind from a retired client and checking it reaches jawd with a seq. Better, one list both `handle` and `mustKeep` read, so they can't drift.

### Unpredictable performance

None. The range adds no statement against jaw.db. `history_more`'s oldest-message query, which jaw now sends whenever you move past a chat's oldest message, is pinned (`store/plans.txt:1652`) and searches `messages_by_time` (chat) with no temporary sort. `renew` reads `whatsmeow_device`, a table of one row.

### Other (follow-ups; none blocks)

- **O1. The restart can go unseen, and then the branch's guarantees don't hold.**
  - **Where:** the supervisor's status is a `watch` (`engine/supervisor.rs:98`, `:186-190`), which keeps only its newest value. `stopped` (`sync/mod.rs:398-407`) acts only if it reads `Restarting`.
  - **When:** sync's loop waits on jaw.db's writer for each `Connected`'s account commit and each logout's synced one (`:328-339`). Held longer than the pause before the next engine (0.5 s at first), the loop reads `Starting` or `Running`.
  - **Reproduced:** writer held 1.2 s, pause 0.3 s.
    - No `engine_restarting` is pushed.
    - The cut-short catch-up isn't ended, and the next engine's `Connected` is swallowed by the catch-up rule, as in I1.
  - **From the code, not measured:** the connection still counts as connected, so a send in the pause goes to a supervisor with no engine. It fails `Unavailable` and counts an attempt, as before the change.
  - **Cost:** nothing is lost, and it needs a writer stall longer than the pause at the moment of a stop.
  - **Fix:** carry the stop in-band (the supervisor puts a marker into `events` after the stopped engine's last event, before it starts the next). Or tag a catch-up with the supervisor's `runs`, and end one from an earlier run when `restarts` has moved, whatever state is read.
- **O2. The phone-offline rule can say offline wrongly, and keep saying it.** See "The phone-offline rule" below.
- **O3. `renew` is guarded on the last client retired, not the current one.**
  - **Where:** `engine.go:229-236` (`retired.Swap(client) == client`) and `:242-259`, which deletes every device in `session.db`.
  - **Probe:** after two logouts and two links, a late `LoggedOut` from the first client reaches jawd as a logout and deletes the device linked now (1 device, then 0).
  - **Not reachable today:** whatsmeow dispatches one `LoggedOut` per client and never connects a deleted store again.
  - **Fix:** it's the one place the engine deletes a session's keys, so the guard should be exact: act only when `client == e.cli()`.
- **O4. In the newer-`session.db` mode, commands fail with `retry: false`.**
  - **Where:** `engine.go:442-445` ("the engine has no session open").
  - **History:** jawd's fetcher takes `retry: false` as "can never be had" (`sync/fetch.rs:169`), so a history notification waiting at start goes to quarantine with that error. It isn't lost: its inbox row stays, it's offered again at the next start, and its quarantine row goes when it drains (`store/write.rs:632`).
  - **Clean stops:** `LinkCancel` is refused too, so `disconnect` (`sync/mod.rs:497`) never notes a stop clean, and every start says "the last run didn't stop cleanly".
  - **Fix:** answer retryable (or `Unavailable`) in that mode, and `done` to `LinkCancel`.
- **O5. A question can be noted after its answer.**
  - **Where:** `history_more` calls `asked_phone` only once the engine has answered (`sync/mod.rs:660-661`), and the drain clears the question with any history offer (`sync/drain.rs:380-382`).
  - **The race:** an answer converted before the reply leaves a question nothing will answer, and `phone_offline` 20 s later.
  - **Likelihood:** with the Go engine, the phone's round trip takes seconds against the reply's milliseconds. With `--fail phone-offline` off, the failing engine answers from a spawned task, but the inbox's synced commit still keeps it behind.
  - **Fix:** note the question before sending, and drop it if the send fails.
- **O6. jaw's "once per message" holds only while you stay in one chat.**
  - **Where:** `jaw/src/view/keys.rs:233-242` remembers only the last chat and oldest message asked about, and jawd doesn't dedupe `history.more`.
  - **Cost:** moving between two chats at their oldest asks each time. Bounded by keystrokes, and stored idempotently: on-demand blobs take history's one path (`sync/fetch.rs`, sync type only logged), whose replays change nothing (`sync/tests.rs:1469`).
  - **Fix:** jawd could keep the last question per chat.
- **O7. The biased select's ordering test catches its mutant only sometimes:** 2 runs of 8 here, 3 of 8 for the builder. A deterministic version would be better.
- **O8. `message()` decrypts a retired client's poll votes and reactions with the current client** (`engine.go:356`, `:362`). It's harmless after a logout, since the old store is being deleted anyway, but it should use the client the event came from.
- **O9. jawd's own logout giving a fresh device has no test** (switch G9).
  - **Where:** `engine.go:452-460`.
  - **Why it doesn't block:** what it means for `session.db` is whatsmeow's own (`Logout` deletes the device before `renew` runs), and `renew` itself is tested through the phone's logout. What's untested is the feature, linking again without a restart.
  - **Why it's untested:** reaching it needs WhatsApp to answer `Logout`. A seam (the logout as a function the test can replace) would let a test reach it.
- **O10. Hygiene:**
  - `--phone-patience` takes `Duration::from_secs_f64` (`main.rs:245`), which panics on a negative or non-finite value.
  - A `session.db` from a newer whatsmeow that says it's compatible (its `compat` at most ours) is opened and written as whatsmeow allows. It's never upgraded or downgraded, and it's not a refusal. A WAL left by a newer whatsmeow is checkpointed into the file when the engine closes it, its content unchanged (measured).

### Outside this range

- **Any connection that ends during a catch-up leaves it running**, not just a logout. A `Disconnected` during a catch-up leaves `catching_up` set, so the next `Connected` is swallowed and the connection stays `connecting` until a new `OfflinePreview` or `OfflineDone`. That's I1's root, older than this branch; I1's fix should end a catch-up whenever its connection ends.
- **No guard on `--engine demo`.** `jawd --engine demo`, and so `--fail`, has no guard against a folder holding a `session.db`, unlike `jawd demo` (`demo/mod.rs:132-140`). The failing engine never touches `session.db`, but run on a real account's folder it would play its account into that jaw.db, as plain `--engine demo` would.
- **No `session.lock` or `session.db` backup yet.** Neither the lock nor the backup the design's Migrations describe is on main. `renew` deletes devices without such a backup, but only devices whatsmeow is deleting itself.
- **Other refusals of `session.db` still exit.** Corruption, foreign tables, or an owner mismatch make the engine exit 1, and jawd starts it again every 0.5 to 30 s. Only the version refusal is said as "update needed".

## The questions asked

- **The engine-restarting window:**
  - Nothing is acknowledged before a synced commit. `restarting()` commits nothing, events and acks pass as before, and in-flight events unacked at a stop come again from WhatsApp.
  - Killed during the restart, as the next engine started, and once online again:
    - the message sent before the stop was `sent` once, with no duplicate rows;
    - the message sent during the restart was `pending` with 0 attempts before the kill, after it, and until it went once jawd ran again;
    - `quick_check` was ok each time.
  - A send already with the engine when it stops counts one attempt, which is right: it may have gone.
  - A catch-up cut short by the stop ends with its summary notification, since the notifier follows `catch_up.seconds`, and what arrived before the stop is stored.
  - Gaps: O1 and T1.
- **The failing engine:**
  - `Failing::new` takes a `DemoEngine`, so it can't wrap the Go engine. `engine()` (`main.rs:362-377`) builds it only for `demo` and `demo-unlinked`, `jawd demo` always uses the demo engine, and `--fail` with the Go engine is refused (`main.rs:248-249`).
  - It passes events with their own acks untouched: each ack carries the demo engine's channel, and `Shared::tell` sends only connection and link events, which need none.
  - It never opens `session.db`.
- **Phone offline:** see below. `history.more` is bounded by jaw's asking, idempotent, and stored through history's normal merge rules (O6).
- **Logout's fresh device:**
  - `session.db` stays whole. Each delete is whatsmeow's `DELETE FROM whatsmeow_device WHERE jid = $1` at `FULL`, with foreign keys on, and every per-device table cascades but `whatsmeow_privacy_tokens`, which whatsmeow's own logout leaves the same way.
  - The fresh device is stored only when it pairs.
  - jaw.db keeps every message: the logout writes only `sync_state`'s account, synced.
  - A kill before whatsmeow's delete commits brings the device back at the next start, and WhatsApp's 401 logs it out again. A kill after it, before jawd's commit, starts with no device, which jawd hears as a logout ("this device isn't linked any more").
  - Dropping a retired client's later events drops nothing jawd stores (T2).
  - A logout during a catch-up: I1.
- **A newer `session.db`:**
  - Never written: byte for byte with no WAL left, and in content when the newer whatsmeow left a WAL.
  - Never upgraded or downgraded: `dbutil` refuses before its upgrade loop.
  - The refusal reaches jawd as "update needed", and the engine waits for jawd instead of exiting. jawd keeps `update_needed` through restarts (`what_whatsapp_said_of_the_account_stands_while_the_engine_starts_again`).
- **Statements:** no new ones; the one this path runs more is pinned and searches an index.

## The phone-offline rule

It's sound for what it was built for, the phone off or asleep while you ask for older history, and it can't do harm to the data. `presence::connected` counts `phone_offline`, and nothing else in jawd tests for `online` exactly, so messages still arrive and send. But as built, a wrong "phone offline" is easy to get and hard to lose:

- **It sticks.** Only a history notification, a logout or an engine with no device clears a question. A reconnect, an engine restart and time don't.
- **It flaps.** The time offline counts toward the 20 s, so after a reconnect with a question waiting jawd says `phone_offline` at once. It goes back to `online` a moment later when the answer comes in the offline queue; the builder's test asserts the first half.
- **It counts questions the phone may never answer:**
  - `history_more` asks before the oldest row of any kind (`sync/mod.rs:647-648`), which can be a send of yours that never reached WhatsApp.
  - whatsmeow's documentation describes the answer ("up to `count` messages immediately before the given message"). It doesn't say an empty answer comes when nothing is older, so the builder's reading that it does is more than it says.
  - A phone that answers with a peer-data response rather than a history notification isn't counted.

So yes, the rule should count only questions the phone is known to answer:
- ask, and count, only before a message WhatsApp has (not one of yours still `pending` or `failed`);
- take any sign of the phone as an answer: a history notification of any type (as now), a peer-data response, or a message or receipt the phone itself sent;
- count the patience only while online;
- give up a question after a bound.

One run on the spare number, scrolling to the top of a chat whose history is all in, settles the main unknown. None of this blocks: it's a header that can be wrong, not data.

## Before landing

- **The trial rebase** onto `fa9265c` (main's 52 Mac-only commits) is clean: 11 commits, no conflicts. The rebased `crates/`, `engine/`, `proto/` and `protocol/` are identical to `6b95b6b`'s.
- **The checks on the rebased branch:** `check-parity` passes, and `check-audit` names exactly the five data-layer commits, which still need the trailer.
- **Nothing doubled.** The reflog shows the doubled rebase the builder saw, two `rebase (finish)` onto `99d125f` back to back (`8c1cf75`, then `fb29c4a`), but the branch has nothing doubled in it:
  - the 11 patch-ids are distinct;
  - the added lines that repeat are repeated test assertions and one doc comment on two functions in different files.

## Measurements

The machine had a load average of 48 to 70 throughout, from other agents' benches. All work was in one clone at `/tmp/jaw-audits/failure-states/jaw`, built with `CARGO_INCREMENTAL=0` under `nice`, with `TMPDIR`, `GOCACHE` and every jawd home in the same folder. The Go engine ran only in its tests.

- **The gate, again:**
  - `go test -race ./...`: ok (`link` 1.5 s, `wa` 3.1 s).
  - jawd's unit tests in `sync`, `engine`, `send` and every `tests` module: 499 of 500. The one failure is `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce`, with a 273 ms outlier under load. It passed 3 of 3 alone, and the feed isn't in this range.
  - `failures.rs`, end to end: 9 of 9 in 207 s.
  - The plan test: ok.
  - `check-parity`: ok at `6b95b6b` and rebased.
- **The kill test** (a socket driver; `jawd --engine demo --fail crash`, `SIGUSR1` for the crash, then `SIGKILL`, then `jawd --engine demo` again). Three kill points, each with 7 messages in jaw.db, no duplicate `(chat, id)` and `quick_check` ok. For the message sent during the restart:

  | Killed | Before the kill | After the kill | Run again |
  | --- | --- | --- | --- |
  | during the restart | `pending`, `('queued', 0)` | same | `delivered` |
  | as the next engine starts | `pending`, `('queued', 0)` | same | `delivered` |
  | online again | `pending`, `('queued', 0)` | `sent` | `sent` |
  | during the restart, T1's switch | `pending`, `('queued', 1)` | same | `delivered` |

- **Probes:**
  - **I1:** pushed `logged_out`; account `linked`, connection `logged_out`, `feed::catching_up` true after the fresh `Connected`.
  - **O1:** with the writer held 1.2 s and a 0.3 s pause, the supervisor was already `Running` (runs 2) when the writer let go. Pushed afterwards: `[catching_up]` only, and the catch-up was still held.
  - **A newer `session.db`:**
    - No WAL: 163,840 bytes with the same SHA-256 (`4ad9126cbecca4ed…`) before and after, no `-wal` or `-shm` left, and the same 17 tables and content.
    - With a newer whatsmeow's WAL (20,632 bytes): the file grows to 167,936 bytes as the WAL is checkpointed, and version 1000/1000 and the newer table's row are unchanged.
    - Every command but `Ping` is refused with "the engine has no session open".
  - **O3:** after two logouts and two links, one device in `session.db`. A late `LoggedOut` from the first client reached jawd as `LOGGED_OUT`, and left 0 devices.
- **Mutation tests:** each switch against the package's tests (Go), or the tests named (Rust).

  | Switch | Result |
  | --- | --- |
  | G1 `mustKeep` without `Receipt` | survives (T2) |
  | G1b without `AppState` | survives (T2) |
  | G1c without `PushName`, `JoinedGroup`, `GroupInfo`, `Picture` | survives (T2) |
  | G2 without `Message` | `TestALogoutGivesAFreshDevice…` fails |
  | G3 a retired client's events never dropped | `TestALogoutGivesAFreshDevice…` fails |
  | G4 a second `LoggedOut` said again | `TestALogoutGivesAFreshDevice…` fails |
  | G5 `renew` deletes no device | `TestALogoutLeavesNoDevice…` fails |
  | G6 `renew` keeps the old client | both logout tests fail |
  | G7 a newer `session.db` exits as before | `TestASessionFromANewerWhatsmeow…` fails |
  | G8 the reason keeps its code | both logout tests fail |
  | G9 jawd's own logout doesn't renew | survives (O9) |
  | R1 no `engine_restarting` | `an_engine_that_stops…` and `what_a_stopped_engine_sent…` fail |
  | R2 a cut-short catch-up not ended | `an_engine_that_stops…` fails |
  | R3 WhatsApp's verdict not kept | `what_whatsapp_said…` fails |
  | R4 no `biased` | `what_a_stopped_engine_sent…` fails in 2 runs of 8 (O7) |
  | R5 offline at once | `a_question_the_phone_leaves_unanswered…` fails |
  | R5b offline over any state | the same fails |
  | R6 the phone's answer not heard | the same fails |
  | R7 a logout keeps the question | `a_question_put_to_the_phone_before_a_logout…` fails |
  | R8 sends stop with the phone offline | `with_the_phone_offline_a_message_still_goes` fails |
  | R9 sends go while the engine restarts | survives the send tests and sync's restart tests, 68 in all; the end-to-end crash test sends nothing during the restart (T1) |
  | R10 `history.more` notes no question | survives the unit tests; end to end, `with_the_phone_offline_older_history_waits…` fails |

## Verdict

Changes needed:
- **I1:** end a catch-up when its connection ends (at least on a logout), with a test.
- **T1:** a test that a send waits with no attempt counted while the engine restarts.
- **T2:** a test that every kind `mustKeep` must keep reaches jawd from a retired client.

O1 to O10 can stand as written-down follow-ups. O1 and O2 are worth taking with I1.

Verdict: changes needed

# Verdict of round two (`d40c721`)

Nothing in `817bc02..d40c721` loses data or leaves jaw.db inconsistent. Round one's three items are closed, each with a test that fails without its rule:
- I1, in sync's tests and the send test that links again, and end to end with a kill at each step;
- T1, in the send test with the engine restarting;
- T2, in the engine's table test, for each of the seven kinds.

T3 to T5, O11 to O13, and round one's O4 and O6 to O10, can stand as follow-ups. T3's test belongs with the keepalive fix. The keepalive bug is main's, not this branch's: give it a branch and an audit of its own before release.

Commit this note, and give the eleven commits `scripts/check-data-audit.py` names the trailer `Data-Audit: docs/audits/2026-10-28-failure-states.md`: `125e40a`, `3c46f84`, `56d14de`, `00754c2`, `035b266`, `3115df6`, `7bfb786`, `6f35023`, `8af6705`, `4f1879d` and `d40c721`. Their hashes change as the trailer goes on; the script names the same eleven after.

Verdict: approved
