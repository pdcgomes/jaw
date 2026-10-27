# jaw: settings

26 October 2026. Settled with the owner over the morning, and amended the same day so that each frontend keeps settings of its own; being built on the `settings` branch. It adds to [the design](2026-10-16-jaw-design.md) and [the protocol](../../protocol/README.md), and the settings wireframes canvas draws it: every theme as jaw would map it, both layouts of your messages, and the panel.

## What it is

jaw gains settings, kept by jawd in a domain for each frontend, so the terminal and a Mac app on one machine keep separate preferences. The terminal's has two to start with: the theme, one of Omarchy's 23, and where your messages sit, on the right as now or on the left under "You", as IRC clients put them. The terminal shows them in a panel in the chat list's place and previews each theme on the whole screen as you move through them. A change is kept by jawd, which pushes it to every connection of the same frontend, and `jaw settings` reads and changes them from the command line.

## Decisions

- **A domain of settings for each frontend** (owner, 26 October, replacing the one set for every interface decided that morning, once the Mac plan had given the Mac app looks and themes of its own), so a terminal and a Mac app on one machine keep separate preferences. A frontend's domain is named by the `client` its hello gives: the terminal's is `jaw`, which `jaw settings` and `jaw status` share, since they say hello as `jaw` too. jawd keeps every domain in jaw.db as jaw's own data, as drafts and folders are. A connection reads its own domain's settings in hello's answer, changes them with `settings.set` and hears every change to them as a `settings` push. Only the terminal's domain is defined so far; the Mac app's comes with the Mac app. The themes stay shared: `themes` serves them to any frontend.
- **Typed, checked and served by jawd.** jaw-proto defines each domain's settings and carries the themes. jawd refuses a domain, a setting or a value it doesn't know, and serves the themes with `themes`, so an interface in any language draws them without porting them again. Opaque values that interfaces define were set aside: nothing would check them, and every interface would carry the palettes again. A window on the feed was set aside too: two values don't need its machinery, and windows arrive after hello, so the first frame would flash.
- **Omarchy's 23 themes, as Omarchy wrote them,** at a pinned commit, mapped onto jaw's colour roles by one rule, with bands of contrast so that every theme reads. Tokyo Night stays the default and becomes Omarchy's own, its pill taking the accent as every theme's does.
- **The IRC layout labels your messages "You"** and shows a name only when the sender changes, after half an hour or on a new day, as jaw does today.
- **The panel opens in the chat list's place,** with Ctrl-, or "settings" in the palette, so the open conversation shows each change on real messages.

## The terminal's settings

In the `jaw` domain:

| Setting | Values | Default |
| --- | --- | --- |
| `theme` | An Omarchy theme's ID: `catppuccin`, `catppuccin-latte`, `ethereal`, `everforest`, `flexoki-light`, `gruvbox`, `hackerman`, `kanagawa`, `last-horizon`, `lumon`, `lupine`, `matte-black`, `miasma`, `nord`, `osaka-jade`, `retro-82`, `ristretto`, `rose-pine`, `solitude`, `starship`, `tokyo-night`, `vantablack` or `white` | `tokyo-night` |
| `messages` | `bubbles`: yours in a column from a third of the way across, as now; `irc`: everyone's on the left under their name, yours under "You" | `bubbles` |

## The protocol

protocol/README.md gains a "Settings" section, served from the start, and the rows below in its tables of methods and pushes. Adding optional fields, methods and pushes doesn't change the protocol's version.

```json
{"id":1,"result":{"protocol":1,"jawd":"0.1.0","root":"/Users/me/Library/Application Support/jaw","seq":81233,"account":{"state":"linked"},"connection":{"state":"online"},"settings":{"theme":"tokyo-night","messages":"bubbles"}}}
{"id":20,"method":"settings.set","params":{"theme":"gruvbox"}}
{"id":20,"result":{"settings":{"theme":"gruvbox","messages":"bubbles"}}}
{"method":"settings","params":{"theme":"gruvbox","messages":"bubbles"}}
{"id":21,"method":"themes"}
{"id":21,"result":{"themes":[{"id":"catppuccin","name":"Catppuccin","mode":"dark","colors":{"accent":"#89b4fa","background":"#1e1e2e","…":"…"}}]}}
```

