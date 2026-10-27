# jaw: scheduled messages and reminders

27 October 2026. A design; nothing here is built. It details M7 of [jaw's design](2026-10-16-jaw-design.md) ("Scheduled messages and reminders"): jawd's step, the terminal's screens, and the Mac's, which [the Mac design](2026-10-26-jaw-mac-design.md) lists as its proposal P9. The screens are drawn in the scheduled messages and reminders canvas (`scheduled-reminders-wireframes.canvas.tsx` in the project's canvases), which also holds the questions left on them. The owner's answers are marked "Owner, 27 Oct".

## What it is

The owner asked for two things on 27 October:

- **Scheduled messages:** a message written now and sent later. WhatsApp is trying this in its own apps, but jawd can do it entirely itself, with screens in the terminal and on the Mac to see, edit and cancel what's waiting.
- **Reminders on messages:** being reminded of a message at a time, as WhatsApp is working on too.

Both are a time jawd waits for, so they share one step, one timer and a pair of lists.

## Decisions

- **In jawd, not WhatsApp's way** (Owner, 27 Oct). jawd keeps the message and sends an ordinary one at its time. WhatsApp's own scheduled messages go ahead of time as a `ConditionalRevealMessage`, readable once `ScheduledMessageMetadata`'s key is revealed at the time, and are taken back with `MESSAGE_UNSCHEDULE`; they need the recipients' apps to understand them, and whatsmeow to make them. jawd's way needs neither. It needs jawd running, and WhatsApp reachable, at the time.
- **Everything jaw sends, and repeating** (Owner, 27 Oct): text, media, voice, polls, events, contacts, places, GIFs and forwards, once or repeating, all in the first step rather than text first.
- **An hour's grace** (Owner, 27 Oct): a message that couldn't go at its time goes once it can if that's within the hour, and is held for you past it.
- **A due reminder marks its chat unread** (Owner, 27 Oct), which WhatsApp syncs, so the phone shows the chat as unread while you're away from the Mac; jawd's notification is the Mac's.
- **M7, before the first release** (Owner, 27 Oct): the step after "Chat actions from jaw", whose app-state patches reminders use; the first release becomes M8.
- **Screens in both interfaces** (Owner, 27 Oct): the terminal's in jawd's step, as every step brings them; the Mac's on a branch beside it, as P9.

## Scheduled messages

### Requests

| Request | What jawd does |
| --- | --- |
| `send.text`, `send.media`, `send.poll`, `voice.send`, the protocol's Composing (`send.event`, `send.contact`, `send.location`, `send.gif`) and `forward`, each with `at`; `send.text`, `send.media`, `send.poll` and `voice.send` with `repeat` too | Keeps what would have been sent, to send at `at` (milliseconds since the epoch), and answers `scheduled`, the item's row, instead of `message`. `at` must be at least a minute ahead and at most a year; otherwise -32602 with `param` `at`. With `repeat`, `at` can be left out, and the rule's first time is used. A `forward` to several chats is an item for each, answered as a list. Each takes a `key` as `send.text` does: asking again with it within a day answers with the same item and keeps nothing new. |
| `scheduled.edit` (`scheduled`, and any of `text`, `captions`, `at`, `repeat`, `reply`, `mentions`, `question`, `options`) | Changes what's waiting: a message's text or the media's captions (one for each file), the time, the repeat (null to send it once), the quote and mentions, a poll's question and options. Files, a voice message and what's forwarded can't be changed; scheduling again can. A new `at` keeps the same limits, and a held item given one waits again. |
| `scheduled.send_now` (`scheduled`) | Sends it now, as at its time. A repeating item sends its next one now and carries on from the one after. |
| `scheduled.skip` (`scheduled`) | Skips a repeating item's next time; error 4 for one that goes once. |
| `scheduled.cancel` (`scheduled`) | Takes it away, every repeat with it, and its files once a synced commit no longer points to them. |
| `scheduled.watch` (optionally `chat`, with `around`, `before`, `after`) | A window of what's scheduled, every chat's or one's: held items first, then by their next time. |
| `when.parse` (`text`, optionally `repeat` true) | Reads a typed time in jawd's time zone and answers `at` and `says` ("Friday 30 October at 18:30"), and with `repeat`, the rule and its `says` ("every weekday at 09:00"); -32602 when it can't read it, with `message` saying so. It needs nothing from the disk, so it's answered at once, and a newer one from the same connection cancels the last, as `find` does. |

Each answers once its change is on disk, synced, and every interface's windows get the rows it changes. An item jawd doesn't have is error 3, which an interface asking again after a lost answer to `scheduled.cancel` takes as done. A change that reaches jawd after the item has gone is error 4 with `data` `{"message": id}`, the message it became, so the interface can offer to edit that within WhatsApp's 15 minutes, or delete it. A chat jawd doesn't have is error 3, as for a send.

### Rows

- **A scheduled row:** `id` (`scheduled:` and a number, never reused), `v`, `sort` (held first, then by the next time), `chat` and `chat_name`, `kind` (`text`, `media`, `voice`, `poll`, `event`, `contact`, `location`, `gif` or `forward`), and what it sends as a message row would show it: `spans` with its mentions marked, `reply`'s quote, `media` with its previews, `voice` with its length and waveform, a poll's question and options, an event's name and time, a contact's name, a place's name, a GIF's preview, the forwarded message's line. Then `at` (its next time), `repeat` (the rule with its `says`, and `left` when it ends after a number of times), `sent` (how many times a repeat has gone), and `state`: `waiting`, or `held` with `why`: `late` (since when), `missed` (how many times a repeat missed), `quote_gone`, `forward_gone` or `outbox_full`.
- **Chat rows** gain `scheduled` (how many are waiting or held) and `next_scheduled` (the soonest time), so the chat list can mark a chat and a conversation knows whether to watch its scheduled messages.

### Repeating

- **The rule:** `every` (`day`, `weekday`, `week`, `month` or `year`); for `week`, `days` (1 to 7, Monday first); for `month`, `day` (1 to 31, or -1 for the last); for `year`, `date` (month and day); `time` (`HH:MM`, local); `zone` (the IANA time zone it was made in, the Mac's); and at most one of `until` (a date) and `times`.
- **Local time:** each next time is reckoned in the rule's zone with the system's time zone database, so 09:00 stays 09:00 across a change of hour. A time the change skips goes at the first minute after it, and one the change repeats goes once. A month without the day goes on its last day, and 29 February on the 28th outside leap years.
- **Never more than daily, never to several chats:** a repeat is one chat's, and "every hour" isn't a rule jawd takes. Its files are kept until its last time.

### How jawd does it

- **`scheduled`, a table of jaw's own,** with the item's kept parts (the message as it will be sent, a poll's options, the forwarded message by its key) and, for media and voice, its files in `scheduled/`, by content hash as `media/` keeps them, each synced and renamed into place before its row points to it. Media is planned and transcoded when it's scheduled, in the background lane, keeping the file to send and its thumbnail, poster or waveform, so a file that can't go (no ffmpeg, too big) says so at once; the upload happens at its time. A voice message is the stopped recording, moved there, and a GIF found by search is fetched there when it's scheduled, since `gif.search`'s results last an hour.
- **One timer for every time jawd waits for,** scheduled messages and reminders alike, set on the wall clock for the soonest: on macOS a dispatch timer on the wall clock, on Linux a `CLOCK_REALTIME` timer that's woken when the clock is set. So a sleep or a change of the clock can't make it late, and nothing wakes jawd between times. The timer's query searches an index on the next time, its plan pinned.
- **At its time,** with WhatsApp reachable, one synced commit writes the message row and its outbox entry exactly as the send request would have (a new ID in whatsmeow's format; for a poll, a fresh secret, as any poll's), and in the same transaction moves the item to its next time or deletes it, with their feed events. From there it's an ordinary send: pending, sent, delivered and read, retried and failed as any message is. It reads nothing in the chat, since you weren't there, and leaves the chat's draft alone; the chat's disappearing timer is the one it has at that moment. A crash before the commit leaves the item due, so it goes when jawd starts again; after it, the outbox has it, under the one ID it ever goes with.
- **Late:** an item goes once its time has come and WhatsApp can be reached, the hour counted from its time. Within the hour it goes. Past it, one synced commit holds it (`state` `held`, `why` `late`), jawd notifies ("Not sent: your message to Ana at 09:00"), and it waits for `scheduled.send_now`, a new `at` or `scheduled.cancel`. A repeat that missed several times holds its latest, `missed` saying how many went by, and its rule goes on from now.
- **Keeping the Mac awake for it:** while an item is due in the next 30 minutes and the Mac is awake, jawd holds a power assertion that keeps it from idling to sleep (`IOPMAssertionCreateWithName` with `PreventUserIdleSystemSleep`; a logind inhibitor on Linux), let go once the item is handed to the outbox or held. The display can still sleep, and closing the lid still sleeps the Mac.
- **Held for a reason:** a message whose quote has been deleted for everyone by its time, or a forward whose message is gone, is held with that reason rather than sent without what it was written around. An item due while the outbox holds its thousand waits for room, the hour's grace counting as if WhatsApp couldn't be reached.
- **Changes go through the writer,** so a change and the send can't both happen: whichever commits first wins, and the other is told. A chat that merges into another (a person's LID and phone chats becoming one) takes its items with it in the merge's transaction. A group you've left by then, or a number no longer on WhatsApp, fails as a send to it does, and the failed message offers Retry.
- **Scheduling clears the chat's draft,** as sending does, since what was typed has gone where it was meant to go.
- **Bounded:** at most 1,000 items, a repeat counting once, and 2 GiB of kept files; past either, error 5 with `data` `{"limit": "scheduled", "most": 1000}` or `{"limit": "scheduled_bytes", "most": 2147483648}`. `jawd status` gives `scheduled`: how many are waiting and held, the next time, the bytes kept and how many were refused.

## Reminders

### Requests

| Request | What jawd does |
| --- | --- |
| `remind` (`message`, `at`, and optionally `chat`) | Sets the message's reminder for `at` or changes it (a snooze is a new `at`), or with `at` null takes it away, which is how it's marked done. `at` must be at least a minute ahead and at most a year; -32602 otherwise. A message jawd doesn't have, or one deleted for you, is error 3. |
| `reminders.watch` (`around`, `before`, `after`) | The reminders: those due first, the longest due at the top, then the rest by their time. |

`remind` answers once its change is on disk, synced, and asking again changes nothing more. A message has one reminder at most. A reminder set on a chat, from an interface's chat menu, is `remind` on its newest message, which the interface has in the chat's row.

### Rows

- **A reminder row:** `id` (`reminder:` and a number, never reused), `v`, `sort`, `at`, `due`, `message` (its ID), `chat` and `chat_name`, and the message as its chat shows it now: `from`, its line (`summary`) and its time. One deleted for everyone says so, and one that disappeared says it's gone.
- **Message rows** gain `reminder` (`at` and `due`) or null, so a conversation draws the flag; **chat rows** gain `reminders_due`, so the chat list, the status line and the sidebar's count can say a chat needs you.

### How jawd does it

- **`reminders`, a table of jaw's own,** naming its message by `messages.n`, so it keeps no copy of what the message says: a disappearing message that's gone is gone from its reminder too, as "Erased when asked" requires, and deleting a message for you deletes its reminder in the same transaction.
- **When one comes due,** one synced commit marks it due, with its feed events, and marks its chat unread (Owner, 27 Oct), writing the chat's `marked_unread` and the outbox's app-state patch as `chat.mark_unread` does in "Chat actions from jaw"; a chat already marked changes nothing. jawd then notifies, "Reminder · Ana Ribeiro" and the message's line, in a muted or archived chat too, since you asked, unless an interface in front is showing the message, which shows it due instead.
- **It stays due** until `remind` takes it away or gives it a new time. Reading the chat clears its unread mark, as reading always does, and leaves the reminder due; the canvas asks whether reading should clear it too.
- **While jawd wasn't running,** reminders that came due are marked due when it starts, however late, and notify as one summary when there are several ("3 reminders came due"); their chats are marked unread once the engine connects.
- **Bounded:** at most 1,000 reminders not yet done; past that, error 5 with `data` `{"limit": "reminders", "most": 1000}`. `jawd status` gives `reminders`: how many are due and waiting, and the next time.
- **WhatsApp's own reminders** aren't in a form jaw can read or write: the protobufs whatsmeow ships name `REMINDER_MESSAGE` and three reminder lines (`REMINDER_SETUP_MESSAGE` and the others) without what they'd carry. A later step reads them and syncs jaw's, once they say more.

## The terminal

As the canvas draws it. The glyphs are the terminal's: ◷ for a scheduled message, ⚑ for a reminder.

- **Sending later.** Ctrl-T with something in the box (text, attached files, a stopped recording) opens the when prompt in the box's place, as the palette's modes take it. A line to type a time shows jawd's reading of it as you type ("Friday 30 October at 18:30", or "jaw can't read that time"), and below it, picked with ↑ and ↓: in an hour, this evening at 18:00 (until five o'clock), tomorrow at 09:00, Monday at 09:00, and Repeat…, which takes a phrase such as "every weekday 9am" with its reading. Enter schedules: the box clears and the message appears under the conversation. Esc goes back to the box with everything still in it. The palette's Send later does the same.
- **In the conversation.** Under the newest message, a rule saying "scheduled", then the chat's scheduled messages in time order, each where yours go, faint, with ◷ and its time ("◷ tomorrow 09:00"), a repeat with its rule ("◷ every weekday 09:00"), and a held one in the warning colour ("◷ not sent at 09:00: jaw wasn't running"). Three show; more say "4 more scheduled", which opens the Scheduled list for the chat.
- **Acting on one.** Up from an empty box selects the newest scheduled message first, then steps into the messages. With one selected, e edits it: the box takes its text, its border saying "editing the message scheduled for tomorrow 09:00"; Enter saves, Esc leaves it as it was, and the chat's draft comes back either way. t gives it a new time; Enter sends it now, after asking on the status line; d cancels it, asking of a repeat whether to skip the next or cancel them all.
- **Reminders.** t on any other selected message opens the when prompt with a reminder's times (in an hour, in 3 hours, tomorrow at 09:00, next Monday at 09:00, or a typed time) and, with one set, "remove". The message then shows ⚑ and the time after its own ("⚑ tomorrow 09:00"), and once due, "⚑ due" in the mention colour.
- **The chat list:** ◷ on a chat with scheduled messages and ⚑ on one with a reminder due, beside the muted and pinned marks.
- **The lists.** The sidebar gains Scheduled and Reminders under the built-in folders, each with its count, and the palette has both. Each takes the conversation's place, as the library does, until Esc. Scheduled shows held items first, in the warning colour, then the rest under day headings (Today, Tomorrow, Friday 30 October), each with its time, its chat, its first line and its rule; Enter goes to it in its chat, and e, t, d and sending now work as in the conversation. Reminders shows those due first, then the rest by time, each with its chat, its sender and its line; Enter goes to the message, d marks it done and t gives it a new time.
- **The status line** says what's held and due on its right, before the other chats' unread ("⚑ 2 due", "◷ 1 held"), each a link to its list.

| Key | Does |
| --- | --- |
| Ctrl-T | in the box: send what's in it later |
| t | on a message: a reminder; on a scheduled message: a new time |
| e, d, Enter | on a scheduled message: edit, cancel, send now |
| ↑ from an empty box | the newest scheduled message, then the messages |

## The Mac

The Mac design's proposal P9, as the canvas draws it; until jawd's step lands, none of it shows.

- **Send Later** from the box: ⌥⌘↩, the menu of the box's Send button (a right click on it, as [the box's design](2026-10-27-jaw-mac-box-design.md) draws Send), the box's context menu, and the Message menu; a Send Later button beside Send in the poll, event, contact and location sheets; and ⌥⌘↩ on a GIF in the GIF picker. A popover above the box offers In 1 Hour, This Evening, Tomorrow Morning and Monday Morning, each with the time it means; a field to type a time, with jawd's reading under it; a date and time picker; Repeat (Never, Every Day, Every Weekday, Every Week, Every Month, Every Year, Custom…) and when it ends; and Schedule (Return). Esc closes it and leaves the box as it was.
- **In the conversation:** under the newest message, a "Scheduled" separator, then each item where yours go. In bubbles, it's a bubble with a dashed outline in your colour at half strength; in lines, your line drawn faint. Under it are ◷ and its time, a repeat's rule, and for a held one "Not sent at 09:00" with Send Now and Change Time. Hovering shows Edit and a clock; its menu has Edit, Change Time…, Send Now, Skip Next (for a repeat) and Cancel or Cancel Series. Edit puts its text in the box with "Editing scheduled message · Tomorrow at 09:00" above it, as replying shows the quote; Return saves, and Esc gives the draft back. With one selected from the keys, e, t, d and Enter do what they do in the terminal.
- **Reminders:** a message's menu gains Remind Me ▸ (In 1 Hour, In 3 Hours, Tomorrow Morning, Next Week, Custom…), or Change Reminder ▸ and Remove Reminder once one is set; t does it from the keys. A chat's menu in the list gains Remind Me ▸, on its newest message. The message shows a bell and the time beside its own time, in the accent when due.
- **The chat list:** a clock beside the muted and pinned marks on a chat with scheduled messages, saying the next time when hovered; a bell on one with a reminder due.
- **The sidebar's Scheduled and Reminders,** under the built-in folders, with their counts, each opening its list in a pane, as the library does. Scheduled is grouped by day with held items first, each row's menu as in the conversation. Reminders has Due, then Upcoming, each row with Done, Snooze ▸ and Go to Message.
- **Notifications,** with P2: a reminder's has Done and Snooze 1 Hour, and a click opens the message; a held message's has Send Now, and a click opens the Scheduled pane. Without P2, jawd's own notifications say the same, without the buttons.
- **The status line** says what's held and due, as the terminal's does.

## The data layer

The step changes `store/`, `send/`, the merge of chats and `notify`, and adds a scheduler beside the writer, so it follows "Data integrity" in jaw's design and is audited by the database expert before it lands.

- **One schema step** adds `scheduled` and `reminders`: `STRICT`, with `CHECK`s on states, times and kinds, `json_valid` on the rule and the kept parts, `AUTOINCREMENT` keys since rows are deleted and interfaces hold their IDs, and indexes for the timer's search and each window's keyset, each plan pinned. Both are declared jaw's own: never dropped or rebuilt, copied by every rebuild, and backed up daily with folders and drafts, `scheduled/`'s files copied beside the backup.
- **Every write is idempotent:** scheduling with a `key` answers with the same item, a change sets what it says, and the commit at an item's time happens once, since it moves or deletes the item in the transaction that writes the message.
- **Derived counts:** chat rows' `scheduled`, `next_scheduled` and `reminders_due` are kept by triggers or counted again in the transaction that changes their items, never adjusted by hand, and the background check compares them with their source.
- **Files:** a kept file is synced and renamed into place before a row points to it, and deleted only after a synced commit stops pointing to it; `scheduled/` is bounded by its 2 GiB and counted in `jawd status`.

## Tests

Each rule above has a test that fails without it, on a clock the tests set rather than the machine's:

- each kind scheduled and sent at its time as its send would have been, and each request's rows through the socket on the demo engine;
- a sleep across an item's time and the clock set back and forward, the item going within a second of its time once awake, and nothing waking jawd between times;
- the hour's grace on both sides of the hour; held items sent, rescheduled and cancelled; a repeat that missed several times holding one;
- a repeat across a change of hour, at the end of a month and on 29 February;
- a crash before and after the commit at an item's time, sending it once;
- an edit and a cancel racing the send, and a change after it answered with the message;
- a held quote and forward, a full outbox, a merge carrying items and reminders, a group left;
- reminders coming due, their chat marked unread with its patch, notified once, a summary after jawd was off, a disappearing message gone from its reminder, deleting a message for you taking its reminder with it;
- every limit refused and counted in `jawd status`, and the schema's plans pinned.

`jawd demo`'s made-up account has scheduled messages, a repeat, a held message and reminders, due and not, so the terminal's and the Mac's end-to-end tests and harnesses see each state, and `jaw snapshot --demo --keys` prints the when prompt, the conversation's scheduled messages and both lists.

## Build order

1. **jawd's step** (on `scheduled`, from main once "Chat actions from jaw" has landed; it comes after M2's events and places, M4's media, contacts and GIF search, and M5's voice, so every kind it schedules is one jawd already sends): the tables and their migration, the timer, sends with `at` and `repeat`, the `scheduled.*` requests and window, `when.parse`, reminders and their window, the protocol's text and fixtures, the demo account's items, the tests and the audit.
2. **The terminal** (the same branch): the when prompt, the conversation's scheduled messages and their keys, t for reminders, the marks, the two lists, the status line.
3. **The Mac** (on a branch of its own, from the Mac's): JawKit's rows and requests, the popover, the conversation's scheduled messages, the menus, the sidebar's lists and the marks; the notifications' buttons once P2 lands.

## What this changes elsewhere

- **jaw's design** gets "Scheduled messages and reminders" as M7, the first release as M8, MCP as M9, the second engine as M10 and the catch-up as M11, with the tables, the timer, the keys, the failure state and the risks.
- **The Mac design** gets P9 in its proposals and these screens in its window, its menus and its keys; **the box's design** gets Send Later in Send's menu; **the Mac plan** gets them as steps of Mac-M3, which the canvas asks about.
- **The protocol** gets the requests' text and fixtures when jawd's step lands, as chat actions' do.
- **The board** (`ws-jaw.canvas.tsx`) is the orchestrator's: it gains M7 and this work's rows when the owner tells it.

## Later

- WhatsApp's own scheduled messages: showing those others send you, which arrive hidden until their time, once whatsmeow can reveal them; until then they're kept raw, as every kind jaw can't show yet is.
- Syncing reminders with the phone, once WhatsApp's protobufs carry them.
- Waking a sleeping Mac for a scheduled message, which only an administrator's scheduled wake can do.
- A note on a reminder ("send her the PDF"), and reminders that aren't on a message.
- One scheduled message to several chats, with "One message to several chats".
