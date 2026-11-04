# Audit: disappearing and view-once messages (M2)

# Round five: `1a630b1a`, round four's fixes

Range: 1708094e..1a630b1a in disappearing, 4 November 2026: round four's 33 commits rebased onto main 1708094e (to 41ae564f), and fix round five's eight, 3077bd35 to 1a630b1a, with main at b827db02. Auditor: database-expert.

## In short

- **Round four's items are fixed as it asked,** each with a test its switch fails at the head:
  - I8: a message of yours with no time to go and no outbox row counts as erasable by the timer in its own raw protobuf, and a revoked one, whose raw protobuf is cleared, counts as before. Its test has the three cases among thirty captioned photos and thirty messages with links: the deletion's late receipt, a deletion for you, and its own late receipt.
  - I9's album arm, T8's test with the timer running, T9's captioned document, O14's vacuumed test and L3's search for any copy of the photo are as round four asked. O17's caveat and O9 are in the design.
- **`received_late`, new code, holds but for one order of two late receipts (O18).**
  - Its statement searches by key and is pinned. It sets a time only where there's none, so a replay changes nothing, and before `tidied` the ordinary path takes the receipt.
  - But if your deletion's late receipt revokes the message before the message's own late receipt says WhatsApp had it, the revoke has already cleared the raw protobuf the timer is read from. Its "You deleted this message" line then keeps no time and stays for good, while the recipients' phones erase theirs at its time.
  - Nothing the message said is kept in either order.
- **The I8 fix's cost puts no budget at risk** (release, the owner's shape, three runs each side at load 20 to 27):
  - `erasable` on a message of yours with no timer takes 5.7 to 8.5 µs at p50, against 1.7 to 2.2 µs before, since its raw protobuf is now read, unpacked and decoded.
  - Edits, revokes, deletions for you and receipts in the live lane, one to a write or twenty in one, are within noise either side.
  - The one path that grows is main's chat merge step, already past 3 ms (O16): with 200 copies of your messages, 1 to 2 ms more at the median, on 21 to 34 ms.
  - Reading the raw protobuf only for a failed message, as `FAILED_WENT` does, would take nearly all of that away, and every test I ran passes with it (Q3, O19).
- **V6's new test pins V6's rule, not just its one case.**
  - Since the I8 fix, the revoked row is erasable whether the cancel comes before or after `erasable` is read. So all that's left of the rule is that the cancel's deletes run with every freed page zeroed.
  - FAST zeroes a deleted cell in its own page but leaves freed overflow pages as they were. So only an outbox row that spills can show the difference: a voice note's doesn't, and the new test's long message's does.
  - My Q1 (only the outbox cancel moved ahead of the erasure) fails the new test. My Q2 (only `media::revoked` moved) passes every test, since media rows are small enough for FAST to zero in their own pages.
- **O17: once both branches are on main, more than the timer's erasures waits for the disk.** git merges the two branches' writer code without a conflict exactly where it matters:
  - The watchdog's `index_step` returns before this branch's whole merges and its emptying of the WAL. So every erasure's follow-ups wait for the disk, the live lane's included.
  - This branch's `idle_for` ignores the watchdog's `index_after`, so the writer wakes every millisecond for as long as the disk stays short.
  - The change the second lander must make, and its test, are below.
- **The suite at the head passes under the flakes rule.**
  - The named modules ran three times: 205 tests in store::tests, both expiry modules, convert, and the schema and migration tests. They passed 199, 198 and 198, with 6 ignored, at load 15 to 75.
  - The one failure, in the second and third runs, is main's WAL-restart timing test, `a_reader_that_lets_go_while_a_restart_waits_for_it_doesnt_keep_the_wal_from_starting_again`. It's on the list among "the other WAL-bound tests", it passed alone 6 of 6 at load up to 69, and this branch doesn't touch the restart it times.
  - Every watched test among them passed every time: both WAL tests, the live writes beside inbox rows, the blob tests, and the restart test (run alone 3 of 3).
  - Media out's tests and send's D1 tests passed in 3 of 3 runs.
- **Switches:** I reran 21 at the head: the fix round's Y1 to Y5, V6, Z2, Z3, X8, I7a, D1 and D2, six older ones (P3, Z4, R6, L3, I7b, B1), and my Q1 to Q3.
  - Every rule switch fails its test, my Q1 among them.
  - Q2 and Q3 pass, as they should: neither takes a rule out that a test could see.
- **Landing:** the branch rebases onto main and then onto the engine-stop branch without a conflict. Migration 31 and `Command` 26 are free, and 38 of the 41 commits take the trailer.

## Round four's items

Each switch takes one rule out, rebuilds jawd's tests (`CARGO_INCREMENTAL=0`) and runs the rule's tests; "fails" means its test fails, as it should.

- **I8** (3077bd35):
  - **The change:** `ERASABLE` has a third column, the row's own raw protobuf, read for a message of yours with no `expires_at`, not view-once and with no outbox row. `erasable` counts it by `went_with` (unpack, decode, `convert::timer_of`). A revoke clears `raw`, so a revoked row counts as it did.
  - **The premise holds for every kind jaw sends.** Every send path writes its row's raw protobuf from the message its outbox row sends: `send.text`, `send.media` and its albums, `voice.send`, forwarding, contact cards, polls, places and events. Media's `shown` rewrites it from that same message as the copy is made or uploaded. The converter gives every other message, yours from other devices among them, its `expires_at` from its own timer, so the third column and `received_late` only matter for rows jaw's send path wrote.
  - **The test** has the three cases among thirty captioned photos and thirty messages with links. After each, the message's words and its link's are in no index's segments, in no page but those free before, and not in the WAL. Y1 (the arm taken out) fails it, and Y5 (`received_late` taken out) fails its third case.
  - The test writes the row's raw protobuf by hand, "as send.text writes it". That's true by construction today, and a test through `send.text` would pin it (a follow-up).
- **I9** (a688a28b): the `album_message` arm in `context_of`. Y2 fails both of its tests. Nothing gives an album an earlier jawd kept its time (O20).
- **T8** (fdee2281): the timer reads nothing again on a plain commit (`wait`, `expiry.rs:200`), so without the stamp the message would wait for the later one, an hour off. Y3 and Z2 fail the test.
- **T9** (4502751a): D1's two cases through `send.media` with a captioned document, and I7's with one beside the text. Y4 and Z3 fail both.
- **O14's test** (4efb502b): jaw.db is vacuumed at `secure_delete = ON` before the erasures, and only pages free then are left out. R6 fails it.
- **L3's test** (4bbf126c): the racy check is gone. The test now waits, at most 60 s, until no file in jaw's folder holds jaw's copy of the photo, which is also the HD copy, or the SD copy. L3's switch fails it.
- **The design** (28c92b33):
  - The caveat now covers any page written before an erasure, whichever b-tree has it (O17 of round four).
  - O9 is in Not yet, as `kinds::guarded` keeps the raw protobuf in quarantine.
  - The new sentence on a message of yours that nothing sends is true but for O18's order.

## `received_late`, as new code

- **The statement:** `FAILED_WENT` searches messages by its key (`SEARCH messages USING INTEGER PRIMARY KEY`), pinned in plans.txt with `ERASABLE`'s new text.
- **Where it's reached:** `received` runs on the server's answer, on every receipt of a message of yours (before the receipt moves its status) and in a chat merge's fold. `received_late` runs when no outbox row names the message.
  - A late receipt of a failed message finds it still `failed`, reads the timer from its raw protobuf and starts its time (`goes_in`). The status then moves on to delivered.
- **Merge rules and replays:** `EXPIRES` sets a time only where there's none, and `FAILED_WENT` asks for a row with none. So a replay, or a read receipt after a delivered one, changes nothing.
- **Out-of-order arrivals:**
  - A receipt before `tidied` finds the failed outbox row and goes the ordinary way.
  - One after a deletion for you finds no row, so `received` isn't reached.
  - One after a chat merge finds the message where it moved.
