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

```json
{"id":7,"method":"send.text","params":{"chat":"351912345678@s.whatsapp.net","text":"on my way"}}
{"id":7,"result":{"message":"3EB0C431F2A9B8E7D6"}}
{"method":"messages","params":{"watch":"w2","seq":81233,"upsert":[{"id":"3EB0C431F2A9B8E7D6","v":1,"status":"pending"}]}}
```

### Errors

| Code | Meaning |
| --- | --- |
| -32700 | A line that isn't JSON |
| -32600 | JSON that isn't a request |
| -32601 | A method jawd doesn't know |
| -32602 | Parameters it can't use; `data` says which |
| 1 | Not linked: the account needs linking first |
| 2 | The engine isn't available right now; `data.retry_ms` says when to try again |
| 3 | Not found: the chat, message, folder or person doesn't exist |
| 4 | Not allowed, and why: editing a message older than WhatsApp allows, deleting someone else's message for everyone, voting twice on a single-choice poll |
| 5 | A limit, and which: presence subscriptions, a file too large to send |
| 6 | Protocol versions don't match |

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
- `account.state`: `unlinked`, `linking` or `linked`.
- `connection.state`: as the `connection` push below.

## Windows

An interface never asks for "all chats" or "all messages". It watches windows: a stretch of rows around an anchor, which jawd keeps current.

- A window is a list of rows sorted by `sort`, a string that compares bytewise. A row's `v` (version) goes up each time it changes.
- An update upserts rows by `id`, newest `v` wins, and removes rows by `id`. Applying the same update twice, or two in the wrong order, ends in the same state, so there are no positions to keep in step.
- `before` and `after` say how many rows to keep on each side of the anchor. jawd keeps that many as rows move, removing those that fall out and upserting those that come in.
- `total` is how many rows the whole list has, for a scroll bar.
- `watch.move` re-anchors a window, for scrolling; `unwatch` ends it. A connection can have any number of windows.

### chats.watch

```json
{"id":2,"method":"chats.watch","params":{"folder":"all","around":"top","before":0,"after":60}}
```

- `folder`: `all`, `unread`, `groups`, `communities`, `archived`, or a folder's `id`.
- `around`: `top`, or a chat's `id`, to keep the selected chat where it is.
- Pinned chats sort first, then by their newest message's time. Older history arriving never changes a chat's sort; a new message does.

```json
{"id":2,"result":{"watch":"w1","total":312,"rows":[{"id":"351912345678@s.whatsapp.net","v":42,"sort":"1~1792145060000","kind":"direct","name":"Ana Ribeiro","avatar":"media/avatars/5f2c.jpg","last":{"from":"Ana","summary":"did you get the photos from Sunday?","kind":"text","status":null,"at":1792145060000},"unread":2,"mentions":0,"pinned":true,"muted_until":null,"archived":false,"draft":null,"typing":[],"community":null}]}}
```

A chat row:

| Field | |
| --- | --- |
| `id` | Its JID |
| `kind` | `direct`, `group`, `community`, `announcement`, `broadcast` or `self` |
| `name`, `avatar` | Its name as you know it, and its picture's path, once fetched |
| `last` | Its newest message: who, a one-line `summary`, its `kind`, its `status` if it's yours, and when |
| `unread`, `mentions` | Unread messages, and how many mention you |
| `pinned`, `muted_until`, `archived`, `marked_unread` | As on the phone |
| `draft` | What's typed in its box, from any interface |
| `typing` | Who's typing or recording now |
| `community` | For a group in a community, the community's `id`; for a community, its groups' unread total in `children_unread` |

### messages.watch

```json
{"id":3,"method":"messages.watch","params":{"chat":"351912345678@s.whatsapp.net","around":"newest","before":80,"after":0}}
```

- `around`: `newest` (the window follows new messages), `unread` (the first unread one), or a message's `id`.
- While a window follows `newest`, new messages come into it and the oldest leave. Otherwise new messages past its end only raise `newer`, which an interface shows as "2 new".
- `older` says whether there's more before the window; moving past the oldest message jaw has asks the phone for more (`history.more`), and they arrive as upserts.

```json
{"id":3,"result":{"watch":"w2","total":5120,"older":true,"newer":0,"rows":[{"id":"3EB0A1B2C3D4E5F6","v":3,"sort":"1792145060000~000123","chat":"351912345678@s.whatsapp.net","at":1792145060000,"from":{"id":"person:12","name":"Ana","colour":3},"mine":false,"kind":"text","spans":[{"t":"text","s":"did you get the "},{"t":"bold","s":"photos"},{"t":"text","s":" from Sunday?"}],"reply":null,"reactions":[{"emoji":"❤","n":2,"mine":true}],"status":null,"edited":false,"deleted":false,"expires_at":null}]}}
```

A message row, render-ready:

| Field | |
| --- | --- |
| `id`, `chat`, `at` | Its WhatsApp ID, its chat and when it was sent (milliseconds since the epoch) |
| `from` | The sender's person `id`, the name to show, and a `colour` index for group senders; absent on your own |
| `mine` | Whether you sent it |
| `kind` | `text`, `image`, `video`, `gif`, `video_note`, `voice`, `audio`, `document`, `sticker`, `location`, `live_location`, `contact`, `poll`, `event`, `system`, `call` or `unknown` |
| `spans` | The text and caption, parsed: `text`, `bold`, `italic`, `strike`, `mono`, `quote`, `list`, `link` (with `url`) and `mention` (with `person`) |
| `reply` | The quoted message: its `id`, who wrote it, and a one-line `summary` |
| `forwarded` | Whether, and how many times |
| `reactions` | Each emoji, how many, and whether one is yours |
| `status` | On yours: `pending`, `sent`, `delivered`, `read`, `played` or `failed`; in groups, the least far any recipient has got |
| `edited`, `deleted`, `expires_at`, `view_once`, `starred` | As they are |
| `media` | For media kinds: `w`, `h`, `seconds`, `bytes`, `name`, `mime`, `hd` (an HD copy exists), `previews` (paths at 256 and 768 pixels), `path` (the file, once downloaded), `state` (`remote`, `downloading` with `progress`, `local`, `expired`, `waiting_for_phone` or `gone`) |
| `voice` | `seconds` and `waveform`, 64 values from 0 to 100 |
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
{"method":"chats","params":{"watch":"w1","seq":81240,"total":313,"upsert":[{"id":"…","v":43,"sort":"1~1792145099000","unread":3,"last":{"…":"…"}}],"remove":[]}}
```

- `chats`, `messages`, `roster` and `folders`, each naming its `watch`. An upserted row that's already in the window may carry only its changed fields with its new `v`; a row new to the window carries all of them.
- `messages` pushes also carry `older` and `newer` when they change.
- **Coalescing:** jawd keeps, per watch, only the newest version of each row, and sends a batch when its feed has been quiet for 5 ms, or every 50 ms during a burst. A burst of 2,000 messages becomes at most 20 batches a second per window, each with only what the window shows.
- **While catching up** after time offline, the chat list reorders at most every 2 seconds; rows update in place in between.
- **A slow client** never holds jawd up. Its pending updates are bounded by its windows, since only the newest of each row is kept; one too far behind gets `resync` for its watches and watches again.

## Reconnecting

A client that loses its connection reconnects, sends `hello` with `since` (the last `seq` it saw), and watches its windows again. Rows that haven't changed since `since` are left out of the results, so a reconnect costs what changed, not the windows. If `since` is older than the feed keeps (7 days), the watches return everything, as a first watch does.

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `hello` | `protocol`, `client`, `version`, `focus`, `since` | See Hello |
| `status` | | jawd's version, connection, account, engine, each pool's depth and times, what's missing (ffmpeg) |
| `focus` | `focused` | |
| `link.start` | `method` (`qr` or `phone`), `phone` | Pushes `link` as it goes |
| `link.cancel`, `link.logout` | | |
| `chats.watch`, `messages.watch`, `roster.watch`, `folders.watch` | See Windows | `watch`, `total`, `rows` |
| `watch.move` | `watch`, `around`, `before`, `after` | `total`, `rows` |
| `unwatch` | `watch` | |
| `search` | `query`, `chat` (optional), `limit`, `cursor` | `hits` (message rows with the match marked) and the next `cursor` |
| `contacts.find` | `query` | People and chats to start or open, best first; numbers are checked against WhatsApp |
| `find` | `query`, `kinds` (any of `chat`, `person`, `folder`, `message`, `link`, `media`, `file`; all by default), `chat` (optional), `limit` | `results`, best first, each with its `kind`, a `score`, the match marked, and the row it found; a newer `find` from the same connection cancels this one |
| `library.watch` | `kind` (`photo`, `video`, `voice`, `file`, `link` or `all`), and optionally `chat`, `person`, `since`, `until`, `text`, with `around`, `before`, `after` | `watch`, `total`, `rows`: each the message's media, file or link with its preview, sender, chat and time, and the message's `id` to jump to |
| `send.text` | `chat`, `text`, `reply`, `mentions`, `preview` (default true) | `message` |
| `send.media` | `chat`, `files` (each a `path`, `kind` and `caption`), `quality` (`sd` or `hd`), `reply` | `messages` |
| `send.poll` | `chat`, `question`, `options`, `max` | `message` |
| `poll.vote` | `message`, `options` | |
| `react` | `message`, `emoji` (empty to remove) | |
| `edit` | `message`, `text` | |
| `delete` | `message`, `for` (`me` or `everyone`) | |
| `forward` | `messages`, `to` (chats) | `messages` |
| `star` | `message`, `starred` | |
| `read` | `chat`, `up_to` (a message) | |
| `typing` | `chat`, `state` (`composing`, `recording` or `paused`) | |
| `draft.set` | `chat`, `text` | |
| `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread`, `chat.clear`, `chat.delete` | `chat`, and `pinned`, `archived`, `until` or `unread` | |
| `chat.info` | `chat` | Its members and their roles, its community, its disappearing timer, its media count |
| `message.info` | `message` | Each recipient's delivered, read and played times |
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

## Presence

jawd tells WhatsApp you're online while any interface's `focus` is true, and away 30 seconds after the last one loses it, so your phone keeps its notifications while you're not looking. WhatsApp only sends others' presence while you're online.

It follows the presence of the people in roster windows and open chats, while some interface has focus, at most 20 new people a minute and 200 per connection to WhatsApp, favourites first; subscribing to everyone is what presence trackers do. Past the limit, rows show their last known state.

## Versioning

- Adding an optional field, a method, a push, a message `kind` or an error code doesn't change the version; interfaces ignore what they don't know, and show an unknown `kind` as "jaw can't show this yet".
- Anything else, such as a field changing meaning or going, raises `protocol` by one.

## Fixtures

`fixtures/` holds one JSON file per message on the wire, named for what it is: `hello.request.json`, `hello.result.json`, `chats.watch.result.json`, `messages.push.json`, `message.row.voice.json`, and so on. jaw-proto's tests read each into its type and write it back, and the two must be equal, so a change to the format is always deliberate and shows in review.
