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
- Requests can be sent without waiting for replies; replies come in any order. A client never blocks on one: whatever it asked for arrives as data.
  - `hello`, `focus`, `themes`, `chats.watch`, `messages.watch`, `watch.move` and `unwatch` are answered at once, without waiting for the connection's other requests, so a chat opens at once even while a message sent just before it waits for the disk.
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
| 4 | Not allowed, and why: editing a message older than WhatsApp allows, deleting someone else's message for everyone, a vote choosing more options than its poll takes (a second choice in a single-choice poll; a new vote replaces your last, as on the phones) |
| 5 | A limit, and which: presence subscriptions, a file too large to send, text longer than 65,536 characters (a message's, an edit's or a draft's), a layout past its size, panes, layouts or slots, a layout for one interface too many, the outbox, a forward's chats, a connection's windows or its finds and saves being answered, the last four with `data.limit` saying so |
| 6 | Protocol versions don't match |
| 7 | Cancelled: a newer request replaced this one, as each `find` replaces the last |

`message` is a sentence an interface can show as it is.

## Hello

The first request on a connection. Anything else first gets error 6.

```json
{"id":1,"method":"hello","params":{"protocol":1,"client":"jaw","version":"0.1.0","focus":true,"since":81200}}
```

- `protocol`: the version the client speaks. jawd serves one version and refuses others with error 6, saying which is older; jaw then says to run `make install` again.
- `client`: the interface's name, `jaw` for the terminal. jawd keeps each interface's layout (see "Layouts") and settings (see "Settings") under the name it gives, so its connections share them and another interface keeps its own.
- `focus`: whether the interface is in front. jawd tells WhatsApp you're online only while some interface has focus (see `focus` below).
- `since`: the last `seq` this client saw, if it's reconnecting. Its watches, sent again, then return only what changed.

```json
{"id":1,"result":{"protocol":1,"jawd":"0.1.0","root":"/Users/me/Library/Application Support/jaw","seq":81233,"account":{"state":"linked","me":{"id":"person:1","name":"Pedro","phone":"+351 912 000 000"}},"connection":{"state":"online"},"settings":{"theme":"tokyo-night","messages":"bubbles"}}}
```

- `root`: jaw's folder. Every `path` jawd sends is relative to it, and the files are readable by the same user.
- `account.state`: `unlinked`, `linking` or `linked`. From the moment jawd starts, an account that was linked when it last connected, and hasn't been logged out since, is `linked`, with `connection.state` `connecting` until the engine connects (`offline` when jawd has no engine to run). If the account was logged out meanwhile, a `connection` push says `logged_out` once the engine finds out, with a `reason`: WhatsApp's, or `this device isn't linked any more` when the engine finds no device. A folder never linked says `unlinked`, and so does one from a jawd before this, until its engine connects.
- `connection.state`: as the `connection` push below.
- `settings`: the settings of the connection's own domain, the one its `client` names, with the values in effect, as "Settings" says; absent for a client jawd keeps no settings for.

## Windows

An interface never asks for "all chats" or "all messages". It watches windows: a stretch of rows around an anchor, which jawd keeps current.

- A window is a list of rows sorted by `sort`, a string that compares bytewise and that no two rows in a list share. A chat list reads from the largest `sort` down; a chat's messages from the smallest up. A row's `v` (version) is the feed's `seq` when it last changed.
- An update upserts rows by `id` and removes rows by `id`. An upserted row replaces the one held when its `v` is at least as new: the same `v` can come again with a different `sort` while the chat list is held during a catch-up, or, in a chat list, with a new name for whoever the chat is with, its members or its last message's sender, which changes what a row shows without changing the row. A message row's `v` moves when anyone it names gets a new name: whoever sent it, whoever wrote what it quotes, and whoever it mentions. Upserts carry whole rows. Applying the same update twice ends in the same state, so there are no positions to keep in step.
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
| `typing` | Always empty, and gone from `protocol` 2: who's typing comes in the `typing` push (see Presence and typing in chats) |
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
| `kind` | `text`, `image`, `video`, `gif`, `video_note`, `voice`, `audio`, `document`, `sticker`, `album`, `location`, `live_location`, `contact`, `poll`, `event`, `invite`, `system`, `call` or `unknown`; the fields below that its kind needs sit in the row beside the rest. A business's templates, buttons and lists, answers to them, Meta AI's answers and comments on a community's announcements are `text`, a comment's `reply` naming the announcement |
| `spans` | The text and caption, parsed: `text`, `bold`, `italic`, `strike`, `mono`, `quote`, `list`, `link` (with `url`) and `mention` (with `person`) |
| `reply` | The quoted message: its `id`, who wrote it, and a one-line `summary` |
| `forwarded` | How many times, 0 if it wasn't |
| `reactions` | Each emoji, how many, and whether one is yours |
| `status` | On yours: `pending`, `sent`, `delivered`, `read`, `played` or `failed`; in groups, as on the phone, delivered once every member it was sent to has it and read once every one has read it (members who joined after don't count, and those who left don't hold it up), with each recipient's times in `message.info` |
| `edited`, `deleted`, `expires_at`, `view_once`, `starred` | As they are |
| `media` | For media kinds: `w`, `h` (the original's size in pixels, which the interface keeps the shape of), `seconds`, `bytes`, `name`, `mime`, `hd` (an HD copy exists), `previews` (`{"256": path, "768": path}`, each JPEG, or PNG for a sticker with its transparency, at most that many pixels on its long side, either absent until it's made), `thumbnail` (the tiny copy that came in the message, as a file, for drawing before a preview is made), `path` (the file, once downloaded), `state` (`remote`, `downloading` with `progress`, `local`, `expired`, `waiting_for_phone` or `gone`). Paths are relative to `hello`'s `root` |
| `album` | For `album`: `items`, each an `id`, a `kind` (`image` or `video`) and its `media` as above, in the order sent; the row's own `spans` are the album's caption |
| `voice` | `seconds` and `waveform`, 64 values from 0 to 100, and `played`: on someone else's, whether you've played it, as the played receipt jawd sent says (the interface shows a dot until then, and plays the next one not yet played when one ends) |
| `link` | A link preview: `url` (the URL it's for, with `https://` before one written without how it's reached), `title`, `description`, and `preview`, the small thumbnail that came with it, as a JPEG file under `media/links/`, once jawd has kept it. A preview someone sends comes from what their message carries; jawd never fetches a page for it. History's previews show their title and description, and their thumbnail only if jawd already keeps that one |
| `poll` | `question`, `options` (each with `name`, `votes`, how many chose it, and `mine`, whether you did), `max` (how many options one vote may choose) and `voters` (how many people chose any). Each voter's latest vote counts, by when they voted; jawd counts them as it reads the poll, so a vote shows in every window showing the poll, as a row upserted with a new `v`. A vote that comes before its poll waits for it, 10,000 at most, past which the oldest are kept in quarantine and come back when their poll does. A poll from before jawd kept votes shows its options with none until jawd has gone through it, soon after it starts |
| `location`, `contact`, `event` | Their fields, ready to show: `location` is `lat`, `lon`, `name`, `address`, `url`; `contact` is `name` and `vcard`; `event` is `name`, `description`, `start`, `end` (milliseconds since the epoch), `place`, `call` (a call link, if it has one), `cancelled`, and `going`, `maybe` and `not_going` counts with `mine` (proposed, with Composing) |
| `invite` | For `invite`: an invitation to join a group (`group`, its JID, and `name`) or to follow a channel (`channel`, its JID, and `name`, with `admin` true for one to be among its admins), and `expires`, when it runs out (milliseconds since the epoch), each as the invitation says; its caption is the row's `spans` |
| `system` | For `system` and `call`: one sentence, such as "Rui added Sofia", "Group added to the community: Festa de Natal" or "Missed voice call" |

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
- **`send.poll`** sends a poll as a message of yours, answered, kept, queued and shown as `send.text`'s is, `key` too: `question`, its `options` in their order, each named once, and `max`, how many of them one vote may choose (1 when it's left out, up to all of them). Each poll goes with a secret of its own, which jawd makes and the engine keeps as it sends it, since every vote in it is encrypted with a key made from it. A question or an option that's only spaces, fewer than 2 options, one named twice (a vote names an option by its name) or `max` outside 1 to their number is -32602; past 12 options, 255 characters for the question or 100 for an option it's error 5, with `data` `{"limit": "options", "most": 12}` (`"question"`, `"option"`).
- **`poll.vote`** is your vote in a poll, all the options you choose now, by name: it replaces your last, and none takes it back. It names its poll as the actions on a message do and is one of them: it answers once it's on disk, takes a `key`, goes as a message of its own after the chat's earlier ones (the engine encrypts it with the poll's secret), shows at once in the poll's `options` and `voters`, and is kept as yours once WhatsApp has it; if WhatsApp refuses it, the poll shows your last vote again, and every interface is sent a `notice`. More options than the poll's `max` is error 4 (a second choice in a poll taking one); an option the poll hasn't is -32602; a message that isn't a poll, or one deleted, is error 4; one jawd doesn't have is error 3. The same option twice is one choice.
- **`chat.start`** takes `phone` (international form; `+`, spaces, dashes and brackets are fine) or `person` (`person:` and a number, as `find` gives them) and answers with `chat`, as `chats.watch` sends it, found or made. A number is checked with WhatsApp first (error 3 if it isn't on WhatsApp, 2 if the engine isn't connected), and WhatsApp's LID for it is kept with it, so messages addressed either way are one chat. A person's chat is found under any JID they're known by before one is made.
- **`draft.set`** keeps `text` as the chat's draft for every interface, shown in the chat list as `draft`; empty text clears it. Error 3 for a chat jawd doesn't have, 5 past 65,536 characters.
- **`read`** says you've read `chat` up to `up_to`, a message's ID: the chat's read position moves forward (back only while it's the phone's guess, before WhatsApp has said where it is), its `unread` is counted again from there, and jawd marks the messages read on WhatsApp, at once and then at most every 2 seconds per chat, however fast an interface reads. Sending a message reads the chat up to it as replying on the phone does, and marks what it reads read on WhatsApp the same way; a forward reads none of the chats it goes to. When read receipts are off on the phone, only your own devices hear. Error 3 for a chat or message jawd doesn't have.
- **`typing`** (`chat`, `state`: `composing`, `recording` or `paused`) answers once the requests sent before it have been, as every request but those answered at once; it needs nothing from the disk itself. jawd tells WhatsApp when typing starts or changes, again every 10 seconds while it's said, and stops it 10 seconds after an interface last said so, or at once on `paused`.

## Reconnecting

A client that loses its connection reconnects, sends `hello` with `since` (the last `seq` it saw), and watches its windows again, each with `have`: the `id`s of the rows it holds for that window. Rows in `have` that haven't changed since `since` are left out of the results, and `remove` lists the ones in `have` that are no longer in the window, so a reconnect costs what changed, not the windows; rows that came into the window come whole, changed or not. A watch can give its own `since`, which wins over hello's; without `have`, a watch returns every row. If `since` is older than the feed keeps (7 days), or newer than its last `seq` (jaw.db was made again), the watches return everything, as a first watch does.

```json
{"id":3,"method":"messages.watch","params":{"chat":"351912345678@s.whatsapp.net","around":"newest","before":80,"after":0,"since":81200,"have":["3EB0A1B2C3D4E5F6","3EB0C431F2A9B8E7D6"]}}
```

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `hello` | `protocol`, `client`, `version`, `focus`, `since` | See Hello |
| `status` | | jawd's version, connection, account, engine, each pool's depth and times, what's missing (ffmpeg), what jaw.db holds (`stored`, among it `links`, `links_backfill`: the messages from before links were kept still to be looked at for theirs, and `link_thumbnails` with their bytes and how many were refused for want of room; `votes_waiting` for polls jawd hasn't seen, `votes_set_aside` past the 10,000 that wait, and `polls_backfill`, the messages still to be looked at for polls from before votes were kept; `kinds_backfill`, the messages still to be looked at for those jawd couldn't show when they came, to convert again, and `encrypted_kept`, the edits and answers to events encrypted with their message's secret, which jawd can't read yet and keeps with what they change), `interfaces`: how many are `connected`, whether one is `focused`, where `notifications` go, the requests `in_lanes`, read and not yet answered (this one among them), the most a connection's lane holds (`lane_most`), the `lanes_full`, whose connections jawd reads nothing more from until one is answered, and the connections with a reply, a `layout` push or a `settings` push waiting for room (`replies_waiting`, one each at most), which it reads nothing from and sends nothing else meanwhile, and the finds and saves being answered on tasks of their own (`apart`, at most `apart_most` a connection); `windows`: how many are `watched` on every connection, how many of them are `distinct`, the `most` one connection can watch, the bytes of the reads kept for windows watched alike to share (`kept_bytes`, at most `kept_most`, the oldest `let_go` past it), and the window `reads` made, `taken` from a kept read and `joined` to one under way; `layouts`: how many interfaces have one `kept`, of the `most`, the `largest_bytes` of a layout that isn't a book, of `bytes_most`, the `books` and the `largest_book_bytes`, of `book_bytes_most`, and how many were `refused` for a limit; `settings`: each domain's settings in effect by its name; and `settings_ignored`: any whose value jawd doesn't take |
| `settings.set` | any of `theme`, `messages` | `settings`, all of them, once kept. See Settings |
| `themes` | | `themes`: every theme with its colours. See Settings |
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
| `send.text` | `chat`, `text`, `reply`, `mentions`, `preview` (default true), `key`, and `at` and `repeat` to send it later | `message` and `row`, or `scheduled` when it's sent later. See Sending, and Scheduled messages and reminders |
| `send.retry` | `message` | |
| `chat.start` | `phone` or `person` | `chat`. See Sending |
| `send.media` | `chat`, `files` (each a `path`, `kind` and `caption`), `quality` (`sd` or `hd`), `reply`, and `at` and `repeat` | `messages`, or `scheduled` |
| `send.poll` | `chat`, `question`, `options`, `max` (default 1), `key`, and `at` and `repeat` | `message` and `row`, as `send.text`, or `scheduled`. See Sending |
| `poll.vote` | `message`, `options` (by name, none to take your vote back), and optionally `chat`, `key` | See Sending |
| `send.event` | `chat`, `name`, `description`, `start`, `end`, `place`, `call`, and `at` | `message`, or `scheduled`. See Composing |
| `send.contact` | `chat`, `cards` (each a `name` and a `vcard`), and `at` | `message`, or `scheduled`. See Composing |
| `send.location` | `chat`, `lat`, `lon`, `name`, `address`, and `at` | `message`, or `scheduled`. See Composing |
| `gif.search` | `query` (empty for what's trending), `limit`, `cursor` | `gifs` and the next `cursor`. See Composing |
| `send.gif` | `chat`, `gif` (an `id` from `gif.search`), `caption`, `reply`, and `at` | `message`, or `scheduled`. See Composing |
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
| `voice.stop`, `voice.send`, `voice.cancel` | `voice.send` takes `at` and `repeat` | `voice.send` returns `message`, or `scheduled` |
| `folder.create` | `name`, `parent` | `folder` |
| `folder.rename`, `folder.move`, `folder.delete` | `folder`, and `name` or `parent` and `position` | |
| `folder.add`, `folder.remove` | `folder`, `item` (a chat's `id` or a `person:` id) | |
| `folder.smart` | `folder`, `rule` | |
| `favourite.set` | `slot` (1 to 9), `target` (a folder or a chat), or null to clear | |
| `layout.get` | `interface` (optional, the connection's own by default), `format` (optional, 1 or 2, the one it was kept in by default) | `interface`, `v`, `layout` and `dropped`. See Layouts |
| `layout.set` | `v`, `layout` | As `layout.get`'s, and `kept`. See Layouts |

## Other pushes

`typing` and `presence` aren't sent yet; see Presence and typing in chats.

| Push | Params |
| --- | --- |
| `link` | `state`: `qr` with `code` and `expires`, `code` with the phone `code` to type, `done`, or `failed` with `reason` |
| `connection` | `state`: `connecting`, `online`, `catching_up` (with `done` and `total`), `phone_offline`, `offline`, `logged_out`, `update_needed`, `banned` (with `until`) or `engine_restarting` |
| `settings` | its domain's settings, as hello's `settings` |
| `typing` | `chat`, `who` (each `id`, `name` and `state`, `typing` or `recording`; empty once nobody is) |
| `presence` | `chat`, `person`, `state` (`online`, `seen` or `hidden`), `at` for `seen`, `known` |
| `job` | `id`, `kind` (`upload`, `download`, `transcode`), `message`, `progress` |
| `playback` | `message`, `state` (`playing`, `paused`, `ended`), `at`, `seconds`, ten times a second while playing |
| `recording` | `state` (`recording`, `stopped`), `seconds`, `level` (0 to 100), ten times a second while recording |
| `notice` | `text` and `tone` (`info`, `warning`, `error`): something an interface should say once |
| `resync` | `watch`: watch it again |
| `layout` | As `layout.get`'s result: the layout another connection of the same interface just kept. See Layouts |

WhatsApp never says the phone is offline, so `connection` says `phone_offline` once something jawd asked the phone and knows it has an answer to (older history in a chat the phone has said it keeps more of, so far) has gone 20 seconds unanswered online, counted afresh each time the connection comes back, and `online` again once the phone sends anything, or two minutes after jawd asked; messages still arrive and send meanwhile.

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

Proposed with the Mac app's box (its design, `docs/plans/2026-10-27-jaw-mac-box-design.md`), for jawd to build: sending events, contacts and places, and GIFs found by search. Events and places come in M2 beside polls, contacts in M4 beside media, and GIF search after M4. Until then the rows above are the draft. Each answers as `send.text` does, with the `message` it made, whose row arrives as an upsert; a `key` makes asking again safe, as for the message actions.

```json
{"id":40,"method":"send.event","params":{"chat":"120363000000000002@g.us","name":"Sintra","description":"Bring the rope","start":1792231200000,"end":1792252800000,"place":"Cabo da Roca","call":false,"key":"9f2c"}}
{"id":41,"method":"gif.search","params":{"query":"thank you","limit":24}}
{"id":41,"result":{"gifs":[{"id":"giphy:3o7abKhOpu0NwenH3O","w":480,"h":270,"preview":"media/g/3o7abK.mp4","title":"Thank You"}],"cursor":"24"}}
```

- **`send.event`:** a WhatsApp event, as the phones make them: `name` (required), `description`, `start` (required), `end`, `place` (a name, or a place as `send.location` takes it), and `call` true to add a WhatsApp call link. Its row is kind `event`; replies to it (going, maybe, not going) update its counts as upserts.
- **`send.contact`:** one card or several, each a `name` and a `vcard` (vCard 3.0, as the Contacts app exports it). Its row is kind `contact`.
- **`send.location`:** a place by `lat` and `lon`, with an optional `name` and `address`. Its row is kind `location`. Live locations aren't sent.
- **`gif.search`:** jawd asks Giphy, and nothing else in jaw talks to a service besides WhatsApp: only when an interface asks, which it does only while a GIF picker is open and something is typed (an empty `query` asks for what's trending). jawd fetches each result's small looping MP4 into its media cache and answers with paths, so interfaces show files, never URLs. Results are kept for an hour; Giphy's key is jawd's, set by environment. Error 6 when Giphy doesn't answer in 3 seconds.
- **`send.gif`:** a result by its `id`: jawd fetches its MP4 and sends it as WhatsApp's GIF (a looping video), with the `caption`. Its row is kind `gif`.

## Message actions (proposed)

Proposed with the terminal's M2 actions, for jawd to build, and served: `send.text`'s `reply` and `mentions`, `react`, `edit`, `delete`, `forward`, `message.info` and `chat.info`. It keeps their names and fills in what they leave open. Each action's result reaches every interface the same way: the rows it changes, upserted into the windows that show them, so an interface draws what jawd says rather than what it guessed.

`react`, `edit` and `delete` answer with nothing once they're on disk, synced, as `send.text` is, and `forward` with its messages then; each takes a `key` as `send.text` does: asking again with it within a day answers the same and does nothing more. Each, and `message.info`, names its message by `message`, its ID (`forward` its `messages`), and optionally `chat`, where it is: with `chat`, jawd finds it at once; without it, jawd looks for the ID in each chat, and an ID it finds in more than one chat is -32602 with `data.param` `chat`. A reaction, an edit or a deletion for everyone goes to WhatsApp as a message of its own, after the chat's earlier messages, waiting while WhatsApp can't be reached and tried again as a message is, and counts toward the outbox's thousand. Windows show it at once, as rows upserted with a new `v`, and the chat list too when it's the chat's newest message; jaw.db keeps it as done only once WhatsApp has it. If WhatsApp refuses it, the rows show as they were, and every interface is sent a `notice` saying so. One refused when it may already have gone, with no receipt for it within a day, shows as refused in the same way, but jawd keeps it for at least 30 days (a thousand at most, outside the outbox's thousand), and a receipt for it in that time does it then.

An action on a message of yours that hasn't reached WhatsApp yet (still pending, or refused) waits until that message has, so it never arrives before it, even while the message's chat is merging into another. If WhatsApp refuses the message, a reaction or an edit waits on, shown as now, and goes after the message once `send.retry` has sent it; a deletion for everyone goes, and once WhatsApp has it, nothing of the message is left to send. `send.retry` never sends a message deleted for everyone: it's error 4 while your deletion of it is on its way, and 3 once it's deleted, from jaw or from your other devices, or once your deletion is kept for a late receipt, since nothing of it is left to send then. A reaction or an edit whose message is deleted for everyone before it goes, by you, your other devices or whoever sent it, isn't sent, whether or not you then delete the message for you: the row shows without it, and every interface is sent a `notice` saying so.

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
  - `everyone` deletes your own for everyone within 60 hours of sending, as WhatsApp allows (error 4 after, for someone else's or one that failed to go; a group's admins deleting others' messages comes later; one deleted already answers as done). The row stays with `deleted` true and no spans, and its `summary` says "You deleted this message" for yours and "This message was deleted" for theirs, which an interface may show its own way, as the terminal's IRC layout reads yours as theirs do; from M3, when jawd keeps media, its media is dropped from the cache.
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

It follows the presence of the people in open one-to-one chats and roster windows, while some interface has focus, at most 20 new people a minute and 200 per connection to WhatsApp: open chats' people first, then favourites, then the rosters' in their order; subscribing to everyone is what presence trackers do. Past the limit, rows show their last known state.

Others' typing in every chat, and the presence of the person in a one-to-one chat, come as the `typing` and `presence` pushes, below; neither is sent yet.

## Presence and typing in chats

Every chat, groups included, shows who's typing or recording in it now, in its chat-list row, its header and the status line, and a one-to-one chat's header and status line show the person's presence. Both come as the `typing` and `presence` pushes, with each watch's answer saying how things are now, as the owner decided on 27 October (jaw's design, "Presence and typing in chats"). Neither is sent yet: typing comes first, and presence once M6's folders are on main, since it comes from the rows M6 writes. Chat rows' `typing`, always empty so far, goes when typing comes, and a field going raises `protocol` by one (see Versioning), so `protocol` is 2 from then.

Carrying them on chat rows instead, so that a watch again from `since` brought them up to date, was turned down: every start, pause and fade of typing would have been a write to jaw.db, on the one writer the inbox's commits share.

```json
{"id":3,"result":{"watch":"w2","seq":81233,"…":"…","typing":{"351912345678@s.whatsapp.net":[{"id":"person:12","name":"Ana","state":"typing"}]},"presence":{"person":"person:12","state":"online","known":true}}}
{"method":"typing","params":{"chat":"120363012345678901@g.us","who":[{"id":"person:12","name":"Ana","state":"typing"},{"id":"person:4","name":"Rui","state":"recording"}]}}
{"method":"typing","params":{"chat":"120363012345678901@g.us","who":[]}}
{"method":"presence","params":{"chat":"351912345678@s.whatsapp.net","person":"person:12","state":"seen","at":1792145000000,"known":true}}
```

- **Where it comes from.** Typing is WhatsApp's chat presence. WhatsApp says it for every chat you're in, groups included, while you're online (while some interface has focus, and for 30 seconds after), with the chat, who, and `composing`, `recording` or `paused`. Online and last seen come only for the people jawd follows (see Presence), within its limits and only while an interface has focus. Nobody new is followed for this: the people of open one-to-one chats are followed first.
- **Who's typing** is a list, oldest first, each with `id` (`person:` and a number), `name` (as a message from them would show it) and `state` (`typing` or `recording`). Your own other devices aren't shown typing.
- **When it ends.** Typing fades 25 seconds after it was last said, as the phones let it fade. `paused` ends it at once, and so does a message from them in that chat, as on the phones. All of it ends at once when jawd tells WhatsApp you're away or WhatsApp can't be reached, since nothing more would come, and jawd starts with nobody typing. jawd says when typing ends, so an interface runs no timer for it.
- **Presence** is the roster's: `state` `online`, `seen` with `at`, or `hidden` (they hide it, or WhatsApp hasn't said), and `known`. `known` is false while jawd isn't following them: nobody's looking, WhatsApp can't be reached, or they're past the limits. The interface then draws the last known state faintly, as the roster does. The roster's other two states, typing and recording, show from the chat's typing, so the two never disagree. Typing is never faint, since it shows only while jawd hears it.
- **Watching.** The answers to `chats.watch`, `messages.watch` and `watch.move` carry `typing`: who's typing in each chat of the window where someone is. It replaces what the interface held for every chat in the window, so a chat left out has nobody typing. A `messages.watch` of a one-to-one chat also carries its person's `presence`.
- **After the answer,** `typing` comes for a chat the connection's windows show when its typing changes, and just after the window's push that brings a chat in with someone typing. `presence` comes when the presence of the person of a one-to-one chat the connection has open changes.
- **What an interface keeps:**
  - typing for the chats its windows show, forgotten as a chat leaves them;
  - presence for the one-to-one chats it has open, forgotten at `unwatch`;
  - nothing of either once its connection ends.

  Neither has a `v`. A watch again from `since`, after `resync` or after a reconnect says how things are now, whatever `since` is, so nothing is left over.
- **Nothing written.** Who's typing in a chat lives in jawd's memory only: it's never written to jaw.db, moves no `v` or `seq`, and never reaches the writer, so a message's ack costs what it did. Presence is what M6 keeps on people's rows for the roster's order (online, last seen, and whether jawd follows them), sent when a person's `v` changes.
- **Busy groups.** jawd takes the engine's typing without waiting, and drops and counts what comes faster than it can take. Each chat's typing reaches an interface at most every 50 ms: the newest replaces what wasn't sent yet, and the first change after a quiet spell goes at once, as window updates do. A connection holds at most one unsent `typing` for each chat it shows, which waits with its window updates while the interface isn't reading (see Pushes for windows), so a slow interface holds nothing up and gets the newest once it reads. `jawd status` gains `typing`: the `chats` with someone typing now, and how many changes were `heard`, `dropped` for coming too fast, `sent`, and `merged` into a newer one before they went.
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

- **Every send takes `at`:** `send.text`, `send.poll` and `forward` as jawd serves them now, and `send.media`, `voice.send` and Composing's `send.event`, `send.contact`, `send.location` and `send.gif` as each is served. With `at` (milliseconds since the epoch), jawd keeps what the request would have sent, to send at that time, and answers `scheduled`, the item's row, in place of `message`. `at` must be at least a minute ahead and at most 366 days; otherwise it's -32602 with `data` `{"param": "at"}`.
- **`send.text`, `send.poll`, `send.media` and `voice.send` take `repeat` too,** a rule (below). With it, `at` is its first time, and can be left out for the rule's first time from a minute ahead.
- **The request is checked as a send is,** when it's scheduled: a chat jawd doesn't have is error 3, text that's only spaces -32602 and too long error 5, a `reply` jawd doesn't have error 3 and one deleted error 4, mentions as Message actions says, a poll's question and options as `send.poll` says, and each forwarded message as `forward` does. Media is planned and transcoded when it's scheduled, so a file that can't go says so then, and its files are kept in jaw's folder until its last time; a voice message is the stopped recording, and a GIF is fetched when it's scheduled, since `gif.search`'s results last an hour.
- **`key`,** as `send.text` takes it: asking again with the same key within a day answers with the same item as it is now and keeps nothing new; if it has gone by then, it's error 4 as a change would be (below), and error 3 once it's been cancelled.
- **A `forward`** to several chats is an item for each, answered as a list of rows in `scheduled`, each item forwarding every message named, in order. Forwards take `at` and not `repeat`.
- **Scheduling clears the chat's draft,** as sending does, since what was typed has gone where it was meant to go; a forward leaves the chats' drafts, as forwarding does.

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
