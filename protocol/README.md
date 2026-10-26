# jaw's interface protocol

Version 1, a draft. How an interface talks to jawd: jaw, a desktop app later, or a script. The types are defined once, in `crates/jaw-proto`, and every Rust interface uses them; `fixtures/` pins the wire format, and interfaces in other languages get a JSON Schema generated from jaw-proto.

The engine link, between jawd and jaw-engine, is a different protocol: `proto/engine.proto`.

## The connection

- A Unix socket, `jaw.sock` in jaw's folder (`~/Library/Application Support/jaw` on macOS, `$XDG_DATA_HOME/jaw` on Linux). The socket is mode 0600 in a folder only you can read, and jawd checks that whoever connects is the same user.
- One JSON object per line, UTF-8, in both directions. No line is longer than 16 MiB; most are a few hundred bytes.
- Messages are shaped like JSON-RPC 2.0 without its `jsonrpc` member:
  - a **request** has an `id` (an integer the client picks), a `method` and `params`;
  - a **reply** has the request's `id` and either a `result` or an `error`;
  - a **push**, from jawd, has a `method` and `params`, and no `id`.
- Requests can be sent without waiting for replies; replies come in any order. A client never blocks on one: whatever it asked for arrives as data. jawd answers a connection's watches, `hello` and `focus` without waiting for its other requests, which it carries out in the order they were sent, so a chat opens at once even while a message sent just before it waits for the disk. A watch is answered with what jaw.db holds when it's read, which may be before a request sent ahead of it has taken effect; that request's effect then comes as a push, and a row it makes can't be an anchor until it has. jawd holds at most 64 of a connection's other requests; past that, or while a reply waits for the interface to read, it reads nothing more from that connection, watches included.

```json
{"id":7,"method":"send.text","params":{"chat":"351912345678@s.whatsapp.net","text":"on my way"}}
{"id":7,"result":{"message":"3EB0C431F2A9B8E7D6"}}
{"method":"messages","params":{"watch":"w2","seq":81233,"upsert":[{"id":"3EB0C431F2A9B8E7D6","v":81233,"…":"…","status":"pending"}]}}
```

### Errors

