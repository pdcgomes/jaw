# jaw's interface protocol

Version 2, a draft. How an interface talks to jawd: jaw, a desktop app later, or a script. The types are defined once, in `crates/jaw-proto`, and every Rust interface uses them; `fixtures/` pins the wire format, and interfaces in other languages get a JSON Schema generated from jaw-proto.

The engine link, between jawd and jaw-engine, is a different protocol: `proto/engine.proto`.

## The connection

- A Unix socket, `jaw.sock` in jaw's folder (`~/Library/Application Support/jaw` on macOS, `$XDG_DATA_HOME/jaw` on Linux). The socket is mode 0600 in a folder only you can read, and jawd checks that whoever connects is the same user.
- One JSON object per line, UTF-8, in both directions. No line is longer than 16 MiB; most are a few hundred bytes.
- Messages are shaped like JSON-RPC 2.0 without its `jsonrpc` member:
  - a **request** has an `id` (an integer the client picks), a `method` and `params`;
  - a **reply** has the request's `id` and either a `result` or an `error`;
  - a **push**, from jawd, has a `method` and `params`, and no `id`.
- Requests can be sent without waiting for replies; replies come in any order. A client never blocks on one: whatever it asked for arrives as data.
  - `hello`, `focus`, `themes`, `chats.watch`, `messages.watch`, `folders.watch`, `roster.watch`, `watch.move` and `unwatch` are answered at once, without waiting for the connection's other requests, so a chat opens at once even while a message sent just before it waits for the disk.
  - `find` and `media.save`, which can take long, are each answered on a task of their own once done, in no order with the rest. A connection can have 64 of them being answered; past that, another is error 5, with `data` `{"limit": "apart", "most": 64}`.
  - Every other request waits for the ones sent before it: jawd carries them out one at a time, in the order they were sent. It holds 64 of a connection's at most; past that, or while a reply, a `layout` push or a `settings` push waits for the interface to read it, it reads nothing more from that connection, the requests answered at once included, until one of them is answered.
  - A watch is answered with what jaw.db holds when it's read, which may be before a request sent ahead of it has taken effect: a send, a read position, a deletion. That request's effect then comes as a push, and a row it makes can't be an anchor until it has; `around: unread` just after a `read` anchors on the first message unread before it.
  - A window's push can come before the reply to the request that caused it, so a `row` in a reply is an upsert like any other, applied by its `v`.
  - An interface that closes its side for writing, as a script does once its input ends, still gets the reply to every request it sent; jawd then closes the connection.

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
| 3 | Not found: the chat, message, folder or person doesn't exist, or the settings of a client jawd keeps none for |
| 4 | Not allowed, and why: editing a message older than WhatsApp allows, deleting someone else's message for everyone, a vote choosing more options than its poll takes (a second choice in a single-choice poll; a new vote replaces your last, as on the phones), pinning, archiving, muting or marking a community's own row, starring a line about the chat or a deleted message, an event's `call` link, which the phones ask WhatsApp's server for and jaw's engine can't yet |
| 5 | A limit, and which: presence subscriptions, a file too large to send, text longer than 65,536 characters (a message's, an edit's or a draft's), an event's name past 255 characters, its description past 2,048 or its place's name or address past 1,024, and a location's name or address past 1,024, each with `data` saying which and its most, a layout past its size, panes, layouts or slots, a layout for one interface too many, a contact message's cards, a card's name or their vCards, the outbox, a forward's chats, a fourth pinned chat, a connection's windows or its finds and saves being answered, the last five with `data.limit` saying so |
| 6 | Protocol versions don't match |
| 7 | Cancelled: a newer request replaced this one, as each `find` replaces the last |

`message` is a sentence an interface can show as it is.

## Hello

The first request on a connection. Anything else first gets error 6.

```json
{"id":1,"method":"hello","params":{"protocol":2,"client":"jaw","version":"0.1.0","focus":true,"since":81200}}
```

- `protocol`: the version the client speaks. jawd serves one version and refuses others with error 6, saying which is older; jaw then says to run `make install` again.
- `client`: the interface's name, `jaw` for the terminal. jawd keeps each interface's layout (see "Layouts") and settings (see "Settings") under the name it gives, so its connections share them and another interface keeps its own.
- `focus`: whether the interface is in front. jawd tells WhatsApp you're online only while some interface has focus (see `focus` below).
- `since`: the last `seq` this client saw, if it's reconnecting. Its watches, sent again, then return only what changed, but for chat lists and the folder tree, which come whole (see Reconnecting).

```json
{"id":1,"result":{"protocol":2,"jawd":"0.1.0","root":"/Users/me/Library/Application Support/jaw","seq":81233,"account":{"state":"linked","me":{"id":"person:1","name":"Pedro","phone":"+351 912 000 000"}},"connection":{"state":"online"},"settings":{"theme":"tokyo-night","messages":"bubbles"}}}
```

- `root`: jaw's folder. Every `path` jawd sends is relative to it, and the files are readable by the same user.
- `account.state`: `unlinked`, `linking` or `linked`. From the moment jawd starts, an account that was linked when it last connected, and hasn't been logged out since, is `linked`, with `connection.state` `connecting` until the engine connects (`offline` when jawd has no engine to run). If the account was logged out meanwhile, a `connection` push says `logged_out` once the engine finds out, with a `reason`: WhatsApp's, or `this device isn't linked any more` when the engine finds no device. A folder never linked says `unlinked`, and so does one from a jawd before this, until its engine connects.
- `connection.state`: as the `connection` push below.
- `settings`: the settings of the connection's own domain, the one its `client` names, with the values in effect, as "Settings" says; absent for a client jawd keeps no settings for.

## Windows

An interface never asks for "all chats" or "all messages". It watches windows: a stretch of rows around an anchor, which jawd keeps current.

- A window is a list of rows sorted by `sort`, a string that compares bytewise and that no two rows in a list share. A chat list reads from the largest `sort` down; a chat's messages from the smallest up. A row's `v` (version) is the feed's `seq` when it last changed.
- An update upserts rows by `id` and removes rows by `id`. An upserted row replaces the one held when its `v` is at least as new: the same `v` can come again with a different `sort` while the chat list is held during a catch-up, or, in a chat list, with a new name for whoever the chat is with, its members or its last message's sender, or, for a community, with what it sums up of the groups of it the list comes to hold, which changes what a row shows without changing the row. A message row's `v` moves when anyone it names gets a new name: whoever sent it, whoever wrote what it quotes, and whoever it mentions. Upserts carry whole rows. Applying the same update twice ends in the same state, so there are no positions to keep in step.
- `before` and `after` say how many rows to keep on each side of the anchor, at most 1,000 each. A window around a row holds `before` rows, the row, and `after` rows; `top` is the place above the chat list's first row, so it takes `after`, and `newest` the place below a chat's newest message, so it takes `before`. jawd keeps that many as rows move, removing those that fall out and upserting those that come in.
- `total` is how many rows the whole list has, for a scroll bar; a chat list's counts a community it holds only for its groups too (see Communities in a chat list).
- Each result and push carries the `seq` it's as of; a client keeps the newest it has seen for reconnecting.
- `watch.move` re-anchors a window, for scrolling; its result is like a push: the rows the client doesn't already hold as they are, and `remove` for those that left. `unwatch` ends a window. A connection watches at most 32 windows at once: watching another is error 5, with `data` `{"limit": "windows", "most": 32}`, until one is unwatched.

### chats.watch

```json
{"id":2,"method":"chats.watch","params":{"folder":"all","around":"top","before":0,"after":60}}
```

- `folder`: `all`, `unread`, `groups`, `communities`, `archived`, or a folder's `id`, smart (`smart:` and a number) or yours (`folder:` and a number); see Folders and the roster. A folder jawd doesn't have is error 3.
- `around`: `top`, or a chat's `id`, to keep the selected chat where it is. The window follows the chat as it moves, and stays where the chat was if it leaves the list. A window around a group in a community is around the community's row.
- `open`: the communities, by `id`, whose groups the window brings with them, as an interface has them open; each other community comes as its one row. Without it, every community brings its groups. `watch.move` takes it too, and keeps the window's when it's left out.
- Pinned chats sort first, the one pinned last on top, and the others by their newest message's time. Older history arriving never changes a chat's sort; a new message does, unless the chat is pinned, and deleting one doesn't. A community is one row, its groups shown under it, sorted by the newest of what it brings (see Communities in a chat list).

```json
{"id":2,"result":{"watch":"w1","seq":81233,"total":312,"rows":[{"id":"351912345678@s.whatsapp.net","v":42,"sort":"1~1792145060000~000000000017","kind":"direct","name":"Ana Ribeiro","avatar":"media/avatars/5f2c.jpg","last":{"from":"Ana","mine":false,"summary":"did you get the photos from Sunday?","kind":"text","status":null,"at":1792145060000},"unread":2,"mentions":0,"pinned":true,"muted_until":null,"archived":false,"marked_unread":false,"draft":null,"community":null}],"typing":{"351912345678@s.whatsapp.net":[{"id":"person:12","name":"Ana","state":"typing"}]}}}
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
| `community` | For a group in a community, the community's `id`; for a community, the unread messages of the groups the window holds under it in `children_unread`, and how many of them mention you in `children_mentions` (see Communities in a chat list) |
| `folders` | The IDs of your folders it sits in directly, or its person does, for a one-to-one chat; absent when none |
| `favourite` | Its Alt slot, 1 to 9, when the chat itself is a favourite |

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
| `kind` | `text`, `image`, `video`, `gif`, `video_note`, `voice`, `audio`, `document`, `sticker`, `album`, `location`, `live_location`, `contact`, `poll`, `event`, `invite`, `system`, `call` or `unknown`; the fields below that its kind needs sit in the row beside the rest. A business's templates, buttons and lists, answers to them, Meta AI's answers and comments on a community's announcements are `text`, a comment's `reply` naming the announcement |
| `spans` | The text and caption, parsed: `text`, `bold`, `italic`, `strike`, `mono`, `quote`, `list`, `link` (with `url`) and `mention` (with `person`) |
| `reply` | The quoted message: its `id`, who wrote it, and a one-line `summary` |
| `forwarded` | How many times, 0 if it wasn't |
| `reactions` | Each emoji, how many, and whether one is yours; none on a message deleted for everyone, as on the phones |
| `status` | On yours: `pending`, `sent`, `delivered`, `read`, `played` or `failed`; in groups, as on the phone, delivered once every member it was sent to has it and read once every one has read it (members who joined after don't count, and those who left don't hold it up), with each recipient's times in `message.info` |
| `edited`, `deleted`, `expires_at`, `view_once`, `starred` | As they are |
| `media` | For media kinds: `w`, `h` (the original's size in pixels, which the interface keeps the shape of), `seconds`, `bytes`, `name`, `mime`, `hd` (an HD copy exists), `previews` (`{"256": path, "768": path}`, each JPEG, or PNG for a sticker with its transparency, at most that many pixels on its long side, absent until both are made), `thumbnail` (the tiny copy that came in the message, drawn at 256 pixels on its long side, as a file, for drawing before a preview is made), `path` (the file, once downloaded), `hd_path` (the HD copy's, once downloaded), `state` (`remote`, `downloading` with `progress` from 0 to 1, `local`, `expired`, `waiting_for_phone` or `gone`; see Media in) and `error` (why the file isn't here, in words to show). Paths are relative to `hello`'s `root` |
| `album` | For `album`: `items`, each an `id`, a `kind` (`image` or `video`) and its `media` as above, in the order sent; the row's own `spans` are the album's caption |
| `voice` | `seconds` and `waveform`, 64 values from 0 to 100, and `played`: on someone else's, whether you've played it, as the played receipt jawd sent says (the interface shows a dot until then, and plays the next one not yet played when one ends) |
| `link` | A link preview: `url` (the URL it's for, with `https://` before one written without how it's reached), `title`, `description`, and `preview`, the small thumbnail that came with it, as a JPEG file under `media/links/`, once jawd has kept it. A preview someone sends comes from what their message carries; jawd never fetches a page for it. History's previews show their title and description, and their thumbnail only if jawd already keeps that one |
| `poll` | `question`, `options` (each with `name`, `votes`, how many chose it, and `mine`, whether you did), `max` (how many options one vote may choose) and `voters` (how many people chose any). Each voter's latest vote counts, by when they voted; jawd counts them as it reads the poll, so a vote shows in every window showing the poll, as a row upserted with a new `v`. A vote that comes before its poll waits for it, 10,000 at most, past which the oldest are kept in quarantine and come back when their poll does. A poll from before jawd kept votes shows its options with none until jawd has gone through it, soon after it starts |
| `location`, `contact`, `event` | Their fields, ready to show: `location` is `lat`, `lon`, `name`, `address`, `url`, each trimmed or null; `contact` is `name` and `vcard`, for a card someone sent and for yours alike: the name it goes by, as its message names it or failing that as its vCard does (null when neither does), and its vCard as it came (null when it came without one). A message of several cards is one row of kind `contact`, its `name` what its message calls them or else their names, and its `vcard` each card's vCard after the one before, as several contacts export to one file, so an interface reads one card or several the same way; WhatsApp's phones mark each number that's on WhatsApp with its account, `TEL;waid=351912345678:+351 912 345 678`. Its words, what `find` finds it by, are the cards' names. `event` is `name`, `description`, `start`, `end` (milliseconds since the epoch, to the second), `place` (a pin's fields as `location` has them, `lat` and `lon` null for a place named alone, or null), `call` (its call link, or null), `cancelled`, and `going`, `maybe` and `not_going`, how many people answered so, each person's newest answer counted once whichever of their JIDs it came from, with `mine`, what you answered (`going`, `maybe` or `not_going`, or null). jawd counts them as it reads the event, so an answer shows in every window showing it, as a row upserted with a new `v`. A location's and an event's `spans` are empty: their fields are what they show |
| `invite` | For `invite`: an invitation to join a group (`group`, its JID, and `name`) or to follow a channel (`channel`, its JID, and `name`, with `admin` true for one to be among its admins), and `expires`, when it runs out (milliseconds since the epoch), each as the invitation says; its caption is the row's `spans` |
| `system` | For `system` and `call`: one sentence, such as "Rui added Sofia", "Group added to the community: Festa de Natal" or "Missed voice call" |

### roster.watch

```json
{"id":4,"method":"roster.watch","params":{"folder":"all","around":"top","before":0,"after":50,"offline":true}}
```

People, online first, then by last seen, then by name. Watching the roster is also what tells jawd whose presence to follow; see Presence. `folder` and `offline` are as Folders and the roster says.

| Field | |
| --- | --- |
| `id` | `person:` and a number |
| `name`, `phone`, `about`, `avatar` | As known, the about text refreshed daily |
| `presence` | `state` (`online`, `seen` or `hidden`), `at` for `seen`, and `known`; typing and recording show from the chat's typing, which the answer's `typing` and the `typing` pushes say for the roster's people's one-to-one chats (see Presence and typing in chats) |
| `chat` | Their one-to-one chat's `id`, if there is one |
| `folders` | The IDs of your folders they're filed in |

### folders.watch

The whole tree; it's small. Each folder: `id`, `parent`, `name`, `position`, `smart` (a smart folder's rule, or null), `unread` and `mentions` (each chat counted once), `count` and `favourite` (its Alt slot, or null), with `favourites`, the nine Alt slots, beside the rows. Built-in folders have fixed IDs: `all`, `unread`, `groups`, `communities`, `archived`.

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
- **Then messages,** from the full-text index, newest first, accent-blind, with every word typed: a word of three letters or more matches the words it starts ("jan" finds jantar), and one of one or two letters matches only itself ("ja" finds já), since gathering every word that starts with one or two letters costs most of a keystroke's budget. `chat` narrows them to one chat; names are found anywhere.
- **Then links,** every URL messages gave, found as messages are, newest first, by the words of their preview's title and description, their host and their URL, each link having every word typed. A link's `text` is its title and host (` · ` between them), or its URL when it has no title, with what matched marked; its `row` is a link as the library's rows have one (`id`, its message's ID with `~` and its place among the message's links; `kind` `link`; `message`, `chat`, `chat_name`, `from` (absent on yours), `mine`, `at`; and `link`: `url`, `host`, `title`, `description`), and its `count` is of the messages whose links matched. Media and files (M3) will come after links; until then those kinds find nothing.
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
  - `chat` must be a chat jawd has (error 3 otherwise; `chat.start` makes one). Text that's only spaces is -32602, and more than 65,536 characters is error 5. `reply` and `mentions` are as "Message actions" says. In a chat with disappearing messages on, the message carries its timer.
  - `preview` (true when it's left out): text with a URL goes with a preview of the first URL's page, as the phones send one: jawd fetches the page, takes its title, its description and a small thumbnail of its image, and the row's `link` has them. The fetch has 3 seconds, the page's and the image's together, and the answer waits for it; a page that doesn't answer in time, isn't HTML or can't be reached leaves the text to go without a preview, and an image that doesn't come in time leaves the preview without its thumbnail. The next message to the same chat, from any interface, waits for it, so none goes ahead of another. jawd fetches only pages at public addresses, connecting to them itself rather than through a proxy, follows 5 redirects at most, reads the first 512 KB of a page and 2 MB of an image at most, sends no cookies, and says it's jaw. `preview` false sends the text as it is and fetches nothing.
  - The outbox holds at most 1,000 messages and actions of yours, waiting, unconfirmed or failed. Past that, `send.text` is refused with error 5 and `data` `{"limit": "outbox", "waiting": 1000, "failed": n}`, and its text isn't kept, so the interface keeps it in its box; nothing already in the outbox is ever dropped, and new sends are taken again once some have gone or been retried.
  - Each chat's messages go in the order they were sent, up to 16 chats at once; a chat waiting to try again leaves its place to the others. As on the phone, a message waits, `pending`, for as long as WhatsApp can't be reached, however long that is, and goes once it can. While it can, a send that fails on the way (the network, the server busy) is tried again after 1 s, doubling to a minute, for about a day of trying. A refusal (a number that isn't on WhatsApp, a group you've left or can't write in, a message the server won't take) makes it `failed`, until `send.retry`.
  - A message is never sent with a second ID: every try, after a restart too, carries the one jawd made. Recipients' phones keep messages by chat, sender and ID, so one that arrives twice is one message to them, as WhatsApp's own resending relies on. What WhatsApp's server answers to an ID it already has is still to be checked on a real account. So when a message that may already have gone (one tried before, or left waiting when jawd stopped) is refused, it isn't shown `failed`: it stays `pending` and waits up to a day for a receipt, which settles it, and only then fails. Any receipt for a message settles it, whatever its send was doing.
  - Deleting a pending message for yourself doesn't cancel its send, as on the phone: it still goes, and stays deleted here. If WhatsApp refuses it, nothing is left to retry, as nothing is of one deleted for you once it had failed: `send.retry` is error 3 for either.
- **`send.poll`** sends a poll as a message of yours, answered, kept, queued and shown as `send.text`'s is, `key` too, but for its chat's draft, which stays, since what's typed there isn't what was sent: `question`, its `options` in their order, each named once, and `max`, how many of them one vote may choose (1 when it's left out, up to all of them). Each poll goes with a secret of its own, which jawd makes and the engine keeps as it sends it, since every vote in it is encrypted with a key made from it. A question or an option that's only spaces, fewer than 2 options, one named twice (a vote names an option by its name) or `max` outside 1 to their number is -32602; past 12 options, 255 characters for the question or 100 for an option it's error 5, with `data` `{"limit": "options", "most": 12}` (`"question"`, `"option"`). An interface that makes the poll from what's typed in the box, as from a `/poll` typed there, clears the draft itself once it's sent, with `draft.set` and empty text, or those words come back when the chat is opened again.
- **`poll.vote`** is your vote in a poll, all the options you choose now, by name: it replaces your last, and none takes it back. It names its poll as the actions on a message do and is one of them: it answers once it's on disk, takes a `key`, goes as a message of its own after the chat's earlier ones (the engine encrypts it with the poll's secret), shows at once in the poll's `options` and `voters`, and is kept as yours once WhatsApp has it; if WhatsApp refuses it, the poll shows your last vote again, and every interface is sent a `notice`. More options than the poll's `max` is error 4 (a second choice in a poll taking one); an option the poll hasn't is -32602; a message that isn't a poll, or one deleted, including while your deletion of it for everyone is on its way, as windows show it then, is error 4, and so is one whose copy in a chat merging into its own is deleted or being deleted, which windows of the chat kept show only once the merge folds that copy in; one jawd doesn't have is error 3. The same option twice is one choice.
- **`send.contact`** sends contact cards as a message of yours, answered, kept, queued and shown as `send.text`'s is, `key` too, but for its chat's draft, which stays, as a poll's does, so an interface that makes the cards from what's typed in the box clears the draft itself, as one making a poll there does: `cards`, one or several, each a `name`, the name it goes by, and a `vcard`, one vCard 3.0 as the Contacts app exports one, from `BEGIN:VCARD` to `END:VCARD`. One card goes as WhatsApp's contact message, and several as one message of them all, named for how many there are, in the order given, each name trimmed and each vCard as given. Its row is kind `contact`, its `contact` as the message row has it: a card's name and vCard, or several's name and their vCards one after another. In a chat with disappearing messages on, the message carries its timer. No cards, a card whose name is only spaces, or a `vcard` that isn't one vCard is -32602 with `data` `{"param": "cards"}`; past 64 cards, a name past 256 characters, or vCards past 256 KiB together it's error 5, with `data` `{"limit": "cards", "most": 64}` (`"name"`, 256; `"vcards"`, 262144); a chat jawd doesn't have is error 3. A card with its photo in it takes far more room than one without, and WhatsApp's phones send theirs without.
- **`chat.start`** takes `phone` (international form; `+`, spaces, dashes and brackets are fine) or `person` (`person:` and a number, as `find` gives them) and answers with `chat`, as `chats.watch` sends it, found or made. A number is checked with WhatsApp first (error 3 if it isn't on WhatsApp, 2 if the engine isn't connected), and WhatsApp's LID for it is kept with it, so messages addressed either way are one chat. A person's chat is found under any JID they're known by before one is made.
- **`draft.set`** keeps `text` as the chat's draft for every interface, shown in the chat list as `draft`; empty text clears it. Error 3 for a chat jawd doesn't have, 5 past 65,536 characters.
- **`read`** says you've read `chat` up to `up_to`, a message's ID: the chat's read position moves forward (back only while it's the phone's guess, before WhatsApp has said where it is), its `unread` is counted again from there, and jawd marks the messages read on WhatsApp, at once and then at most every 2 seconds per chat, however fast an interface reads. Sending a message reads the chat up to it as replying on the phone does, and marks what it reads read on WhatsApp the same way; a forward reads none of the chats it goes to. When read receipts are off on the phone, only your own devices hear. Error 3 for a chat or message jawd doesn't have.
- **`typing`** (`chat`, `state`: `composing`, `recording` or `paused`) answers once the requests sent before it have been, as every request but those answered at once; it needs nothing from the disk itself. jawd tells WhatsApp when typing starts or changes, again every 10 seconds while it's said, and stops it 10 seconds after an interface last said so, or at once on `paused`.

## Reconnecting

A client that loses its connection reconnects, sends `hello` with `since` (the last `seq` it saw), and watches its windows again, each with `have`: the `id`s of the rows it holds for that window. Rows in `have` that haven't changed since `since` are left out of the results, and `remove` lists the ones in `have` that are no longer in the window, so a reconnect costs what changed, not the windows; rows that came into the window come whole, changed or not. Chat lists, like the folder tree, are watched again whole, whatever `since` says: every row comes, and `remove` lists the ones in `have` that are no longer in the window, since the tree's counts change with no new version, and so does what a community's row sums up of the groups of it a list holds, when a folder moves under another, a smart folder's rule changes, a folder becomes a favourite or a mute runs out. A watch can give its own `since`, which wins over hello's; without `have`, a watch returns every row. If `since` is older than the feed keeps (7 days), or newer than its last `seq` (jaw.db was made again), the watches return everything, as a first watch does.

```json
{"id":3,"method":"messages.watch","params":{"chat":"351912345678@s.whatsapp.net","around":"newest","before":80,"after":0,"since":81200,"have":["3EB0A1B2C3D4E5F6","3EB0C431F2A9B8E7D6"]}}
```

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `hello` | `protocol`, `client`, `version`, `focus`, `since` | See Hello |
| `status` | | jawd's version, connection, account, engine, each pool's depth and times (the `downloads` and `images` lanes among them), what's missing (ffmpeg), what jaw.db holds (`stored`, among it `links`, `links_backfill`: the messages from before links were kept still to be looked at for theirs, and `link_thumbnails` with their bytes and how many were refused for want of room; `votes_waiting` for polls jawd hasn't seen, `votes_set_aside` past the 10,000 that wait, and `polls_backfill`, the messages still to be looked at for polls from before votes were kept; `places_backfill`, the messages still to be looked at for locations from before jawd found them by their address too; `events_backfill`, the messages still to be looked at for events from before answers were kept, to read again with what was kept sealed for them, `answers_waiting` for events jawd hasn't seen, `answers_set_aside` past the 10,000 that wait, `edits_set_aside`, their edits past the 10,000 that wait sealed, `events_opening`, the events jawd has with answers and edits that waited for them still to be taken, a few at a time in the background, past the 16 the event's own transaction takes, and `events_unopened`, those with answers or edits kept sealed that didn't open with the JIDs jawd knew their people by, tried again once one of them is known by another; `kinds_backfill`, the messages still to be looked at for those jawd couldn't show when they came, to convert again, `copies_backfill`, the messages from before jaw.db kept media still to be looked at for those that bring a file, to give each its copy, `contacts_backfill`, the messages still to be looked at for contact cards from before a message of several showed as one card's does, to read again, and `encrypted_kept`, the edits and answers sealed with their message's secret that jawd keeps with what they change: an event's, until jawd has the event, and those it can't open, such as other messages' edits; `quarantined`, everything jawd keeps in quarantine: the changes it failed to convert or to write, frontends' settings among them, the votes set aside, and the chat actions and deletions for you it gave up on; and `awaiting`, the chat settings and stars changed from jaw whose change WhatsApp hasn't sent back yet, one each at most, what WhatsApp sends for one of them meanwhile being kept but not shown, as "Chat actions" says), `interfaces`: how many are `connected`, whether one is `focused`, where `notifications` go, the requests `in_lanes`, read and not yet answered (this one among them), the most a connection's lane holds (`lane_most`), the `lanes_full`, whose connections jawd reads nothing more from until one is answered, and the connections with a reply, a `layout` push or a `settings` push waiting for room (`replies_waiting`, one each at most), which it reads nothing from and sends nothing else meanwhile, and the finds and saves being answered on tasks of their own (`apart`, at most `apart_most` a connection); `windows`: how many are `watched` on every connection, how many of them are `distinct`, the `most` one connection can watch, the bytes of the reads kept for windows watched alike to share (`kept_bytes`, at most `kept_most`, the oldest `let_go` past it), and the window `reads` made, `taken` from a kept read and `joined` to one under way; `layouts`: how many interfaces have one `kept`, of the `most`, the `largest_bytes` of a layout that isn't a book, of `bytes_most`, the `books` and the `largest_book_bytes`, of `book_bytes_most`, and how many were `refused` for a limit; `typing`: the `chats` with someone typing now, how many changes WhatsApp sent (`heard`) and of them `dropped` for coming too fast, those `waiting` to be taken now, of the `waiting_most` that can, the `typing` pushes `sent`, and the changes `merged` into a newer one before they went (see Presence and typing in chats); `settings`: each domain's settings in effect by its name; `settings_ignored`: any whose value jawd doesn't take; `presence`: how many people are `followed` on this connection to WhatsApp (at most `most`, and `per_minute` newly), how many the windows show (`wanted`) and of them how many the limits leave `waiting`, the presence `heard` and `dropped` (coming faster than it could be written), and the batches of about texts asked for (`about_asked`); `folders`, the background check of the folders' counts against what they count: how many `checks` ran, how many found them `drifted` and built them again, and the `last_drift` it found; and `media`: the cache's `cache_files`, `cache_bytes` and `cache_limit`, `auto_bytes`, how many copies are `wanted`, `downloading` (of them `unasked`, which nobody asked for, at most `unasked_most` at once) and `waiting_for_phone`, files no message names (`unnamed`), files downloaded and waiting to be named (`naming`), the progress of downloads nobody waits on that waits to be written (`progress_waiting`, at most `progress_most`, and `progress_passed` over past that since jawd started), preview writes not yet made durable (`unsettled`), whether the start's check of previews a power cut may have broken is going (`checking`) and the files it hasn't passed (`suspect`), whether files are flushed in full rather than fenced (`full_flush`), and since jawd started the `partial_removed` and `orphans_removed` at start, files `evicted` and previews `remade`, with `paused` saying why background downloads wait while the disk is nearly full; and for media out, the copies of your files `preparing`, `uploading` and `unsent` (failed until `send.retry`), jaw's own copies of the files you picked (`kept_files`, `kept_bytes`) and those nothing needs any more (`kept_unneeded`), with the `transcodes` and `uploads` lanes among the pools |
| `settings.set` | any of `theme`, `messages` | `settings`, all of them, once kept. See Settings |
| `themes` | | `themes`: every theme with its colours. See Settings |
| `focus` | `focused` | |
| `link.start` | `method` (`qr` or `phone`), `phone` | Pushes `link` as it goes |
| `link.cancel` | | |
| `link.logout` | | Answered once the account is logged out and jawd has forgotten it; a `connection` push says `logged_out` (`logged out from jaw`), on the connection that asked just after the answer |
| `chats.watch`, `messages.watch`, `roster.watch`, `folders.watch` | See Windows, and `since` and `have` when watching again | `watch`, `seq`, `total`, `rows`, `remove` (with `have`); `older` and `newer` for messages; `typing` but for the folder tree, and `presence` for a one-to-one chat's messages (see Presence and typing in chats) |
| `watch.move` | `watch`, `around`, `before`, `after`, and `open` for a chat list | As its watch's, with only the rows the client lacks and `remove`; a library window keeps its kind and filters |
| `unwatch` | `watch` | |
| `search` | `query`, `chat` (optional), `limit`, `cursor` | `hits` (message rows with the match marked) and the next `cursor` |
| `contacts.find` | `query` | People and chats to start or open, best first; numbers are checked against WhatsApp |
| `find` | `query`, `kinds` (any of `chat`, `person`, `folder`, `message`, `link`, `media`, `file`; all by default), `chat` (optional), `limit` (of each kind) | `results`, best first, each with its `kind`, a `score`, the match marked, and the row it found, and `counts` of each kind; a newer `find` from the same connection cancels this one. See Finding |
| `chat.opened` | `chat` | |
| `library.shown` | `items`: the `id`s of the library's items on screen, at most 200 | See Library |
| `library.watch` | `kind` (`photo`, `video`, `voice`, `file`, `link` or `all`), and optionally `chat`, `person`, `since`, `until`, `text`, with `around`, `before`, `after` | `watch`, `seq`, `total`, `older`, `counts`, `rows`: each an item, a message's photo, video, voice message, file or link, with what it needs to show, its sender, chat and time, and the message's `id` to jump to; and `note`. See Library |
| `send.text` | `chat`, `text`, `reply`, `mentions`, `preview` (default true), `key`, and `at` and `repeat` to send it later | `message` and `row`, or `scheduled` when it's sent later. See Sending, and Scheduled messages and reminders |
| `send.retry` | `message` | |
| `chat.start` | `phone` or `person` | `chat`. See Sending |
| `send.media` | `chat`, `files` (each a `path`, `kind` and `caption`), `quality` (`sd` or `hd`), `reply`, `key`, and `at` and `repeat` to send it later | `messages`, `album` and `rows`, or `scheduled` when it's sent later. See Media out, and Scheduled messages and reminders |
| `send.poll` | `chat`, `question`, `options`, `max` (default 1), `key`, and `at` and `repeat` | `message` and `row`, as `send.text`, or `scheduled`. See Sending |
| `poll.vote` | `message`, `options` (by name, none to take your vote back), and optionally `chat`, `key` | See Sending |
| `send.event` | `chat`, `name`, `description`, `start`, `end`, `place`, `call`, `key`, and `at` | `message` and `row`, as `send.text`, or `scheduled`. See Composing |
| `send.contact` | `chat`, `cards` (each a `name` and a `vcard`), `key`, and `at` | `message` and `row`, as `send.text`, or `scheduled`. See Sending |
| `send.location` | `chat`, `lat`, `lon`, `name`, `address`, `key`, and `at` | `message` and `row`, as `send.text`, or `scheduled`. See Composing |
| `gif.search` | `query` (empty for what's trending), `limit`, `cursor` | `gifs` and the next `cursor`. See Composing |
| `send.gif` | `chat`, `gif` (an `id` from `gif.search`), `caption`, `reply`, and `at` | `message`, or `scheduled`. See Composing |
| `link.preview` | `text` | `url`, the URL `send.text` would preview in it, and its `link`, as a message row's with `thumbnail` in place of `preview`; each null when there's none. See Composing |
| `react` | `message`, `emoji` (empty to remove), and optionally `chat`, `key` | |
| `edit` | `message`, `text`, and optionally `chat`, `key` | |
| `delete` | `message`, `for` (`me` or `everyone`), and optionally `chat`, `key` | |
| `forward` | `messages`, `to` (chats), and optionally `chat`, `key`, `at` | `messages`, or `scheduled`, one for each chat |
| `scheduled.edit` | `scheduled`, any of `text`, `captions`, `at`, `repeat`, `reply`, `mentions`, `question`, `options`, `max`, and optionally `v` | `scheduled`. See Scheduled messages and reminders |
| `scheduled.send_now` | `scheduled`, and optionally `v` | `message` and `row`, or a forward's `messages` |
| `scheduled.skip`, `scheduled.cancel` | `scheduled`, and optionally `v` | `scheduled.skip`: `scheduled`, null once nothing is left |
| `scheduled.watch` | optionally `chat`, with `around`, `before`, `after` | `watch`, `seq`, `total`, `held`, `rows` |
| `when.parse` | `text`, and optionally `repeat` (true) | `at` and `says`, and `repeat` with it |
| `remind` | `message`, `at` (null to take it away), and optionally `chat` | `reminder`, null once it's taken away |
| `reminders.watch` | `around`, `before`, `after` | `watch`, `seq`, `total`, `due`, `rows` |
| `star` | `message`, `starred`, and optionally `chat` | See Chat actions |
| `read` | `chat`, `up_to` (a message) | See Sending |
| `typing` | `chat`, `state` (`composing`, `recording` or `paused`) | See Sending |
| `draft.set` | `chat`, `text` | See Sending |
| `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` | `chat`, and `pinned`, `archived`, `until` or `unread` | See Chat actions |
| `chat.clear`, `chat.delete` | `chat` | Not served yet: clearing and deleting a chat come in a later step |
| `chat.info` | `chat` | Its kind, its members and their roles, its community and its disappearing timer; its media count comes with the library |
| `message.info` | `message`, and optionally `chat` | When it was sent, and each recipient's delivered, read and played times |
| `history.more` | `chat` | More arrives as upserts, once the phone answers |
| `media.fetch` | `message`, `variant` (`sd` or `hd`, `sd` by default), and optionally `chat` | Progress and the path arrive as upserts to the message. See Media in |
| `media.save` | `message`, `to` (a folder or a file's path, whole or starting `~/`), and optionally `chat`, `variant` | `path`. See Media in |
| `media.cancel` | `message`, and optionally `chat` | See Media in, and for one of yours on its way, Media out |
| `voice.play` | `message`, `at`, `speed` | Pushes `playback` |
| `voice.pause`, `voice.seek`, `voice.speed` | `at` or `speed` | |
| `voice.record` | `chat` | Pushes `recording` |
| `voice.stop`, `voice.send`, `voice.cancel` | `voice.send` takes `at` and `repeat` | `voice.send` returns `message`, or `scheduled` |
| `folder.create` | `name`, `parent`, and `rule` for a smart folder | `folder` |
| `folder.rename`, `folder.move`, `folder.delete` | `folder`, and `name` or `parent` and `position` | |
| `folder.add`, `folder.remove` | `folder`, `item` (a chat's `id` or a `person:` id) | |
| `folder.smart` | `rule`, and `folder` to change a smart folder's rule or `name` to make one | `folder`, when it made one |
| `favourite.set` | `slot` (1 to 9), `target` (a folder or a chat, or null to clear the slot) | |
| `layout.get` | `interface` (optional, the connection's own by default), `format` (optional, 1 or 2, the one it was kept in by default) | `interface`, `v`, `layout` and `dropped`. See Layouts |
| `layout.set` | `v`, `layout` | As `layout.get`'s, and `kept`. See Layouts |

## Other pushes

| Push | Params |
| --- | --- |
| `link` | `state`: `qr` with `code` and `expires`, `code` with the phone `code` to type, `done`, or `failed` with `reason` |
| `connection` | `state`: `connecting`, `online`, `catching_up` (with `done` and `total`), `phone_offline`, `offline`, `logged_out`, `update_needed`, `banned` (with `until`) or `engine_restarting` |
| `settings` | its domain's settings, as hello's `settings` |
| `typing` | `chat`, `who` (each `id`, `name` and `state`, `typing` or `recording`; empty once nobody is) |
| `presence` | `chat`, `person`, `state` (`online`, `seen` or `hidden`), `at` for `seen`, `known` |
| `job` | `id`, `kind` (`upload`, `download`, `transcode`), `message`, `variant` (`sd` or `hd`), `progress` from 0 to 1. See Media out |
| `playback` | `message`, `state` (`playing`, `paused`, `ended`), `at`, `seconds`, ten times a second while playing |
| `recording` | `state` (`recording`, `stopped`), `seconds`, `level` (0 to 100), ten times a second while recording |
| `notice` | `text` and `tone` (`info`, `warning`, `error`): something an interface should say once |
| `resync` | `watch`: watch it again |
| `layout` | As `layout.get`'s result: the layout another connection of the same interface just kept. See Layouts |

WhatsApp never says the phone is offline, so `connection` says `phone_offline` once something jawd asked the phone and knows it has an answer to (older history in a chat the phone has said it keeps more of, so far) has gone 20 seconds unanswered online, counted afresh each time the connection comes back, and `online` again once the phone sends anything, or two minutes after jawd asked; messages still arrive and send meanwhile.

When WhatsApp stops answering the engine's keepalive pings, `connection` says `connecting` while the connection stays open: what you send waits, as it does whenever jawd is connecting, and what arrives still comes. Once a ping gets through again it says `online`, or `catching_up` with how far the catch-up has got if one was under way, since a timeout doesn't end one, and what waited goes, once. If the pings go on failing for 3 minutes, the engine connects again, and `online` comes with the new connection, ending a catch-up the old one left.

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

## Composing (proposed)

Proposed with the Mac app's box (its design, `docs/plans/2026-10-27-jaw-mac-box-design.md`), for jawd to build: sending events, contacts and places, and GIFs found by search; and, with the Mac app's links and polls (`docs/plans/2026-10-30-jaw-mac-links-and-polls-plan.md`), a link's preview before it's sent. Contact cards are built, sent with `send.contact` (see Sending) and shown as the message row's `contact`, each card or several; events and places are served, in M2 beside polls; and GIF search comes after M4, and until then its rows above are the draft. Each answers as `send.text` does, with the `message` it made and its `row`, which arrives as an upsert too, once it's on disk, synced, with its outbox entry, and goes in its chat's order carrying the chat's disappearing timer; a `key` makes asking again safe, as for `send.text`.

```json
{"id":40,"method":"send.event","params":{"chat":"120363000000000002@g.us","name":"Sintra","description":"Bring the rope","start":1792231200000,"end":1792252800000,"place":"Cabo da Roca","call":false,"key":"9f2c"}}
{"id":41,"method":"gif.search","params":{"query":"thank you","limit":24}}
{"id":41,"result":{"gifs":[{"id":"giphy:3o7abKhOpu0NwenH3O","w":480,"h":270,"preview":"media/g/3o7abK.mp4","title":"Thank You"}],"cursor":"24"}}
{"id":42,"method":"link.preview","params":{"text":"the routes are up: www.climb.pt/sintra."}}
{"id":42,"result":{"url":"https://www.climb.pt/sintra","link":{"url":"https://www.climb.pt/sintra","title":"Sintra: the routes","description":"Granite slabs above the sea","thumbnail":"/9j/4AAQSkZJRgABAQAAAQABAAD"}}}
```

- **`send.event`:** a WhatsApp event, as the phones make them: `name` (required), `description`, `start` (required), `end` (milliseconds since the epoch, which WhatsApp keeps to the second), `place` (a name, or a place as `send.location` takes it), and `call` true to add a WhatsApp call link. Each event goes with a secret of its own, as the phones give one, which every answer to it and every edit of it is sealed with. A name that's only spaces, a `start` of 0 or less, an `end` before `start`, or a `place` that's neither a name nor a pin is -32602, as a pin's latitude or longitude out of range is, with `data.param` `place.lat` or `place.lon`; past 255 characters for the name, 2,048 for the description, or 1,024 for a place's name (`place`, or a pin's `place.name`) or address (`place.address`), it's error 5, with `data` `{"limit": "name", "most": 255}` and so on. `call` true is error 4 for now: the phones ask WhatsApp's server for a call link, which jaw's engine can't yet. Its row is kind `event`. Answers to it, going, maybe or not going, update its counts as upserts, each person's newest answer counting once, by when they answered and then its ID; jawd opens those WhatsApp seals with the event's secret itself, whichever way they come, live, from history or kept by an earlier jawd, and one that comes before its event waits for it, 10,000 at most, past which the oldest are kept in quarantine and come back when their event does, edits alike; the event takes 16 of what waited for it at once, and the rest a few at a time in the background, so its counts may land in a moment's steps. An edit by the event's author, as the phones make one to change it or cancel it, replaces what it shows, the newest by its time, and marks it `edited`; one from anyone else is kept and changes nothing.
- **`send.contact`:** built, as Sending says: one card or several, each a `name` and a `vcard` (vCard 3.0, as the Contacts app exports it). Its row is kind `contact`.
- **`send.location`:** a place by `lat` and `lon` in degrees, with an optional `name` and `address`, each trimmed. A latitude outside -90 to 90 or a longitude outside -180 to 180 is -32602, with `data.param` `lat` or `lon`; past 1,024 characters for the name or the address it's error 5, with `data` `{"limit": "name", "most": 1024}` (`"address"`). Its row is kind `location`, found by its name and its address. Live locations aren't sent.
- **`gif.search`:** jawd asks Giphy, and nothing else in jaw talks to a service besides WhatsApp: only when an interface asks, which it does only while a GIF picker is open and something is typed (an empty `query` asks for what's trending). jawd fetches each result's small looping MP4 into its media cache and answers with paths, so interfaces show files, never URLs. Results are kept for an hour; Giphy's key is jawd's, set by environment. Error 6 when Giphy doesn't answer in 3 seconds.
- **`send.gif`:** a result by its `id`: jawd fetches its MP4 and sends it as WhatsApp's GIF (a looping video), with the `caption`. Its row is kind `gif`.
- **`link.preview`** (the Mac design's P11): a link's preview before its message is sent, for a box to show while the link is typed. `text` is the box's words as `send.text` would send them, and jawd finds the URL to preview in them as `send.text` does, so an interface never parses for it: the first starting `https://`, `http://` or `www.`, without the punctuation that ends a sentence after it or an unbalanced `)`, with `https://` before a bare `www.`. It answers with that `url`, or null when the words have none, and its `link` as a message row has one (`url`, `title`, `description`) but with `thumbnail`, the JPEG itself in base64, in place of `preview`, or null when the page gives none, fetched as `send.text`'s preview is: 3 seconds for the page and its image together, public addresses only, 5 redirects, 512 KB of page and 2 MB of image at most, no cookies, saying it's jaw.
  - **Kept by URL:** jawd keeps each answer by its `url` for ten minutes, 64 at most, a null `link` too, so a page with nothing to show isn't fetched again at each pause. The `send.text` that follows with `preview` true sends the one kept for its URL, or no preview when it was null, without fetching again; one that comes while its URL is still being fetched waits for that fetch, within what's left of its 3 seconds, rather than starting another. A newer `link.preview` from the same connection cancels the one before, as a newer `find` does. A `link.preview` whose URL is being fetched waits for that fetch, as `send.text` does, and one cancelled is answered with error 7 while its fetch carries on into the cache, within its 3 seconds.
  - **Nothing on disk until it's sent:** a preview's thumbnail waits in memory with its answer, 64 KB at most as a kept thumbnail is, never in `media/links/` nor against their 256 MB. A `send.text` that sends the preview keeps it there as it keeps a fetched one, in the message's synced commit, so one never sent goes with its answer. `status` says how many answers are kept and being fetched, and their thumbnails' bytes: 64 and 4 MB at most.
  - **Off, as WhatsApp's "Disable link previews":** fetching as you type tells a page's server your address for a link you may never send, so jawd's `link_previews` setting (the Mac design's P5, and an environment variable, `JAW_LINK_PREVIEWS`, until then) is on by default, as WhatsApp's is, and off answers every `link.preview` with a null `link` without fetching and has `send.text` send no preview whatever its `preview` says.
  - An interface asks once its words have stayed the same for half a second with `http://`, `https://` or `www.` among them, and asks again only when they change, so nothing is fetched while a link is being typed; `send.text`'s `preview` false sends the text without one, as an × on the box's preview asks.

## Message actions (proposed)

Proposed with the terminal's M2 actions, for jawd to build, and served: `send.text`'s `reply` and `mentions`, `react`, `edit`, `delete`, `forward`, `message.info` and `chat.info`. It keeps their names and fills in what they leave open. Each action's result reaches every interface the same way: the rows it changes, upserted into the windows that show them, so an interface draws what jawd says rather than what it guessed.

`react`, `edit` and `delete` answer with nothing once they're on disk, synced, as `send.text` is, and `forward` with its messages then; each takes a `key` as `send.text` does: asking again with it within a day answers the same and does nothing more. Each, and `message.info`, names its message by `message`, its ID (`forward` its `messages`), and optionally `chat`, where it is: with `chat`, jawd finds it at once; without it, jawd looks for the ID in each chat, and an ID it finds in more than one chat is -32602 with `data.param` `chat`. A reaction, an edit or a deletion for everyone goes to WhatsApp as a message of its own, after the chat's earlier messages, waiting while WhatsApp can't be reached and tried again as a message is, and counts toward the outbox's thousand. Windows show it at once, as rows upserted with a new `v`, and the chat list too when it's the chat's newest message; jaw.db keeps it as done only once WhatsApp has it. If WhatsApp refuses it, the rows show as they were, and every interface is sent a `notice` saying so. One refused when it may already have gone, with no receipt for it within a day, shows as refused in the same way, but jawd keeps it for at least 30 days (a thousand at most, outside the outbox's thousand), and a receipt for it in that time does it then.

An action on a message of yours that hasn't reached WhatsApp yet (still pending, or refused) waits until that message has, so it never arrives before it, even while the message's chat is merging into another. If WhatsApp refuses the message, a reaction or an edit waits on, shown as now, and goes after the message once `send.retry` has sent it; a deletion for everyone goes, and once WhatsApp has it, nothing of the message is left to send. `send.retry` never sends a message deleted for everyone: it's error 4 while your deletion of it is on its way, and 3 once it's deleted, from jaw or from your other devices, or once your deletion is kept for a late receipt, since nothing of it is left to send then. A reaction, an edit or a vote whose message is deleted for everyone before it goes, by you, your other devices or whoever sent it, isn't sent, whether or not you then delete the message for you: the row shows without it, and every interface is sent a `notice` saying so. Once you've deleted a message for you, though, a deletion of it for everyone that comes after is one jawd no longer sees, so a reaction or an edit still waiting goes, as it would from your phone.

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
- **`react`** (`message`, `emoji`): one reaction of yours per message, as WhatsApp keeps them; another emoji replaces it, and an empty one removes it, so asking for the same twice is the same as once (an interface toggles one off by sending an empty one). Any emoji WhatsApp takes, with or without its variation selector, which jawd keeps as it came; more than 64 bytes or one with spaces is -32602. The row's `reactions` change, with `mine` on yours. Error 3 for a message jawd doesn't have, 4 for one deleted, including while your deletion of it for everyone is on its way, as windows show it then, or a line about the chat, and 4 for one whose copy in a chat merging into its own is deleted or being deleted, which windows of the chat kept show only once the merge folds that copy in.
- **`edit`** (`message`, `text`): your own text message, within 15 minutes of sending, as WhatsApp allows; past that, someone else's, a deleted one (including while your deletion of it for everyone is on its way, and one whose copy in a chat merging into its own is deleted or being deleted, which windows of the chat kept show only once the merge folds that copy in), one that failed to go or another kind is error 4, with the reason in `message` (captions come later). Text that's only spaces is -32602. The row's `spans` become the new text's and `edited` true; replies quoting it keep their summary, as the phones do.
- **`delete`** (`message`, `for`):
  - `me` removes it from jaw only, at once: it goes from every window as a `remove`, and from search, a copy arriving later doesn't bring it back, and jawd tells your other devices, by WhatsApp's app state. Any message. Telling them takes a place in the outbox's thousand, so past it a deletion for you is error 5, unless it's of a message of yours that failed to go, whose place it takes, so a full outbox can always be emptied.
  - `everyone` deletes your own for everyone within 60 hours of sending, as WhatsApp allows (error 4 after, for someone else's or one that failed to go; a group's admins deleting others' messages comes later; one deleted already answers as done). The row stays with `deleted` true and no spans, and its `summary` says "You deleted this message" for yours and "This message was deleted" for theirs, which an interface may show its own way, as the terminal's IRC layout reads yours as theirs do; from M3, when jawd keeps media, its media is dropped from the cache.
- **`forward`** (`messages`, `to`): each message to each chat, in order, as new messages of yours with `forwarded` counting the times the original had been, plus one; WhatsApp marks five or more "forwarded many times". Answers with the new messages' IDs, chat by chat, once all are on disk as `send.text`'s is; each then goes, with its own outbox row and ticks, as a message of yours does, and the chats' drafts stay. A forward is chosen from a list of chats rather than sent in one, so it reads none of them: their `unread` and read positions stay, and WhatsApp is told nothing was read. A message goes as it says now, as last edited; media goes as it came, without being sent again, so it reaches whoever WhatsApp still has it for; a quote or mentions it carried don't go with it. At most five chats at once, as the phones allow (error 5, `data` `{"limit": "forward", "most": 5}`), past the outbox's thousand error 5 as `send.text`; a chat jawd doesn't have or a message it can't find is error 3; a deleted message, a view-once one, a line about the chat, a kind WhatsApp doesn't forward (a poll, an event, a live location, an album) or one jawd hasn't what it said for is error 4.
- **`message.info`** (`message`): when your message was `sent` (null while it's still on its way), and for each recipient their `person`, `name` and the times it was `delivered`, `read` and, for voice, `played`, each null until it is, by name. The recipients are everyone a receipt for it came from, whichever of their JIDs it came by, and, while it's still on its way to some, those jawd sent it to (in a group, its members when it was sent, but you; in a direct chat, the other person). For someone else's message, `recipients` is empty and `sent` is when it was sent.
- **`chat.info`** (`chat`) answers with the chat's `kind`, its `community` if it's in one, its disappearing timer as `ephemeral` (seconds, 0 when off), and `members`: each member's `person`, the `name` to show, `phone` (`+` and the number) where jawd knows it, and `admin`, for completing `@` names, each person once, by name. A direct chat's members are the other person and you. Error 3 for a chat jawd doesn't have.

## Media in

A message's file comes into jaw's cache by itself or when asked, and its row says how it is as it goes, as upserts to the message, like everything else about it.

```json
{"id":61,"method":"media.fetch","params":{"message":"3EB0A1B2C3D4E5F6","variant":"hd"}}
{"id":61,"result":{}}
{"method":"messages","params":{"watch":"w2","seq":81302,"upsert":[{"id":"3EB0A1B2C3D4E5F6","…":"…","media":{"w":4032,"h":3024,"bytes":3480211,"mime":"image/jpeg","state":"downloading","progress":0.4,"thumbnail":"media/previews/9d6c…20c8.instant.jpg","hd":true}}]}}
{"id":62,"method":"media.save","params":{"message":"3EB0A1B2C3D4E5F6","to":"~/Downloads"}}
{"id":62,"result":{"path":"/Users/me/Downloads/3EB0A1B2C3D4E5F6.jpg"}}
```

- **What comes by itself.** As a message arrives, its `thumbnail` is drawn from the tiny copy it carries, within a moment; then photos, stickers, GIFs and voice messages download, and videos, video messages, audio files and documents too when they're no bigger than the size jawd is set to (16 MB, `JAW_MEDIA_AUTO_MB`); the rest wait to be asked for. A message from history is drawn and downloaded the same way once a window shows it. Once the file is here, `path` names it and `previews` follow. After jawd stops without warning, `previews` and `thumbnail` made in its last few seconds are left out until a check at the next start finds them whole, which takes a moment, and are made again if a power cut broke them. An HD copy downloads only when asked for, with `variant` `hd`, and then `hd_path` names it. A view-once message's file is never kept.
- **`media.fetch`** downloads the copy now, ahead of what downloads by itself, and answers once that's on disk, synced, so it's fetched however jawd stops. Error 3 for a message jawd doesn't have, or one with no file, or no HD copy when `variant` is `hd`; 4 for one deleted or view-once; -32602 with `data.param` `chat` for an ID in more than one chat.
- **`state`:** `remote`, not here, and fetchable; `downloading`, with `progress`; `local`, here; `expired`, WhatsApp's server no longer has it, as is often so of history from before jaw was linked, or the phone, asked for it, didn't answer, and `error` says which; `waiting_for_phone`, the phone was asked to send it again; `gone`, the phone no longer has it either. jawd asks the phone by itself, a few times a minute at most, for a file that arrived live or that an interface asked for; for history, only once it's asked for. The phone has two minutes to answer, which needs it online; past that the file is `expired` and saying why, and asking for it again asks again, so none waits for good.
- **`media.save`** copies the file to `to`, into it if it's a folder (named as the message named it, or by its ID), downloading it first if it isn't here yet; it answers with `path` once the copy is whole and synced. It never writes over a file: a name that's taken gets ` (1)`, ` (2)` and so on. -32602 with `data.param` `to` for a path that's neither whole nor from `~/`; 4 if the download fails, with why, or doesn't come within ten minutes (it goes on).
- **`media.cancel`** stops the message's download and anything waiting to be done for it; the file isn't fetched again until it's asked for.
- **The cache** keeps files by their content, so a file sent to several chats is downloaded once, and drops what was used longest ago once it passes its size (5 GB, `JAW_MEDIA_CACHE_MB`), never a starred message's file or one waiting to be sent; a dropped file is `remote` again. A message deleted for you or for everyone takes its file with it, unless another message has the same one.

## Media out

`send.media` sends files to a chat, a message each, in the order given, as WhatsApp's phones send them, and answers once every message is on disk, synced, so they're sent however jawd stops.

```json
{"id":64,"method":"send.media","params":{"chat":"351912345678@s.whatsapp.net","files":[{"path":"~/Pictures/lake.jpg","caption":"the lake on Sunday"},{"path":"/Users/me/Movies/swim.mov","kind":"video"}],"quality":"hd","reply":"3EB0A1B2C3D4E5F6","key":"jaw-7c2e19a4"}}
{"id":64,"result":{"messages":["3EB0D7A16C2B9F04E1A3C5","3EB0E2F93B7A1C6D58049F"],"album":"3EB0C5A18E24D97B3F60A1","rows":[{"id":"3EB0C5A18E24D97B3F60A1","kind":"album","status":"pending","…":"…"},{"id":"3EB0D7A16C2B9F04E1A3C5","kind":"image","media":{"w":1600,"h":1200,"mime":"image/jpeg"},"…":"…"},{"id":"3EB0E2F93B7A1C6D58049F","kind":"video","…":"…"}]}}
{"method":"job","params":{"id":"upload:131","kind":"upload","message":"3EB0D7A16C2B9F04E1A3C5","variant":"sd","progress":0.42}}
```

- **`files`**, 1 to 30, each a `path` (whole, or starting `~/`), what it goes as (`kind`: `image`, `video`, `gif`, `audio` or `document`; from what the file is when it's left out, and a document goes as it is, whatever it is) and its `caption`, up to 1,024 characters. A file that isn't there or can't be read, or that can't go as the `kind` asked, is -32602 with `data` `{"param": "files", "file": n, "path": …}`; one over 2 GB, as WhatsApp allows, error 5 with `{"limit": "file"}`; more than 30 files, error 5 with `{"limit": "files", "most": 30}`. Voice messages go with `voice.send`.
- **What's answered.** `messages` are the files' messages' IDs, in the order of `files`, and `rows` the messages as `messages.watch` sends them, `pending`, the album's first; every window showing the chat gets them at the same moment. Before a file's copy is made its row has the size and type the copy will have, and no `state`; once it's made, `state` is `local` and `path` names it, and its previews follow, as for any file in the cache. `key` is as `send.text`'s: asking again with it within a day answers with the same messages and sends nothing new.
- **jaw keeps its own copy** of each file before it answers, a clone where the disk can make one, which costs no time or room, and a copy where it can't, so what you picked can change or go as soon as it has answered. It's removed once the copies sent are made from it. jaw keeps at most 8 GB of these waiting at once; past that, `send.media` is error 5 with `{"limit": "media"}` until some have been made.
- **What goes.** A photo goes as a JPEG, a video as H.264 and AAC in MP4, a GIF as an MP4 that loops without sound (`gif`), audio as AAC, and a document as it is; what already fits goes untouched, without its metadata. Each message carries a thumbnail, a video's from its poster frame. Two or more photos and videos go as an album: a message of its own, `kind` `album`, sent first, saying how many are coming, which each of them names; the others go beside it in their order. A caption goes with its own file, and `reply` with the first. The chat's draft is cleared once a caption goes, since it's what was sent; files sent without one, and audio, whose message has none, leave it as it is. In a chat with disappearing messages on, each carries its timer.
- **`quality` `hd`** also sends an HD copy of each photo and video that has more to give than SD keeps, as WhatsApp's dual upload sends it: the SD message first, then the HD copy as a message of its own naming it, which the row shows as `hd` and, once it's made, `hd_path`. A file that fits SD goes once.
- **Order.** Each chat's messages go in the order they were sent, a message with a file waiting, `pending`, until its file is uploaded, and the messages after it behind it, as the phones keep them. Copies are made and uploaded in the background meanwhile, SD before HD, so an SD copy uploads while its HD one is made.
- **`job` pushes** say how far each copy has got, at most four times a second and once more at 1: `transcode` while it's made, then `upload`. `id` names the job, `message` its message (an HD copy's is its SD message's), and `variant` which copy it is.
- **`media.cancel`** on a message of yours whose file is still being made or uploaded stops it at once, whatever step it's at, and it's `failed`, with `media.error` `you cancelled it` once its copy was made, until `send.retry` sends it again. One already uploaded is on its way, and goes.
- **Failures.** An upload that fails on the way (the network, the server) is tried again, as a send is; one WhatsApp refuses, a file that can't be read or made, no room on the disk for its copy (twice its size, and a little more), or ffmpeg missing for a video, GIF or audio file, makes the message `failed`, `media.error` saying why once its copy was made, and every interface is told with a `notice`. `send.retry` makes or uploads it again from where it got to; on a message that went without its HD copy, it sends that again. A failed message deleted for you or for everyone takes its copies, and jaw's copy of its file, with it.

## Chat actions

What you set on a chat from jaw, as your phone sets it: pinned, archived, muted, marked unread or read; and a message starred. Each is done to its row at once and told to your other devices by WhatsApp's app state, as deleting a message for you is.

```json
{"id":16,"method":"chat.pin","params":{"chat":"351912345678@s.whatsapp.net","pinned":true}}
{"id":16,"result":{}}
{"id":18,"method":"chat.mute","params":{"chat":"120363012345678901@g.us","until":-1}}
{"id":20,"method":"star","params":{"message":"3EB0A1B2C3D4E5F6","starred":true,"chat":"351912345678@s.whatsapp.net"}}
{"id":20,"result":{}}
```

- **`chat.pin`** (`chat`, `pinned`) pins the chat now, or unpins it. Pinned chats sort first, the one pinned last on top. At most three chats are pinned, as WhatsApp's apps allow, archived ones apart; pinning a fourth is error 5, with `data` `{"limit": "pinned", "most": 3}`.
- **`chat.archive`** (`chat`, `archived`) archives the chat, which unpins it, as WhatsApp's apps do, or brings it back, which unpins it too if three other chats are pinned. An archived chat leaves `all` and comes into `archived`.
- **`chat.mute`** (`chat`, `until`): null unmutes; a time in milliseconds since the epoch mutes until then; -1, or any time from the last millisecond of 9999 on, mutes for ever, and the row's `muted_until` is then 253402300799999. A time already past is -32602, with `data` `{"param": "until"}`.
- **`chat.mark_unread`** (`chat`, `unread`): true marks the chat unread, leaving its messages as they are; false takes the mark off and reads the chat up to its newest message, as `read` does, read receipts and all. On a chat jaw holds none of the messages of, whose `unread` is the phone's count, the mark comes off at once, but the count clears only when WhatsApp sends the change back, usually within a second, so a refusal loses nothing; asking again before then sends the change again, which is harmless.
- **`star`** (`message`, `starred`, and optionally `chat`, where it is, as the message actions take it) stars a message or takes its star off. A line about the chat, or a message deleted for everyone, including while your deletion of it is on its way, can't be starred (error 4); a star can always be taken off.
- **Answers.** Each answers with nothing once its change is on disk, synced, with what tells your other devices. The row it changes comes to every window that shows it with a new `v`, so a window watched again from an earlier `seq` is sent it. Asking for what's already so changes nothing and answers the same, so an interface that lost an answer asks again.
- **Errors.** A chat or message jawd doesn't have is error 3; a community's own row, whose groups carry these, is error 4 for the four chat requests; an account that isn't linked is error 1; past the outbox's thousand, error 5, as for `send.text`.
- **Your other devices** hear of it after the chat's earlier sends, waiting while WhatsApp can't be reached, as an action on a message does; reading a chat sends its read receipts first. A change your phone makes to the same setting before jaw's reaches WhatsApp is kept but not shown, since jaw's goes after it and is what every device ends with; one made after it shows as it comes. WhatsApp may keep a setting under each of the person's JIDs your devices have named the chat by, so across those the newest stands, by the time each was made (of two made in the same millisecond, the same one each time): a change under another JID than jaw's named shows only if it was made after jaw's, and after whatever is held under the chat's other JIDs. Reading the chat on another of your devices meanwhile, or sending in it there, reads it here too, but leaves its unread mark as jaw set it, for the same reason. Even so, if your phone changes the same setting while jaw's change to it is on its way, for instance while WhatsApp can't be reached, jaw can show the other value until that setting next changes or the engine next starts, since jaw isn't always told in which order WhatsApp took the two. Until WhatsApp sends jaw's change back, `status` counts it in `stored.awaiting`.
- **Refused or out of tries.** A change WhatsApp refuses for good, or that runs out of tries, is kept in jawd's quarantine (`status`'s `stored.quarantined`), and what it changed is put back, unless you've changed it again since, in which case your later change stands, and fetched again from WhatsApp, once it can be reached, so the chat ends as your phone has it. Every interface is sent a `notice` naming the change and the error, such as "WhatsApp refused pinning a chat: …" for a refusal, "jaw couldn't confirm pinning a chat with WhatsApp: …" for one that ran out of tries, which WhatsApp may have taken, and "jaw couldn't tell your other devices of a change: …" for one jaw couldn't send at all. A refused archive of a pinned chat still unpins it, since the unpin is a change of its own: the chat ends unarchived and unpinned on every device.

## Library

Every photo, video, voice message, file and link shared, across all chats or in one, as a window jawd keeps: `library.watch`.

```json
{"id":60,"method":"library.watch","params":{"kind":"photo","chat":"120363000000000002@g.us","person":"person:4","since":1785542400000,"text":"praia","around":"top","before":0,"after":100}}
{"id":60,"result":{"watch":"w7","seq":81300,"total":6,"older":false,"counts":{"photo":6,"video":1,"voice":0,"file":0,"link":1},"rows":[{"id":"3EB0A1B2C3D4E5F6","v":81230,"sort":"1785763200000~0000000","kind":"photo","message":"3EB0A1B2C3D4E5F6","chat":"120363000000000002@g.us","chat_name":"Família Gomes","from":{"id":"person:4","name":"Inês","colour":4},"mine":false,"at":1785763200000,"caption":[{"t":"text","s":"na "},{"t":"mark","s":"praia"}],"media":{"w":4032,"h":3024,"previews":{"256":"media/previews/9d6c20c8.256.jpg","…":"…"}}}]}}
{"method":"library","params":{"watch":"w7","seq":81301,"total":7,"counts":{"photo":7,"…":"…"},"upsert":[{"id":"…","…":"…"}]}}
```

- **What's asked.** `kind` is `photo`, `video` (GIFs and video messages too), `voice`, `file` (documents and audio files), `link` or `all`. `chat` and `person` narrow to one chat and one sender: `person:` and a number, by any of their JIDs, or `me` for yours, as the account's own person is. `since` and `until` are milliseconds since the epoch, both included, either end open. `text` matches captions and file names, and a link by its message's links' hosts, URLs and previews' titles and descriptions, as `find` matches messages: accent-blind, every word typed, one of three letters or more matching the words it starts and one of one or two only itself. A message's links are matched together, so a message whose links have every word gives each of its links. Every filter can be absent.
- **A window,** newest first, paged by keyset: `around` is `top` or an item's `id`, with `before` (newer) and `after` (older) items, at most 1,000 each. Around an item it holds `before` items, the item and `after` items, and keeps that place if the item goes. `watch.move` re-anchors it with the same kind and filters. `older` says whether more items come after the window's last row; `total` is how many items of the kind match. `kind` `all` isn't paged: it holds the newest `after` items of each kind, `older` false and `total` every kind's together, so an interface can show a shelf of each and narrow to one kind to page.
- **`counts`** are how many items of each kind match the other filters, for the kind strip. For a kind across chats or in one chat they're exact, kept as items come and go; with `person`, `since`, `until` or `text`, each is counted up to 1,000, where 1,000 means at least that many, so they cost the same at any depth, as `find`'s counts do. A jawd from before links were kept leaves `link` out.
- **Rows** are items, sorted by `sort` descending: its message's time, then its place in that millisecond, and for a link 99,999 less its place among its message's links, so a message's links come first to last. An item is a message's photo, video, voice message or file, or one of the links it gives (every URL its words mark, and its link preview's); stickers, messages seen once and messages deleted for everyone are no photo, video, voice message or file, and a message deleted for everyone gives no links. Each has its own `id` (its message's ID, and `~` and the place for one of a message's links, from 0), its message's `v`, its `kind`, its `message` to go to, its `chat` and `chat_name`, `from` (absent on yours, with `mine` true), `at`, `caption` (the message's caption on one line, at most 160 characters from a little before the first word `text` matched, with the words it matched marked as `find` marks them; empty when there's none, and never a file's name), and what its kind needs:
  - `photo` and `video`: `media` as message rows have it, with `seconds` for a video;
  - `voice`: `voice`, its `seconds`;
  - `file`: `file`: its `name` (an audio file has none, and goes by its line, such as "Audio, 3:02"), `type` as shown (`PDF`, `GPX`, from its name or its MIME type), `bytes`, `pages` where known, and `path` once downloaded;
  - `link`: `link`: its `url` (made whole, with `https://` before one written without), `host`, and the preview's `title` and `description` where it had one; `caption` is its message's words.
- **Albums.** An album's photos and videos each arrive as a message of their own, so each is an item with its own message's `id`. Once jaw keeps an album as one message, its items will be that message's ID with `:` and their place.
- **Pushes.** `library` pushes upsert items as they come and change and remove those that go, with `total`, `older` and `counts` when they change, as other windows' pushes do. A deletion for everyone of yours still on its way leaves its item, as it leaves the counts, until WhatsApp has it.
- **`note`** says what a window can't hold yet and why, in words to show. This jawd holds everything and sends none; one from before links were kept said so of links, for a `link` or `all` window.
- **What's on screen.** A library window holds more items than a screen shows, and a walk through the library reads thousands, so reading one asks media for nothing. An interface names the items it has on screen with `library.shown`, again whenever they change, and jawd asks media for what they lack, as it does for every row a messages window holds: a thumbnail drawn from what WhatsApp sent, previews, and the file where it downloads by itself. An item named again soon after, or one that lacks nothing, changes nothing, and an `id` that names no item is passed over. More than 200 `items` is error 5.
- **Errors.** A `chat`, `person` or anchoring item jawd doesn't have is error 3. `since` after `until`, a `person` that's neither, `around` `newest` or `unread`, and an item to anchor `all` on are -32602, with `data.param`.

## Folders and the roster

Folders are jaw's own, kept in jaw.db and shared by every interface; WhatsApp never sees them. They're one tree, nested as deep as you like, holding chats and people, and an item can sit in any number of folders.

```json
{"id":50,"method":"folders.watch","params":{}}
{"id":50,"result":{"watch":"w5","seq":81240,"rows":[{"id":"folder:9","v":81240,"parent":null,"name":"Family","position":0,"smart":null,"unread":3,"mentions":1,"count":6,"favourite":1}],"favourites":[{"slot":1,"folder":"folder:9"},{"slot":3,"chat":"351912345678@s.whatsapp.net","name":"Ana Ribeiro"}]}}
{"method":"folders","params":{"watch":"w5","seq":81241,"upsert":[{"id":"folder:9","unread":4,"…":"…"}]}}
{"id":51,"method":"roster.watch","params":{"folder":"folder:9","around":"top","before":0,"after":200,"offline":true}}
{"id":51,"result":{"watch":"w6","seq":81241,"total":6,"rows":[{"id":"person:1","v":81241,"sort":"0~ana ribeiro\u0001000000000001","name":"Ana Ribeiro","phone":"+351912345678","about":"Lisboa","avatar":null,"presence":{"state":"online","known":true},"chat":"351912345678@s.whatsapp.net","folders":["folder:9"]}],"typing":{"351912345678@s.whatsapp.net":[{"id":"person:1","name":"Ana Ribeiro","state":"typing"}]}}}
{"id":52,"method":"folder.create","params":{"name":"Cousins","parent":"folder:9"}}
{"id":52,"result":{"folder":{"id":"folder:10","parent":"folder:9","name":"Cousins","position":0,"…":"…"}}}
{"id":53,"method":"folder.add","params":{"folder":"folder:10","item":"person:4"}}
{"id":54,"method":"favourite.set","params":{"slot":3,"target":"351912345678@s.whatsapp.net"}}
```

- **The tree.** `folders.watch` answers with every folder, built-in and smart ones too, and pushes `folders` with `upsert` and `remove` as they change; it's one window, without `around`, so `watch.move` doesn't take it. A folder row: `id`, `v`, `parent` (null at the top), `name`, `position` among its siblings of its kind, `smart` (a smart folder's rule, or null), `unread` and `mentions` (its chats' and its subfolders' chats', each chat counted once), `count` (the chats and people in it and under it, each once, a person with a one-to-one chat counting as that chat) and `favourite` (its Alt slot, or null). The built-in folders, `all`, `unread`, `groups`, `communities` and `archived`, and the smart ones (`smart:` and a number) sit at the top with yours (`folder:` and a number); an interface lays them out by `parent` and `position`. A built-in or smart folder counts the chats its list holds; a smart folder's `count` is counted up to 1,000, where 1,000 means at least that many. The counts of the built-in folders and yours are kept by jaw.db itself as messages come and go, and checked against what they count in the background, an hour apart (see `status`).
- **Favourites** are nine Alt slots, each holding a folder (a built-in one too) or a chat. `favourites` comes with the tree's result, and whole in each `folders` push that changes a slot: each a `slot`, and its `folder`, or its `chat` with the chat's `name`; empty slots are left out. `favourite.set` (`slot`, `target`) puts a folder or chat in a slot, taking it from the slot it had and putting out what the slot held; a null `target` empties the slot. A folder or chat that goes leaves its slot empty, and a chat row carries its own slot as `favourite`.
- **Smart folders** are rules: an object whose fields must all hold, any of `unread` (true: unread messages, or marked unread), `mentions` (true: unread messages that mention you), `kind` (`direct`; `group`, with announcement groups; or `community`, communities with their groups), `community` (a community's `id`: it and its groups), `muted` (true: muted now), `waiting` (seconds: a one-to-one chat whose newest message is theirs, not a call or a system message, and came at least that long ago) and `favourites` (true: a favourite, or in a folder of yours that is). As All does, they leave archived chats out. jaw starts with three: Waiting on me (`{"waiting":86400}`), Mentions me (`{"mentions":true}`) and Muted, unread (`{"muted":true,"unread":true}`). `folder.create` with a `rule`, or `folder.smart` with a `name` and a `rule`, makes one, at the top; `folder.smart` with a `folder` and a `rule` changes its rule. A rule asking for nothing, a field set false, or a field jawd doesn't know is -32602. A chat that comes to have waited long enough shows the next time its window or the tree is read for a change, or watched again.
- **A folder's chats.** `chats.watch` takes any folder's `id` as `folder`: the window holds the chats in it and in its subfolders, each once, a person filed standing for their one-to-one chat, sorted as All is. Your folders hold their chats archived or not; the built-in folders other than `archived`, and the smart ones, leave archived chats out. `unread` holds the chats with unread messages or marked unread, `groups` groups and announcement groups, and `communities` communities and their groups. Chat rows carry `folders`, the IDs of your folders the chat or its person sits in directly, and `community`, the community a group belongs to.
- **Communities in a chat list.** A community is one row, of `kind` `community`, wherever its groups would show: a chat list that holds a community, or any group of it, holds the community's row and the groups of it the list holds, each with `community` naming it and its own `sort`, which an interface shows under the community's row, the announcement group (`kind` `announcement`) first and the rest by their `sort`, so the community is one row that opens to its groups. The community's row sums up what's under it, worked out as the window is read: `children_unread` is those groups' unread messages and `children_mentions` how many of them mention you, its `last` is the newest of their messages (its own, with none under it), and it sorts just above the newest of itself and those groups, so a pinned group pins it. A community and its groups are one row of a window's `before` and `after`. A group whose community jaw doesn't have yet is a row of its own, with `community` naming it; once jaw has the community, the community's row comes into the list and the group is shown under it. A chat list's `total` counts its chats, the groups of communities it doesn't bring among them, and each community it holds only for its groups. A community the window doesn't have `open` comes as its one row, summing up the same groups, which the window leaves out, so an interface receives only what it shows. The Communities folder holds every community with its groups, and the groups whose community jaw doesn't have yet. A community's row read on its own, as `find` sends it, sums up every group of it.
- **The roster.** `roster.watch` takes `folder` as `chats.watch` does, `offline` (true by default; false leaves out people who aren't online), and a window's `around` (`top` or a `person:` id), `before` and `after`. `all` holds your contacts on WhatsApp and anyone you have a one-to-one chat with, but not you; a folder of yours, the people in it and under it, filed themselves or through their chat; `unread`, `archived` and a smart folder, the people whose one-to-one chat it holds; `groups` and `communities` hold nobody. Its rows are people: `id`, `v`, `sort`, `name`, `phone`, `about`, `avatar`, `presence`, `chat` (their one-to-one chat's `id`, if there is one) and `folders`, as chats have them. `sort` puts them online first (`0~` and the name), then by when last seen, the most recent first (`1~` and the time counted down), then those who hide it or whom WhatsApp hasn't told jawd about (`2~` and the name), each ending with the person's number so no two share one. `presence` is `state` (`online`, `seen` or `hidden`) with `at` for `seen`, and `known`, false when jawd isn't following that person now (scrolled past, or over the limits in Presence), so an interface draws the last known state faintly. Who's typing or recording is never kept in jaw.db, so it never moves a roster row: it shows from the chat's typing. A roster shows its people's one-to-one chats for it, as a chat list shows its chats: its answer carries `typing` for those where someone is, and `typing` pushes follow as they change (see Presence and typing in chats).
- **Your folders.** `folder.create` (`name`, `parent`, null or left out for the top) answers with `folder`, the new row, placed last among its siblings; `folder.rename` (`folder`, `name`); `folder.move` (`folder`, `parent`, `position`) nests a folder in another, or puts it back at the top with a null `parent`, at `position` among its new siblings (last without it); `folder.delete` (`folder`) puts its subfolders where it was, in their order, and its chats and people stay wherever else they are. `folder.add` and `folder.remove` (`folder`, `item`: a chat's `id` or a `person:` id) file and unfile one item; filing someone's one-to-one chat files them. Asking for what's already so changes nothing and pushes nothing. Each is on disk, synced, before it's answered, and pushes the folder rows it changes, and the chat and roster rows whose `folders` change, to every interface. A folder's ID is never given to another, even once it's deleted; when two people turn out to be one (a number and a LID), what either was filed in and their favourite slot go with the one kept.
- **Errors.** A name that's empty or only spaces, longer than 256 characters or on more than one line is -32602; one a sibling already has is error 4, as is deleting a folder whose subfolder would then sit beside one of its name. Renaming, moving or deleting a built-in folder, moving a smart one, filing into either, moving a folder into itself or one of its own subfolders, putting a folder into anything but a folder of yours, or making a smart folder anywhere but the top, is error 4. A folder, chat or person jawd doesn't have is error 3.

## Settings

Each frontend's settings, kept by jawd in jaw.db in a domain of its own, named by the `client` its hello gives, so a terminal and a Mac app on one machine keep separate preferences. A connection reads its own domain's settings in hello's answer, changes them with `settings.set` and is pushed them when they change. The terminal's domain, `jaw`, is the only one jawd keeps so far; `jaw settings` and `jaw status` share it, since they say hello as `jaw` too.

```json
{"id":20,"method":"settings.set","params":{"theme":"gruvbox"}}
{"id":20,"result":{"settings":{"theme":"gruvbox","messages":"bubbles"}}}
{"method":"settings","params":{"theme":"gruvbox","messages":"bubbles"}}
{"id":21,"method":"themes"}
{"id":21,"result":{"themes":[{"id":"catppuccin","name":"Catppuccin","mode":"dark","colors":{"accent":"#89b4fa","background":"#1e1e2e","…":"…"}}]}}
```

- **In the terminal's domain,** `theme` is one of Omarchy's themes by its ID, as `themes` lists them; `tokyo-night` by default. `messages` is where your messages sit: `bubbles`, in a column from a third of the way across, by default, or `irc`, on the left under "You" as everyone's are.
- **`settings.set`** changes the connection's own domain; a connection whose client has no domain is error 3, with `data` `{"client": "<client>"}`. It takes one setting or more and checks every one before changing any: a setting jawd doesn't know is -32602 with `data` `{"param": "<setting>"}`, a value it doesn't take or of the wrong type is -32602 with `data` `{"param": "<setting>", "allowed": [...]}`, and no settings at all is -32602. It answers with `settings`, all of them as they now are, once the change is on disk, synced; a setting asked with the value jaw.db already holds isn't written again, and an ask of nothing else is answered at once. While jaw.db can't be written for a reason that passes, such as a full disk, the answer waits as every write's does; a change that can't be kept at all is -32603 saying why, and nothing changes.
- **The `settings` push** goes to every connection of the same domain that has said hello, the one that asked included, whenever that domain's settings change, and carries all of them. A connection always ends on the latest: a change made while it's slow to read may be skipped, never the last, unless it has closed its writing side. A connection that has closed its writing side still gets a push that waits, and jawd closes it once it has every reply, so a change made while that push waited, or after, may not reach it. Connections of other domains hear nothing of it.
- **A push and an answer:** on the connection that asked, the `settings` push a change brings can come before the answer to the `settings.set` that made it, but a later change's push never does, since a change is answered before the next one is published. The answer carries the settings as its change left them, and what jawd keeps is what the latest push says.
- **`themes`** answers with every theme, ordered by ID: its `id`, a `name` to show, its `mode` (`dark` or `light`) and `colors`, Omarchy's 25 colours by their keys as lowercase `#rrggbb`; where a theme leaves them out, `orange` is its `yellow` and `brown` its `orange` mixed half with black, as Omarchy resolves them. They're `protocol/themes/`, each theme's colors.toml as Omarchy wrote it (MIT, `docs/licenses/omarchy.txt`), which an interface in any language can read as well.
- **`status`** says each domain's settings in effect as `settings`, by the domain's name, and as `settings_ignored` each setting whose value in jaw.db this jawd doesn't take, with its `domain`, its `key` and that `value`; such a setting has its default instead.

## Focus and reading

```json
{"id":14,"method":"focus","params":{"focused":false}}
{"id":14,"result":{}}
```

- `focus` says whether the interface is in front now: a terminal sends it as focus comes in and goes out, and `hello`'s `focus` says how a connection starts. It's per connection, and you count as looking while any connection's focus is true.
- jawd keeps which chats each connection is reading: those its `messages.watch` windows show, until it unwatches them, and the chat it last said it opened with `chat.opened`, until it opens another or no window of its shows that one.
- A chat a connection in front is reading gets no notifications, and presence follows focus (see Presence).

## Layouts

jawd keeps each interface's layout, by the `client` name it gives in hello (`jaw` for the terminal), so it opens with the panes it was left with, and so another interface can open it: the Mac app can open the terminal's, and the terminal the Mac's. A layout is in one of two formats: a tree of split panes (`format` 1, the terminal's), or a book of named layouts on a grid (`format` 2, the Mac app's).

```json
{"id":31,"method":"layout.set","params":{"v":1792000000123,"layout":{"format":1,"focused":1,"tree":{"split":{"how":"across","first":500,"a":{"chat":"351912345678@s.whatsapp.net"},"b":{"chat":"100000000002@lid"}}}}}}
{"id":31,"result":{"interface":"jaw","v":1792000000123,"layout":{"format":1,"focused":1,"tree":{"split":{"how":"across","first":500,"a":{"chat":"351912345678@s.whatsapp.net"},"b":{"chat":"351912222222@s.whatsapp.net"}}}},"kept":true}}
```

- **A tree** (format 1): a pane is `{"chat": JID}`, a split is `{"split": {"how", "first", "a", "b"}}`, with `how` `across` (the sides beside each other, `a` on the left) or `down` (`a` on top) and `first` the first side's share in thousandths, 1 to 999. `focused` is the pane with the keys, counted from 0 in the tree's order, `a` before `b`.
- **A book** (format 2) is the Mac app's `LayoutBook`, key for key, with `format` where the app keeps its own version (its `v`):

  ```json
  {"id":32,"method":"layout.set","params":{"v":1792000000789,"layout":{"format":2,"current":"Work","unnamed":{"cols":[0.0,0.5,1.0],"rows":[0.0,1.0],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":1,"chat":"351912345678@s.whatsapp.net"},{"id":2,"c0":1,"c1":2,"r0":0,"r1":1}]},"unnamedFocus":1,"layouts":[{"name":"Work","grid":{"cols":[0.0,0.6,1.0],"rows":[0.0,0.5,1.0],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":2,"chat":"120363012345678901@g.us"},{"id":2,"c0":1,"c1":2,"r0":0,"r1":1,"chat":"100000000002@lid"},{"id":3,"c0":1,"c1":2,"r0":1,"r1":2,"chat":"351919999999@s.whatsapp.net"}]},"edited":{"cols":[0.0,0.6,1.0],"rows":[0.0,1.0],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":1,"chat":"120363012345678901@g.us"},{"id":2,"c0":1,"c1":2,"r0":0,"r1":1,"chat":"100000000002@lid"}]},"focus":2}]}}}
  {"id":32,"result":{"interface":"jaw-mac","v":1792000000789,"layout":{"format":2,"current":"Work","unnamed":{"cols":[0.0,0.5,1.0],"rows":[0.0,1.0],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":1,"chat":"351912345678@s.whatsapp.net"},{"id":2,"c0":1,"c1":2,"r0":0,"r1":1}]},"unnamedFocus":1,"layouts":[{"name":"Work","grid":{"cols":[0.0,0.6,1.0],"rows":[0.0,0.5,1.0],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":2,"chat":"120363012345678901@g.us"},{"id":2,"c0":1,"c1":2,"r0":0,"r1":1,"chat":"351912222222@s.whatsapp.net"},{"id":3,"c0":1,"c1":2,"r0":1,"r1":2}]},"edited":{"cols":[0.0,0.6,1.0],"rows":[0.0,1.0],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":1,"chat":"120363012345678901@g.us"},{"id":2,"c0":1,"c1":2,"r0":0,"r1":1,"chat":"351912222222@s.whatsapp.net"}]},"focus":2}]},"kept":true,"dropped":["351919999999@s.whatsapp.net"]}}
  ```

  - `layouts` are the named layouts, in the order they were made, each with its `name` (1 to 64 bytes, no other layout's), its saved shape (`grid`), the shape it was edited to until it's saved or reverted (`edited`), and `focus`, the id of its slot with the keys in the shape on screen. `current` names the layout on screen; without it, it's the unnamed layout, `unnamed`, what's on screen before any is saved, with its `unnamedFocus`.
  - A shape is slots on a grid's lines: `cols` and `rows` are the lines as fractions of the width and the height, from 0 to 1, each past the one before; each slot spans them by index, `c0` to `c1` and `r0` to `r1`, with an `id` no other slot of the shape has, and its `chat`, a JID, or none while it waits for one. The slots cover the grid with no holes and no overlaps, every line inside it is some slot's edge, and a chat is in one slot of a shape at most. A focus on no slot is kept as it's given, since the app then goes to the first slot.
  - jawd refuses a key it doesn't know, with -32602 and `param` `layout`, rather than keep the book without it. It answers the lines as the very numbers it was given, not rounded.
- **`layout.set`** keeps the connection's interface's layout with a synced commit before it answers. `v` is its version, which the interface gives: more than any version it has been told of, such as the time in milliseconds. Only a newer version replaces what's kept, whatever either's format, so asking again, or late, changes nothing: the answer has `kept`, whether the layout asked for is the one kept, and always the layout kept, which is newer when `kept` is false, in the format of the layout asked for. A format other than 1 or 2 is -32602 with `data` `{"param": "layout.format", "allowed": [1, 2]}`; a chat in two panes, or two slots of a shape, a pane or slot that isn't a JID, a share or `focused` out of range, a shape that isn't one as above, a name empty, too long or another's, a `current` the book doesn't name, or a `v` of 0 is -32602 with the `param` (`layout.tree`, `layout.focused`, `layout.unnamed`, `layout.layouts`, `layout.current` or `layout.v`).
- **Limits**, each error 5: a tree of 16 panes and 16 KiB of JSON; a book of ten layouts, the unnamed one and nine named, the ten ⌃⌘0 to ⌃⌘9 reach, with 12 slots in each shape and 64 KiB of JSON (one at every other limit, each shape edited and its chats' JIDs 64 bytes long, takes about 30 KiB); and 16 interfaces with a layout kept, an interface that has one always able to change it. `status`'s `layouts` has how many are `kept`, of the `most`, the `largest_bytes` of a layout that isn't a book, of `bytes_most`, the `books` among those kept and the `largest_book_bytes`, of `book_bytes_most`, and how many were `refused` for a limit.
- **`layout.get`** answers with what's kept for the connection's interface, or for `interface`, in the format it was kept in, or in `format` when it asks for 1 or 2. Panes and slots name chats by JID, and jawd answers with each chat as it knows it now: the chat a JID was merged into, or the same person's chat under their other JID (phone number or LID), never a chat being merged away. A pane whose chat is gone, or that shows a chat a pane before it shows, is left out, and its split closes; if it had the keys, the pane before it takes them, or the first. A slot whose chat is gone, or that shows a chat a slot of the same shape before it shows, top to bottom and then left to right, is emptied, keeping its place, its id and the focus. Their JIDs are in `dropped`, each once. With nothing kept, or nothing left, there's no `layout` and `v` is what's kept, 0 for none.
- **Each format read as the other**, so the Mac app opens the terminal's panes and the terminal could open the Mac's layout on screen:

  ```json
  {"id":33,"method":"layout.get","params":{"interface":"jaw","format":2}}
  {"id":33,"result":{"interface":"jaw","v":1792000000123,"layout":{"format":2,"unnamed":{"cols":[0.0,0.5,1.0],"rows":[0.0,1.0],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":1,"chat":"351912345678@s.whatsapp.net"},{"id":2,"c0":1,"c1":2,"r0":0,"r1":1,"chat":"120363012345678901@g.us"}]},"unnamedFocus":2,"layouts":[]},"dropped":["351919999999@s.whatsapp.net"]}}
  ```

  - **A tree as a grid,** as the Mac app's `PaneGrid.from` reads one: the tree, its panes as format 1 reads them, is laid out in the unit square, a split across at `first` thousandths putting a column line at that share of what it splits and a split down a row line; the grid's `cols` and `rows` are the panes' edges, each once, in order; and each pane is a slot spanning the lines at its edges, with the id of its place in the tree's order, from 1. The book has that grid as its unnamed layout, the focused pane's slot as `unnamedFocus`, and nothing else. A pane whose share rounds to nothing, which only splits nested far past any screen's pixels can make, is left out, its JID in `dropped`.
  - **A grid as a tree:** the book's layout on screen, the one `current` names in its edited shape if it has one, or the unnamed one, is cut in two along the first column line from the left that no slot crosses, or else the first such row line from the top, and each part again, until each is one slot: each cut is a split across or down, its `first` the thousandths at which its line falls, rounded, 1 to 999. The slots are the panes in the order the cuts give, read as format 1 reads panes, an empty slot left out as a pane whose chat is gone is; the focused slot's pane has the keys, or the first pane when the focus is on no slot. A grid no line cuts, such as a pinwheel, isn't a tree, and there's no `layout`.
  - Read as the other format and back, a tree comes back as the same panes in the same places, and a grid that's a tree as the same slots, each edge to within a few thousandths of the window rather than exactly: each share read from a grid is rounded to the thousandth of what its split divides, and the roundings add up through nested splits.
- **Both requests** wait their turn behind the requests sent before them (see The connection), and each is carried out under the `client` name of the hello the connection had said when it sent it.
- **The `layout` push** goes to the interface's other connections when one keeps a newer layout, in the format it was kept in, never to the one that kept it, and to no other interface. A push that finds the interface not reading waits for it, as a reply does, and is never dropped; meanwhile only the newest layout is kept for that connection, so one slow to read may miss a layout, but never the last, unless it has closed its writing side and is owed no reply. A connection that has closed its writing side still gets a push that waits, and jawd closes it once it has every reply, whatever is kept after; owed no reply, it gets the push that waits and is then closed, and isn't told newer layouts.
- **A push and an answer** can come in either order: a `layout` push can come before the answer to a `layout.get` or `layout.set` read before it, so an interface goes by `v`, as it does for a reply's `row`, and keeps the newest layout it has been told of.

## Notifications

jawd sends them itself, so they come with no interface open: Notification Center on macOS, `notify-send` on Linux, and none where neither is. `jawd --no-notify` or `JAW_NOTIFY=off` turns them off, and `status` says where they go.

- A new message from someone else, arriving since jawd started, notifies: the chat's name, how many are unread when it's more than one, and the message on one line (who wrote it, in a group), or its kind's word for media.
- A burst in one chat is one notification; what comes in that chat in the next 10 seconds goes as one more when they've passed. More than three chats at once are one summary.
- Nothing goes while catching up. When a catch-up ends, the chats whose newest message it brought are one notification, that chat's own or a summary such as "12 new messages in 4 chats", however long ago they were sent; so what came while jawd was off is one summary once it has caught up after starting.
- Muted and archived chats, your own messages, history, and the chat an interface in front is reading stay quiet.
- A reminder that comes due notifies in a muted or archived chat too, and a scheduled message held past its hour says so, as Scheduled messages and reminders (proposed) has them.

## Presence

jawd tells WhatsApp you're online once an interface's `focus` has been true for half a second, and away 30 seconds after the last one loses it (or its connection ends), so your phone keeps its notifications while you're not looking and switching windows doesn't flicker. It says so again each time the connection to WhatsApp comes back. WhatsApp only sends others' presence while you're online.

It follows the presence of the people in open one-to-one chats and roster windows, while some interface has focus, at most 20 new people a minute and 200 per connection to WhatsApp: open chats' people first, then favourites, then the rosters' in their order; subscribing to everyone is what presence trackers do. Past the limit, rows show their last known state, with `known` false. A new connection to WhatsApp starts the count again: who was online before it no longer shows so, and last seen times stay. A keepalive timeout isn't a new connection: while `connection` says `connecting` with the connection still open (see Other pushes), nobody's presence is `known` and each shows their last known state, and once the pings get through again the people it followed are followed again at once, without being asked for again, and the count goes on. `status` says how many people are followed and how many wait.

The about texts of everyone the roster can show are asked of WhatsApp through the engine, 50 people at a time and a batch every 10 seconds while it's connected, and each again once it's a day old; someone new is asked about within a minute.

Others' typing in every chat, and the presence of the person in a one-to-one chat, come as the `typing` and `presence` pushes, below.

## Presence and typing in chats

Every chat, groups included, shows who's typing or recording in it now, in its chat-list row, its header and the status line, and a one-to-one chat's header and status line show the person's presence. Both come as the `typing` and `presence` pushes, with each watch's answer saying how things are now, as the owner decided on 27 October (jaw's design, "Presence and typing in chats"). Both are sent: typing from protocol 2, and presence from M6's folders on, since it comes from the rows M6 writes. Chat rows' `typing`, always empty before, went with typing's coming, and a field going raises `protocol` by one (see Versioning), so `protocol` is 2; presence's fields and push are new, which leaves it at 2.

Carrying them on chat rows instead, so that a watch again from `since` brought them up to date, was turned down: every start, pause and fade of typing would have been a write to jaw.db, on the one writer the inbox's commits share.

```json
{"id":3,"result":{"watch":"w2","seq":81233,"…":"…","typing":{"351912345678@s.whatsapp.net":[{"id":"person:12","name":"Ana","state":"typing"}]},"presence":{"person":"person:12","state":"online","known":true}}}
{"method":"typing","params":{"chat":"120363012345678901@g.us","who":[{"id":"person:12","name":"Ana","state":"typing"},{"id":"person:4","name":"Rui","state":"recording"}]}}
{"method":"typing","params":{"chat":"120363012345678901@g.us","who":[]}}
{"method":"presence","params":{"chat":"351912345678@s.whatsapp.net","person":"person:12","state":"seen","at":1792145000000,"known":true}}
```

- **Where it comes from.** Typing is WhatsApp's chat presence. WhatsApp says it for every chat you're in, groups included, while you're online (while some interface has focus, and for 30 seconds after), with the chat, who (by one JID, often with their other, phone number or LID, beside it, as it does for messages), and `composing`, `recording` or `paused`. Online and last seen come only for the people jawd follows (see Presence), within its limits and only while an interface has focus. Nobody new is followed for this: the people of open one-to-one chats are followed first.
- **Who's typing** is a list, oldest first, each with `id` (`person:` and a number), `name` (as a message from them would show it) and `state` (`typing` or `recording`). Your own other devices aren't shown typing, nor is anyone jaw has no person for, nor typing in a chat jaw doesn't have. An interface draws them in their messages' colour, the `colour` their message rows' `from` carries for the same `id`, which is their person's number among the eight.
- **When it ends.** Typing fades 25 seconds after it was last said, as the phones let it fade. `paused` ends it at once, and so does a message from them in that chat, whichever of their JIDs it comes by, as on the phones. All of it ends at once when jawd tells WhatsApp you're away or WhatsApp can't be reached, a keepalive timeout's `connecting` among them, since nothing more would come; it's heard again once WhatsApp has been told you're online on the connection that's back. jawd starts with nobody typing. jawd says when typing ends, so an interface runs no timer for it.
- **Presence** is the roster's: `state` `online`, `seen` with `at`, or `hidden` (they hide it, or WhatsApp hasn't said), and `known`. `known` is false while jawd isn't following them: nobody's looking, WhatsApp can't be reached, or they're past the limits. The interface then draws the last known state faintly, as the roster does. The roster's other two states, typing and recording, show from the chat's typing, so the two never disagree. Typing is never faint, since it shows only while jawd hears it.
- **Watching.** The answers to `chats.watch`, `messages.watch`, `roster.watch` and `watch.move` carry `typing`: who's typing in each chat of the window where someone is. A chat list shows its chats, a conversation its chat, and a roster its people's one-to-one chats; the folder tree shows none. It replaces what the interface held for every chat in the window, so a chat left out has nobody typing. The answer to a `messages.watch` or `watch.move` of a one-to-one chat also carries its person's `presence`.
- **After the answer,** `typing` comes for a chat the connection's windows show when its typing changes, and just after the window's push that brings a chat in with someone typing (in a roster, the push that brings in their person). `presence` comes when the presence of the person of a one-to-one chat the connection has open changes: their state, when they were last seen, or whether jawd follows them. It's read with the conversation's window, so it's paced as the window's updates are and waits with them while the interface isn't reading, the newest replacing what wasn't sent.
- **What an interface keeps:**
  - typing for the chats its windows show, a roster's people's one-to-one chats among them, forgotten as a chat leaves them;
  - presence for the one-to-one chats it has open, forgotten at `unwatch`;
  - nothing of either once its connection ends.

  Neither has a `v`. A watch again from `since`, after `resync` or after a reconnect says how things are now, whatever `since` is, so nothing is left over.
- **Nothing written.** Who's typing in a chat lives in jawd's memory only: it's never written to jaw.db, moves no `v` or `seq`, and never reaches the writer, so a message's ack costs what it did. Presence is what M6 keeps on people's rows for the roster's order (online, last seen, and whether jawd follows them), sent when it changes. Of the rows windows hold, only the person's roster row takes a new `v` for it, so someone coming online or going away sends no message or chat row again. Telling interfaces either writes nothing more: an answer's `presence` and the `presence` push read the person's row, and a keepalive timeout and its restore write only whom jawd follows.
- **Busy groups.** jawd takes the engine's typing without waiting, and drops and counts what comes faster than it can take. Each chat's typing reaches an interface at most every 50 ms: the newest replaces what wasn't sent yet, and the first change after a quiet spell goes at once, as window updates do. A connection holds at most one unsent `typing` for each chat it shows, which waits with its window updates while the interface isn't reading (see Pushes for windows), so a slow interface holds nothing up and gets the newest once it reads. `jawd status` gains `typing`: the `chats` with someone typing now; how many changes WhatsApp sent (`heard`), and of them how many were `dropped` for coming too fast; those `waiting` to be taken now, of the `waiting_most` that can (1,024); the `typing` pushes `sent`; and the changes `merged` into a newer one before they went. A message that comes while what waits is full ends everyone's typing, since whose it ended can't be known.
- **The Mac app** keeps typing by chat and presence by open chat. From them it draws its chat list's "typing…", its header's who's typing and online or last seen, and its status line's left half, for any open chat, whether a chat list shows it or not.

## Scheduled messages and reminders (proposed)

Proposed with jawd's M7, for jawd to build, as jaw's design's "Scheduled messages and reminders" and [its own design](../docs/plans/2026-10-27-jaw-scheduled-messages-and-reminders-design.md) have it, and as the Mac design's proposal P9 asks: everything jaw sends can be given a time instead, once or repeating, and any message a reminder. jawd keeps both, as jaw's own, and does the rest: at its time a scheduled message becomes an ordinary message of yours, sent as its request would have sent it then, and a reminder comes due, notifies and marks its chat unread. WhatsApp sees only ordinary messages, so whoever gets them needs nothing new. Until jawd serves them, none of this is answered.

```json
{"id":70,"method":"send.text","params":{"chat":"351912345678@s.whatsapp.net","text":"bom dia ☀️","repeat":{"every":"weekday","time":"09:00","zone":"Europe/Lisbon"},"key":"jaw-7a1c"}}
{"id":70,"result":{"scheduled":{"id":"scheduled:12","v":81402,"sort":"1~1793350800000~000000000012","chat":"351912345678@s.whatsapp.net","chat_name":"Ana Ribeiro","kind":"text","spans":[{"t":"text","s":"bom dia ☀️"}],"reply":null,"at":1793350800000,"repeat":{"every":"weekday","time":"09:00","zone":"Europe/Lisbon","says":"every weekday at 09:00"},"sent":0,"state":"waiting"}}}
{"id":71,"method":"when.parse","params":{"text":"fri 18:30"}}
{"id":71,"result":{"at":1793385000000,"says":"Friday 30 October at 18:30"}}
{"id":72,"method":"scheduled.watch","params":{"around":"top","before":0,"after":50}}
{"id":72,"result":{"watch":"w9","seq":81402,"total":2,"held":1,"rows":[{"id":"scheduled:9","v":81390,"sort":"0~1793264400000~000000000009","chat":"120363012345678901@g.us","chat_name":"Lisbon Climbing","kind":"text","spans":[{"t":"text","s":"saio agora"}],"reply":null,"at":1793264400000,"repeat":null,"sent":0,"state":"held","why":"late","since":1793264400000},{"id":"scheduled:12","…":"…"}]}}
{"id":73,"method":"remind","params":{"message":"3EB0A1B2C3D4E5F6","chat":"351912345678@s.whatsapp.net","at":1793350800000}}
{"id":73,"result":{"reminder":{"id":"reminder:4","v":81410,"sort":"1~1793350800000~000000000004","at":1793350800000,"due":false,"message":"3EB0A1B2C3D4E5F6","chat":"351912345678@s.whatsapp.net","chat_name":"Ana Ribeiro","from":{"id":"person:12","name":"Ana","colour":4},"mine":false,"summary":"did you get the photos from Sunday?","message_at":1792145060000}}}
{"method":"reminders","params":{"watch":"w10","seq":81530,"due":1,"upsert":[{"id":"reminder:4","v":81530,"sort":"0~1793350800000~000000000004","due":true,"…":"…"}]}}
```

### Sending later

- **Every send takes `at`:** `send.text`, `send.poll`, `send.contact` and `forward` as jawd serves them now, and `send.media`, `voice.send` and Composing's `send.event`, `send.location` and `send.gif` as each is served. With `at` (milliseconds since the epoch), jawd keeps what the request would have sent, to send at that time, and answers `scheduled`, the item's row, in place of `message`. `at` must be at least a minute ahead and at most 366 days; otherwise it's -32602 with `data` `{"param": "at"}`.
- **`send.text`, `send.poll`, `send.media` and `voice.send` take `repeat` too,** a rule (below). With it, `at` is its first time, and can be left out for the rule's first time from a minute ahead.
- **The request is checked as a send is,** when it's scheduled: a chat jawd doesn't have is error 3, text that's only spaces -32602 and too long error 5, a `reply` jawd doesn't have error 3 and one deleted error 4, mentions as Message actions says, a poll's question and options as `send.poll` says, and each forwarded message as `forward` does. Media is planned and transcoded when it's scheduled, so a file that can't go says so then, and its files are kept in jaw's folder until its last time; a voice message is the stopped recording, and a GIF is fetched when it's scheduled, since `gif.search`'s results last an hour.
- **`key`,** as `send.text` takes it: asking again with the same key within a day answers with the same item as it is now and keeps nothing new; if it has gone by then, it's error 4 as a change would be (below), and error 3 once it's been cancelled.
- **A `forward`** to several chats is an item for each, answered as a list of rows in `scheduled`, each item forwarding every message named, in order. Forwards take `at` and not `repeat`.
- **Scheduling text clears the chat's draft,** as sending it does, since what was typed has gone where it was meant to go; a poll or a contact card leaves what's typed, as sending one does (an interface that made it from what's typed clears the draft itself, as when sending it), and a forward leaves the chats' drafts, as forwarding does.

### The rule

```json
{"every":"week","days":[1,3],"time":"07:30","zone":"Europe/Lisbon","times":10,"says":"every Monday and Wednesday at 07:30, 10 times","left":10}
```

- `every`: `day`, `weekday` (Monday to Friday), `week`, `month` or `year`; with `week`, `days` (1 to 7, Monday first); with `month`, `day` (1 to 31, or -1 for the last day); with `year`, `date` (`MM-DD`). Each takes the first time's when it's left out.
- `time` (`HH:MM`, local) and `zone` (an IANA time zone, jawd's own when it's left out, which is the Mac's): each next time is reckoned in that zone with the system's time zone database, so 09:00 stays 09:00 across a change of hour. A time the change skips goes at the first minute after it, and one the change repeats goes once. A month without the day goes on its last day, and 29 February on the 28th outside leap years.
- At most one of `until` (`YYYY-MM-DD`, the last day it can go, in its zone) and `times` (how many times it goes, 1 to 1,000).
- Nothing repeats more than daily, and a repeat is one chat's: there's no rule for every hour, nor for several chats.
- jawd adds `says`, the rule in words ("every weekday at 09:00"), to every rule it answers, and `left` for one with `times`: how many times it still goes.
- A rule jawd can't take is -32602 with `data` `{"param": "repeat"}` and `message` saying why.

### Reading a typed time

- **`when.parse`** (`text`, and `repeat` true for a repeat) reads what's typed in jawd's time zone, as every interface shows it: "18:30", "6:30pm", "tomorrow 9", "fri 18:30", "30 oct 18:30", "2026-10-30 18:30", "in 2 hours", "this evening", "next monday 9am"; and with `repeat`, "every day 8:00", "every weekday 9am", "every monday and wednesday 7:30", "every month on the 1st at 9", "every month on the last day at 18:00", "every year on 30 oct at 9", each with "until 1 dec" or "10 times" after it if wanted. It answers `at`, the first time, and `says`, the time in words ("Friday 30 October at 18:30"), and with `repeat`, `repeat`, the rule with its `says`. A time of day alone is its next one; a day alone is at 09:00.
- What it can't read is -32602, its `message` saying so in words to show ("jaw can't read that time"), and so is a time less than a minute ahead or more than 366 days, as a send would refuse it.
- It needs nothing from the disk, so it's answered at once, as `hello` is, without waiting for the connection's other requests: an interface can send one at each keystroke, and keeps the answer to its latest.

### Scheduled rows and their window

- **`scheduled.watch`** (optionally `chat`, with `around`, `before` and `after`, and `since` and `have` when watching again) is a window of what's scheduled, every chat's or one's: held items first, longest held first, then the rest by their next time. `around` is `top`, the place above the first row, which takes `after`, or an item's `id`. It's answered at once, as watches are, and pushes `scheduled`, with `upsert`, `remove`, `total` and `held` as they change. `held` is how many of the window's list are held, so an interface can say so with a window of no rows (`after` 0).
- **A scheduled row:**

| Field | |
| --- | --- |
| `id` | `scheduled:` and a number, never used again |
| `v`, `sort` | As every row's: `sort` is `0~` and when it should have gone for a held item, `1~` and its next time for the rest, then its number |
| `chat`, `chat_name` | Where it goes, as the chat list names it |
| `kind` | `text`, `poll`, `forward`, and as their sends are served `media`, `voice`, `event`, `contact`, `location` and `gif` |
| What it sends | As a message row shows it: `spans` with its mentions marked, `reply`'s quote, a poll's `poll` (`question`, `options` by `name`, `max`), a forward's `forward` (each message's `id`, `from` and one-line `summary`), and `media` with its previews, `voice` with its length and waveform, an event's, a contact's, a place's or a GIF's fields as theirs are served |
| `at` | Its next time; for a held item that goes once, when it should have gone |
| `repeat` | The rule, with `says` and `left`, or null when it goes once |
| `sent` | How many times a repeat has gone |
| `state` | `waiting`, or `held` with `why`: `late` (not sent within the hour of its time), `missed` (a repeat that missed more than one of its times, `missed` saying how many), `quote_gone` (what it replies to was deleted for everyone), `forward_gone` (what it forwards was deleted) or `outbox_full` (the outbox held its thousand until the hour had passed); and `since`, when what it holds should have gone |

- **Chat rows** gain `scheduled`, how many of the chat's items are waiting or held, and `next_scheduled`, the soonest next time of those waiting, null when none is, so the chat list can mark a chat and a conversation knows whether to watch its scheduled messages. They're counted from the items as the row is read, so they can never disagree with them.

### Changing what's waiting

- **`scheduled.edit`** (`scheduled`, and any of `text`, `reply`, `mentions` for text; `question`, `options`, `max` for a poll; `captions`, one for each file, for media; `at`; `repeat`, null to send it once) changes what's waiting, checked as scheduling checks it. Files, a voice message and what's forwarded can't be changed, -32602 with the `param`; scheduling them again can. A held item given a new `at` waits again. It answers `scheduled`, the row.
- **`scheduled.send_now`** (`scheduled`) sends it now, as at its time, whether or not WhatsApp can be reached, as `send.text` does: its message waits in the outbox from there. It answers `message` and `row` as `send.text` does, or `messages` for a forward. A held item sends what it holds; a waiting repeat sends its next time's now and goes on from the one after.
- **`scheduled.skip`** (`scheduled`) skips a repeat's next time, or what a held repeat holds, and answers `scheduled`, the row, or null when that was its last; error 4 for an item that goes once.
- **`scheduled.cancel`** (`scheduled`) takes it away, every repeat with it, and its files once a synced commit no longer points to them, and answers with nothing.
- **Each answers once its change is on disk, synced,** and every window showing what it changes gets its rows; asking again changes nothing more. An item jawd doesn't have is error 3, which an interface asking again after a lost answer to `scheduled.cancel` takes as done.
- **Changes race nothing.** They and the sends at their times go through jaw's one writer, so whichever commits first wins and the other is told. A change that reaches jawd after the item has gone, or after one of its times has gone since the version `v` it names, is error 4 with `data` `{"message": id}`, the message it became (a forward's first, with `messages` naming each), so an interface can offer to edit that within WhatsApp's 15 minutes, or delete it. `v` is the row's version the interface acted on; without it, a change to a repeat applies to the times still to come.

### At its time

- **One timer,** on the wall clock, waits for the soonest time of every item and reminder, so neither the Mac's sleep nor a change of the clock makes one late, and nothing wakes jawd between times. An item due while jawd runs is handed to the outbox within a second of its time, once WhatsApp can be reached.
- **An ordinary send.** At its time, with WhatsApp reachable, one synced commit writes the message row and its outbox entry as the send would have, with a new ID and, for a poll, a new secret, and in the same commit moves the item to its next time or removes it. From there it's sent, ticked, retried and failed as any message is. It reads nothing in its chat, since you weren't there, and leaves the chat's draft alone; the chat's disappearing timer is the one it has then, and a link's preview is fetched then. A crash before that commit leaves the item due, to go when jawd starts again; after it, the outbox has it, under the one ID it ever goes with.
- **The hour's grace.** An item goes once its time has come and WhatsApp can be reached, within the hour of its time. Past the hour it's held (`why` `late`), jawd notifies ("Not sent: your message to Ana Ribeiro at 09:00"), and it waits for `scheduled.send_now`, a new `at`, `scheduled.skip` or `scheduled.cancel`. A repeat that missed several of its times holds the latest (`why` `missed`, with how many), and its rule goes on from now: when its next time goes, what it held is let go of, unsent, since the same message has just gone.
- **Held for a reason.** A message whose quote has been deleted for everyone by its time, or a forward whose message is gone, is held (`quote_gone`, `forward_gone`) rather than sent without what it was written around. One due while the outbox holds its thousand waits for room as if WhatsApp couldn't be reached, and is held (`outbox_full`) past the hour.
- **Kept awake.** While an item is due in the next 30 minutes and the Mac is awake, jawd keeps it from idling to sleep (a power assertion on macOS, a logind inhibitor on Linux), and lets go once the item has gone to the outbox or is held. The display can still sleep, and closing the lid still sleeps the Mac.
- **A group you've left by then, or a number no longer on WhatsApp,** fails as a send to it does, and the failed message can be retried.
- **A chat merged into another,** a person's number and LID becoming one chat, takes its items and reminders with it.

### Reminders

- **`remind`** (`message`, `at`, and optionally `chat`, as the actions on a message name theirs) sets the message's reminder for `at`, or changes it, a snooze being a new `at`, or with `at` null takes it away, which is how it's marked done. `at` must be at least a minute ahead and at most 366 days, -32602 with `param` `at` otherwise. A message has one reminder at most, anyone's message, and asking again changes nothing more. It answers once its change is on disk, synced, with `reminder`, the row, or null once it's taken away. A message jawd doesn't have, or one deleted for you, is error 3. A reminder on a chat, from an interface's chat menu, is `remind` on the chat's newest message, which the chat's row has.
- **When one comes due,** one synced commit marks it due and marks its chat unread, with the app-state patch that tells your phone, through the outbox, as a chat marked unread from jaw is; a chat marked already changes nothing. jawd then notifies, "Reminder · Ana Ribeiro" and the message's line, in a muted or archived chat too, since you asked, unless an interface in front is showing the chat. It stays due until `remind` takes it away or gives it a new time; reading the chat leaves it due. Reminders that came due while jawd wasn't running are marked due once it starts, however late, and notify as one summary when there are several ("3 reminders came due").
- **`reminders.watch`** (`around`, `before`, `after`, and `since` and `have` when watching again) is a window of every reminder: those due first, the longest due at the top, then the rest by their time. It's answered at once and pushes `reminders`, with `upsert`, `remove`, `total` and `due`, how many of them are due, as they change.
- **A reminder row:** `id` (`reminder:` and a number, never used again), `v`, `sort` (`0~` and its time for one due, `1~` and its time for the rest, then its number), `at`, `due`, `message` (its ID, null once it's gone), `chat` and `chat_name`, and the message as its chat shows it now: `from` (absent on yours) and `mine`, its one-line `summary`, and `message_at`, when it was sent. A reminder keeps no copy of its message: one deleted for everyone says so as its row does, and one that disappeared has `gone` true and no `summary`. Deleting a message for you takes its reminder with it.
- **Message rows** gain `reminder`, `at` and `due`, or null, so a conversation draws its flag; **chat rows** gain `reminders_due`, how many of the chat's reminders are due, counted as the row is read.
- **WhatsApp's own reminders** aren't in a form jaw can read or write yet, so these are jaw's alone, and your phone sees only the chat marked unread.

### Limits and status

- At most 1,000 scheduled items, waiting or held, a repeat counting once, holding at most 2 GiB of files; and 1,000 reminders not yet done. Past one, the request is error 5, with `data` `{"limit": "scheduled", "most": 1000}`, `{"limit": "scheduled_bytes", "most": 2147483648}` or `{"limit": "reminders", "most": 1000}`, and nothing is kept.
- `status` gains `scheduled`: how many are `waiting` and `held`, the `next` time, the `bytes` of files kept, the `most` of each, and how many were `refused` for a limit since jawd started; and `reminders`: how many are `due` and `waiting`, the `next` time, the `most` and how many were `refused`; and `awake`, whether jawd is keeping the Mac from idling to sleep for an item now.

## Versioning

- Adding an optional field, a method, a push, a message `kind` or an error code doesn't change the version; interfaces ignore what they don't know, and show an unknown `kind` as "jaw can't show this yet".
- Anything else, such as a field changing meaning or going, raises `protocol` by one.

## Fixtures

`fixtures/` holds one JSON file per message on the wire, named for what it is: `hello.request.json`, `hello.result.json`, `chats.watch.result.json`, `messages.push.json`, `message.row.voice.json`, and so on. jaw-proto's tests read each into its type and write it back, and the two must be equal, so a change to the format is always deliberate and shows in review.
