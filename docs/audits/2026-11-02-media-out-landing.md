# Audit: media out's landing on main after presence and contact cards (`media-out-land-trial` onto `7b1ad56e`)

# Round two: `81c45fa1`

Range: `7b1ad56e..81c45fa1` in `media-out-land-trial` (worktree `~/src/jaw-media-out`), 2 November 2026: round one's first 33 commits, as it read them, and `81c45fa1` in place of `d53934f0`. Auditor: database-expert.

Only the 34th commit is in scope. It implements the coordinator's default for D1, for the owner to confirm or reverse: the draft clears only when a caption actually goes. Round one stands for everything else. Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `81c45fa1`.

## In short

Approved. D1 is closed in jawd, T1 and O1 are closed, nothing else changed, and the full suite passes under the flakes rule. One new finding is the terminal's, outside the data layer (O5).

- **Nothing else changed.** `git range-diff 7b1ad56e..d53934f0 7b1ad56e..81c45fa1` gives 33 `=`, then `d53934f0` out and `81c45fa1` in, both on `33db588e`, the same 33rd commit. The 34th touches `send/compose.rs`, `send/media.rs`, `store/write/outbox.rs`, `media/outgoing/tests.rs` and `protocol/README.md`, and nothing else.
- **D1 is closed in jawd.** A file's message clears its chat's draft only when its caption goes.
  - `send.media` decides it for each message as it composes (`send/media.rs:346`), from the same kind and caption, in the same loop step, as it gives `file_message` (`:318-326`). `attach` passes it on (`store/write/outbox.rs:937-938`), and `enqueue_one` clears only for such a message sent in its chat (`:827`).
  - An audio file's caption, a file without one, one of only blanks, and the album's own message (`send/media.rs:309`) leave the draft.
  - Round one's probes A and D are now the branch's tests, and pass.
- **The composer and the rule can't disagree.**
  - `file_message` trims a caption and drops an empty one (`send/compose.rs:54`), and gives one to a photo, video, GIF and document, never to audio.
  - `caption_goes` (`:121-129`) is the same predicate over the same five kinds, the only five `AttachmentKind` has, with no wildcard arm, so a new kind can't compile without a decision.
  - The composer's test (`:392-412`) ties the two for every kind and four captions (none, empty, blanks, words).
- **T1 is closed.** The album's test sends two photos with a key, the first with the caption, then sets a newer draft and asks again: the same messages, nothing written, the newer draft left, and still left after the copies' later steps (`media/outgoing/tests.rs:395-419`).
- **O1 is closed.** The protocol says it in `send.text`'s words (`protocol/README.md:433`).
- **Still holding:**
  - the clear is in `attach`'s transaction, the synced commit `send.media` answers after (`send/media.rs:352`), and rolls back with the messages if it fails;
  - a replay returns at the key's look before anything is written (`send/media.rs:134-139` on a reader, `store/write/outbox.rs:914-919` in the write);
  - `typed` is never stored, so restarts and kills are as round one read them;
  - HD copies are written by their own statement (`outbox.rs:953-960`), never through `enqueue_one`, so they never touch the draft;
  - sends heard from other devices come through the inbox, which writes no draft.
- **Every clause of the rule fails a test when switched off,** the lander's or main's, in two rounds alike (Measurements). My two probes pass at the trial: a document's, a video's and a GIF's caption, and an album's second photo's alone, clear the draft once; audio first with the caption, then a photo, leaves it.
- **The full suite passes under the flakes rule:** 1,500 passed, 8 failed and 23 ignored, and each of the eight passes alone 3 of 3. Five are on the list. Three are first sightings, to watch, none in a file the 34th touches: one of them is main's checkpointer test, which waits a bounded number of writes for a checkpoint (Measurements). `go test -race` and `cargo fmt --check` pass.
- **O5, the terminal's, outside the data layer:** with an audio file first, the terminal still loses words typed as its caption. Words typed in its last 1.5 s are never saved, and words saved before, which jawd now keeps, are hidden from the box it empties and replaced by the next words typed in that chat. jawd's rule can't close it, and this commit doesn't change it.

## What was asked, checked