| Code | Meaning |
| --- | --- |
| -32700 | A line that isn't JSON |
| -32600 | JSON that isn't a request |
| -32601 | A method jawd doesn't know |
| -32602 | Parameters it can't use; `data` says which |
| -32603 | Something went wrong inside jawd; `message` says what |
| 1 | Not linked: the account needs linking first |
| 2 | The engine isn't available right now; `data.retry_ms` says when to try again |
| 3 | Not found: the chat, message, folder or person doesn't exist |
| 4 | Not allowed, and why: editing a message older than WhatsApp allows, deleting someone else's message for everyone, voting twice on a single-choice poll |
| 5 | A limit, and which: presence subscriptions, a file too large to send, text longer than 65,536 characters (a message's, an edit's or a draft's), the outbox, a forward's chats or a connection's windows, the last three with `data.limit` saying so |
| 6 | Protocol versions don't match |
| 7 | Cancelled: a newer request replaced this one, as each `find` replaces the last |

`message` is a sentence an interface can show as it is.

## Hello

The first request on a connection. Anything else first gets error 6.

```json
{"id":1,"method":"hello","params":{"protocol":1,"client":"jaw","version":"0.1.0","focus":true,"since":81200}}
```

- `protocol`: the version the client speaks. jawd serves one version and refuses others with error 6, saying which is older; jaw then says to run `make install` again.
- `focus`: whether the interface is in front. jawd tells WhatsApp you're online only while some interface has focus (see `focus` below).
- `since`: the last `seq` this client saw, if it's reconnecting. Its watches, sent again, then return only what changed.

```json
{"id":1,"result":{"protocol":1,"jawd":"0.1.0","root":"/Users/me/Library/Application Support/jaw","seq":81233,"account":{"state":"linked","me":{"id":"person:1","name":"Pedro","phone":"+351 912 000 000"}},"connection":{"state":"online"}}}
```

- `root`: jaw's folder. Every `path` jawd sends is relative to it, and the files are readable by the same user.
- `account.state`: `unlinked`, `linking` or `linked`. From the moment jawd starts, an account that was linked when it last connected, and hasn't been logged out since, is `linked`, with `connection.state` `connecting` until the engine connects (`offline` when jawd has no engine to run). If the account was logged out meanwhile, a `connection` push says `logged_out` once the engine finds out, with a `reason`: WhatsApp's, or `this device isn't linked any more` when the engine finds no device. A folder never linked says `unlinked`, and so does one from a jawd before this, until its engine connects.
- `connection.state`: as the `connection` push below.

## Windows

An interface never asks for "all chats" or "all messages". It watches windows: a stretch of rows around an anchor, which jawd keeps current.

- A window is a list of rows sorted by `sort`, a string that compares bytewise and that no two rows in a list share. A chat list reads from the largest `sort` down; a chat's messages from the smallest up. A row's `v` (version) is the feed's `seq` when it last changed.
- An update upserts rows by `id` and removes rows by `id`. An upserted row replaces the one held when its `v` is at least as new: the same `v` can come again with a different `sort` while the chat list is held during a catch-up, or with a sender's new name, which changes what a row shows without changing the row. Upserts carry whole rows. Applying the same update twice ends in the same state, so there are no positions to keep in step.
- `before` and `after` say how many rows to keep on each side of the anchor, at most 1,000 each. A window around a row holds `before` rows, the row, and `after` rows; `top` is the place above the chat list's first row, so it takes `after`, and `newest` the place below a chat's newest message, so it takes `before`. jawd keeps that many as rows move, removing those that fall out and upserting those that come in.
- `total` is how many rows the whole list has, for a scroll bar.
- Each result and push carries the `seq` it's as of; a client keeps the newest it has seen for reconnecting.
- `watch.move` re-anchors a window, for scrolling; its result is like a push: the rows the client doesn't already hold as they are, and `remove` for those that left. `unwatch` ends a window. A connection watches at most 32 windows at once: watching another is error 5, with `data` `{"limit": "windows", "most": 32}`, until one is unwatched.

### chats.watch

```json
{"id":2,"method":"chats.watch","params":{"folder":"all","around":"top","before":0,"after":60}}
```

- `folder`: `all`, `unread`, `groups`, `communities`, `archived`, or a folder's `id`. jawd serves `all` and `archived` so far, and answers -32602 for the others.
- `around`: `top`, or a chat's `id`, to keep the selected chat where it is. The window follows the chat as it moves, and stays where the chat was if it leaves the list.
- Pinned chats sort first, then by their newest message's time. Older history arriving never changes a chat's sort; a new message does, and deleting one doesn't.

```json
{"id":2,"result":{"watch":"w1","seq":81233,"total":312,"rows":[{"id":"351912345678@s.whatsapp.net","v":42,"sort":"1~1792145060000~000000000017","kind":"direct","name":"Ana Ribeiro","avatar":"media/avatars/5f2c.jpg","last":{"from":"Ana","mine":false,"summary":"did you get the photos from Sunday?","kind":"text","status":null,"at":1792145060000},"unread":2,"mentions":0,"pinned":true,"muted_until":null,"archived":false,"marked_unread":false,"draft":null,"typing":[],"community":null}]}}
```

A chat row:

| Field | |
| --- | --- |
| `id` | Its JID |
| `kind` | `direct`, `group`, `community`, `announcement`, `broadcast` or `self` |
| `name`, `avatar` | Its name as you know it, and its picture's path, once fetched |
| `last` | Its newest message: who (`from`, absent on yours, and `mine`), a one-line `summary`, its `kind`, its `status` if it's yours, and when |
| `unread`, `mentions` | Unread messages, and how many mention you |
| `pinned`, `muted_until`, `archived`, `marked_unread` | As on the phone |
| `draft` | What's typed in its box, from any interface |
| `typing` | Who's typing or recording now, by person `id` |
| `community` | For a group in a community, the community's `id`; for a community, its groups' unread total in `children_unread` |

### messages.watch

```json
{"id":3,"method":"messages.watch","params":{"chat":"351912345678@s.whatsapp.net","around":"newest","before":80,"after":0}}
```

- `around`: `newest` (the window follows new messages), `unread` (the first unread one: counting back the chat's `unread` among messages others sent, found when the window is watched; `newest` when nothing's unread), or a message's `id`.
- While a window follows `newest`, new messages come into it and the oldest leave. A window around a message keeps `after` rows after it: new messages fill that room, and past it only raise `newer`, which an interface shows as "2 new".
- `older` says whether there's more before the window; moving past the oldest message jaw has asks the phone for more (`history.more`), and they arrive as upserts. `newer` is how many messages come after the window's last row, counted up to 1,000 so it costs the same at any depth: an interface that notes it when it moves the window shows the growth since as new.
- A chat jaw has no messages for yet is an empty window, which fills when they come.

```json
{"id":3,"result":{"watch":"w2","seq":81233,"total":5120,"older":true,"newer":0,"rows":[{"id":"3EB0A1B2C3D4E5F6","v":81230,"sort":"1792145060000~000000000123","chat":"351912345678@s.whatsapp.net","at":1792145060000,"from":{"id":"person:12","name":"Ana","colour":4},"mine":false,"kind":"text","spans":[{"t":"text","s":"did you get the "},{"t":"bold","s":"photos"},{"t":"text","s":" from Sunday?"}],"reply":null,"forwarded":0,"reactions":[{"emoji":"❤","n":2,"mine":true}],"status":null,"edited":false,"deleted":false,"expires_at":null,"view_once":false,"starred":false}]}}
```

A message row, render-ready:

| Field | |
| --- | --- |
| `id`, `chat`, `at` | Its WhatsApp ID, its chat and when it was sent (milliseconds since the epoch) |
| `from` | The sender's person `id`, the name to show, and a `colour` index for group senders; absent on your own |
| `mine` | Whether you sent it |
| `sort` | Its time, then its place among messages, both zero-padded |
| `kind` | `text`, `image`, `video`, `gif`, `video_note`, `voice`, `audio`, `document`, `sticker`, `album`, `location`, `live_location`, `contact`, `poll`, `event`, `system`, `call` or `unknown`; the fields below that its kind needs sit in the row beside the rest |
| `spans` | The text and caption, parsed: `text`, `bold`, `italic`, `strike`, `mono`, `quote`, `list`, `link` (with `url`) and `mention` (with `person`) |
| `reply` | The quoted message: its `id`, who wrote it, and a one-line `summary` |
| `forwarded` | How many times, 0 if it wasn't |
| `reactions` | Each emoji, how many, and whether one is yours |
| `status` | On yours: `pending`, `sent`, `delivered`, `read`, `played` or `failed`; in groups, as on the phone, delivered once every member it was sent to has it and read once every one has read it (members who joined after don't count, and those who left don't hold it up), with each recipient's times in `message.info` |
| `edited`, `deleted`, `expires_at`, `view_once`, `starred` | As they are |
| `media` | For media kinds: `w`, `h` (the original's size in pixels, which the interface keeps the shape of), `seconds`, `bytes`, `name`, `mime`, `hd` (an HD copy exists), `previews` (`{"256": path, "768": path}`, each JPEG, or PNG for a sticker with its transparency, at most that many pixels on its long side, either absent until it's made), `thumbnail` (the tiny copy that came in the message, as a file, for drawing before a preview is made), `path` (the file, once downloaded), `state` (`remote`, `downloading` with `progress`, `local`, `expired`, `waiting_for_phone` or `gone`). Paths are relative to `hello`'s `root` |
| `album` | For `album`: `items`, each an `id`, a `kind` (`image` or `video`) and its `media` as above, in the order sent; the row's own `spans` are the album's caption |
| `voice` | `seconds` and `waveform`, 64 values from 0 to 100, and `played`: on someone else's, whether you've played it, as the played receipt jawd sent says (the interface shows a dot until then, and plays the next one not yet played when one ends) |
| `link` | A link preview: `url`, `title`, `description`, `preview` |
| `poll` | `question`, `options` (each with `name`, `votes` and `mine`), `max` (how many choices), `voters` |
| `location`, `contact`, `event` | Their fields, ready to show |
| `system` | For `system` and `call`: one sentence, such as "Rui added Sofia" or "Missed voice call" |

### roster.watch

```json
{"id":4,"method":"roster.watch","params":{"folder":"all","around":"top","before":0,"after":50,"offline":true}}
```

People, online first, then by last seen, then by name. Watching the roster is also what tells jawd whose presence to follow; see Presence.

| Field | |
| --- | --- |
| `id` | `person:` and a number |
| `name`, `phone`, `about`, `avatar` | As known, the about text refreshed daily |
| `presence` | `state` (`online`, `typing`, `recording`, `seen` or `hidden`) and, for `seen`, `at` |
| `chat` | Their one-to-one chat's `id`, if there is one |

### folders.watch

The whole tree; it's small. Each folder: `id`, `parent`, `name`, `position`, `smart` (a smart folder's rule, or null), `unread` (each chat counted once), `count` and `favourite` (its Alt slot, or null). Built-in folders have fixed IDs: `all`, `unread`, `groups`, `communities`, `archived`.