- **`hello`'s answer** carries `settings`: every setting of the connection's own domain, the one its `client` names, with the value in effect. It's absent for a client jawd keeps no settings for. An interface has them before it draws, and a reconnect brings them current again.
- **`settings.set`** changes the connection's own domain. A connection whose client has no domain is error 3, with `data` `{"client": "<client>"}`. It takes one setting or more and checks every one before changing any. A setting jawd doesn't know is -32602 with `data` `{"param": "<setting>"}`; a value it doesn't take, or one of the wrong JSON type, is -32602 with `data` `{"param": "<setting>", "allowed": [...]}`; no settings at all is -32602. It answers with `settings`, all of them as they now are, once the change is on disk, synced. A setting asked for with the value jaw.db already holds for it isn't written again, and an ask of nothing else is answered at once. While jaw.db can't be written for a reason that passes, such as a full disk, the answer waits as every write's does; a change that can't be kept at all is -32603 saying why, and nothing changes.
- **The `settings` push** goes to every greeted connection of the same domain, the one that asked included, whenever that domain's settings change, and carries all of them. A connection always ends on the latest settings: one change made while it's slow to read may be skipped, but never the last. Connections of other domains hear nothing of it.
- **A push and an answer** can come in either order: on the connection that asked, the `settings` push can come before the answer to the `settings.set` that made it. The answer carries the settings as that change left them, and another interface's change made just after it can be pushed before it too, so what jawd keeps is what the latest push says.
- **`themes`** answers with every theme, ordered by ID. Each has its `id`; a `name` to show, the ID in title case with "Rosé Pine" for `rose-pine`; its `mode`, `dark` or `light`; and `colors`, Omarchy's 25 colours by their keys as lowercase `#rrggbb`. Where a theme leaves them out, `orange` is its `yellow` and `brown` its `orange` mixed half with black, as Omarchy's own `omarchy-theme-color` resolves them. Keys a theme carries beyond those, such as Hyprland's borders, aren't served.
- **`status`** gains `settings`, each domain's settings in effect by the domain's name (`{"jaw": {"messages": "bubbles", "theme": "tokyo-night"}}`), and `settings_ignored`, each setting whose value in jaw.db this jawd doesn't take, with its domain, its key and that value. `jawd status` and `jaw status` print them on a line of their own.
- **Fixtures:** `hello.result.json` and `status.result.json` gain their fields, and `settings.set.request.json`, `settings.set.result.json`, `settings.push.json`, `themes.request.json` and `themes.result.json` (two themes, to pin the shape) are new.

## jawd

- **A `settings` module** (`crates/jawd/src/settings.rs`) holds each domain's settings in effect and what jaw.db holds for each, read at start, and publishes each domain's settings in effect on a tokio `watch` channel of its own, which always holds the latest value.
- **A connection's domain is settled at its hello,** from its `client`. Its receiver for that domain's channel is made then, and hello's answer carries the value that receiver has seen, so no change between the answer and the first push is missed.
- **One change at a time.** Under a lock, a change is checked, committed with a synced commit, published and answered, in that order. So the settings pushed are always what jaw.db holds, in the order the commits were made: two interfaces of one frontend changing its settings at once end on the later commit's values, in jaw.db and on every screen.
- **No push is lost.** Each greeted connection watches its domain's channel in its loop and sends the `settings` push when it changes, waiting for room in its queue as a reply does, so a full queue delays the push rather than dropping it. Pushes on the broadcast channel, such as `connection` and `notice`, can be dropped for a connection that falls behind; settings can't.
- **A stop between the commit and the answer** leaves the interface without an answer, and its next hello carries what was committed. jaw takes hello's settings as the truth.
- `themes` is answered at once in `sock.rs`, as `focus` is, and `settings.set` down the connection's lane, after the requests sent before it, as every request that writes jaw.db is.

## Storage and integrity