| What's sent | Which messages are `typed` | The draft | Tested by |
| --- | --- | --- | --- |
| A photo with a caption | the photo's | cleared, in the commit the answer follows | the lander's caption test |
| A video, GIF or document with one | the file's | cleared | the composer's test; my probes E and F |
| An album with a caption on any photo | that photo's, never the album's own | cleared once | the album test; probe E (the second photo's alone, and both) |
| A file without a caption, or with only blanks | none | left | the no-caption test |
| An album without captions | none | left | the no-caption test |
| An audio file with a caption, which jawd drops, as its message has none | none | left, with the words | the audio test; probe F, with a photo after it |
| A card or a poll | none | left | main's cards test; the lander's caption test |
| Text | the message | cleared | main's text test |
| A forward | none, and sent as `Sent::Forwarded` | left | main's forward test, with both guards off |
| A replay with the key | nothing is written | a newer draft left | the album test |
| The copies' later steps | nothing writes the draft | a newer draft left | the album test |
| A send heard from your phone | the inbox writes no draft | left | by reading, as in round one |

- **No typed words are lost in jawd.** What a send carries clears the draft, and what it doesn't carry leaves it. A file whose caption went and whose send then fails keeps its row, failed, with the caption, as failed text does. In the terminal, words can still be lost (O5).
- **A draft is never cleared by a send that carries no caption.** A caption that goes clears the draft whatever the draft says, as `send.text` does with its text: that's main's rule, and the terminal and the Mac's design both send what's typed in the box as the first file's caption (`docs/plans/2026-10-27-jaw-mac-box-design.md:31`). Round one's O2, a draft saved from another interface while the files are kept, stands as main's follow-up.

## Findings

### Data loss

None. D1 is closed in jawd. What remains of it is in the terminal (O5).

### Inconsistency

None.

### Test gap

None. T1 is closed, and every clause of the rule fails a test when switched off. A video's, GIF's and document's clear is tested by the composer's test rather than end to end. That's enough, since `send.media` gives `caption_goes` the inputs it gives `file_message`.

### Unpredictable performance

None.
- The commit adds no statement. `CLEAR_DRAFT` is main's and pinned (the outbox's plan test, `store/write/outbox.rs:1633`): one UPDATE by rowid in `attach`'s transaction.
- It now runs only for a message whose caption goes, so a send without one runs none.
- The lander's `JAW_PLANS=write` left `plans.txt` unchanged.

### Other

- **O5. In the terminal, words typed as an audio file's caption can still be lost** (the terminal's code, outside the data layer, which this commit doesn't change).
  - **Where:** `crates/jaw/src/view/attach.rs:285-299`. `send_attached` takes the box's words (`:287`) and sends them as the first file's caption (`crates/jaw/src/wire.rs:917-923`). It then forgets its draft and cancels the save it had pending (`attach.rs:292-293`). The box takes a chat's draft only when the chat is opened (`crates/jaw/src/view.rs:860-864`), and a save waits 1.5 s (`view.rs:52`, `:969-970`).
  - **What happens** when the first file is audio, whose message has no caption, so jawd leaves the draft:
    - Words typed in the last 1.5 s before Enter were never saved, so they're neither sent nor kept.
    - Words saved earlier, which jawd now keeps, are gone from the box, which stays empty until the chat is opened again. The next words typed there replace them by `draft.set`, the terminal's next save.
    - On `d53934f0` jawd cleared them all; on `7e71e2b3` it was as now.
  - **Fix, in the terminal:** save the box before asking (`save_draft`, as leaving the chat does, `view.rs:887-890`), so jawd holds the words its rule decides on. Then, once `send.media` answers, put the chat's draft back in the box as jawd left it: empty when the caption went, the words when it didn't. This also covers round one's O1 follow-up, an emptied box's pending save.
  - **The Mac's box** sends "the text … as the first one's caption" too (`docs/plans/2026-10-27-jaw-mac-box-design.md:31`), and should do the same when it sends media.
  - **Why it doesn't hold the landing:** it's an interface's, outside the data layer; jawd never had the unsaved words, so its rule can't keep them; and this commit doesn't change it.
- **O6 (hygiene, a follow-up). `caption_goes` restates `file_message`'s rule rather than reading it from the message made.** They agree for every input today: the same inputs at the same step, the same predicate, and the composer's test over every kind. Taking `typed` from the composed message (whether it carries a caption) would make that so by construction, and the test's list of kinds would no longer need keeping by hand.
- **Carried from round one:**
  - O1's other follow-up: `send.media` should refuse an audio file's caption, or the protocol should say it's dropped. The new sentence says only that audio's message has none, so the draft stays.
  - O2, main's rule, for text and media alike.
  - O4's watch list, with this round's sightings (Measurements).
  - O3 is applied at landing (below).

