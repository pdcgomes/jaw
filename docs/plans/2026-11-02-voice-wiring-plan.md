# jawd: voice messages wired in (M5's last piece)

**Goal:** voice messages in jawd, for every interface: a received one shows its length and waveform and plays from the cache, downloaded first if it isn't there, with its playhead ten times a second, its speed and its seek; you record one with its levels as you speak, WhatsApp seeing you're recording, then stop, listen, and send it or throw it away; it goes through the outbox and media out's upload as WhatsApp's voice message; and, last, the played receipt for someone else's. The protocol's "Voice (proposed)" becomes final. The terminal's player and recorder, built against the protocol, then work on a real jawd.

**Architecture:** a `voice` module in jawd owns jaw-voice's one audio thread, started with jawd and holding no device while idle, and a task that is the only thing that drives it: every `voice.*` request and every event of the thread's goes through it in order, so "one thing at a time" is one place's state. It finds a voice message's file through media (`Media::voice_file`), which wants it at interactive priority, as `media.fetch` does, when it isn't in the cache; it turns the thread's events into the `playback` and `recording` pushes; it tells WhatsApp you're recording through sending's typing; and `voice.send` hands the stopped recording to sending (`Sending::send_voice`), which writes it through the outbox as `send.media` writes a file. A voice message's length and waveform are render-ready in its row's `detail`, as its converter reads them from its raw protobuf.

**Tech stack:** Rust (jawd, jaw-proto, jaw-voice as built), rusqlite, the demo engine; jaw-voice's `FakeSpeaker` and `FakeMicrophone` in every test, with its fixture files.

## Questions

What the design and the protocol leave open, each settled as its lean, each a small change in the one place named.

1. **Does a stopped recording outlive jawd?** (the jaw-voice audit's J2.) A: no. A recording, going or stopped, lasts while jawd runs and belongs to the connection that asked for it; a start empties the recordings folder of whatever a stopped jawd left, `.part` files and whole ones alike, since no row ever names one; a stop throws away a recording going or stopped. Nothing was acknowledged as kept past jawd's life, and no interface could be told of one after a restart (lean). B: a row in a table of jaw's own, written synced before `voice.stop` answers, and a way for interfaces to learn of it when they connect; a protocol change and a migration for little.
2. **When the connection that recorded ends.** A: its recording is thrown away, going or stopped, so a terminal that quits or dies mid-recording doesn't leave the microphone on, and WhatsApp told you're recording, for up to fifteen minutes; and what it plays stops (lean). B: kept, stopped, for any interface to send; but none could see it.
3. **`voice.send` with `at` or `repeat`.** M7's scheduler isn't on main, and its own design comes after M5. A: jawd takes both parameters by name and refuses either, -32602 with `data` `{"param": "at"}` or `{"param": "repeat"}`, until the scheduler serves them; the recording stays as it was, so it can go now or be thrown away, and nothing meant for later goes now (lean). B: ignore them, which sends now what was meant for later.
4. **Idempotence.** A: `voice.send` takes `key`, as every send does, answered with the same message within a day, through the outbox's own look for it (`forwarded_by_key`, as `send.media`'s); the terminal sends one (lean).
5. **A sender's waveform.** A: from the message's `AudioMessage.waveform` when it has 64 points, each 0 to 100, as WhatsApp's phones send it; without one, `voice` has `seconds` alone and an interface draws a plain bar (lean). A waveform drawn from the downloaded file for those (jaw-voice's `analyse`) is a follow-up.
6. **The microphone on macOS.** A: jawd's Info.plist, which `make mac` embeds today, is embedded by jawd's build in every macOS build of its binary, so a jawd from `make install` or Homebrew asks in jaw's words too; macOS asks whoever is responsible for jawd once: jawd itself when the app or launchd started it, the terminal app when a terminal did. A refusal is digital silence, which jaw-voice notices (`recording` `silent`), and `status` says so (lean).

## Global constraints