Migration 12 is a schema step: one new table, which costs the same however big jaw.db is, so it runs at start with any other pending schema steps in one IMMEDIATE transaction.

```sql
-- Class: jaw's own. Each frontend's settings, in a domain named by the client its hello gives, a
-- row for each setting chosen: never dropped or rebuilt, and copied as they are by any rebuild. A
-- setting without a row has its default.
CREATE TABLE settings (
    domain TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL CHECK (json_valid(value)),
    at INTEGER NOT NULL,
    PRIMARY KEY (domain, key)
) STRICT, WITHOUT ROWID;
```

- **Rows only for settings chosen.** A fresh jaw.db has none, so everyone starts on Tokyo Night with bubbles, and a default changed in a later jaw changes for whoever never chose.
- **Written through the writer thread** as a new `Change` carrying every setting one `settings.set` changes, applied in one step, so they're kept all or none, each an idempotent upsert (`INSERT … ON CONFLICT (domain, key) DO UPDATE SET value = excluded.value, at = excluded.at WHERE value IS NOT excluded.value`), in a synced commit (`apply_durable`: `FULL`, and `fullfsync` on macOS) before the answer. `value` is the setting's JSON, such as `"gruvbox"`, and `at` is when it was chosen, in milliseconds.
- **Read by key** at start: one search of the primary key for each setting of each domain jawd knows, so no statement scans the table. Its statements' plans are pinned in the plan test.
- **Bounded.** jawd writes only domains and settings it knows, so the table holds at most one row for each, two today; `jawd status` reports what's in effect and anything ignored.
- **Kept when not understood.** A row whose value this jawd doesn't take, written by a newer jawd within the compatibility window or naming a theme a later Omarchy drops, is left as it is and read as the default, and `status` says so. A row for a domain or a setting this jawd doesn't know is left alone and not read. Being jaw's own, neither is ever dropped.
- **Not in the feed.** The feed keeps windows current, and settings aren't a window: hello and the push deliver them. So a change to the settings writes no `events` row; the rule that a change and its event commit together is about the feed's rows.
- **A change that can't be kept** publishes nothing, and the answer says why; one waiting for a jaw.db that can't be written yet waits, as every write does.
- **Backups.** The design's daily backup of jaw's own tables, still to be built, includes `settings`.
- **The audit.** `crates/jawd/src/settings` joins the data-layer paths in `scripts/check-data-audit.py`, and the database expert audits the branch before it lands.

## The themes

- **The data** is Omarchy's 23 `colors.toml` files, as Omarchy wrote them at commit `4c1b4cb3a94447aff39af98e2fcc0490a9fb32e7` of `omacom/omarchy`, the commit the owner's dotfiles pin, in `protocol/themes/<id>.toml`, with Omarchy's MIT licence in `docs/licenses/omarchy.txt`. `scripts/omarchy-themes` fetches them again from the commit it pins, to move to a newer Omarchy.
- **jaw-proto reads them:** each file is included in the crate and parsed into a palette of Omarchy's keys, `mode` and 25 colours; where they're missing, `orange` is `yellow` and `brown` is `orange` mixed half with black, each channel rounded half up, as Omarchy resolves them. A file that doesn't parse, or lacks a key, fails a test rather than an interface. jawd serves the palettes with `themes`, jaw maps them onto its roles, and an interface in another language can read the same files or ask `themes`.
- **The mapping** is in jaw's `theme.rs`, where drawing still asks what a colour is for, never for a hue:

| jaw's role | From the theme | Kept readable by |
| --- | --- | --- |
| `bg`: the ground | `background` | |
| `surface`: overlays, monospace, reactions | `lighter_background` | at least 1.05:1 against the ground |
| `selection`: the selected row | `selection` | at least 1.15:1, against the ground and against the surface, which overlays draw it on |
| `fg` and `text`: text that matters, and ordinary text | `bright_foreground` and `foreground`, the one with more contrast as `fg` | |
| `muted`: secondary text, previews and hints | halfway between `comment` and `text` | lying between them |
| `comment`: times, day lines; `unseen`, ticks before they're read | `dark_foreground` | 2.5:1 to 4.5:1 |
| `gutter`: borders at rest | `muted` | 1.4:1 to 2.6:1 |
| `brand`, `focus` and `progress`: the name's pill, the focused border and the keys, how far catching up has got | `accent` | |
| `link` and `read`: links, read ticks | `cyan` | at least 3:1 |
| `mention`, `arrived` and `warn`: a mention, "n new", trouble | `orange` | at least 3:1 |
| `online`: online and typing | `green` | at least 3:1 |
| `notice`: the footer's notices | `yellow` | at least 3:1 |
| `mark` and `unread` | as `fg` | |
| Group senders, by jawd's index | `red`, `yellow`, `blue`, `magenta`, `cyan`, `green`, `orange`, `bright_cyan` | each at least 3:1 |

- **Bands of contrast.** Contrast is WCAG's, against the theme's background. A colour outside its band is moved in a straight line toward the background, if it stands out too much, or toward `fg`, if too little, until it reaches the band's target: 1.8:1 for borders, 3:1 for dim text and colours, 1.1:1 for the surface, and for the selection 1.25:1 against the ground, then 1.15:1 against the surface (owner, 26 October, once the review found White's and Vantablack's selection the same colour as their overlays). So it keeps its hue. At commit 4c1b4cb, seven themes' greys come through untouched (Catppuccin, Flexoki Light, Gruvbox, Hackerman, Lumon, Matte Black and Rosé Pine), and sixteen have one or more moved: dim text raised in Catppuccin Latte, Everforest, Last Horizon, Miasma, Nord, Solitude and White; borders and dim text lowered in Ethereal, Osaka Jade, Retro 82 and Starship; borders lowered in Miasma, Ristretto, Vantablack and White; the surface raised in Last Horizon, Lupine and Solitude; the selection raised against the surface in Kanagawa, Tokyo Night, Vantablack and White. Colours under 3:1, mostly light themes' yellows and greens, are raised the same way.
- **A test** checks every theme: the ladder of ground, borders, dim text, secondary text and text, in that order of contrast, each within its band; the selection at least 1.15:1 against the surface; `fg` on the selection at least 4.5:1; and every colour, the accent among them, at least 3:1.
- **Tokyo Night** becomes Omarchy's. Its background, text, dim text and accent stay; its selection becomes `#2d3247`, its borders `#414868`, its secondary text `#8088b0`, its links and read ticks `#449dab`, its mentions, "n new" and trouble Omarchy's orange `#eb927b` where they were `#ff9e64`, two group senders' colours Omarchy's magenta `#ad8ee6` and bright cyan `#0db9d7` where they were `#bb9af7` and `#73daca`, and the name's pill the accent, `#7aa2f7`, where it was teal.
- **Light themes:** Catppuccin Latte, Flexoki Light, Lupine, Rosé Pine and White. In a light theme the linking screen's QR code is drawn with its dark modules in the text colour on the background, dark on light as printed codes are; in a dark theme it stays light on dark, as now. The Mimic keeps its own colours.
- **Unchanged:** the 256-colour fallback takes each role's nearest of the 256, and `NO_COLOR` draws without colour, as now.

## The terminal interface

### Applying them

- The view takes the terminal's settings, the `jaw` domain's, from hello's answer, from `settings` pushes and from `settings.set`'s answers, each carrying all of them. A theme or a value jaw doesn't know falls back to the default, and so does a hello without settings.
- **A change shows at once,** while jawd's answer is on its way. If jawd refuses it, or it was never sent, the screen goes back to what jawd has and the footer says why after "not kept:", as in "not kept: jawd isn't connected; try again in a moment" for one never sent because jawd isn't connected. If its answer is lost, so jawd may have kept it, or jaw can't read the answer, the screen goes back as it does for a refusal, and the footer says "not confirmed: jawd went away; you'll see what it kept when it's back" or "not confirmed: jaw couldn't read jawd's answer". Any notice too long for the footer ends in "…".
- A change rebuilds the theme and clears every cached layout (messages, chat rows, the library's and the palette's), so the next frame draws every line again, within a keypress's 16 ms.
- **The first frame waits** for hello, for jawd to say it can't answer (starting, unreachable) or for 250 ms, whichever comes first, so a light theme never flashes Tokyo Night first. When jawd isn't running, the "starting jawd" screen draws in Tokyo Night and changes once hello comes.
- The made-up account behind `jaw demo`, `jaw snapshot --demo` and `jaw bench` keeps the settings in memory, answering and pushing as jawd does.