## Measurements

**Method:**
- **Scratch:** one folder, `/tmp/jaw-audits/media-out-landing-2/`, deleted once this note was written.
  - It held one clone of `~/src/jaw-media-out` at `81c45fa1`.
  - Its `target` was the only build folder, built with `CARGO_INCREMENTAL=0`, with `TMPDIR` and Go's build cache inside the folder.
  - The switches and my two probes were one patch, applied in the clone, built once, and the test binary copied out. The tree was then checked out clean for the full suite.
- **Bounds:** every build and test ran under `nice -n 10` and `gtimeout --kill-after`.
- **Switches:** turned on by name in `JAW_MUT`, each against 12 tests, twice:
  - the lander's five: the caption, no-caption, audio and album tests, and the composer's;
  - main's five that send beside a draft: text's `a_message_shows_pending_at_once_then_goes_sent_delivered_and_read`, `a_draft_is_kept_for_every_interface_shown_in_the_chat_list_and_cleared_when_empty`, the cards' `a_card_or_a_poll_sent_reads_its_chat_and_leaves_what_was_typed_there_where_text_sent_is_what_was`, the forward's `a_forward_sends_each_message_to_each_chat_in_order_as_new_ones_of_yours_marked_forwarded`, and `each_sending_call_that_keeps_something_is_one_synced_commit`;
  - my probes E and F.
- **Machine:** Apple M1 Ultra, 20 cores. Other agents kept the load at 60 to 75.
- **What I didn't touch:**
  - I opened nothing under `~/Library/Application Support/jaw` and copied no `session.db`.
  - The only Go engine that ran was the one tests start in their own temporary folders.
  - Nothing was sent to WhatsApp, and I signalled no process.
  - In `~/src/jaw-media-out` I only read.

**The switches** (two rounds, alike as sets, 8 to 22 s a run; probes E and F left out of the last column):

| Switch | Rule switched off | What fails |
| --- | --- | --- |
| none | | nothing |
| `A_typed_true` | `attach` passes each message's `typed` on; here every message clears, `d53934f0`'s rule | audio, no-caption |
| `A_typed_false` | the same; here none clears, `7e71e2b3`'s | caption, album |
| `M_all_typed` | `send.media` decides for each message | audio, no-caption |
| `M_caption_given` | it decides by `caption_goes`, not by whether a caption was given | audio, no-caption |
| `M_album_typed` | the album's own message isn't `typed` | no-caption |
| `C_audio_carries` | audio's message has no caption | composer, audio |
| `C_no_trim` | a caption is trimmed | composer, no-caption |
| `C_any_caption` | an empty caption doesn't go | composer, no-caption |
| `C_image_none` | a photo's goes | composer, caption, album |
| `C_video_none` | a video's goes | composer |
| `C_gif_none` | a GIF's goes | composer |
| `C_document_none` | a document's goes | composer |
| `E_all_typed` | a card or a poll isn't `typed` | caption, cards |
| `E_none_typed` | text is | cards, text |
| `T_ignored` | `enqueue_one` clears for `typed` messages alone | caption, no-caption, audio, cards |
| `F_typed` | a forward isn't `typed` | nothing, rightly: `Sent::Forwarded` still holds it |
| `H_no_here` | `Sent::Here` | nothing, rightly: a forward isn't `typed` |
| `F_typed` with `H_no_here` | both | forward |

Probe E failed with `A_typed_false`, `C_image_none` and `C_document_none`. Probe F failed with `A_typed_true`, `A_typed_false`, `M_all_typed`, `M_caption_given`, `C_audio_carries`, `C_video_none`, `C_gif_none` and `T_ignored`.

**Runs:**

| Step | Result | Took |
| --- | --- | --- |
| `cargo test -p jawd --no-run`, with the switches and probes | 0 | 3 min |
| The switches, 19 settings, twice | as above | 8 to 22 s a run |
| `make engine` | 0 | 114 s |
| `go test -race ./...` | 3 packages ok | 144 s |
| `cargo fmt --all --check` | 0 | 5 s |
| `cargo test --workspace --no-fail-fast` | 1,500 passed, 8 failed, 23 ignored. jawd's unit tests 989 passed, 6 failed, 17 ignored; the terminal's 344 passed, 1 failed, 2 ignored; failures 10 passed; composing 3, settings 3, typing 2, contacts 1; jaw-client 26 and 3; jaw-media 28 and pipeline 18; jaw-proto 25 and 3 fixtures; jaw-voice 35 passed, 1 failed; roundtrip 3; jawd's installed 1, starting 1, stopping 4 and upgrading 1 | 53 min |
| The eight failures alone, 3 runs each | 24 of 24 pass | 1 to 10 s each |