### Pushes for windows

```json
{"method":"chats","params":{"watch":"w1","seq":81240,"total":313,"upsert":[{"id":"351912345678@s.whatsapp.net","v":81240,"sort":"1~1792145099000~000000000017","…":"…"}],"remove":["120363012345678901@g.us"]}}
```

- `chats`, `messages`, `roster` and `folders`, each naming its `watch`, with `upsert` and `remove` left out when empty.
- `total` comes when it changed; `messages` pushes also carry `older` and `newer` when they change.
- **Coalescing:** jawd keeps, per watch, only the newest version of each row. The first change after jawd hasn't read a window again for 50 ms goes at once, so a lone message reaches the interface within 5 ms of its commit; the changes that follow it go when the feed has been quiet for 5 ms, or every 50 ms while they keep coming, and jawd reads a window again at most once each 50 ms, so its batches are never less than 50 ms apart. A burst of 2,000 messages becomes at most 20 batches a second per window, each with only what the window shows.
- **While catching up** after time offline, the chat list reorders at most every 2 seconds; rows update in place in between, carrying the `sort` they had when the list last reordered, and at the next reorder the rows that moved come again with their new `sort` and the same `v`.
- **A slow client** never holds jawd up. jawd keeps what each window's rows were when it last sent them, so a client's pending updates are bounded by its windows however far behind it is, and when it reads again it gets one batch with what changed. One that leaves its updates unread for 5 seconds gets `resync` for each watch once it reads again, and watches again; jawd has forgotten those windows meanwhile.
- **A window jawd can't read** is tried again at its usual pace; once its reads have failed for 5 seconds, its interface gets `resync` for it, and watching again answers with the error.

## Finding

`find` is the palette's one search, sent at each keystroke. A newer `find` on the same connection cancels the one before, which is answered with error 7, so typing never queues searches and an interface keeps only the reply to its latest.

```json
{"id":12,"method":"find","params":{"query":"ar","limit":10}}
{"id":12,"result":{"results":[{"kind":"chat","score":62,"text":[{"t":"mark","s":"A"},{"t":"text","s":"na "},{"t":"mark","s":"R"},{"t":"text","s":"ibeiro"}],"field":"name","row":{"id":"351912345678@s.whatsapp.net","name":"Ana Ribeiro","…":"…"}}],"counts":{"chat":1,"person":2,"message":214}}}
```

- **Names first:** chats, people and folders, matched fuzzily against jawd's index in memory: initials ("ar" finds Ana Ribeiro), word starts and parts of words, ignoring case and accents on either side ("ines" finds Inês, and "Inês" finds Ines). Letters scattered inside words don't match. A group is matched by its name; a one-to-one chat by its person's name, the name they gave themselves, a business's verified name and their number, which needs three digits typed and matches a run of them. Someone with a one-to-one chat is found as that chat, so `person` results are people you have no chat with yet.
- **Then messages,** from the full-text index, newest first, accent-blind, with every word typed: a word of three letters or more matches the words it starts ("jan" finds jantar), and one of one or two letters matches only itself ("ja" finds já), since gathering every word that starts with one or two letters costs most of a keystroke's budget. `chat` narrows them to one chat; names are found anywhere. Links (M2), media and files (M3) will come after messages; until then those kinds find nothing.
- `limit` is the most results of each kind, 10 by default and 200 at most, so names never crowd out messages.
- `score` ranks results of a kind: word starts and runs above scattered letters, raised by up to half for chats you open often, so a chat you open daily goes ahead of one that matched as well, but not ahead of one that matched half as well again: initials still beat letters inside a word. Messages have 0 and come in time order.
- `text` is what matched, as spans: `text`, and `mark` where the query matched. For a message, it's a stretch around its first match, at most 160 characters, with `…` where text is left out, on one line.
- `field` says which name matched (`name`, `push_name`, `verified_name` or `phone`), `chat_name` is a message's chat, and `folders` are the folders a chat or person is in.
- `row` is what was found: a chat as `chats.watch` sends it, a message as `messages.watch` does, a person (`id`, `name`, `phone`), or a folder (`id`, `name`, `parent`; built-in folders have their fixed IDs, the rest `folder:` and a number).
- `counts` says how many of each kind matched, for the strip above the results: names exactly, messages up to 1,000, where 1,000 means at least that many.
- **Nothing typed** returns the chats you open most, then the newest; `score` is then a chat's frecency in hundredths of an open.
- **Frecency** is opens per chat, each counting half as much a week later, kept in jaw.db and shared by every interface. `chat.opened` is how an interface says you opened a chat yourself, from the palette, the chat list or a favourite; a window watched again after reconnecting isn't an open.

## Sending

```json
{"id":7,"method":"send.text","params":{"chat":"351912345678@s.whatsapp.net","text":"on my way","key":"jaw-5f1c9a7e"}}
{"id":7,"result":{"message":"3EB0C431F2A9B8E7D61A2B","row":{"id":"3EB0C431F2A9B8E7D61A2B","status":"pending","…":"…"}}}
{"id":9,"method":"chat.start","params":{"phone":"+351 913 333 333"}}
{"id":9,"result":{"chat":{"id":"351913333333@s.whatsapp.net","kind":"direct","…":"…"}}}
```

