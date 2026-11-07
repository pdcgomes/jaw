# jaw for the Mac: Settings and themes

6 November 2026. A design; nothing here is built. It changes Settings and the "Themes" of [the Mac design](2026-10-26-jaw-mac-design.md), and the themes of [the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 3.5, which it brings forward with Redlamp's families from Mac-M5. The wireframes canvas draws it (`mac-settings-themes-wireframes.canvas.tsx` in the project's canvases) and asks the questions it leaves open. The owner's choices are marked "Owner, 6 Nov"; the rest is marked "Default" for his review.

## What it is

The owner wrote on 6 November:

> the settings layout, in particular the done button and the top segmented controls are not looking great - the panel doesn't quite conform to the standards.

> Additionally, and more importantly, there are no available themes to select from, we must add them.

> for the settings instead of using a segmented control use icons - you may use redlamp as a reference

Settings is a sheet over the main window today (`MainWindowController.showSettings()`): a segmented control across its top picks Appearance, Messages or Panes, a form 480 by 360 points shows the one picked, and Done closes it. Appearance holds the appearance (the system's, dark or light) and a Theme menu with one entry, Jaw (Tokyo Night), above "Redlamp's themes come in a later version." The segments stand in for tabs because a SwiftUI tab view outside a `Settings` scene draws its tabs across its own top edge, which a sheet has nowhere to hold, as `SettingsView.swift` says; Done is there because a sheet needs a way out.

jawd has served Omarchy's 23 themes with `themes` since `settings` landed on 27 October (ebbd905), and the terminal draws them, mapped onto its roles by `crates/jaw/src/theme.rs`. The Mac asks for none of them, and Redlamp's families wait for Mac-M5.

Redlamp, the reference, opens Settings with ⌘, as a SwiftUI `Settings` scene (`RedlampApp.swift`): a tab view 500 points wide whose tabs are icons with their names in the window's toolbar (Appearance's is `paintpalette`), each pane a grouped form whose explanations are its sections' footers, and no Done, since every change is kept as it's made (`packages/RedlampUI/Sources/Settings/SettingsView.swift`). Its theme menu picks one of 22 families, each a matched dark and light pair, and its harness's theme gallery draws each family as a small panel in its colours.

## Decisions