The full run's eight failures, against the flakes list:
- **On the list (five):**
  - the half-closed connection, `feed::tests::a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`;
  - the long-request lane, `feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`;
  - the socket's connection-full test, `sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`;
  - media's save-waiting test, `media::tests::a_save_waiting_for_its_file_holds_up_nothing_else_on_its_connection`;
  - media in's starts-check test, `media::tests::the_starts_check_reads_a_page_again_that_it_couldnt_and_shows_nothing_of_it_meanwhile`.
- **First sightings (three), to watch.** Each passes alone 3 of 3, in a file the 34th doesn't touch, and failed at a load of 60 to 75:
  - **The store's `store::tests::the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it`:** "the checkpointer never flushed jaw.db". It's main's (`ec83a727`, 26 October), unchanged by the landing. It writes at most 12 bursts of 4,000 messages while it waits for a checkpoint pass (`store/mod.rs:3613-3625`), and load can outlast that.
  - **The terminal's `bench::tests::a_flood_alone_is_drawn_at_most_60_times_a_second`:** "it keeps drawing what arrives", at 18 frames a second, the first after 2.9 s.
  - **jaw-voice's `tests::a_recordings_end_is_held_for_a_caller_that_falls_behind_and_never_dropped`:** its playback failed on its file.
- **Passed:** the watched `downloads_wait_while_the_disk_is_nearly_full_and_go_on_once_there_is_room`, `a_flood_of_photos_never_holds_up_a_message` and the terminal's offline-history test in `failures.rs`; and, of the list, the coalescing test `a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce` and the feed's `a_watch_result_waiting_for_room_comes_before_anything_its_window_sends`.

## At landing, once approved

1. Move `media-out` to the trial's head, `81c45fa1` (it isn't checked out anywhere, so `git branch -f media-out 81c45fa1` in `~/src/jaw-media-out`).
2. The trailers, as round one's O3 plans them, with this note's `Data-Audit: docs/audits/2026-11-02-media-out-landing.md` and media out's `Data-Audit: docs/audits/2026-10-30-media-out.md`:
   - commit 3, `ba8dd2d3` (the renumbering, the one `pub use` and the contact rig's `Makers`): both;
   - commit 34, the draft commit (`81c45fa1`): this note's alone;
   - commit 2, the engine's uploads: media out's alone;
   - the terminal's three ("Attaching files in the terminal: Ctrl-O, …", "The experiments' pinned screens take the two M4 changes on purpose: …", "The picker reads a folder once for each path typed, …"): none;
   - the other 28 of media out's 30 (its 33 but the terminal's three): media out's.
3. Commit both notes in the branch: media out's, `docs/audits/2026-10-30-media-out.md`, and this one as `docs/audits/2026-11-02-media-out-landing.md`.
4. `make check-audit`, the full gate, and the push from `~/src/jaw-media-out`, with the pre-push hook and never `--no-verify`.

# Round one: `d53934f0`