- **`send.text`** answers once the message is on disk, synced, with its outbox entry, so it's sent however jawd stops; once it's there, the answer is a success, and the message is on its way at once. `message` is its ID, made by jawd in WhatsApp's format (`3EB0` and 18 hex digits), which WhatsApp knows it by too; `row` is the message as `messages.watch` sends it, `pending`, and every window showing the chat gets it at the same moment, so their push may come before this answer (`row` is left out in the rare case it can't be read back; the message is sent all the same). It then moves on by itself: `sent` once the server has it, `delivered` and `read` as receipts come; its `at` stays when you sent it. The chat's draft is cleared, since it's what was sent.
  - `key` is the interface's own name for the message: a random string, 1 to 128 bytes, made once for each message it composes and sent again with it if the answer was lost. Within a day, `send.text` with a key it has seen answers with that message and its row as they are now, and writes and sends nothing new, however often or however close together it's asked. Without a key, each `send.text` is a new message, so an interface that can lose an answer should send one.
  - `chat` must be a chat jawd has (error 3 otherwise; `chat.start` makes one). Text that's only spaces is -32602, and more than 65,536 characters is error 5. `reply` and `mentions` are as "Message actions" says; `preview` changes nothing until link previews come. In a chat with disappearing messages on, the message carries its timer.
  - The outbox holds at most 1,000 messages and actions of yours, waiting, unconfirmed or failed. Past that, `send.text` is refused with error 5 and `data` `{"limit": "outbox", "waiting": 1000, "failed": n}`, and its text isn't kept, so the interface keeps it in its box; nothing already in the outbox is ever dropped, and new sends are taken again once some have gone or been retried.
  - Each chat's messages go in the order they were sent, up to 16 chats at once; a chat waiting to try again leaves its place to the others. As on the phone, a message waits, `pending`, for as long as WhatsApp can't be reached, however long that is, and goes once it can. While it can, a send that fails on the way (the network, the server busy) is tried again after 1 s, doubling to a minute, for about a day of trying. A refusal (a number that isn't on WhatsApp, a group you've left or can't write in, a message the server won't take) makes it `failed`, until `send.retry`.
  - A message is never sent with a second ID: every try, after a restart too, carries the one jawd made. Recipients' phones keep messages by chat, sender and ID, so one that arrives twice is one message to them, as WhatsApp's own resending relies on. What WhatsApp's server answers to an ID it already has is still to be checked on a real account. So when a message that may already have gone (one tried before, or left waiting when jawd stopped) is refused, it isn't shown `failed`: it stays `pending` and waits up to a day for a receipt, which settles it, and only then fails. Any receipt for a message settles it, whatever its send was doing.
  - Deleting a pending message for yourself doesn't cancel its send, as on the phone: it still goes, and stays deleted here. If WhatsApp refuses it, nothing is left to retry, as nothing is of one deleted for you once it had failed: `send.retry` is error 3 for either.
- **`chat.start`** takes `phone` (international form; `+`, spaces, dashes and brackets are fine) or `person` (`person:` and a number, as `find` gives them) and answers with `chat`, as `chats.watch` sends it, found or made. A number is checked with WhatsApp first (error 3 if it isn't on WhatsApp, 2 if the engine isn't connected), and WhatsApp's LID for it is kept with it, so messages addressed either way are one chat. A person's chat is found under any JID they're known by before one is made.
- **`draft.set`** keeps `text` as the chat's draft for every interface, shown in the chat list as `draft`; empty text clears it. Error 3 for a chat jawd doesn't have, 5 past 65,536 characters.
- **`read`** says you've read `chat` up to `up_to`, a message's ID: the chat's read position moves forward (back only while it's the phone's guess, before WhatsApp has said where it is), its `unread` is counted again from there, and jawd marks the messages read on WhatsApp, at once and then at most every 2 seconds per chat, however fast an interface reads. Sending a message reads the chat up to it as replying on the phone does, and marks what it reads read on WhatsApp the same way; a forward reads none of the chats it goes to. When read receipts are off on the phone, only your own devices hear. Error 3 for a chat or message jawd doesn't have.
- **`typing`** (`chat`, `state`: `composing`, `recording` or `paused`) answers at once. jawd tells WhatsApp when typing starts or changes, again every 10 seconds while it's said, and stops it 10 seconds after an interface last said so, or at once on `paused`.

## Reconnecting

A client that loses its connection reconnects, sends `hello` with `since` (the last `seq` it saw), and watches its windows again, each with `have`: the `id`s of the rows it holds for that window. Rows in `have` that haven't changed since `since` are left out of the results, and `remove` lists the ones in `have` that are no longer in the window, so a reconnect costs what changed, not the windows; rows that came into the window come whole, changed or not. A watch can give its own `since`, which wins over hello's; without `have`, a watch returns every row. If `since` is older than the feed keeps (7 days), or newer than its last `seq` (jaw.db was made again), the watches return everything, as a first watch does.