- The spec is the design's "Voice", M5 and Media's "Incoming" and "Outgoing"; the protocol's `voice.*` rows, "Voice (proposed)", the `playback` and `recording` pushes and "Sending later"; the jaw-voice audit's J1 to J10 (`docs/audits/2026-10-16-voice.md`); and `.cursor/rules/data-integrity.mdc`. Where this plan and they differ, they win.
- Every send through the outbox's rules: idempotent, answered only after a synced commit, bounded and in `jawd status`.
- Tests use files and jaw-voice's synthetic devices, never a real microphone or speaker, and never send anything to WhatsApp: the demo engine stands in for it.
- The worktree is `~/src/jaw-voice-wiring`, on `voice-wiring` from main at `ea0f4134`. Scratch is `/tmp/jaw-voice-wiring/`, deleted before the report, with `make prune-builds`. Nothing is pushed: the database expert reads it first.
- A commit for each task, at least every hour; commit messages as the project's.
- Migrations: the next number, 28, taken now and renumbered at landing if another lands first.
- Played receipts are the last task, built only once disappearing and view-once messages (`~/src/jaw-disappearing`) has landed with the engine's played `MarkRead` and the outbox's `played`, which they reuse; until then the task stops before it starts.

## Files

- `crates/jaw-proto/src/voice.rs` (new): the requests' params, the two pushes, `status`'s `voice`.
- `crates/jawd/src/voice/mod.rs`, `voice/folder.rs`, `voice/tests.rs` (new): the module, its folder, its tests.
- `crates/jawd/src/send/voice.rs` (new): `voice.send` through the outbox; `send/compose.rs`'s `voice_message`.
- `crates/jawd/src/media/mod.rs`, `media/outgoing.rs`: a message's voice file for playing; a voice copy planned from its own file, without ffprobe.
- `crates/jawd/src/convert/kinds.rs`, `store/write/voices.rs` (new), `store/schema.rs`: `detail.voice`, and the `voices` backfill with migration 30.
- `crates/jawd/src/sock.rs`, `state.rs`, `main.rs`: routing, the connection's end, status, start and stop.
- `crates/jawd/build.rs` (new), `mac/scripts/bundle`: the Info.plist.
- `crates/jaw/src/wire.rs`: `voice.send`'s key.
- `protocol/README.md`, `scripts/check-data-audit.py` (the voice module joins the data layer).

## Task 1 · A voice message's row: its length and waveform

A message of kind `voice` has `voice` in its row: `seconds`, from its `AudioMessage`, and `waveform`, its 64 points when the sender sent them (question 5). They're render-ready in `detail`, made by the converter from the raw protobuf, so they're the same however the message came: live, from history, sent by you. The voice messages jaw.db already holds are read again from their raw protobufs by a backfill, `voices`, declared by migration 28, a schema step that only adds its `backfills` row; it walks `messages` by `n` in the background lane's short transactions, as `contacts` does, each voice message taking the detail it converts to now at a new version with its event where that changes it, one whose conversion fails quarantined while the step goes on. `status`'s `stored` gains `voices_backfill`, the messages still to be looked at.

Done when: a voice message's row, live or from history, carries `voice` with its length and waveform; one stored by an earlier jawd gets it from the backfill, in steps, resumed after a restart, idempotent, a version and an event each; the plan test pins the new statements; the migration's hash is in the frozen list.

Tests: `convert/tests.rs`: a PTT `AudioMessage` with and without a waveform, one of the wrong length, and an audio file, which has none. `store/write/voices/tests.rs`: an earlier jawd's voice messages (the `detail` written without `voice`) take it from the backfill, a message changed only where it changes, the rest left alone, a restart halfway through resuming, a replay changing nothing, a raw protobuf that doesn't read left as it is, one whose conversion panics quarantined while the rest go on. The migration test's frozen hashes and the plan test.

## Task 2 · Playing

`voice.play` (`message`, `at`, `speed`, or `recording: true`), `voice.pause`, `voice.seek` (`at`) and `voice.speed` (`speed`), and the `playback` push, as the protocol has them. jawd starts jaw-voice's thread with the system's speaker and microphone (fakes in tests) and one task that owns what's playing.