Range: `7b1ad56e..d53934f0` in `media-out-land-trial` (worktree `~/src/jaw-media-out`), 2 November 2026. It holds the 33 commits approved in round four of `docs/audits/2026-10-30-media-out.md` as `77d44257..314887ce` in `media-out`, rebased onto `origin/main` at `7b1ad56e` (presence, contact cards, and the Mac's pictures and files), and a 34th, `d53934f0`. Auditor: database-expert.

Only the landing is in scope: what the rebase does beyond keeping both sides, and the lander's two changes. Everything else stands as the media out note approved it. Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `d53934f0`.

## In short

Changes needed, for one finding marked data loss (D1), in the draft change.

- **The range-diff shows the three changes and nothing else but main's context.** `git range-diff 77d44257..314887ce 7b1ad56e..d53934f0`: 29 commits are `=`, and four are `!`.
  - Commit 2, the engine's uploads, differs only in `engine.pb.go`'s and the proto's context: main's `Typing.sender_alt = 4` beside media out's two fields.
  - Commit 3, media out in jawd (`ba8dd2d3`), differs by the renumbering, the one `pub use`, change 1, and main's context: the cards' step and lock line, `mod contact`, the `send.contact` arms, and presence's `comings` in the demo engine.
  - Commits 4 and 10, the terminal's, keep both sides.
  - The 34th is change 2.
  - `e06dc810..7b1ad56e` touches only `mac/` and `docs/`.
- **The renumbering is right.** The lock's line 27 has the hash approved as 26 (`0c7b0505…`), after main's `26 schema 3732b60a…`. The migration's SQL is byte for byte the approved one; only its comment says 27 (`store/schema.rs:1410`), after the cards' step whole (`:1402`). Nothing in the code or tests names it by number. The `pub use` holds both `contacts` and `outgoing` (`store/mod.rs`).
- **Change 1 stands as keeping both sides,** under the presence landing note's rule for later landings (below).
- **Change 2 clears the draft wherever `send.media` writes, including where what was typed doesn't go (D1).** Words sent as an audio file's caption are dropped by the composer and then cleared from the draft in the same commit, so they're neither sent nor kept. A file sent with no caption also clears a draft it didn't send.
- **Otherwise the draft rule holds on the data layer's terms** (the table below): one synced commit, replays and restarts, an album, and other devices' sends. The one exception is a draft saved from another interface while the files are kept (O2), which main's `send.text` has as well.
- **Every rule at the meeting point fails a test when switched off:** the new clause, the cards' rule (a card or poll leaves the draft), text's rule, and a forward's.
- **The full suite passes under the flakes rule:** 1,498 passed, 6 failed and 23 ignored, and each of the six passes alone 3 of 3. Four are on the list; two are new sightings (O4). `go test -race` and `cargo fmt --check` pass.

## The renumbering and the `pub use`

- `store/migrations.lock` ends `25 data d3e337e2…`, `26 schema 3732b60a…` (contact cards), `27 schema 0c7b0505…` (media out, its hash as approved).
- The commit message says "Migration 27, a schema step".
- No test or code names the migration by number (searched for `migration 2[67]`, `MIGRATIONS[2x]`, `user_version` and lock lines).
- `kept.rs` seeds media out's two tables at 27: the lander's run with a throwaway 28 shows it, and the full suite runs its test as it is.
- A jaw.db made by the unlanded branch's jawd at its old 26 isn't upgradable, as for any unlanded branch. The owner's jaw.db is made by main's jawd.

## Change 1: the contact rig's `Makers`

- **What:** `send/contact/tests.rs:14`, `:67-70` pass `Makers { previews: link::Previews::off(), media: None }` to `Sending::start`, whose last parameter media out made `makers: Makers` (`send/mod.rs:174-183`).
- **The places landing note's rule doesn't cover it,** since its condition 1 needs a parameter main added.
- **The presence landing note's rule for later landings does.** Each of its conditions holds:
  - **A branch's own new parameter, at a call in a test main added after the branch's base.** `Makers` is media out's. The rig is contact cards' (`fb19ae45`, after `77d44257`).
  - **The value the branch passes at main's other such calls.** It's the same value at `send/tests.rs:105`, `send/poll/tests.rs:53`, `send/places/tests.rs:52`, `feed/tests.rs:1176` and `send/bench.rs:69`. Its `previews` is the very value main's rig passed.
  - **That value means "none" in production.** `media: None` is a jawd that keeps no media: `send.media` is refused (`send/media.rs:114-116`), `send.retry` wakes nothing (`send/outbox.rs:617`, `:649`), and the copies' watch never moves (`send/mod.rs:186`).
  - **The test doesn't use what the parameter adds.** The cards' tests send no media; the file's one mention of media is this line.
- It's folded into `ba8dd2d3`, where `Makers` comes in, so every commit builds.

## Change 2: `send.media` clears its chat's draft

`Outgoing` carries `typed` (`store/write/outbox.rs:787-794`). It's set by `enqueue` for text alone (`:780`, main's `kind == "text"`), by `attach` for every message `send.media` writes (`:935`), and never by a forward (`:889`). `enqueue_one` clears the draft for a `typed` message sent in its chat (`:825`), by main's `CLEAR_DRAFT` (`:206`), which clears whatever draft the chat has at the commit.