- **Both sets of themes now** (Owner, 6 Nov): Omarchy's 23, read from jawd's `themes` and mapped onto the Mac's roles as the terminal maps them onto its own, and Redlamp's families, copied into jaw, beside jaw's Tokyo Night. This brings Redlamp's families forward from Mac-M5, where the parity plan's step 3.5 (Owner, 27 Oct) had put them.
- **A theme for dark and a theme for light** (Owner, 6 Nov): with Appearance on the system's, Jaw switches between them as the Mac does, and Tokyo Night fills both. Most of Omarchy's themes have one mode; Redlamp's families and Tokyo Night have both.
- **A gallery** (Owner, 6 Nov): the Appearance pane shows each theme as a small chat in its colours, as Redlamp's gallery shows its themes and as the Mac design's Appearance pane already has it.
- **This round** (Owner, 6 Nov): the Settings window as Redlamp's, with icon tabs, opened with ⌘, and no Done button, and the themes. The settings jawd keeps (downloads, the cache, notifications: parity step 3.5's other half) come after.
- **A window of its own, made by AppKit's tab view controller** (Default): an `NSTabViewController` in its toolbar style, each tab a SwiftUI pane. SwiftUI's `Settings` scene, Redlamp's way, exists only in a SwiftUI `App`, and Jaw is an AppKit app (`main.swift`, `AppDelegate`, a menu bar made from `Commands`); moving its life cycle to SwiftUI's for one window would reach into launch, which has a 250 ms budget, the menus and every window. The tab view controller draws the toolbar of icon tabs the Mac's own apps have, and the panes stay SwiftUI, where the Mac design puts settings.
- **500 points wide, each pane its own height, nothing to confirm** (Default): Redlamp's width. ⌘, opens it or brings it to the front, on the pane last shown; its close button or ⌘W closes it; every change is drawn and kept as it's made, as today, so Done goes with the sheet.
- **The system's look** (Default): the window is drawn in the system's forms and controls, in the app's appearance, as Redlamp's Settings is; the themes show in the gallery's cards and in the main window beside it.
- **The gallery shows one slot's themes at a time** (Owner, 6 Nov): the slot being chosen for decides which half of each theme its card shows, and themes only of the other mode are left out.
- **A theme with both halves fills both slots** (Owner, 6 Nov): Tokyo Night and Redlamp's families fill both, one of Omarchy's fills the slot of its mode, and ⌥-click fills only the slot chosen for.
- **Every theme shown, under its source** (Default): Jaw's, Omarchy's and Redlamp's sections, each theme as its source names it, so three Tokyo Nights show, each in its section; a theme named outside its section carries its source ("Nord, Redlamp's").
- **Omarchy's, by theme.rs's rule, held on both grounds the Mac draws on** (Owner, 6 Nov): the terminal's mapping ported to Swift as it is, then each colour also held at 3:1 against the surface, where the Mac draws their bubbles and its chat list.
- **A role for links** (Default): the Mac's roles gain `link`, links and read ticks, from Omarchy's `cyan` as `theme.rs` takes it, where the Mac draws them in the accent today.
- **Redlamp's, by the Mac design's table, held by the same bands** (Owner, 6 Nov), so a family made for a photo editor's panels reads in a chat as Omarchy's do.
- **The chosen themes' colours remembered** (Default): with the slots in the app's preferences, so the first frame is drawn in them before jawd answers, and a Jaw whose jawd can't be reached keeps them.
- **Kept by the app, not jawd** (Default): the Mac's look stays in its own preferences (Owner, 27 Oct); jawd keeps no settings domain for `jaw-mac`.
- **Redlamp's catalog relicensed for jaw under jaw's MIT** (Owner, 6 Nov): in his words, "I own redlamp, MIT is fine." Its owner relicenses Redlamp's catalog for jaw, as the Mac design allowed ("or the owner, who holds both, relicenses them for jaw"), so the copy carries jaw's licence with one line saying it's Redlamp's catalog, relicensed by its owner for jaw, and there's no `docs/licenses/redlamp.txt`.
- **`scripts/sync-themes` runs where a Redlamp checkout is** (Default): Redlamp's repository is private, so not in CI, where the Mac plan had it.
- **Tint, translucency, native controls and the palette's own theme stay in Mac-M5** (Owner, 6 Nov).
- **Every colour readable where it's drawn, Redlamp's red kept to its accent, and Neutral's senders in colour under a grey accent** (Owner, 6 Nov, three answers): "Yes: all of them at 3:1, in every theme, Tokyo Night Day included"; "Yes: light mentions take another colour, and the senders' colours start away from red"; and "Yes: with a grey accent, senders' names stay colourful enough to tell apart". Every theme's roles, Jaw's, Omarchy's and Redlamp's alike, go through one hold after their own mapping, which keeps each colour drawn as text or as a mark at 3:1 on every ground it's drawn on, the tints laid over a ground among them, with no mark drawn on a tint laid on a tint, the accent drawn as text in a colour of its own; the Redlamp family's light mentions take the brand's filament and its senders' hues keep 30° or more from its red; and Neutral's senders take the system's blue's hue where the accent is a grey ("Every theme, held where it's drawn", below).

## How it works

### The window

- **Made:** `SettingsWindowController`, in the app, holds an `NSWindow` that is titled and closable, neither resizable nor minimisable, its toolbar in the `.preference` style, whose content is an `NSTabViewController` with `tabStyle` `.toolbar`. Each tab item has the pane's name as its label, an SF Symbol as its image, and an `NSHostingController` of the pane's SwiftUI view, from JawUI, as its view controller. The window's title is the chosen pane's name.
- **Its size:** 500 points wide. Each hosting controller gives its pane's height as its preferred content size, and the window takes it when its tab is chosen, keeping its top edge where it is: Appearance is 640 points tall with its gallery scrolling inside it, and Messages and Panes are as tall as their forms, about 360 and 220, Messages' with Send photos and videos in HD in a section of its own since main's attachments.
- **Opening:** ⌘, and Jaw › Settings…, the `settings` command that opens the sheet today, open the window or bring it to the front; so does the palette's Settings, and a new palette action, "Choose a theme", opens it on Appearance, as the terminal's palette offers "Change the theme". It opens on the pane last shown, kept as `settingsPane`, centred over the main window the first time and where it was left after, its frame kept as `jaw.settings` except in a snapshot run, as the main window's is.
- **Closing:** its close button or ⌘W; Esc doesn't close it, since it's a window and not a sheet. Nothing waits on closing: each change is drawn in every window and kept in the preferences as it's made, through `Preferences.changed`, as today.
- **Beside the chats:** a window rather than a sheet, so the main window stays in use and shows each change while Settings is open: a theme chosen in the gallery is seen on the chats at once.
- **Its look:** the system's grouped forms, section headers and footers and controls, in the app's appearance as the main window takes it, not the theme's roles. Each explanation becomes its section's footer, where today it's a row of the form.

### The panes

| Pane | Icon | What it holds |
| --- | --- | --- |
| Appearance | `paintpalette`, Redlamp's | the appearance, the two slots and the gallery |
| Messages | `text.bubble` | bubbles or lines, comfortable or compact, the text size and the font, as built |
| Panes | `rectangle.split.2x1`, Split Across's in the Panes menu | how much the other panes are dimmed, and the focused pane's outline and its colour, as built |