- **From the cache.** The message is found as media finds a file for `media.fetch`, by its ID and optionally its chat: error 3 for one jawd doesn't have, -32602 with `param` `chat` for an ID in two chats, 4 for one deleted, one to view once (whose file jaw never keeps), or one that isn't a voice message. Its file in the cache is played, marked used so the cache keeps it longest. One not in the cache is wanted now, synced, at interactive priority, as `media.fetch` wants one, and the answer goes; it plays once its file is in, unless something has taken over since, and a download that fails says why with `failed`.
- **Speed and seek.** `speed` is 1, 1.5 or 2 (-32602 with `param` `speed` otherwise), and `at` a number of seconds from 0 (-32602 with `param` `at`); a seek past the end ends it. Without `at`, a message paused carries on where it paused; any other starts at 0.
- **One thing at a time.** One message plays at once, on jawd's machine: a new `voice.play` stops the last, which is pushed `stopped`, and replaces one waiting for its file; `voice.play` of a message while a recording is going is error 4, since a recording is never stopped for it; `voice.record` stops what plays first.
- **`playback`,** to every connection, ten times a second while playing and at each change: `message` (or `recording: true`), `state`, `at`, `seconds`, `speed`, and `error` for `failed`. What a connection played stops when it ends, unless another has played something since.
- **`status`** gains `voice`: what's `playing` (a message's ID, `recording`, or null) and its `state`, the thread's `underruns`, `overruns`, `dropped_events` and `concealed`.

Done when: an interface plays, pauses, seeks and changes the speed of a voice message in the cache and one that downloads first, and the pushes say so; every refusal is as the protocol says.

Tests (`voice/tests.rs`, on a rig like media out's, the demo engine and jaw-voice's fakes, its speaker paced fast): a message in the cache plays with pushes at the tick and ends at its length; pause, then `voice.play` without `at`, carries on from there; seek and speed are pushed and change the playhead; one not in the cache is downloaded, at interactive priority, then played; one whose download fails is pushed `failed` with why; a second `voice.play` stops the first with `stopped`; a play waiting for its file is replaced by a newer; no audio output is `failed` with why; a file that isn't a voice message is `failed`; deleted, view-once, not voice, missing, and two chats are refused as the protocol says; a bad speed and `at` are -32602; a connection that ends stops what it plays.

## Task 3 · Recording

`voice.record` (`chat`), `voice.stop`, `voice.cancel`, `voice.play` with `recording: true`, and the `recording` push.

- **The folder,** `voice/recordings` in jaw's folder, mode 0700, made and synced at the start, then emptied of what a stopped jawd left (question 1), the count in `status`; a hard link made and removed there once (J9), since a recording is put in place by one, and `status` says whether it works.
- **`voice.record`:** the chat must be one jawd has (3); a recording going or stopped is error 4; the disk must have 64 MB free (error 5, `{"limit": "disk"}`, J6); a folder without hard links is 4. Each recording goes to a fresh name, the message ID made for it up front (J1), never used again. Playback stops first. It answers once the thread has started it, its first level, or with the thread's refusal (J10), a busy thread being an error and never a success (J4). jawd tells WhatsApp you're recording in the chat as it starts, again every 5 seconds so typing's quiet spell never stops it, and paused as it ends, however it ends.
- **`recording`,** to every connection: `level` ten times a second; `silent` once, after a second of digital silence; `stopped` with `waveform` and `end` (`stopped`, `limit` at 15 minutes, `interrupted` with `reason`); `cancelled`; `failed` with `error`.
- **`voice.stop`** answers once the recording is synced and in place under its name (J4), with what was recorded pushed as `stopped`; with nothing recorded it's error 3, and once stopped it changes nothing. **`voice.cancel`** throws it away, going or stopped, its file removed. **`voice.play` with `recording: true`** plays the stopped recording; error 4 while it's still going, 3 with none.
- **The connection that asked, and jawd's stop** (question 2): its recording is thrown away when it ends; a stop throws away a recording going or stopped, which nothing acknowledged as kept (J5's other side).
- **`status`'s `voice`:** the recording's `state` and `seconds`, the folder's `files` and `bytes` (one recording at most, up to 2.7 MB), `removed_at_start`, `hard_links`, and the last recording's `gaps_ms` and whether it was `silent`.

Done when: an interface records with levels, stops and listens, or throws it away; WhatsApp is told you're recording while it lasts and no longer once it ends; nothing is left in the folder but the one recording.