| What the brief asked | At the trial |
| --- | --- |
| In the synced commit `send.media` answers after | Yes. The clear is in `attach`'s transaction, the one `durably` commits before the answer (`send/media.rs:350`). If it fails, the clear rolls back with the messages. |
| A replay with the same key clears nothing twice, nor a newer draft | Holds. The reader's look (`send/media.rs:134-139`) answers before anything is kept, and `attach`'s look (`outbox.rs:912-917`) returns before `enqueue_one`. Probe B: an album with a key, then `draft.set` "newer", then the same request answers the same messages, writes nothing, and leaves "newer", also through the copies' later steps. |
| A restart, a kill at any commit | Holds. Only `attach`'s commit clears. A kill before it writes nothing (the next start removes the kept files), and the key's retry clears once. A kill after it leaves the clear on disk with the messages, and the retry answers without writing. No later step of a send writes the draft, and nothing runs `attach` at a start. (By reading: I killed no jawd for this.) |
| An album clears once | Holds. One transaction, and `CLEAR_DRAFT`'s `draft IS NOT NULL`: the album's message clears it, and its files' messages find none, so no second stamp. |
| Only sends made here | Holds. Only interface requests reach `enqueue_one`: text, polls, cards, places and events (`send/outbox.rs:286`, `send/poll.rs:140`, `send/contact.rs:123`, `send/places.rs:103`, `send/events.rs:114`), forwards (`send/forward.rs:127`) and `send.media` (`send/media.rs:348`). Your phone's messages come through the inbox, which writes no draft. Only `draft.set` sets one (`send/mod.rs:347`), and the chat upsert keeps it when none is given (`store/write.rs:1469`). `send.retry` takes its own step. |
| A newer draft typed after the send | From the same interface, holds: `send.media` and `draft.set` are lane requests, answered in order (`sock.rs:401`, `:570-575`). From another interface while the files are kept, it doesn't (O2). |
| The cards' rule and the new clause fail their tests when switched off | Yes, both rounds (Measurements). |
| What was typed is never lost | No: D1. |

## Findings

### Data loss

- **D1. Words sent as an audio file's caption are neither sent nor kept, and a file sent without a caption clears a draft it didn't send.**
  - **Where:** `store/write/outbox.rs:935` (`typed: true` for every message `attach` writes), with `send/compose.rs:83-91`. An audio file's message has no caption (WhatsApp's `AudioMessage` has no field for one), and the caption given is dropped without a word. The protocol says "A caption goes with its own file" (`protocol/README.md:433`).
  - **What happens:**
    - The terminal sends what's typed as the first file's caption, whatever the file is (`crates/jaw/src/wire.rs:921-922`). Once it has asked, it empties the box and forgets its draft (`crates/jaw/src/view/attach.rs:287-293`), counting on jawd to clear it as it does for text.
    - jawd sends the audio without the words, and change 2 clears the draft in the same commit.
    - **Probe A:** a draft "for the drive", then `send.media` of a WAV with it as the caption. The audio is sent, its row's `text` is NULL, and the chat's `draft` is NULL. With `attach`'s `typed` switched off, which is `7e71e2b3`'s behaviour, the draft keeps the words.
    - **Probe D:** a photo sent with no caption clears the chat's draft too. An interface or script that sends a file without what's typed, as a drop that sends at once would, loses the draft. The terminal can't do this today, since its caption is always the box's words.
  - **Why it's this landing's:** on media out's approved base every send cleared the draft, so the approved branch did the same. But the media out note never looked at drafts (it doesn't name them), and change 2 is what brings it back.
  - **Fix:** clear the draft only when what was typed goes with the send, as main's reason for text has it ("since it's what was typed there").
    - `typed`, for a file's message, is whether its caption goes. That means a caption is given, it isn't empty once trimmed (as `send/compose.rs:54` trims it), and the file is a photo, video, GIF or document.
    - It's false for an audio file, for a file without a caption, and for the album's own message.
    - Decide it where `send.media` composes the messages (`send/media.rs:314-345`), and carry it to `attach`, for example as a field of `Attached`.
  - **Tests,** each failing with its part switched off:
    - the lander's (a photo's caption clears the draft; a card and a poll leave it);
    - an audio file with the draft as its caption leaves the draft;
    - a photo without a caption leaves the draft;
    - an album whose first photo carries the caption clears it, and a replay with its key leaves a newer draft.
    - My probes are in the store as `integration/media-out-landing-probes.diff`, written against `d53934f0`, for the builder to adapt: A, B and D for these tests, and C for O2.
  - **Or the lander's option (a),** which also loses nothing: `send.media` leaves the draft as on `7e71e2b3`, and interfaces clear it themselves. Probes A and D then keep the words. That's for the owner; either way, O1.