### The panel

- **Opening it:** Ctrl-,, which reaches jaw where the terminal speaks the kitty keyboard protocol (Ghostty, kitty, WezTerm, iTerm2, Alacritty), or the palette, which offers "Settings", with ctrl-, beside it, "Change the theme", which opens the panel on the theme list, and "Turn the IRC layout on" or "Turn the IRC layout off", which switches it at once, so "irc" finds it whichever layout is on. Both work wherever Ctrl-K does: once jawd has answered and the account is linked. Ctrl-, closes the panel from anywhere in it, keeping nothing previewed; Esc steps back, from the theme list to the panel and from the panel out. The keys sheet lists Ctrl-,; it drops the blank line between its two sections when the screen can't hold that line too, so every line shows at 80 by 24.
- **Where it sits:** in the chat list's column, with the focused border, so the open conversation beside it shows each change on real messages. Below 90 columns it takes the screen, as the chat list does there; in the spike's focused mode, it unfolds into the chat list's place while it's open.
- **While it's open,** it has the keys: the conversation and the box give up their focused borders, and the box its cursor; pastes, clicks and the wheel do nothing; and opening it takes the keys from a sidebar that would be drawn over it. It stays on screen while jawd is away, a change asked meanwhile being refused at once as never sent, and closes once the account is no longer linked.
- **Look › Theme:** Enter opens the list of the 23, each with its mode and a strip of its colours, the row with the keys marked ▸ in front and the theme kept ✓ after its name, so one row can show both. Moving through the list previews each theme on the whole screen at once, inside jaw only. Enter keeps the one previewed, sending `settings.set`, and Esc goes back to the one kept.
- **Chat › Your messages:** "on the right" or "on the left, as IRC". ← → or Enter switches it, sending `settings.set` at once.
- **What it says:** the footer shows the panel's keys while it's open. If a change isn't kept or confirmed, the footer says so and the screen goes back to what jawd has, as "Applying them" says. A push from another jaw while the panel is open changes the theme it shows as kept; a preview under way goes on, and Esc returns to the new one.

| Keys | In the panel | In the theme list |
| --- | --- | --- |
| Ctrl-, | Opens it, and closes it again | Closes the panel, keeping nothing previewed |
| ↑ ↓, j k | Moves between settings | Previews the theme before or after |
| PgUp PgDn, Home End | | Previews the theme ten before or after, or the first or the last |
| ⏎ | Opens the theme list; switches your messages | Keeps the theme previewed, back to the panel |
| ← → | Switches your messages | |
| Esc | Closes the panel | Back to the panel, with the theme kept |

### The IRC layout

- **`bubbles` doesn't change:** it's exactly today's layout.
- **In `irc`, your messages are laid out as others' are.** "You" goes in the name column, six columns wide, in `fg`, bold, so it stands apart from group senders' colours; your text starts at that column, with wrapped lines under it.
- **"You" follows the rule for names:** it shows when the sender changes, after half an hour or on a new day, and the messages after it sit under it.
- **Ticks and the time stay at the right edge,** as in `bubbles`: ticks on each message of yours, the time when the minute or the sender changes.
- **What hangs off a message moves with it:** a quote, reactions, pictures, a long message's "more lines", "not sent · ⏎ tries again" and the selected message's line of detail all start at the name column.
- **Your messages read as theirs do:** the selected one's line of detail starts with "You", as theirs start with their name, and one you deleted reads "This message was deleted", as theirs do; `bubbles` keeps "You deleted this message".
- **The rest is as it is:** system and call lines stay centred, and day separators, "n new", and the blank lines before a new sender in a group and after half an hour don't change.
- In `layout.rs`, the layout is part of `Shape`, so a message's cached lines are kept for each layout, and where your messages start is decided in one function.

