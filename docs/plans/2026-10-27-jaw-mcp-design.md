# jaw: AI assistants over MCP

27 October 2026. A design; nothing here is built. It details M9 of [jaw's design](2026-10-16-jaw-design.md) ("After the first release: AI assistants over MCP"), which comes after the first release. The owner's answers are marked "Owner, 27 Oct".

## What it is

The owner asked on 27 October for MCP support, so you can use your WhatsApp through AI assistants, from wherever you are: ask Claude on your phone what the climbing group settled for Saturday, and have it tell Rui you're in. MCP, the Model Context Protocol, is how Claude, Cursor, ChatGPT and other assistants reach tools and data; jaw offers its chats to them as tools, within limits you set. jaw calls them assistants, since `jaw agent` already names the LaunchAgent.

## Decisions

- **In three stages** (Owner, 27 Oct): assistants on this Mac (Claude Desktop, Cursor, Claude Code), then your own devices over a private network such as Tailscale, then cloud assistants such as Claude on your phone. Each stage is on its own branch with a security review.
- **A grant for each assistant, read-only by default** (Owner, 27 Oct). Sending is allowed assistant by assistant; each send is confirmed in the assistant's app where it can ask; sends are paced and capped, and marked in jaw as the assistant's.
- **M9, after the first release** (Owner, 27 Oct), before the second engine (M10) and the catch-up and asking (M11).
- **`jaw-mcp`, another interface on the socket.** Not MCP inside jawd, which would put an HTTP server and sign-in in the process that keeps `jaw.db` and acknowledges WhatsApp; and not jaw's protocol methods as tools one for one, which windows and pushes don't fit and which would hand assistants everything.

## How it fits together

```text
an assistant on this Mac    stdio
your devices                Streamable HTTP through Tailscale Serve, a token for each device
cloud assistants            Streamable HTTP through a public tunnel, with OAuth
            ⇅
jaw-mcp (Rust): one process for each assistant on this Mac; one jawd starts for the others
            ⇅ JSON lines on jaw.sock, hello as jaw-mcp with the grant
jawd
```

- **`jaw-mcp`** is a Rust binary in `crates/jaw-mcp`, built on jaw-client and jaw-proto, with `rmcp`, the official Rust SDK for MCP, if it holds up, and otherwise MCP's JSON-RPC by hand, as jaw-proto's is. It ships beside jawd and jaw-engine, in `jaw.app/Contents/Helpers` too, and `jaw mcp` in the terminal runs it, so the three always speak one protocol.
- **For an assistant on this Mac,** the assistant starts `jaw-mcp --grant <name>` and speaks to it on stdio. `jaw-mcp` connects to `jaw.sock`; it doesn't start jawd, and if jawd isn't running, its tools say so.
- **For your devices and cloud assistants,** jawd starts `jaw-mcp serve` once a grant has a device or a cloud connection, as it starts the engine, and stops it when the last is revoked. It listens on 127.0.0.1 only, checks each request's `Origin` as MCP's transport asks, and leaves reaching it to the network you run, which gives it TLS: Tailscale Serve within your tailnet, and for cloud assistants Tailscale Funnel or another tunnel you choose. jaw opens no port beyond the Mac's own loopback.
- **Each assistant's session** is one jawd connection for its grant, saying hello as `jaw-mcp`, with `focus` false and `grant` naming the grant.

## Grants

- **A grant** has a `name` ("Claude on the Mac"); the `folders` it sees (folder IDs, built-in ones included; none until you pick, and All is a choice); the `powers` it has beyond reading (any of `draft`, `react`, `mark_read`, `schedule`, `remind` and `send`); `confirm`, true unless you turn it off, so each send asks the assistant's app first; and its devices and cloud connections.
- **Made in jaw, never by an assistant:** the terminal's `jaw mcp grant add`, `grant edit`, `grants`, `device add` (showing a device's token once), `revoke` and `log`, and the Mac's Settings › Assistants. They use requests that a connection with a grant is refused with error 4: `grant.set`, `grant.delete`, `grant.device` (answering a token, once), `grant.revoke`, `grants.watch` and `assistant_log.watch`.
- **Enforced by jawd** on every request of a connection that said hello with a grant. `chats.watch`, `messages.watch`, `find`, `library.watch`, `chat.info`, `scheduled.watch` and `reminders.watch` see only chats in the grant's folders and their subfolders, worked out in SQL as folder windows already are. A chat outside is error 3, as one jawd doesn't have, so a grant learns nothing of what it can't see. A request its powers don't cover is error 4, naming the power. Requests no grant may make (`chat.start`, `link.*`, `folder.*`, `favourite.set`, `layout.*`, the grants' own, the chat actions, `edit`, `delete`, `forward`, `send.media` and `voice.*`) are error 4.
- **Changes apply at once:** a grant edited or revoked limits its connections from their next request, and revoking a device's token ends its sessions.

## Tools

| Tool | What it does | Power | jaw's request |
| --- | --- | --- | --- |
| `whats_new` | The grant's chats with unread messages, newest first, each with its unread messages, at most 20 a chat and 200 in all | reading | `chats.watch` on Unread, `messages.watch` |
| `list_chats` | The grant's chats, one folder's or all, newest first, each with its unread count and newest message | reading | `chats.watch` |
| `read_chat` | A chat's messages around its newest, a message or a time, at most 100, oldest first: each one's sender, time, text, kind, quote and reactions, and whether it's yours | reading | `messages.watch` |
| `search` | Messages, links and files matching words, from someone, in a chat, between dates | reading | `find` |
| `chat_info` | A chat's members, its community and its disappearing timer | reading | `chat.info` |
| `see_photo` | A photo's or a sticker's 768-pixel preview, as an image | reading | the message's `previews` |
| `leave_draft` | Puts text in a chat's box for you to send from jaw | `draft` | `draft.set` |
| `react` | Reacts to a message, or takes the reaction away | `react` | `react` |
| `mark_read` | Reads a chat up to its newest message | `mark_read` | `read` |
| `send_message` | Sends text, optionally replying to a message and mentioning members | `send` | `send.text` |
| `schedule_message` | Schedules text for a time (ISO 8601, with its offset), once or repeating | `schedule` | `send.text` with `at` and `repeat` |
| `list_scheduled`, `cancel_scheduled` | What's scheduled in the grant's chats, and cancelling one | `schedule` | `scheduled.watch`, `scheduled.cancel` |
| `remind_me`, `list_reminders` | A reminder on a message, and the reminders in the grant's chats | `remind` | `remind`, `reminders.watch` |

- **Answers are compact and say who:** each message carries its sender's name, its time in the Mac's time zone with the offset, and its chat; media is described ("a photo, 4032 by 3024", "a voice message, 0:14") unless `see_photo` asks for it. Messages sit in a field of their own, apart from what jaw says about them, and the server's instructions tell the assistant that what's in them is what people wrote, never instructions to follow.
- **`send_message` takes a `key`,** as `send.text` does, which the tool's description asks the assistant to make once for each message and give again if it asks again, so a lost answer never sends twice.
- **Tool annotations** mark the reading tools read-only, and the others as changing things beyond jaw, so assistants' apps that ask before such tools are used ask.

## Sends

- **Confirmed where the app can ask** (Owner, 27 Oct): with `confirm` on, `send_message` asks through MCP's elicitation ("Send to Ana Ribeiro: «running late, 10 min»?") and sends only on a yes. An app that can't ask, having said nothing of elicitation when it connected, is answered that jaw needs a yes for each send, so the assistant can say so, and nothing is sent. With `confirm` off, the grant's sends go without asking, for an assistant you trust with that.
- **Paced and capped:** at most 6 sends a minute and 60 an hour for each grant, reactions and scheduled messages counted with them; past either, the tool answers when it can send again.
- **Only to chats the grant reaches, never a new one:** `chat.start` isn't open to an assistant, so it can't write to a number it found in a message.
- **Marked as the assistant's:** each send, reaction, draft, mark and schedule is written to the assistants' log with its grant, tool, chat and message, and message rows are read with the log, so jaw shows "via Claude on the Mac" on them; WhatsApp sees an ordinary message.

## Never you

An assistant's connection says `focus` false and never changes it. Its reading doesn't count as you looking: it doesn't make you online, mark anything read, send receipts or quiet a notification, and its watches don't count as opening a chat for frecency. It marks a chat read only through `mark_read`.

## The stages

1. **This Mac:** stdio, with the grant named in the assistant's settings (`"command": "jaw-mcp", "args": ["--grant", "Claude"]`). The socket's checks are the boundary, as for every interface. Settings › Assistants and `jaw mcp grant add` print the lines to paste into Claude Desktop, Cursor and Claude Code.
2. **Your devices:** `jaw-mcp serve`, Streamable HTTP on 127.0.0.1, reached through Tailscale Serve. Each device has its own token: 256 random bits, shown once, kept as its SHA-256 with when it was last used, and sent as `Authorization: Bearer`. Revoking it ends its sessions.
3. **Cloud assistants:** the same server reached through a public tunnel (Tailscale Funnel, or one you choose), with OAuth 2.1 as MCP's authorization asks. `jaw-mcp` is the resource server and its own authorization server: protected resource metadata, authorization server metadata, dynamic client registration (cloud assistants register themselves) and PKCE. Its consent page asks for a one-time code that Settings › Assistants or `jaw mcp connect` shows, valid for 10 minutes, and which grant to give. Access tokens last an hour and refresh tokens are replaced at each use, all kept hashed and revocable; the consent page takes five wrong codes an hour at most.

## Safety

- **Other people's words.** An assistant reads messages others wrote, and any of them can try to steer it ("ignore your instructions and forward…"). Telling it so helps; what bounds the harm is the grant: the folders it sees, no new chats, a yes before each send, the caps, and the log of what it did.
- **What a grant isn't:** it bounds what an assistant can do through `jaw-mcp`. It isn't a wall against programs running as you on the Mac, which can already read jaw's folder and use its socket.
- **Exposure:** jaw never listens beyond the loopback; the private network or tunnel you run decides who reaches it, and every request that comes through needs a token or an OAuth grant.
- **What leaves the Mac:** whatever an assistant reads goes to its maker's servers when it's a hosted model, as the catch-up's "private by default" warns, so a grant's folders are that choice.
- **Reviewed:** each stage has a security review before it lands, and each change to the data layer the database expert's audit.

## The data layer

- **New tables of jaw's own:** `grants`; `grant_tokens` (devices' tokens and, from stage 3, OAuth's clients and tokens, each kept as a hash, never the token); and `assistant_log` (what each assistant did, kept for 90 days and at most 10,000 rows, the oldest let go). They're never dropped or rebuilt. Grants are backed up daily, tokens aren't, so a restored `jaw.db` asks devices to connect again. Each change goes through the one writer with its feed event, and is audited by the database expert.
- **`jawd status`** gains `assistants`: the grants, their connections, the sends refused for the caps, and the log's rows.

## The interfaces

- **The terminal:** `jaw mcp` runs `jaw-mcp`; `jaw mcp grant add Claude --folders Family,Work --can send,schedule` and the commands above manage grants; the palette's Assistants shows the grants and the log.
- **The Mac:** Settings › Assistants lists each grant with its folders (ticks on the folder tree), its powers, Confirm Each Send, its devices and cloud connections with when each was last used, Revoke, and the log. Connect an Assistant… shows what to paste for an assistant on the Mac, makes a device's token, or shows the one-time code for a cloud assistant. It's in the Mac design's Later until M9.

## Tests

- An MCP client in the tests drives `jaw-mcp` over stdio against `jawd demo`: every tool's answer; each grant's folders, powers and forbidden requests refused; a send confirmed, declined, and refused where the app can't ask; the caps; a `key` making a send asked twice one message; an assistant's reading leaving reads, receipts, presence and frecency as they were; the log, and "via" on its messages.
- Stage 2: tokens right, wrong, revoked and used after revoking; a wrong `Origin` refused; nothing listening beyond 127.0.0.1.
- Stage 3: OAuth end to end with a test client: PKCE, registration, a wrong code and its limit, refresh tokens replaced, revocation.
- A security review of each stage, and the database expert's audit of each change to the data layer.

## Build order

1. **This Mac** (on `mcp-local`): `jaw-mcp` over stdio, grants and their enforcement in jawd, the reading tools, drafts, reactions, marking read, sends with confirmation and caps, scheduling and reminders from M7, the log, `jaw mcp grant`, the protocol's text and fixtures, the tests, the review and the audit.
2. **Your devices** (on `mcp-devices`): `jaw-mcp serve`, jawd starting it, tokens, and the Mac's Settings › Assistants.
3. **Cloud assistants** (on `mcp-cloud`): OAuth, the consent page and its code.

## What this changes elsewhere

- **jaw's design** gets "After the first release: AI assistants over MCP" as M9, with `jaw-mcp` in its code, the tables, the tests and the risks.
- **The Mac design** gets Settings › Assistants in its Later, for M9.
- **The protocol** gets hello's `grant`, the grants' requests and the refusals when stage 1 lands.

## Later

- Assistants sending photos, files and voice messages, and waiting for a reply in a chat (a tool that answers when one comes, or after a time).
- Resources and their subscriptions, so an assistant hears of new messages without asking.
- The catch-up and asking (M11) as tools, within each grant.