### Inconsistency

None.

### Test gap

- **T1 (with D1's round): the lander's test sends one photo.** No test covers an album's send, a replay with the key, or the copies' later steps against the draft. Probe B covers all three, passes at the trial, and fails with `attach`'s `typed` off. It's one of D1's tests above.

### Unpredictable performance

None.
- The change adds no statement. `CLEAR_DRAFT` is main's and pinned (`store/plans.txt:3011`, the outbox's plan test at `outbox.rs:1630`): one UPDATE by rowid in `attach`'s transaction, once a send.
- The lander's `JAW_PLANS=write` left `plans.txt` as it was.

### Other

- **O1. The protocol doesn't say what `send.media` does to the draft.**
  - **Where:** the Media out section (`protocol/README.md:420-438`) is silent, where the protocol says it for every other send: `send.text` clears it (`:212`); `send.poll` and `send.contact` leave it (`:220`, `:222`); a forward leaves the chats' drafts.
  - **Why it matters:** the terminal counts on jawd to clear it (`view/attach.rs:292-293`), and the Mac's sending will read it there.
  - **Fix:** say it, as D1's fix decides.
  - **With D1's fix, a terminal follow-up:** when it sends no caption, the terminal should keep a pending save of an emptied box (`draft_due`), so words deleted just before the send don't come back.
  - **Media out's, a follow-up:** `send.media` should refuse a caption on an audio file (-32602, as for a kind it can't send), or the protocol should say it's dropped.