Mac-M5's General, Notifications, Media, Account, Keys and Advanced join the toolbar as they come, in the Mac plan's order with Panes after Messages, the settings jawd keeps (P5) among them.

### The Appearance pane

From the top:

1. **Appearance:** The system's, Dark or Light, as built, radio buttons in a row.
2. **The slots,** Dark and Light, side by side: each a tile 224 by 64 points with its theme's card at half size, its name and its source ("Gruvbox, Omarchy's"), the one Jaw draws now marked "In use". One tile is outlined, the slot the gallery below chooses for; a click on the other moves the outline, and the pane opens on the slot in use. With Appearance on Dark or Light, the slot not in use says when it would be ("Used when Appearance is Light") and can still be chosen for. Their footer: "Jaw draws the dark theme while the Mac is dark, and the light one while it's light."
3. **The gallery,** in three sections, each a grid of cards three across, every card the half the slot would take: **Jaw's**, Tokyo Night, or Tokyo Night Day for Light; **Omarchy's**, the 18 dark or the 5 light, in jawd's order, by ID; and **Redlamp's**, its 22 families in Redlamp's order, Neutral, Redlamp, then the twenty from omoi-station. Choosing for Dark shows 41 cards; for Light, 28. The cards in the slots carry a tick and the system's accent around them.

**Choosing a theme:** a click on a card fills the slot, and every window redraws at once.

- One with both halves, Tokyo Night or any of Redlamp's, fills both slots, each with its own half, so the choice holds when the Mac changes appearance, as Redlamp's families are made to.
- One of Omarchy's, which have one mode each, fills the slot of its mode and leaves the other as it is.
- ⌥-click, or Use for Dark Only (or Light Only) in a card's menu, fills only the slot chosen for, so dark and light can come from different families.
- A click on the theme already chosen for the slot shown changes nothing, with ⌥ or without (Owner, 7 Nov).
- The arrow keys move through the cards, each chosen as it's reached, as the terminal previews a theme as it moves through its list.
- A slot holds only a theme of its own mode. At first both hold Jaw's Tokyo Night (Owner, 26 Oct and 6 Nov).

**A card** is 144 by 104 points, its corners rounded, drawn from that half's roles as the chat list and the conversation draw them: on the ground, a chat list row on the surface, with an avatar in a sender's colour and its online dot, "Lisbon Climbing" in the text colour, "Rui: Sunday at 9?" in the secondary, and "13:58" in the accent drawn as text and an unread count of 2 on the accent, as an unread chat's are; a separator; their bubble on the surface, "Ana" in another sender's colour over "@Pedro bring the rope", @Pedro in the mention colour on a tint of it, as a mention of you is drawn; and yours, "On it" with its time in the secondary text's colour, as your bubble's are, and its read ticks in the link colour. Under it, its name in the system's label colour. VoiceOver reads a card as its name, its section and the slot it fills ("Gruvbox, Omarchy's, the dark theme").

**While jawd's themes can't be read,** the Omarchy section shows the themes the slots hold, from their remembered colours, and in place of the rest one line in jawd's state: "Starting jawd" while the jawd the app started starts, "Omarchy's themes come from jawd, which isn't answering; they show once it does" while it can't be reached, and, for a jawd from before `settings` that answers `themes` with -32601, "This jawd is older and serves no themes; restart it". Jaw's and Redlamp's sections never wait.

### Omarchy's themes, from jawd

- **Asked once a connection:** JawKit asks `themes` after hello, never before the first frame. jawd answers at once, about 15 KB for the 23, and the answer stays in memory for the gallery while the connection lasts.
- **Mapped as the terminal maps them:** `theme.rs`'s `Roles::of`, ported to Swift in JawThemes: the same keys, the same bands of contrast, and the same straight line toward the ground or the text that keeps a colour's hue, so the greys move in the 16 themes the settings design lists. The Mac's roles are then the terminal's:

| The Mac's role | The terminal's role, and Omarchy's key |
| --- | --- |
| ground | `bg`: `background` |
| surface | `surface`: `lighter_background`, at least 1.05:1 |
| selection | `selection`: `selection`, apart from the ground and the surface |
| text | `fg`: whichever of `foreground` and `bright_foreground` reads more |
| secondary | `muted`: halfway between the dim text and the text |
| tertiary | `comment`: `dark_foreground`, 2.5 to 4.5:1 |
| separator | `gutter`: `muted`, 1.4 to 2.6:1 |
| accent: focus, unread counts | `accent`, as Omarchy wrote it |
| the accent drawn as text: an unread chat's time, Draft:, a mention of someone else, Show more, You in a reply | `accent`, held as every colour is ("Every theme, held where it's drawn") |
| link: links, read ticks | `link`: `cyan`, at least 3:1 |
| yours | the accent at 18% over the ground, 12% in a light theme, as Tokyo Night's |
| mention | `mention`: `orange`, at least 3:1 |
| online | `online`: `green`, at least 3:1 |
| a selected row's online | the online colour, moved toward the text until it reads 4.5:1 on the selection |
| failed | `red`, at least 3:1, as the terminal holds its colours |
| senders | the terminal's eight: `red`, `yellow`, `blue`, `magenta`, `cyan`, `green`, `orange`, `bright_cyan`, each at least 3:1 |

- **Held on the surface too:** the terminal holds its colours at 3:1 against the ground, where it draws them. The Mac draws them on the surface as well, its chat list and their bubbles, so each colour above with a band is also held at 3:1 against the surface, moved by the same rule. Computed on 6 November with a port of the rule that gives `theme.rs`'s pinned Tokyo Night: at Omarchy's 4c1b4cb, 14 themes come through as the terminal draws them, and 9 have colours raised on the surface (Catppuccin Latte, Flexoki Light, Kanagawa, Last Horizon, Matte Black, Miasma, Nord, Rosé Pine and Solitude): 24 senders' colours, mentions in 4, failed in 4, links in 3 and online in 2. The accent stays as Omarchy wrote it, as in the terminal, where it fills; drawn as text it's held with every other colour, after this ("Every theme, held where it's drawn").
- **`link`** is new in the Mac's roles. Jaw's Tokyo Night gives it its blue, so nothing changes there, and a Redlamp family its accent role; in Omarchy's it's the terminal's cyan, so Kanagawa's links still stand out, where its accent is its text colour.

### Redlamp's families, copied

- **What's copied,** from Redlamp's main at 9fa9d91, whose catalog hasn't changed since 001d98a on 30 September: `ThemeCatalog.swift`'s families in their slot order, each with its ID, its name and the ten HSL colours of each half; `ThemePalette` and `PrimaryRoles`; the HSL parser; and from `Brand.swift` the nine colours the Redlamp family is made of. About 200 lines, in one file of JawThemes under jaw's licence, with one line saying it's Redlamp's catalog, relicensed by its owner for jaw. Redlamp's tokens and `ThemeMapping` aren't copied: they're a photo editor's, and jaw maps the ten colours onto its own roles in a file of its own.
- **Mapped** by the Mac design's table:

| The Mac's role | From a family's half |
| --- | --- |
| ground | `background` |
| surface | `card` |
| selection | `primary`, or `neutral` where `primaryRoles` keeps it out, at 20% over `card`, 16% in light, as Redlamp's selection |
| text | `foreground` |
| secondary | `muted` |
| tertiary | `muted` at 62% over `background`, Redlamp's tertiary |
| separator | `border` |
| accent, link and the accent drawn as text | `primary`, or `neutral` where `primaryRoles` keeps it out |
| yours | the selection's colour at 18% over `background`, 12% in light |
| mention | `accent`, or the brand's filament where `accent` is the safelight, as in the Redlamp family's light half |
| online | `success` |
| failed | `destructive` |
| senders | eight hues 45° apart from `primary`'s, or, in a half that keeps `primary` out of its selection (the Redlamp family's), 37.5° apart over the wheel outside 30° either side of it; each lightened or darkened until it reads at 4.5:1 on their bubble |