- **The two late receipts in either order** (probe `audit_r5_order.rs`, I8's state as its test makes it):
  - The message's own receipt and then the deletion's: it's revoked, its placeholder has a day to go, and it goes then.
  - The deletion's and then its own: it's revoked with no time, so "You deleted this message" stays for good (O18).
  - Its own receipt alone: it has a day, and still says what it said until then.
  - The deletion's alone: revoked, no time, as WhatsApp may never have had it.
  - In none of them is anything the message said kept: its text and raw protobuf are NULL.

## What the I8 fix costs

`erasable` now reads, unpacks and decodes the raw protobuf of every message of yours with no timer that something edits, revokes, deletes for you, copies over with other words or folds. And a receipt of a message of yours with no outbox row looks it up once more.

- **Per call:** 5.7 to 8.5 µs at p50 and 9 to 46 µs at p99 at the head, against 1.7 to 2.2 and 4 to 12 before. Unpacking and decoding alone take 2.2 to 3.5 µs; the rest is reading the raw protobuf's pages.
- **The live lane:** edits, revokes, deletions for you and receipts of your messages, one to a live write, are within noise either side. They take 0.1 to 0.9 ms at p50, and at p99 at most 6.9 ms before and 6.3 ms after under this load. Twenty deletions for you in one write take 1.7 to 2.8 ms, and twenty receipts 0.3 to 2.6 ms.
- **The background lane:**
  - A chat merge step folding 200 copies of your messages took 21 to 34 ms before and 23 to 31 ms after, 1 to 2 ms more at the median. That's about 400 more reads of 4 to 6 µs each, and matches.
  - A live write beside that merge waits up to 13 to 23 ms at p99 before and 17 to 23 ms after.
  - This is main's merge step, already seven to ten times the 3 ms budget (O16). History copies that change what a message of yours says, and backfills reading one again, add a read each to transactions already paced by time.
- **What would take it away:** only a failed message reaches I8's state. `tidied` removes a failed outbox row, and every other way a message of yours loses its outbox row either gives it its time (`received`) or clears its raw protobuf (a revoke) or its row (a deletion for you). So the third column could read the raw protobuf only `AND m.status = 'failed'`, as `FAILED_WENT` does. All 84 tests I ran pass with that (Q3: the store's expiry and outbox tests, I8's and V6's among them, the voice upload's, and send's and media out's D1 tests).

## V6's new test (1a630b1a)

- **Why the voice tests stopped catching V6:**
  - V6's switch moves the revoke's cancel of the failed send, and `media::revoked`, ahead of `reworded`, so they run before `erasable` is read and outside the erasure.
  - Before I8's fix, `erasable` then found neither a time nor an outbox row, so the revoke was a plain change: its words stayed in the index segments, nothing noted the WAL, and every voice test failed.
  - Since the fix, `erasable` finds the timer in the row's own raw protobuf, so the row's change is an erasure either way. All the switch still changes is that the cancel's deletes run at FAST.
  - FAST zeroes a deleted cell within its page (SQLite overwrites what it frees when that costs no extra write) but leaves freed overflow pages on the freelist as they were. A voice note's outbox row fits in its page, so nothing is left.
- **The new test:** a long message of yours (900 words and a link) with a timer, its send refused, so its outbox row stays. The test checks that row spills onto overflow pages. jaw.db is vacuumed at ON, then the message is revoked from another device; after that, its link's word is in no index's segments, no page and not the WAL.
- **It pins the rule.**
  - What's left of V6's rule is that the cancel's deletes zero what they free, and only a row that spills can show it. The outbox row is the only one of the revoke's rows that holds the whole message as it's sent, its thumbnail or long text among it.
  - My Q1 (only `outbox::deleted_for_everyone` moved out) fails the new test.
  - My Q2 (only `media::revoked` moved out) passes the new test and the voice tests: media rows are small, and FAST zeroes them in their pages. So moving that half changes nothing a test could see, and isn't a gap.

## O17: erasures and the disk watchdog

What git does: the watchdog's branch (92154dc7, on b827db02) and this one conflict only in imports, `Store`'s fields and accessors, the protocol's `status` row, the status fixture and `check-data-audit.py`, each a matter of keeping both sides. What decides O17 merges without a conflict:

- `background_changes(.., false)` calls `wait_for_disk` before every background transaction but the inbox's leftovers'. The timer's erasures go through it (`expiry.rs:113`, `Lane::Background`), so a message whose time has come stays in jaw.db while the disk is below the floor (2 GB, or 5% where less), against "Erased when asked".
- The watchdog's `index_step` returns at `!self.disk.room()` before anything else. This branch's `index_step` is what begins and goes on with the contentless indexes' whole merges and what empties the WAL after an erasure. So below the floor, every erasure's follow-ups wait too, the live lane's (a deletion for you, a revoke, a view-once message closed) among them: its words stay in links_fts's and library_fts's segments and in the WAL's frames.
- This branch's `idle_for`, with no backlog and nothing unmerged, sleeps until the next whole merge or WAL emptying is due, then at least `INDEX_PAUSE` (1 ms), and doesn't look at the `index_after` the watchdog sets. So once a follow-up is due and the disk is short, the writer wakes every millisecond, and `index_step` returns at once each time, for as long as the disk stays short. That breaks "Idle: no wake-ups, no polling".

**The change, if the watchdog lands first** (this branch's lander, in a commit of its own with the trailer, audited):

1. **The timer's erasures never wait for the disk.** `expiry::erase_due` applies its batch of `Change::Expire` through the path the inbox's leftovers take (`Store::apply_leftovers`'s channel, or a lane of its own served the same way: taken in `Writer::run` ahead of `background`, and inside `wait_for_disk`), so `background_changes(.., true)` skips `wait_for_disk`. They still wait for the WAL's room, still end each transaction after its first secure delete, and still commit at NORMAL. Nothing else of this branch changes lanes: the `once` backfill and the other backfills keep waiting for the disk.
2. **An erasure's follow-ups never wait for the disk.** In `index_step`, `merge()` runs while a contentless index's whole merge is due or under way (`whole_merge_due()` past, or a `whole.merging` set, as its steps go on), and `empty_wal()` once `wal_empty_due()` has passed, before the disk is looked at. The watchdog's check then holds only `index()` and the merges with no erasure behind them.
3. **No polling below the floor.** With (2), nothing an erasure leaves stays due. Whatever does wait for the disk is woken at `disk::POLL`: `idle_for` takes `index_after` into account in its first arm too.
4. **The test** (`Disk::faked` below the floor, say 1 GB free of 100 GB, and `Store::open_watched`, with the timer running), among several captioned photos and messages with links:
   - a message with a link and a captioned photo, both due a second later, are erased while the disk stays short: their rows gone, their words in no index's segments (messages_fts at once, links_fts and library_fts once merged whole), `erased.wal`, `erased.links` and `erased.library` gone from `sync_state`, and nothing of them in the WAL or in a page not free before;
   - a disappearing message deleted for you in the live lane below the floor, the same;
   - a background change applied meanwhile (a backfill's step, or a history message in `Lane::Background`) isn't made until the fake disk has room again, so the watchdog's rule still holds;
   - and once idle below the floor, the writer wakes no more often than `disk::POLL` (a count of its idle steps the test can read, as the checkpointer's wakes are counted for its idle test).
   - Each part fails with its switch: the timer's batch back in `Lane::Background`; the disk checked before the follow-ups in `index_step`; `idle_for` ignoring `index_after`.
5. Then `plans.txt` written again, the migrations', plan and kept tests and the gate, and a short audit of that commit.

**If this branch lands first,** the watchdog's branch makes the same change as it rebases onto it, before it lands: the timer's erasures left out of the disk wait as the inbox's leftovers are, its check in `index_step` put after this branch's follow-ups, `idle_for` not waking every millisecond, and the same test. It keeps both sides in the files above, and its own audit checks it.

## Findings

No data loss or inconsistency remains.

### Other (follow-ups that may stand, written down here)

- **O18. A failed message of yours with a timer, revoked by your deletion's late receipt before its own late receipt says WhatsApp had it, keeps its placeholder line with no time to go.** (other)
  - **Where:** `outbox::received_late` (`store/write/outbox.rs:1275`) reads the timer from the row's raw protobuf, which the revoke (`store/write.rs:3038`) has already cleared.
  - **What happens:** see the probe above. The other order, or either receipt alone, ends as intended. Nothing the message said is kept in any order: the row is a placeholder with no text, raw protobuf, reply or detail. But it stays for good, where the recipients' phones, which had the message, erase theirs at its time. And the design's new words, "a late receipt saying WhatsApp had it after all starts its time", aren't true in that order.
  - **Fix:** when a revoke takes a message of yours that's erasable by its own raw protobuf and has no time, give its placeholder the timer it went with, counted from the revoke, before the raw protobuf is cleared. It then goes at most a timer's length after the revoke, in either order. Or write the order into the design's Not yet, and qualify the new sentence.
- **O19. The I8 fix reads the raw protobuf of most of your messages** (unpredictable performance, within budgets). That's 4 to 7 µs more per `erasable` at p50, and 1 to 2 ms more on main's 200-copy merge step (O16).
  - **Fix:** `AND m.status = 'failed'` in `ERASABLE`'s third column, as in `FAILED_WENT` (Q3 passes all 84 tests I ran), with `plans.txt` written again.
- **O20. An album an earlier jawd kept in a disappearing chat has no time to go.** (other)
  - I9's arm reaches only what's converted from now on, and no backfill reads an album again: the kinds backfill reads only what it couldn't show.
  - Its photos go at their times, while its "Album: N photos" line and raw protobuf stay.
  - **Fix:** read albums again in a backfill, or write it into Not yet with the other things an earlier jawd kept.
- **I8's test through `send.text`** (other): its row's raw protobuf is made by hand, so nothing would catch a send path that wrote its row from another message than the one it sends.
- **Standing:** O16 (main's merge step), and the live write's tail beside a backlog, for the load harness on an idle Mac. O9 is in the design now.
- **For whichever lands second:** O17, as above.

## Measurements

**Method:**
- **Scratch and builds:** one clone of the worktree in `/tmp/jaw-audits/disappearing-r5/` (`--no-hardlinks`), its `target` the only build folder, `CARGO_INCREMENTAL=0`, `TMPDIR` inside it. The rebase was done in a throwaway worktree of that clone, with nothing built there.
  - A debug test build at the head, copied out for the suite.
  - Release builds, with the workspace's profile as jawd ships, of the head and of 41ae564f (round four's head rebased, before I8), each with my probe (`audit_r5.rs`, beside the store's report) as a child of the expiry tests.
- **The I8 cost probe:**
  - The database: main's jaw.db at 30, made by SQL as the branch's measurement makes it (87,000 messages in 502 chats), then migrated.
  - For each class, 300 messages of yours, sent, with no timer. Their raw protobufs are packed WebMessageInfos of what they went as: a text (142 B packed), a link with its preview's 8 KB thumbnail (8.3 KB), and a captioned photo with its 4 KB thumbnail and keys (4.4 KB).
  - What's timed:
    - `erasable` asked of 200 of them on a reader, each call timed, and their raw protobufs unpacked and decoded alone;
    - twenty edits, twenty revokes, twenty deletions for you and twenty receipts, one to a live write, each write timed by the writer;
    - a hundred deletions for you and a hundred receipts, twenty to a live write;
    - a chat merge of 200 copies of messages of yours in two chats of one person, step by step, alone and beside a live write every 10 ms.
  - Two rounds a run and three runs a side, alternating before and head, at load 20 to 27. Nothing else of mine ran but the suite at nice 10.
- **The order probe** (`audit_r5_order.rs`, beside the store's report): a debug build at the head.
- **The suite:** the named modules from the head's debug binary (8 threads, nice 10), three runs, each failure alone three times; and media out's tests and send's D1 tests, three runs.
- **Switches:** fix round five's harness, loaded as it is, with my Q1 to Q3 added (`disappearing-audit-r5-switches.py`, beside the store's report). Each ran at nice 19, rebuilding jawd's tests and running the rule's tests.
- **Machine:** Apple M1 Ultra, 20 cores; other agents kept the load as each line says.
- **What I didn't touch:** I read nothing under `~/Library/Application Support/jaw`, copied no `session.db`, ran no engine on the owner's data, played or recorded no audio, and sent nothing to WhatsApp. In the worktree I wrote only this note.

Times are ranges over the six rounds and three classes: p50 / p99, in ms unless said.

| | Before (41ae564f) | Head (1a630b1a) |
| --- | --- | --- |
| `erasable`, a text of yours (µs) | 1.7–1.8 / 5.0–12.1 | 5.7–6.0 / 8.7–15.1 |
| `erasable`, a link with its thumbnail (µs) | 1.8–2.2 / 4.1–8.2 | 8.2–8.5 / 11.9–31.0 |
| `erasable`, a captioned photo (µs) | 1.8–1.9 / 4.2–5.9 | 6.6–7.9 / 10.1–45.6 |
| Unpacking and decoding a raw protobuf alone, p50 (µs) | 2.3–3.5 | 2.2–3.5 |
| An edit, one to a live write | 0.18–0.46 / 2.3–6.9 | 0.14–0.62 / 2.1–6.0 |
| A revoke | 0.13–0.51 / 1.1–6.8 | 0.18–0.53 / 1.0–6.3 |
| A deletion for you | 0.26–0.88 / 0.9–5.2 | 0.32–0.78 / 0.8–2.8 |
| A receipt | 0.10–0.27 / 0.3–3.4 | 0.09–0.31 / 0.3–0.9 |
| Twenty deletions for you in one live write, each round's median | 1.88–2.67 (2.12) | 1.74–2.75 (2.14) |
| Twenty receipts in one live write, each round's median | 0.36–2.55 (0.93) | 0.34–1.78 (0.91) |
| A merge step of 200 copies of your texts, alone (median) | 21.3–24.2 (22.3) | 23.2–24.9 (24.0) |
| The same with your photos (median) | 25.5–34.0 (27.4) | 27.8–29.8 (28.5) |
| The step beside a live write every 10 ms, texts / photos (medians) | 23.7 / 27.3 | 25.2 / 29.4 |
| A live write beside that merge, p99, texts / photos | 13.4–21.0 / 19.0–23.4 | 16.7–22.4 / 20.0–23.2 |

- **The budgets:**
  - A live message's 5 ms at p99 and an incoming message's 10 ms are untouched: every amendment's p99 is the same either side within this load's noise.
  - The design's bound for twenty deletions for you in one write (3.5 to 17 ms for ordinary ones) holds at 1.7 to 2.8 ms.
  - The background lane's 3 ms was already broken by main's merge step (O16), and is now broken by 1 to 2 ms more.
- **Receipts** of a message of yours with a link take longer than a text's either side (1.2 to 2.6 ms for twenty, against 0.3 to 0.6). That's main's: the status update rewrites the row and its 8 KB raw protobuf.

## Landing on today's main (b827db02, with the engine-stop fix)

- **The rebase,** in my clone with rerere off. The engine-stop fix (17b9d617, 12 commits on b827db02) wasn't on GitHub's main at 16:58, so I rebased onto main and then onto its branch.
  - **Onto main (b827db02):** all 41 picks apply without a conflict, and `git range-diff` finds all 41 identical. Main's one commit since 1708094e, the release README and its pictures, touches nothing this branch does.
  - **Onto the engine-stop branch:** all 41 picks apply without a conflict. 40 are identical, and jawd's commit (da76d7ff) differs only in context lines:
    - engine-stop's new `start(home, engine, options, supervised)` signature, where this branch adds `expiry::start` and `.with_expiry`;
    - and engine-stop's line in `check-data-audit.py`, beside this branch's.
  - **No conflict needed more than keeping both sides, and none needed even that by hand.** The rebased tree is the three-way merge's (94f202a4):
    - `main.rs` keeps `mod expiry`, `expiry::start` right after `sync::running`, `.with_expiry` and the `once_backfill` check in engine-stop's reworked start;
    - `check-data-audit.py` lists engine-stop's `crates/jawd/src/main.rs` and `engine/cmd/jaw-engine/` and this branch's `crates/jawd/src/expiry.rs`.
  - **At the rebased head:** jawd's tests build. The plan, schema and kept tests pass: 135 of 140 in one run, the 4 `measure_*` ignored, and the migrations' fault test alone, in 25 minutes. `tests/upgrading.rs` passes. `check-data-audit.py` finds the 38 commits without a trailer, as it should before approval.
- **The numbers:** migration 31 is still the next free one (main's, engine-stop's and the watchdog's last is `30 schema ca276a3d…`), and this branch's is `31 schema d609fb3b…`, as round three hashed it. `Disappearing` is still engine `Command` 26; theirs end at 25.
- **Trailers:** `Data-Audit: docs/audits/2026-10-31-disappearing.md` goes on every commit that touches a data-layer path, 38 of the 41.
  - Three commits don't take it: the terminal's cd450695, and the design-only 1beb5d81 (round three's O15) and 28c92b33 (this round's). None of them touches `main.rs` or `engine/cmd/jaw-engine/`, which engine-stop's list adds.
  - The hashes change with the rebase, so go by the script's paths. The note is committed in the branch, since `make check-audit` reads it at HEAD.
- **The disk watchdog:**
  - If it's on main by the landing, its conflicts with this branch keep both sides: `store/mod.rs`'s imports and `Store`'s fields and accessors, the protocol's `status` row, the status fixture, and `check-data-audit.py`.
  - O17's change then goes in before the push, in its own commit with the trailer, with its test and a short audit, as above.
  - If this branch lands first, the watchdog's lander makes that change instead.
- **Then:** write `plans.txt` again, run the migrations', plan and kept tests and the gate, and `make check-audit` must pass before the push.

# Round four: `7d6499c9`, round three's fixes on today's main

Range: b4d9624d..7d6499c9 in disappearing, 3 November 2026: round three's 32 commits rebased onto main as migration 31, and fix round four's 7d6499c9 (voice's paths and four gaps on a revoke's path), with main at b4d9624d. Auditor: database-expert.

## In short

- **Round three's items are fixed as it meant them, but for two holes.**
  - D1 holds for every kind jaw sends but an album, which now gets no time at all (I9).
  - I7 holds while a message of yours waits, but a failed one whose deletion for everyone waits for a late receipt stops counting as erasable, and its words then stay in every index's segments and the WAL (I8).
  - T6, T7 and P3 are fixed, and each switch fails its test. O12, O13 and O15 are in the design as measured; O14's caveat says less than its test found.
- **The rebase holds.** Migration 31 follows main's 30, its SQL and lock line as round three's (`31 schema d609fb3b…`), and the hash, number, fingerprint, upgrade and kept tests pass. `drain.rs` is main's but for one call inside `convert_row`. The voices backfill reads a disappearing voice note again through the erasure, and main's voice and contact-card code is otherwise untouched.
- **The four revoke-path fixes hold,** each with a test its switch fails. But the revoke path has one more plain step: the late receipt's revoke in I8.
- **O14's failures are the design's caveat, not a leak.** Its test should change before landing: vacuumed at `secure_delete = ON` before the erasures, it passed 24 of 24 here, as for the builder, and R6 still fails it 6 of 6. L3's check at `media/outgoing/tests.rs:1919` races the dropper and should change too.
- **The owner's bound holds at the head** (release build, load 35 to 73):
  - An erasure's transaction takes 1.0 to 2.3 ms at p50 and 5 to 11 ms at p99 across both sizes, and a 62 KB message's 0.12 to 0.68 s.
  - P3's merge steps take 0.8 to 0.95 ms.
  - A live write beside a backlog took 8 to 27 ms at p99, within what rounds two and three measured, as the load moved.
- **The suite passes under the flakes rule:** jawd's unit tests had three failures, all on the landers' list, each passing alone 3 of 3; every watched test passed.
- **Switches:** 139 of 141 fail their tests, in my clone: fix round four's 137 and my Z1 and Z4. My Z2 and Z3 pass every test they name, so two rules this round added are untested (T8, T9).

## Round three's items

Each switch takes one rule out, rebuilds jawd's tests (`CARGO_INCREMENTAL=0`) and runs the rule's tests; "fails" means its test fails, as it should.

- **D1** (591cb559, and 7d6499c9 for a voice note): when the server has a message of yours, `outbox::received` reads the timer from its outbox row's raw protobuf (`WENT`, `convert::timer`) and counts it from then; one that went without gets none.
  - Both of the probe's cases are tested through `send.text` and `voice.send`, with WhatsApp's answer held while `chat.disappearing` changes the timer.
  - D1, D2 and V1 fail their tests. My Z2 (the timer not woken for the time it's given) and Z3 (the timer not read through wrappers) pass every test they name (T8, T9).
  - But `convert::timer` reads `context_of`, which has no arm for an album (I9).
- **I7** (3c6c3ae4): `ERASABLE` reads what a queued message of yours goes as, so one that goes with a timer is erasable from when it's written. Once it has gone, its outbox row goes through `unsaid` too: in `dropped` for one deleted for you, and in `received`'s dequeue for one deleted for everyone. I7a, I7b and I7c fail the text's test and the voice note's. But the mark lives only in the outbox row (I8).
- **T6** (a4277f83): thirteen cases, each a captioned photo or a message with links among thirty of each. X1 to X5 fail it, X1 retargeted at `reworded`'s change taking the Stamps.
- **T7** (c4f02b81): X6 and X7 fail it. It passed in the suite.
- **P3** (612211bb): `merge_step` ends its step after a move that made a secure delete, and returns as a full step does, so the merging task asks for the next. P3 fails its test, and so does my Z1 (the early end going on to finish the merge, which would remove the chat being merged while messages are still in it).
- **O12, O13 and O15** are in the design as the fix round wrote them, and my measurements agree with O15's (below).
- **O14:** each test word has two letters of its own, so the test checks every word. Its caveat, though, names pages used again by another table or index, while the residue the test still trips on is in pages of the same b-tree (O17).

## The rebase onto main

- **Migration 31:** it comes after main's 30 (voices), its SQL untouched, and its lock line is `31 schema d609fb3b…`, round three's hash. The Rust comment says 31; the `once` backfill's tests and the measurement make main's jaw.db at 30 (`MIGRATIONS[..30]`, `user_version` 30).
  - Tests that pass at the head: `each_merged_migration_is_still_the_text_it_was_merged_with`, `two_migrations_with_one_number_or_a_gap_are_refused`, `a_schema_changed_by_hand_is_refused`, `a_database_at_every_earlier_version_keeps_its_rows`, the kept test, the fault test, and `tests/upgrading.rs`.
  - 31 is the next free number: main's last is 30.
  - `Disappearing` is still engine `Command` 26, after main's `MediaUpload = 25`.
- **The conflicts that needed more than keeping both sides,** each checked:
  - **`sync/drain.rs`** is main's file but for `people::group_change(&change, me)` in `convert_row`'s `group_change` arm. That's inside main's `catch_unwind`, so a panic there is quarantined. The played-receipt arms in `receipt_changes` are outside `convert_row`, as round three had them.
  - **`write/links.rs`:** `BACKFILLS` ends `CONTACTS, VOICES, VIEW_ONCE`, with both arms.
  - **`state.rs`, jaw-proto's `Stored` and the protocol's `status` row** keep main's `voices_backfill`, `spool_files`, `spool_bytes` and `voice`, then this branch's `once_backfill` and `disappearing`.
  - **`check-data-audit.py`** keeps main's paths, jaw-voice's among them, then `crates/jawd/src/expiry.rs`.
  - **`again.rs`** keeps main's `VOICES` and this branch's `expiry`.
- **Voice's paths through the erasure:**
  - Main's voices backfill reads a message again through `kinds::guarded(…, read_again)`, and `read_again` goes through `expiry::unsaid` when `erasable`. V7 and V8 fail `the_voices_backfill_reading_a_disappearing_voice_note_again_leaves_nothing_of_what_its_row_said`.
  - `voice.send` puts the chat's timer in the audio message's ContextInfo, which `convert::timer` reads (V1).
- **Main's code, otherwise untouched:** the drain fix but for the one call above; contact cards but for `read_again`'s route through `unsaid` (L1, L2); jawd's voice module but for its tests; `media/mod.rs` gains only this branch's view-once functions; jaw-voice isn't touched.

## Voice's paths and the four revoke-path fixes (7d6499c9)

- **The four fixes,** each with its switch failing a test:
  - `prepared` and `uploaded` leave a revoked message's row as it is (V2, V3).
  - `unsent` tidies through `failed_message` (V4).
  - `tidied` removes an unretryable row through `dropped`, so through `unsaid` for a timed one (V5).
  - The revoke's outbox and media rows go within `reworded`'s change, after `erasable` has been read (V6).
- **The voice tests:** D1's with a voice note, I7's six cases, the upload's six, and the voices backfill's. Each holds thirty captioned photos and thirty messages with links, and checks no page but those free before, and not the WAL.
- **The rest of the revoke path:**
  - A revoke from another device or history, the engine's ack of jaw's own revoke (`acted`), a revoke waiting in `pending`, a fold's revoke, and a revoke of a message in a chat being merged all reach `apply_amendment`'s arm, so they go through `reworded`.
  - The HD rows `tidied` deletes at FAST (`ORPHANED_HD`, `HD_OF_DELETED`) go while the message still says what they say, so a later erasure or revoke notes the WAL. That's within the design's caveat.
  - But a late receipt's revoke of a failed timed message, or its deletion for you, reaches `reworded` and `delete_row` with `erasable` false (I8).

## O14 and L3

- **O14 (`a_merge_taking_erasures_words_out_before_their_whole_merge_zeroes_the_pages_it_frees`):** the builder's diagnostic, run again in my clone.
  - As the test is, it passed 168 of 168 times, eight at a time at load 40 to 64, so the 1-in-28 failure didn't show here.
  - Vacuumed at `secure_delete = ON` before the erasures, it passed 24 of 24.
  - With R6 (no zeroing in the merge steps) it fails 6 of 6 either way, each time with 40 of the 60 words in pages the merge freed. So the vacuumed form keeps R6 caught in full.
  - The builder's failures were each one page of a b-tree that holds words (`messages_fts_data`, `links_fts_docsize`, `library_fts_docsize`) which held the word before the erasures, while no index block held it after. Once a vacuum at ON clears what pages held before, nothing is left.
  - So the failures are residue in the unused part of a page written before the erasures, the design's caveat, not something the erasure or its merges leave. SQLite can leave a page's unused part as it was when it rebuilds the page to rebalance a b-tree.
  - The test should change before landing, as the builder suggests, and the caveat's words should widen (O17).
- **L3 (`an_erased_photo_of_yours_whose_hd_copy_is_on_its_way_leaves_no_copy_row_file_or_failed_send`, `media/outgoing/tests.rs:1919`):** the check that jaw's copy of the photo is still in `media/outgoing` before the erasure races the dropper, which removes it once both copies are made and nothing names it.
  - So the check can fail, and when the dropper has won, the later "jaw's copy went" proves nothing about the erasure. The rows it checks right after the erasure, while the upload is held, are what L3's rule is about, and L3's switch fails them.
  - Drop the check, and look instead, after the erasure and the cleaner, for no copy of the photo's bytes anywhere in jaw's folder, as the voice upload test does (`copies_of`).

## What else holds

- **Durability:**
  - Every change on these paths runs in the writer's transaction with its event.
  - `unsaid` notes the WAL and each contentless index in the change's own transaction, so a kill leaves them to do.
  - An interface's `send.delete`, `voice.send` and `chat.disappearing` commit synced before they answer.
  - The timer's erasures and the backfills commit at NORMAL, as before.
  - Media's cleaner and the dropper delete a file only after a synced commit stops naming it.
- **Plans:** `UNRETRYABLE` is a `SELECT` now, pinned in `outbox.rs`'s plan list and `plans.txt`. `WENT`, `GOES_AS`, `REVOKED` and `ERASABLE`'s join each search by key. Every plan test passes, and nothing new scans or sorts.
- **Idempotence:**
  - `received` is a no-op once the outbox row has gone.
  - `EXPIRES` sets a time only where there's none.
  - `dropped`, `prepared` and `uploaded` change nothing the second time.

## Findings

### Inconsistency

- **I8. A failed message of yours with a timer stops counting as erasable once your deletion of it for everyone waits for a late receipt.** (inconsistency)
  - **Where:** `expiry::erasable` (`store/write/expiry.rs:93`, `ERASABLE` at `:85`) knows that a message of yours with no `expires_at` goes with a timer only from its outbox row. `outbox::tidied` (`store/write/outbox.rs:1370`, `UNRETRYABLE` at `:344`) removes that row when the message failed and its deletion for everyone had no receipt in its day (`no_receipt`, `:1340`). The message's row still says all it said, and has no `expires_at`, since WhatsApp never had it. The outbox's own test `a_deletion_kept_for_a_late_receipt_takes_the_hd_copy_of_a_message_that_never_went` reaches this state.
  - **What happens:** a probe in my clone (`disappearing-audit-r4-probes.rs`, beside the store's report), with I7's test's message among thirty captioned photos and thirty messages with links.
    - Queued, the message is erasable. WhatsApp refuses it (406), the deletion waits for a receipt (503), and none comes (`NoReceipt`). Then the outbox holds only the deletion, the row still says "see https://gnufailing.example", and `erasable` is false.
    - The deletion's late receipt then applies the revoke through `acted`, `apply_amendment` and `reworded` as a plain update. A deletion for you is a plain `delete_row` the same way.
    - Either way the word is left in live blocks of all three indexes' segments (messages_fts two, links_fts and library_fts one each) and in the WAL, and, until the next checkpoint, in jaw.db's pages of six b-trees (`messages`, `links` and its unique index, and the three indexes' `_data`). No WAL is noted.
  - **Fix:** count a message of yours as erasable by the timer its own row's raw protobuf went with when no outbox row says it, for one with no `expires_at`, or keep the mark when `tidied` removes the row.
  - **Test:** both of the probe's cases, among several documents in each index: its words in no block, in no page but those free before, and not in the WAL.
- **I9. An album of yours sent with a timer gets no time to go.** (inconsistency, D1 for albums)
  - **Where:** `convert::timer` (`convert/mod.rs:734`) reads `context_of` (`:740`, main's), which has no arm for `album_message`. send.media writes an album message with the chat's timer for two photos or more (`send/media.rs:301`, `compose::album_message`), as its own outbox row.
  - **What happens:** probe: `convert::timer` of the album's raw protobuf is none, and a photo's is a day. Enqueued beside a timed text and both answered by the server, the album has no time and the text has one.
    - So the album stays here for good, an "Album: 2 photos" line, while WhatsApp erases it everywhere else; once its photos have gone at their time, the chat can show it as its last message.
    - On main, `EXPIRES` gave it the chat's timer at the answer.
    - An album from someone else is read the same way (`context_of`, `:507`), so one in a disappearing chat gets no time unless history gives `ephemeral_duration`. That's main's, but this branch is the one that erases what disappears.
  - **Fix:** an `album_message` arm in `context_of`. Test that an album of yours with a timer gets its time once WhatsApp has it, as the photos in it do.

### Test gap

- **T8. The timer woken for a message of yours given its time at the server's answer is untested** (Z2).
  - **Where:** `outbox::received` (`store/write/outbox.rs:1269`) stamps the time it gives (`stamps.expires`), which wakes the timer for a sooner time.
  - **What I switched:** the stamp, taken out. The 41 tests Z2 names pass: D1's text and voice tests erase with `Change::Expire` themselves, main's test of the timer starting checks only the time, and the timer's own tests (`expiry::tests`) send no message of yours.
  - **Why it matters:** with nothing due, the timer waits only for a stamped sooner time (`expiry.rs:226`); a commit wakes it only while it waits for one. So without the stamp, a message of yours whose time starts at the server's answer goes only once something else has the timer read the soonest time again, or jawd starts again.
  - **Fix:** a test with the timer running, as `a_fold_bringing_a_sooner_time_to_go_wakes_the_timer_and_the_message_goes_at_it` runs it for a fold: a message of yours with a timer, WhatsApp's answer, then the clock past its time; it's erased with nothing else committed.
- **T9. D1 and I7 for a message of yours inside a wrapper are untested** (Z3).
  - **Where:** `convert::timer` (`convert/mod.rs:736`) reads the timer inside the message's wrappers. send.media sends a captioned document inside `document_with_caption_message` (`send/compose.rs:106`), and an HD copy's own message inside `associated_child_message` (`:192`).
  - **What I switched:** the timer read from the outer message alone. The 7 tests Z3 names pass: D1's, I7's and the upload's, with a text and a voice note, and L3's.
  - **Why it matters:** a captioned document of yours sent with a timer would then get no time once WhatsApp has it, so it would stay here though WhatsApp erases it everywhere else; and it wouldn't count as erasable while queued.
  - **Fix:** test D1's two cases and I7's with a captioned document.

### Other (follow-ups; O14's and L3's tests to change before landing)

- **O14's test:** vacuum jaw.db at `secure_delete = ON` before the erasures, and then leave out only free pages, as above. It's flaky as it is: about 1 in 28 under load by the builder's 112 runs, and once here, in X1's run, which X1's change doesn't reach.
- **L3's test:** drop the check at `:1919`, and look after the erasure for no copy of the photo anywhere in jaw's folder, as above.
- **O17. The FAST caveat names too little.** It says pages freed under FAST before an erasure keep what they held, "which a page used again since by another table or index needn't do for the part it doesn't use". O14's failures are in pages of the very b-trees that held the words, which kept part of what they held in their unused part. Say so for any page written before the erasure, whichever b-tree has it now.
- **O9 still stands:** a backfill's failed re-reading keeps an erased message's raw protobuf in quarantine. Main's voices backfill is one more such path (`kinds::guarded`). It's written down only in this note; put it in the design's Not yet, or take it as `forget_waiting` takes the set-aside rows.
- **O16 still stands for main:** a plain merge step of 200 copies takes 23 to 26 ms at both sizes, and a live write beside it waits up to 19 to 21 ms.

## Measurements

**Method:**
- **Scratch and builds:** one clone of the worktree at 7d6499c9 in `/tmp/jaw-audits/disappearing-r4/`, its `target` the only build folder, `CARGO_INCREMENTAL=0`, `TMPDIR` inside it. The suite and the timings ran at nice 10, and the switches at 19.
- **The suite:** `make test`'s steps on the untouched head:
  - the engine built as `make engine` builds it, then `go test -race -count=1 ./...`;
  - then each of the 22 test executables `cargo test --workspace --no-run` built (debug), run straight from its package's folder, so the switch builds could take cargo's lock meanwhile;
  - then `cargo test --workspace --doc`;
  - then each failure alone three times.
- **Switches:** fix round four's harness, loaded unchanged, with my Z1 to Z4 added (`disappearing-audit-r4-switches.py`, beside the store's report).
  - Each switch replaces a rule's text, runs `cargo test -p jawd --bin jawd --no-run` and the rule's tests from that binary (`jawd-6773237c88a3db5c`), and puts the files back.
  - That build leaves `target/debug/jawd`, which the integration tests run, as it was.
- **Probes:** `disappearing-audit-r4-probes.rs` (I8, D3), a child of the expiry tests, in a debug build in the clone only.
- **O14:** the builder's diagnostic (`disappearing-fix-r4-o14-probe.diff`) applied in the clone. The test ran 168 times as it is, 24 times vacuumed at `secure_delete = ON`, and 6 times each way with R6.
- **Timings:** a release build (the workspace's profile: thin LTO, one codegen unit), as jawd ships, with the switch harness paused throughout.
  - The branch's ignored `measure_erasing_a_backlog_and_the_once_backfill`, unchanged: three runs at the owner's shape and two at a million.
  - Fix round three's merge and amendment probe (`disappearing-fix-r3-probes.rs`, its test renamed): one run at each size, each with two rounds.
  - Main's jaw.db at 30, made by SQL as the measurement makes it: 87,000 messages in 502 chats (93 MB), or 1,000,000 in 3,000 chats (1,048 MB).
  - A live write is a message through the live lane every 10 ms. Transaction lengths are the writer's own, commit included.
- **Machine:** Apple M1 Ultra, 20 cores, rustc 1.99.1. Other agents kept the load at 35 to 73, as each column says.
- **What I didn't touch:** I read nothing under `~/Library/Application Support/jaw`, copied no `session.db`, ran no engine on the owner's data, played or recorded no audio, and sent nothing to WhatsApp. In the worktree I wrote only this note.

Times are p50 / p99 / most, in ms, as ranges over the runs.

| | Owner's shape | A million |
| --- | --- | --- |
| Load (one minute) | 43 to 73 | 35 to 66 |
| A live write alone | 0.55–0.57 / 4.81–4.94 / 8.6–17.1 | 0.52–0.56 / 1.24–3.48 / 6.8–12.5 |
| Backlog of 3,200: a transaction | 1.04–1.25 / 5.16–10.2 / 9.4–46.0; 7–8% past 3 ms | 1.74–2.25 / 7.98–11.2 / 35.5–36.2; 19–34% |
| A live write beside the backlog | 1.27–1.40 / 8.24–22.9 / 20.3–29.4 | 2.02–2.16 / 23.1–26.8 / 63.0–773 |
| `once` backfill: a transaction | 1.59–2.12 / 7.96–8.98 / 11.5–15.1; 16–32% | 1.62–1.97 / 7.35–8.79 / 36.5–63.7; 17–27% |
| `once` backfill in all | 1.55–3.08 s | 12.8–22.8 s |
| A live write beside the backfill | 0.69–1.00 / 5.65–16.3 / 7.3–36.0 | 1.00–1.31 / 12.8–13.1 / 46.8–245 |
| The 62 KB message erased / deleted for you | 116–140 / 32–33 | 446–681 / 144–166 |
| P3: a merge of 200 disappearing copies, each of its 201 steps | 0.82–0.95 / 2.78–6.52 / 3.6–19.4 | 0.80–0.94 / 3.92–14.9 / 6.9–30.5 |
| A live write beside that merge | 0.67–0.72 / 3.03–3.60 / 3.3–30.4 | 0.48–1.04 / 3.51–16.0 / 4.0–23.1 |
| A plain merge's two steps (main's, O16) / a live write beside it at most | 22.5–26.4 / 19.7–19.8 | 23.0–24.1 / 18.8–20.6 |
| One amendment in a live write, disappearing (edit, revoke, deletion for you) at p50; the most | 0.35–0.80; 3.83 | 0.33–0.69; 6.26 |
| The same, ordinary | 0.19–0.46; 7.77 | 0.18–0.51; 6.88 |
| 20 deletions for you in one live write, disappearing / ordinary | 3.3–14.5 / 2.6–9.4 | 3.9–10.8 / 3.1–14.7 |

- **The owner's bound:** an erasure's transaction is about 1 to 2 ms at p50 and 5 to 11 ms at p99, at a load of 35 to 73 against round three's 31 to 55. The 62 KB message takes up to 0.68 s. Each background transaction still ends after one erasure (B1's test), and a merge step after one fold's (P3's).
- **The live write's tail:** beside the backlog, a live write took 8 to 27 ms at p99 over the five runs. That's within the 7 to 34 ms rounds two and three measured, and it moves with the load, not with the erasure's own length.
  - The one live write of 773 ms came while the load rose from 35 to 62, with the 62 KB messages already gone; the longest erasure that run was 36 ms.
  - So it's the machine and the commits' flushes setting the tail, as round two found. It's for the load harness to measure again on an idle Mac.
- **O15's numbers** in the design hold within this machine's noise.

**Tests run here:**
- The engine's race tests passed, and so did every Rust target but jawd's unit tests.
- jawd's unit tests: 1,259 passed, 3 failed, 21 ignored, in 46 minutes at load 30 to 72. The fault test, the plan tests and every migration test were among the passes. The three failures are on the landers' list: the feed's lane test, media in's starts-check and the socket's connection-full test. Each passed alone 3 of 3.
- jawd's integration tests (installed, microphone, starting, stopping, upgrading), jaw-voice's tests (the recording-end and playhead tests among them) and wa-proto's passed.
- Every watched test ran and passed: sync's stop bound, the restart test, the feed's starvation and coalescing tests, the two WAL tests and the blob tests.
- No doc tests in any crate.
- 141 switches, `disappearing-audit-r4-switches.txt` beside the store's report: 139 fail their tests, as they should, fix round four's 137 among them; Z2 and Z3 pass (T8, T9).

## Landing on today's main (`b4d9624d`)

- **The numbers:** migration 31 is the next free (main's last is 30), and `Disappearing` is engine `Command` 26 (main's last is 25). If anything takes 31 first, the lock line moves with its hash, and so do the Rust comment, the three commit messages that say 31, and "main's jaw.db at 30".
- **Trailers, once a round approves:** 31 of the 33 commits take `Data-Audit: docs/audits/2026-10-31-disappearing.md`, all but the terminal's 612c2035 and O15's 269a73c0, which changes only the design. So does every commit that fixes I8 and I9.
- **If main moves first:**
  - The disk watchdog changes jaw-proto's `lib.rs`, `state.rs`, `store/mod.rs`, `sync/drain.rs`, the protocol's `status` row and `check-data-audit.py`; the engine-stop fix changes `check-data-audit.py`. Keep both sides, with `people::group_change(&change, me)` inside `convert_row` and every side's `status` fields and audit paths.
  - Check that the watchdog's pause of background work holds the timer's erasures only as it holds a backfill's.
  - Write `plans.txt` again, and run the migrations', plan and kept tests and the gate.

# Round three: `d9acfb02`, round two's fixes and the owner's budget

Range: 77d44257..d9acfb02 in disappearing, 2 November 2026: round two's 17 commits and seven of fixes (b10a33e0 to d9acfb02), with main at 0dd286c9. Auditor: database-expert.

## In short

- **I4 to I6 and T3 to T5 are fixed as round two meant them.** Each fix has a test with several documents to the segments of the index it's about, and every switch of this round's fails its test in my clone: R1 to R14, A1, A5, A8 and B1, with four of the six of round one's that this round moved, and S29. What I6's paths do to the contentless indexes is mostly untested, though (T6).
- **The budget the owner chose holds for the timer and the `once` backfill.** At a million messages an erasure's transaction takes 1.72 ms at p50 and 7.56 ms at p99 (load 35 to 55), the 62 KB message's 432 ms, and a live write beside the backlog 1.55 / 10.2 ms against 0.56 / 1.73 ms alone. The design's new text is true. But a chat merge's step, which folds up to 200 copies, now makes an erasure of each copy with a time to go, so a live write can wait behind 200 of them (P3).
- **A message of yours goes by the chat's timer as it is when the server has it, not the one it went with (D1, data loss).** Now that jawd erases at that time, a message sent before the chat's timer was turned on is erased here, though WhatsApp keeps it. A probe in jaw shows it.
- **Until the server has it, a message of yours has no time to go (I7).** So a deletion for you or a revoke of one in a chat with a timer is a plain deletion, which leaves its words in messages_fts's segments.
- **Seven of my eight switches pass (T6, T7).** Nothing tests the contentless indexes' notes on the paths I6 added, since their tests have one document or none in those indexes. Nor does anything test an erasure that comes while a whole merge runs.
- **The suite passes under the flakes rule.** Four of jawd's 971 tests failed at load 55 to 81, each in a listed family and each passing alone 3 of 3. The engine's race tests pass.
- **Landing:** main conflicts in 22 files now, kept.rs among them; see the landing section.

## Round two's items

Each switch takes one rule out, rebuilds jawd's tests (`CARGO_INCREMENTAL=0`) and runs the rule's tests; "fails" means its test fails, as it should.

- **I4, the library's index** (b10a33e0): an erasure, and every change through `expiry::unsaid`, notes each contentless index the message had words in. The writer merges each noted index whole, `library_fts` as `links_fts`, from a note it loads at start. R1 (the library's note), R2 (the links' note), R3 (`library_fts` merged whole) and R4 (the note loaded at start) fail. The test holds sixty captioned photos in three segments.
- **I5, whole merges zeroed** (df950f1b): these run at `secure_delete = ON` (`expiry::zeroing`): the first step of a whole merge, and every merge step of a contentless index while an erasure's words wait in it. A whole merge, once done, notes the WAL to empty. R5, R6 and R7 fail. FTS5's automerge is 0 on all three indexes, so only jaw's own steps and a crisis merge rewrite segments (O13).
- **I6, earlier words** (209e879a): these go through `unsaid`: an edit, an event's edit and a revoke (`reworded`); a deletion for you (`delete_row`); a fold where either copy has a time to go or opens once; a copy that changes such a message's words or links; and the places, events and kinds backfills. R8 to R14 fail, and so does my X8 (a view-once message not counted erasable). Main's contacts backfill, which this branch doesn't have yet, must join at landing.
- **T3 to T5** (8d3949d2): A1, A5 and A8 fail the new tests.
- **The budget** (596ab6c5): a background transaction ends after a group that made a secure delete, and B1 fails its test. The backlog of 3,200 erasures took 3,200 transactions at both sizes. The design's text now states the owner's bound in place of "about 90 ms".
- **S29's test** (d9acfb02): S29 fails it.

## What else holds

- **Durability.** `unsaid` makes its notes (the WAL, and each contentless index) in the change's own transaction, so a kill leaves them to do. An I6 change from an interface or an inbox row commits synced; the timer's and the backfills' commit at NORMAL, as before. A failed group's savepoint rolls back its notes in jaw.db, and the writer keeps what every group noted (`undo` leaves `left_in`), which at worst merges an index whole once more.
- **The whole merges.** `Whole::due` decides both whether a whole merge begins and whether the writer has one due. An erasure that comes while one runs keeps its note, in sync_state (the `DELETE … AND value = ?2`) and in the writer (`erased != began`), and the next whole merge takes it out. A kill during one leaves its note, so jawd started again begins it again. The first two are untested (T7).
- **`IN_CONTENTLESS` matches the triggers.** A message waiting in `fts_backlog` has no entry in any index, and only an insert puts a message there. An item's entry is there when `shelf IS NOT NULL AND text IS NOT NULL`, and a message's links' entry when it has links.
- **An erasure inside an erasure** (`erasing()` reading `PRAGMA secure_delete`) runs its closure as it is. Only `zeroing` sets it to 1, and it sets FAST again after, whether its closure worked or not.
- **FTS5's secure-delete with a long position list.** Probed with Python's SQLite 3.54 and messages_fts's definition: a word repeated up to 4,000 times in one message, so that its position list spans leaf pages, leaves every block. The one copy left in the file was in a page freed under FAST before the deletion.
- **Plans.** The new statements (`ERASABLE`, `IN_CONTENTLESS`, the library's note, the merges) are pinned and search by key, and the plan tests pass.
- **Where an erased message lived:**
  - its row, its raw protobuf and its text;
  - its words: in messages_fts at once, and in links_fts and library_fts by their whole merges, pages freed since included (those freed under FAST before it are the design's caveat);
  - what waits for it (I2);
  - its media rows, by trigger, and their files and previews, by media's cleaner after a synced commit;
  - its events, which hold only references;
  - the chat's last message, made again;
  - link previews' thumbnails in `media/links/` stay, as the design's Not yet says.

  Its HD copies and failed sends come at landing, with main's `outbox::deleted_for_you(db, stamps, n)`. Replies keep their quote of it (O12).

## Findings

### Data loss

- **D1. A message of yours goes by the chat's timer as it is when the server has it, not the one it went with.** (data loss)
  - **Where:** `outbox::received` (`store/write/outbox.rs:950`) gives a message of yours its time with `EXPIRES` (`:197`): the time of the server's answer plus `chats.ephemeral` as it is then. But the message went with the timer it was written with: `text_message` (`send/quote.rs:197`), and the polls' and places' senders, put the chat's timer of that moment in `contextInfo.expiration`. And `send/outbox.rs:281` writes the row with no time until the server has it.
  - **What happens:** the probe in my clone (release), two cases.
    - A message written while its chat had no timer, the chat's timer then set to a day before the server's answer (`chat.disappearing` changes the chat's row at once, and offline the answer waits): jawd gives it a time a day after the answer, and the timer erases it, with a tombstone. WhatsApp keeps it, and so does your phone, so it's lost here for good.
    - A message written with a day's timer, the timer turned off before the answer: it gets no time, and stays here though WhatsApp erases it everywhere else.
  - `EXPIRES` is main's, from 16 October; until this branch, nothing erased at `expires_at`.
  - **Fix:** when the server has a message of yours, give it the timer it went with, counted from then, and no time if it went without one. Keep the seconds it went with when it's queued: its raw protobuf's `contextInfo.expiration`, or a column of the outbox row. Test both of the probe's cases.

### Inconsistency

- **I7. A message of yours deleted for you or revoked before the server has it isn't erased as a disappearing message is.** (inconsistency)
  - **Where:** `expiry::erasable` (`store/write/expiry.rs:84`) is `expires_at IS NOT NULL OR view_once`, and a message of yours has no `expires_at` until the server has it (D1). So `delete_row` and `reworded` take a plain deletion for one that's still waiting.
  - **What happens:**
    - In a chat with a timer, a message of yours deleted for you before the server has it still goes: the outbox keeps it, as the phone does, and it disappears on WhatsApp. Here, its words stay in messages_fts's segments marked deleted, and its links' words in links_fts and library_fts with no whole merge noted.
    - A revoke of one still waiting leaves its text the same way, and the timer's later erasure of its row doesn't take that.
    - Online the window is the second before the server answers; offline, it's as long as the outbox waits.
  - **Fix:** count as erasable a message of yours that waits for the server and goes with a timer: with D1's fix, the timer it went with. Test a deletion for you and a revoke of one, with several documents to each index's segments: its words in no block and in no page freed since.

### Test gap

- **T6. The contentless indexes' notes on I6's paths are untested** (X1 to X5).
  - **What I switched:** `unsaid` given no message, so it notes no contentless index, in `reworded` (an edit, an event's edit, a revoke), `delete_row` (a deletion for you), `again.rs`, `kinds.rs` and `upsert_message` (a copy). Each time, every test still passes.
  - **Why:** in the six-case test, the edit test and the backfills' tests, the message changed has no link, and only the view-once case has a caption, the one item in library_fts. FTS5 then rewrites a segment of one entry, which shares its page with other rows, and FAST zeroes the cells it removes from a page it writes anyway. Only a segment whose leaves fill pages frees whole pages, which FAST leaves as they were. The fold's test, with forty captioned photos, is the one that can show a missing note.
  - **Why it matters:** most where the change takes the links or the caption away, as a revoke or a deletion for you of a captioned photo does. Then no later erasure notes the index.
  - **Fix:** test each path with a captioned photo and a message with links, among several of each.
- **T7. An erasure that comes while a whole merge runs is untested** (X6, X7). With the writer forgetting it once the merge ends (X6), or the merge's end deleting its note whatever erasure it began after (X7), every test passes. Its words may be in the segment the merge is writing, so they wait for the next whole merge, which these two rules keep. Test it: a second erasure during a whole merge of several steps; its note in sync_state and in the writer after the first merge ends; its words gone after the next (with a short `whole_merge_every`); and a kill between the two.

### Unpredictable performance

- **P3. A chat merge's step can make a live write wait behind 200 erasures.** (unpredictable performance)
  - **Where:** `merge_step` (`store/write.rs:2006`) moves up to 200 messages (`MERGE_STEP`) in one change. Since I6, `fold` (`:2285`) goes through `unsaid` for each copy that has a time to go or opens once. The background stop (B1) ends a transaction only between groups, and a step is one group.
  - **Measured** (probe, release): two chats of one person holding the same 200 messages, joined and merged. With messages_fts in one segment first, as an old jaw.db has it:
    - A step takes 21 to 30 ms when the copies are plain and 30.5 to 35.6 ms when they have a time to go, at the owner's shape and at a million.
    - A live write beside it waited up to 15 to 22 ms beside a plain step and 28 to 29 ms beside a disappearing one.
    - With the copies freshly indexed into a small segment, the step took 39 to 42 ms.
    - Each fold's erasure of a short message is cheap (0.05 to 0.07 ms), but a long message's is 0.43 s at a million, so a step holding many is as long as their sum.
  - **Fix:** end a merge step after a fold that made a secure delete, as the timer's transactions and the backfills' steps already end: the loop returns as a full step does once `stamps.costliest()` is past zero, and the next step goes on. Test: a merge of two chats holding the same disappearing messages takes each erasure in a transaction of its own. Alternatively, write the exception into the design's bound.

### Other (follow-ups)

- **O12. Replies keep their quote of an erased message.** A reply keeps the quoted message's summary in its `reply` column (`convert/mod.rs:293`) and the whole quoted message in its raw protobuf, as WhatsApp's replies carry their quote. An erasure takes neither. In a disappearing chat, the reply goes at its own time, later; a reply outside one keeps the quote, as a private reply to a group's message does. Write it in the design's Not yet, or leave it to the owner.
- **O13. A crisis merge isn't zeroed.** The design says every merge of a contentless index, from an erasure until its whole merge, zeroes the pages it frees. But FTS5's crisis merge (64 segments on a level, at a commit) runs at FAST, outside jaw's steps. The writer's idle merges keep levels short, so it's rare. Either run the backlog's index steps (the commits that add segments) zeroed while an erasure's words wait, or write the exception into the design.
- **O14. Sixty test words share their prefix.** `a_merge_taking_erasures_words_out_before_their_whole_merge_zeroes_the_pages_it_frees` erases `numbat{batch}x{k}`. FTS5 stores each such term on a leaf as a suffix of the one before it, so a byte search finds a word only where it begins a leaf. The test catches R6 through those words; sixty words with distinct beginnings would check all of them.
- **O15. I6's cost in the live lane.** A deletion for you, an edit or a revoke of a disappearing message is now a secure delete in the live lane. Twenty deletions for you in one live write took 3.3 to 10.7 ms against 3.1 to 8.4 ms plain (old segments, both sizes). The 62 KB message's deletion for you took 35 ms at the owner's shape and 139 ms at a million. A phone's inbox row deleting a selection is one such write.
- **O16. Main's merge step is past the background budget by itself:** 21 to 30 ms for 200 plain folds, at both sizes. A follow-up for main.

## Measurements

**Method:**
- **Scratch and builds:** one clone of the worktree at d9acfb02 in `/tmp/jaw-audits/disappearing-r3/`, its `target` the only build folder, `CARGO_INCREMENTAL=0`, `TMPDIR` inside it. Everything ran under `nice -n 10`, and the switches under 19.
- **Tests:** `make test`'s steps on the untouched head: the engine built, `go test -race ./...`, and `cargo test --workspace --no-fail-fast` (debug). Then each failure ran alone three times.
- **Switches:** the fix round's harness, loaded unchanged, with my X1 to X8 added (`disappearing-audit-r3-switches.py`, beside the store's report).
  - Each switch replaces a rule's text, runs `cargo test -p jawd --bin jawd --no-run` and the rule's tests from that binary, and puts the files back.
  - That build makes `jawd-b44536a916431896`, not the workspace build's `jawd-2489ecafd4be1811` (features unify differently), so the harness was pointed at it, and it was built before the first switch.
  - Mine ran the expiry, rules, timer, view-once media, once, kinds, again and library tests, 96 of them. With the outbox's, copies', links' and refused timers' tests too, 145 pass at the head.
  - Round one's harness has its own C1 to C11, so its C1 to C7 and C9 ran too.
- **Timings:** the fix round's ignored `measure_erasing_a_backlog_and_the_once_backfill`, unchanged, and my probe module (`audit_r3`, a child of the expiry tests, in the clone only). Both were built with the workspace's release profile (thin LTO, one codegen unit), as jawd ships.
  - Main's jaw.db at 25, made by SQL as the measurement makes it: 87,000 messages in 502 chats (93 MB), or 1,000,000 in 3,000 chats (1,048 MB).
  - A live write is a message through the live lane every 10 ms. Transaction lengths are the writer's own, commit included.
  - The probe's merge: two chats of one person, a number and a LID, holding the same 200 messages, plain or with a time to go, joined and merged by `MergeStep`, alone and beside live writes. Its deletions: 20 of plain or disappearing messages in one live write, five rounds. Each ran with messages_fts optimized to one segment first, and once with the copies freshly indexed.
  - The timer probe (D1): a message of yours queued, the chat's timer changed, then `Step::Sent`.
- **FTS5 alone:** Python's SQLite 3.54, with messages_fts's definition.
- **Machine:** Apple M1 Ultra, 20 cores, macOS 26.6.2, rustc 1.99.1. Other agents kept the load at 30 to 81, as each column says.
- **What I didn't touch:** I read nothing under `~/Library/Application Support/jaw`, copied no `session.db`, ran no engine on the owner's data and sent nothing to WhatsApp. In the worktree I wrote only this note.

Times are p50 / p99 / most, in ms, unless they're ranges of whole steps.

| | Owner's shape | A million |
| --- | --- | --- |
| Load (one minute) | 30 to 50 | 31 to 55 |
| A live write alone | 0.53 / 1.38 / 9.07 | 0.56 / 1.73 / 17.3 |
| Backlog of 3,200: a transaction | 1.17 / 4.73 / 10.1; 5% past 3 ms | 1.72 / 7.56 / 84.8; 19% |
| A live write beside the backlog | 1.14 / 8.27 / 10.6 | 1.55 / 10.2 / 85.1 |
| `once` backfill: a transaction | 1.55 / 5.74 / 11.3; 13% | 1.63 / 7.20 / 34.8; 17% |
| `once` backfill in all | 0.98 s | 13.0 s |
| A live write beside the backfill | 1.23 / 8.39 / 9.68 | 1.20 / 13.2 / 29.9 |
| The 62 KB message erased / deleted for you | 103 / 34.9 | 432 / 139 |
| A merge step of 200 folds, plain / disappearing | 21.3–29.6 / 30.5–35.6 | 21.2–23.1 / 30.7–35.6 |
| The longest live write beside it, plain / disappearing | 15.7–22.0 / 27.9–28.2 | 15.4–15.6 / 27.8–29.1 |
| 20 deletions for you in one live write, plain / disappearing | 3.4–8.4 / 3.3–7.3 | 3.1–7.6 / 4.4–10.7 |

The merge and deletion rows are with messages_fts in one segment. With the copies freshly indexed, a disappearing step took 39.2 to 42.4 ms at both sizes, and a plain one 22.0 to 26.2.

**Tests run here:**
- jawd's unit tests: 950 passed, 4 failed, 17 ignored; the migration fault test was among the passes. The four were the feed's lane test, the socket's connection-full test, the WAL-bound test `the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it` and the stopping test `jawd_stops_well_within_its_bound_while_messages_pour_into_a_watched_chat`. Each is in a family on the landers' list and passed alone 3 of 3. None of the brief's watched tests failed.
- Every other target passed (1,444 tests in all), and so did the engine's race tests.
- 39 switches: 32 fail their tests, as they should, and X1 to X7 pass (T6, T7).

## Landing on today's main (`0dd286c9`)

- **The numbers.** Main's last migration is 28 (communities), and its last `Command` is 25 (`MediaUpload`). Chat actions lands next, as 29.
  - **The migration** takes 30 if chat actions lands first, 29 if not. Its lock line keeps `schema d609fb3b…`, since its SQL isn't edited. (Round two's note quoted 6aba4e35's `c4dfc9b4…`; the branch's has been `d609fb3b…` since cffffd97.) The Rust comment's "26" and 6aba4e35's message take the new number. "Main's jaw.db" in the `once` backfill's two tests and in the measurement (`MIGRATIONS[..25]`, `user_version` 25) becomes one less than it.
  - **`Disappearing`** takes `Command` 26; then run `make proto` and `make check-generated`. `engine.pb.go` conflicts in 59 places: make it again rather than merging it.
- **22 files conflict** (`git merge-tree` with 0dd286c9): round two's 13 and nine more from media out, communities and presence.
  - The files: jaw-proto's `lib.rs` and `windows.rs`; the terminal's `demo/actions.rs` and `wire.rs`; jawd's `main.rs`, `send/mod.rs`, `send/outbox.rs`, `state.rs`, `store/migrations.lock`, `store/mod.rs`, `plans.txt`, `read.rs`, `schema.rs`, `schema/kept.rs`, `write.rs`, `write/links.rs`, `write/outbox.rs` and `sync/backfill.rs`; `engine.proto` and `engine.pb.go`; the Mac's parity plan and the protocol.
  - In most of them each side adds beside the other's, so keep both. These need more:
    - `read.rs`: main's `children_mentions` and this branch's `ephemeral` both read column 35 of the chat list's row. Whichever comes second in the `SELECT` must read 36, and the plan pinned again.
    - `send/mod.rs`: both sides add a `media` field, main's `Option<Media>` for sending and this branch's `OnceLock` for view-once downloads. Make them one, or give them different names.
    - `kept.rs`, which merged cleanly in round two: keep main's `outgoing` and `outgoing_drop` classes and seeds, then `opened`'s.
    - `write.rs`: `tombstone_row` takes main's `outbox::deleted_for_you(db, stamps, n)` (failed sends forgotten, HD copies withdrawn, `copies_taken`), with this branch's closing of an open view-once message. `Change` and `Stamps` keep both sides' variants and fields.
    - `write/links.rs`: `BACKFILLS` keeps `CONTACTS` and `VIEW_ONCE`, and the backfill `match` keeps both arms.
    - `state.rs`: `status` keeps both backfills' counts.
- **The erasure and media out.** An erasure goes through `delete_row`, `tombstone_row` and `outbox::deleted_for_you`, inside `unsaid`. So an erased message of yours with an HD copy still to go leaves no HD copy, no `outgoing` row (it goes with its outbox row) and no file kept for it (`outgoing_drop`, after a synced commit), and no failed send. Test that.
- **Main's contacts backfill** (`contacts.rs`'s `SHOWN`) rewrites a card's text, spans and detail itself. Route it through `expiry::unsaid` when `expiry::erasable`, as `again.rs` does, and test a disappearing card read again among several documents, as T6's tests should.
- **Then:** write `plans.txt` again; run the migrations' hash, number, fingerprint, upgrade and kept tests; run the gate.
- **Trailers, once a round approves:** 23 of the 24 commits take `Data-Audit: docs/audits/2026-10-31-disappearing.md`, all but the terminal's b04d82a6, and so does every later commit that touches the data layer.

# Round two: `edb195b1`, round one's fixes and the rebase onto the library

Range: 77d44257..edb195b1 in disappearing, 2 November 2026: round one's seven commits rebased onto the library (77d44257) and ten commits of fixes. Auditor: database-expert.

## In short

- **Round one's items are fixed as it meant them.** I ran the round's 16 switches in my clone and each fails its test. Of nine switches of my own, six fail theirs and three show small test gaps (T3 to T5).
- **The rebase holds.** Migration 26, `kept.rs` and the plans are sound. The full suite passes but for six flakes on the landers' list, each passing alone 3 of 3, and the engine's tests pass.
- **Erasure isn't complete yet.** After an erasure, the WAL step and every merge, jaw.db still holds an erased message's words in three places: the library's index (I4), the pages the links' whole merge frees (I5), and `messages_fts`'s segments when the message was edited (I6). The branch's test of "gone from the files" passes only because each index in it holds one document.
- **The budgets are still broken** (P1, P2), and no option keeps both the 5 ms budget and a long message's words gone at once, so that's the owner's choice. Ending each background transaction after its first secure delete, which I measured, brings the transactions within 3 ms at p50 but not at p99.

## Round one's items

Each switch takes one rule out, rebuilds the crate (`CARGO_INCREMENTAL=0`) and runs the rule's tests; "fails" means its test fails, as it should.

- **I1, a refused timer's restore** (61b643a2): fixed in both of round one's orders, and right with a phone's setting between two of jaw's. F1 and F2 fail. A1 (the setting written into the waiting row whatever the row holds, not only when newer) passes: T3.
- **I2, what waits for an erased message** (5b701996): `forget_waiting` takes its `pending` rows and the `opened`, vote, answer and edit rows set aside for it, inside the secure deletes. `pending.chat` is the chat's resolved JID, which the erasure looks it up by. F3, A2 (the set-aside rows kept) and A3 (the `opened` row kept) fail.
- **I3, two copies folded into one** (ba90fb16): the sooner time, the mark with its caption hidden inside the secure deletes, and the opening moved to the copy kept. F4, F5, F6 and A4 (the caption left in the kept copy) fail. A5 (the fold not waking the timer for its sooner time) passes: T4.
- **O1, the openings of erased messages** (df04105b): F7, F8 and A6 (`status` not ending them) fail.
- **T1, the WAL** (d202f8f4): the step is main's truncate, with the readers' 20 ms wait, run after a quiet second, tried again a second later while a reader holds the frames, and noted in `sync_state` with the erasure. F12, F13, F14 and A7 (`PASSIVE` for `TRUNCATE`) fail. A8 (no quiet second) passes: T5.
- **T2, `view_once.open`'s lookups** (021eaf71): pinned; F9 fails.
- **P1** (c8db5f9e) **and P2** (de15d166): S21, A9 (the batch as one live transaction), F10 and F11 fail. The budgets are below.
- **O2 to O8** (edb195b1): written into the design's Not yet and the protocol. O7's test is there, and F15 fails.

## What else holds

- **Durable before acknowledged:**
  - The timer's erasures acknowledge nothing and commit at NORMAL.
  - `view_once.close` and `chat.disappearing` commit synced before they answer, and a connection's end erases what it had open in a synced commit.
  - Media's cleaner deletes a file only after a synced commit of its own, which in WAL order makes every erasure before it durable.
  - The WAL step's checkpoint is main's truncate, whose syncs the VFS fences before the WAL is cut.
- **A kill loses no erasure:** a background transaction commits a prefix of its batch in due order and the rest stay due. The kill test takes jaw.db as a kill would leave it after each batch and after single erasures, and each copy, started again, ends as a run never killed does. The WAL note commits with its erasure, so a kill leaves the step to do (F13).
- **`erased` counts each erasure once:** it's stamped only when the row went, undone with a rolled-back savepoint or transaction, and added after the commit, as the timer test that waits for all 70 checks.
- **The `once` backfill:**
  - It's idempotent: a row already as this jawd keeps it is left.
  - It's resumable: `after` commits with each step, and O7's test kills it after 32 of its 64 commits.
  - It's bounded per step: 64 rows, stopping before the next caption once its time with the costliest so far reaches 1 ms, and always taking one.
- **Migration 26** is round one's 21 renumbered, its SQL unchanged (`26 schema c4dfc9b4…`): a nullable column, the empty STRICT `opened`, and the backfill's row. `kept.rs` keeps main's mechanism and seeds, with `("opened", Own, &[])` and its seed. The 21-minute fault test and the fingerprint, kept and upgrade tests pass.
- **Plans:** every new statement is pinned. The new scans are chats walked for a lookup by ID alone (main's allowed one), `opened` (16 rows at most) and the outbox on a refusal (2,000 rows at most, allowed in `plans.rs`); none sorts in a temporary b-tree.
- **The background lane** is a queue of 16 whose producers each wait for their own work, so an erasure waits behind at most 15 other batches.

## Findings

### Inconsistency

- **I4. The library's index keeps an erased message's words.** (inconsistency)
  - **Where:** `library_fts` (main's migration 25, `store/schema.rs:1289`) is contentless with `contentless_delete = 1`, and the triggers delete from it by rowid (`messages_fts_delete`, `messages_fts_update`, `links_unshelved`), which only tombstones the entry. `erasing()` (`store/write/expiry.rs:286`) turns FTS5's secure-delete on for `messages_fts` and `links_fts` only, and the writer's whole merge after an erasure (`Writer::merge`, `store/mod.rs:1784`) is of `links_fts` only.
  - **What happens:** probes A and B. An erased photo's caption, and an erased message's link's words, stay in a live block of `library_fts_data` after the erasure, the WAL step and the merges, where search doesn't find them. So do the words of every erased photo, video, voice message or file with a caption or a name, and of every erased message with a link, until a merge happens to rewrite their segment, which nothing schedules.
  - **SQLite** (3.54): a contentless-delete table's deleted words stay in the file with or without FTS5's secure-delete; only a merge that rewrites their segment drops them.
  - **Fix:** note an erasure of a message with an entry there (an item with text, or links) as an erasure of one with links is noted, and merge `library_fts` whole as `links_fts` is, both under I5's fix. Test with several entries to a segment, as probes A and B have them.

- **I5. The links' whole merge leaves what it takes out in the pages it frees.** (inconsistency)
  - **Where:** `Writer::merge` runs on the writer's connection, at `secure_delete = FAST`.
  - **What happens:** probe B2. With 300 messages with links in six indexed transactions and the WAL truncated just before one is erased, no free page holds its link's word. After the erasure and the links' whole merge, a page freed since does. On SQLite 3.54, after a whole merge under FAST the word is still in the file; after one under ON it isn't.
  - Round one accepted this merge on a probe of a 1.4 MB index that found the word gone after it, and the branch's test holds one document in `links_fts`. B2, with the WAL truncated just before the erasure, tells apart what the merge freed.
  - **Fix:** run each step of a whole merge (`links_fts`'s, and `library_fts`'s with I4) under `secure_delete = ON`, as `erasing()` does, then FAST again. Test: probe B2's, no copy of the word in either file.

- **I6. An erasure leaves the words of a message's earlier text.** (inconsistency)
  - **Where:** an edit changes `text`, and `messages_fts_update` takes the old text out with FTS5's `'delete'` without secure-delete. That writes a delete marker and leaves the old entries in their segment, and the erasure's secure delete takes out only the text the row has then. The same goes for `fold`'s `DELETE` of the copy that goes (`store/write.rs:2360`, outside `erasing()`) if that copy held the caption, and for a backfill's re-conversion of a disappearing message's text (kinds, places, events, and main's contacts).
  - **What happens:** probe C. A disappearing message among 40, indexed, edited and indexed again, then erased: its first text is in two live blocks of `messages_fts_data` after the erasure and the WAL step, and its edited text is in none.
  - **Fix:** change the text of a message with a time to go, or of a view-once one, inside `erasing()`: an edit, a fold's deletion of the copy that goes when either copy has a time to go or is view-once, and a re-conversion. Test: probe C's.

### Test gap

- **T3. I1's "if newer than what the row holds" is untested** (A1). Two settings older than a waiting timer, arriving newest first, would leave the older in the row, and a refusal would restore it. Test that order.
- **T4. A fold's sooner time waking the timer is untested** (A5). Without `stamps.expires` in `fold` (`store/write.rs:2300`), the timer sleeps on to its earlier time, or, if it had none, until another message's time or a restart, and the message outlives its time until then. Test that a fold bringing a sooner time erases at it.
- **T5. T1's quiet second is untested** (A8). With `WAL_QUIET` at 0 the WAL is emptied between a backlog's transactions, which the fix round measured as doubling the live write's wait beside the backfill. Test that a backlog of erasures, or a backfill, leaves the WAL until it's done and a second has passed.

### Unpredictable performance (break the budgets; the owner's choice)

- **P1 and P2, measured again** with a release build (method and numbers below):
  - Beside a backlog of 3,200 erasures, a live write takes 10.4 to 15.3 ms at p99 at the owner's shape (three runs) and 23.4 ms at a million, against 4.1 to 6.1 and 4.4 ms alone. The backlog's transactions take 4.8 to 5.3 ms and 3.9 ms at p50, 79 to 92% of them past 3 ms.
  - The `once` backfill's transactions take 3.5 to 3.7 ms at p50 and 7 to 12 ms at p99, 69 to 77% past 3 ms. The backfill takes 0.8 to 1.5 s at the owner's shape and 19.3 s at a million.
  - A 9,000-word message's erasure is one transaction of 103 ms at the owner's shape and 723 ms at a million; the design says about 90 ms.
- **Why,** as the fix round found: one message's secure delete can't be split, and at these loads it takes 1 to 10 ms. The pace counts it but not the commit that follows, so a transaction ends a commit past its time, and a long message is one secure delete as long as its words.
- **Options:**
  - **(a) End a background transaction after its first secure delete.** This is the report's first option at its strictest, measured as variant V1.
    - The backlog's transactions then take 1.05 to 1.10 ms at p50 at the owner's shape and 2.17 ms at a million, 6 to 8% and 31% of them past 3 ms against the head's 79 to 92%.
    - The backfill's take 1.42 to 1.74 ms, 13 to 20% past 3 ms against 69 to 77%, and it takes about as long in all.
    - But one erasure alone still takes up to about 10 ms at p99 under load (9.6 ms at a million). A live write beside the backlog took 7.3 to 34 ms at p99 at the owner's shape over three runs and 21.6 ms at a million, about what it took beside the head's: the machine's load and the commits' stalls set the tail.
    - It keeps the words gone at once and is a change of one line. It doesn't bring the live write within 5 ms under load, nor a long message's erasure.
  - **(b) Take an erasure's words out of `messages_fts` by a merge later** (the report's second). An erasure then costs what a deletion for you does (6 to 11 ms for the long message), but its words stay in the segments, and on disk, until a whole merge of `messages_fts` (49 MB at a million) under `secure_delete = ON` (I5). That merge is due after every erasure, so the whole index is rewritten each time and the words stay for minutes. It breaks "at once" and costs the index's size in writes per merge.
  - **(c) Bound the idle merge steps** (the report's third). These are main's merges, which delay any live write: a follow-up for main, not this branch.
  - **(d) Index only the first part of a disappearing message** (mine). It bounds the long message's erasure, at about 0.08 ms a word at a million under load, at the cost of search past that part.
  - **(e) A separate small index for messages with a time to go** (mine). On SQLite 3.54 a short message's secure delete costs 0.50 ms at 5,000 messages and 0.89 ms at a million, and a long one's 110 to 150 ms at 5,000 and 87,000 against 262 ms at a million: a small gain for a schema change. And a message of yours gains its time only when WhatsApp has it, after it's indexed.
- **The trade:** no option keeps both the 5 ms p99 and a long message's words gone at once. (a) keeps the promise and shortens the transactions; (b) keeps the budget and leaves erased words on disk for minutes.
- **Whether they block landing:** not by themselves, if the owner takes (a) and the design writes the bound down. It would say that a live write may wait behind one erasure: an ordinary message's, about 2 ms at p50 and up to 10 ms at p99 at a million messages under load, or a 62 KB message's, up to 0.75 s, in place of "about 90 ms". Then they're a measured follow-up, to be measured again by the load harness on an idle Mac.

### Other (follow-ups)

- **O9. Failures quarantined for an erased message keep its content.** A backfill's failed re-conversion (`store/write/kinds.rs:96`, `copies.rs:60`, and the backfills through them) is kept under `{chat} {id}` with the message's raw protobuf, and events' failed openings (`events.rs:454`) under `{chat} {id} …` with an amendment's. An erasure takes neither. Take them as `forget_waiting` takes the set-aside rows, or say so in the design's Not yet.
- **O10. A view-once message folded while open goes at its `until`, not when its connection ends.** `ended` (`send/once.rs:266`) closes by n, which the fold removed; `close` finds it by ID through the copy kept, and `ended` could too. It can't be opened twice meanwhile, since `OPEN` refuses a second.
- **O11. The design's caveat, measured:** in probes A and C, three and two pages freed under FAST before the erasure still held its words after it. The owner's choice to zero every freed page stands.

## Measurements

**Method:**
- **Scratch and builds:**
  - One clone of the worktree at edb195b1 in `/tmp/jaw-audits/disappearing-r2/`, its `target` the only build folder, `CARGO_INCREMENTAL=0`, everything under `nice -n 10`, with `TMPDIR` inside it.
  - **Tests:** `cargo test --workspace --no-fail-fast` (debug) and `go test -race ./...` on the untouched head, then each failure alone three times.
  - **Switches:** the fix round's harness, loaded unchanged, with A1 to A9 added. Each replaces a rule's text, runs `cargo test -p jawd --bin jawd --no-run` and the rule's tests from the binary, and puts the files back. A first run without `CARGO_INCREMENTAL=0` built a different binary from the one it ran, so I set its results aside and ran it again.
- **Probes:** a child module of the expiry tests, in the clone only. Each opens a `Store` in a temporary folder, applies messages through the live lane, waits for `store.indexed()`, erases one (`Change::Expire`), and waits for the writer's WAL step (and the links' merge). It then finds each page of jaw.db holding the word through `dbstat`, and counts each index's `_data` blocks that hold it.
- **Timings:**
  - The fix round's ignored test `measure_erasing_a_backlog_and_the_once_backfill`, unchanged, built with the workspace's release profile (thin LTO, one codegen unit), as jawd ships.
  - Main's jaw.db at 25, made by SQL: 87,000 messages in 502 chats (93 MB), or 1,000,000 in 3,000 chats (1,048 MB). It's migrated, its links backfilled, its indexes made and its WAL truncated, and each run takes a copy.
  - A live write is a message through the live lane every 10 ms. Transaction lengths are the writer's own, commit included.
  - (a) is the head with `make_groups`'s stop ending a background transaction after any group that made a secure delete (`stamps.costliest() > 0`).
- **FTS5's cost by size:** Python's SQLite 3.54, a regular FTS5 table of messages of 3 to 25 words from 20,000, in WAL mode at NORMAL with no autocheckpoint, 200 single deletes and one of 9,000 words.
- **Machine:** Apple M1 Ultra, 20 cores, macOS 26.6.2, rustc 1.99.1. Other agents kept the load at 20 to 72, as each column says.
- **What I didn't touch:** I read nothing under `~/Library/Application Support/jaw`, copied no `session.db`, ran no engine on the owner's data and sent nothing to WhatsApp. In the worktree I wrote only this note.

Times are p50 / p99 / most, in ms; the owner's shape is three runs of each, alternated, as ranges.

| | Owner's shape, head | Owner's shape, (a) | A million, head | A million, (a) |
| --- | --- | --- | --- | --- |
| Load (one minute) | 20 to 61 | 29 to 64 | 24 to 47 | 63 to 72 |
| A live write alone | 0.48–0.51 / 4.06–6.06 / 7.9–24.9 | 0.50–0.53 / 4.43–11.8 / 7.2–168 | 0.51 / 4.38 / 9.63 | 0.50 / 3.40 / 11.2 |
| Backlog of 3,200: a transaction | 4.76–5.33 / 10.6–12.8 / 15.6–30.8; 90–92% past 3 ms | 1.05–1.10 / 5.46–9.71 / 14.4–36.3; 6–8% | 3.86 / 11.8 / 68.7; 79% | 2.17 / 9.63 / 60.9; 31% |
| A live write beside the backlog | 2.16–2.38 / 10.4–15.3 / 33.0–88.5 | 1.06–1.42 / 7.32–34.0 / 15.9–88.8 | 2.23 / 23.4 / 146 | 1.85 / 21.6 / 220 |
| `once` backfill: a transaction | 3.52–3.74 / 7.09–11.9 / 9.2–116; 69–77% | 1.42–1.65 / 6.13–8.10 / 11.7–25.6; 13–19% | 3.45 / 9.12 / 41.6; 76% | 1.74 / 6.63 / 23.1; 20% |
| `once` backfill in all | 0.78–1.47 s | 0.93–1.32 s | 19.3 s | 16.2 s |
| A live write beside the backfill | 1.36–1.51 / 6.93–7.94 / 8.6–13.4 | 1.15–1.21 / 7.25–11.2 / 9.6–23.1 | 1.24 / 11.9 / 41.2 | 0.97 / 9.71 / 57.3 |
| A 9,000-word message erased / deleted for you | 103 / 6.1 | 107 / 6.9 | 723 / 11.3 | 744 / 9.6 |

FTS5 alone (Python, load 53): a short message's secure delete took 0.50 / 4.66 ms at 5,000 messages, 0.53 / 5.61 at 87,000 and 0.89 / 1.65 at a million (p50 / p99); a 9,000-word one took 150, 110 and 262 ms.

**Tests run here:**
- jawd's unit tests: 935 passed, 6 failed, 17 ignored. The fault test was among the passes. The six failures were the half-closed connection, the feed's lane test, media's save test, media in's starts-check, the send restart test and the socket's connection-full test, each in a family on the landers' list and each passing alone 3 of 3.
- The 25 other targets passed, jawd's integration tests among them, and so did the engine's race tests.
- The 25 switches and four probes are above.

## Landing on today's main (`7b1ad56e`)

- **The numbers:** main holds migration 26 (contact cards' `contacts` backfill) and adds no engine `Command`; its last is still `MediaRetry = 24`. Media out lands next as 27, with `media_upload` as `Command` 25 and `Reply` 14; communities lands as 28, and chat actions after.
  - **The migration** takes one more than main's last lock line when it merges: 29 if it lands after media out and communities, 30 if chat actions lands before it too.
    - Its lock line moves with its hash (`schema c4dfc9b4…`), since the SQL isn't edited.
    - The Rust comment's "26", which is outside the string, changes to the new number, and so does 6aba4e35's message.
    - "main's jaw.db" in the `once` backfill's two tests and in the measurement (`MIGRATIONS[..25]`, `user_version` 25) becomes one less than it.
  - **The engine's `Disappearing`** takes the next free `Command`: 26 once media out has 25. Then run `make proto` and `make check-generated`.
    - jaw.db stores no `Command` tag (the outbox keeps the inner `MarkRead` or `Disappearing`), so no row moves.
    - `MarkRead`'s field 5 (`type`) stays, since media out adds nothing to `MarkRead`.
- **`kept.rs`** merges cleanly with main. It keeps main's mechanism and seeds and this branch's `("opened", Own, &[])` and seed, and, after media out, media out's `outgoing` and `outgoing_drop` classes and seeds as its note says. Run the kept test with a migration after this one.
- **Conflicts:** `git merge-tree` with `7b1ad56e` conflicts in 13 files, starting with the terminal's `wire.rs`:
  - jaw-proto's `lib.rs`;
  - the terminal's `demo/actions.rs` and `wire.rs`;
  - jawd's `main.rs`, `send/mod.rs`, `state.rs`, `migrations.lock`, `store/mod.rs`, `schema.rs`, `write/links.rs` and `sync/backfill.rs`;
  - the Mac's parity plan and the protocol.
  In `write/links.rs`, `BACKFILLS` keeps main's `CONTACTS` beside `VIEW_ONCE`, and the backfill loop keeps P2's `+ stamps.costliest()`.
- **Where it meets media out's outbox:** an erasure goes through `delete_row`, which calls `outbox::deleted_for_you`. Media out changes that call to take `stamps` and withdraw the message's HD copies (`HD_WITHDRAWN`), beside `FORGOTTEN`, which removes a failed send. Keep the erasure on that path, and test that an erased message of yours with an HD copy still to go leaves no HD copy and no send to fail.
- **Main's `contacts` backfill:** re-check that it keeps a disappearing card's `expires_at`, shows a card that came in a view-once wrapper as it is, and, with I6, rewrites a disappearing card's text under secure-delete.
- **Then:** write `plans.txt` again and read it, run the migrations' hash, number, fingerprint and upgrade tests and the kept test, then the gate.
- **Trailers, once a later round approves:** every commit `check-data-audit.py` names takes `Data-Audit: docs/audits/2026-10-31-disappearing.md`. At edb195b1 that's 16 of the 17 commits, all but the terminal's b04d82a6, and the next round's take it too.

# Round one: `8f092a70` (kept as written)

Range: 9c745877..8f092a70 in disappearing, 1 November 2026. Auditor: database-expert.

The core is sound:
- Migration 21 is a real schema step.
- The timer reads only the `messages_expiring` index.
- An erasure is idempotent and leaves its tombstone.
- A view-once file is never downloaded or named but through `view_once.open`.
- The `once` backfill keeps what main's jawd stored as this jawd would.

But three merge rules break in orders the tests don't try (I1, I3), an erasure leaves rows of its message behind (I2), and erasing breaks the live budget (P1). The test of "gone from the files" passes only because it truncates the WAL first (T1). Each finding below was reproduced by a scratch probe or bench in my clone; none is in the worktree.

## Findings

### Inconsistency

- **I1. A refused timer can leave the chat at a timer WhatsApp doesn't have.** (inconsistency)
  - **Where:** `store/write/outbox.rs`, `disappearing` (the row's `effect` keeps `was` and `was_at` as the chat had them), `timer_refused` and `TIMER_BACK`; `store/write.rs`, `newest_timer`.
  - **What happens, first order:** jaw sets 7 days at `T1`. Before WhatsApp answers, the phone's "off" arrives, older than jaw's but newer than the timer jaw replaced. `newest_timer` refuses it as older. WhatsApp refuses jaw's, and `TIMER_BACK` restores `was`, the timer from before both. Probe in `send/tests.rs`: while waiting `(604800, T1)`; after the refusal `(86400, 1000)`; the phone's was `(0, T1 - 5000)`.
  - **Second order:** two timers set from jaw in a group before WhatsApp answers, both refused, as a group refuses a member who isn't an admin. The first refusal changes nothing, since the chat's `ephemeral_at` is the second's. The second restores its `was`, which is the first, also refused. Probe: while waiting `(7776000, T2)`; after both refusals `(604800, T1)`; the group's was `(86400, 1000)`. Both stay wrong until the next setting or the next group list.
  - **Fix:** what a refusal puts back must be the newest setting that took effect.
    - When `newest_timer` refuses a setting only because the chat's time belongs to a queued `ephemeral` row, write it into that row's `effect` as `was` and `was_at`, if newer than what's there.
    - When a queued row is refused and a later queued row replaced it, give the later row the refused row's `was` and `was_at`.
    - Test both orders.

- **I2. An erasure leaves what waits in `pending` for its message.** (inconsistency)
  - **Where:** `store/write/expiry.rs`, `erase`, through `delete_row`. It writes the tombstone and removes the row, but leaves `pending` rows for the message's chat and ID, and its set-aside rows in `quarantine`.
  - **What happens:** a kept amendment for a disappearing message stays in `pending` after the message is erased, raw protobuf and all. Examples are an edit or an event's answer sealed with its message's secret, which this jawd keeps for a later one. It can never apply, since the tombstone refuses what follows. It holds the erased message's content, and `status`'s `encrypted_kept` counts it for ever. Probe: `1 before its erasure, 1 after (erased 1, tombstone Some(1))`.
  - **Not new to deletions:** a message that arrives after its time already drops these rows (`PENDING_FOR`), but the erasure doesn't; main's delete for you shares the gap. With places and events on main, sealed answers and edits waiting for an erased event stay too.
  - **Fix:** the erasure's transaction deletes `pending` rows for the message's chat JID and ID (`PENDING_FOR`), and the `quarantine` rows set aside for it: `opened`'s, whose `ref` is the chat and ID, and votes', whose `ref` begins with them (a range on `quarantine_by_ref`). Test that an erasure with a kept amendment waiting leaves none.

- **I3. Two copies of a message folded into one keep neither the sooner time nor the view-once mark.** (inconsistency)
  - **Where:** `store/write.rs`, `fold`.
    - Its `UPDATE messages AS k` merges text, status, stars, mentions and the raw protobuf of the copy that goes, but not `expires_at` or `view_once`.
    - `opened`'s row for the copy that goes is deleted with it, by its cascade.
  - **What happens:** probe in `rules.rs`.
    - Under Ana's LID: a message with a time to go, and a view-once photo.
    - Under her number: the same IDs, without a time, and as a photo with its caption.
    - After `joined` and `merged`: `("goes", expires_at: None)` and `("once", view_once: false, text: "a caption")`.
  - **Effect:** the disappearing message never disappears, and the view-once photo shows its caption. That breaks the design's "a copy only ever brings a message's time sooner, and never makes a view-once message an ordinary one", which `upsert_message` and `kinds::show` keep and `fold` doesn't.
  - **Opened copies too:** a message opened under the copy that goes loses its `opened` row. Its close then erases nothing, and the copy kept can be opened again.
  - **Fix:** in `fold`:
    - set `expires_at` to the sooner of the two (and `stamps.expires` it);
    - set `view_once` to either's for a kind that opens once, with text, spans and summary as `upsert_message` makes them for one;
    - move `opened`'s row to the kept copy before the other goes.
    - Test it as the probe does, each way round.

### Unpredictable performance

- **P1. A batch of erasures is one live transaction of about 50 ms at a million messages, and live writes wait behind it.** (unpredictable performance; breaks the live budget)
  - **Where:** `expiry::erase_due` hands `apply_durable` 32 `Change::Expire` (the live lane, one transaction, synced). In `store/write/expiry.rs`, `erasing` turns FTS5's `secure-delete` on, so each word comes out of `messages_fts`'s segments at once.
  - **Measured** (method below; 3,200 due messages):
    - At the owner's shape, at a load of 65 to 80: 101 batches of 27 ms at p50 and up to 87 ms.
    - At a million, at a load of 35 to 40: 46 ms at p50, 84 ms at p99, 93 ms at most.
    - A live write every 10 ms beside them took 72 ms at p99 at a million and 46 to 48 ms at the owner's shape. Alone, at the same loads, it takes 1.5 to 1.8 ms.
  - **By parts,** at a million, 32 erasures to a transaction committed at NORMAL:

    | | p50 |
    | --- | --- |
    | Erased as the branch does | 49.5 to 51 ms |
    | Secure-delete on once for the batch | 50 to 52 ms |
    | `secure_delete=ON` for pages alone | 7.4 to 8.3 ms |
    | Deleted for you | 7.6 to 7.8 ms |

    FTS5's secure-delete in a 49 MB `messages_fts` is the cost, about 1.3 ms a message. Turning it on and off around each erasure costs nothing measurable.
  - **One long message:** a 62 KB disappearing message (9,000 words) alone takes 40 ms to erase at the owner's shape and 89.5 ms at a million, against 10 to 11 ms to delete for you.
  - **Why it matters:** the live budget is 5 ms at p99, and the design's second principle allows maintenance to delay a message by one short transaction at most. A backlog is ordinary: what came due while jawd was stopped or the Mac slept, or a busy group's burst one timer's length ago.
  - **Fix:**
    - Erase in the background lane, in transactions bounded by time as backfills are (one or two messages each at this cost), ending early when live work waits.
    - The timer's erasures acknowledge nothing, so they can commit at `NORMAL`. One lost to a power cut is erased again at the next start, and media's cleaner already makes its own synced commit before it deletes a file.
    - Keep the live lane and a synced commit for `view_once.close`, which answers an interface.
    - For one message whose words alone take past the budget, take its words out by a merge instead, as `links_fts` does, or say in the design that it costs one long transaction.
    - Measure the live write beside a backlog again.

- **P2. The `once` backfill's transactions mostly run past 3 ms.** (unpredictable performance; breaks the background budget)
  - **Measured,** at a million messages with 8,333 view-once, 60% of them captioned:

    | | Load 35 to 40 | Load 65 to 80 |
    | --- | --- | --- |
    | The backfill in all | 7.8 s | 5.5 to 5.8 s |
    | Its transactions at p50 | 4.2 ms | 3.5 to 3.7 ms |
    | At p99 | 8.2 ms | 30 to 44 ms |
    | Past 3 ms | 89% | 67 to 71% |

    At the owner's shape it takes 0.4 to 0.5 s. Media in's audit measured the `copies` backfill at 2.8 ms at p50 and 5.4 to 6.1 ms at p99, at a load of 55 to 73.
  - **Why:** a step always takes its first view-once message, and each caption taken out costs a secure-delete, as P1 measures. The step's 1 ms (`UNMARKING`) doesn't stop a transaction from taking one more step past 3 ms.
  - **Fix:** count a caption taken out at P1's cost when the step and the transaction pace themselves, so a transaction ends once it has taken one past its budget; then measure again. Its total length, about 8 s a million, isn't the problem.

### Test gap

- **T1. "Gone from the files" is tested only after a clean stop's truncate; while jawd runs, the WAL keeps the erased message.** (test gap, and the rule fails)
  - **Where:** `store/write/expiry/tests.rs`, `an_erased_messages_words_and_raw_protobuf_are_gone_from_jaw_db_and_its_wal`. It calls `store.truncate()`, the `wal_checkpoint(TRUNCATE)` of a clean stop, before it looks.
  - **What happens:** a running jawd restarts the WAL and keeps its file up to `journal_size_limit`, 64 MiB. So frames written before the erasure stay in `jaw.db-wal` until overwritten or a clean stop: the message's insert, and every page that held its row or its words. Probe: the same message and words, 230 other writes around the erasure, no truncate. Result: the WAL, 12.7 MB, holds both the raw protobuf and the words; `jaw.db` holds neither; after `truncate()`, neither file does. The protocol promises they "go from jaw's folder".
  - **Fix:**
    - After an erasure, the writer empties the WAL in an idle step: `wal_checkpoint(TRUNCATE)` with the readers' short wait, tried again later if a reader holds it. Note it in `sync_state` as the links merge is, so a kill keeps it to do.
    - Test without `truncate()`.

- **T2. `view_once.open`'s two lookups aren't pinned in the plan test.** (test gap)
  - **Where:** `send/once.rs`, `format!("{FOUND} {IN_CHAT}")` and `format!("{FOUND} {ANYWHERE}")`. `plans.txt` has neither, as the test passes over a literal that starts with a name.
  - **Plans now** (`EXPLAIN QUERY PLAN` on the million-message database): the first searches chats, then `messages_by_id`; the second scans `sqlite_autoindex_chats_1` and searches `messages_by_id` for each chat, as main's pinned lookups of a message by ID alone do.
  - **Fix:** write each as one constant, or have the test fill in such literals; then pin both.

### Other (follow-ups; O1 should be fixed in this branch)

- **O1. A view-once message the timer erases at `until` stays counted as open.**
  - **Where:** `send/once.rs`, `close`: for a message it can't find, it answers `{}` without taking it out of `Open`. A connection that ends while its `view_once.open` is still downloading leaves an entry too.
  - **What happens:** the terminal closes the view itself at `until`, and the timer erases the message at `until`, so the timer often wins. Probe in `media/tests.rs`: after the timer erased it and `close` answered `{}`, `status` says `"open":1`. Sixteen such on one connection and it can open no more until it reconnects.
  - **Fix:** `close` removes its connection's entry by message ID when the message is gone, and so does the erasure of an opened message.

- **O2. What an erasure leaves, once I1 to T1 are fixed:**
  - the tombstone (chat, ID, time), by design;
  - link previews' thumbnails in `media/links/`, in the design's Not yet;
  - pages freed under FAST before the erasure, such as its inbox row's overflow pages, also in the design;
  - `jaw.db.before-vN`, the copy taken before a data step, kept 30 days with every message it held. The design and the protocol's "go from jaw's folder" should say so.
  - Notifications already shown aren't withdrawn.

- **O3. A Mac clock set forward erases messages early, for good.** The timer and the after-its-time check compare `expires_at` with the wall clock. The data review's H8 asked that a clock change not fire every timer at once. A guard would erase only once the wall clock is within a margin of the newest time WhatsApp has stamped.

- **O4. Settings compared across clocks.**
  - A group's list is taken as of the moment it's written (`ephemeral_at` is now), so a list fetched before a change but written after it wins over the change.
  - One-to-one settings carry each phone's own clock, so a phone ahead of the others makes the next real setting look older.
  - Stamping a list with when the engine asked for it would close the first.

- **O5. The first erasure moves both FTS5 indexes to format version 5 for good**, which turning `secure-delete` on does (checked on SQLite 3.51). SQLite before 3.42 can't read them; jaw's 3.53 and the Mac's 3.51 can.

- **O6. Receipts of view-once messages opened elsewhere aren't in `status` by themselves**, as votes' are. A played receipt from your phone becomes an `opened` row for every ID it names, voice messages included. For a message jaw never gets, it waits in `pending` (1,000 at most), then in `quarantine` for ever. Add `opened_waiting` and `opened_set_aside`.

- **O7. The test of a backfill killed is after its end, not halfway.** `once` resumes by construction (its `after` commits with each step), so this is a gap of one assertion.

- **O8. Owner's choices, as the builders listed them:** a view-once row without a raw protobuf keeps its caption, so windows, search and the chat list still show it; replies an earlier jawd kept keep a view-once photo's caption in their quote. Whether WhatsApp sends jaw's own played receipt back to it as `played-self` is for the live checklist: if it does, jaw erases the message while it's shown.

## What holds

- **Migration 21** is a schema step, its kind and its tables' classes declared, `kept.rs` taking `opened`, and its lock line `21 schema d609fb3b…`. At a million messages its SQL took 5 ms from `BEGIN IMMEDIATE` to a synced commit, against 1 to 3 ms for a step that sets only `user_version`.
- **The timer's statements** search `messages_expiring` and stop at their limit; nothing scans on a wake. Every new statement but T2's is pinned, with no scan or temporary b-tree of a large table.
- **An erasure** happens once whatever the order of its timer, a played receipt and a deletion. It's counted once in `erased`, undone with a savepoint rolled back. Its row goes with its event; media's cleaner removes the file and previews after its own synced commit.
- **`messages_fts`** loses an erased message's words at once. **`links_fts`**, contentless, keeps them until a merge, so its whole merge is needed. A probe on a 1.4 MB index: the word was still there after the secure delete and the idle merges, and gone after a whole merge, which took 18 ms. The builders' finding that secure-delete alone took the links' words out holds only for their one-document index. At a million messages `links_fts` is 1.05 MB, so merging it whole every ten minutes costs little.
- **View-once files** are never downloaded or named but through `view_once.open`: `media.fetch`, `media.save` and `media.cancel` refuse one, the scheduler skips one not being opened, and windows get no path, previews or thumbnail.
- **The `once` backfill** leaves no caption in any view-once row with a raw protobuf, at both sizes, and taken again it changes nothing.
- **Tests run here:** the 231 tests of the branch's areas, the 19 schema, fixture and migration tests and the upgrade-from-fixtures integration test all pass. I didn't rerun the 74 switches.

## Landing on today's main

- **Conflicts:** `git merge-tree` of the head with `e12d8d34` conflicts in ten files: `convert/mod.rs`, `send/mod.rs`, `store/migrations.lock`, `store/mod.rs`, `store/plans.txt`, `store/schema.rs`, `store/schema/kept.rs`, `store/write.rs`, `store/write/links.rs` and `protocol/README.md`. The other 37 shared files merge cleanly.
- **The migration** takes the next number free on main when it lands: 25 if it lands before the library, which is to take 25, and the number after that otherwise.
  - Its lock line moves there with the same hash, as long as the SQL isn't edited (the Rust comment's "21" is outside the string).
  - The `once` backfill test's "as main's jawd left it" becomes `MIGRATIONS[..N-1]` and `user_version` N-1.
- **The engine's `Command`:** `Disappearing = 25` is free on main, whose last is `MediaRetry = 24`. Whichever of this and media out lands second takes 26 and regenerates `engine.pb.go`. jaw.db stores no `Command` tag: the outbox keeps the inner `MarkRead` or `Disappearing`, so renumbering moves no row. Media out mustn't also take `MarkRead`'s field 5.
- **`kept.rs`:** main's places landing solved what this branch's `SEEDS_SINCE` solves (media's tables from 20 on), with one media seed for every version and `ON CONFLICT DO UPDATE`.
  - Keep main's mechanism and seeds, and drop `SEEDS_SINCE` with its media rows.
  - Add `("opened", Own, &[])` and the `opened` seed row.
  - Its test then re-checks that main's `event_answers`, `openings` and `unopened` rows survive the new migration.
- **Re-check on the rebased head:**
  - `plans.txt` written again and read;
  - the migrations hash, number, fingerprint and upgrade tests;
  - main's kinds test;
  - that erasing an event takes its `event_answers`, `openings` and `unopened` rows by their cascades and, with I2's fix, its sealed answers and edits in `pending`;
  - that places' `again` backfill and `send.event` and `send.location` keep the timer and the view-once rule.
- **`failures.rs`:** this branch doesn't change jawd's demo account; it changes the terminal's own and the demo engine's answers to the new commands. So the library's change to the offline-history test isn't affected.
- **Trailers, once a later round approves:** the six data-layer commits take `Data-Audit: docs/audits/2026-10-31-disappearing.md`: 00c0308c, e5321c0c, 507ea9e5, 88ee025a, a5839bf7 and 8f092a70 (`check-data-audit.py` names all six). The terminal's 4c3cf933 touches none.

## Measurements

**Method:**
- **Scratch and builds:**
  - One clone of the worktree at 8f092a70 in `/tmp/jaw-audits/disappearing-2/`, its `target` the only build folder, `CARGO_INCREMENTAL=0`, everything under `nice -n 10`.
  - Probes and benches were scratch tests in the clone (`store/write/expiry/audit.rs`, and appended to `send/tests.rs`, `media/tests.rs` and `rules.rs`). The only other changes there were a record of each writer transaction's length (test builds only) and `sync::backfill` made `pub(crate)`.
  - Correctness probes ran on the debug test build; timings on `cargo test --release` with `lto=false` and 16 codegen units, to build faster. Nearly all the time measured is in SQLite's C, which LTO doesn't change.
- **Machine:** Apple M1 Ultra, 20 cores, macOS 26.6.2, rustc 1.99.1, SQLite 3.53.2. Other agents kept the load at 35 to 80, as each figure says.
- **What I didn't touch:** I read nothing under `~/Library/Application Support/jaw`, copied no `session.db`, ran no engine and sent nothing to WhatsApp. In the worktree I wrote only this note.
- **Databases:**
  - Made by SQL on migrations 1 to 20 as main's jawd leaves them: chats, people, messages with raw protobufs of 380 to 620 bytes, text of 3 to 25 words from a 20,000-word vocabulary, and FTS5 merged as the writer's idle merges leave it.
  - One in 22 has a link; one in 120 is a view-once photo as main kept it, 60% of them captioned and a quarter of those with a link; one in 200 goes within a week; 3,200 are past their time.
  - The owner's shape: 87,000 messages in 502 chats, 85 MB (an earlier audit's copy of the owner's was 79.5 MB).
  - A million: 1,000,000 in 3,000 chats, 931 MB, `messages_fts` 49 MB, `links_fts` 1.05 MB.
- **Each run:** on a `VACUUM INTO` copy, opened by the head's `Store::open`, which ran migration 21.
  - The `once` backfill was driven by `sync::backfill::run`, as jawd runs it.
  - Erasures were `expiry::erase_due` called as the timer calls it, until nothing was due.
  - Live writes were one `Change::Message` every 10 ms through the live lane, timed from the call to its answer.
  - Transaction lengths are the writer's own, commit included.

| | Owner's shape (load 65 to 80) | A million (load 35 to 40) | A million (load 65 to 80) |
| --- | --- | --- | --- |
| Migration 21 (SQL, synced) | | 5 ms (an empty step 1 to 3 ms) | |
| `once` backfill, in all | 0.40 to 0.49 s | 7.81 s | 5.52 to 5.84 s |
| Its transactions, p50 / p99 / most | 3.4 / 37 to 40 / 37 to 40 ms | 4.23 / 8.21 / 19.6 ms | 3.5 to 3.7 / 30 to 44 / 63 to 78 ms |
| Past 3 ms | 61 to 65% | 89% | 67 to 71% |
| 3,200 erased: batches | 101 in 3.1 to 3.2 s | 101 in 4.9 s | 101 in 4.7 to 5.9 s |
| A batch, synced, p50 / p99 / most | 27 / 66 / 80 to 87 ms | 46 / 84 / 93 ms | 36 to 54 / 121 to 126 / 134 to 194 ms |
| A live write beside them, p50 / p99 | 1.2 to 1.4 / 46 to 48 ms | 0.78 / 72 ms | 1.2 to 2.2 / 97 to 112 ms |
| A live write alone, p50 / p99 | 0.59 / 1.66 ms | 0.65 / 1.53 ms | 0.65 / 1.81 ms |
| A 62 KB message erased / deleted for you | 40 / 10.2 ms | | 89.5 / 10.9 ms |

The probes' results are quoted with their findings.

## Verdict

Changes needed:
- **The merge rules:** I1 (a refused timer restores the newest setting that took effect, in both orders), I3 (`fold` keeps the sooner time, the view-once mark and `opened`'s row).
- **Erasure:** I2 (it takes what waits for its message) and T1 (the WAL emptied after an erasure, tested without a truncate).
- **Budgets:** P1 (erasures in short background transactions at `NORMAL`, but for `view_once.close`) and P2 (the `once` backfill paced by what a caption costs), measured again.
- **Plans:** T2, `view_once.open`'s lookups pinned.
- **Also this round:** O1. The other follow-ups may stand, written down.

Verdict: changes needed

# Verdict of round two (`edb195b1`)

Changes needed:
- **Erasure complete,** each fix tested with several documents to a segment, as the probes have them:
  - I4: the library's index merged whole after an erasure that left an entry there.
  - I5: whole merges run under `secure_delete = ON`.
  - I6: a disappearing or view-once message's text changed only under secure-delete.
- **Tests:** T3, T4 and T5.
- **The owner's choice on P1 and P2.** Erasing a disappearing message takes its words out of search's index at once, in a step that can't be split: about 2 ms for an ordinary message at a million messages (up to 10 ms at p99 under load), and up to 0.75 s for a 62 KB one. A live message can wait behind it. So: keep that, with each background transaction ending after its erasure and the bound written into the design, or keep the 5 ms budget by leaving erased words on disk until a later whole merge of the index, minutes later?
  - (a) Keep it, with the bound in the design. I'd pick this: what's erased being gone is the owner's first rule, and the cost is one wait of measured length.
  - (b) Merge later: the budget kept, erased words left on disk for minutes, and the 49 MB index rewritten after erasures.
  - (d) Either, and also index only the first part of a disappearing message, which bounds a long one's erasure at the cost of search past that part.
- **Follow-ups that may stand, written down:** O9 to O11.

Verdict: changes needed

# Verdict of round three (`d9acfb02`)

Changes needed:
- **D1, data loss:** when the server has a message of yours, it takes the timer it went with, counted from then, and no time if it went without one. Test both of the probe's cases.
- **I7:** a message of yours that waits for the server and goes with a timer counts as erasable. Test its deletion for you and its revoke, with several documents to each index's segments.
- **Tests:** T6 (each of I6's paths with a captioned photo and a message with links, among several of each) and T7 (an erasure while a whole merge runs, and a kill between it and the next).
- **P3:** a merge step ends after a fold that made a secure delete, with a test; or the design writes the exception into the owner's bound.
- **Follow-ups that may stand, written down:** O12 to O16, with round two's O9 to O11.
- **Then the landing,** as the landing section says, once a round approves.

Verdict: changes needed

# Verdict of round four (`7d6499c9`)

Changes needed:
- **I8:** a message of yours that went with a timer stays erasable whatever becomes of its outbox row. A failed one whose deletion for everyone waits for a late receipt, then revoked by that receipt or deleted for you, takes its words as an erasure does. Test both cases among several documents in each index.
- **I9:** an album of yours sent with a timer gets its time once WhatsApp has it, through an `album_message` arm in `context_of`, with a test.
- **Tests:** T8 (the timer woken by the time `received` gives) and T9 (D1 and I7 with a captioned document).
- **Before landing:** O14's test vacuums jaw.db at `secure_delete = ON` before its erasures, and L3's racy check gives way to one for no copy of the photo left in jaw's folder.
- **Follow-ups that may stand, written down:**
  - O17 (the caveat's words), O9 (a failed re-reading's quarantine) and O16 (main's merge step);
  - the live write's tail beside a backlog, for the load harness to measure again on an idle Mac.
- **Then the landing,** as round four's landing section says, once a round approves.

Verdict: changes needed

# Verdict of round five (`1a630b1a`)

Approved:
- **Round four's changes are made as it asked:**
  - I8: a failed message of yours with a timer stays erasable by its own raw protobuf, and its own late receipt starts its time.
  - I9: an album's timer is read from its ContextInfo.
  - T8 and T9 have their tests, O14's and L3's tests are changed as asked, and the design has the widened caveat and O9 in Not yet.
  - Each rule has a test its switch fails at the head, and the suite passes under the flakes rule.
- **No data loss or inconsistency remains.** These may stand as follow-ups, written down above:
  - O18: a contentless placeholder is kept for good in one order of two late receipts;
  - O19: the I8 fix's reads, within every live budget;
  - O20: albums an earlier jawd kept have no time;
  - I8's test through `send.text`;
  - and, standing from earlier rounds, O16 and the live write's tail beside a backlog.
- **Landing:**
  - Rebase onto today's main and the engine-stop fix, keeping both sides, as the landing section says. No conflict needs more than that. The rebased tree passes the plan, schema, kept and upgrade tests, and migration 31 and `Command` 26 are free.
  - Put `Data-Audit: docs/audits/2026-10-31-disappearing.md` on the 38 commits that touch the data layer (all but cd450695, 1beb5d81 and 28c92b33, by today's hashes), and on any commit made since that does. Commit this note in the branch.
  - **O17:** if the disk watchdog is on main first, this branch's lander makes O17's change before the push, in its own commit, with a short audit of it:
    - the timer's erasures and every erasure's idle follow-ups never wait for the disk;
    - the writer doesn't poll below the floor;
    - and its test runs below the floor.
    - If this branch lands first, the watchdog's branch makes that change before it lands.
  - Then write `plans.txt` again and run the migrations', plan and kept tests, the gate and `make check-audit`, from the worktree the branch was built in.

Verdict: approved