- **O2 (main's rule, follow-up): a draft saved from another interface while `send.media` keeps its files is cleared by the send's commit.**
  - **Probe C:** `send.media` is held after its keep (`pause_next_media`), `draft.set` "and the picnic" goes through another call, and the send is let go. The draft is NULL.
  - **Within one interface it can't happen,** since the lane answers in order.
  - **Main's `send.text` has the same window,** up to its link preview's 3 s (`send/outbox.rs:218`, `:256-258`), and so did media out on its approved base. The trial gives `send.media` main's rule over the time its files take to keep and probe, which is seconds or more for a file copied from another volume.
  - **Fix, for text and media alike, beyond this landing:** the request reads the chat's draft when it starts, and the write clears it only if it's still that one (`AND draft IS ?3`, still by rowid).
- **O3. `make check-audit` can't see the integration,** since the trailers will name the approved note. Once a round approves, the commits the landing changed take that round's landing note's trailer:
  - `ba8dd2d3` (the renumbering, the `pub use` and change 1), beside media out's note's;
  - the draft commit (`d53934f0`, or what replaces it), alone, since media out's note never saw it.
  - Commit 2, whose `engine.pb.go` is regenerated from both sides' proto, takes media out's trailer alone. Commits 4 and 10 are the terminal's and take none, as the lander planned.
- **O4. The flakes list.** My full run failed six tests, and each passed alone 3 of 3.
  - **Four are on the list:**
    - the one the list calls the socket's, `feed::tests::a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`;
    - the long-request lane, `feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`;
    - media's `a_save_waiting_for_its_file_holds_up_nothing_else_on_its_connection`;
    - the socket's connection-full test, `sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`.
  - **Two aren't, and are first sightings, to watch:**
    - **Media in's `media::tests::the_starts_check_reads_a_page_again_that_it_couldnt_and_shows_nothing_of_it_meanwhile`.** It failed at its status read (`media/tests.rs:2305`), which media out's commit 17 says failed in three full runs under load before that commit. This is its first sighting since.
    - **The terminal's `with_the_phone_offline_older_history_waits_and_says_so_while_messages_still_go`** (`crates/jaw/tests/failures.rs:353`). Its last frame lacked "an older message from the phone, 3 of 3". Its earlier breaks were demo messages added to Ana's chat; this one passes alone, so it's load.
  - The coalescing test `a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce` and the feed's `a_watch_result_waiting_for_room_comes_before_anything_its_window_sends` passed. So did the watched `a_flood_of_photos_never_holds_up_a_message`.

## Measurements

**Method:**
- **Scratch:** one folder, `/tmp/jaw-audits/media-out-landing/`, deleted once this note was written.
  - It held one clone of `~/src/jaw-media-out` at `d53934f0`, with `media-out` at `7e71e2b3`.
  - Its `target` was the only build folder, built with `CARGO_INCREMENTAL=0`, with `TMPDIR` and Go's build cache inside the folder.
  - The probes and switches were one patch, applied in the clone, built once and copied out. The tree was then checked out clean for the full suite.
- **Bounds:** every build and test ran under `nice -n 10` and `gtimeout --kill-after`.
- **Switches:** turned on by name in `JAW_MUT`. Each was run twice against 11 tests: every test whose name says draft, main's text, cards and forward tests, the lander's test, and probes A to D.
- **Machine:** Apple M1 Ultra, 20 cores. Other agents kept the load at 16 to 56.
- **What I didn't touch:**
  - I opened nothing under `~/Library/Application Support/jaw` and copied no `session.db`.
  - The only Go engine that ran was the one tests start in their own temporary folders, unlinked.
  - Nothing was sent to WhatsApp, and I signalled no process.
  - In `~/src/jaw-media-out` I only read.

**Probes at the trial:**

| Probe | Result |
| --- | --- |
| A: a draft sent as a WAV's caption | fails: audio sent, its `text` NULL, `draft` NULL |
| B: an album with a key, a newer draft, the same request again | passes |
| C: `draft.set` from another call while `send.media` is held after its keep | fails: `draft` NULL |
| D: a photo without a caption, with a draft | `draft` NULL (prints only) |
| The lander's test | passes |

**The switches** (two rounds, alike; probes A and C fail in every row but the second, so they're left out):

| Switch | Rule switched off | What fails |
| --- | --- | --- |
| none | | nothing else |
| `S_attach_untyped` | `send.media` clears the draft (`:935`) | the lander's test and probe B; probes A and C pass |
| `S_enqueue_all_typed` | a card, poll, place or event leaves it (`:780`) | main's cards test and the lander's test |
| `S_enqueue_none_typed` | text clears it (`:780`) | main's text test (`a_message_shows_pending_at_once_then_goes_sent_delivered_and_read`) and the cards test |
| `S_forward_typed` | a forward isn't `typed` (`:889`) | nothing, rightly: `Sent::Here` still holds it |
| `S_no_here_check` | `Sent::Here` (`:825`) | nothing, rightly: a forward isn't `typed` |
| `S_forward_typed_anywhere` | both | main's forward test |

**Runs:**

| Step | Result | Took |
| --- | --- | --- |
| `make engine` | 0 | 30 s |
| `go test -race ./...` | 3 packages ok | 54 s |
| `cargo test --workspace --no-run` | 0 | 82 s |
| `cargo test --workspace --no-fail-fast` | 1,498 passed, 6 failed (O4), 23 ignored. jawd's unit tests 986 passed, 5 failed, 17 ignored; the terminal's 345 passed, 2 ignored; failures 9 passed, 1 failed; composing 3, settings 3, typing 2, contacts 1; jaw-client 26 and 3; jaw-media 28 and pipeline 18; jaw-proto 25 and 3 fixtures; jaw-voice 36; roundtrip 3; jawd's installed 1, starting 1, stopping 4 and upgrading 1 | 47 min |
| The six failures alone, 3 runs each | 18 of 18 pass | 0.3 to 65 s each |
| `cargo fmt --all --check` | 0 | |
| Probes and switches, one build | as above | 2 min to build, 13 s a run |

## Verdict

D1 is data loss: at the trial, words sent as an audio file's caption are dropped by the composer and cleared from the draft in the same commit, and a file sent without a caption clears a draft it didn't send. The rest of the landing keeps both sides:
- the renumbering and the `pub use` are right;
- change 1 stands under the presence note's rule;
- the draft rule holds for replays, restarts, albums and other devices' sends.

**What must change:** D1's fix, either clearing only when a caption goes, or option (a), with its tests and O1's protocol sentence. Then a short round on the new draft commit alone, which takes O3's trailers once approved.

Verdict: changes needed

# Verdict of round two (`81c45fa1`)

D1 is closed in jawd. A file's message clears its chat's draft only when its caption goes, decided from what the composer sends, so an audio file's caption and the draft beside a file sent without one are kept. T1 and O1 are closed, and every clause of the rule fails a test when switched off. Nothing else in the landing changed since round one, and the full suite passes under the flakes rule. O5 (the terminal's) and O6 stand as follow-ups, and the landing takes the steps in round two's "At landing, once approved".

Verdict: approved
