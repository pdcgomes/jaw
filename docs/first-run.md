# Your first run

What happens the first time jaw runs on your account, and what to look at. Your `jaw.db` was made by the spike on 16 October and is still at schema version 1; the first jawd from `main` upgrades it once.

## Before you start

- **Use `main` as the canvas says.** The workstream canvas's "Needs you" says when `main` is ready; until it does, a change to the data layer may still be waiting for its audit.
- **Room on the disk.** The upgrade backs `jaw.db` up first and needs about three times its size free (about 220 MB for its 73 MB). jawd refuses the upgrade, and says how much it needs, if there isn't room.
- **Nothing else running.** jawd takes `jaw.lock` and refuses to start if another jawd holds it.

## Build and start

```sh
cd ~/src/jaw
git pull
make build          # target/engine/jaw-engine, target/debug/jawd and target/debug/jaw
make hooks          # the pre-push audit check, if this checkout doesn't have it yet
```

Then either start jawd in a terminal of its own, to watch its log as it upgrades:

```sh
target/debug/jawd
```

and the interface in another:

```sh
target/debug/jaw
```

or run `target/debug/jaw` alone, which starts jawd if nothing answers (its log goes to `jawd.log` in jaw's folder, `~/Library/Application Support/jaw`).

For a release build, use `cargo build --release` and `target/release/…`; jawd finds the engine at `target/engine/jaw-engine` either way.

## What the first start does

1. **Backs up `jaw.db`** whole and synced, to `jaw.db.before-v…` beside it, before the first data step. The backup is kept for 30 days (the newest only), and `jaw status` shows it.
2. **Upgrades from version 1** through every migration, each checked by its schema fingerprint and foreign keys; on a copy of your database this took about 5.5 s.
3. **Converts what the spike kept raw:** 84,394 history rows become about 84,540 messages in 450 chats. What isn't a message (997 edits, 21 live-location positions, 7 receipt-only shells) is kept whole, not dropped.
4. **Fetches app state again in full** (pins, archives, mutes, read marks), since the last stop wasn't a clean one by this jawd's standards. Watch for the log line "fetched app state again in full in N s"; its cost on a real account hasn't been measured.
5. **Catches up** on what arrived while nothing was running, then notifies once with a summary, such as "12 new messages in 4 chats".
6. jaw may show "not linked" for a moment until the engine reports, if `linked-at-hello` hasn't landed yet; it moves on by itself.

Reading every chat afterwards owes about 69 read receipts, one per unread message, and none for the history.

## What to look at

- `target/debug/jaw status`: the connection, the engine, storage (the WAL's size, the backup), the outbox, quarantine and merges in progress. Quarantine holds anything jawd couldn't convert, kept whole rather than dropped; a few rows from the spike's history notifications are expected there.
- Send a message to yourself or a friend and watch its ticks: pending, sent, delivered, read.
- `target/debug/jaw agent install` starts jawd at login, if you want that; `jaw agent status` and `jaw agent uninstall` undo it.

## On the spare number, when you can

These need your phone and the spare number; nothing here is automated.

1. **A message ID sent twice** (the sending audit's I2c): send a message to the second device, kill `jaw-engine` with `kill -9` right after the phone shows it and before jawd records it as sent, restart jawd, and read the log line "WhatsApp answered <id> in <chat> sent again with: …". Check the recipient shows it once. Repeat with read receipts off on the recipient.
2. **Receipts for a LID-addressed message at the kept chat's JID** (the sending audit's H15): after a person's number chat and LID chat merge, read a message that came under the LID and check the sender sees it read.
3. **Pictures in Ghostty and iTerm2:** `target/debug/jaw demo`, Ana's lake photo, then Família Gomes; scroll with Page Up and Page Down and look for pieces left behind.
4. **SD and HD limits:** send a photo and a video from the phone in SD and HD, and compare with `crates/jaw-media/src/limits.rs`, whose numbers are estimates.

If anything looks wrong, stop jawd (Ctrl-C, or `kill <its pid>`, never by name), keep `jaw.db.before-v…`, and the canvas's log says what each step did.