Tests: a recording from the fake microphone pushes levels at the tick, then `stopped` with its waveform and length, the file whole under its name before `voice.stop` answers; `voice.play` with `recording: true` plays it; cancel removes it; the microphone failing midway ends `interrupted` with what came before kept; no microphone is refused and leaves nothing; silence is `silent`; 15 minutes caps it (`limit`, with the limit shortened by the thread's `record` call, which takes one); a second `voice.record` is 4, going or stopped; a missing chat is 3; the disk too full is 5; a start empties the folder of a stopped jawd's files and parts; the typing said to the demo engine, again while it lasts, and paused at its end; a connection ending throws its recording away.

## Task 4 · Sending

`voice.send` (`key`, `at`, `repeat`) sends the stopped recording, or stops one going and sends it, to the chat it was recorded in, as a message of yours.

- **`at` and `repeat`** are refused, as question 3 has it, before anything changes.
- **Through the outbox.** jaw's copy of the recording is kept in `media/outgoing`, cloned, as `send.media` keeps a file you picked; then one synced commit writes the message, pending, its outbox row and its copy to make (`outbox::Step::Attach`, `typed` false, since a voice message carries no caption and the chat's draft stays), before it answers `message` and `row`, as `send.text` does; and the recording's own file goes after that commit. The outbox's 1,000 and the 8 GB kept for copies bound it, each refused as `send.media`'s are, the recording kept as it was. `key` answers again with the same message within a day. Sending reads the chat up to it, as sending a file does.
- **WhatsApp's voice message:** an `AudioMessage` with `ptt`, `audio/ogg; codecs=opus`, its seconds rounded up, the recording's waveform, and the chat's disappearing timer; the copy goes as it is (jaw-media's plan for Ogg Opus, planned from the recording itself, so no ffprobe or ffmpeg is needed), uploaded with WhatsApp's audio keys, and the message goes once it's uploaded, in its chat's order; failures and retries as media out's.
- **Refusals:** no recording is error 3 (unless its key was seen); the recording's chat gone is 3; a recording that ended with nothing in it is 4.

Done when: a recorded voice message reaches the demo engine's WhatsApp as a PTT with its waveform, length and Ogg Opus file, after its row and outbox row are synced; asked again with its key it answers the same and sends nothing new.

Tests: what the demo engine is sent and uploaded (a PTT, its mime, seconds, waveform, the file byte for byte, audio keys); the answer comes after the synced commit (as media out's test of it); the same key answers the same message; the outbox full and the kept bytes past their bound are refused and the recording kept; `at` and `repeat` refused with the recording kept; sending while recording stops it first; the chat's draft stays; a disappearing chat's timer goes with it; the recording's file is gone once it's sent and jaw's copy goes once its copy is made; with no ffmpeg on the path it still goes.

## Task 5 · The microphone on macOS

`crates/jawd/build.rs` embeds `mac/JawApp/Helpers/jawd-Info.plist` in jawd's binary on macOS (`-sectcreate __TEXT __info_plist`, for the binary alone), and `mac/scripts/bundle` stops embedding it a second time. A refusal shows as `silent`, which `status`'s `voice` keeps for the last recording; the protocol says how each start of jawd is asked.

Done when: every macOS build of jawd carries its Info.plist, and the app's bundle builds with it once.

Tests: an integration test reads the built jawd and finds its `NSMicrophoneUsageDescription` in `__TEXT,__info_plist` (macOS only).

## Task 6 · Played receipts (last, once disappearing has landed)

As disappearing sends a view-once message's: jawd's own accepted `voice.play` of someone else's voice message not yet played (J7) writes, in one synced commit before it answers, that it was played and an outbox row of kind `played` whose raw is the engine's `MarkRead` with `type` `played`, sent once, settled by its `Done`, never "may have gone"; the row's `voice` gains `played`, from what's kept, and a `played-self` receipt from your phone marks it played too. Nothing of it is built before disappearing lands; if it hasn't by then, this plan stops here and says so.

Done when: playing someone else's voice message sends its played receipt once, however often it's played and however jawd stops; its row says `played`.

Tests: the receipt sent once to the demo engine after the commit; a replay of `voice.play`, a restart before it's sent, and your phone's own played receipt each change nothing twice; your own message sends none.

## The protocol

"Voice (proposed)" becomes "Voice": the questions' answers, the refusals, `key`, the row's `voice`, `status`'s `voice`, the microphone, and the connection that records. Its rows in Methods and Other pushes say the same.