```json
{"id":3,"method":"messages.watch","params":{"chat":"351912345678@s.whatsapp.net","around":"newest","before":80,"after":0,"since":81200,"have":["3EB0A1B2C3D4E5F6","3EB0C431F2A9B8E7D6"]}}
```

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `hello` | `protocol`, `client`, `version`, `focus`, `since` | See Hello |
| `status` | | jawd's version, connection, account, engine, each pool's depth and times, what's missing (ffmpeg), `interfaces`: how many are `connected`, whether one is `focused`, where `notifications` go, the requests `in_lanes`, read and not yet answered (this one among them), the most a connection's lane holds (`lane_most`), the `lanes_full`, whose connections jawd reads nothing more from until one is answered, and the connections with a reply waiting for room (`replies_waiting`, one each at most), which it reads nothing from and sends nothing else meanwhile, and `windows`: how many are `watched` on every connection, how many of them are `distinct`, the `most` one connection can watch, the bytes of the reads kept for windows watched alike to share (`kept_bytes`, at most `kept_most`, the oldest `let_go` past it), and the window `reads` made, `taken` from a kept read and `joined` to one under way |
| `focus` | `focused` | |
| `link.start` | `method` (`qr` or `phone`), `phone` | Pushes `link` as it goes |
| `link.cancel` | | |
| `link.logout` | | Answered once the account is logged out and jawd has forgotten it; a `connection` push says `logged_out` (`logged out from jaw`), on the connection that asked just after the answer |
| `chats.watch`, `messages.watch`, `roster.watch`, `folders.watch` | See Windows, and `since` and `have` when watching again | `watch`, `seq`, `total`, `rows`, `remove` (with `have`); `older` and `newer` for messages |
| `watch.move` | `watch`, `around`, `before`, `after` | As its watch's, with only the rows the client lacks and `remove` |
| `unwatch` | `watch` | |
| `search` | `query`, `chat` (optional), `limit`, `cursor` | `hits` (message rows with the match marked) and the next `cursor` |
| `contacts.find` | `query` | People and chats to start or open, best first; numbers are checked against WhatsApp |
| `find` | `query`, `kinds` (any of `chat`, `person`, `folder`, `message`, `link`, `media`, `file`; all by default), `chat` (optional), `limit` (of each kind) | `results`, best first, each with its `kind`, a `score`, the match marked, and the row it found, and `counts` of each kind; a newer `find` from the same connection cancels this one. See Finding |
| `chat.opened` | `chat` | |
| `library.watch` | `kind` (`photo`, `video`, `voice`, `file`, `link` or `all`), and optionally `chat`, `person`, `since`, `until`, `text`, with `around`, `before`, `after` | `watch`, `total`, `rows`: each the message's media, file or link with its preview, sender, chat and time, and the message's `id` to jump to |
| `send.text` | `chat`, `text`, `reply`, `mentions`, `preview` (default true) | `message` and `row`. See Sending |
| `send.retry` | `message` | |
| `chat.start` | `phone` or `person` | `chat`. See Sending |
| `send.media` | `chat`, `files` (each a `path`, `kind` and `caption`), `quality` (`sd` or `hd`), `reply` | `messages` |
| `send.poll` | `chat`, `question`, `options`, `max` | `message` |
| `poll.vote` | `message`, `options` | |
| `react` | `message`, `emoji` (empty to remove), and optionally `chat`, `key` | |
| `edit` | `message`, `text`, and optionally `chat`, `key` | |
| `delete` | `message`, `for` (`me` or `everyone`), and optionally `chat`, `key` | |
| `forward` | `messages`, `to` (chats), and optionally `chat`, `key` | `messages` |
| `star` | `message`, `starred` | |
| `read` | `chat`, `up_to` (a message) | See Sending |
| `typing` | `chat`, `state` (`composing`, `recording` or `paused`) | See Sending |
| `draft.set` | `chat`, `text` | See Sending |
| `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread`, `chat.clear`, `chat.delete` | `chat`, and `pinned`, `archived`, `until` or `unread` | |
| `chat.info` | `chat` | Its kind, its members and their roles, its community and its disappearing timer; its media count comes with the library |
| `message.info` | `message`, and optionally `chat` | When it was sent, and each recipient's delivered, read and played times |
| `history.more` | `chat` | More arrives as upserts, once the phone answers |
| `media.fetch` | `message`, `variant` (`sd` or `hd`) | Progress and the path arrive as upserts to the message |
| `media.save` | `message`, `to` | `path` |
| `media.cancel` | `message` | |
| `voice.play` | `message`, `at`, `speed` | Pushes `playback` |
| `voice.pause`, `voice.seek`, `voice.speed` | `at` or `speed` | |
| `voice.record` | `chat` | Pushes `recording` |
| `voice.stop`, `voice.send`, `voice.cancel` | | `voice.send` returns `message` |
| `folder.create` | `name`, `parent` | `folder` |
| `folder.rename`, `folder.move`, `folder.delete` | `folder`, and `name` or `parent` and `position` | |
| `folder.add`, `folder.remove` | `folder`, `item` (a chat's `id` or a `person:` id) | |
| `folder.smart` | `folder`, `rule` | |
| `favourite.set` | `slot` (1 to 9), `target` (a folder or a chat), or null to clear | |

## Other pushes

| Push | Params |
| --- | --- |
| `link` | `state`: `qr` with `code` and `expires`, `code` with the phone `code` to type, `done`, or `failed` with `reason` |
| `connection` | `state`: `connecting`, `online`, `catching_up` (with `done` and `total`), `phone_offline`, `offline`, `logged_out`, `update_needed`, `banned` (with `until`) or `engine_restarting` |
| `typing` | `chat`, `who`, `state` |
| `presence` | `person`, `state`, `at` |
| `job` | `id`, `kind` (`upload`, `download`, `transcode`), `message`, `progress` |
| `playback` | `message`, `state` (`playing`, `paused`, `ended`), `at`, `seconds`, ten times a second while playing |
| `recording` | `state` (`recording`, `stopped`), `seconds`, `level` (0 to 100), ten times a second while recording |
| `notice` | `text` and `tone` (`info`, `warning`, `error`): something an interface should say once |
| `resync` | `watch`: watch it again |

## Voice (proposed)

Proposed with the `jaw-voice` crate in M5, for jawd to wire in; until then the `voice.*` rows above are the draft. It keeps their names and fills in what they leave open.

```json
{"id":31,"method":"voice.play","params":{"message":"3EB0A1B2C3D4E5F6","at":9.0,"speed":1.5}}
{"method":"playback","params":{"message":"3EB0A1B2C3D4E5F6","state":"playing","at":9.1,"seconds":31.0,"speed":1.5}}
{"method":"recording","params":{"state":"stopped","seconds":7.4,"waveform":[0,12,48,"…",30],"end":"stopped"}}
```

- **One thing at a time.** One message plays at once, on jawd's machine; a new `voice.play` stops the last, which gets `playback` with `stopped`. `voice.record` stops playback first.
- `voice.play`: `message`, `at` in seconds (0 by default) and `speed` (1, 1.5 or 2; 1 by default). Without `at`, a paused message carries on where it paused. A voice message that isn't downloaded yet is fetched first, at interactive priority. `recording: true` instead of `message` plays the stopped recording, to listen before sending.
- `voice.pause`, `voice.seek` (`at`) and `voice.speed` (`speed`) act on what's playing or paused, and answer with nothing; the `playback` push says what happened. Faster speeds keep the voice's pitch.
- **`playback`:** `message` (or `recording: true`), `state`, `at`, `seconds` (the length) and `speed`, ten times a second while playing and at each change. `state` is `playing`, `paused`, `ended` (`at` is then `seconds`), `stopped` (another message or a recording took over) or `failed`, with `error` saying why in words to show (no audio output, a file that isn't a voice message).
- `voice.record`: `chat`. jawd records, tells WhatsApp you're recording in that chat while it lasts, and answers error 4 if it's already recording. Recordings stop themselves at 15 minutes.
- **`recording`:** `state` and `seconds`, with
  - `recording`: `level`, 0 to 100, ten times a second;
  - `silent`: once, when the microphone has given nothing but digital silence for a second, as macOS gives when it hasn't allowed jawd the microphone; recording carries on;
  - `stopped`: `waveform` (64 numbers from 0 to 100, as WhatsApp sends) and `end`: `stopped`, `limit`, or `interrupted` with `reason` when the microphone went away (what came before is kept);
  - `cancelled`, or `failed` with `error`; nothing is kept after either.
- `voice.stop` stops recording and keeps it, to listen to or send. `voice.send` sends the stopped recording as a voice message, or stops and sends one still recording (Enter while recording), and returns `message`. `voice.cancel` throws it away, recording or stopped.
- The played receipt: jawd sends it for someone else's voice message when it first starts playing, once.

## Message actions (proposed)

Proposed with the terminal's M2 actions, for jawd to build, and served: `send.text`'s `reply` and `mentions`, `react`, `edit`, `delete`, `forward`, `message.info` and `chat.info`. It keeps their names and fills in what they leave open. Each action's result reaches every interface the same way: the rows it changes, upserted into the windows that show them, so an interface draws what jawd says rather than what it guessed.

`react`, `edit` and `delete` answer with nothing once they're on disk, synced, as `send.text` is, and `forward` with its messages then; each takes a `key` as `send.text` does: asking again with it within a day answers the same and does nothing more. Each, and `message.info`, names its message by `message`, its ID (`forward` its `messages`), and optionally `chat`, where it is: with `chat`, jawd finds it at once; without it, jawd looks for the ID in each chat, and an ID it finds in more than one chat is -32602 with `data.param` `chat`. A reaction, an edit or a deletion for everyone goes to WhatsApp as a message of its own, after the chat's earlier messages, waiting while WhatsApp can't be reached and tried again as a message is, and counts toward the outbox's thousand. Windows show it at once, as rows upserted with a new `v`, and the chat list too when it's the chat's newest message; jaw.db keeps it as done only once WhatsApp has it. If WhatsApp refuses it, the rows show as they were, and every interface is sent a `notice` saying so. One refused when it may already have gone, with no receipt for it within a day, shows as refused in the same way, but jawd keeps it for at least 30 days (a thousand at most, outside the outbox's thousand), and a receipt for it in that time does it then.

An action on a message of yours that hasn't reached WhatsApp yet (still pending, or refused) waits until that message has, so it never arrives before it, even while the message's chat is merging into another. If WhatsApp refuses the message, a reaction or an edit waits on, shown as now, and goes after the message once `send.retry` has sent it; a deletion for everyone goes, and once WhatsApp has it, nothing of the message is left to send. `send.retry` never sends a message deleted for everyone: it's error 4 while your deletion of it is on its way, and 3 once it's deleted, from jaw or from your other devices, or once your deletion is kept for a late receipt, since nothing of it is left to send then. A reaction or an edit whose message is deleted for everyone before it goes, by you, your other devices or whoever sent it, isn't sent: the row shows without it, and every interface is sent a `notice` saying so.

```json
{"id":40,"method":"send.text","params":{"chat":"120363000000000002@g.us","text":"@Inês Gomes vens buscar a avó?","reply":"3EB0A1B2C3D4E5F6","mentions":["person:4"]}}
{"id":41,"method":"react","params":{"message":"3EB0A1B2C3D4E5F6","emoji":"❤️"}}
{"id":42,"method":"edit","params":{"message":"3EB0C431F2A9B8E7D61A2B","text":"afinal chego às 21h"}}
{"id":43,"method":"delete","params":{"message":"3EB0C431F2A9B8E7D61A2B","for":"everyone"}}
{"id":44,"method":"forward","params":{"messages":["3EB0A1B2C3D4E5F6"],"to":["351913000031@s.whatsapp.net"]}}
{"id":44,"result":{"messages":["3EB0D2A1C3B4E5F60718A9"]}}
{"id":45,"method":"message.info","params":{"message":"3EB0C431F2A9B8E7D61A2B"}}
{"id":45,"result":{"message":"3EB0C431F2A9B8E7D61A2B","sent":1792145090123,"recipients":[{"person":"person:4","name":"Inês","delivered":1792145091000,"read":1792145400000,"played":null}]}}
{"id":46,"method":"chat.info","params":{"chat":"120363000000000002@g.us"}}
{"id":46,"result":{"chat":"120363000000000002@g.us","members":[{"person":"person:4","name":"Inês Gomes","phone":"+351 913 002 210","admin":false}]}}
```

- **`send.text`'s `reply`** is the ID of the message answered, in the same chat (error 3 otherwise, and 4 for one deleted). The new row's `reply` carries that message's `id`, who wrote it as `from` and its one-line `summary`, as every quote does: `from` is their name once jawd knows one, `You` for yours, and their JID until then. WhatsApp is sent the quote as its phones send one: the message's ID, who wrote it and what it says, as last edited.
- **`mentions`** are the person IDs mentioned, each written in `text` as `@` and the name the interface showed (as `chat.info` gives it). jawd sends WhatsApp each person's JID in the group, and the text with each `@name` written as WhatsApp writes it, `@` and the JID's number; the row's spans mark each as a `mention`, with `s` the `@name` again and `person` the JID WhatsApp named them by, as every mention has. A person not in the chat is -32602 with `data.param` `mentions`, its message naming them and the group as jaw shows them; mentions outside a group are dropped, as WhatsApp's phones drop them.
- **`react`** (`message`, `emoji`): one reaction of yours per message, as WhatsApp keeps them; another emoji replaces it, and an empty one removes it, so asking for the same twice is the same as once (an interface toggles one off by sending an empty one). Any emoji WhatsApp takes, with or without its variation selector, which jawd keeps as it came; more than 64 bytes or one with spaces is -32602. The row's `reactions` change, with `mine` on yours. Error 3 for a message jawd doesn't have, 4 for one deleted or a line about the chat.
- **`edit`** (`message`, `text`): your own text message, within 15 minutes of sending, as WhatsApp allows; past that, someone else's, a deleted one, one that failed to go or another kind is error 4, with the reason in `message` (captions come later). Text that's only spaces is -32602. The row's `spans` become the new text's and `edited` true; replies quoting it keep their summary, as the phones do.
- **`delete`** (`message`, `for`):
  - `me` removes it from jaw only, at once: it goes from every window as a `remove`, and from search, a copy arriving later doesn't bring it back, and jawd tells your other devices, by WhatsApp's app state. Any message. Telling them takes a place in the outbox's thousand, so past it a deletion for you is error 5, unless it's of a message of yours that failed to go, whose place it takes, so a full outbox can always be emptied.
  - `everyone` deletes your own for everyone within 60 hours of sending, as WhatsApp allows (error 4 after, for someone else's or one that failed to go; a group's admins deleting others' messages comes later; one deleted already answers as done). The row stays with `deleted` true and no spans, and shows as "You deleted this message" in yours and "This message was deleted" in theirs; from M3, when jawd keeps media, its media is dropped from the cache.
- **`forward`** (`messages`, `to`): each message to each chat, in order, as new messages of yours with `forwarded` counting the times the original had been, plus one; WhatsApp marks five or more "forwarded many times". Answers with the new messages' IDs, chat by chat, once all are on disk as `send.text`'s is; each then goes, with its own outbox row and ticks, as a message of yours does, and the chats' drafts stay. A forward is chosen from a list of chats rather than sent in one, so it reads none of them: their `unread` and read positions stay, and WhatsApp is told nothing was read. A message goes as it says now, as last edited; media goes as it came, without being sent again, so it reaches whoever WhatsApp still has it for; a quote or mentions it carried don't go with it. At most five chats at once, as the phones allow (error 5, `data` `{"limit": "forward", "most": 5}`), past the outbox's thousand error 5 as `send.text`; a chat jawd doesn't have or a message it can't find is error 3; a deleted message, a view-once one, a line about the chat, a kind WhatsApp doesn't forward (a poll, an event, a live location, an album) or one jawd hasn't what it said for is error 4.
- **`message.info`** (`message`): when your message was `sent` (null while it's still on its way), and for each recipient their `person`, `name` and the times it was `delivered`, `read` and, for voice, `played`, each null until it is, by name. The recipients are everyone a receipt for it came from, whichever of their JIDs it came by, and, while it's still on its way to some, those jawd sent it to (in a group, its members when it was sent, but you; in a direct chat, the other person). For someone else's message, `recipients` is empty and `sent` is when it was sent.
- **`chat.info`** (`chat`) answers with the chat's `kind`, its `community` if it's in one, its disappearing timer as `ephemeral` (seconds, 0 when off), and `members`: each member's `person`, the `name` to show, `phone` (`+` and the number) where jawd knows it, and `admin`, for completing `@` names, each person once, by name. A direct chat's members are the other person and you. Error 3 for a chat jawd doesn't have.

## Library (proposed)

Proposed with the terminal's library in M3, for jawd to build; until then the `library.watch` row above is the draft. It keeps its name and fills in what it leaves open.

```json
{"id":60,"method":"library.watch","params":{"kind":"photo","chat":"120363000000000002@g.us","person":"person:4","since":1785542400000,"until":null,"text":"praia","around":"top","before":0,"after":100}}
{"id":60,"result":{"watch":"w7","seq":81300,"total":6,"older":false,"counts":{"photo":6,"video":1,"voice":0,"file":0,"link":2},"rows":[{"id":"3EB0A1B2C3D4E5F6","sort":"1785763200000~3EB0A1B2C3D4E5F6","kind":"photo","message":"3EB0A1B2C3D4E5F6","chat":"120363000000000002@g.us","chat_name":"Família Gomes","from":{"id":"person:4","name":"Inês","colour":2},"mine":false,"at":1785763200000,"media":{"w":4032,"h":3024,"previews":{"256":"media/p/3eb0a1.256.jpg"}},"caption":[{"t":"text","s":"na "},{"t":"mark","s":"praia"}]}]}}
{"method":"library","params":{"watch":"w7","seq":81301,"total":7,"counts":{"photo":7,"…":"…"},"upsert":[{"id":"…","…":"…"}]}}
```

- **What's asked.** `kind` is `photo`, `video` (GIFs and video notes too), `voice`, `file`, `link` or `all`. `chat` and `person` narrow to one chat and one sender (`person:` and a number, or `me` for yours); `since` and `until` are milliseconds since the epoch, either end open; `text` matches captions, file names and link titles, hosts and descriptions, as `find` matches messages: accent-blind, each word typed starting a word. Every filter can be absent.
- **A window,** newest first, paged by keyset as the chat list is: `around` is `top` or an item's `id`, with `before` (newer) and `after` (older) rows, and `watch.move` re-anchors it. `older` says whether more come after the window's last row; `total` is how many items of the kind match. `kind` `all` is different: it isn't paged, and holds the newest `after` items of each kind, so an interface can show a shelf of each, and narrows to one kind to page.
- **`counts`** are how many items of each kind match the other filters, for the kind strip, and come again in pushes when they change.
- **Rows** are items, sorted by `sort` descending (the time, then the `id`). An item is a message's photo, video, voice message or file, one of an album's photos, or one of a message's links, so a message can give several. Each has its own `id` (the message's ID, with `:` and the place for an album's photos and `~` and the place for links), its `kind`, its `message` to go to, its `chat` and `chat_name`, `from` (absent on yours, with `mine` true), `at`, `caption` (the message's text, the words that matched marked as `find` marks them, empty when there's none), and what its kind needs:
  - `photo` and `video`: `media` as message rows have it, with `seconds` for a video;
  - `voice`: `voice` as message rows have it;
  - `file`: `file`: its `name`, `type` (as shown: `PDF`, `GPX`), `bytes`, `pages` where known, and `path` once downloaded;
  - `link`: `link`: its `url`, `host`, and the preview's `title` and `description` where it had one.
- **Pushes.** `library` pushes upsert items as they come and change and remove those deleted, with `total`, `older` and `counts` when they change, as other windows' pushes do.
- **Errors.** A `chat` or `person` jawd doesn't have is error 3; `since` after `until` is -32602.

## Folders and the roster (proposed)

Proposed with the terminal's sidebar in M6, for jawd to build; until then `folders.watch`, `roster.watch`, `chats.watch`'s `folder` and the `folder.*` rows above are the draft. It keeps their names and fills in what they leave open. Folders are jaw's own, kept in jaw.db and shared by every interface; WhatsApp never sees them.

```json
{"id":50,"method":"folders.watch","params":{}}
{"id":50,"result":{"watch":"w5","seq":81240,"rows":[{"id":"folder:1","parent":null,"name":"Family","position":0,"smart":null,"unread":3,"mentions":1,"count":6,"favourite":1}]}}
{"method":"folders","params":{"watch":"w5","seq":81241,"upsert":[{"id":"folder:1","unread":4,"…":"…"}]}}
{"id":51,"method":"roster.watch","params":{"folder":"folder:1","around":"top","before":0,"after":200,"offline":true}}
{"id":51,"result":{"watch":"w6","seq":81241,"total":6,"rows":[{"id":"person:1","sort":"0~ana ribeiro","name":"Ana Ribeiro","phone":"+351 912 345 678","about":"","presence":{"state":"typing"},"chat":"351912345678@s.whatsapp.net","folders":["folder:1"]}]}}
{"id":52,"method":"folder.create","params":{"name":"Cousins","parent":"folder:1"}}
{"id":52,"result":{"folder":{"id":"folder:7","parent":"folder:1","name":"Cousins","position":2,"…":"…"}}}
{"id":53,"method":"folder.add","params":{"folder":"folder:7","item":"person:4"}}
```

- **The tree.** `folders.watch` answers with every folder, built-in and smart ones too, and pushes `folders` with `upsert` and `remove` as they change; it's one window, without `around`. A folder row: `id`, `parent` (null at the top), `name`, `position` among its siblings, `smart` (a smart folder's rule, or null), `unread` and `mentions` (its chats' and its subfolders' chats', each chat counted once), `count` (the chats and people in it and under it, each once) and `favourite` (its Alt slot, or null). The built-in folders, `all`, `unread`, `groups`, `communities` and `archived`, have `parent` null and come first, by `position`; smart folders have IDs `smart:` and a number, yours `folder:` and a number.
- **Smart folders** are rules: an object whose fields must all hold, any of `unread` (true), `mentions` (true: it mentions you, unread), `kind` (`direct`, `group` or `community`), `community` (a community's `id`), `muted` (true), `waiting` (seconds: their message is the last and unanswered for that long) and `favourites` (true: in a favourite slot, or in a folder that is). jaw starts with three: Waiting on me (`{"waiting":86400}`), Mentions me (`{"mentions":true}`) and Muted, unread (`{"muted":true,"unread":true}`). `folder.smart` (`folder`, `rule`) makes one, or changes its rule.
- **A folder's chats.** `chats.watch` takes any folder's `id` as `folder`: the window holds the chats in it and in its subfolders, each once, with a person standing for their one-to-one chat, sorted as All is. Chat rows gain `folders`, the IDs of your folders the chat sits in directly, and `community`, the community a group belongs to. A community is a row of `kind` `community` carrying its groups' unread total; its groups are rows in the same window with `community` naming it, the announcement group (`kind` `announcement`) first, so an interface can show a community as one row that opens to its groups.
- **The roster.** `roster.watch` takes `folder` as `chats.watch` does, `offline` (false leaves out people who aren't online, typing or recording), and a window's `around`, `before` and `after`. Its rows are people: `id`, `name`, `phone`, `about`, `avatar`, `presence`, `chat` (their one-to-one chat's `id`, if there is one) and `folders`, as chats have them. `sort` puts them online first (`0~` and the name), then by when last seen, the most recent first (`1~` and the time counted down), then those who hide it or whom WhatsApp hasn't told jawd about (`2~` and the name). `presence` is `state` (`online`, `typing`, `recording`, `seen` or `hidden`) with `at` for `seen`, and `known` false when jawd isn't following that person now (scrolled past, or over the limits in Presence), so an interface draws the last known state faintly.
- **Your folders.** `folder.create` (`name`, `parent`, null for the top) answers with `folder`, the new row, placed last among its siblings; `folder.rename` (`folder`, `name`); `folder.move` (`folder`, `parent`, `position`) nests a folder in another, or puts it back at the top with a null `parent`, at `position` among its new siblings (last without it); `folder.delete` (`folder`) moves its subfolders up to its parent, and its chats and people stay wherever else they are. `folder.add` and `folder.remove` (`folder`, `item`: a chat's `id` or a `person:` id) file and unfile one item; an item can sit in any number of folders. Each pushes the folder rows it changes, and the chat and roster rows whose `folders` change, to every interface.
- **Errors.** A name that's empty or only spaces is -32602; one a sibling already has is error 4. Moving a folder into itself or one of its own subfolders, renaming, moving, deleting or filing into a built-in folder, or filing into a smart one, is error 4. A folder or item jawd doesn't have is error 3.

## Focus and reading

```json
{"id":14,"method":"focus","params":{"focused":false}}
{"id":14,"result":{}}
```

- `focus` says whether the interface is in front now: a terminal sends it as focus comes in and goes out, and `hello`'s `focus` says how a connection starts. It's per connection, and you count as looking while any connection's focus is true.
- jawd keeps which chats each connection is reading: those its `messages.watch` windows show, until it unwatches them, and the chat it last said it opened with `chat.opened`, until it opens another or no window of its shows that one.
- A chat a connection in front is reading gets no notifications, and presence follows focus (see Presence).

## Notifications

jawd sends them itself, so they come with no interface open: Notification Center on macOS, `notify-send` on Linux, and none where neither is. `jawd --no-notify` or `JAW_NOTIFY=off` turns them off, and `status` says where they go.

- A new message from someone else, arriving since jawd started, notifies: the chat's name, how many are unread when it's more than one, and the message on one line (who wrote it, in a group), or its kind's word for media.
- A burst in one chat is one notification; what comes in that chat in the next 10 seconds goes as one more when they've passed. More than three chats at once are one summary.
- Nothing goes while catching up. When a catch-up ends, the chats whose newest message it brought are one notification, that chat's own or a summary such as "12 new messages in 4 chats", however long ago they were sent; so what came while jawd was off is one summary once it has caught up after starting.
- Muted and archived chats, your own messages, history, and the chat an interface in front is reading stay quiet.

## Presence

jawd tells WhatsApp you're online once an interface's `focus` has been true for half a second, and away 30 seconds after the last one loses it (or its connection ends), so your phone keeps its notifications while you're not looking and switching windows doesn't flicker. It says so again each time the connection to WhatsApp comes back. WhatsApp only sends others' presence while you're online.

It follows the presence of the people in roster windows and open chats, while some interface has focus, at most 20 new people a minute and 200 per connection to WhatsApp, favourites first; subscribing to everyone is what presence trackers do. Past the limit, rows show their last known state.

## Versioning

- Adding an optional field, a method, a push, a message `kind` or an error code doesn't change the version; interfaces ignore what they don't know, and show an unknown `kind` as "jaw can't show this yet".
- Anything else, such as a field changing meaning or going, raises `protocol` by one.

## Fixtures

`fixtures/` holds one JSON file per message on the wire, named for what it is: `hello.request.json`, `hello.result.json`, `chats.watch.result.json`, `messages.push.json`, `message.row.voice.json`, and so on. jaw-proto's tests read each into its type and write it back, and the two must be equal, so a change to the format is always deliberate and shows in review.