- **Then held,** as Omarchy's are: every colour at 3:1 against the ground and the surface, secondary text too, the dim text at 2.5 to 4.5:1, and the selection and your bubbles moved toward the ground until text reads at 4.5:1 on them. Computed on 6 November: of the 42 halves made of ten colours, 5 come through untouched (the dark halves of Redlamp, Kanagawa, Catppuccin, Gruvbox and Ocean). In the rest, the dim text is raised in 36, since Redlamp's tertiary, its muted colour at 62%, reads under 2.5:1 on most grounds; mentions in 18, the Redlamp family's light half's filament among them, online in 9, secondary text in 9, failed in 5, the accent in 2, the selection in 2 and your bubbles in 1, Everforest's. Most of the colours raised are light halves', whose `accent` and `success` are bright on a light ground. Every half then goes through the hold every theme does ("Every theme, held where it's drawn").
- **Redlamp** keeps its red to the accent, as its `primaryRoles` keeps the brand's red to one place (Owner, 6 Nov): its accent and links are the safelight's red, and its selection and your bubbles come from its neutral grey, `ring` in dark and `steel` in light. Its fills, the unread badges among them, keep the safelight; where it's drawn as text the hold takes it a hair darker on paper (`CF3429`), to read on the accent's tint behind a card's symbols in your bubbles, and a step lighter in dark (`E46353`), to read on its selection, as its links are (`D7352A` on paper, `E46353` in dark). Its mentions take the brand's filament in both halves: the dark half's `accent` field is the filament, `FFB08A`, and the light half's is the safelight itself, `E0402E`, a hair from its accent `D8352A`, so a mention there would look like an unread count; the filament, darkened to read on paper, is a brown (`936654` where it's drawn), the one colour the brand offers that's neither red, grey nor online's green. Its senders' eight hues spread 37.5° apart over the wheel outside 30° either side of the safelight's, from 48.75° past it to 48.75° short of it, rather than 45° apart from it, the first its own; once held, none comes within 48° of the red. Only the Redlamp family keeps `primary` out of its selection, so every other family's senders are as they were.
- **Neutral** is Redlamp's own grey, authored as its standard tokens (greys at alphas over its panel) rather than ten colours, with its accent left to the system's. jaw takes its greys from those tokens' values, the system's accent for its accent, links and your bubbles, the system's orange, green and red for mentions, online and failed, held as above, and its senders' hues from the accent's. All four are the system's as AppKit draws them in each appearance, read together and again when the system's colours change, so Neutral is drawn again when any of them does; the tests and the harness draw it in macOS 27.0.1's, the default accent `007AFF` in both appearances, orange `FF9230` in dark and `FF8D28` in light, green `30D158` and `34C759`, red `FF4245` and `FF383C`. Under a grey accent, below 0.2 saturation as Graphite's is, the senders take the system's blue's hue and saturation at the accent's lightness, so a group's eight still come out as eight colours, where they'd otherwise be one grey (Owner, 6 Nov).
- **`scripts/sync-themes`** pins Redlamp's commit, as `scripts/omarchy-themes` pins Omarchy's. It copies the data again from a Redlamp checkout at that commit (`~/src/darkroom`, or the folder `REDLAMP` names), and `--check` says what differs between jaw's copy and the pinned commit's, and what Redlamp's main has changed since. Redlamp's repository is private, so it runs where a checkout is, on the owner's Mac, and CI skips it, saying so.
- **The licence** (Owner, 6 Nov): Redlamp is under the MPL 2.0, and its owner, who holds both, relicenses its catalog for jaw under jaw's MIT ("I own redlamp, MIT is fine"). The copied file is under jaw's MIT, as the rest of jaw is, with one line saying it's Redlamp's catalog, relicensed by its owner for jaw; there's no `docs/licenses/redlamp.txt`.

### Jaw's own

Jaw's Tokyo Night stays ooze's palette, with Tokyo Night Day as its light half, first in the gallery and in both slots at first, its ID still `jaw`, held as every theme is (below): its dark half's secondary text a step lighter, `7E87AF` for `737AA2`, and its dim text a shade lighter, `66709A` for `565F89`; Tokyo Night Day's blue drawn as text and its links `2D68C4`, where its blue is `2E7DE9`, its red `C0316D` for `F52A65`, its mention `9A5814` for `B15C00` and its dim text `757FAC` for `848CB5`, its fills as they were. Omarchy's Tokyo Night, the terminal's since 27 October, and Redlamp's, a lighter take, are in their own sections.

### Every theme, held where it's drawn

Every theme's roles go through one hold after their own mapping, Jaw's Tokyo Night's, Omarchy's, Redlamp's families' and Neutral's alike, where the catalog makes a theme (Owner, 6 Nov): each colour JawUI draws as text or as a mark is moved toward the text, along the straight line `theme.rs` moves colours on, until it reads at 3:1 on the worst of the grounds JawUI draws it on, a selected row's online at 4.5:1 on its selection; a colour that reads already is kept as it is. The grounds are the five JawUI fills with a theme's colours, the window's ground (a conversation in lines, the composer, the panes, the library, the glance), the surface (the chat list, the palette, the status line, the box's field, their bubbles), a selected row, your bubbles and a selected message's band in lines, the selection at 55% over the window's ground; and the tints JawUI lays over them, made once in the roles and taken by every place that draws one: a card's well, the text at 7% (6% in light); a quote's well, the text at 6% (5%); the accent's tint behind its symbols, 15%; the box's chip, the accent at 16% (12%); and a mention of you on 16% of its own colour. That last tint moves with the colour, so the mention is held in passes until one moves nothing, which every colour reaches: each pass moves its channels only toward the text's, at most 765 steps in all, and no theme's mention takes more than three. No mark is drawn on a tint laid on a tint: a link card's thumbnail, until it's loaded, shows its link symbol on the card's own well, and a selected library row draws a file's symbol and a link's icon on the selection itself, without their tints, so a few small marks don't decide a whole theme's secondary text, accent drawn as text and links. For every colour the hold moves, each of its grounds lies on the far side of it from the text, so moving one way serves them all, and the text reads at 4.5:1 on each, so every colour gets there; the furthest goes 56% of its way to the text (Redlamp's One Dark dark's secondary text, for a card's well in your bubble).

| Colour | Where it's drawn | Held at 3:1 on |
| --- | --- | --- |
| secondary text | previews, times, details, a card's words, a quote's text, a picture's placeholder; the dim text's words and marks on your bubbles, a selected row, a selected message and a well (a link card's host, a tile's placeholder) | the five grounds; a card's well on the window's ground, the surface, your bubbles and a selected message; a quote on the surface and your bubbles |
| dim text | ticks not yet read, the pending clock, pin and mute marks, placeholders, hints | the window's ground and the surface |
| the accent drawn as text | an unread chat's time, Draft:, a mention of someone else, Show more, You in a reply, the palette's and the library's marks, a card's action and symbols, the box's chip | the five grounds; a card's well on the window's ground, the surface, your bubbles and a selected message; a quote on the surface and your bubbles; the accent's tint on the window's ground, the surface, your bubbles and a selected message; the chip |
| links | links, read ticks, a link card's bar, the library's links | the five grounds, and a card's well on the window's ground, the surface, your bubbles and a selected message |
| mentions | a mention of you, the "new" marker, the glance's @ | the five grounds, and its own tint on the window's ground, the surface, your bubbles and a selected message |
| online | typing in the chat list, the header and the status line | the window's ground and the surface |
| a selected row's online | typing on a selected row | a selected row, at 4.5:1 |
| failed | the ! on a failed message, Not sent · Retry | the five grounds |
| senders | names in a group, a quote's name, the glance | the five grounds, and a quote on the surface and your bubbles |

- **The accent's fills keep the colour the theme wrote:** the unread badge, the tints above and the ones the selection and your bubbles are made from, the focus, selection and reaction outlines, the pane outline, the composer's caret. Where the accent is drawn as text it's a role of its own, the accent held, so an unread chat's time reads beside its badge, which stays the theme's; the count on the badge reads at 3.42:1 at the least, in Rosé Pine.
- **The dim text stays the quietest:** it's held at 3:1 on the window's ground and the surface, and everywhere else it's drawn, on your bubbles, a selected row, a selected message and a card's or a tile's well, where it's drawn only as marks and a few short words, it's drawn in the secondary text's colour, as holding it there would bring it level with the secondary text, or near it, as WhatsApp draws your bubble's grey ticks in its time's, and as jaw's sidebar takes a stronger tone on its selection. Held so, it moves in 59 of the 69 halves, stays quieter than the secondary text in every one, and comes within 15% of it in one, Redlamp's Everforest light (3.26:1 against 3.58:1); held on the wells too, it came within 15% in 16. White's faint ticks and pin marks, at 1.67:1 on its surface, read.
- **Left out:** separators, borders and rules, as authored, which carry no words; the accent's fills, and the senders' as an avatar's fill, whose initials only gain as the senders move; the sidebar, drawn on the system's material in the system's selection; words over a picture's scrim, and over the header's translucent ground as the conversation scrolls under it; the dimming of panes not in focus, which the owner sets; and the voice note's waveform and a poll's bars, pictures rather than words.
- **What it moves,** computed on 6 November with a port of the hold: something in 67 of the 69 halves, Omarchy's Hackerman and Redlamp's Catppuccin dark coming through as mapped; the dim text in 59, mentions in 31, secondary text in 27, failed in 26, the accent drawn as text in 25, links in 19, and 74 senders' colours in 21 halves; online nowhere, as both mappings hold it already. Each moved colour lands at 3 to 3.046:1 on the worst of its grounds. What decides most of it is words: a link's description and a contact's detail on a card's well in your bubble, and a selected row's words, for the secondary text, the chat list's and the palette's words on the surface for the dim text, and a selected row's words for the accent drawn as text and links; and, of small marks, the accent's tint behind a card's symbols in your bubble or on a selected message, and a link card's bar in your bubble.

### Kept, and before jawd answers

- **The slots** are kept in the app's preferences as theme IDs, `themeDark` and `themeLight`: `jaw`, `omarchy:<id>` as jawd names it, or `redlamp:<id>` as Redlamp does, since the two share names such as `nord`.
- **One of Omarchy's** is kept with its 25 colours as jawd last served them (`themeDarkColours`, `themeLightColours`). So the first frame is drawn in the theme chosen before jawd answers, where the terminal waits up to 250 ms for hello, and a Jaw whose jawd can't be reached keeps its theme. When jawd answers, its colours replace the copy, so a newer Omarchy in a newer jawd shows at once.
- **A theme jawd no longer serves,** dropped by a later Omarchy, draws Jaw's Tokyo Night in its slot, as the terminal draws an unknown theme, and the choice is kept, its tile saying "Gruvbox isn't among jawd's themes", so a jawd that serves it again brings it back, as jawd keeps a setting it doesn't take rather than dropping it. A slot holding an ID no source knows, or a theme of the other mode, draws Tokyo Night the same way.
- **From the single `theme` key,** which holds `jaw` today, the only theme there is: it's read once into both slots while they're unset, and left as it is, so an earlier build still reads it.
- **For one run,** a slot can be given as an argument, as any preference can (`-themeDark omarchy:nord`).

## What changes, part by part

- **JawThemes:** `Roles` gains `link`. `Theme` takes the place of `ThemeFamily`: an ID, a name, its source, and a dark half, a light half or both, as `Roles`. Omarchy's mapping, `theme.rs`'s rule ported, with the Mac's roles and the surface's hold; a reader of Omarchy's `colors.toml` as jaw-proto reads it, with `orange` and `brown` where a theme leaves them out, for the tests and the harness, which ask jawd for nothing. Redlamp's copy, under jaw's MIT, and jaw's mapping of it with Neutral's greys. The `Catalog`: Jaw's own, Redlamp's 22, and Omarchy's as jawd or the remembered copy gives them, looked up by ID with the fallbacks above. The slots and how a choice fills them, as a value. `Roles` gains `accentText`, the accent drawn as text, and the tints JawUI lays over a ground; every theme's roles go through the hold, where `Theme` is made and where `Catalog.jaw` is; and the system's accent carries its orange, green and red, which Neutral takes.
- **JawKit:** `themes()`, and its answer typed (`Themes`, each theme its ID, name, mode and colours); `themes.result.json` decodes into it and `themes.request.json` is what it writes, in `FixtureTests`; `FakeJawd` answers it from the fixture; a jawd that doesn't know `themes` reads as one that serves none. The protocol doesn't change: `themes` is served as it has it.
- **JawUI:** `SettingsView`, its segmented control and Done go. `AppearancePane` (the appearance, the slots, the gallery and its states), the card, `MessagesPane` and `PanesPane` take their place, the last two the forms as built with their explanations as section footers. `Preferences` holds the slots and their remembered colours in place of `theme`, with the migration, and `style(dark:)` takes the slot's theme. Links and read ticks are drawn in `link` where they're drawn in the accent now (the cards, the message layout and the library's drawing among them). The accent drawn as text is drawn in `accentText`, a file of yours on its way's ring and × among them, the tints are taken from the roles, the dim text's words and marks everywhere but on the window's ground and the surface in the secondary text's colour, the conversation telling a message's layout when it's selected in lines; a link card's waiting thumbnail and a selected library row's file and link marks are drawn on one ground, not on a tint laid on a tint; and `Preferences` reads the system's orange, green and red with its accent.
- **The app:** `SettingsWindowController`, its frame and its pane kept. `MainWindowController.showSettings()` and its sheet go; the `settings` command opens the window, and "Choose a theme" joins the palette's actions. `AppModel` asks `themes` after each hello, gives the answer to the catalog and the window, and restyles when a slot's theme's colours change. The `--keys` step `settings` opens the window, `settings:appearance`, `settings:messages` and `settings:panes` on a pane, and `--snapshot` draws the Settings window once such a step has opened it.
- **The harness:** its Theme condition lists every theme by section, with `--theme jaw`, `--theme omarchy:<id>` and `--theme redlamp:<id>`, Omarchy's read from `protocol/themes/`, copied into the harness as it's built. New scenes draw the three panes at the window's width, the dark slot's gallery and the light one's, a card in each of its states (filling a slot, the slot chosen for, under the pointer), and the Omarchy section while jawd starts, can't be reached and is older; the Roles scene draws any theme's roles with their contrast, as it does Jaw's.
- **Scripts:** `scripts/sync-themes`.
- **`mac/parity.toml`** keeps its rows: the terminal reads Omarchy's themes from jaw-proto and never sends `themes`, so the Mac sending it is no gap, and `settings.set` and the `settings` push stay with step 3.5's other half.
- **The docs:** the Mac design's decisions, "The UI stack" (Settings in a window of its own) and "Themes" (Redlamp's copied now, the slots, Omarchy's mapping and its hold on the surface, `link` in the roles' table, the Appearance pane); the Mac plan's Mac-M5 (step 1 left with tint, translucency, native controls and the palette's own theme; step 6 with the panes still to come; `sync-themes` out of CI); the parity plan's step 3.5 (both sets now, Owner, 6 Nov) and Phase 4; the parity canvas's Themes and settings rows, and the board's steps 3.5 and 4.3; and the Mac README's `--keys`, preferences by argument and the harness's `--theme`.

## What it doesn't do

- **The settings jawd keeps,** what downloads by itself, the cache's size and notifications (P5): parity step 3.5's other half, with the panes they come in.
- **Tint, translucency, native controls in the theme's accent, and the palette in a theme of its own:** Redlamp has them around its families, and Mac-M5's step 1 keeps them (Owner, 6 Nov).
- **The Mac's theme kept by jawd,** or following the terminal's: the Mac's look is its own (Owner, 27 Oct), and a frontend reading another's settings is the settings design's Later.
- **Senders at 4.5:1 in every theme,** which the Mac design's Accessibility asks. Every theme's senders read at 3:1 or more on every ground they're drawn on, the hold's floor (Owner, 6 Nov): Omarchy's at 3:1 or more, Jaw's at 4.43:1 or more on their bubble, Tokyo Night Day's red moved to `C0316D`, and Redlamp's derived hues at 4.5:1 there, as the Mac design has them; 4.5:1 in every theme is Mac-M5's accessibility step, with Increase Contrast.
- **Changes to jawd, the protocol or the terminal.** Nothing touches the data layer's paths, so no audit is needed.

## Testing

- **The mappings:** Omarchy's ported rule gives `theme.rs`'s pinned values, Tokyo Night's greys and colours, and its lists of what moves in which theme, each grey and colour it moves landing within 0.05 of its target, as `theme.rs`'s own test has them. Neutral's greys come from Redlamp's tokens, its orange, green and red are the system's it's given, macOS 27.0.1's in the tests, and its senders come out as eight colours under a grey accent; the Redlamp family keeps its red to its accent in both halves, its mentions the filament and its senders' hues 30° or more from the red. What the surface's hold, Redlamp's bands and the hold every theme goes through move is listed and pinned as `theme.rs` pins its own, with where each moved colour lands, within 0.05 of its target, so a change to any of them is seen.
- **Every theme reads,** of every source and in each half, as its mapping makes it: text at 4.5:1 on the ground, the surface, the selection and your bubbles; secondary text at 3:1 on the ground and the surface; dim text from 2.5 to 4.5:1; links, mentions, online, failed and the senders at 3:1 on the ground and the surface, and the accent on the ground; and a selected row's online at 4.5:1 on its selection. Once held, every colour in the hold's table reads at its floor on each of its grounds and tints, the table itself pinned as JawUI's draw sites have it, a mention of you on its own tint in passes that end, and every theme the catalog lists is a mapping's held; the accent drawn as text, the tints, the three small marks on one ground, and the dim text's words and marks on your bubbles, a selected row, a selected message and a well are tested where JawUI draws them. The tests read Omarchy's 23 from `protocol/themes/`, as the Swift client's read `protocol/fixtures`, so an Omarchy that jaw-proto moves to is checked on the Mac the same day.
- **The slots:** a theme with both halves fills both, one of Omarchy's its own, ⌥ one; a slot never holds the other mode; the `theme` key read once into both; an unknown ID or a theme of the other mode drawn as Tokyo Night and kept; the remembered colours drawn before jawd answers and replaced when it does.
- **JawKit:** the fixtures decode and are written as they are, and the live suite asks `jawd demo` for `themes` and finds the 23 as `protocol/themes/` has them.
- **Pictures:** the harness's scenes against references, in the app's dark and light appearance: the three panes, the dark and light galleries, the card's states and the Omarchy section's; and the app's `--snapshot` with `--keys settings:appearance` for the window with its toolbar.
- **Budgets:** launch stays within its 250 ms p99 with `themes` asked after the first frame (`--bench launch`); a theme chosen redraws the main window as a change of look does; with Settings open and nothing changing, nothing is drawn (`--bench idle`).
- **By hand,** the owner's: the window on a real screen, its toolbar and the gallery, the Mac's own appearance switched to Auto while two slots are filled, and VoiceOver over the gallery.

## Order of work

1. **JawThemes:** `link`, Omarchy's mapping ported, Redlamp's copy and its mapping with Neutral, `Theme`, the catalog and the slots, and the tests that every theme reads.
2. **JawKit:** `themes`, its fixtures, `FakeJawd` and the live test.
3. **Preferences:** the slots, the remembered colours and the migration; `link` drawn where links and read ticks are.
4. **The window:** `SettingsWindowController`, the panes moved into it, the sheet and Done gone, ⌘, and the palette's "Choose a theme".
5. **The Appearance pane:** the slots, the gallery, the cards and the states.
6. **The harness's scenes and pictures,** and `scripts/sync-themes`.
7. **The docs and canvases** above, and the owner's look on a real screen.