### `jaw settings`

- `jaw settings` prints each of the terminal's settings, its value and the values it takes, the list of themes wrapped at 80 columns under it. It says hello as `jaw`, so it reads and changes the terminal's own domain.
- `jaw settings theme=gruvbox messages=irc` changes them with one `settings.set` and prints them as they now are. A mistake in its arguments, a change that isn't a `name=value` pair or names no setting, or a setting named twice, is refused before it connects, saying what's wrong, exiting with 2. A setting or value jawd refuses prints jawd's message, and jawd not running prints so, each exiting with 1, as `jaw status` does. So the owner's `dot theme` can keep jaw in step with Ghostty and Neovim.

## Testing

- **jaw-proto:** each new and changed fixture reads into its type and writes back the same; all 23 theme files parse with their 25 colours and `mode`, and the fallbacks fill `orange` and `brown` for Last Horizon, Solitude and White.
- **jawd:**
  - `settings.set` refuses unknown settings, values and types, and no settings, with -32602 and the `data` above.
  - It answers only after a synced commit: no answer comes while jaw.db's write lock is held from outside, and the synced commits rise by one, as the logout test does.
  - The value jaw.db already holds is answered at once and writes nothing.
  - Two connections changing settings at once end with jaw.db, the answers' last state and every connection's last push agreeing.
  - Each domain's settings reach only its own connections: a `jaw` connection is pushed a change to the terminal's settings, and a connection of another client gets no push, no `settings` in its hello's answer, and error 3 from `settings.set`.
  - A connection that leaves its queue full while changes come still ends on the latest settings.
  - Hello carries the settings; a jawd started again keeps them; a row with a value jawd doesn't take is kept, read as the default and named in `status`.
  - A change that can't be kept publishes nothing and answers with why.
  - Migration 12 joins the migrations' lock file, the upgrades from every fixture database, the failure injected at each statement and the plan test.
- **jaw:**
  - Every theme holds the contrast ladder and its bands.
  - Both layouts: yours under "You" at the name column with ticks and the time, the name shown when the sender changes, and quotes, reactions and pictures lined up.
  - The panel through snapshot keys: Ctrl-, opens it, moving through the themes previews them, Esc goes back, Enter sends `settings.set` once, and ← → switches the layout.
  - A push changes the theme on screen; the first frame is drawn with hello's theme; a light theme draws the QR code dark on light.
  - `jaw settings` prints and changes them against `jawd demo`.
- **Budgets:** the first frame stays within 50 ms, `jaw bench --first` before and after; a change of theme redraws the screen within a keypress's 16 ms; settings add no wake-ups when idle.

## Scope

- **In:** the settings table and the protocol (hello, `settings.set`, the `settings` push, `themes`, `status`); Omarchy's 23 themes and their mapping; the panel; the IRC layout; `jaw settings`; the made-up account; and the docs: the protocol's "Settings", a "Settings" part in the design's jawd and terminal sections with Ctrl-, among its keys, and a line in the README with Omarchy's licence.
- **Later:** the Mac app's domain, with its own looks and Redlamp's themes, which comes with the Mac app; a `jawd` domain for what jawd itself does, such as the Mac plan's media and notifications; a frontend reading or changing another's domain, through an optional `domain` in `settings.set`; following the terminal's own colours; more chat options; and the spike's experimental switches, which `JAW_EXPERIMENTS` holds for now, kept as settings once they're settled.

## How it lands

- Built in a worktree of its own, `../jaw-settings` on the branch `settings`, in this order: the protocol's types and fixtures; jawd's storage, module and socket; the theme files and their reading; jaw's mapping and applying the settings; the panel; the IRC layout; `jaw settings`; the docs.
- The gate passes first (`make test`, `make lint`, `make check-generated`, `make check-audit`). Then a fresh database expert audits the branch, round after round, until its note in `docs/audits/` ends `Verdict: approved`; every commit touching the data layer carries the `Data-Audit` trailer naming it.
- It lands from that worktree, and the wireframes canvas marks what's built as it does.
